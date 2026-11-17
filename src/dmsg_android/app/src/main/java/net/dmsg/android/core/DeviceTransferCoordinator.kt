package net.dmsg.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.DeviceDiscovery
import net.dmsg.android.platform.TransferAddress
import net.dmsg.core.transferAdvert

fun transferHex(value: String) = value.matches(Regex("[0-9a-f]{64}"))
fun validTransferSession(value: String) = value.matches(Regex("[0-9a-f]{32}"))
data class DeviceTransferReport(val peer: String?, val entries: ULong, val records: ULong, val kept: ULong, val skipped: ULong, val blobs: ULong, val bytes: ULong, val total: ULong)
data class DeviceTransferStatus(val role: String, val session: String, val advert: String, val to: String, val succession: String?, val port: UShort?, val addresses: List<String>, val state: String, val error: String?, val report: DeviceTransferReport) {
    val running get() = state in setOf("waiting", "running")
}
data class DeviceTransferDraft(val session: String = "", val device: String = "", val address: String = "") {
    fun valid(succession: String?) = validTransferSession(session.trim()) && (succession != null || transferHex(device.trim())) && (address.isBlank() || TransferAddress.parse(address) != null)
}
data class DeviceTransferPlan(val receive: Boolean, val session: String?, val to: String?, val address: TransferAddress?, val succession: String?)
data class PendingDeviceSend(val plan: DeviceTransferPlan, val approval: ApprovalDetail)
fun parseDeviceTransfer(value: JsonElement): DeviceTransferStatus? {
    if (value == JsonNull) return null
    val row = value.objectValue(); val report = row["report"]!!.objectValue()
    val port = row.unsigned("port")?.also { check(it in 1uL..65535uL) { "INTEGRITY_FAILED" } }?.toUShort()
    return DeviceTransferStatus(row.required("role"), row.required("session"), row.required("advert"), row.required("to"), row.text("succession"), port, row.strings("addresses"), row.required("state"), row["error"]?.takeIf { it != JsonNull }?.objectValue()?.required("code"),
        DeviceTransferReport(report.text("peer"), report.requiredUnsigned("entries"), report.requiredUnsigned("records"), report.requiredUnsigned("kept"), report.requiredUnsigned("skipped"), report.requiredUnsigned("blobs"), report.requiredUnsigned("bytes"), report.requiredUnsigned("total_bytes"))).also {
        check(it.role in setOf("receive", "send") && validTransferSession(it.session) && transferHex(it.to) && it.state in setOf("waiting", "running", "done", "failed", "cancelled")) { "INTEGRITY_FAILED" }
    }
}

/** One account-owned live transfer. Background drops the socket and advertisement.
 * Receive can reopen the same session; another send always needs a fresh review. */
