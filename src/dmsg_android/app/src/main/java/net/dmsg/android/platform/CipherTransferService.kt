package net.dmsg.android.platform

import android.app.*
import android.app.job.*
import android.content.*
import android.os.Build
import android.os.PersistableBundle
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.*
import net.dmsg.android.BuildConfig
import net.dmsg.android.MainActivity
import net.dmsg.android.R
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.net.URL
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference
import javax.net.ssl.HttpsURLConnection

/** This executor only sees scoped URLs and ciphertext. It never opens MobileEngine. */
class CipherTransferService : JobService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private class Run(val id: String, val attempt: Int) {
        val connection = AtomicReference<HttpsURLConnection?>()
        var work: Job? = null
        fun stop() { work?.cancel(); connection.getAndSet(null)?.disconnect() }
    }
    private var active: Run? = null
    override fun onStartJob(params: JobParameters): Boolean {
        val id = params.extras.getString("id") ?: return false
        val attempt = params.extras.getInt("attempt", -1)
        if (Build.VERSION.SDK_INT >= 34 && params.isUserInitiatedJob) setNotification(params, NOTIFICATION_ID, notification(this, id, attempt), JOB_END_NOTIFICATION_POLICY_REMOVE)
        active?.stop()
        val run = Run(id, attempt)
        active = run
        run.work = scope.launch {
            var retry = false
            try {
                retry = withContext(Dispatchers.IO) {
                    val store = cipherStore(this@CipherTransferService) ?: return@withContext false
                    val snapshot = store.snapshot()?.takeIf { it.accepts(id, attempt) } ?: return@withContext false
                    try {
                        var lastNotification = 0L
                        for (part in snapshot.plan.parts) {
                            ensureActive()
                            val current = store.snapshot()?.takeIf { it.accepts(id, attempt) } ?: return@withContext false
                            if (part.slot in current.completed) continue
                            if (current.plan.expires <= System.currentTimeMillis().coerceAtLeast(0).toULong()) throw CipherHttpFailure("TRANSFER_EXPIRED", false)
                            val url = URL(part.url)
                            val network = params.network ?: throw CipherHttpFailure("TRANSFER_FAILED", true)
                            val socket = network.openConnection(url) as? HttpsURLConnection ?: throw CipherHttpFailure("INTEGRITY_FAILED", false)
                            run.connection.set(socket)
                            try {
                                ensureActive()
                                socket.instanceFollowRedirects = false
                                socket.connectTimeout = 15_000; socket.readTimeout = 20_000
                                socket.useCaches = false; socket.requestMethod = current.plan.method
                                socket.setRequestProperty("Accept-Encoding", "identity")
                                socket.setRequestProperty("Content-Type", "application/octet-stream")
                                if (current.plan.method == "PUT") {
                                    val bytes = store.verifiedPart(current.plan, part.slot)
                                    try {
                                        socket.doOutput = true; socket.setFixedLengthStreamingMode(bytes.size)
                                        socket.outputStream.use { output ->
                                            var offset = 0
                                            while (offset < bytes.size) {
                                                ensureActive()
                                                val count = minOf(64 * 1024, bytes.size - offset); output.write(bytes, offset, count); offset += count
                                            }
                                        }
                                    } finally { bytes.fill(0) }
                                }
                                val response = socket.responseCode
                                if (response !in 200..299) throw CipherHttpFailure(if (response in setOf(401, 403, 410)) "TRANSFER_EXPIRED" else "TRANSFER_FAILED", response in setOf(408, 429, 500, 502, 503, 504))
                                if (current.plan.method == "GET") {
                                    val declared = socket.contentLengthLong
                                    if (declared >= 0 && declared != part.size) throw CipherHttpFailure("INTEGRITY_FAILED", false)
                                    val output = ByteArrayOutputStream(part.size.toInt())
                                    val buffer = ByteArray(64 * 1024)
                                    try {
                                        socket.inputStream.use { input ->
                                            var total = 0L
                                            while (true) {
                                                ensureActive()
                                                val count = input.read(buffer); if (count < 0) break
                                                total += count
                                                if (total > part.size) throw CipherHttpFailure("INTEGRITY_FAILED", false)
                                                output.write(buffer, 0, count)
                                            }
                                        }
                                        val bytes = output.toByteArray()
                                        try { store.receive(id, attempt, part.slot, bytes) } finally { bytes.fill(0) }
                                    } finally { buffer.fill(0); output.close() }
                                } else store.complete(id, attempt, part.slot)
                            } finally { run.connection.compareAndSet(socket, null); socket.disconnect() }
                            if (Build.VERSION.SDK_INT >= 34 && params.isUserInitiatedJob && android.os.SystemClock.elapsedRealtime() - lastNotification >= 1000) withContext(Dispatchers.Main.immediate) {
                                lastNotification = android.os.SystemClock.elapsedRealtime()
                                val latest = store.snapshot()
                                if (active === run && latest?.plan?.id == id && latest.attempt == attempt) {
                                    setNotification(params, NOTIFICATION_ID, notification(this@CipherTransferService, id, attempt, latest), JOB_END_NOTIFICATION_POLICY_REMOVE)
                                }
                            }
                        }
                        false
                    } catch (error: CancellationException) { throw error }
                    catch (error: CipherHttpFailure) { store.fail(id, attempt, error.code, error.retry) }
                    catch (_: IOException) { store.fail(id, attempt, "TRANSFER_FAILED", true) }
                    catch (_: Throwable) { store.fail(id, attempt, "CIPHERTEXT_UNAVAILABLE", false) }
                }
            } catch (_: CancellationException) { }
            catch (_: Throwable) { /* No token, URL or private file path is logged. UI retains the journal. */ }
            finally {
                if (active === run) {
                    active = null
                    if (isActive) jobFinished(params, retry)
                }
            }
        }
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        val id = params.extras.getString("id") ?: return false
        val attempt = params.extras.getInt("attempt", -1)
        val run = active?.takeIf { it.id == id && it.attempt == attempt } ?: return false
        active = null
        run.stop()
        val store = runCatching { cipherStore(this) }.getOrNull() ?: return false
        if (params.stopReason == JobParameters.STOP_REASON_USER || params.stopReason == JobParameters.STOP_REASON_CANCELLED_BY_APP) {
            runCatching { store.pause(id, attempt, "TRANSFER_STOPPED") }
            return false
        }
        val current = runCatching { store.snapshot() }.getOrNull() ?: return false
        return current.accepts(id, attempt) && current.plan.expires > System.currentTimeMillis().coerceAtLeast(0).toULong()
    }
    override fun onDestroy() { active?.stop(); active = null; scope.cancel(); super.onDestroy() }

    private class CipherHttpFailure(val code: String, val retry: Boolean) : IOException()
    companion object {
        const val JOB_ID = 0x444d5301
        const val NOTIFICATION_ID = 0x444d5302
        const val CHANNEL = "dmsg.transfers.v1"
        const val OPEN = "net.dmsg.android.OPEN_TRANSFERS"
        private fun notification(context: Context, id: String, attempt: Int, state: CipherJob? = null): Notification {
            val manager = context.getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(NotificationChannel(CHANNEL, str(R.string.cipher_transfer_title), NotificationManager.IMPORTANCE_LOW).apply { setSound(null, null); lockscreenVisibility = Notification.VISIBILITY_PRIVATE })
            val open = PendingIntent.getActivity(context, 10, Intent(context, MainActivity::class.java).setAction(OPEN), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val stop = pauseIntent(context, id, attempt)
            return NotificationCompat.Builder(context, CHANNEL).setSmallIcon(R.drawable.dmsg_notification).setContentTitle("dMsg")
                .setContentText(str(R.string.cipher_transfer_running)).setContentIntent(open).setOngoing(true).setOnlyAlertOnce(true)
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setProgress(state?.plan?.parts?.size ?: 0, state?.completed?.size ?: 0, state == null)
                .addAction(0, str(R.string.text_pause_c65f066), stop).build()
        }
        internal fun pauseIntent(context: Context, id: String, attempt: Int): PendingIntent {
            // Android ignores extras when determining PendingIntent identity.
            val intent = Intent(context, CipherTransferStopReceiver::class.java)
                .setData(android.net.Uri.Builder().scheme("dmsg-internal").authority("cipher-pause").appendPath(id).appendPath(attempt.toString()).build())
                .putExtra("id", id).putExtra("attempt", attempt)
            return PendingIntent.getBroadcast(context, 11, intent, PendingIntent.FLAG_IMMUTABLE)
        }
    }
}

class CipherTransferStopReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                val store = cipherStore(context) ?: return@launch
                store.pause(intent.getStringExtra("id").orEmpty(), intent.getIntExtra("attempt", -1)) {
                    context.getSystemService(JobScheduler::class.java).cancel(CipherTransferService.JOB_ID)
                }
            } finally { pending.finish() }
        }
    }
}

fun cipherStore(context: Context): CipherTransferStore? {
    val root = File(context.noBackupFilesDir, "dmsg/" + if (BuildConfig.IS_STAGING) "staging" else "production")
    val marker = File(root, "installation")
    if (!marker.isFile || marker.length() !in 1..128) return null
    val installation = marker.readText().also { check(UUID.fromString(it).toString() == it) { "INSTALLATION_MISMATCH" } }
    return CipherTransferStore(File(root, "cipher-transfers"), installation)
}

class CipherScheduler(private val context: Context) {
    private val scheduler = context.getSystemService(JobScheduler::class.java)
    fun schedule(state: CipherJob) {
        check(state.phase == CipherPhase.TRANSFERRING) { "TRANSFER_PENDING" }
        val extras = PersistableBundle().apply { putString("id", state.plan.id); putInt("attempt", state.attempt) }
        val builder = JobInfo.Builder(CipherTransferService.JOB_ID, ComponentName(context, CipherTransferService::class.java))
            .setExtras(extras).setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).setPersisted(true)
            .setEstimatedNetworkBytes(if (state.plan.method == "GET") state.plan.size else 0, if (state.plan.method == "PUT") state.plan.size else 0)
        if (Build.VERSION.SDK_INT >= 33) builder.setMinimumNetworkChunkBytes(state.plan.parts.maxOf { it.size }.coerceAtLeast(1))
        if (Build.VERSION.SDK_INT >= 34) builder.setUserInitiated(true)
        else builder.setExpedited(true)
        var result = scheduler.schedule(builder.build())
        if (result != JobScheduler.RESULT_SUCCESS && Build.VERSION.SDK_INT < 34) result = scheduler.schedule(builder.setExpedited(false).build())
        check(result == JobScheduler.RESULT_SUCCESS) { "SYSTEM_TRANSFER_UNAVAILABLE" }
    }
    fun exists(state: CipherJob): Boolean = scheduler.getPendingJob(CipherTransferService.JOB_ID)?.extras?.let { it.getString("id") == state.plan.id && it.getInt("attempt", -1) == state.attempt } == true
    fun cancel() = scheduler.cancel(CipherTransferService.JOB_ID)
}
