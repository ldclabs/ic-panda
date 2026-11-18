package net.dmsg.android.core

import android.content.Context
import android.net.Uri
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.android.platform.QrImages

sealed interface QrState {
    data object Idle : QrState
    val result: QrResult? get() = this as? QrResult
}

enum class QrStep { CAMERA, CAMERA_PROMPTED, PICKING_IMAGE, READING_IMAGE }

data class QrRequest(val id: Long, val target: String, val stamp: SessionStamp, val step: QrStep = QrStep.CAMERA) : QrState {
    val scanning get() = step == QrStep.CAMERA || step == QrStep.CAMERA_PROMPTED
}
data class QrResult(val target: String, val text: String) : QrState

/** Camera and document picker results belong to one request in one native session. */
class QrCoordinator(private val client: ClientController) {
    private var sequence = 0L
    private val mutable = MutableStateFlow<QrState>(QrState.Idle)
    val state = mutable.asStateFlow()

    fun open(target: String) {
        val stamp = client.currentStamp() ?: return
        clear()
        mutable.value = QrRequest(++sequence, target, stamp)
    }

    private fun camera(id: Long) = (mutable.value as? QrRequest)?.takeIf { it.id == id && it.scanning }

    fun permission(id: Long) {
        val request = camera(id) ?: return
        client.beginExternalInteraction()
        mutable.value = request.copy(step = QrStep.CAMERA_PROMPTED)
    }

    fun chooseImage(id: Long): Boolean {
        val request = camera(id) ?: return false
        client.beginExternalInteraction()
        mutable.value = request.copy(step = QrStep.PICKING_IMAGE)
        return true
    }

    fun image(context: Context, id: Long, uri: Uri?) {
        val request = (mutable.value as? QrRequest)?.takeIf { it.id == id && it.step == QrStep.PICKING_IMAGE } ?: return
        if (uri == null) { mutable.value = QrState.Idle; return }
        val reading = request.copy(step = QrStep.READING_IMAGE)
        mutable.value = reading
        client.deliverExternalResult("qr-image", reading.stamp, discard = { mutable.compareAndSet(reading, QrState.Idle) }) {
            if (mutable.value == reading) client.perform({ QrImages.read(context, uri) }, key = "qr:read", quiet = true, onError = { code ->
                if (mutable.compareAndSet(reading, QrState.Idle)) client.showError(if (code == "NOT_FOUND") "QR_NOT_FOUND" else code)
            }) { finish(reading, it) }
        }
    }

    fun cameraResult(id: Long, text: String) { camera(id)?.let { finish(it, text) } }

    private fun finish(request: QrRequest, text: String) {
        if (mutable.value != request || !client.sessionCurrent(request.stamp)) return
        if (text.length > 16 * 1024) { clear(); client.showError("FILE_LIMIT"); return }
        mutable.value = QrResult(request.target, text)
    }

    fun consume(target: String): String? {
        val result = mutable.value.result?.takeIf { it.target == target } ?: return null
        mutable.value = QrState.Idle
        return result.text
    }

    fun clear() {
        mutable.value = QrState.Idle
        client.cancelOperation("external-result:qr-image")
        client.cancelOperation("qr:read")
    }
}
