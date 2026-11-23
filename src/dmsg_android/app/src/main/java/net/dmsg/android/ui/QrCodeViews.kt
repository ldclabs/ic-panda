package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import android.util.Base64
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.view.CameraController
import androidx.camera.view.LifecycleCameraController
import androidx.camera.view.PreviewView
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.*
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.QrRequest
import net.dmsg.android.core.QrStep
import net.dmsg.core.qrPng
import net.dmsg.core.scanQrLuma
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

@Composable fun QrCode(text: String) {
    var bitmap by remember(text) { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(text) {
        bitmap = withContext(Dispatchers.Default) {
            runCatching {
                val data = qrPng(text).removePrefix("data:image/png;base64,")
                val bytes = Base64.decode(data, Base64.DEFAULT)
                try { android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.asImageBitmap() }
                finally { bytes.fill(0) }
            }.getOrNull()
        }
    }
    bitmap?.let { Image(it, str(R.string.text_device_request_qr_code_34cd075), Modifier.fillMaxWidth().heightIn(max = 280.dp).padding(12.dp)) }
        ?: Notice(str(R.string.text_the_text_is_available_to_copy_while_the_qr_code_is_prepared_large_bc7825e))
}

/** Keep the launcher registered while its scanner is concealed or reading the selected image. */
@Composable internal fun QrScannerHost(client: ClientController, concealed: Boolean) {
    val state by client.qr.state.collectAsStateWithLifecycle()
    val request = state as? QrRequest ?: return
    val context = LocalContext.current
    key(client, request.id) {
        val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            client.qr.image(context, request.id, uri)
        }
        if (request.scanning && !concealed) {
            QrScanner(requestPermission = request.step == QrStep.CAMERA, beforePermission = { client.qr.permission(request.id) },
                readImage = {
                    if (client.qr.chooseImage(request.id)) {
                        try { picker.launch(arrayOf("image/*")) }
                        catch (_: ActivityNotFoundException) {
                            client.qr.image(context, request.id, null)
                            client.showError("FILE_UNAVAILABLE")
                        }
                    }
                }, close = client.qr::clear, decoded = { client.qr.cameraResult(request.id, it) })
        }
    }
}

@Composable private fun QrScanner(requestPermission: Boolean, beforePermission: () -> Unit, readImage: () -> Unit, close: () -> Unit, decoded: (String) -> Unit) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current
    var allowed by remember { mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) }
    var cameraUnavailable by remember { mutableStateOf(false) }
    var front by remember { mutableStateOf(false) }
    val receive by rememberUpdatedState(decoded)
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed = it }
    LaunchedEffect(Unit) { if (!allowed && requestPermission) { beforePermission(); permission.launch(Manifest.permission.CAMERA) } }
    DisposableEffect(lifecycle, context) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) allowed = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED }
        lifecycle.lifecycle.addObserver(observer)
        onDispose { lifecycle.lifecycle.removeObserver(observer) }
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().systemBarsPadding().padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    TextButton(onClick = close) { Text(str(R.string.text_close_6cf4a77)) }
                    TextButton(onClick = { front = !front }) { Text(str(R.string.text_switch_camera_01e5df0)) }
                }
                Text(str(R.string.text_scan_a_qr_code_bbd1380), style = MaterialTheme.typography.headlineSmall)
                Notice(str(R.string.text_scanning_only_reads_the_request_review_the_account_device_and_per_4f5b558))
                if (allowed) {
                    val controller = remember(context, front) {
                        LifecycleCameraController(context).apply {
                            cameraSelector = if (front) CameraSelector.DEFAULT_FRONT_CAMERA else CameraSelector.DEFAULT_BACK_CAMERA
                            setEnabledUseCases(CameraController.IMAGE_ANALYSIS)
                            imageAnalysisBackpressureStrategy = ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST
                            imageAnalysisTargetSize = CameraController.OutputSize(Size(1280, 720))
                        }
                    }
                    DisposableEffect(controller, lifecycle) {
                        val alive = AtomicBoolean(true)
                        val accepted = AtomicBoolean(false)
                        val executor = Executors.newSingleThreadExecutor()
                        val main = ContextCompat.getMainExecutor(context)
                        var last = 0L
                        controller.setImageAnalysisAnalyzer(executor) { frame ->
                            try {
                                val now = android.os.SystemClock.elapsedRealtime()
                                if (!alive.get() || accepted.get() || now - last < 300) return@setImageAnalysisAnalyzer
                                last = now
                                val width = frame.width; val height = frame.height
                                if (width <= 0 || height <= 0 || width.toLong() * height > 4_000_000) return@setImageAnalysisAnalyzer
                                val plane = frame.planes[0]; val buffer = plane.buffer.duplicate(); val origin = buffer.position()
                                val end = origin.toLong() + (height - 1L) * plane.rowStride + (width - 1L) * plane.pixelStride
                                if (end >= buffer.limit()) return@setImageAnalysisAnalyzer
                                val pixels = ByteArray(width * height)
                                val found = try {
                                    for (y in 0 until height) for (x in 0 until width) pixels[y * width + x] = buffer.get(origin + y * plane.rowStride + x * plane.pixelStride)
                                    runCatching { scanQrLuma(width.toUInt(), height.toUInt(), pixels) }.getOrNull()
                                } finally { pixels.fill(0) }
                                if (found != null && alive.get() && accepted.compareAndSet(false, true)) main.execute {
                                    if (alive.get() && lifecycle.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) receive(found)
                                    else accepted.set(false)
                                }
                            } finally { frame.close() }
                        }
                        cameraUnavailable = runCatching { controller.bindToLifecycle(lifecycle) }.isFailure
                        onDispose { alive.set(false); controller.clearImageAnalysisAnalyzer(); controller.unbind(); executor.shutdownNow() }
                    }
                    AndroidView(factory = { PreviewView(it).apply { implementationMode = PreviewView.ImplementationMode.COMPATIBLE; this.controller = controller } }, update = { it.controller = controller }, modifier = Modifier.fillMaxWidth().weight(1f))
                } else Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                    Column {
                        Notice(str(R.string.text_camera_access_is_unavailable_paste_the_code_choose_an_image_or_al_224dd63))
                        TextButton(onClick = { beforePermission(); context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + context.packageName))) }) { Text(str(R.string.text_open_settings_214dbbf)) }
                    }
                }
                if (cameraUnavailable) Notice(str(R.string.text_the_camera_could_not_start_choose_an_image_or_paste_the_text_0883aee), true)
                OutlinedButton(onClick = readImage, modifier = Modifier.fillMaxWidth()) { Text(str(R.string.text_read_from_an_image_2e25ed4)) }
                TextButton(onClick = close) { Text(str(R.string.text_use_pasted_text_0909f6d)) }
            }
        }
    }
}
