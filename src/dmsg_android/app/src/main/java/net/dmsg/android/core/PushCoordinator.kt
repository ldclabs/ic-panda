package net.dmsg.android.core

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.GooglePush
import net.dmsg.android.platform.PushDeliveryStore
import net.dmsg.android.platform.SystemExportStore
import net.dmsg.android.platform.SystemNotices
import net.dmsg.core.AndroidIntegrityProof
import net.dmsg.core.AndroidPushRegistration
import java.util.Locale

/** Main-owner registration only. The FCM service never instantiates this coordinator. */
class PushCoordinator(private val client: ClientController, context: Context, private val scope: CoroutineScope) {
    enum class Status { UNCONFIGURED, CONSENT, PERMISSION, LOCKED, INACTIVE, READY, WORKING, FAILED }
    private val platform = GooglePush(context)
    private val store = PushDeliveryStore(context)
    private val system = SystemNotices(context)
    private val value = MutableStateFlow(Status.UNCONFIGURED)
    val status = value.asStateFlow()
    val configured get() = platform.available
    private var active = false
    private var running = false
    private var again = false
    private var wake: Job? = null
    fun start() { if (!client.state.value.unlocked || client.state.value.concealed) return; if (!active) { active = true; reconcile() } }
    fun stop() { active = false; wake?.cancel(); wake = null; client.cancelOperation("push:registration"); running = false; again = false }
    fun enable() { val account = client.state.value.session?.account ?: return; client.activity(); store.consent(account); reconcile(force = true) }
    fun changed(preferences: NoticePreferences) {
        val remove = if (!preferences.global.enabled) store.leases().map { it.registration }
            else if (preferences.account?.enabled == false) listOfNotNull(preferences.accountId?.let { store.lease(it)?.registration }) else emptyList()
        store.edit(preferences)
        remove.forEach(system::removeRegistration)
    }
    fun forget(account: String) {
        val registration = store.lease(account)?.registration
        store.forget(account)
        registration?.let(system::removeRegistration)
    }
    private data class Registered(val lease: PushDeliveryStore.Lease, val preferences: String, val sequence: Long)
    private data class Outcome(val status: Status, val registered: Registered? = null)
    fun reconcile(force: Boolean = false) {
        if (!platform.available) { value.value = Status.UNCONFIGURED; return }
        if (!store.consented()) { value.value = Status.CONSENT; return }
        if (!system.allowed()) { value.value = Status.PERMISSION; return }
        if (!client.state.value.unlocked || client.state.value.concealed) { value.value = Status.LOCKED; return }
        if (running) { if (force) again = true; return }
        val stamp = client.currentStamp() ?: return
        val account = stamp.account ?: return
        val device = client.state.value.session?.deviceId ?: return
        if (!store.canRegister(account)) { value.value = Status.INACTIVE; return }
        val sequence = store.sequence()
        running = true; value.value = Status.WORKING
        client.perform({ core ->
            val preferences = parseNoticePreferences(core.notificationPreferences(account).decode())
            check(preferences.accountId == account) { "INTEGRITY_FAILED" }
            if (!preferences.global.enabled || preferences.account?.enabled != true) {
                // Native receive revocation still owns its restricted bearer. Provider removal
                // needs this unlocked device, and is retried on the next explicit foreground.
                core.androidUnregisterPush(core.notificationInstallation())
                core.notificationRefresh()
                return@perform Outcome(Status.INACTIVE)
            }
            val config = checkNotNull(platform.configuration)
            val remote = core.androidConfiguration().decode().objectValue()
            check(remote.requiredFlag("available") && remote.required("protocol") == "dmsg-android/1" && remote.required("package_name") == config.packageName && remote.required("project_id") == config.projectId && remote.required("cloud_project_number") == config.projectNumber.toString() && remote.required("environment") == config.environment) { "GOOGLE_UNAVAILABLE" }
            val token = platform.token()
            val tokenHash = SystemExportStore.digest(token.toByteArray())
            val old = store.lease(account)
            val now = System.currentTimeMillis()
            if (!force && old != null && old.tokenHash == tokenHash && old.expires > now + 86_400_000 && !preferences.pending && !preferences.registrationRequired && !preferences.conflict && old.preferences == preferences && !store.blocked()) return@perform Outcome(Status.READY)
            val installation = core.notificationInstallation()
            val details = AndroidPushRegistration(installation, token, config.projectId)
            val challenge = core.androidPushChallenge(details).decode().objectValue()
            val challengeId = challenge.required("challenge_id")
            val requestHash = challenge.required("request_hash")
            check(validNoticeHash(challengeId) && requestHash.matches(Regex("[A-Za-z0-9_-]{43}"))) { "INTEGRITY_FAILED" }
            val expires = challenge.requiredUnsigned("expires_at")
            check(expires > now.toULong() && expires <= (now + 310_000).toULong()) { "INTEGRITY_FAILED" }
            val integrity = platform.integrity(requestHash)
            check(System.currentTimeMillis().toULong() < expires) { "RESULT_EXPIRED" }
            val registered = core.androidRegisterPush(details, AndroidIntegrityProof(challengeId, integrity), requestId()).decode().objectValue()
            check(registered.required("installation") == installation) { "INTEGRITY_FAILED" }
            val providerRevision = registered.requiredUnsigned("revision")
            val providerExpiry = registered.requiredUnsigned("expires_at")
            check(providerRevision > 0uL && providerExpiry > System.currentTimeMillis().toULong() && providerExpiry <= (System.currentTimeMillis() + 7 * 86_400_000L + 60_000).toULong()) { "INTEGRITY_FAILED" }
            val received = core.notificationRegisterAndroid(installation, providerRevision, Locale.getDefault().toLanguageTag()).decode().objectValue()
            check(received.required("account") == account && received.required("installation") == installation && received.required("device") == device) { "INTEGRITY_FAILED" }
            val registration = received.required("registration")
            val receiptExpiry = received.requiredUnsigned("expires_at")
            check(validNoticeHash(registration) && receiptExpiry > System.currentTimeMillis().toULong() && receiptExpiry <= (System.currentTimeMillis() + 7 * 86_400_000L + 60_000).toULong()) { "INTEGRITY_FAILED" }
            val freshJson = core.notificationPreferences(account).json
            val fresh = parseNoticePreferences(kotlinx.serialization.json.Json.parseToJsonElement(freshJson))
            if (!fresh.global.enabled || fresh.account?.enabled != true) return@perform Outcome(Status.INACTIVE)
            val lease = PushDeliveryStore.Lease(account, installation, registration, providerRevision, minOf(providerExpiry, receiptExpiry).coerceAtMost(Long.MAX_VALUE.toULong()).toLong(), tokenHash, fresh)
            Outcome(Status.READY, Registered(lease, freshJson, sequence))
        }, key = "push:registration", quiet = true, onError = {
            running = false; value.value = Status.FAILED
            schedule()
            retryDeferred()
        }) { outcome ->
            running = false
            val registered = outcome.registered
            val saved = registered == null || store.put(registered.lease, registered.preferences, registered.sequence)
            value.value = if (saved) outcome.status else Status.WORKING
            if (!saved) again = true
            if (outcome.status == Status.INACTIVE) store.forget(account)
            client.notifications.load(); client.notices.refresh(receive = true)
            schedule()
            retryDeferred()
        }
    }
    private fun retryDeferred() {
        if (again) { again = false; scope.launch { yield(); if (client.state.value.unlocked && !client.state.value.concealed) reconcile(force = true) } }
    }
    private fun schedule() { wake?.cancel(); if (active) wake = scope.launch { delay(15 * 60_000); if (active) reconcile() } }
}
