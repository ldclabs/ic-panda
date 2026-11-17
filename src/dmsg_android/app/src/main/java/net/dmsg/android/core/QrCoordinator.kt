package net.dmsg.android.core

import android.content.Context
import android.net.Uri
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.QrImages

data class QrResult(val target: String, val text: String)
data class QrRequest(val id: Long, val target: String, val stamp: SessionStamp, val showing: Boolean = true, val permissionRequested: Boolean = false)
data class QrState(val request: QrRequest? = null, val result: QrResult? = null, val reading: Boolean = false)

/** The Activity result outlives a concealed page but never its native session. */
class QrCoordinator(private val client: ClientController) {
    private var sequence = 0L
    private var imageRequest: QrRequest? = null
    private val mutable = MutableStateFlow(QrState())
    val state = mutable.asStateFlow()
    fun open(target: String) { val stamp = client.currentStamp() ?: return; imageRequest = null; client.cancelOperation("qr:read"); mutable.value = QrState(request = QrRequest(++sequence, target, stamp)) }
    fun permission() { val request = mutable.value.request ?: return; client.beginExternalInteraction(); mutable.value = mutable.value.copy(request = request.copy(permissionRequested = true)) }
    fun chooseImage() { val request = mutable.value.request ?: return; imageRequest = request; client.beginExternalInteraction(); mutable.value = mutable.value.copy(request = request.copy(showing = false), reading = true) }
    fun image(context: Context, uri: Uri?) {
        val request = imageRequest ?: return
        imageRequest = null
        if (mutable.value.request?.id != request.id) return
        if (uri == null) { mutable.value = QrState(); return }
        client.deliverExternalResult("qr-image", request.stamp) {
            if (mutable.value.request?.id == request.id) client.perform({ QrImages.read(context, uri) }, key = "qr:read", replace = true,
                onError = { mutable.value = QrState(); client.showError("QR_NOT_FOUND") }) { accept(it, request) }
        }
    }
    fun accept(text: String, expected: QrRequest? = mutable.value.request) {
        val current = mutable.value.request ?: return
        val owner = expected ?: return
        if (owner.id != current.id || owner.target != current.target || owner.stamp != current.stamp || !client.sessionCurrent(current.stamp)) return
        if (text.length > 16 * 1024) { clear(); client.showError("FILE_LIMIT"); return }
        mutable.value = QrState(result = QrResult(current.target, text))
    }
    fun consume(target: String): String? {
        val result = mutable.value.result?.takeIf { it.target == target } ?: return null
        mutable.value = QrState(); return result.text
    }
    fun clear() { imageRequest = null; client.cancelOperation("qr:read"); mutable.value = QrState() }
}
