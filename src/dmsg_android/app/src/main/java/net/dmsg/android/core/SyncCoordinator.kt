package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.core.*

/** Foreground-only drivers. The Rust scheduler owns coalescing, retry and receive-only policy. */
class SyncCoordinator(private val client: ClientController, private val scope: CoroutineScope) {
    private val mutable = MutableStateFlow<ContentSyncStatus?>(null)
    val status = mutable.asStateFlow()
    private val connection = MutableStateFlow(false)
    val connected = connection.asStateFlow()
    private var owner: SessionStamp? = null
    private var sync: Job? = null
    private var hints: Job? = null
    var automatic = true
        private set
    private val constraint = MutableStateFlow(false)
    val constrained = constraint.asStateFlow()

    fun start() {
        val stamp = client.currentStamp() ?: return
        if (!client.accepts(stamp)) return
        if (owner == stamp && sync?.isActive == true) return
        stop(clear = owner?.account != stamp.account)
        owner = stamp
        val core = client.scopedHandle() ?: return
        sync = scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    client.operation { operation -> core.runContentSync(automatic && !constraint.value, object : ContentSyncObserver {
                        override fun status(status: ContentSyncStatus) { scope.launch { if (client.accepts(stamp)) mutable.value = status } }
                        override fun passed(changed: Boolean, error: String?) {
                            scope.launch { if (client.accepts(stamp)) client.contentChanged(changed) }
                        }
                    }, operation) }
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Throwable) { /* Authoritative failure remains in core status; foreground/manual retry can restart. */ }
            finally { core.destroy() }
        }
        val hintsCore = client.scopedHandle() ?: return
        hints = scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    client.operation { operation -> hintsCore.listenHints(object : HintObserver {
                        override fun connected(connected: Boolean) { scope.launch { if (client.accepts(stamp)) connection.value = connected } }
                        override fun hint(kind: String, resource: String?, news: Boolean) {
                            scope.launch {
                                if (!client.accepts(stamp)) return@launch
                                if (kind == "content" && news) hintsCore.requestContentSync()
                                client.receivedHint(kind, resource)
                            }
                        }
                    }, operation) }
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Throwable) { scope.launch { if (client.accepts(stamp)) connection.value = false } }
            finally { hintsCore.destroy() }
        }
    }

    fun setAutomatic(enabled: Boolean) {
        if (automatic == enabled) return
        automatic = enabled; stop(clear = false); start()
    }
    fun setConstrained(value: Boolean) {
        if (constraint.value == value) return
        constraint.value = value; stop(clear = false); start()
    }
    fun networkChanged() {
        val stamp = owner ?: return
        if (!client.accepts(stamp)) return
        client.scopedHandle()?.let { core -> try { core.reconnectHints(); core.requestContentSync() } finally { core.destroy() } }
    }
    fun manual() {
        start()
        client.perform({ core ->
            client.operation { core.reconcile(it) }
            client.operation { core.sync(false, it) }
            core.contentSyncStatus()
        }, key = "sync:manual", onError = { client.contentChanged(false) }) { status ->
            mutable.value = status
            client.contentChanged(true)
        }
    }
    fun stop(clear: Boolean = false) {
        sync?.cancel(); hints?.cancel(); sync = null; hints = null; owner = null
        connection.value = false
        if (clear) mutable.value = null
    }
}
