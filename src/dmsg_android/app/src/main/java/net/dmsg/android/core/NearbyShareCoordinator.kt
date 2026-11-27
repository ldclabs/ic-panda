package net.dmsg.android.core

import android.os.Build
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.DeviceDiscovery
import net.dmsg.android.platform.NearbyDiscoveredPeer
import net.dmsg.core.*

data class NearbyFailure(val code: String, val message: String)
data class NearbyFileSummary(val index: UInt, val name: String, val mime: String, val size: ULong)
data class NearbyProgress(val totalItems: UInt, val completedItems: UInt, val totalBytes: ULong, val bytes: ULong, val currentName: String?)
data class NearbyPendingPeer(
    val peerId: String,
    val sas: String,
    val qrBound: Boolean,
    val transport: String,
    val kind: String,
    val files: List<NearbyFileSummary>,
    val packageSize: ULong?,
    val alias: String?,
)
data class NearbyReceivedFile(val index: UInt, val fileId: String, val versionId: String, val name: String, val size: ULong)
data class NearbyPeerIdentity(val environment: String, val account: String, val device: String, val handle: String?, val status: String)
data class NearbyReceiveOutcome(val kind: String, val files: List<NearbyReceivedFile>, val packagePreview: SecretPackage?, val identity: NearbyPeerIdentity?)
data class NearbySendOutcome(val kind: String, val sentItems: UInt, val bytes: ULong, val packageDigest: String?, val identity: NearbyPeerIdentity?)

data class NearbyReceiveStatus(
    val id: ULong,
    val state: String,
    val alias: String,
    val discoverable: Boolean,
    val advert: String,
    val pkHint: String,
    val port: UShort,
    val addrs: List<String>,
    val offer: String,
    val expiresAt: ULong,
    val relayReady: Boolean,
    val transport: String?,
    val pendingPeers: List<NearbyPendingPeer>,
    val verificationCodes: List<String>,
    val activePeer: String?,
    val progress: NearbyProgress,
    val outcome: NearbyReceiveOutcome?,
    val error: NearbyFailure?,
) {
    val active: Boolean get() = state in setOf("waiting", "running")
}

data class NearbySendStatus(
    val id: ULong,
    val state: String,
    val kind: String,
    val transport: String?,
    val sas: String?,
    val transcript: String?,
    val qrBound: Boolean,
    val peerLabel: String,
    val shareJobId: String?,
    val approvalId: String?,
    val progress: NearbyProgress,
    val outcome: NearbySendOutcome?,
    val error: NearbyFailure?,
) {
    val active: Boolean get() = state in setOf("connecting", "awaiting_sas", "awaiting_approval", "awaiting_accept", "running")
}

data class NearbyStatus(val receive: NearbyReceiveStatus?, val send: NearbySendStatus?)
data class NearbySelectedFile(val id: String, val version: String?, val name: String, val size: ULong)

// A nonbreaking numeric separator keeps both groups in protocol order in RTL text.
fun groupedNearbySas(sas: String): String =
    if (sas.length == 6) sas.substring(0, 3) + "\u00a0" + sas.substring(3) else sas

private fun parseNearbyFailure(value: JsonElement?): NearbyFailure? {
    val row = value?.takeIf { it != JsonNull }?.objectValue() ?: return null
    return NearbyFailure(row.required("code"), row.text("message") ?: row.required("code"))
}

private fun parseNearbyProgress(row: JsonObject): NearbyProgress = NearbyProgress(
    row.requiredUInt("total_items"),
    row.requiredUInt("completed_items"),
    row.requiredUnsigned("total_bytes"),
    row.requiredUnsigned("bytes"),
    row.text("current_name"),
)

private fun parseNearbyIdentity(value: JsonElement?): NearbyPeerIdentity? {
    val row = value?.takeIf { it != JsonNull }?.objectValue() ?: return null
    return NearbyPeerIdentity(row.required("environment"), row.required("account"), row.required("device"), row.text("handle"), row.required("status"))
}

