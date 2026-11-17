package net.dmsg.android.platform

import android.app.Activity
import android.hardware.biometrics.BiometricManager.Authenticators
import android.hardware.biometrics.BiometricPrompt
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import net.dmsg.core.MobileException
import java.lang.ref.WeakReference
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import javax.crypto.Cipher
import java.security.Signature

/** Rust calls this from its blocking pool, never from Client's owner or the UI thread. */
class PresencePrompts {
    @Volatile private var activity = WeakReference<Activity>(null)
    private val main = Handler(Looper.getMainLooper())
    private val active = AtomicReference<Pending?>()

    val canPrompt: Boolean get() = activity.get()?.let { it.hasWindowFocus() && !it.isFinishing && !it.isDestroyed } == true
    val isAuthenticating: Boolean get() = active.get() != null

    private class Pending(val owner: Any?) {
        val cancel = CancellationSignal()
        val latch = CountDownLatch(1)
        @Volatile var result: Result<BiometricPrompt.CryptoObject>? = null
            private set
        @Synchronized fun complete(value: Result<BiometricPrompt.CryptoObject>) {
            if (result == null) { result = value; latch.countDown() }
        }
    }

    fun attach(value: Activity) { activity = WeakReference(value) }
    fun detach(value: Activity) { if (activity.get() === value) activity.clear() }

    fun cipher(title: String, subtitle: String, cipher: Cipher, biometryOnly: Boolean = false, valid: () -> Boolean = { true }): Cipher =
        authenticate(title, subtitle, BiometricPrompt.CryptoObject(cipher), biometryOnly, valid).cipher
            ?: throw failure("INTEGRITY_FAILED")

    fun signature(title: String, subtitle: String, signature: Signature, biometryOnly: Boolean, owner: Any? = null, valid: () -> Boolean = { true }): Signature =
        authenticate(title, subtitle, BiometricPrompt.CryptoObject(signature), biometryOnly, valid, owner).signature
            ?: throw failure("INTEGRITY_FAILED")

    private fun authenticate(title: String, subtitle: String, crypto: BiometricPrompt.CryptoObject, biometryOnly: Boolean, valid: () -> Boolean, owner: Any? = null): BiometricPrompt.CryptoObject {
        check(Looper.myLooper() != Looper.getMainLooper()) { "PRESENCE_ON_MAIN_THREAD" }
        if (!valid()) throw failure("CANCELLED")
        val pending = Pending(owner)
        if (!active.compareAndSet(null, pending)) throw failure("PRESENCE_BUSY")
        main.post {
            val owner = activity.get()
            if (owner == null || owner.isFinishing || owner.isDestroyed || active.get() !== pending || !valid()) {
                pending.complete(Result.failure(failure("CANCELLED")))
                return@post
            }
            try {
                val builder = BiometricPrompt.Builder(owner).setTitle(title).setSubtitle(subtitle)
                    .setAllowedAuthenticators(if (biometryOnly) Authenticators.BIOMETRIC_STRONG else Authenticators.BIOMETRIC_STRONG or Authenticators.DEVICE_CREDENTIAL)
                if (biometryOnly) builder.setNegativeButton(str(net.dmsg.android.R.string.text_cancel_998b9c4), owner.mainExecutor) { _, _ -> pending.complete(Result.failure(failure("CANCELLED"))) }
                builder.build()
                    .authenticate(crypto, pending.cancel, owner.mainExecutor,
                        object : BiometricPrompt.AuthenticationCallback() {
                            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                                val authenticated = result.cryptoObject
                                if (!valid()) pending.complete(Result.failure(failure("CANCELLED")))
                                else if (authenticated == null) pending.complete(Result.failure(failure("INTEGRITY_FAILED")))
                                else pending.complete(Result.success(authenticated))
                            }

                            override fun onAuthenticationError(code: Int, message: CharSequence?) {
                                val reason = when (code) {
                                    BiometricPrompt.BIOMETRIC_ERROR_CANCELED, BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED -> "CANCELLED"
                                    BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT -> "BIOMETRIC_LOCKOUT"
                                    BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT_PERMANENT -> "BIOMETRIC_LOCKOUT_PERMANENT"
                                    BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS -> "BIOMETRIC_NOT_ENROLLED"
                                    BiometricPrompt.BIOMETRIC_ERROR_NO_DEVICE_CREDENTIAL -> "DEVICE_CREDENTIAL_NOT_SET"
                                    BiometricPrompt.BIOMETRIC_ERROR_SECURITY_UPDATE_REQUIRED -> "HARDWARE_SECURITY_UPDATE_REQUIRED"
                                    BiometricPrompt.BIOMETRIC_ERROR_TIMEOUT -> "AUTHENTICATION_TIMEOUT"
                                    else -> "HARDWARE_UNAVAILABLE"
                                }
                                pending.complete(Result.failure(failure(reason)))
                            }
                        })
            } catch (_: Exception) { pending.complete(Result.failure(failure("PRESENCE_UNAVAILABLE"))) }
        }
        try {
            if (!pending.latch.await(75, TimeUnit.SECONDS)) {
                pending.complete(Result.failure(failure("CANCELLED")))
                main.post { pending.cancel.cancel() }
            }
            if (!valid()) throw failure("CANCELLED")
            return pending.result?.getOrThrow() ?: throw failure("CANCELLED")
        } finally { active.compareAndSet(pending, null) }
    }

    fun cancel() {
        val pending = active.getAndSet(null) ?: return
        pending.complete(Result.failure(failure("CANCELLED")))
        main.post { pending.cancel.cancel() }
    }

    fun cancel(owner: Any) {
        val pending = active.get() ?: return
        if (pending.owner !== owner || !active.compareAndSet(pending, null)) return
        pending.complete(Result.failure(failure("CANCELLED")))
        main.post { pending.cancel.cancel() }
    }

    companion object { fun failure(code: String) = MobileException.Failure(code) }
}