class DeviceTransferCoordinator(private val client: ClientController, private val scope: CoroutineScope, private val discovery: DeviceDiscovery, private val stopNative: () -> Unit) {
    val status = ReadStore<DeviceTransferStatus?> { it == null }
    private val mutableDraft = MutableStateFlow(DeviceTransferDraft())
    val draft = mutableDraft.asStateFlow()
    private val pendingValue = MutableStateFlow<PendingDeviceSend?>(null)
    val pending = pendingValue.asStateFlow()
    private val noticeValue = MutableStateFlow<String?>(null)
    val notice = noticeValue.asStateFlow()
    private val suspendedValue = MutableStateFlow(false)
    val suspended = suspendedValue.asStateFlow()
    private var plan: DeviceTransferPlan? = null
    private var owner: String? = null
    private var watch: Job? = null
    private var advertJob: Job? = null
    private var advert: DeviceDiscovery.Advertisement? = null
    private var sequence = 0L
    fun edit(value: DeviceTransferDraft) { client.activity(); mutableDraft.value = value }
    fun load() {
        if (client.isWorking("device-transfer:status")) return
        val read = status.begin()
        client.perform({ parseDeviceTransfer(it.transferStatus().decode()) }, key = "device-transfer:status", quiet = true, onError = { status.fail(read, it) }) {
            status.accept(read, it)
            if (it?.running == false) { stopWatching(); if (it.state == "done") { plan = null; suspendedValue.value = false; client.library.browse() } }
        }
    }
    fun receive(succession: String? = null) = start(DeviceTransferPlan(true, null, null, null, succession))
    fun send(succession: String? = null) {
        val form = mutableDraft.value
        if (!form.valid(succession)) return
        start(DeviceTransferPlan(false, form.session.trim(), form.device.trim().ifEmpty { null }, form.address.takeUnless { it.isBlank() }?.let(TransferAddress::parse), succession))
    }
    fun resume() {
        val last = plan ?: status.value()?.let { DeviceTransferPlan(it.role == "receive", it.session, it.to, it.addresses.firstOrNull()?.let(TransferAddress::parse), it.succession) } ?: return
        start(last) // No old export approval can be reused.
    }
    fun openApproval() { pendingValue.value?.let { client.approvals.open(it.approval.id) } }
    fun sendApproved() {
        val pending = pendingValue.value ?: return
        if (client.isWorking("device-transfer:approval")) return
        client.perform({ core -> parseApproval(core.approval(pending.approval.id).decode()).also { check(it.digest == pending.approval.digest && it.state == "done") { "PENDING" } } },
            key = "device-transfer:approval") {
            if (pendingValue.value == pending) { pendingValue.value = null; start(pending.plan, it.id) }
        }
    }
    private fun start(next: DeviceTransferPlan, approval: String? = null) {
        if (client.isWorking("device-transfer:start") || status.value()?.running == true) return
        val stamp = client.currentStamp() ?: return
        val current = ++sequence
        stopWatching(); pendingValue.value = null; noticeValue.value = null; suspendedValue.value = false
        owner = stamp.account; plan = next
        client.perform({ core ->
            if (next.receive) core.transferReceiveStart(next.session, next.succession).decode()
            else {
                val address = next.address ?: discovery.find(transferAdvert(next.session!!))
                core.transferSendStart(next.session!!, next.to, address.host, address.port, next.succession, approval).decode()
            }
        }, key = "device-transfer:start", quiet = true, onError = {
            if (current == sequence) { noticeValue.value = it; suspendedValue.value = next.session != null; load() }
        }) { result ->
            if (current != sequence) return@perform
            val request = (result as? JsonObject)?.get("approval")?.takeIf { it != JsonNull }
            if (request != null) {
                pendingValue.value = PendingDeviceSend(next, parseApproval(request))
                client.approvals.open(pendingValue.value!!.approval.id)
            } else {
                val transfer = parseDeviceTransfer(result) ?: error("INTEGRITY_FAILED")
                plan = next.copy(session = transfer.session)
                val read = status.begin(); status.accept(read, transfer)
                if (transfer.role == "receive" && transfer.port != null) advertJob = scope.launch {
                    try {
                        val registered = discovery.advertise(transfer.advert, transfer.port)
                        if (current == sequence && client.accepts(stamp)) advert = registered else registered.close()
                    } catch (_: CancellationException) { }
                    catch (_: Throwable) { if (current == sequence) noticeValue.value = "DISCOVERY_UNAVAILABLE" }
                }
                watch = scope.launch { while (isActive && current == sequence && client.accepts(stamp)) { delay(500); load() } }
            }
        }
    }
    private fun stopWatching() { watch?.cancel(); watch = null; advertJob?.cancel(); advertJob = null; advert?.close(); advert = null }
    fun cancel() {
        ++sequence; client.cancelOperation("device-transfer:start"); client.cancelOperation("device-transfer:approval")
        stopNative(); stopWatching(); pendingValue.value = null; suspendedValue.value = false
        load()
    }
    fun background() {
        val ongoing = status.value()?.running == true
        ++sequence; stopNative(); stopWatching()
        client.cancelOperation("device-transfer:start"); client.cancelOperation("device-transfer:approval"); client.cancelOperation("device-transfer:status")
        if (ongoing) suspendedValue.value = true
        status.reset(); pendingValue.value = null
    }
    fun foreground() {
        if (owner != client.state.value.session?.account || !client.state.value.unlocked || client.state.value.concealed) return
        if (suspendedValue.value && plan?.receive == true) plan?.let { start(it) } else load()
    }
    fun clear(retainPlan: Boolean = false) {
        background()
        if (!retainPlan) { plan = null; owner = null; mutableDraft.value = DeviceTransferDraft(); noticeValue.value = null; suspendedValue.value = false }
    }
}
