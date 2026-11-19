package net.dmsg.android

import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.system.Os
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.platform.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class SystemExportsTest {
    /** Synthetic snapshot key only in the test APK. No production bypass. */
    private class Keys : ExportKeys {
        override fun create(header: ExportHeader) = ExportWrappedKey(ByteArray(32) { 17 }, byteArrayOf(1))
        override fun open(header: ExportHeader, valid: () -> Boolean) = error("Test must never bypass interactive authentication")
        override fun remove(header: ExportHeader) {}
    }
    @Test fun documentsProviderReadsOnlyAuthenticatedCopiesAndLockStopsNewAndUncachedReads() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        val initialized = app.hasMainClient
        val store = SystemExportStore(File(app.noBackupFilesDir, "system/exports/files"), ExportPurpose.files, Keys())
        store.invalidate(); ExportFileAccess.lock()
        val rootUri = DocumentsContract.buildChildDocumentsUri(app.packageName + ".documents", "root")
        app.contentResolver.query(rootUri, null, null, null, null)!!.use { assertEquals(0, it.count); assertNotNull(it.extras.getString(DocumentsContract.EXTRA_INFO)) }
        val source = File(app.cacheDir, "export-qa-" + UUID.randomUUID())
        val content = ByteArray(3 * SystemExportStore.PART + 7) { (it % 251).toByte() }
        source.writeBytes(content)
        var prepared: PreparedExport? = null
        try {
            prepared = store.prepareFiles(listOf(ExportFileInput(source, "Selected sample.bin", "application/octet-stream", content.size.toLong(), SystemExportStore.digest(content))), "synthetic-account", false)
            store.publish(prepared)
            ExportFileAccess.grant(app, store, prepared.opened)
            val uri = app.contentResolver.query(rootUri, null, null, null, null)!!.use { cursor ->
                assertEquals(1, cursor.count); assertTrue(cursor.moveToFirst())
                assertEquals("Selected sample.bin", cursor.getString(cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)))
                assertEquals(0, cursor.getInt(cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_FLAGS)))
                DocumentsContract.buildDocumentUri(app.packageName + ".documents", cursor.getString(cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)))
            }
            assertTrue(runCatching { app.contentResolver.openFileDescriptor(uri, "rw")!!.close() }.isFailure)
            app.contentResolver.openFileDescriptor(uri, "r")!!.use { fd ->
                val start = ByteArray(31)
                assertEquals(31, Os.pread(fd.fileDescriptor, start, 0, start.size, 0))
                assertArrayEquals(content.copyOfRange(0, 31), start)
                ExportFileAccess.lock()
                assertTrue(runCatching { app.contentResolver.openFileDescriptor(uri, "r")!!.close() }.isFailure)
                // Already delivered pages cannot be recalled; request an uncached tail.
                assertTrue(runCatching { Os.pread(fd.fileDescriptor, ByteArray(7), 0, 7, content.size.toLong() - 7) }.isFailure)
            }
            app.contentResolver.query(rootUri, null, null, null, null)!!.use { assertEquals(0, it.count) }
            ExportFileAccess.grant(app, store, prepared.opened)
            store.invalidate()
            assertTrue(runCatching { app.contentResolver.openFileDescriptor(uri, "r")!!.close() }.isFailure)
            assertEquals(initialized, app.hasMainClient)
        } finally { ExportFileAccess.lock(); store.invalidate(); prepared?.close(); source.delete(); content.fill(0) }
    }
    @Test fun publishedFileAccessExpiresAfterFiveRealMinutes() {
        org.junit.Assume.assumeTrue(InstrumentationRegistry.getArguments().getString("checkAccessExpiry") == "true")
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val store = SystemExportStore(File(app.noBackupFilesDir, "system/exports/files"), ExportPurpose.files, Keys())
        val root = DocumentsContract.buildChildDocumentsUri(app.packageName + ".documents", "root")
        val source = File.createTempFile("expiry-", ".txt", app.cacheDir)
        val bytes = "Disposable expiry sample".toByteArray()
        source.writeBytes(bytes)
        try {
            store.invalidate()
            store.prepareFiles(listOf(ExportFileInput(source, "Expiry sample.txt", "text/plain", bytes.size.toLong(), SystemExportStore.digest(bytes))), "test-account", false).use { prepared ->
                store.publish(prepared)
                ExportFileAccess.grant(app, store, prepared.opened)
                val uri = app.contentResolver.query(root, null, null, null, null)!!.use { rows ->
                    assertTrue(rows.moveToFirst())
                    DocumentsContract.buildDocumentUri(app.packageName + ".documents", rows.getString(rows.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)))
                }
                assertArrayEquals(bytes, app.contentResolver.openInputStream(uri)!!.use { it.readBytes() })
                android.os.SystemClock.sleep(300_100)
                app.contentResolver.query(root, null, null, null, null)!!.use { assertEquals(0, it.count) }
                assertTrue(runCatching { app.contentResolver.openInputStream(uri)!!.use { it.read() } }.isFailure)
            }
        } finally { ExportFileAccess.lock(); store.invalidate(); source.delete() }
    }

    @Test fun exportedProviderCannotBeQueriedByAnUnrelatedUidWithoutASystemGrant() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext
        val probe = Uri.parse("content://" + instrumentation.context.packageName + ".incoming-fixture/probe-documents")
            .buildUpon().appendQueryParameter("authority", app.packageName + ".documents").build()
        app.contentResolver.query(probe, null, null, null, null)!!.use { cursor ->
            assertTrue(cursor.moveToFirst()); assertEquals(1, cursor.getInt(0))
            assertNotEquals(android.os.Process.myUid(), cursor.getInt(1))
        }
    }
    @Test fun realAutofillRequestOffersOnlyIndependentAuthenticationAndNeverFillsWithoutItsKey() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext.applicationContext as DMsgApplication
        val initialized = app.hasMainClient
        val ui = instrumentation.uiAutomation
        fun shell(command: String) = android.os.ParcelFileDescriptor.AutoCloseInputStream(ui.executeShellCommand(command)).use { it.readBytes().toString(Charsets.UTF_8).trim() }
        fun find(node: android.view.accessibility.AccessibilityNodeInfo?, text: String): android.view.accessibility.AccessibilityNodeInfo? {
            if (node == null || !node.refresh()) return null
            if (node.text?.toString() == text) return node
            for (index in 0 until node.childCount) find(node.getChild(index), text)?.let { return it }
            return null
        }
        fun waitFor(text: String): android.view.accessibility.AccessibilityNodeInfo {
            val until = android.os.SystemClock.elapsedRealtime() + 12_000
            while (android.os.SystemClock.elapsedRealtime() < until) {
                ui.windows.forEach { window -> find(window.root, text)?.let { return it } }
                Thread.sleep(100)
            }
            // Only this disposable QA installation and its synthetic fixture are inspected.
            val state = StringBuilder("Missing system autofill view: $text; locale=${AppLanguage.selected()}\n")
            fun describe(node: android.view.accessibility.AccessibilityNodeInfo?, depth: Int = 0) {
                if (node == null || depth > 12) return
                state.append(" ".repeat(depth)).append(node.className).append(" text=").append(node.text?.take(240))
                    .append(" visible=").append(node.isVisibleToUser).append(" enabled=").append(node.isEnabled).append('\n')
                for (index in 0 until node.childCount.coerceAtMost(80)) describe(node.getChild(index), depth + 1)
            }
            ui.windows.forEach { window ->
                state.append("window active=").append(window.isActive).append(" focused=").append(window.isFocused)
                    .append(" package=").append(window.root?.packageName).append('\n')
                if (window.root?.packageName in setOf(app.packageName, instrumentation.context.packageName)) describe(window.root)
            }
            error(state.toString())
        }
        fun click(text: String, appWindow: Boolean = false) {
            // System suggestion windows expose text before their clickable parent attaches.
            // Re-read the current tree instead of retaining that transient node.
            if (appWindow) instrumentation.waitForIdleSync()
            val until = android.os.SystemClock.elapsedRealtime() + 12_000
            while (android.os.SystemClock.elapsedRealtime() < until) {
                for (window in ui.windows) {
                    // The authentication Activity replaces the system suggestion window.
                    // Do not send a click to a closing or not-yet-focused app window.
                    if (appWindow && (!window.isActive || !window.isFocused || window.root?.packageName != app.packageName)) continue
                    var target = find(window.root, text)
                    while (target != null && !target.isClickable) target = target.parent
                    if (target?.isEnabled != true || !target.isVisibleToUser) continue
                    if (appWindow) {
                        val bounds = android.graphics.Rect()
                        target.getBoundsInScreen(bounds)
                        if (!bounds.isEmpty) {
                            shell("input tap " + bounds.centerX() + " " + bounds.centerY())
                            return
                        }
                    } else if (target.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK)) return
                }
                Thread.sleep(100)
            }
            error("Missing clickable system autofill view: " + text)
        }
        val flags = ui.serviceInfo
        val originalFlags = flags.flags
        flags.flags = flags.flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
        ui.serviceInfo = flags
        val old = shell("settings get secure autofill_service")
        val store = SystemExportStore(File(app.noBackupFilesDir, "system/exports/credentials"), ExportPurpose.credentials, Keys())
        store.invalidate()
        val prepared = store.prepareCredentials(listOf(ExportCredentialInput("Selected sample", "sample-user", "Never-deliver-sample", "https://example.com")), "synthetic-account", false)
        store.publish(prepared)
        val authority = instrumentation.context.packageName + ".incoming-fixture"
        try {
            shell("settings put secure autofill_service " + app.packageName + "/net.dmsg.android.platform.DMsgAutofillService")
            shell("am start -W -n " + instrumentation.context.packageName + "/net.dmsg.android.AutofillFixtureActivity")
            click("Unlock dMsg passwords")
            waitFor("These copies are no longer available. Unlock dMsg and update them.")
            assertEquals(initialized, app.hasMainClient)
            app.contentResolver.query(Uri.parse("content://" + authority + "/autofill-state"), null, null, null, null)!!.use { cursor ->
                assertTrue(cursor.moveToFirst()); assertEquals("", cursor.getString(0)); assertNotEquals(android.os.Process.myUid(), cursor.getInt(1))
            }
            click("Cancel", appWindow = true)
        } finally {
            if (old.isBlank() || old == "null") shell("settings delete secure autofill_service") else shell("settings put secure autofill_service '" + old.replace("'", "'\\''") + "'")
            app.contentResolver.query(Uri.parse("content://" + authority + "/autofill-finish"), null, null, null, null)?.close()
            store.invalidate(); prepared.close(); AutofillRequests.clear()
            flags.flags = originalFlags; ui.serviceInfo = flags
        }
    }

    @Test fun independentAuthenticationActivityDoesNotStartTheMainClient() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        val before = app.hasMainClient
        ActivityScenario.launch<ExportAccessActivity>(Intent(app, ExportAccessActivity::class.java).putExtra("files", true)).use { scenario ->
            scenario.onActivity { activity ->
                assertEquals(before, app.hasMainClient)
                assertTrue(activity.window.attributes.flags and android.view.WindowManager.LayoutParams.FLAG_SECURE != 0)
                assertFalse(activity.isFinishing)
            }
        }
        assertEquals(before, app.hasMainClient)
    }
}
