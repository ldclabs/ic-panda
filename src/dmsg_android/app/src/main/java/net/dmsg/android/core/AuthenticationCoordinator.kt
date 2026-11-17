package net.dmsg.android.core

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.NativePasskeys
import net.dmsg.core.*

data class AuthenticationCompletion(val operation: PasskeyOperation, val approval: String? = null)

data class AuthenticationState(
    val working: Boolean = false,
    val canContinue: Boolean = false,
    val canResetSessions: Boolean = false,
    val error: String? = null,
)

/** Retains the exact credential proof for a bounded retry, in memory only. */
class AuthenticationCoordinator(private val provider: NativePasskeys) {
    private data class Attempt(val start: PasskeyStart, val proof: NativePasskeyResponse, val generation: ULong)
    private var attempt: Attempt? = null
    private var limitedCredential: ByteArray? = null
    private var limitedOperation: PasskeyOperation? = null
    private val mutable = MutableStateFlow(AuthenticationState())
    val state = mutable.asStateFlow()

    fun cancel() {
        provider.cancel()
        attempt?.let { it.proof.clientDataJson.fill(0); it.proof.authenticatorData.fill(0); it.proof.signature.fill(0) }
        attempt = null; limitedCredential = null; limitedOperation = null
        mutable.value = AuthenticationState()
    }

    suspend fun authenticate(core: MobileEngine, operation: PasskeyOperation, credential: ByteArray? = null, resume: Boolean = false): AuthenticationCompletion {
        check(!mutable.value.working) { "PRESENCE_BUSY" }
        mutable.value = mutable.value.copy(working = true, error = null)
        try {
            if (!resume) {
                attempt = null
                val start = core.beginAndroidPasskey(operation, credential, provider.certificate())
                val proof = provider.authenticate(start)
                attempt = Attempt(start, proof, core.controlGeneration())
            }
            val current = attempt ?: throw MobileException.Failure("EXPIRED")
            if (current.generation != core.controlGeneration() || current.start.deadlineMs <= System.currentTimeMillis().toULong()) {
                cancel(); core.cancelLogin(); throw MobileException.Failure("EXPIRED")
            }
            val approval = if (current.start.operation == PasskeyOperation.BIND || current.start.operation == PasskeyOperation.BIND_EXISTING)
                core.completePasskeyBinding(current.start.state, current.proof).decode().objectValue().required("id")
            else { core.completePasskey(current.start.state, current.proof); null }
            val completed = current.start.operation
            attempt = null
            mutable.value = AuthenticationState()
            return AuthenticationCompletion(completed, approval)
        } catch (cancelled: CancellationException) {
            cancel(); core.cancelLogin(); throw cancelled
        } catch (error: Throwable) {
            val code = (error as? MobileException.Failure)?.code ?: "PASSKEY_FAILED"
            if (code == "SESSION_LIMIT_EXCEEDED" && operation in listOf(PasskeyOperation.LOGIN, PasskeyOperation.BIND_EXISTING)) {
                limitedCredential = attempt?.proof?.credentialId?.copyOf(); limitedOperation = operation
            }
            val retryable = code in setOf("UNAVAILABLE", "EXECUTION_UNKNOWN", "OPERATION_FAILED", "PRESENCE_UNAVAILABLE", "KEYCHAIN_UNAVAILABLE")
            val keep = attempt?.let { retryable && it.start.operation !in listOf(PasskeyOperation.LOGOUT_ALL, PasskeyOperation.RESET_SESSIONS) && it.start.deadlineMs > System.currentTimeMillis().toULong() } == true
            if (!keep) attempt = null
            mutable.value = AuthenticationState(canContinue = keep, canResetSessions = limitedCredential != null, error = code)
            throw error
        } finally { mutable.value = mutable.value.copy(working = false) }
    }

    fun retryOperation(): PasskeyOperation? = attempt?.start?.operation
    fun capacityRecovery(): Pair<ByteArray, PasskeyOperation>? = limitedCredential?.let { id -> limitedOperation?.let { id.copyOf() to it } }
}