fun parseNearbyStatus(value: JsonElement): NearbyStatus {
    val root = value.objectValue()
    val receive = root["receive"]?.takeIf { it != JsonNull }?.objectValue()?.let { row ->
        val port = row.requiredUnsigned("port").also { check(it in 1uL..65535uL) { "INTEGRITY_FAILED" } }.toUShort()
        val pending = row.rows("pending_peers").map { peer ->
            val files = peer.rows("files").map { file ->
                NearbyFileSummary(file.requiredUInt("index"), file.required("name"), file.required("mime"), file.requiredUnsigned("size"))
            }
            NearbyPendingPeer(
                peer.required("peer_id"),
                peer.required("sas"),
                peer.requiredFlag("qr_bound"),
                peer.required("transport"),
                peer.required("kind"),
                files,
                peer.unsigned("package_size"),
                peer.text("alias"),
            )
        }
        val outcome = row["outcome"]?.takeIf { it != JsonNull }?.objectValue()?.let { out ->
            val files = out.rows("files").map { f ->
                NearbyReceivedFile(f.requiredUInt("index"), f.required("file_id"), f.required("version_id"), f.required("name"), f.requiredUnsigned("size"))
            }
            val preview = out["package_preview"]?.takeIf { it != JsonNull }?.let(::parseSecretPackage)
            NearbyReceiveOutcome(out.required("kind"), files, preview, parseNearbyIdentity(out["identity"]))
        }
        NearbyReceiveStatus(
            row.requiredUnsigned("id"),
            row.required("state"),
            row.required("alias"),
            row.requiredFlag("discoverable"),
            row.required("advert"),
            row.required("pk_hint"),
            port,
            row.strings("addrs"),
            row.required("offer"),
            row.requiredUnsigned("expires_at"),
            row.requiredFlag("relay_ready"),
            row.text("transport"),
            pending,
            row.strings("verification_codes"),
            row.text("active_peer"),
            parseNearbyProgress(row["progress"]!!.objectValue()),
            outcome,
            parseNearbyFailure(row["error"]),
        )
    }
    val send = root["send"]?.takeIf { it != JsonNull }?.objectValue()?.let { row ->
        val outcome = row["outcome"]?.takeIf { it != JsonNull }?.objectValue()?.let { out ->
            NearbySendOutcome(
                out.required("kind"),
                out.requiredUInt("sent_items"),
                out.requiredUnsigned("bytes"),
                out.text("package_digest"),
                parseNearbyIdentity(out["identity"]),
            )
        }
        NearbySendStatus(
            row.requiredUnsigned("id"),
            row.required("state"),
            row.required("kind"),
            row.text("transport"),
            row.text("sas"),
            row.text("transcript"),
            row.requiredFlag("qr_bound"),
            row.required("peer_label"),
            row.text("share_job_id"),
            row.text("approval_id"),
            parseNearbyProgress(row["progress"]!!.objectValue()),
            outcome,
            parseNearbyFailure(row["error"]),
        )
    }
    return NearbyStatus(receive, send)
}

