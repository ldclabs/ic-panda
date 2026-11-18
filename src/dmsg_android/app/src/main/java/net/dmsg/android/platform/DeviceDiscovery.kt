package net.dmsg.android.platform

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import kotlinx.coroutines.*
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

data class TransferAddress(val host: String, val port: UShort) {
    fun text() = (if (':' in host) "[$host]" else host) + ":" + port
    companion object {
        fun parse(value: String): TransferAddress? {
            val text = value.trim()
            if (text.length !in 3..300 || text.any { it.isWhitespace() || it.isISOControl() } || text.any { it in "/\\@?#" }) return null
            val split = text.lastIndexOf(':')
            if (split <= 0) return null
            val raw = text.substring(0, split)
            val host = if (raw.startsWith('[') && raw.endsWith(']')) raw.substring(1, raw.length - 1) else raw.takeUnless { ':' in it } ?: return null
            val port = text.substring(split + 1).takeIf { it.isNotEmpty() && it.all(Char::isDigit) }?.toUShortOrNull()?.takeIf { it > 0u } ?: return null
            if (host.isEmpty() || !host.all { it.isLetterOrDigit() || it in ".:-_%" } || host == "." || host == "..") return null
            return TransferAddress(host, port)
        }
    }
}

/** Discovery returns an address only. The Rust protocol still authenticates both devices. */
class DeviceDiscovery(context: Context) {
    private val nsd = context.applicationContext.getSystemService(NsdManager::class.java)
    private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)
    interface Advertisement { fun close() }

    suspend fun advertise(name: String, port: UShort): Advertisement {
        val closed = AtomicBoolean()
        var registered = false
        lateinit var listener: NsdManager.RegistrationListener
        fun stop() { if (closed.compareAndSet(false, true) && registered) runCatching { nsd.unregisterService(listener) } }
        try {
            withTimeoutOrNull(15_000) {
                suspendCancellableCoroutine<Unit> { continuation ->
                    listener = object : NsdManager.RegistrationListener {
                        override fun onServiceRegistered(info: NsdServiceInfo) {
                            if (closed.get()) { runCatching { nsd.unregisterService(this) }; return }
                            if (info.serviceName != name) { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("DISCOVERY_NAME_CONFLICT")); stop() }
                            else if (continuation.isActive) continuation.resume(Unit)
                        }
                        override fun onRegistrationFailed(info: NsdServiceInfo, code: Int) { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("DISCOVERY_UNAVAILABLE")); stop() }
                        override fun onServiceUnregistered(info: NsdServiceInfo) = Unit
                        override fun onUnregistrationFailed(info: NsdServiceInfo, code: Int) = Unit
                    }
                    continuation.invokeOnCancellation { stop() }
                    val info = NsdServiceInfo().apply { serviceName = name; serviceType = TYPE; setPort(port.toInt()); setAttribute("v", "1") }
                    registered = true
                    try { nsd.registerService(info, NsdManager.PROTOCOL_DNS_SD, listener) }
                    catch (error: Exception) { stop(); if (continuation.isActive) continuation.resumeWithException(error) }
                }
            } ?: error("DISCOVERY_UNAVAILABLE")
            return object : Advertisement { override fun close() = stop() }
        } catch (error: Throwable) { stop(); throw error }
    }

    @Suppress("DEPRECATION")
    suspend fun find(name: String): TransferAddress {
        val multicast = wifi.createMulticastLock("dmsg-device-discovery").apply { setReferenceCounted(false) }
        var listener: NsdManager.DiscoveryListener? = null
        val resolving = AtomicBoolean()
        return try {
            multicast.acquire()
            withTimeoutOrNull(15_000) {
                suspendCancellableCoroutine { continuation ->
                    val discovered = object : NsdManager.DiscoveryListener {
                        override fun onDiscoveryStarted(type: String) = Unit
                        override fun onDiscoveryStopped(type: String) = Unit
                        override fun onStopDiscoveryFailed(type: String, code: Int) = Unit
                        override fun onStartDiscoveryFailed(type: String, code: Int) { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("DISCOVERY_UNAVAILABLE")) }
                        override fun onServiceLost(info: NsdServiceInfo) = Unit
                        override fun onServiceFound(info: NsdServiceInfo) {
                            if (!continuation.isActive || info.serviceName != name || info.serviceType.trimEnd('.') != TYPE.trimEnd('.') || !resolving.compareAndSet(false, true)) return
                            try {
                                nsd.resolveService(info, object : NsdManager.ResolveListener {
                                    override fun onResolveFailed(info: NsdServiceInfo, code: Int) { if (continuation.isActive) continuation.resumeWithException(IllegalStateException("DISCOVERY_UNAVAILABLE")) }
                                    override fun onServiceResolved(info: NsdServiceInfo) {
                                        if (!continuation.isActive) return
                                        val host = info.host?.hostAddress
                                        val version = info.attributes["v"]
                                        if (info.serviceName != name || host.isNullOrEmpty() || info.port !in 1..65535 || version == null || !version.contentEquals(byteArrayOf('1'.code.toByte())))
                                            continuation.resumeWithException(IllegalStateException("DISCOVERY_UNAVAILABLE"))
                                        else continuation.resume(TransferAddress(host, info.port.toUShort()))
                                    }
                                })
                            } catch (error: Exception) { if (continuation.isActive) continuation.resumeWithException(error) }
                        }
                    }
                    listener = discovered
                    try { nsd.discoverServices(TYPE, NsdManager.PROTOCOL_DNS_SD, discovered) }
                    catch (error: Exception) { if (continuation.isActive) continuation.resumeWithException(error) }
                }
            } ?: error("DISCOVERY_UNAVAILABLE")
        } finally {
            listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
            if (multicast.isHeld) multicast.release()
        }
    }

    companion object { const val TYPE = "_dmsg-xfer._tcp." }
}
