package net.dmsg.android.platform

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import kotlinx.coroutines.*
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

data class NearbyDiscoveredPeer(
    val advert: String,
    val alias: String,
    val pkHint: String,
    val host: String,
    val port: UShort,
)

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

    suspend fun advertiseNearby(name: String, port: UShort, alias: String, pkHint: String): Advertisement {
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
                            if (continuation.isActive) continuation.resume(Unit)
                        }
                        override fun onRegistrationFailed(info: NsdServiceInfo, code: Int) {
                            if (continuation.isActive) continuation.resumeWithException(IllegalStateException("DISCOVERY_UNAVAILABLE"))
                            stop()
                        }
                        override fun onServiceUnregistered(info: NsdServiceInfo) = Unit
                        override fun onUnregistrationFailed(info: NsdServiceInfo, code: Int) = Unit
                    }
                    continuation.invokeOnCancellation { stop() }
                    val info = NsdServiceInfo().apply {
                        serviceName = name
                        serviceType = NEARBY_TYPE
                        setPort(port.toInt())
                        setAttribute("v", "1")
                        setAttribute("n", alias)
                        setAttribute("p", pkHint)
                    }
                    registered = true
                    try { nsd.registerService(info, NsdManager.PROTOCOL_DNS_SD, listener) }
                    catch (denied: SecurityException) { stop(); if (continuation.isActive) continuation.resumeWithException(IllegalStateException("LOCAL_NETWORK_DENIED", denied)) }
                    catch (error: Exception) { stop(); if (continuation.isActive) continuation.resumeWithException(error) }
                }
            } ?: error("DISCOVERY_UNAVAILABLE")
            return object : Advertisement { override fun close() = stop() }
        } catch (error: Throwable) { stop(); throw error }
    }

    @Suppress("DEPRECATION")
    fun browseNearby(onPeers: (List<NearbyDiscoveredPeer>) -> Unit, onDenied: () -> Unit): Advertisement {
        val closed = AtomicBoolean()
        val multicast = wifi.createMulticastLock("dmsg-nearby-discovery").apply { setReferenceCounted(false) }
        val peers = LinkedHashMap<String, NearbyDiscoveredPeer>()
        val lock = Any()
        val executor = Executors.newSingleThreadExecutor()
        val callbacks = mutableMapOf<String, Any>()
        val pendingLegacy = ArrayDeque<NsdServiceInfo>()
        var resolvingLegacy = false
        var listener: NsdManager.DiscoveryListener? = null

        fun publish() {
            val snapshot = synchronized(lock) { peers.values.sortedBy { it.alias.lowercase() } }
            onPeers(snapshot)
        }

        fun extractPeer(info: NsdServiceInfo): NearbyDiscoveredPeer? {
            val version = info.attributes["v"] ?: return null
            if (!version.contentEquals(byteArrayOf('1'.code.toByte()))) return null
            val port = info.port.takeIf { it in 1..65535 }?.toUShort() ?: return null
            val host = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                info.hostAddresses.firstOrNull { !it.isLoopbackAddress && !it.isLinkLocalAddress }?.hostAddress
                    ?: info.hostAddresses.firstOrNull()?.hostAddress
            } else {
                info.host?.hostAddress
            } ?: return null
            val alias = info.attributes["n"]?.toString(Charsets.UTF_8)?.trim()?.takeIf { it.isNotEmpty() } ?: info.serviceName
            val pkHint = info.attributes["p"]?.toString(Charsets.UTF_8)?.trim().orEmpty()
            return NearbyDiscoveredPeer(info.serviceName, alias, pkHint, host, port)
        }

        fun pumpLegacy() {
            val next = synchronized(lock) {
                if (closed.get() || resolvingLegacy || pendingLegacy.isEmpty()) return
                resolvingLegacy = true
                pendingLegacy.removeFirst()
            }
            try {
                nsd.resolveService(next, object : NsdManager.ResolveListener {
                    override fun onResolveFailed(info: NsdServiceInfo, code: Int) {
                        synchronized(lock) { resolvingLegacy = false }
                        pumpLegacy()
                    }
                    override fun onServiceResolved(info: NsdServiceInfo) {
                        extractPeer(info)?.let { peer ->
                            synchronized(lock) { peers[peer.advert] = peer }
                            publish()
                        }
                        synchronized(lock) { resolvingLegacy = false }
                        pumpLegacy()
                    }
                })
            } catch (_: SecurityException) {
                synchronized(lock) { resolvingLegacy = false }
                onDenied()
            } catch (_: Exception) {
                synchronized(lock) { resolvingLegacy = false }
                pumpLegacy()
            }
        }

        fun stop() {
            if (!closed.compareAndSet(false, true)) return
            listener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                val registered = synchronized(lock) { callbacks.values.toList().also { callbacks.clear() } }
                registered.forEach { cb ->
                    runCatching { nsd.unregisterServiceInfoCallback(cb as NsdManager.ServiceInfoCallback) }
                }
            }
            synchronized(lock) { pendingLegacy.clear(); peers.clear() }
            runCatching { if (multicast.isHeld) multicast.release() }
            executor.shutdownNow()
        }

        try {
            runCatching { multicast.acquire() }
            val discovered = object : NsdManager.DiscoveryListener {
                override fun onDiscoveryStarted(type: String) = Unit
                override fun onDiscoveryStopped(type: String) = Unit
                override fun onStopDiscoveryFailed(type: String, code: Int) = Unit
                override fun onStartDiscoveryFailed(type: String, code: Int) { stop() }
                override fun onServiceLost(info: NsdServiceInfo) {
                    val removed = synchronized(lock) {
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                            (callbacks.remove(info.serviceName) as? NsdManager.ServiceInfoCallback)?.let { cb ->
                                runCatching { nsd.unregisterServiceInfoCallback(cb) }
                            }
                        }
                        peers.remove(info.serviceName) != null
                    }
                    if (removed) publish()
                }
                override fun onServiceFound(info: NsdServiceInfo) {
                    if (closed.get() || info.serviceType.trimEnd('.') != NEARBY_TYPE.trimEnd('.')) return
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                        val cb = object : NsdManager.ServiceInfoCallback {
                            override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) = Unit
                            override fun onServiceUpdated(updated: NsdServiceInfo) {
                                if (closed.get()) return
                                extractPeer(updated)?.let { peer ->
                                    synchronized(lock) { peers[peer.advert] = peer }
                                    publish()
                                }
                            }
                            override fun onServiceLost() {
                                val removed = synchronized(lock) { peers.remove(info.serviceName) != null }
                                if (removed) publish()
                            }
                            override fun onServiceInfoCallbackUnregistered() = Unit
                        }
                        val previous = synchronized(lock) { callbacks.put(info.serviceName, cb) as? NsdManager.ServiceInfoCallback }
                        previous?.let { runCatching { nsd.unregisterServiceInfoCallback(it) } }
                        try { nsd.registerServiceInfoCallback(info, executor, cb) }
                        catch (_: SecurityException) { onDenied() }
                        catch (_: Exception) { Unit }
                    } else {
                        synchronized(lock) { pendingLegacy.addLast(info) }
                        pumpLegacy()
                    }
                }
            }
            listener = discovered
            nsd.discoverServices(NEARBY_TYPE, NsdManager.PROTOCOL_DNS_SD, discovered)
        } catch (_: SecurityException) {
            stop()
            onDenied()
        } catch (_: Exception) {
            stop()
        }
        return object : Advertisement { override fun close() = stop() }
    }

    companion object {
        const val TYPE = "_dmsg-xfer._tcp."
        const val NEARBY_TYPE = "_dmsg-share._tcp."
    }
}
