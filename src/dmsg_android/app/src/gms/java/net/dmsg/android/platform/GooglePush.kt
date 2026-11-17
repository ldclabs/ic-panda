package net.dmsg.android.platform

import android.content.Context
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailabilityLight
import com.google.android.gms.tasks.Task
import com.google.android.play.core.integrity.IntegrityManagerFactory
import com.google.android.play.core.integrity.StandardIntegrityManager
import com.google.android.play.core.integrity.StandardIntegrityException
import com.google.android.play.core.integrity.model.StandardIntegrityErrorCode
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeout
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

class GooglePush(private val context: Context) {
    val configuration get() = GoogleConfiguration.load(context)
    val available get() = configuration != null && GoogleApiAvailabilityLight.getInstance().isGooglePlayServicesAvailable(context) == ConnectionResult.SUCCESS
    private var provider: StandardIntegrityManager.StandardIntegrityTokenProvider? = null
    suspend fun token(): String {
        check(available) { "GOOGLE_UNAVAILABLE" }
        initialize(context, checkNotNull(configuration))
        return withTimeout(60_000) { FirebaseMessaging.getInstance().token.awaitGoogle() }
    }
    suspend fun integrity(requestHash: String): String {
        check(available && requestHash.matches(Regex("[A-Za-z0-9_-]{43}"))) { "GOOGLE_UNAVAILABLE" }
        val config = checkNotNull(configuration)
        return withTimeout(60_000) {
            suspend fun request(): String {
                val selected = provider ?: IntegrityManagerFactory.createStandard(context.applicationContext)
                    .prepareIntegrityToken(StandardIntegrityManager.PrepareIntegrityTokenRequest.builder().setCloudProjectNumber(config.projectNumber).build())
                    .awaitGoogle().also { provider = it }
                return selected.request(StandardIntegrityManager.StandardIntegrityTokenRequest.builder().setRequestHash(requestHash).build()).awaitGoogle().token()
            }
            try { request() } catch (error: StandardIntegrityException) {
                if (error.errorCode != StandardIntegrityErrorCode.INTEGRITY_TOKEN_PROVIDER_INVALID) throw error
                provider = null; request()
            }
        }
    }
    companion object {
        private val lock = Any()
        fun initializeConsented(context: Context) {
            val config = GoogleConfiguration.load(context) ?: return
            if (PushDeliveryStore(context).armed()) runCatching { initialize(context, config) }
        }
        private fun initialize(context: Context, config: GoogleConfiguration) = synchronized(lock) {
            if (FirebaseApp.getApps(context).none { it.name == FirebaseApp.DEFAULT_APP_NAME }) {
                val options = FirebaseOptions.Builder().setApplicationId(config.appId).setApiKey(config.apiKey).setProjectId(config.projectId).setGcmSenderId(config.projectNumber.toString()).build()
                FirebaseApp.initializeApp(context.applicationContext, options)
            }
            // Token acquisition is explicit and follows native user consent.
            FirebaseMessaging.getInstance().isAutoInitEnabled = false
        }
    }
}

private suspend fun <T> Task<T>.awaitGoogle(): T = suspendCancellableCoroutine { continuation ->
    addOnSuccessListener { if (continuation.isActive) continuation.resume(it) }
    addOnFailureListener { if (continuation.isActive) continuation.resumeWithException(it) }
    addOnCanceledListener { continuation.cancel() }
}
