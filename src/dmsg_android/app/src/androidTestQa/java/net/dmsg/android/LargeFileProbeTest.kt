package net.dmsg.android

import android.net.Uri
import android.os.Debug
import android.os.SystemClock
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import net.dmsg.android.core.Route
import net.dmsg.android.core.valueOrNull
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.atomic.AtomicReference

/** Opt-in bounded-memory probe using 1000 MiB of generated, non-secret zero bytes. */
class LargeFileProbeTest {
    @get:Rule val compose = createAndroidComposeRule<QaWorkspaceActivity>()

    @Test fun maximumLocalFileStagesEncryptsVerifiesAndExportsWithoutWholeFileBuffers() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("largeFileProbe") == "true")
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        compose.waitUntil(20_000) { compose.activity.qaClient.state.value.unlocked }
        val client = compose.activity.qaClient
        val size = 1000L * 1024 * 1024
        val digest = "da87281c9f9ab6cef8f9362935f4fc864db94606d52212614894f1253461a762"
        val uri = Uri.parse("content://${instrumentation.context.packageName}.incoming-fixture/large-probe")
        compose.runOnIdle { client.navigation.open("files"); client.navigation.push(Route("transfers")) }
        compose.waitForIdle()
        val started = SystemClock.elapsedRealtime()
        data class Memory(val elapsed: Long, val pss: Int, val nativePss: Int, val dalvikPss: Int, val nativeHeap: Long, val javaHeap: Long, val freeDisk: Long)
        fun memory(): Memory {
            val stats = Debug.MemoryInfo().also(Debug::getMemoryInfo)
            val runtime = Runtime.getRuntime()
            return Memory(SystemClock.elapsedRealtime() - started, stats.totalPss, stats.nativePss, stats.dalvikPss,
                Debug.getNativeHeapAllocatedSize(), runtime.totalMemory() - runtime.freeMemory(), context.noBackupFilesDir.usableSpace)
        }
        val samples = mutableListOf(memory())
        val samplingScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        val sampling = samplingScope.launch { while (isActive) { delay(500); samples += memory() } }
        val report = JSONObject().put("bytes", size).put("sha256", digest).put("api", android.os.Build.VERSION.SDK_INT)
            .put("package", context.packageName).put("variant", BuildConfig.BUILD_TYPE).put("page_size", shell("getconf PAGE_SIZE").trim())
            .put("sample_interval_ms", 500).put("source", "Generated zero bytes from a separate test APK ContentProvider")
            .put("scope", "Real Android staging, Rust local encrypted import, verified open, private share copy; no network or external recipient")
        var shared: File? = null
        try {
            save("before-meminfo.txt", shell("dumpsys meminfo ${context.packageName}"))
            val importStart = SystemClock.elapsedRealtime()
            compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); client.transfers.importResult(uri) }
            compose.waitUntil(180_000) { client.transfers.tasks.value.values.any { it.kind == "import" && it.phase in setOf("done", "failed") } }
            val imported = client.transfers.tasks.value.values.single { it.kind == "import" }
            assertEquals(imported.error, "done", imported.phase)
            report.put("stage_and_import_ms", SystemClock.elapsedRealtime() - importStart)
            compose.waitUntil(10_000) { client.library.listing.value()?.files?.any { it.name == "1000 MiB generated sample.bin" } == true }
            val file = client.library.listing.value()!!.files.single { it.name == "1000 MiB generated sample.bin" }
            assertEquals(size.toULong(), file.size)
            assertEquals(digest, file.sha256)
            save("after-import-meminfo.txt", shell("dumpsys meminfo ${context.packageName}"))
            val output = AtomicReference<File?>()
            val exportStart = SystemClock.elapsedRealtime()
            compose.runOnIdle { client.transfers.share(file) { local, _ -> output.set(local) } }
            compose.waitUntil(180_000) { output.get() != null || client.transfers.tasks.value["file:open:" + file.id]?.phase == "failed" }
            shared = output.get()
            assertNotNull(client.transfers.tasks.value["file:open:" + file.id]?.error, shared)
            report.put("verify_and_copy_ms", SystemClock.elapsedRealtime() - exportStart)
            assertEquals(size, shared!!.length())
            val hash = MessageDigest.getInstance("SHA-256")
            shared!!.inputStream().use { input ->
                val buffer = ByteArray(64 * 1024)
                while (true) { val count = input.read(buffer); if (count < 0) break; hash.update(buffer, 0, count) }
            }
            assertEquals(digest, hash.digest().joinToString("") { "%02x".format(it) })
            save("after-export-meminfo.txt", shell("dumpsys meminfo ${context.packageName}"))
            report.put("outcome", "passed")
        } finally {
            runBlocking { sampling.cancelAndJoin() }; samplingScope.cancel()
            shared?.parentFile?.deleteRecursively()
            samples += memory()
            report.put("baseline_pss_kib", samples.first().pss).put("peak_pss_kib", samples.maxOf { it.pss })
                .put("peak_pss_growth_kib", samples.maxOf { it.pss } - samples.first().pss)
                .put("peak_native_heap_bytes", samples.maxOf { it.nativeHeap }).put("peak_java_heap_bytes", samples.maxOf { it.javaHeap })
                .put("minimum_free_disk_bytes", samples.minOf { it.freeDisk }).put("elapsed_ms", SystemClock.elapsedRealtime() - started)
                .put("samples", JSONArray(samples.map { sample -> JSONObject().put("ms", sample.elapsed).put("pss_kib", sample.pss)
                    .put("native_pss_kib", sample.nativePss).put("dalvik_pss_kib", sample.dalvikPss).put("native_heap_bytes", sample.nativeHeap)
                    .put("java_heap_bytes", sample.javaHeap).put("free_disk_bytes", sample.freeDisk) }))
            save("report.json", report.toString(2))
        }
    }

    private fun shell(command: String): String = android.os.ParcelFileDescriptor.AutoCloseInputStream(
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
    ).use { it.readBytes().toString(Charsets.UTF_8) }

    private fun save(name: String, text: String) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(context.getExternalFilesDir(null), "large-file-$name").apply { writeText(text) }
        shell("mkdir -p /sdcard/Download/dmsg-large-file-qa")
        shell("cp ${file.absolutePath} /sdcard/Download/dmsg-large-file-qa/$name")
    }
}