/** Coordinates foreground face-to-face nearby sharing (LAN + blind relay) and mDNS discovery. */
class NearbyShareCoordinator(
    private val client: ClientController,
    private val scope: CoroutineScope,
    private val discovery: DeviceDiscovery,
    private val cancelNative: () -> Unit,
) {
    private val statusValue = MutableStateFlow<NearbyStatus?>(null)
    val status = statusValue.asStateFlow()
    private val peersValue = MutableStateFlow<List<NearbyDiscoveredPeer>>(emptyList())
    val peers = peersValue.asStateFlow()
    private val browsingValue = MutableStateFlow(false)
    val browsing = browsingValue.asStateFlow()
    private val localNetworkDeniedValue = MutableStateFlow(false)
    val localNetworkDenied = localNetworkDeniedValue.asStateFlow()
    private val errorValue = MutableStateFlow<String?>(null)
    val error = errorValue.asStateFlow()
    private val selectedFilesValue = MutableStateFlow<List<NearbySelectedFile>>(emptyList())
    val selectedFiles = selectedFilesValue.asStateFlow()

    private var watch: Job? = null
    private var advertJob: Job? = null
    private var advert: DeviceDiscovery.Advertisement? = null
    private var browser: DeviceDiscovery.Advertisement? = null
    private var sequence = 0L
    private var reloadedReceiveId: ULong? = null

    private fun deviceAlias(): String {
        val model = Build.MODEL?.trim().orEmpty()
        val manufacturer = Build.MANUFACTURER?.trim().orEmpty()
        val raw = if (model.isEmpty()) "Android" else if (manufacturer.isNotEmpty() && !model.startsWith(manufacturer, ignoreCase = true)) "$manufacturer $model" else model
        return nearbySanitizeAlias(raw)
    }

    fun prepareSendFiles(files: List<NearbySelectedFile>) {
        selectedFilesValue.value = files
        errorValue.value = null
    }

    fun startReceive(discoverable: Boolean) {
        val stamp = client.currentStamp() ?: return
        val current = ++sequence
        stopAdvert()
        errorValue.value = null
        localNetworkDeniedValue.value = false
        client.perform({ core ->
            parseNearbyStatus(core.nearbyReceiveStart(deviceAlias(), discoverable).decode())
        }, key = "nearby:receive:start", replace = true, quiet = true, onError = {
            if (current == sequence) errorValue.value = it
        }) { next ->
            if (current != sequence) return@perform
            statusValue.value = next
            val recv = next.receive
            if (discoverable && recv != null && recv.active) {
                advertJob = scope.launch {
                    try {
                        val registered = discovery.advertiseNearby(recv.advert, recv.port, recv.alias, recv.pkHint)
                        if (current == sequence && client.accepts(stamp)) advert = registered else registered.close()
                    } catch (_: CancellationException) { }
                    catch (err: Throwable) {
                        if (current == sequence) {
                            if (err.message == "LOCAL_NETWORK_DENIED") localNetworkDeniedValue.value = true
                            else errorValue.value = "DISCOVERY_UNAVAILABLE"
                        }
                    }
                }
            }
            startWatch(stamp, current)
        }
    }

    fun accept(peerId: String, folder: String, selected: List<UInt>, shareIdentity: Boolean) {
        val stamp = client.currentStamp() ?: return
        val current = sequence
        errorValue.value = null
        client.perform({ core ->
            parseNearbyStatus(core.nearbyReceiveAccept(peerId, folder, selected, shareIdentity).decode())
        }, key = "nearby:receive:decision", replace = true, quiet = true, onError = {
            if (current == sequence) errorValue.value = it
        }) { next ->
            statusValue.value = next
            startWatch(stamp, current)
        }
    }

    fun reject(peerId: String) {
        errorValue.value = null
        client.perform({ core ->
            parseNearbyStatus(core.nearbyReceiveReject(peerId).decode())
        }, key = "nearby:receive:decision", replace = true, quiet = true, onError = {
            errorValue.value = it
        }) { next ->
            statusValue.value = next
        }
    }

    fun cancelReceive() {
        stopAdvert()
        client.cancelOperation("nearby:receive:start")
        client.perform({ core ->
            parseNearbyStatus(core.nearbyReceiveCancel().decode())
        }, key = "nearby:receive:cancel", replace = true, quiet = true) { next ->
            statusValue.value = next
        }
    }

    fun startBrowse() {
        stopBrowse()
        localNetworkDeniedValue.value = false
        browsingValue.value = true
        val ownAdvert = statusValue.value?.receive?.advert
        browser = discovery.browseNearby(
            onPeers = { list ->
                val filtered = list.filter { it.advert != ownAdvert }
                scope.launch { peersValue.value = filtered }
            },
            onDenied = {
                scope.launch { localNetworkDeniedValue.value = true }
            },
        )
    }

    fun stopBrowse() {
        browser?.close()
        browser = null
        browsingValue.value = false
    }

    private fun resolveKnownPeer(offer: String?): NearbyDiscoveredPeer? {
        val text = offer?.trim()?.takeIf { it.isNotEmpty() } ?: return null
        val parsed = runCatching { Json.parseToJsonElement(nearbyParseOffer(text, null)).objectValue() }.getOrNull() ?: return null
        val adv = parsed.text("advert") ?: return null
        return peersValue.value.firstOrNull { it.advert == adv }
    }

    fun sendFiles(offer: String?, peer: NearbyDiscoveredPeer?, files: List<NearbySelectedFile>, shareIdentity: Boolean) {
        if (files.isEmpty()) return
        val stamp = client.currentStamp() ?: return
        val current = ++sequence
        errorValue.value = null
        val matched = if (peer == null) resolveKnownPeer(offer) else null
        val nativeFiles = files.map { NearbyFileItem(it.id, it.version) }
        client.perform({ core ->
            parseNearbyStatus(
                core.nearbySendFiles(
                    offer = offer?.trim()?.takeIf { it.isNotEmpty() },
                    host = peer?.host,
                    port = peer?.port,
                    peerAlias = peer?.alias,
                    resolvedHost = matched?.host,
                    resolvedPort = matched?.port,
                    awdlBridgeId = null,
                    files = nativeFiles,
                    alias = deviceAlias(),
                    shareIdentity = shareIdentity,
                ).decode()
            )
        }, key = "nearby:send:start", replace = true, quiet = true, onError = {
            if (current == sequence) errorValue.value = it
        }) { next ->
            if (current != sequence) return@perform
            statusValue.value = next
            startWatch(stamp, current)
        }
    }

    fun sendVault(offer: String?, peer: NearbyDiscoveredPeer?, input: SecretSnapshotInput, shareIdentity: Boolean) {
        val stamp = client.currentStamp() ?: return
        val current = ++sequence
        errorValue.value = null
        val matched = if (peer == null) resolveKnownPeer(offer) else null
        client.perform({ core ->
            parseNearbyStatus(
                core.nearbySendVault(
                    offer = offer?.trim()?.takeIf { it.isNotEmpty() },
                    host = peer?.host,
                    port = peer?.port,
                    peerAlias = peer?.alias,
                    resolvedHost = matched?.host,
                    resolvedPort = matched?.port,
                    awdlBridgeId = null,
                    input = input,
                    alias = deviceAlias(),
                    shareIdentity = shareIdentity,
                ).decode()
            )
        }, key = "nearby:send:start", replace = true, quiet = true, onError = {
            if (current == sequence) errorValue.value = it
        }) { next ->
            if (current != sequence) return@perform
            statusValue.value = next
            startWatch(stamp, current)
        }
    }

    fun confirmSas() {
        errorValue.value = null
        client.perform({ core ->
            parseNearbyStatus(core.nearbySendConfirmSas().decode())
        }, key = "nearby:send:sas", replace = true, quiet = true, onError = {
            errorValue.value = it
        }) { next ->
            statusValue.value = next
        }
    }

    fun cancelSend() {
        client.cancelOperation("nearby:send:start")
        client.perform({ core ->
            parseNearbyStatus(core.nearbySendCancel().decode())
        }, key = "nearby:send:cancel", replace = true, quiet = true) { next ->
            statusValue.value = next
            if (next.receive?.active != true) stopWatch()
        }
    }

    private fun startWatch(stamp: SessionStamp, current: Long) {
        stopWatch()
        watch = scope.launch {
            while (isActive && current == sequence && client.accepts(stamp)) {
                delay(350)
                if (client.isWorking("nearby:status")) continue
                client.perform({ core -> parseNearbyStatus(core.nearbyStatus().decode()) }, key = "nearby:status", quiet = true) { next ->
                    if (current != sequence) return@perform
                    statusValue.value = next
                    val recv = next.receive
                    if (recv != null && !recv.active && recv.progress.completedItems > 0u && recv.outcome?.kind != "secret_package" && reloadedReceiveId != recv.id) {
                        reloadedReceiveId = recv.id
                        client.library.browse()
                    }
                    if (recv?.active != true) stopAdvert()
                    if (recv?.active != true && next.send?.active != true) stopWatch()
                }
            }
        }
    }

    private fun stopAdvert() {
        advertJob?.cancel()
        advertJob = null
        advert?.close()
        advert = null
    }

    private fun stopWatch() {
        watch?.cancel()
        watch = null
    }

    fun background() {
        ++sequence
        stopWatch()
        stopAdvert()
        stopBrowse()
        client.cancelOperation("nearby:receive:start")
        client.cancelOperation("nearby:receive:decision")
        client.cancelOperation("nearby:send:start")
        client.cancelOperation("nearby:status")
        cancelNative()
        statusValue.value = null
    }

    fun clear() {
        background()
        peersValue.value = emptyList()
        localNetworkDeniedValue.value = false
        errorValue.value = null
        selectedFilesValue.value = emptyList()
        reloadedReceiveId = null
    }
}
