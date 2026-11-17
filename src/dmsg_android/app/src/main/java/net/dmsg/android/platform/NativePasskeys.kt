package net.dmsg.android.platform

import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import androidx.credentials.*
import androidx.credentials.exceptions.*
import kotlinx.coroutines.*
import net.dmsg.core.MobileException
import net.dmsg.core.NativePasskeyResponse
import net.dmsg.core.PasskeyStart
import java.lang.ref.WeakReference
import java.security.MessageDigest

class NativePasskeys(private val context: Context) {
    private var activity = WeakReference<Activity>(null)
    private var active: Job? = null
    val isAuthenticating: Boolean get() = active != null

    fun attach(owner: Activity) { activity = WeakReference(owner) }
    fun detach(owner: Activity) {
        if (activity.get() === owner) { cancel(); activity.clear() }
    }
    fun cancel() { active?.cancel(); active = null }

    /** Current APK signer, including Play App Signing; never a hardcoded upload/debug hash. */
    fun certificate(): ByteArray {
        val info = context.packageManager.getPackageInfo(context.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        val signers = info.signingInfo?.apkContentsSigners ?: throw MobileException.Failure("PASSKEY_ORIGIN_MISMATCH")
        if (signers.size != 1) throw MobileException.Failure("PASSKEY_ORIGIN_MISMATCH")
        return MessageDigest.getInstance("SHA-256").digest(signers.single().toByteArray())
    }

    suspend fun authenticate(start: PasskeyStart): NativePasskeyResponse = withContext(Dispatchers.Main.immediate) {
        if (active != null) throw MobileException.Failure("PRESENCE_BUSY")
        val owner = activity.get()?.takeUnless { it.isFinishing || it.isDestroyed }
            ?: throw MobileException.Failure("CANCELLED")
        val job = currentCoroutineContext()[Job]!!
        active = job
        try {
            val request = PasskeyWire.request(start, System.currentTimeMillis().toULong())
            val manager = CredentialManager.create(context)
            val raw = if (PasskeyWire.creating(start.operation)) {
                val result = manager.createCredential(owner, CreatePublicKeyCredentialRequest(requestJson = request))
                (result as? CreatePublicKeyCredentialResponse)?.registrationResponseJson
            } else {
                val result = manager.getCredential(owner, GetCredentialRequest(listOf(GetPublicKeyCredentialOption(request))))
                (result.credential as? PublicKeyCredential)?.authenticationResponseJson
            } ?: throw MobileException.Failure("INTEGRITY_FAILED")
            currentCoroutineContext().ensureActive()
            if (System.currentTimeMillis().toULong() >= start.deadlineMs) throw MobileException.Failure("EXPIRED")
            PasskeyWire.response(raw, start, certificate())
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: GetCredentialCancellationException) { throw MobileException.Failure("CANCELLED") }
        catch (_: CreateCredentialCancellationException) { throw MobileException.Failure("CANCELLED") }
        catch (_: NoCredentialException) { throw MobileException.Failure("PASSKEY_NOT_FOUND") }
        catch (_: GetCredentialProviderConfigurationException) { throw MobileException.Failure("PASSKEY_PROVIDER_UNAVAILABLE") }
        catch (_: CreateCredentialProviderConfigurationException) { throw MobileException.Failure("PASSKEY_PROVIDER_UNAVAILABLE") }
        catch (_: GetCredentialUnsupportedException) { throw MobileException.Failure("PASSKEY_PROVIDER_UNAVAILABLE") }
        catch (_: CreateCredentialUnsupportedException) { throw MobileException.Failure("PASSKEY_PROVIDER_UNAVAILABLE") }
        finally { if (active === job) active = null }
    }
}
