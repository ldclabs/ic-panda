package net.dmsg.android.core

import kotlinx.serialization.json.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.core.*

data class AccountDevice(val id: String, val role: String, val capabilities: List<String>, val addedAt: ULong, val addedBy: String?, val revokedAt: ULong?, val current: Boolean) {
    val managesAccount: Boolean get() = revokedAt == null && role == "Administrator" && "RootManage" in capabilities
}
data class AccountDevices(val account: String, val home: String, val version: ULong, val epoch: ULong, val rootState: String, val rootGeneration: ULong, val devices: List<AccountDevice>) {
    val canManage: Boolean get() = devices.any { it.current && it.managesAccount }
}
data class LoginIdentity(val principal: String, val current: Boolean)
data class AccountSecurity(val logins: List<LoginIdentity>, val recoveryDelayMs: ULong, val recoveryEnabled: Boolean, val recoveryUpdating: Boolean)
data class HardwareKey(val id: String, val registeredAt: ULong, val registeredBy: String, val here: Boolean, val biometryOnly: Boolean?, val changed: Boolean?)
data class HardwareDevice(val id: String, val current: Boolean, val key: String?)
data class HardwareSecurity(val available: Boolean, val canManage: Boolean, val bound: Boolean, val boundAvailable: Boolean?, val biometry: Boolean, val key: HardwareKey?, val waiting: List<String>, val policy: List<String>, val devices: List<HardwareDevice>, val boundBiometryOnly: Boolean?, val backgroundOnline: Boolean, val residentAgents: ULong)
data class RecoveryPending(val id: String, val executeAfter: ULong, val expiresAt: ULong, val current: Boolean, val digest: String? = null)
data class RecoveryStatus(val account: String, val enabled: Boolean, val member: Boolean, val ready: Boolean, val delay: ULong, val pending: RecoveryPending?) {
    fun canRequest(now: ULong) = enabled && !member && (pending == null || now >= pending.expiresAt)
    fun canComplete(now: ULong) = enabled && (member || pending?.let { it.current && now >= it.executeAfter && now < it.expiresAt } == true)
}
fun JsonObject.optionalFlag(key: String): Boolean? = if (this[key] == null || this[key] == JsonNull) null else requiredFlag(key)
fun parseDevices(value: JsonElement): AccountDevices {
    val row = value.objectValue()
    return AccountDevices(row.required("account"), row.required("home"), row.requiredUnsigned("account_version"), row.requiredUnsigned("security_epoch"), row.required("vault_write_state"), row.requiredUnsigned("root_generation"), row.rows("devices").map {
        AccountDevice(it.required("device_id"), it.required("role"), it.strings("capabilities"), it.requiredUnsigned("added_at"), it.text("added_by"), it.unsigned("revoked_at"), it.requiredFlag("current"))
    })
}
fun parseSecurity(value: JsonElement): AccountSecurity {
    val row = value.objectValue()
    return AccountSecurity(row.rows("logins").map { LoginIdentity(it.required("principal"), it.requiredFlag("current")) }, row.requiredUnsigned("recovery_delay_ms"), row.requiredFlag("recovery_enabled"), row.requiredFlag("recovery_updating"))
}
fun parseHardware(value: JsonElement): HardwareSecurity {
    val row = value.objectValue(); val unprotected = row["unprotected"]!!.objectValue()
    val key = row["key"]?.takeIf { it != JsonNull }?.objectValue()?.let { HardwareKey(it.required("key_id"), it.requiredUnsigned("registered_at"), it.required("registered_by"), it.requiredFlag("here"), it.optionalFlag("biometry_only"), it.optionalFlag("fingerprints_changed")) }
    return HardwareSecurity(row.requiredFlag("available"), row.requiredFlag("can_manage_policy"), row.requiredFlag("device_bound"), row.optionalFlag("bound_key_available"), row.requiredFlag("biometry"), key, row.strings("waiting"), row.strings("policy"), row.rows("devices").map { HardwareDevice(it.required("device_id"), it.requiredFlag("this_device"), it.text("key_id")) }, row.optionalFlag("bound_biometry_only"), unprotected.requiredFlag("background_online"), unprotected.requiredUnsigned("resident_agents"))
}
fun parseRecovery(value: JsonElement): RecoveryStatus {
    val row = value.objectValue()
    return RecoveryStatus(row.required("account"), row.requiredFlag("enabled"), row.requiredFlag("member"), row.requiredFlag("ready"), row.requiredUnsigned("recovery_delay_ms"), row["pending"]?.takeIf { it != JsonNull }?.objectValue()?.let { RecoveryPending(it.required("op_id"), it.requiredUnsigned("execute_after"), it.requiredUnsigned("expires_at"), it.requiredFlag("this_device"), it.required("digest").also { digest -> check(digest.matches(Regex("[a-f0-9]{64}"))) { "INTEGRITY_FAILED" } }) })
}

data class HardwareDraft(val policy: Set<String> = emptySet(), val biometryOnly: Boolean = false, val token: String = "", val device: String = "")
data class RecoveryDraft(val enabled: Boolean = false, val days: Int = 3)

class AccountCoordinator(private val client: ClientController) {
    val devices = ReadStore<AccountDevices>()
    val security = ReadStore<AccountSecurity>()
    val hardware = ReadStore<HardwareSecurity>()
    val recovery = ReadStore<RecoveryStatus>()
    private val mutableHardware = MutableStateFlow(HardwareDraft())
    val hardwareDraft = mutableHardware.asStateFlow()
    private val mutableRecovery = MutableStateFlow(RecoveryDraft())
    val recoveryDraft = mutableRecovery.asStateFlow()
    private var hardwareDirty = false
    private var recoveryDirty = false
    fun editHardware(value: HardwareDraft) { mutableHardware.value = value; hardwareDirty = true }
    fun editRecovery(value: RecoveryDraft) { mutableRecovery.value = value; recoveryDirty = true }
    fun devices() {
        val request = devices.begin()
        client.perform({ parseDevices(it.devices().decode()) }, key = "account:devices", replace = true, onError = { devices.fail(request, it) }) { devices.accept(request, it) }
    }
    fun security() {
        val request = security.begin()
        client.perform({ parseSecurity(it.security().decode()) }, key = "account:security", replace = true, onError = { security.fail(request, it) }) { security.accept(request, it); if (!recoveryDirty) mutableRecovery.value = RecoveryDraft(it.recoveryEnabled, (it.recoveryDelayMs / 86_400_000uL).toInt().coerceIn(1, 7)) }
        devices()
    }
    fun hardware() {
        val request = hardware.begin()
        client.perform({ parseHardware(it.hardwareSecurity().decode()) }, key = "account:hardware", replace = true, onError = { hardware.fail(request, it) }) { hardware.accept(request, it); if (!hardwareDirty) mutableHardware.value = HardwareDraft(it.policy.toSet()) }
    }
    fun recovery() {
        val request = recovery.begin()
        client.perform({ parseRecovery(it.recoveryStatus().decode()) }, key = "account:recovery", replace = true, quiet = true, onError = { recovery.fail(request, it) }) { recovery.accept(request, it) }
    }
    fun clear(retainForms: Boolean = false) {
        devices.reset(); security.reset(); hardware.reset(); recovery.reset()
        if (!retainForms) { mutableHardware.value = HardwareDraft(); mutableRecovery.value = RecoveryDraft(); hardwareDirty = false; recoveryDirty = false }
    }
}
