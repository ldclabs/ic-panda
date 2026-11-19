package net.dmsg.android

import net.dmsg.android.core.objectValue
import net.dmsg.android.core.rows
import net.dmsg.android.core.required
import android.graphics.Bitmap
import androidx.compose.ui.test.*
import androidx.compose.runtime.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import net.dmsg.android.core.QrRequest
import net.dmsg.android.core.QrState
import net.dmsg.android.core.QrStep
import net.dmsg.android.core.ClientController
import net.dmsg.android.core.ReadState
import net.dmsg.android.core.valueOrNull
import net.dmsg.android.ui.DMsgApp
import net.dmsg.core.MobileEngine
import net.dmsg.core.VaultEdit
import net.dmsg.core.FieldEdit
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/** Isolated QA variant only. Reads and writes use the real offline Rust database. */
@RunWith(AndroidJUnit4::class)
class WorkspaceFlowTest {
    @get:Rule val compose = createComposeRule()
    private lateinit var client: ClientController
    private lateinit var native: MobileEngine
    private lateinit var directory: File
    private lateinit var stagingDirectory: File
    private var uptime = 0L
    private var uiVersion by mutableStateOf(0)

    @Before fun open() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        // Each fixture has its own native database. Installation navigation hints and
        // preferences must be fresh too; process-restart flows exercise persistence
        // within their own test instead of leaking it into the next test account.
        val preferences = File(net.dmsg.android.platform.PrivateFiles(app).root, "preferences.json")
        if (preferences.isFile) {
            val previous = org.json.JSONObject(preferences.readText())
            val pending = listOf("pending_connect", "pending_workspace", "pending_notice", "pending_reminder")
                .filter { previous.has(it) && !previous.isNull(it) }
            if (pending.isNotEmpty()) android.util.Log.i("dMsgQA", "Discarding prior fixture navigation keys: " + pending.joinToString())
            check(preferences.delete())
        }
        compose.runOnUiThread {
            uptime = android.os.SystemClock.elapsedRealtime()
            client = ClientController(app, uptime = { uptime }) { paths, platform ->
                directory = File(paths.workspace, "ui-fixtures/" + UUID.randomUUID())
                stagingDirectory = paths.staging
                val core = MobileEngine.openUiFixture(directory.absolutePath, paths.staging.absolutePath, platform).also { native = it }
                val item = core.vaultPage("", null, false, 0u, 100u).items.first { it.title == "Studio account" }
                val detail = core.vaultItem(item.id, null).item
                core.saveItem(item.id, item.revision, VaultEdit(detail.title, detail.kind, detail.notes, detail.favorite, detail.tags,
                    detail.fields.map { FieldEdit(it.id, it.kind, it.label, it.section, if (it.id == "password") "Sample-only password" else it.value) }, detail.files))
                core
            }
            client.start()
        }
        compose.setContent { key(uiVersion) { DMsgApp(client, browser = { error("Unexpected external navigation") }, share = { _, _ -> error("Unexpected external share") }) } }
        compose.waitUntil(20_000) { client.state.value.unlocked }
        compose.runOnIdle { client.automaticSync(false); client.dark(false) }
        try { compose.waitUntil(10_000) { client.vault.listing.state.value is ReadState.Ready } }
        catch (error: Throwable) {
            throw AssertionError("Fixture vault not ready: page=${client.navigation.state.value.current.page}, " +
                "read=${client.vault.listing.state.value.javaClass.simpleName}, unlocked=${client.state.value.unlocked}, " +
                "concealed=${client.state.value.concealed}, operations=${client.state.value.operations}", error)
        }
    }

    @After fun close() {
        if (::client.isInitialized) runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
        if (::directory.isInitialized) directory.deleteRecursively()
    }

    @Test fun newLoginExposesNamedInputsAndKeepsSaveWithinReach() {
        compose.onNodeWithContentDescription("New item").assertIsDisplayed().performClick()
        compose.onNodeWithTag("vault-editor-title").performTextInput("Travel login")
        compose.onNodeWithText("Username").performScrollTo().performTextInput("traveler")
        compose.onNode(hasSetTextAction() and hasText("Password")).performScrollTo().performTextInput("fixture-only")
        compose.onNodeWithTag("vault-editor-save").assertIsDisplayed()
        capture("ux-password-editor")
        compose.onNodeWithTag("vault-editor-save").performClick()
        compose.waitUntil(10_000) { client.vault.listing.value()?.items?.any { it.title == "Travel login" } == true }
        compose.onNodeWithText("Travel login").performClick()
        compose.onNodeWithText("traveler").assertExists()
        compose.onNodeWithText("fixture-only").assertDoesNotExist()
        capture("ux-new-login-saved")
    }

    @Test fun directorySearchFindsVerifiedFileAndReturnsToTheSameQuery() {
        val meta = runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiReceivedFile().json) as kotlinx.serialization.json.JsonObject }
        val channel = (meta.getValue("channel") as kotlinx.serialization.json.JsonPrimitive).content
        compose.runOnIdle { client.navigation.open("messages") }
        compose.onNode(hasSetTextAction()).performTextInput("NoSuchFixtureResult")
        compose.waitUntil(10_000) { compose.onAllNodesWithText("No matching results").fetchSemanticsNodes().isNotEmpty() }
        capture("ux-search-no-matches")
        compose.onNode(hasSetTextAction()).performTextReplacement("Received image.png")
        compose.waitUntil(12_000) { client.messages.matches.value()?.singleOrNull()?.channel == channel }
        capture("ux-search-across-conversations")
        compose.onNode(hasClickAction() and hasText("A file already saved on this device")).performScrollTo().performClick()
        compose.waitUntil(10_000) { client.messages.reference.value()?.message?.sequence == 1uL }
        capture("ux-search-original")
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNode(hasSetTextAction()).assertTextContains("Received image.png")
    }

    @Test fun unbackedFilesReviewTheExactSelectedVersionsTogether() {
        val chosen = runBlocking {
            listOf("Alpha.txt", "Beta.txt", "Gamma.txt").map { name ->
                val data = net.dmsg.android.core.parseFileDetails(kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiFileHistory().json))
                native.renameFile(data.file.id, name)
                data.file.id to data.versions.single { !it.current }
            }
        }
        compose.runOnIdle { client.navigation.open("files") }
        compose.waitUntil(10_000) { (client.accountWorkspace.local.value()?.unbacked ?: 0uL) >= 6uL }
        compose.onNodeWithText("Unbacked file versions").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.cloudFiles.unbacked.value()?.size == 3 }
        chosen.take(2).forEach { (id, version) ->
            compose.onNodeWithTag("unbacked-version:" + id + ":" + version.id).performScrollTo().performClick()
        }
        compose.onNodeWithTag("backup-review-selection").assertIsEnabled().performClick()
        val selected = chosen.take(2).map { net.dmsg.android.core.BackupVersionKey(it.first, it.second.id) }
        compose.runOnIdle {
            val batch = client.cloudFiles.batch.value
            assertEquals(selected.toSet(), batch.review.map { it.key }.toSet())
            assertEquals(chosen.take(2).sumOf { it.second.size }, batch.review.sumOf { it.file.size })
        }
        compose.onNodeWithText("Alpha.txt").assertExists()
        compose.onNodeWithText("Beta.txt").assertExists()
        compose.onNodeWithText("Gamma.txt").assertDoesNotExist()
        compose.onNodeWithTag("backup-queue-selection").assertIsNotEnabled()
        capture("ux-batch-backup-review")
        // The test provides a display-only quota. Queue admission still uses the real offline core.
        compose.waitUntil(10_000) { client.cloudFiles.capacity.state.value is ReadState.Failed || client.cloudFiles.capacity.state.value is ReadState.Stale }
        compose.runOnIdle {
            client.cloudFiles.capacity.accept(client.cloudFiles.capacity.begin(), net.dmsg.android.core.BackupCapacity(1024uL, 1uL, 0uL, 0uL, null))
        }
        compose.onNodeWithTag("backup-queue-selection").assertIsNotEnabled()
        compose.runOnIdle {
            client.cloudFiles.capacity.accept(client.cloudFiles.capacity.begin(), net.dmsg.android.core.BackupCapacity(1024uL, 2048uL, 0uL, 0uL, null))
        }
        compose.onNodeWithTag("backup-queue-selection").assertIsEnabled().performClick()
        compose.waitUntil(20_000) { client.cloudFiles.batch.value.outcomes.size == 2 && !client.isWorking("cloud:backup-batch") }
        compose.runOnIdle { assertEquals(selected.toSet(), client.cloudFiles.batch.value.outcomes.keys) }
        capture("ux-batch-backup-results")
    }

    @Test fun receivingRulesOnlyOfferRelevantAccountsAndEdits() {
        compose.runOnIdle { client.navigation.open("inbox-settings") }
        compose.waitUntil(10_000) { client.safety.policy.state.value is ReadState.Failed || client.safety.policy.state.value is ReadState.Stale || client.safety.policy.value() != null }
        val saved = listOf("080g2081040g2081040g")
        compose.runOnIdle {
            client.safety.policy.accept(client.safety.policy.begin(), net.dmsg.android.core.ReceivingPolicy("closed", saved, 100u, 1u, false))
            client.safety.editReceiving(net.dmsg.android.core.ReceivingDraft("closed", saved))
        }
        compose.onNodeWithText("Add account").assertDoesNotExist()
        compose.onNodeWithText("Save receiving policy").performScrollTo().assertIsNotEnabled()
        capture("ux-closed-inbox")
        compose.onNodeWithText("Selected accounts").performScrollTo().performClick()
        compose.onNodeWithText(saved.single()).assertExists()
        compose.onNodeWithText("Save receiving policy").performScrollTo().assertIsEnabled()
    }

    @Test fun newVaultDraftSurvivesBackgroundAndFlushesBeforeImmediateLock() {
        compose.runOnIdle { client.vaultDrafts.open(null) }
        compose.waitUntil(5_000) { client.vaultDrafts.presented.value }
        compose.onNodeWithTag("vault-editor-title").performTextInput("Unfinished new item")
        compose.runOnIdle { client.background() }
        compose.runOnIdle { client.foreground() }
        compose.waitUntil(10_000) { !client.state.value.concealed }
        compose.onNodeWithTag("vault-editor-title").assertTextContains("Unfinished new item")
        compose.runOnIdle {
            client.vaultDrafts.update { it.copy(title = "Final keystroke", tags = "unfinished,", notes = "Not yet submitted") }
            client.lock()
        }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        assertNull(client.vaultDrafts.current.value)
        compose.onNodeWithTag("vault-editor").assertDoesNotExist()
        compose.runOnIdle { client.perform({ it.unlockUiFixture() }, transition = true) }
        compose.waitUntil(10_000) { client.state.value.unlocked && client.vaultDrafts.presented.value }
        compose.onNodeWithTag("vault-editor-title").assertTextContains("Final keystroke")
        assertEquals("unfinished,", client.vaultDrafts.current.value!!.tags)
        capture("qa-vault-draft-after-lock")
    }

    @Test fun lastInputSurvivesRepeatedLockWhileTheDatabaseIsBusy() {
        compose.runOnIdle { client.vaultDrafts.open(null) }
        compose.waitUntil(5_000) { client.vaultDrafts.presented.value }
        val account = requireNotNull(client.state.value.session?.account)
        val workspace = File(directory, "accounts/$account")
        val database = android.database.sqlite.SQLiteDatabase.openDatabase(
            File(workspace, "workspace.sqlite").path, null,
            android.database.sqlite.SQLiteDatabase.OPEN_READWRITE or android.database.sqlite.SQLiteDatabase.ENABLE_WRITE_AHEAD_LOGGING)
        val previous = native.scoped(native.controlGeneration())
        val last = "最后输入的内容🔒"
        val id = net.dmsg.android.core.requestId()
        try {
            database.beginTransactionNonExclusive()
            compose.runOnIdle {
                client.vaultDrafts.update { it.copy(title = last, tags = "未完成,") }
                client.letters.update(net.dmsg.android.core.LetterDraft(id = id, text = last))
                // The background grace can already have fenced the UI's old handle.
                native.`suspend`()
                client.lock()
                client.lock()
                assertFalse(client.state.value.unlocked)
                assertNull(client.vaultDrafts.current.value)
                assertTrue(client.letters.drafts.value.isEmpty())
            }
            assertTrue(runCatching { runBlocking { previous.vaultDrafts() } }.isFailure)
            // Keep the writer longer than the old two-second deadline. Lock and
            // the encrypted checkpoint must finish without acquiring this writer.
            compose.waitUntil(10_000) { client.state.value.session?.state == "locked" && !client.state.value.busy }
            android.os.SystemClock.sleep(2_100)
            val checkpoint = File(workspace, "mobile-lock-drafts.v1")
            assertTrue(checkpoint.isFile)
            assertFalse(checkpoint.readText().contains(last))
        } finally {
            if (database.inTransaction()) database.endTransaction()
            database.close()
            previous.destroy()
        }
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
        compose.runOnUiThread {
            client = ClientController(app, uptime = { uptime }) { paths, platform ->
                MobileEngine.reopenUiFixture(directory.absolutePath, paths.staging.absolutePath, platform).also { native = it }
            }
            uiVersion += 1
            client.start()
        }
        compose.waitUntil(20_000) { client.state.value.unlocked && client.vaultDrafts.presented.value }
        compose.onNodeWithTag("vault-editor-title").assertTextContains(last)
        assertEquals("未完成,", client.vaultDrafts.current.value!!.tags)
        assertEquals(last, runBlocking { net.dmsg.android.core.parseLetterDrafts(
            kotlinx.serialization.json.Json.parseToJsonElement(native.letterDrafts().json)) }.single { it.id == id }.text)
        assertFalse(File(workspace, "mobile-lock-drafts.v1").exists())
    }

    @Test fun unlockReturnsAnOversizedPasteToItsLetterEditor() {
        val id = net.dmsg.android.core.requestId()
        val pasted = "a".repeat(33 * 1024)
        compose.runOnIdle {
            client.letters.update(net.dmsg.android.core.LetterDraft(id = id, text = pasted))
            client.lock()
        }
        compose.waitUntil(10_000) { client.state.value.session?.state == "locked" && !client.state.value.busy }
        compose.runOnIdle { client.perform({ it.unlockUiFixture() }, transition = true) }
        compose.waitUntil(10_000) { client.state.value.unlocked && client.letters.drafts.value[id]?.text == pasted }
        compose.runOnIdle { client.letters.update(client.letters.drafts.value.getValue(id).copy(text = "可继续编辑")) }
        compose.waitUntil(10_000) { client.letters.pending().isEmpty() }
        val current = native.scoped(native.controlGeneration())
        try { assertTrue(runBlocking { current.letterDrafts() }.json.contains("可继续编辑")) }
        finally { current.destroy() }
        assertFalse(File(directory, "accounts/${client.state.value.session?.account}/mobile-lock-drafts.v1").exists())
    }

    @Test fun unreadableDraftCheckpointLeavesTheUnlockedWorkspaceUsable() {
        val account = requireNotNull(client.state.value.session?.account)
        compose.runOnIdle {
            client.letters.update(net.dmsg.android.core.LetterDraft(text = "Latest unsaved input"))
            client.lock()
        }
        compose.waitUntil(10_000) { client.state.value.session?.state == "locked" && !client.state.value.busy }
        val checkpoint = File(directory, "accounts/$account/mobile-lock-drafts.v1")
        assertTrue(checkpoint.isFile)
        checkpoint.writeText("unreadable checkpoint fixture")
        compose.runOnIdle { client.perform({ it.unlockUiFixture() }, transition = true) }
        compose.waitUntil(10_000) { client.state.value.unlocked && client.vault.listing.state.value is ReadState.Ready }
        compose.onNodeWithText("Studio account").assertIsDisplayed()
        assertEquals("unreadable checkpoint fixture", checkpoint.readText())
    }

    @Test fun lockContinuationWaitsForAReadyForeground() {
        var resumed = 0
        compose.runOnIdle {
            client.lock { resumed++ }
            client.background()
        }
        compose.waitUntil(10_000) { !client.state.value.busy && client.state.value.concealed }
        assertEquals(0, resumed)
        compose.runOnIdle { client.foreground() }
        compose.waitUntil(10_000) { resumed == 1 }
        assertEquals("locked", client.state.value.session?.state)
    }

    @Test fun lockingAfterACommittedDiscardDoesNotRestoreTheDraft() {
        val draft = net.dmsg.android.core.LetterDraft(text = "a".repeat(33 * 1024))
        val committed = kotlinx.coroutines.CompletableDeferred<Unit>()
        compose.runOnIdle {
            client.letters.update(draft)
            client.perform({ core ->
                core.discardLetterDraft(draft.id)
                committed.complete(Unit)
                kotlinx.coroutines.awaitCancellation()
            }, key = "letter:discard:" + draft.id)
        }
        runBlocking { kotlinx.coroutines.withTimeout(10_000) { committed.await() } }
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { client.state.value.session?.state == "locked" && !client.state.value.busy }
        compose.runOnIdle { client.perform({ it.unlockUiFixture() }, transition = true) }
        compose.waitUntil(10_000) { client.state.value.unlocked }
        assertFalse(draft.id in client.letters.drafts.value)
    }

    @Test fun existingVaultDraftSurvivesTimeoutAndNativeControllerRestart() {
        val summary = runBlocking { native.vaultPage("", null, false, 0u, 100u) }.items.first { it.title == "Studio account" }
        val item = runBlocking { native.vaultItem(summary.id, null) }.item
        compose.runOnIdle { client.vaultDrafts.open(item) }
        compose.waitUntil(5_000) { client.vaultDrafts.presented.value }
        compose.onNodeWithTag("vault-editor-title").performTextReplacement("Edited after restart")
        compose.runOnIdle { client.background(); uptime += 181_000; client.foreground() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
        compose.runOnUiThread {
            client = ClientController(app, uptime = { uptime }) { paths, platform ->
                MobileEngine.reopenUiFixture(directory.absolutePath, paths.staging.absolutePath, platform).also { native = it }
            }
            uiVersion += 1; client.start()
        }
        compose.waitUntil(20_000) { client.state.value.unlocked && client.vaultDrafts.presented.value }
        compose.onNodeWithTag("vault-editor-title").assertTextContains("Edited after restart")
        assertEquals(item.revision, client.vaultDrafts.current.value!!.revision)
        assertNull(client.vaultDrafts.current.value!!.fields.first { it.id == "password" }.edit().value)
        val input = client.vaultDrafts.input()
        compose.onNodeWithTag("vault-editor-save").assertIsDisplayed().performClick()
        compose.waitUntil(10_000) { !client.vaultDrafts.presented.value }
        val saved = runBlocking { native.vaultItem(item.id, null) }
        assertEquals("Edited after restart", saved.item.title)
        assertFalse(saved.item.conflict)
        assertEquals("Sample-only password", runBlocking { native.revealField(saved.item.id, saved.item.revision, "password") }.value)
        assertFalse(runBlocking { native.checkpointVaultDraft(input) })
        compose.runOnIdle { client.background(); client.foreground() }
        compose.waitUntil(10_000) { !client.state.value.concealed }
        compose.onNodeWithTag("vault-editor").assertDoesNotExist()
        capture("qa-vault-draft-committed-after-restart")
    }

    private fun capture(name: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.waitForIdle()
        instrumentation.waitForIdleSync()
        Thread.sleep(250) // System-bar appearance is applied by SystemUI after the Compose frame.
        val bitmap = checkNotNull(instrumentation.uiAutomation.takeScreenshot())
        val app = instrumentation.targetContext
        val file = File(app.getExternalFilesDir(null), "$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand("mkdir -p /sdcard/Download/dmsg-qa")).use { it.readBytes() }
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand("cp " + file.absolutePath + " /sdcard/Download/dmsg-qa/$name.png")).use { it.readBytes() }
    }

    private fun capturePermissionFailure(name: String) {
        capture(name)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext
        val nodes = StringBuilder("flags=" + instrumentation.uiAutomation.serviceInfo.flags +
            " unlocked=" + client.state.value.unlocked + " concealed=" + client.state.value.concealed +
            " qrStep=" + (client.qr.state.value as? QrRequest)?.step + "\n")
        fun inspect(node: android.view.accessibility.AccessibilityNodeInfo?, depth: Int = 0) {
            if (node == null || depth > 20) return
            val bounds = android.graphics.Rect().also(node::getBoundsInScreen)
            nodes.append(" ".repeat(depth)).append(node.packageName).append(" | ").append(node.className)
                .append(" | ").append(node.viewIdResourceName).append(" | ").append(node.text?.take(200))
                .append(" | enabled=").append(node.isEnabled).append(" | ").append(bounds).append("\n")
            for (i in 0 until node.childCount) inspect(node.getChild(i), depth + 1)
        }
        inspect(instrumentation.uiAutomation.rootInActiveWindow)
        val file = File(app.getExternalFilesDir(null), name + "-tree.txt").apply { writeText(nodes.toString()) }
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
            "cp " + file.absolutePath + " /sdcard/Download/dmsg-qa/" + file.name
        )).use { it.readBytes() }
    }

    @Test fun coalescedChannelRefreshAllowsLaterRefreshesAfterFailure() {
        val channel = "ab".repeat(32)
        compose.runOnIdle {
            client.messages.load(channel, refresh = false)
            client.messages.refresh(channel)
            client.messages.refresh(channel)
        }
        compose.waitUntil(15_000) { !client.isWorking("message:refresh") }
        compose.runOnIdle {
            client.messages.refresh(channel)
            assertTrue("A coalesced refresh must not leave the channel permanently busy", client.isWorking("message:refresh"))
        }
        compose.waitUntil(15_000) { !client.isWorking("message:refresh") }
    }

    @Test fun nativePresentationUsesAGenericInAppBannerWithoutMarkingBusinessContentRead() {
        compose.waitUntil(15_000) { client.state.value.operations.none { it.startsWith("notices:") } }
        val account = client.state.value.session!!.account!!
        val original = runBlocking { native.notificationPreferences(account) }
        val preferences = net.dmsg.android.core.parseNoticePreferences(kotlinx.serialization.json.Json.parseToJsonElement(original.json))
        runBlocking {
            native.notificationSetGlobal(preferences.global.revision, preferences.global.copy(enabled = true).json())
            native.notificationSetAccount(account, preferences.account!!.revision, preferences.account.copy(enabled = true).json())
            native.seedUiReminders()
        }
        compose.runOnIdle { client.notices.refresh() }
        compose.waitUntil(10_000) { client.notices.presented.value != null }
        val plan = client.notices.presented.value!!
        compose.onNodeWithText("Open reminder").assertIsDisplayed()
        compose.onNodeWithText("Sample-only password").assertDoesNotExist()
        val snapshot = runBlocking { native.notificationReference(account, plan.target.json()) }
        val unchanged = net.dmsg.android.core.parseReminder(kotlinx.serialization.json.Json.parseToJsonElement(snapshot.json))
        assertEquals(0uL, unchanged.readVersion); assertEquals(0uL, unchanged.observed)
        capture("qa-notice-foreground-banner")
        compose.runOnIdle { client.notices.dismiss() }
        compose.onNodeWithText("Open reminder").assertDoesNotExist()
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("notifications")) }
        compose.waitUntil(10_000) { client.notifications.preferences.value() != null }
        capture("qa-notice-android-settings")
        compose.runOnIdle { client.background() }
        assertNull(client.notices.presented.value)
    }

    @Test fun reminderAccountChoiceSurvivesLockAndANewerIntentDismissesItWithoutReading() {
        val seeded = runBlocking { native.seedUiReminders() }
        val rows = kotlinx.serialization.json.Json.parseToJsonElement(seeded.json) as kotlinx.serialization.json.JsonArray
        val item = rows.map { net.dmsg.android.core.parseReminder(it) }.first { it.target.kind == "channel" }
        val account = client.state.value.session!!.account
        val foreign = item.copy(account = "081040g2081040g20810")
        compose.runOnIdle { client.notifications.open(foreign) }
        compose.onNodeWithText("This reminder belongs to another account").assertIsDisplayed()
        assertEquals(account, client.state.value.session!!.account)
        val saved = client.noticeNavigation.save()!!
        capture("qa-notice-account-choice")
        compose.runOnIdle {
            client.noticeNavigation.clear(discard = true)
            client.noticeNavigation.restore(saved)
            client.routePendingNotice()
        }
        compose.onNodeWithText("This reminder belongs to another account").assertIsDisplayed()
        val current = runBlocking { native.notificationReference(item.account, item.target.json()) }
        val untouched = net.dmsg.android.core.parseReminder(kotlinx.serialization.json.Json.parseToJsonElement(current.json))
        assertEquals(0uL, untouched.observed); assertEquals(0uL, untouched.readVersion)
        compose.runOnIdle { client.lock(false) }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        compose.onNodeWithText("This reminder belongs to another account").assertIsDisplayed()
        compose.runOnIdle { client.requestWorkspace(net.dmsg.android.core.WorkspaceLink("files")) }
        compose.onNodeWithText("This reminder belongs to another account").assertDoesNotExist()
        assertNull(client.noticeNavigation.save())
    }

    @Test fun missingOpaqueReminderAndChangedRecoveryNeverFallThroughToAnotherRequest() {
        val approvals = runBlocking { native.approvals().json }
        compose.runOnIdle { client.requestNotice(net.dmsg.android.core.NoticeRoute.parse("local:" + "fa".repeat(32))!!) }
        compose.waitUntil(10_000) { client.noticeNavigation.destination.state.value is ReadState.Failed }
        compose.onNodeWithText("Reminder unavailable").assertIsDisplayed()
        compose.onNodeWithText("Close").performClick()
        assertEquals(approvals, runBlocking { native.approvals().json })
        val expected = "bb".repeat(32)
        val actual = "cc".repeat(32)
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("recovery", id = expected)) }
        compose.waitUntil(10_000) { client.account.recovery.state.value is ReadState.Failed }
        val now = System.currentTimeMillis().toULong()
        compose.runOnIdle {
            client.account.recovery.accept(client.account.recovery.begin(), net.dmsg.android.core.RecoveryStatus(client.state.value.session!!.account!!, true, true, false, 86_400_000u,
                net.dmsg.android.core.RecoveryPending("dd".repeat(32), now + 10_000u, now + 20_000u, false, actual)))
        }
        compose.onNodeWithText("This recovery request is no longer pending").assertIsDisplayed()
        compose.onNodeWithText("Review cancelling recovery").assertDoesNotExist()
        capture("qa-notice-old-recovery")
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("recovery", id = actual)) }
        compose.onNodeWithText("Pending recovery").assertIsDisplayed()
        assertEquals(approvals, runBlocking { native.approvals().json })
    }

    @Test fun passwordPickerUsesTheUnlockedVaultWithoutManualPublication() {
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("system-autofill")) }
        compose.onNodeWithText("Choose a login when an app or website asks for a password.").assertExists()
        compose.onNodeWithText("Select login items").assertDoesNotExist()
        val catalog = runBlocking { native.autofillCatalog("", false) }
        val logins = kotlinx.serialization.json.Json.parseToJsonElement(catalog.json).objectValue().rows("entries")
        assertTrue(logins.any { it.required("title") == "Studio account" && it.required("id").isNotEmpty() })
        compose.onNodeWithText("Sample-only password").assertDoesNotExist()
        compose.runOnIdle { client.lock(false) }
        compose.waitUntil(10_000) { !client.state.value.unlocked }
        assertNull(client.systemExports.status.value())
    }

    @Test fun workspaceEntryWaitsForUnlockAndCannotApproveOrOverrideALaterIntent() {
        val before = runBlocking { native.approvals().json }
        compose.runOnIdle { client.acceptCallback("dmsg://open/approvals") }
        assertEquals("approvals", client.navigation.state.value.current.page)
        assertEquals(before, runBlocking { native.approvals().json })
        compose.runOnIdle { client.acceptCallback("dmsg://open/approvals?approve=true") }
        assertEquals(before, runBlocking { native.approvals().json })
        compose.runOnIdle { client.background() }
        compose.runOnIdle { client.acceptCallback("dmsg://open/files") }
        assertEquals("approvals", client.navigation.state.value.current.page)
        compose.runOnIdle { client.foreground() }
        compose.waitUntil(10_000) { !client.state.value.concealed && client.navigation.state.value.current.page == "files" }
        val refreshed = native.scoped(client.state.value.session!!.generation)
        try { assertEquals(before, runBlocking { refreshed.approvals().json }) } finally { refreshed.destroy() }
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        compose.runOnIdle { client.acceptCallback("dmsg://open/imports") }
        assertEquals("files", client.navigation.state.value.current.page)
        compose.onNodeWithText("Import into this account").assertDoesNotExist()
    }

    @Test fun crossApplicationDropReleasesItsReadGrantAndWaitsForExplicitImport() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext
        val authority = instrumentation.context.packageName + ".drag-fixture"
        val sample = UUID.randomUUID().toString()
        val uri = android.net.Uri.parse("content://$authority/sample").buildUpon().appendQueryParameter("sample", sample).build()
        val provider = app.packageManager.resolveContentProvider(authority, 0)!!
        assertNotEquals(android.os.Process.myUid(), provider.applicationInfo.uid)
        fun accessDenied(): Boolean = try {
            app.contentResolver.query(uri, null, null, null, null)?.close(); false
        } catch (_: SecurityException) { true }
        assertTrue("The source is private before the OS drag grant", accessDenied())
        val root = File(app.noBackupFilesDir, "system/incoming")
        val storage = net.dmsg.android.platform.IncomingShareStore(root, net.dmsg.android.platform.IncomingKeys(app, root))
        assertTrue(storage.list().isEmpty())
        val baseline = runBlocking { native.files("root").json }
        compose.runOnIdle { client.navigation.open("files") }
        compose.waitForIdle()
        val bounds = compose.onRoot().fetchSemanticsNode().boundsInWindow
        app.startActivity(android.content.Intent().setClassName(instrumentation.context.packageName, "net.dmsg.android.DragFixtureActivity")
            .putExtra("sample", sample).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            var source: android.graphics.Rect? = null
            var stableSince = android.os.SystemClock.elapsedRealtime()
            compose.waitUntil(10_000) {
                val observed = instrumentation.uiAutomation.rootInActiveWindow?.findAccessibilityNodeInfosByText("Drag sample file")
                    ?.firstOrNull()?.let { android.graphics.Rect().also(it::getBoundsInScreen) }
                if (observed != source) { source = observed; stableSince = android.os.SystemClock.elapsedRealtime() }
                source != null && android.os.SystemClock.elapsedRealtime() - stableSince >= 250
            }
            val start = checkNotNull(source)
            val x = start.exactCenterX(); val y = start.exactCenterY()
            val endX = bounds.left + bounds.width * 0.25f; val endY = bounds.top + bounds.height * 0.65f
            val down = android.os.SystemClock.uptimeMillis()
            fun motion(action: Int, px: Float, py: Float) {
                val event = android.view.MotionEvent.obtain(down, android.os.SystemClock.uptimeMillis(), action, px, py, 0)
                event.source = android.view.InputDevice.SOURCE_TOUCHSCREEN
                try { assertTrue(instrumentation.uiAutomation.injectInputEvent(event, true)) } finally { event.recycle() }
            }
            motion(android.view.MotionEvent.ACTION_DOWN, x, y)
            Thread.sleep(android.view.ViewConfiguration.getLongPressTimeout().toLong() + 250)
            for (step in 1..20) {
                motion(android.view.MotionEvent.ACTION_MOVE, x + (endX - x) * step / 20f, y + (endY - y) * step / 20f)
                Thread.sleep(16)
            }
            instrumentation.waitForIdleSync()
            motion(android.view.MotionEvent.ACTION_UP, endX, endY)
            try { compose.waitUntil(15_000) { !client.incoming.intake.value.working && client.incoming.entries.value()?.size == 1 } }
            catch (error: Throwable) {
                capturePermissionFailure("qa-drag-failure")
                throw AssertionError("Drop intake=${client.incoming.intake.value}; route=${client.navigation.state.value.current}; source=$start; target=$endX,$endY", error)
            }
            compose.waitUntil(5_000) { accessDenied() }
            assertEquals(baseline, runBlocking { native.files("root").json })
            assertEquals("incoming-shares", client.navigation.state.value.current.page)
            assertEquals("External sample.txt", storage.list().single().name)
            for (file in root.walkTopDown().filter(File::isFile)) {
                assertFalse(file.readBytes().toString(Charsets.ISO_8859_1).contains("External sample share"))
            }
            capture("qa-cross-app-file-drop-review")
        } finally { storage.list().forEach { storage.remove(it.id) } }
    }

    @Test fun externalProviderShareStaysEncryptedUntilExplicitAccountImportAndIsDeduplicated() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext
        val authority = instrumentation.context.packageName + ".incoming-fixture"
        val provider = app.packageManager.resolveContentProvider(authority, 0)
        assertNotNull(provider)
        assertNotEquals(android.os.Process.myUid(), provider!!.applicationInfo.uid)
        val uri = android.net.Uri.parse("content://$authority/sample").buildUpon().appendQueryParameter("sample", UUID.randomUUID().toString()).build()
        val intent = android.content.Intent(android.content.Intent.ACTION_SEND).apply { type = "text/plain"; putExtra(android.content.Intent.EXTRA_STREAM, uri) }
        val root = File(app.noBackupFilesDir, "system/incoming")
        val storage = net.dmsg.android.platform.IncomingShareStore(root, net.dmsg.android.platform.IncomingKeys(app, root))
        assertTrue(storage.list().isEmpty())
        val baseline = runBlocking { native.files("root").json }
        try {
            val finished = java.util.concurrent.atomic.AtomicInteger()
            compose.runOnIdle { assertTrue(client.incoming.receive(intent) { finished.incrementAndGet() }) }
            compose.waitUntil(10_000) { !client.incoming.intake.value.working && client.incoming.entries.value()?.size == 1 }
            compose.waitUntil(5_000) { finished.get() == 1 }
            val entry = client.incoming.entries.value()!!.single()
            assertEquals("External sample.txt", entry.name)
            for (file in root.walkTopDown().filter(File::isFile)) assertFalse(file.readBytes().toString(Charsets.ISO_8859_1).contains("External sample share"))
            assertEquals(baseline, runBlocking { native.files("root").json })
            compose.onNodeWithText("Preview shared text").performScrollTo().performClick()
            compose.waitUntil(10_000) { client.incoming.previews.value[entry.id] != null }
            compose.onNodeWithText("External sample share, never a real secret.").performScrollTo().assertIsDisplayed()
            capture("qa-incoming-encrypted-review")
            compose.onNodeWithText("Import into this account").performScrollTo().performClick()
            compose.onNodeWithText("Cancel", useUnmergedTree = true).performClick()
            assertEquals(baseline, runBlocking { native.files("root").json })
            compose.onNodeWithText("Import into this account").performScrollTo().performClick()
            compose.onNodeWithText("Import", useUnmergedTree = true).performClick()
            compose.waitUntil(10_000) { client.incoming.entries.state.value is ReadState.Empty && !client.state.value.working("incoming:import:" + entry.id) }
            val files = net.dmsg.android.core.parseFiles(runBlocking { native.files("root").json }).second
            assertEquals(1, files.size)
            assertEquals(entry.digest, files.single().sha256)
            compose.runOnIdle { assertTrue(client.incoming.receive(intent)) }
            compose.waitUntil(10_000) { !client.incoming.intake.value.working && client.incoming.intake.value.duplicate }
            assertTrue(storage.list().isEmpty())
            assertEquals(files.single().id, net.dmsg.android.core.parseFiles(runBlocking { native.files("root").json }).second.single().id)
            capture("qa-incoming-imported-once")
        } finally { storage.list().forEach { storage.remove(it.id) } }
    }

    @Test fun incomingShareRejectsOwnProviderAndNeverInterpretsSenderAccountOrApprovalFields() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(app.noBackupFilesDir, "system/incoming")
        val storage = net.dmsg.android.platform.IncomingShareStore(root, net.dmsg.android.platform.IncomingKeys(app, root))
        assertTrue(storage.list().isEmpty())
        val approvals = runBlocking { native.approvals().json }
        val own = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(android.content.Intent.EXTRA_STREAM, android.net.Uri.parse("content://" + app.packageName + ".files/private-data"))
        }
        compose.runOnIdle { assertTrue(client.incoming.receive(own)) }
        assertEquals("INVALID_INPUT", client.incoming.intake.value.error)
        assertTrue(storage.list().isEmpty())
        assertEquals(approvals, runBlocking { native.approvals().json })
        val text = "Locked intake sample " + UUID.randomUUID().toString()
        val share = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
            type = "text/plain"; putExtra(android.content.Intent.EXTRA_TEXT, text)
            data = android.net.Uri.parse("dmsg://connect/request?approve=true")
            putExtra("account", "foreign-account"); putExtra("approve", true)
        }
        try {
            compose.runOnIdle { client.lock() }
            compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
            compose.runOnIdle { assertTrue(client.incoming.receive(share)) }
            compose.waitUntil(10_000) { !client.incoming.intake.value.working && client.incoming.intake.value.count == 1 }
            assertNull(client.incoming.entries.value())
            assertNull(storage.list().single().account)
            compose.onNodeWithText(text).assertDoesNotExist()
            for (file in root.walkTopDown().filter(File::isFile)) assertFalse(file.readBytes().toString(Charsets.ISO_8859_1).contains(text))
            capture("qa-incoming-locked")
        } finally { storage.list().forEach { storage.remove(it.id) } }
    }

    @Test fun anOldNotificationPauseCannotCancelTheRetriedTransfer() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val store = checkNotNull(net.dmsg.android.platform.cipherStore(app))
        val account = client.state.value.session!!.account!!
        val id = "3c".repeat(32)
        val now = System.currentTimeMillis().toULong()
        val part = net.dmsg.android.platform.CipherPart("0", 1, "ab".repeat(32),
            "https://relay.example.test/mobile-transfer/v1/$account/$id/0?token=" + "bc".repeat(32))
        val plan = net.dmsg.android.platform.CipherPlan(id, account, client.state.value.session!!.deviceId!!,
            "GET", "ef".repeat(32), now + 60_000u, listOf(part))
        fun deliver(action: android.app.PendingIntent) {
            val finished = java.util.concurrent.CountDownLatch(1)
            action.send(app, 0, null, { _, _, _, _, _ -> finished.countDown() }, null)
            assertTrue(finished.await(5, java.util.concurrent.TimeUnit.SECONDS))
        }
        val initial = store.start(plan, now)
        try {
            val first = store.retry(initial, now)
            val old = net.dmsg.android.platform.CipherTransferService.pauseIntent(app, id, first.attempt)
            assertTrue(store.pause(id, first.attempt))
            val next = store.retry(store.snapshot()!!, now)
            val current = net.dmsg.android.platform.CipherTransferService.pauseIntent(app, id, next.attempt)
            assertNotEquals(old, current)
            deliver(old)
            assertEquals(next, store.snapshot())
            deliver(current)
            compose.waitUntil(5_000) { store.snapshot()?.phase == net.dmsg.android.platform.CipherPhase.PAUSED }
        } finally { store.discard(plan) }
    }

    @Test fun expiredCipherGrantStopsInTheRealSystemJobWithoutOpeningOrFinishingAFile() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val store = checkNotNull(net.dmsg.android.platform.cipherStore(app))
        assertNull(store.snapshot())
        val bytes = "sample encrypted bytes only".toByteArray()
        val id = "9a".repeat(32)
        val account = client.state.value.session!!.account!!
        val now = System.currentTimeMillis().toULong()
        val part = net.dmsg.android.platform.CipherPart("0", bytes.size.toLong(), net.dmsg.android.platform.CipherTransferStore.digest(bytes), "https://relay.example.test/mobile-transfer/v1/" + account + "/" + id + "/0?token=" + "bc".repeat(32))
        val plan = net.dmsg.android.platform.CipherPlan(id, account, client.state.value.session!!.deviceId!!, "GET", "de".repeat(32), now - 1000u, listOf(part))
        val scheduler = net.dmsg.android.platform.CipherScheduler(app)
        val filesBefore = runBlocking { native.files("root").json }
        try {
            val initial = store.start(plan, now - 2000u)
            val active = store.retry(initial, now - 1500u)
            compose.runOnIdle { scheduler.schedule(active) }
            compose.waitUntil(15_000) { store.snapshot()?.error == "TRANSFER_EXPIRED" }
            assertEquals(net.dmsg.android.platform.CipherPhase.FAILED, store.snapshot()!!.phase)
            assertTrue(store.snapshot()!!.completed.isEmpty())
            assertEquals(filesBefore, runBlocking { native.files("root").json })
            compose.runOnIdle { client.navigation.open("file-transfers"); client.backgroundFiles.load() }
            compose.waitUntil(10_000) { client.backgroundFiles.current.value()?.plan?.id == id }
            compose.onNodeWithText("Verify and finish").assertDoesNotExist()
            compose.onNodeWithText("No pending transfers").assertDoesNotExist()
            capture("qa-background-expired-grant")
        } finally { scheduler.cancel(); store.discard(plan) }
    }

    @Test fun deviceReceiverStopsInBackgroundAndResumesTheOriginalSessionWithoutExportApproval() {
        val before = runBlocking { native.approvals().json }
        compose.runOnIdle { client.navigation.open("device-transfer") }
        compose.waitUntil(10_000) { client.deviceTransfers.status.state.value is ReadState.Empty && client.accountWorkspace.local.state.value is ReadState.Ready }
        compose.onNodeWithText("Receive here").performScrollTo().performClick()
        compose.waitForIdle()
        try { compose.waitUntil(10_000) { client.deviceTransfers.status.value()?.state == "waiting" } }
        catch (error: Throwable) {
            capture("qa-device-receiver-failure")
            throw AssertionError("receiver status=" + client.deviceTransfers.status.state.value + " notice=" + client.deviceTransfers.notice.value + " native=" + native.transferStatus().json, error)
        }
        val started = client.deviceTransfers.status.value()!!
        assertTrue(started.port!! > 0u)
        capture("qa-device-receiver")
        compose.runOnIdle { client.background() }
        compose.waitUntil(10_000) {
            net.dmsg.android.core.parseDeviceTransfer(kotlinx.serialization.json.Json.parseToJsonElement(native.transferStatus().json))?.running == false
        }
        assertNull(client.deviceTransfers.status.value())
        assertTrue(client.deviceTransfers.suspended.value)
        assertTrue(runCatching { java.net.Socket().use { it.connect(java.net.InetSocketAddress("127.0.0.1", started.port.toInt()), 500) } }.isFailure)
        compose.runOnIdle { client.foreground() }
        compose.waitUntil(10_000) { client.deviceTransfers.status.value()?.state == "waiting" && !client.state.value.concealed }
        assertEquals(started.session, client.deviceTransfers.status.value()!!.session)
        val refreshed = native.scoped(client.state.value.session!!.generation)
        try { assertEquals(before, runBlocking { refreshed.approvals().json }) } finally { refreshed.destroy() }
        compose.onNodeWithText("Stop transfer").performClick()
        compose.waitUntil(10_000) { client.deviceTransfers.status.value()?.state == "cancelled" }
        assertFalse(client.deviceTransfers.suspended.value)
        compose.onNodeWithText("Resume transfer").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.deviceTransfers.status.value()?.state == "waiting" }
        assertEquals(started.session, client.deviceTransfers.status.value()!!.session)
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        assertNull(client.deviceTransfers.status.value())
        assertFalse(net.dmsg.android.core.parseDeviceTransfer(kotlinx.serialization.json.Json.parseToJsonElement(native.transferStatus().json))!!.running)
    }

    @Test fun malformedHandoverRequestNeverCreatesApprovalAndKeepsTheAuthoredToken() {
        val before = runBlocking { native.approvals().json }
        compose.runOnIdle { client.navigation.open("handover-review"); client.handovers.editToken("not-a-valid-request") }
        compose.onNodeWithText("Check request").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.handovers.review.state.value is ReadState.Failed }
        assertEquals("not-a-valid-request", client.handovers.token.value)
        compose.onNodeWithText("Review replacement approval").assertIsNotEnabled()
        assertEquals(before, runBlocking { native.approvals().json })
        capture("qa-handover-invalid-request")
    }

    @Test fun localAgentPermissionsNeedReviewAndManagedPrivateKeyCannotBeRevealed() {
        compose.runOnIdle { client.navigation.open("agents") }
        compose.waitUntil(10_000) { client.agents.listing.state.value is ReadState.Empty }
        compose.onNodeWithTag("agent-label").performTextInput("research")
        compose.onNodeWithText("Create agent").performClick()
        compose.waitUntil(10_000) { client.agents.listing.value()?.size == 1 }
        val agent = client.agents.listing.value()!!.single()
        assertTrue(agent.id.startsWith("did:agent:"))
        assertNotEquals(agent.id, agent.label)
        assertFalse(agent.resident)
        compose.onNodeWithText("research").performClick()
        compose.waitUntil(10_000) { client.agents.grants.state.value is ReadState.Empty }
        compose.onNodeWithTag("Pause this agent").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.agents.listing.value()?.single()?.paused == true }
        compose.onNodeWithTag("agent-permission-picker").performScrollTo().performClick()
        compose.onNodeWithTag("permission-option:vault.list").performScrollTo().performClick()
        compose.onNodeWithTag("agent-permission-add").performScrollTo().performClick()
        capture("ux-agent-permissions")
        compose.onNodeWithText("Review permissions").performClick()
        compose.waitUntil(10_000) { client.approvals.detail.state.value is ReadState.Ready }
        val approval = client.approvals.detail.value()!!
        assertEquals("grant", approval.kind)
        assertEquals("awaiting_user", approval.state)
        assertEquals("[]", runBlocking { native.agentGrants().json })
        capture("qa-agent-permissions-review")
        val key = runBlocking { native.vaultPage("", null, false, 0u, 100u).items.first { it.reserved && it.tags.contains("dmsg-agent") } }
        val detail = runBlocking { native.vaultItem(key.id, null).item }
        assertTrue(runBlocking { runCatching { native.revealField(key.id, detail.revision, "key") }.isFailure })
        compose.runOnIdle { client.navigation.open("vault"); client.navigation.push(net.dmsg.android.core.Route("vault", key.id)) }
        compose.waitUntil(10_000) { client.vault.detail.value()?.id == key.id }
        compose.onNodeWithText("Share secret snapshot").assertDoesNotExist()
        compose.onNodeWithText("This key is managed by dMsg. Its private fields cannot be revealed, edited or copied.").performScrollTo().assertIsDisplayed()
        capture("qa-agent-managed-key")
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("agent", agent.id)) }
        compose.waitUntil(10_000) { client.agents.listing.value()?.size == 1 }
        compose.onNodeWithText("Remove this local agent").performScrollTo().performClick()
        compose.onNodeWithText("Cancel").performClick()
        assertEquals(1, net.dmsg.android.core.parseManagedAgents(kotlinx.serialization.json.Json.parseToJsonElement(runBlocking { native.agents().json })).size)
        compose.onNodeWithText("Remove this local agent").performScrollTo().performClick()
        compose.onNodeWithText("Remove", useUnmergedTree = true).performClick()
        compose.waitUntil(10_000) { client.agents.listing.state.value is ReadState.Empty }
    }

    @Test fun unreadableAgentPrincipalDoesNotOfferEnableOrCreateCredentials() {
        val approvalsBefore = runBlocking { native.approvals().json }
        compose.runOnIdle { client.navigation.open("agent-delegations") }
        compose.waitUntil(10_000) { client.agents.principal.state.value is ReadState.Failed }
        compose.onNodeWithText("Review enabling a principal").assertDoesNotExist()
        compose.onNodeWithText("No credentials").assertDoesNotExist()
        compose.onNodeWithText("Grant delegation").assertDoesNotExist()
        assertEquals(approvalsBefore, runBlocking { native.approvals().json })
        capture("qa-agent-principal-unavailable")
    }

    @Test fun browserDeepLinkRequiresTheMatchingAccountAndNeverApproves() {
        val foreign = "080g2081040g2081040g"
        val baseline = runBlocking { native.approvals().json }
        val link = net.dmsg.android.core.ConnectLink("request", foreign, "ab".repeat(32))
        compose.runOnIdle { client.acceptCallback(link.url()) }
        compose.onNodeWithText("Choose account").assertExists()
        compose.onNodeWithText(foreign).assertExists()
        compose.onNodeWithText("Approve").assertDoesNotExist()
        assertEquals(ReadState.Idle, client.browserConnections.request.state.value)
        assertEquals(baseline, runBlocking { native.approvals().json })
        capture("qa-browser-wrong-account")
        compose.onNodeWithText("Dismiss request").performClick()
        compose.runOnIdle { client.acceptCallback(link.url() + "&approve=true") }
        assertNotEquals("browser-request", client.navigation.state.value.current.page)
        assertEquals(baseline, runBlocking { native.approvals().json })
        compose.runOnIdle { client.dismissConnectLink(); client.clearNotice() }
    }

    @Test fun serviceDeletionRequiresDependencyReviewAndCancelHasNoEffect() {
        val account = client.state.value.session!!.account!!
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("account-delete")) }
        compose.waitUntil(10_000) { client.accountDeletion.status.state.value is ReadState.Failed && !client.state.value.working("account:devices") }
        val review = net.dmsg.android.core.AccountDeletionPreview("ab".repeat(32), account, 3u, false,
            listOf(net.dmsg.android.core.AccountDeletionChannel("cd".repeat(32), "Owned conversation")), emptyList(), false)
        compose.runOnIdle {
            client.accountDeletion.status.accept(client.accountDeletion.status.begin(), net.dmsg.android.core.AccountDeletionStatus(null))
            client.accountDeletion.preview.accept(client.accountDeletion.preview.begin(), review)
            client.account.devices.accept(client.account.devices.begin(), net.dmsg.android.core.AccountDevices(account, "aaaaa-aa", 1u, 1u, "Ready", 1u, listOf(
                net.dmsg.android.core.AccountDevice("aa".repeat(32), "Administrator", listOf("RootManage"), 1u, null, null, true))))
            client.accountDeletion.edit(net.dmsg.android.core.AccountDeletionDraft(account, false))
            client.clearNotice()
        }
        compose.onNodeWithText("Request account deletion").assertIsNotEnabled()
        compose.runOnIdle { client.accountDeletion.preview.accept(client.accountDeletion.preview.begin(), review.copy(channels = emptyList())) }
        compose.onNodeWithText("Request account deletion").assertIsNotEnabled()
        compose.runOnIdle { client.accountDeletion.edit(net.dmsg.android.core.AccountDeletionDraft(account, true)) }
        compose.onNodeWithText("Request account deletion").assertIsEnabled().performClick()
        compose.onNodeWithText("Permanently delete this service account?").assertExists()
        capture("qa-service-deletion-review")
        compose.onNodeWithText("Cancel").performClick()
        assertTrue(client.state.value.unlocked)
        assertNull(client.accountDeletion.status.value()!!.deletion)
        compose.runOnIdle { client.accountDeletion.status.fail(client.accountDeletion.status.begin(), "UNAVAILABLE") }
        compose.onNodeWithText("Request account deletion").assertIsNotEnabled()
    }

    @Test fun walletViewerCannotProposeAndUnverifiedAddressesCannotBeCopied() {
        val wallet = "aaaaa-aa"
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("wallet", wallet)) }
        compose.waitUntil(10_000) { client.wallets.details.state.value is ReadState.Failed }
        compose.runOnIdle {
            client.wallets.details.accept(client.wallets.details.begin(), net.dmsg.android.core.WalletDetails(wallet, "Reviewed wallet", false,
                net.dmsg.android.core.WalletMember("owner", "", "Viewer", "Active"), emptyList(),
                listOf(net.dmsg.android.core.WalletAddress(0u, "unverified-destination", "Account", "Icp", false, "chain_key", false, null)),
                emptyList(), net.dmsg.android.core.WalletPolicy(2u, 2u, kotlinx.serialization.json.JsonObject(emptyMap()))))
            client.wallets.history.accept(client.wallets.history.begin(), net.dmsg.android.core.WalletProposalPage(emptyList(), null))
            client.clearNotice()
        }
        compose.onNodeWithText("Propose transfer").assertDoesNotExist()
        compose.onNodeWithText("Address verification failed").assertExists()
        compose.onNodeWithText("Copy verified address").assertDoesNotExist()
        capture("qa-wallet-viewer")
    }

    @Test fun transferAmountPrecisionRejectsUnreviewedSubmission() {
        val wallet = "aaaaa-aa"
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("wallet-transfer", wallet)) }
        compose.waitUntil(10_000) { client.wallets.assets.state.value is ReadState.Failed && client.wallets.details.state.value is ReadState.Failed }
        compose.runOnIdle {
            client.wallets.details.accept(client.wallets.details.begin(), net.dmsg.android.core.WalletDetails(wallet, "Reviewed wallet", false,
                net.dmsg.android.core.WalletMember("owner", "", "Admin", "Active"), emptyList(),
                listOf(net.dmsg.android.core.WalletAddress(0u, "verified-source", "Account", "Icp", true, "chain_key", false, null)),
                emptyList(), net.dmsg.android.core.WalletPolicy(2u, 2u, kotlinx.serialization.json.JsonObject(emptyMap()))))
            client.wallets.assets.accept(client.wallets.assets.begin(), listOf(net.dmsg.android.core.WalletAsset("icp", "ICP", 8, null)))
            client.wallets.edit(net.dmsg.android.core.WalletTransferDraft(wallet, 0u, "icp", "reviewed-recipient", "1.00000001"))
            client.clearNotice()
        }
        compose.onNodeWithText("100000001").assertExists()
        val input = client.wallets.input(client.wallets.transfers.value.getValue(wallet))!!
        assertEquals("100000001", input.amount)
        assertEquals("reviewed-recipient", input.recipient)
        compose.onNodeWithText("Amount").performTextClearance()
        compose.onNodeWithText("Amount").performTextInput("1.000000001")
        assertNull(client.wallets.input(client.wallets.transfers.value.getValue(wallet)))
        compose.onNodeWithText("Preview transfer").assertIsNotEnabled()
        capture("qa-wallet-exact-amount")
        // No preview means no submission; neither a request journal nor proposal is invented.
        compose.runOnIdle { client.wallets.submitTransfer(wallet) }
        assertNull(client.wallets.submissions.value["transfer:" + wallet])
        assertTrue(runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.pendingWalletProposals().json) as kotlinx.serialization.json.JsonArray }.isEmpty())
    }

    @Test fun membershipKeepsUnverifiedRightsUnknownAndComparesAllFourTiers() {
        // Synthetic display values only; no certified remote entitlement is claimed.
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("membership")) }
        compose.waitUntil(10_000) { client.state.value.operations.none { it.startsWith("membership:") } }
        compose.runOnIdle {
            val limits = net.dmsg.android.core.PlanLimits(100uL * 1024u * 1024u, 2u, 5u, 2u)
            val plan = net.dmsg.android.core.MembershipPlan("Max", "Unverifiable", "Unverifiable", 1u, limits, null)
            client.membership.plan.accept(client.membership.plan.begin(), plan)
            client.membership.usage.accept(client.membership.usage.begin(), net.dmsg.android.core.MembershipUsage(null, null, "UNAVAILABLE", "AUTH_REQUIRED"))
            client.membership.catalog.accept(client.membership.catalog.begin(), net.dmsg.android.core.MembershipCatalog(
                net.dmsg.android.core.membershipTiers.mapIndexed { index, tier -> net.dmsg.android.core.PlanBenefit(tier, limits.copy(storage = limits.storage * (index + 1).toULong()), 100uL * 1024u * 1024u, index.toULong() * 10u, null) }, "UNAVAILABLE"))
            client.clearNotice()
        }
        compose.onNodeWithText("Status needs updating").assertExists()
        compose.onNodeWithText("Some usage is unavailable. Unknown values are not zero.").assertExists()
        capture("qa-membership-unverified")
        compose.onNodeWithTag("membership-list").performScrollToNode(hasText("Compare all plans"))
        compose.onNodeWithText("Compare all plans").performClick()
        compose.onNodeWithTag("membership-list").performScrollToNode(hasTestTag("plan-card-Max"))
        compose.onNodeWithTag("plan-card-Max").assertExists()
        compose.onAllNodesWithText("Current plan").assertCountEquals(0)
        capture("qa-membership-comparison")
    }

    @Test fun notificationChoicesUseIndependentConsentAndKeepConflictingEdits() {
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("notifications")) }
        compose.waitUntil(10_000) { client.notifications.preferences.state.value is ReadState.Ready }
        assertFalse(client.notifications.draft.value!!.global.enabled)
        assertFalse(client.notifications.draft.value!!.account!!.enabled)
        compose.onNodeWithTag("notification-settings").performScrollToNode(hasTestTag("Enable notifications"))
        compose.onNodeWithTag("Enable notifications").performClick()
        compose.waitUntil(10_000) { !client.state.value.working("notices:save") && client.notifications.preferences.value()?.global?.enabled == true }
        assertFalse(client.notifications.preferences.value()!!.account!!.enabled)
        compose.onNodeWithTag("notification-settings").performScrollToNode(hasTestTag("Notify for this account"))
        compose.onNodeWithTag("Notify for this account").performClick()
        compose.onNodeWithText("Cancel").performClick()
        assertFalse(client.notifications.preferences.value()!!.account!!.enabled)
        compose.onNodeWithTag("Notify for this account").performClick()
        compose.onNodeWithText("Enable for this account").performClick()
        compose.waitUntil(10_000) { client.notifications.preferences.value()?.account?.enabled == true && !client.state.value.working("notices:save") }
        val old = client.notifications.preferences.value()!!
        runBlocking { native.notificationSetGlobal(old.global.revision, old.global.copy(badge = "off").json()) }
        compose.runOnIdle { client.notifications.edit(global = old.global.copy(enabled = false)) }
        compose.waitUntil(10_000) { client.notifications.saveError.value == "VERSION_CONFLICT" && client.notifications.preferences.value()?.global?.badge == "off" }
        assertFalse(client.notifications.draft.value!!.global.enabled)
        compose.onNodeWithTag("notification-settings").performScrollToNode(hasText("Review and retry saved edits"))
        compose.onNodeWithText("Review and retry saved edits").performClick()
        compose.onNodeWithText("Apply").performClick()
        compose.waitUntil(10_000) { !client.state.value.working("notices:save") && client.notifications.saveError.value == null && client.notifications.preferences.value()?.global?.enabled == false }
        val stored = runBlocking { net.dmsg.android.core.parseNoticePreferences(kotlinx.serialization.json.Json.parseToJsonElement(native.notificationPreferences(null).json)) }
        assertFalse(stored.global.enabled); assertTrue(stored.account!!.enabled)
        capture("qa-notification-preferences")
    }

    @Test fun reminderReadAndSnoozeDoNotObserveOrCompleteTheOriginalAction() {
        val seeded = runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiReminders().json) as kotlinx.serialization.json.JsonArray }
        val action = net.dmsg.android.core.parseReminder(seeded[0])
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("reminders")) }
        compose.waitUntil(10_000) { client.notifications.reminders.value()?.items?.any { it.id == action.id } == true }
        compose.onNodeWithText("Mark reminder read").performClick()
        compose.waitUntil(10_000) { client.notifications.reminders.value()?.items?.find { it.id == action.id }?.isUnread == false }
        val read = client.notifications.reminders.value()!!.items.single { it.id == action.id }
        assertTrue(read.open); assertTrue(read.actionable); assertEquals(0uL, read.observed)
        assertEquals(9007199254740993uL, read.readVersion)
        compose.onNodeWithText("Remind later").performClick()
        compose.onNodeWithText("In 15 minutes").performClick()
        compose.waitUntil(10_000) { client.notifications.reminders.value()?.items?.find { it.id == action.id }?.snoozedUntil != null }
        capture("qa-reminders-action")
        compose.onNodeWithText("Recent").performClick()
        compose.waitUntil(10_000) { client.notifications.reminders.value()?.items?.size == 2 }
        compose.onNodeWithTag("reminders-list").performScrollToNode(hasText("Mark reminders for all accounts as read"))
        compose.onNodeWithText("Mark reminders for all accounts as read").performClick()
        compose.onNodeWithText("Mark as read").performClick()
        compose.waitUntil(10_000) { client.notifications.reminders.value()?.items?.all { !it.isUnread } == true }
        assertTrue(client.notifications.reminders.value()!!.items.all { it.observed == 0uL })
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked }
        assertNull(client.notifications.reminders.value())
    }

    @Test fun separateVaultDetailAndLockConcealTheDecryptedContent() {
        capture("qa-vault-light")
        compose.onNodeWithText("Studio account").performClick()
        compose.waitUntil(10_000) { client.vault.detail.state.value is ReadState.Ready }
        compose.onNodeWithText("Offline QA fixture · no real credentials").assertExists()
        capture("qa-vault-detail-light")
        compose.runOnIdle { client.dark(true) }
        capture("qa-vault-detail-dark")
        compose.onNodeWithContentDescription("Field actions").performClick()
        compose.onNodeWithText("Reveal").performClick()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("Sample-only password").fetchSemanticsNodes().isNotEmpty() }
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        compose.onNodeWithText("Offline QA fixture · no real credentials").assertDoesNotExist()
        compose.onNodeWithText("Studio account").assertDoesNotExist()
        compose.onNodeWithText("Sample-only password").assertDoesNotExist()
        assertEquals("locked", runBlocking { native.sessionState() }.state)
    }

    @Test fun automaticReadsAndTransferPollingDoNotExtendTheIdleUnlockWindow() {
        compose.runOnIdle { client.activity(); uptime += 14 * 60_000L + 30_000 }
        var read = false
        compose.runOnIdle { client.perform({ it.accounts() }, key = "test:automatic-read", quiet = true) { read = true } }
        compose.waitUntil(10_000) { read }
        val clipboard = InstrumentationRegistry.getInstrumentation().targetContext.getSystemService(android.content.ClipboardManager::class.java)
        compose.runOnIdle {
            client.clipboard.copy("fixture-only clipboard")
            assertEquals("fixture-only clipboard", clipboard.primaryClip?.getItemAt(0)?.text)
            uptime += 30_001
            client.checkInactivity()
            assertFalse(clipboard.hasPrimaryClip())
        }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        assertEquals("locked", runBlocking { native.sessionState() }.state)
    }

    @Test fun operationTimeoutAllowsAnImmediateRetryWithTheSameKey() {
        var result: String? = null
        var error: String? = null
        compose.runOnIdle {
            client.perform({ kotlinx.coroutines.withTimeout(10) { kotlinx.coroutines.delay(500) } }, key = "test:retry", quiet = true, onError = { code ->
                error = code
                client.perform({ "second" }, key = "test:retry") { result = it }
            })
        }
        compose.waitUntil(10_000) { result == "second" }
        assertEquals("TIMEOUT", error)
    }

    @Test fun openingAWorkspaceLinkAtColdStartKeepsTheSavedLockAndAppearanceSettings() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        val preferences = File(net.dmsg.android.platform.PrivateFiles(app).root, "preferences.json")
        fun saved() = org.json.JSONObject(preferences.readText())
        compose.runOnIdle { client.immediateLock(true); client.dark(true) }
        compose.waitUntil(10_000) { preferences.isFile && saved().optString("appearance") == "dark" && saved().optBoolean("immediate_lock") }
        runBlocking { withContext(Dispatchers.Main.immediate) { client.shutdown() } }
        compose.runOnUiThread {
            client = ClientController(app) { paths, platform ->
                MobileEngine.reopenUiFixture(directory.path, paths.staging.path, platform).also { native = it; it.lockNow() }
            }
            client.requestWorkspace(net.dmsg.android.core.WorkspaceLink("files"))
            uiVersion++
            client.start()
        }
        compose.waitUntil(15_000) { client.state.value.session?.state == "locked" && saved().optString("pending_workspace") == "dmsg://open/files" }
        val persisted = saved()
        assertEquals("dark", persisted.getString("appearance"))
        assertTrue(persisted.getBoolean("immediate_lock"))
    }

    @Test fun feedbackCannotSendAReportThatReplacedTheReviewedDraft() {
        compose.runOnIdle { client.feedback.load() }
        compose.waitUntil(10_000) { client.feedback.draft.value != null }
        compose.runOnIdle { client.feedback.update(client.feedback.draft.value!!.copy(text = "Reviewed report")) }
        compose.waitUntil(10_000) { client.feedback.report.value()?.text == "Reviewed report" }
        val reviewed = client.feedback.report.value()!!
        val other = net.dmsg.core.FeedbackDraftInput(net.dmsg.android.core.requestId(), 0u, "Different report", "", emptyList())
        runBlocking { native.clearFeedback(reviewed.id, reviewed.revision); native.saveFeedback(other) }
        compose.runOnIdle { client.feedback.send() }
        compose.waitUntil(10_000) { client.feedback.report.value()?.id == other.id }
        assertEquals("draft", client.feedback.report.value()?.state)
    }

    @Test fun cloudInventoryAndVerificationProgressRemainDistinctFromCompletedTransfers() {
        // Synthetic display states only. No remote upload/download is claimed here.
        compose.runOnIdle { client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("cloud-files")) }
        compose.waitUntil(10_000) { !client.state.value.operations.any { it.startsWith("cloud:list:") } }
        compose.runOnIdle {
            client.cloudFiles.inventory.accept(client.cloudFiles.inventory.begin(), net.dmsg.android.core.CloudInventory(listOf(
                net.dmsg.android.core.CloudBackup("qa-cloud", "file-one", "version-one", "Design archive.zip", listOf("Studio"), 42uL * 1024u * 1024u,
                    "application/zip", "01".repeat(32), "Other authorized device", 4u, null, 1_800_000_000_000u, 44uL * 1024u * 1024u, false, false),
                net.dmsg.android.core.CloudBackup("qa-locked-cloud", null, null, null, emptyList(), null, null, null,
                    "Other authorized device", 3u, null, 1_800_000_000_000u, 18uL * 1024u * 1024u, true, false)
            ), true, 1_800_000_000_000u))
            client.clearNotice()
        }
        compose.onNodeWithText("Design archive.zip").assertExists()
        capture("qa-cloud-inventory")
        compose.onNodeWithText("Encrypted backup").performClick()
        compose.onNodeWithText("Synchronize the account to obtain the content key before restoring this copy.").assertExists()
        compose.onNodeWithText("Restore this copy").assertDoesNotExist()
        capture("qa-cloud-missing-key")
        compose.runOnIdle { client.fileWork.pause(); client.navigation.push(net.dmsg.android.core.Route("file-transfers")) }
        compose.waitUntil(10_000) { !client.state.value.working("files:jobs") }
        compose.runOnIdle {
            client.fileWork.stop()
            client.clearNotice()
            client.fileWork.jobs.accept(client.fileWork.jobs.begin(), listOf(net.dmsg.android.core.DurableFileJob(
                "qa-download", "download", "Design archive.zip", null, null, "verifying", 42u, 42u, 42uL * 1024u * 1024u, 42uL * 1024u * 1024u, null, null)))
        }
        compose.onNodeWithText("Restore · Verifying all contents").assertExists()
        compose.onNodeWithText("Verified and complete").assertDoesNotExist()
        capture("qa-file-verifying")
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(app.cacheDir, "dmsg-shared/version-fixture/" + UUID.randomUUID() + ".txt")
        source.parentFile!!.mkdirs(); source.writeText("Version navigation fixture, no personal content")
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", source)
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); client.transfers.importResult(uri) }
        compose.waitUntil(15_000) { client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } }
        source.delete()
        val local = runBlocking { net.dmsg.android.core.parseFiles(native.files("root").json).second.single() }
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("files", id = local.id, revision = "missing-version")) }
        compose.waitUntil(10_000) { client.library.detail.state.value is ReadState.Ready }
        compose.onNodeWithText("The requested version is unavailable on this device. Choose another version explicitly.").assertExists()
        compose.onNodeWithText("Share a verified copy").assertDoesNotExist()
        capture("qa-file-version-missing")
        compose.onNodeWithText("View the current version").performClick()
        compose.onNodeWithText(local.name).assertExists()
    }

    @Test fun fileLinkReviewDefaultsToAnonymousAndRetriesOneFixedVersion() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(app.cacheDir, "dmsg-shared/link-fixture/Reviewed version.txt")
        source.parentFile!!.mkdirs(); source.writeText("Public QA sample for fixed-version sharing")
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", source)
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); client.transfers.importResult(uri) }
        compose.waitUntil(15_000) { client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } }
        source.delete()
        val file = runBlocking { net.dmsg.android.core.parseFiles(native.files("root").json).second.single() }
        compose.runOnIdle { client.clearNotice(); client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("file-link-compose", id = file.id, revision = file.version)) }
        compose.waitUntil(10_000) { client.fileLinks.target.state.value is ReadState.Ready }
        compose.onNodeWithTag("file-link-profile").performScrollTo().assertIsOff()
        capture("qa-file-link-privacy")
        compose.onNodeWithText("Optional password").performScrollTo().performTextInput("QA-only optional password")
        compose.onNodeWithText("Share by link").assertIsDisplayed()
        compose.waitUntil(5_000) { compose.onNodeWithText("Optional password").isDisplayed() }
        capture("qa-file-link-compose")
        compose.runOnIdle { client.fileLinks.create() }
        compose.waitUntil(20_000) { client.fileLinks.intents.value()?.size == 1 && !client.state.value.operations.any { it.startsWith("links:run:") || it == "links:prepare" } }
        val original = client.fileLinks.intents.value()!!.single()
        assertEquals(file.version, original.version); assertFalse(original.showProfile); assertTrue(original.password)
        assertEquals("", client.fileLinks.draft.value.password)
        compose.runOnIdle { client.clearNotice(); client.fileLinks.resume(original.id) }
        compose.waitUntil(20_000) { !client.state.value.working("links:run:${original.id}") }
        val persisted = runBlocking { native.fileLinkIntents().json }
        val rows = kotlinx.serialization.json.Json.parseToJsonElement(persisted) as kotlinx.serialization.json.JsonArray
        assertEquals(1, rows.size)
        assertEquals(original.id, net.dmsg.android.core.parseFileLinkIntent(rows.single()).id)
        assertFalse(persisted.contains("QA-only optional password"))
        compose.runOnIdle { client.clearNotice() }
        capture("qa-file-link-original-task")
    }

    @Test fun requestBatchRetriesOriginalRecipientsAndKeepsUnpublishedKeysPrivate() {
        compose.runOnIdle { client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("file-request-compose")) }
        compose.onNodeWithText("What do you need?").performTextInput("Signed project documents")
        compose.onNodeWithText("Instructions (optional)").performTextInput("Upload the signed copy, please.")
        compose.onNodeWithText("Who is it for? (optional)").performTextInput("Ada\nLee")
        capture("qa-file-request-compose")
        compose.runOnIdle { client.fileRequests.create() }
        compose.waitUntil(20_000) { client.fileRequests.intents.value()?.size == 2 && !client.state.value.working("requests:prepare") && !client.state.value.working("requests:publish") }
        val original = client.fileRequests.intents.value()!!
        val batch = original.first().batch
        assertEquals(setOf("Ada", "Lee"), original.map { it.label }.toSet())
        assertTrue(original.all { it.batch == batch && it.link == null })
        compose.runOnIdle { client.clearNotice(); client.fileRequests.resumeBatch(batch) }
        compose.waitUntil(20_000) { !client.state.value.working("requests:publish") }
        val persisted = runBlocking { native.fileRequestIntents().json }
        val rows = (kotlinx.serialization.json.Json.parseToJsonElement(persisted) as kotlinx.serialization.json.JsonArray).map { net.dmsg.android.core.parseFileRequestIntent(it) }
        assertEquals(original.map { it.id }.toSet(), rows.map { it.id }.toSet())
        assertFalse(persisted.contains("hpke_seed")); assertFalse(persisted.contains("page_key"))
        compose.runOnIdle { client.clearNotice() }
        capture("qa-file-request-original-batch")
    }

    @Test fun receivedFileStatesNeverAutomaticallyImportAndDowngradeKeepsSaveAvailable() {
        // Display fixtures only: no remote receipt, download or deletion is claimed.
        val before = runBlocking { native.files("root").json }
        compose.runOnIdle { client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("file-requests")) }
        compose.waitUntil(10_000) { !client.state.value.operations.any { it.startsWith("requests:list:") || it == "cloud:capacity" } }
        val id = "ca".repeat(32)
        val now = System.currentTimeMillis().toULong()
        val request = net.dmsg.android.core.FileRequest(id, "https://dmsg.net/r/qa-only#synthetic", "Project delivery", "One link per collaborator", "Ada", now + 14uL * 86_400_000u, 500uL * 1024u * 1024u, now, "paused", 3u, 900u, true, now)
        compose.runOnIdle {
            client.fileRequests.links.accept(client.fileRequests.links.begin(), listOf(request))
            client.cloudFiles.capacity.accept(client.cloudFiles.capacity.begin(), net.dmsg.android.core.BackupCapacity(100uL * 1024u * 1024u, 1_000_000u, 900u, 0u, 0u))
            client.clearNotice()
        }
        compose.onNodeWithText("New collection link").assertIsNotEnabled()
        compose.onNodeWithText("Project delivery").performClick()
        compose.waitUntil(10_000) { !client.state.value.working("requests:files") }
        val incoming = net.dmsg.android.core.ReceivedFile("cb".repeat(32), id, "received", "Signed agreement.pdf", "application/pdf", 300u, "Ada", "Please review", null, now, 320u, null)
        compose.runOnIdle {
            client.fileRequests.received.accept(client.fileRequests.received.begin(), net.dmsg.android.core.FileRequestsCoordinator.Received(id, listOf(incoming,
                incoming.copy(id = "cc".repeat(32), state = "unreadable", name = null, mime = null, size = null, senderName = null, senderNote = null),
                incoming.copy(id = "cd".repeat(32), state = "withdrawn", name = "Withdrawn draft.pdf"))))
            client.clearNotice()
        }
        compose.onNodeWithText("Save to library").performScrollTo().assertIsEnabled()
        capture("qa-file-request-received")
        compose.onNodeWithText("Save to library").performClick()
        compose.onNodeWithText("Save into folder").assertExists()
        compose.onNodeWithText("Cancel").performClick()
        assertEquals(before, runBlocking { native.files("root").json })
        assertTrue(client.fileRequests.saving.value.isEmpty())
    }

    @Test fun multipleAttachmentsStayInTheDraftThenQueueTogetherWithoutErasingNewTyping() {
        val channel = "ab".repeat(32)
        compose.runOnIdle { client.navigation.open("messages"); client.navigation.select(channel) }
        compose.waitUntil(10_000) { client.messages.draftsReady.value }
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val sources = listOf("First attachment.txt", "Second attachment.txt").map { name ->
            File(app.cacheDir, "dmsg-shared/composer-${UUID.randomUUID()}/$name").apply { parentFile!!.mkdirs(); writeText("Synthetic attachment contents") }
        }
        val uris = sources.map { androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", it) }
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root", channel)); client.transfers.importResults(uris) }
        compose.waitUntil(15_000) { client.messages.composer(channel).attachments.size == 2 && client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } }
        sources.forEach(File::delete)
        assertTrue(runBlocking { native.outbox(channel).items }.isEmpty())
        compose.onNode(hasSetTextAction() and hasText("Continue the conversation")).performTextReplacement("Review both files before Friday.")
        compose.waitUntil(10_000) { runBlocking { native.messageComposers().json }.contains("Review both files before Friday.") }
        val before = client.messages.composer(channel)
        assertEquals(2, before.attachments.size)
        compose.runOnIdle { client.navigation.open("files"); client.navigation.open("messages"); client.clearNotice() }
        assertEquals(before, client.messages.composer(channel))
        capture("qa-message-multiple-attachments")
        compose.runOnIdle {
            client.messages.send(channel)
            client.messages.edit(channel, before.copy(text = "Next draft while the queue commits"))
        }
        compose.waitUntil(20_000) { !client.state.value.working("message:queue:$channel") && client.messages.composer(channel).attachments.isEmpty() }
        val queued = runBlocking { native.outbox(channel).items }
        assertEquals(2, queued.size)
        assertEquals("Review both files before Friday.", queued[0].text)
        assertTrue(queued[1].text.isEmpty())
        assertEquals(listOf("First attachment.txt", "Second attachment.txt"), queued.map { it.attachmentName })
        assertEquals("Next draft while the queue commits", client.messages.composer(channel).text)
        compose.waitUntil(10_000) { runBlocking { native.messageComposers().json }.contains("Next draft while the queue commits") }
        compose.runOnIdle { client.clearNotice() }
        capture("qa-message-persistent-outbox")
    }

    @Test fun lockingRevokesAndDeletesAnUnfinishedCameraCapture() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val paths = net.dmsg.android.platform.PrivateFiles(app)
        val owner = checkNotNull(client.state.value.session?.account)
        assertTrue(runCatching { runBlocking { paths.stage(android.net.Uri.fromFile(File(app.filesDir, "private-file")), owner) { true } } }.isFailure)
        lateinit var uri: android.net.Uri
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); uri = checkNotNull(client.transfers.beginCapture()) }
        val image = Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888)
        checkNotNull(app.contentResolver.openOutputStream(uri)).use { image.compress(Bitmap.CompressFormat.JPEG, 90, it) }; image.recycle()
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        assertTrue(runCatching { app.contentResolver.openInputStream(uri)?.use { it.read() } }.isFailure)
        compose.runOnIdle { client.transfers.captureResult(true) }
        assertTrue(client.transfers.tasks.value.isEmpty())
    }

    @Test fun documentPickerCancellationReturnsThroughThePrivacyCover() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        compose.runOnIdle { client.navigation.open("files") }
        compose.waitUntil(10_000) { client.library.listing.value()?.folder == "root" }
        capture("qa-file-picker-launch")
        compose.onNodeWithContentDescription("Import files").performClick()
        compose.onNodeWithText("Choose documents").performClick()
        compose.waitUntil(10_000) { instrumentation.uiAutomation.rootInActiveWindow?.packageName?.toString()?.contains("documentsui") == true }
        compose.runOnUiThread { client.background() }
        assertTrue(client.state.value.concealed)
        assertNotNull(client.transfers.pendingImportStamp())
        assertTrue(instrumentation.uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK))
        instrumentation.waitForIdleSync()
        compose.runOnUiThread { client.foreground() }
        compose.waitUntil(10_000) { !client.state.value.concealed && client.transfers.pendingImportStamp() == null }
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); client.transfers.importResult(null) }
    }

    @Test fun previewAndExportRequireTheCompleteVerifiedFileAndClearOnLock() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(app.cacheDir, "dmsg-shared/preview-${UUID.randomUUID()}/Review notes.txt")
        val content = "Only verified file contents appear here.\n中文与 emoji 🧪 remain intact."
        source.parentFile!!.mkdirs(); source.writeText(content)
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", source)
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); client.transfers.importResult(uri) }
        compose.waitUntil(15_000) { client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } }
        source.delete()
        val file = runBlocking { net.dmsg.android.core.parseFiles(native.files("root").json).second.single() }
        compose.runOnIdle { client.clearNotice(); client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("file-preview", id = file.id, revision = file.version)) }
        compose.waitUntil(10_000) { client.filePreview.preview.state.value is ReadState.Ready }
        compose.onNodeWithText("Complete file verified").assertExists()
        compose.onNodeWithText(content).assertExists()
        capture("qa-file-verified-text")
        val target = File(app.cacheDir, "dmsg-shared/export-${UUID.randomUUID()}/Saved copy.txt")
        target.parentFile!!.mkdirs(); target.createNewFile()
        val destination = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", target)
        compose.runOnIdle { assertTrue(client.transfers.beginExport(file)); client.transfers.exportResult(destination) }
        compose.waitUntil(10_000) { client.transfers.tasks.value["file:export:${file.id}"]?.phase == "done" }
        assertEquals(content, target.readText())
        val blob = directory.walkTopDown().filter { it.isFile && it.name.matches(Regex("[0-9a-f]{64}")) && it.parentFile?.parentFile?.name == "blobs" }.single()
        val original = blob.readBytes()
        blob.writeBytes(original.copyOf().apply { this[0] = (this[0].toInt() xor 64).toByte() })
        compose.runOnIdle { client.filePreview.open(file.id, file.version) }
        compose.waitUntil(10_000) { client.filePreview.preview.state.value is ReadState.Failed }
        compose.onNodeWithText(content).assertDoesNotExist()
        compose.onNodeWithText("Complete file verified").assertDoesNotExist()
        capture("qa-file-verification-failed")
        blob.writeBytes(original)
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        assertNull(client.filePreview.preview.value())
    }

    @Test fun verifiedImagePreviewRespectsOrientationAndBoundsItsBitmap() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(app.cacheDir, "dmsg-shared/preview-${UUID.randomUUID()}/Reference image.jpg")
        source.parentFile!!.mkdirs()
        val image = Bitmap.createBitmap(2400, 1200, Bitmap.Config.ARGB_8888)
        val canvas = android.graphics.Canvas(image)
        canvas.drawColor(android.graphics.Color.rgb(245, 247, 239))
        val paint = android.graphics.Paint().apply { color = android.graphics.Color.rgb(49, 91, 64) }
        canvas.drawRoundRect(220f, 220f, 2180f, 980f, 100f, 100f, paint)
        source.outputStream().use { image.compress(Bitmap.CompressFormat.JPEG, 90, it) }; image.recycle()
        android.media.ExifInterface(source).apply {
            setAttribute(android.media.ExifInterface.TAG_ORIENTATION, android.media.ExifInterface.ORIENTATION_ROTATE_90.toString())
            saveAttributes()
        }
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", source)
        compose.runOnIdle { assertTrue(client.transfers.beginImport("root")); client.transfers.importResult(uri) }
        compose.waitUntil(15_000) { client.transfers.tasks.value.values.any { it.kind == "import" && it.phase == "done" } }
        val file = runBlocking { net.dmsg.android.core.parseFiles(native.files("root").json).second.single() }
        compose.runOnIdle { client.clearNotice(); client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("file-preview", id = file.id, revision = file.version)) }
        compose.waitUntil(10_000) { client.filePreview.preview.state.value is ReadState.Ready }
        val bitmap = checkNotNull(client.filePreview.preview.value()?.bitmap)
        assertTrue(bitmap.width < bitmap.height && bitmap.height <= 1600)
        capture("qa-file-verified-image")
        source.delete()
    }

    @Test fun fileMetadataConflictsKeepInputAndHistoryUsesItsOwnMediaType() {
        val detail = runBlocking { net.dmsg.android.core.parseFileDetails(kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiFileHistory().json)) }
        val id = detail.file.id
        val original = detail.versions.first { !it.current }
        assertEquals("text/plain", original.mime)
        assertEquals("image/png", detail.file.mime)
        compose.runOnIdle { client.navigation.open("files"); client.navigation.push(net.dmsg.android.core.Route("files", id = id)) }
        compose.waitUntil(10_000) { client.library.detail.value()?.file?.id == id }
        compose.onNodeWithText("Edit notes, tags and favorite").performScrollTo().performClick()
        compose.onNodeWithText("Note", substring = false).performTextInput("My reviewed note")
        runBlocking { native.updateFileMetadata(id, net.dmsg.core.FileMetadata(emptyList(), "", false), net.dmsg.core.FileMetadata(listOf("added elsewhere"), "Concurrent stored note", true)) }
        compose.onNodeWithText("Save", substring = false).performClick()
        compose.waitUntil(10_000) { client.library.editorError.value == "VERSION_CONFLICT" && client.library.detail.value()?.file?.note == "Concurrent stored note" }
        compose.onNodeWithText("My reviewed note").assertExists()
        compose.onNodeWithText("Use my edits after reviewing this update").performScrollTo().performClick()
        compose.onNodeWithText("Save", substring = false).performClick()
        compose.waitUntil(10_000) { client.library.editor.value == null && client.library.detail.value()?.file?.note == "My reviewed note" }
        assertEquals(listOf("added elsewhere"), client.library.detail.value()!!.file.tags)
        assertTrue(client.library.detail.value()!!.file.starred)
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("files", id = id, revision = original.id)) }
        compose.onNodeWithText("Make this the current version").performScrollTo().assertExists()
        capture("qa-file-version-review")
        compose.onNodeWithText("Make this the current version").performClick()
        compose.onNodeWithText("Confirm").performClick()
        compose.waitUntil(10_000) { client.library.detail.value()?.file?.version == original.id }
        assertEquals("text/plain", client.library.detail.value()!!.file.mime)
        compose.onNodeWithText("Preview verified contents").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.filePreview.preview.value()?.file?.version == original.id }
        compose.onNodeWithText("Original reviewed text version").assertExists()
    }

    @Test fun batchFileActionsKeepFailuresSelectedAndPurgeOnlyTrashedFiles() {
        val detail = runBlocking { net.dmsg.android.core.parseFileDetails(kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiFileHistory().json)) }
        val id = detail.file.id
        val folder = runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.createFolder("root", "Batch target").json) as kotlinx.serialization.json.JsonObject }
        val folderId = (folder["id"] as kotlinx.serialization.json.JsonPrimitive).content
        compose.runOnIdle { client.navigation.open("files"); client.library.browse("root") }
        compose.waitUntil(10_000) { client.library.listing.value()?.files?.any { it.id == id } == true }
        compose.runOnIdle {
            client.library.choose(id); client.library.choose(folderId)
            client.library.batch("move", listOf(net.dmsg.android.core.FileActionTarget(id, detail.file.name, false), net.dmsg.android.core.FileActionTarget(folderId, "Batch target", true)), folderId)
        }
        compose.waitUntil(10_000) { !client.state.value.working("files:batch") && client.library.results.value.size == 2 }
        assertNull(client.library.results.value.first { it.id == id }.error)
        assertNotNull(client.library.results.value.first { it.id == folderId }.error)
        assertTrue(folderId in client.library.selection.value)
        val moved = runBlocking { net.dmsg.android.core.parseFileDetails(kotlinx.serialization.json.Json.parseToJsonElement(native.fileDetails(id).json)) }
        assertEquals(folderId, moved.file.folder)
        assertTrue(runCatching { runBlocking { native.purgeFile(folderId) } }.isFailure)
        compose.runOnIdle { client.library.clearSelection(); client.library.batch("trash", listOf(net.dmsg.android.core.FileActionTarget(folderId, "Batch target", true))) }
        compose.waitUntil(10_000) { !client.state.value.working("files:batch") }
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("file-trash")) }
        compose.waitUntil(10_000) { client.library.listing.value()?.trash == true && client.library.listing.value()?.folders?.any { it.id == folderId } == true }
        capture("qa-file-trash-risk")
        compose.onNodeWithText("Empty local trash").performScrollTo().performClick()
        compose.onNodeWithText("Confirm").performClick()
        compose.waitUntil(10_000) { !client.state.value.working("files:batch") && client.library.results.value.any { it.id == folderId && it.action == "purge" && it.error == null } }
        assertTrue(runCatching { runBlocking { native.fileDetails(id) } }.isFailure)
        compose.onNodeWithText("Operation results").assertExists()
    }

    @Test fun aSavedMessageAttachmentUsesItsLocalVerifiedCopyWithoutDownloading() {
        // Message metadata is synthetic; image import, local lookup and preview use the core.
        val seeded = runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiReceivedFile().json) as kotlinx.serialization.json.JsonObject }
        val channel = (seeded["channel"] as kotlinx.serialization.json.JsonPrimitive).content
        val id = (seeded["file"] as kotlinx.serialization.json.JsonPrimitive).content
        compose.runOnIdle { client.navigation.open("messages"); client.navigation.select(channel) }
        compose.waitUntil(10_000) { client.messageAttachments.states.value["$channel:1"]?.file?.id == id }
        compose.onNodeWithText("Local copy saved to Files").assertExists()
        assertFalse(client.state.value.operations.any { it.startsWith("message:file-save:") })
        capture("qa-message-saved-attachment")
        compose.onNodeWithText("Preview", substring = false).performClick()
        compose.waitUntil(10_000) { client.filePreview.preview.value()?.bitmap != null }
        compose.onNodeWithText("Complete file verified").assertExists()
    }

    @Test fun markingAnOpenLetterUnreadDoesNotImmediatelyMarkItReadAgain() {
        val letter = net.dmsg.android.core.Letter("ac".repeat(32), "040g2081040g2081040g",
            "An already read sample letter", true, false, "delivered", 1u)
        compose.runOnIdle { client.navigation.open("inbox") }
        compose.waitUntil(10_000) {
            client.letters.incoming.value() != null && client.safety.blocked.value() != null &&
                client.state.value.operations.none { it.startsWith("letter:") }
        }
        compose.runOnIdle { client.letters.incoming.accept(client.letters.incoming.begin(), listOf(letter)) }
        compose.onNodeWithText(letter.text).performClick()
        compose.onNodeWithText("Mark unread").performScrollTo().performClick()
        compose.waitUntil(10_000) { !client.isWorking("letter:mark:" + letter.id) }
        var markedAgain = false
        val observer = runBlocking { withContext(Dispatchers.Main.immediate) {
            CoroutineScope(Dispatchers.Main.immediate).launch(start = CoroutineStart.UNDISPATCHED) {
                client.state.collect { if (it.working("letter:mark:" + letter.id)) markedAgain = true }
            }
        } }
        try {
            // Supply the acknowledged unread projection without requiring a live inbox service.
            compose.runOnIdle { client.letters.incoming.accept(client.letters.incoming.begin(), listOf(letter.copy(read = false))) }
            compose.waitForIdle()
            compose.onNodeWithText("Mark read").assertExists()
            compose.runOnIdle { assertFalse("The explicit unread choice must survive the detail recomposition", markedAgain) }
        } finally { observer.cancel() }
    }

    @Test fun letterTextSurvivesClosingTheEditorAndIsSealedInTheNativeWorkspace() {
        compose.onAllNodesWithText("Messages").onLast().performClick()
        compose.onNodeWithText("Letters").performClick()
        compose.onNodeWithContentDescription("Write a letter").performClick()
        compose.onNodeWithText("Recipient account ID").performTextReplacement("040g2081040g2081040g")
        val text = "Last authored line · 中文 🧪"
        compose.onNodeWithText("Message", substring = false).performTextInput(text)
        compose.onNodeWithText("Keep draft").performScrollTo().assertIsDisplayed()
            .performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.OnClick)
        compose.waitUntil(10_000) { compose.onAllNodes(hasSetTextAction() and hasText(text)).fetchSemanticsNodes().isEmpty() }
        compose.onNodeWithText("Drafts").performClick()
        compose.waitUntil(10_000) { !client.state.value.operations.any { it.startsWith("letter:save:") } }
        compose.onNodeWithText(text).assertExists()
        capture("qa-letter-drafts")
        val saved = runBlocking { native.letterDrafts().json }
        assertTrue(saved.contains(text))
        // No plaintext form state is written by Android or SQLite's clear metadata.
        directory.walkTopDown().filter { it.isFile && (it.name.endsWith("sqlite") || it.name.endsWith("-wal")) }.forEach {
            assertFalse(it.readBytes().toString(Charsets.ISO_8859_1).contains("Last authored line"))
        }
        compose.runOnIdle { client.navigation.open("vault") }
        compose.onNodeWithContentDescription("New item").assertExists()
        compose.runOnIdle { client.navigation.open("messages") }
        compose.onNodeWithText("Drafts").performClick()
        compose.onNodeWithText(text).assertExists()
    }

    @Test fun qrAcceptsOnlyTheChosenSourceAndDeliversItsResultOnce() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(app.cacheDir, "dmsg-shared/qr-fixture/" + UUID.randomUUID() + ".png")
        file.parentFile!!.mkdirs()
        val text = "Offline QR fixture"
        file.writeBytes(android.util.Base64.decode(net.dmsg.core.qrPng(text).substringAfter(','), android.util.Base64.DEFAULT))
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", file)
        val qr = client.qr
        try {
            compose.runOnIdle {
                qr.open("old")
                val old = (qr.state.value as QrRequest).id
                assertTrue(qr.chooseImage(old))
                qr.open("current")
                val current = (qr.state.value as QrRequest).id
                assertTrue(qr.chooseImage(current))
                qr.image(app, old, null)
                qr.cameraResult(current, "late frame")
                assertEquals(QrStep.PICKING_IMAGE, (qr.state.value as QrRequest).step)
                qr.image(app, current, uri)
                qr.cameraResult(old, "old scan")
                qr.image(app, current, null)
            }
            compose.waitUntil(10_000) { qr.state.value.result?.text == text }
            compose.runOnIdle {
                assertNull(qr.consume("old"))
                assertEquals(text, qr.consume("current"))
                assertNull(qr.consume("current"))
                qr.open("camera")
                val current = (qr.state.value as QrRequest).id
                qr.permission(current)
                qr.cameraResult(current, "camera frame")
                assertEquals("camera frame", qr.consume("camera"))
                qr.open("cancel")
                val cancelled = (qr.state.value as QrRequest).id
                assertTrue(qr.chooseImage(cancelled))
                qr.image(app, cancelled, null)
                assertEquals(QrState.Idle, qr.state.value)
            }
            java.io.RandomAccessFile(file, "rw").use { it.setLength(8L * 1024 * 1024 + 1) }
            compose.runOnIdle {
                qr.open("large")
                val current = (qr.state.value as QrRequest).id
                assertTrue(qr.chooseImage(current))
                qr.image(app, current, uri)
            }
            compose.waitUntil(10_000) { qr.state.value == QrState.Idle && client.state.value.error == "FILE_LIMIT" }
        } finally { file.delete() }
    }

    @Test fun deniedCameraRetainsPasteAndDoesNotCreateAnApproval() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val app = instrumentation.targetContext
        instrumentation.uiAutomation.serviceInfo = instrumentation.uiAutomation.serviceInfo.apply {
            flags = flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_REPORT_VIEW_IDS
        }
        instrumentation.uiAutomation.revokeRuntimePermission(app.packageName, android.Manifest.permission.CAMERA)
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(
            "pm clear-permission-flags " + app.packageName + " android.permission.CAMERA user-set user-fixed"
        )).use { it.readBytes() }
        compose.runOnIdle { client.navigation.open("pair-approve") }
        compose.onNodeWithText("Scan device request").performClick()
        // Advance Compose's test clock so the scanner's LaunchedEffect requests permission
        // before polling the separate system window.
        compose.waitForIdle()
        instrumentation.uiAutomation.waitForIdle(200, 5000)
        val deadline = android.os.SystemClock.elapsedRealtime() + 8000
        var deniedBySystem = false
        var denyRequested = false
        while (android.os.SystemClock.elapsedRealtime() < deadline) {
            fun denied(node: android.view.accessibility.AccessibilityNodeInfo?): Boolean {
                node ?: return false
                val id = node.viewIdResourceName?.substringAfter(":id/")
                if (id == "permission_deny_button" || id == "permission_deny_and_dont_ask_again_button")
                    return node.isEnabled && node.isVisibleToUser && node.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK)
                for (i in 0 until node.childCount) if (denied(node.getChild(i))) return true
                return false
            }
            val root = instrumentation.uiAutomation.rootInActiveWindow
            val owner = root?.packageName?.toString()
            if (denyRequested && owner == app.packageName &&
                app.checkSelfPermission(android.Manifest.permission.CAMERA) == android.content.pm.PackageManager.PERMISSION_DENIED) {
                deniedBySystem = true
                break
            }
            if (owner != null && owner != app.packageName && owner != instrumentation.context.packageName && denied(root)) denyRequested = true
            Thread.sleep(50)
        }
        if (!deniedBySystem) capturePermissionFailure("qa-camera-permission-not-denied")
        assertTrue("The system permission dialog was explicitly denied", deniedBySystem)
        try {
            compose.waitUntil(10_000) { runCatching { compose.onAllNodesWithText("Use pasted text").fetchSemanticsNodes().isNotEmpty() }.getOrDefault(false) }
        } catch (error: Throwable) {
            capturePermissionFailure("qa-camera-permission-return")
            throw error
        }
        compose.onNodeWithText("Read from an image").assertExists()
        assertTrue(client.approvals.receipts.value.isEmpty())
        capture("qa-camera-denied")
        compose.onNodeWithText("Use pasted text").performClick()
        compose.onNodeWithText("Device request").performTextInput("untrusted QR text")
        compose.onNodeWithText("Review request").assertExists()
        assertTrue(client.approvals.receipts.value.isEmpty())
    }

    @Test fun historicalReadsCannotReplaceTheCurrentDetailOrEditorRevision() {
        val id = client.vault.listing.value()!!.items.first { it.title == "Studio account" }.id
        val previous = runBlocking { native.vaultItem(id, null).item }
        val current = runBlocking { native.saveItem(id, previous.revision, VaultEdit("Updated studio account", previous.kind, "Current notes", previous.favorite, previous.tags,
            previous.fields.map { FieldEdit(it.id, it.kind, it.label, it.section, if (it.concealed) null else it.value) }, previous.files)).item }
        compose.runOnIdle { client.vault.browse() }
        compose.waitUntil(10_000) { client.vault.listing.value()?.items?.any { it.title == "Updated studio account" } == true }
        compose.onNodeWithText("Updated studio account").performClick()
        compose.onNodeWithText("Version history").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.vaultHistory.history.value()?.versions?.size == 3 }
        compose.runOnIdle {
            val changes = client.vaultHistory.history.value()!!.versions.first().changes
            assertEquals(setOf("title", "notes"), changes.toSet())
        }
        compose.onNodeWithText("Current notes").assertDoesNotExist()
        compose.onNodeWithTag("vault-version-device:" + current.revision).performClick()
        compose.onNode(hasSetTextAction()).performTextInput("Studio phone")
        compose.onNodeWithText("Save").performClick()
        compose.waitUntil(10_000) { client.vaultHistory.history.value()?.versions?.all { it.alias == "Studio phone" } == true }
        capture("qa-vault-history")
        compose.onNodeWithTag("vault-version:" + previous.revision).performClick()
        compose.onNodeWithText("You are viewing an immutable historical version.").assertExists()
        compose.onNodeWithText("Offline QA fixture · no real credentials").assertExists()
        assertEquals(current.revision, client.vault.detail.value()?.revision)
        compose.runOnIdle { client.navigation.back(); client.navigation.back() }
        compose.onNodeWithText("Current notes").assertExists()
    }

    @Test fun privacyAndSavedAccountMetadataRemainAvailableWhileLocked() {
        val account = client.state.value.session!!.account!!
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        compose.onNodeWithText("Privacy policy").performScrollTo().performClick()
        compose.onNodeWithText("Android app").assertExists()
        compose.waitUntil(10_000) { compose.onAllNodesWithText("dMsg Privacy Policy").fetchSemanticsNodes().isNotEmpty() }
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals(23, app.assets.list("privacy")!!.size)
        capture("qa-offline-privacy")
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Saved accounts").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.accountWorkspace.accounts.value()?.any { it.id == account } == true }
        compose.onAllNodesWithText(account).onLast().assertExists()
        compose.onNodeWithText("Current account").assertExists()
        assertFalse(client.state.value.unlocked)
        assertEquals("locked", runBlocking { native.sessionState() }.state)
    }

    @Test fun publicProfileDraftAndOrientedAvatarStayLocalAndCacheClearKeepsLibraryFiles() {
        runBlocking { native.seedUiProfile() }
        val file = runBlocking { net.dmsg.android.core.parseFileDetails(kotlinx.serialization.json.Json.parseToJsonElement(native.seedUiFileHistory().json)) }
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("profile")) }
        compose.waitUntil(10_000) { client.profile.saved.value()?.name == "Mina's workspace" }
        compose.onNodeWithText("Name", substring = false).performTextReplacement("Reviewed local name")
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(app.cacheDir, "dmsg-shared/profile-orientation.jpg").apply { parentFile!!.mkdirs() }
        val picture = Bitmap.createBitmap(1600, 800, Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.rgb(49, 91, 64)) }
        val jpg = java.io.ByteArrayOutputStream().use { picture.compress(Bitmap.CompressFormat.JPEG, 90, it); it.toByteArray() }
        picture.recycle()
        val exif = "ffe1002245786966000049492a0008000000010012010300010000000600000000000000".chunked(2).map { it.toInt(16).toByte() }.toByteArray()
        source.writeBytes(jpg.take(2).toByteArray() + exif + jpg.drop(2).toByteArray())
        val normalized = net.dmsg.android.platform.ProfileImages.prepare(source)
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeByteArray(normalized, 0, normalized.size, bounds)
        assertEquals(256, bounds.outWidth); assertEquals(512, bounds.outHeight)
        assertTrue(normalized.size <= 512 * 1024); normalized.fill(0)
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", source)
        compose.runOnIdle { assertTrue(client.profile.beginPhoto()); client.profile.photoResult(uri) }
        compose.waitUntil(10_000) { client.profile.bitmap.value != null && client.profile.form.value.avatarChanged }
        val cached = runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.cachedDisplay("profile-editor", "").json) as kotlinx.serialization.json.JsonObject }
        val savedProfile = cached["data"] as kotlinx.serialization.json.JsonObject
        assertEquals("Mina's workspace", (savedProfile["name"] as kotlinx.serialization.json.JsonPrimitive).content)
        assertEquals(kotlinx.serialization.json.JsonNull, savedProfile["avatar_url"])
        compose.onNodeWithText("Public avatar", substring = false).performScrollTo().assertExists()
        capture("qa-profile-local-avatar")
        compose.runOnIdle { client.navigation.back(); client.navigation.push(net.dmsg.android.core.Route("profile")) }
        compose.onNodeWithText("Reviewed local name").assertExists()
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("display-cache")) }
        compose.waitUntil(10_000) { (client.accountWorkspace.cache.value()?.bytes ?: 0u) > 0u }
        compose.onNodeWithText("Clear cached data").performClick()
        compose.onNodeWithText("Clear", substring = false).performClick()
        compose.waitUntil(10_000) { client.accountWorkspace.cache.value()?.bytes == 0uL }
        assertEquals(file.file.id, runBlocking { net.dmsg.android.core.parseFileDetails(kotlinx.serialization.json.Json.parseToJsonElement(native.fileDetails(file.file.id).json)) }.file.id)
        source.delete()
    }

    @Test fun removingLocalDataRequiresTheActualRiskListAndConfirmation() {
        runBlocking { native.seedUiFileHistory() }
        val account = client.state.value.session!!.account!!
        compose.runOnIdle { client.navigation.open("account"); client.navigation.push(net.dmsg.android.core.Route("account-remove")) }
        compose.waitUntil(10_000) { client.accountWorkspace.local.value()?.unbacked == 2uL }
        compose.onNodeWithText("Unbacked file versions").assertExists()
        capture("qa-local-removal-risk")
        compose.onNodeWithText("Remove this account's local data").performScrollTo().performClick()
        compose.onNodeWithText("Remove local data permanently?").assertExists()
        compose.onNodeWithText("Cancel").performClick()
        assertTrue(client.state.value.unlocked)
        compose.onNodeWithText("Remove this account's local data").performClick()
        compose.onNode(hasText("Remove local data", substring = false) and hasClickAction()).performClick()
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        val scoped = native.scoped(runBlocking { native.sessionState() }.generation)
        val accounts = try { runBlocking { net.dmsg.android.core.parseSavedAccounts(kotlinx.serialization.json.Json.parseToJsonElement(scoped.accounts().json)) } } finally { scoped.destroy() }
        assertTrue(accounts.none { it.id == account })
        assertFalse(client.accountNames.value.containsKey(account))
    }

    @Test fun anUnpostedInvitationCanBeFoundAfterTheUiFormIsLost() {
        val channel = "ab".repeat(32)
        compose.runOnIdle { client.navigation.open("messages"); client.channels.editInvite(channel, net.dmsg.android.core.ChannelInvite("040g2082040g2082040g")); client.navigation.push(net.dmsg.android.core.Route("channel-invite", channel = channel)) }
        compose.onNodeWithText("Create invitation").performScrollTo().performClick()
        compose.waitUntil(15_000) { !client.state.value.working("channel:invite:" + channel) && client.channels.invitationAttempts.value()?.items?.size == 1 }
        val original = client.channels.invitationAttempts.value()!!.items.single()
        compose.runOnIdle { client.channels.clear(); client.channels.loadInvites(channel) }
        compose.waitUntil(10_000) { client.channels.invitationAttempts.value()?.items?.size == 1 }
        compose.onNodeWithText("Continue original invitation").performScrollTo().performClick()
        compose.waitUntil(15_000) { !client.state.value.working("channel:invite:" + channel) }
        assertEquals(original.request, client.channels.invites.value[channel]?.request)
        assertEquals(listOf(original), client.channels.invitationAttempts.value()!!.items)
    }

    @Test fun localBlocksApplyEvenWhenOnlineSynchronizationFailsAndUnblockIsExplicit() {
        val target = "040g2082040g2082040g"
        compose.runOnIdle { client.navigation.open("messages"); client.safety.openBlock(target) }
        compose.onNodeWithText("Block account").performScrollTo().performClick()
        compose.onNodeWithText("Block this account?").assertExists()
        compose.onNodeWithText("Confirm").performClick()
        compose.waitUntil(10_000) { client.safety.blocked.value()?.contains(target) == true }
        assertTrue(runBlocking { native.blockedAccounts() }.contains(target))
        compose.waitUntil(15_000) { client.safety.blockSyncError.value != null }
        compose.onNodeWithText("Unblock").performScrollTo().assertExists()
        capture("qa-local-block-offline")
        compose.onNodeWithText("Unblock").performClick()
        compose.onNodeWithText("Unblock this account?").assertExists()
        compose.onNodeWithText("Cancel").performClick()
        assertTrue(runBlocking { native.blockedAccounts() }.contains(target))
        compose.onNodeWithText("Unblock").performClick()
        compose.onNodeWithText("Confirm").performClick()
        compose.waitUntil(10_000) { client.safety.blocked.value()?.isEmpty() == true }
        assertTrue(runBlocking { native.blockedAccounts() }.isEmpty())
    }

    @Test fun reportingReviewsOnlyChosenEvidenceAndOfflineRetriesKeepTheSameReport() {
        val channel = "ab".repeat(32)
        runBlocking { native.seedUiReceivedFile() }
        val account = client.state.value.session!!.account!!
        compose.runOnIdle { client.navigation.open("messages"); client.safety.openReport(net.dmsg.android.core.ReportTarget("channel", account, channel, 1u)) }
        compose.onNodeWithText("Describe the issue").performTextInput("Only this reviewed explanation")
        InstrumentationRegistry.getInstrumentation().uiAutomation.waitForIdle(200, 5000)
        compose.onNodeWithText("Review report").assertIsDisplayed().performClick()
        compose.waitUntil(10_000) { client.safety.report.value() != null }
        val original = client.safety.report.value()!!.report.id
        assertNull(client.safety.report.value()!!.disclosedText)
        compose.onNodeWithText("No message text is included.").assertExists()
        compose.onNodeWithText("A file already saved on this device").assertDoesNotExist()
        capture("qa-report-disclosure-review")
        compose.onNodeWithText("Submit this report").performScrollTo().performClick()
        compose.waitUntil(15_000) { !client.state.value.working("safety:report:submit:$original") && client.safety.submissionErrors.value[original] != null }
        assertEquals("pending", client.safety.report.value()!!.report.state)
        compose.runOnIdle { client.clearNotice() }
        compose.onNodeWithText("Submit this report").performScrollTo().performClick()
        compose.waitUntil(15_000) { !client.state.value.working("safety:report:submit:$original") }
        val reports = runBlocking { net.dmsg.android.core.parseContentReports(kotlinx.serialization.json.Json.parseToJsonElement(native.reportHistory(false).json)) }
        assertEquals(listOf(original), reports.map { it.id })
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("reports", id = original)) }
        compose.waitUntil(10_000) { client.safety.report.value()?.report?.id == original }
        compose.onNodeWithText("Only this reviewed explanation").assertExists()
        compose.onNodeWithText("No message text is included.").assertExists()
        assertFalse(client.safety.report.value()!!.report.state in setOf("received", "reviewed"))
    }

    @Test fun channelSettingsKeepNameDraftsAndMuteDoesNotPublishAnUnfinishedRename() {
        val channel = "ab".repeat(32)
        runBlocking { native.seedUiChannelMembers("owner") }
        compose.runOnIdle { client.navigation.open("messages"); client.navigation.push(net.dmsg.android.core.Route("channel-settings", channel = channel)) }
        compose.waitUntil(10_000) { client.channels.review.value()?.details?.members?.size == 3 }
        compose.onNodeWithText("Conversation name").performTextReplacement("Name still being edited")
        compose.onNodeWithText("15 minutes").performClick()
        compose.waitUntil(10_000) { client.channels.review.value()?.details?.muted == true }
        assertEquals("test channel", client.channels.review.value()!!.details.name)
        compose.onNodeWithText("Name still being edited").assertExists()
        assertEquals("Name still being edited", client.channels.names.value[channel])
        compose.onNodeWithText("Save name").performClick()
        compose.waitUntil(10_000) { client.channels.review.value()?.details?.name == "Name still being edited" }
        // The lazy member row may not be composed while the name editor/IME is visible.
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("040g2082040g2082040g"))
        compose.onNodeWithText("040g2082040g2082040g").assertIsDisplayed()
        compose.waitUntil(10_000) { client.participants.people.value["040g2082040g2082040g"]?.name == "Mina" }
        capture("qa-channel-member-identity")
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Archive conversation"))
        compose.onNodeWithText("Archive conversation").performClick()
        compose.onNodeWithText("Archive this conversation?").assertExists()
        compose.onNodeWithText("Confirm change").assertExists()
        capture("qa-channel-archive-review")
        compose.onNodeWithText("Cancel").performClick()
        assertTrue(runBlocking { kotlinx.serialization.json.Json.parseToJsonElement(native.pendingChannelOperations(channel).json) as kotlinx.serialization.json.JsonArray }.isEmpty())
        compose.runOnIdle { client.channels.hide(channel, true) }
        compose.waitUntil(10_000) { client.channels.review.value()?.details?.hidden == true }
        assertTrue(runBlocking { native.channels() }.items.none { it.id == channel })
        compose.runOnIdle { client.navigation.push(net.dmsg.android.core.Route("channel-hidden")) }
        compose.waitUntil(10_000) { client.channels.hidden.value()?.singleOrNull()?.id == channel }
        compose.onNodeWithText("Show again").performClick()
        compose.waitUntil(10_000) { client.channels.hidden.value()?.isEmpty() == true }
        assertTrue(runBlocking { native.channels() }.items.any { it.id == channel })
    }

    @Test fun channelMemberCannotReachOwnerActionsAndInviteIsReviewedBeforeJoining() {
        val channel = "ab".repeat(32)
        runBlocking { native.seedUiChannelMembers("member") }
        compose.runOnIdle { client.navigation.open("messages"); client.navigation.push(net.dmsg.android.core.Route("channel-settings", channel = channel)) }
        compose.waitUntil(10_000) { client.channels.review.value()?.details?.members?.size == 3 }
        compose.onNodeWithText("Leave conversation").performScrollTo().assertExists()
        compose.onNodeWithText("Archive conversation").assertDoesNotExist()
        compose.onNodeWithText("Update conversation keys").assertDoesNotExist()
        compose.onNodeWithText("Remove member").assertDoesNotExist()
        val invitation = """{"format":"dmsg-channel-invitation/1","channel":"${"ab".repeat(32)}","genesis":"${"bc".repeat(32)}","head":"${"cd".repeat(32)}","invitation":"${"de".repeat(32)}","account":"040g2082040g2082040g","name":"Invitation for Mina","expires_at":${System.currentTimeMillis() + 60_000}}"""
        val link = "dmsg://channel/join#" + java.util.Base64.getUrlEncoder().withoutPadding().encodeToString(invitation.toByteArray())
        compose.runOnIdle { client.acceptCallback(link) }
        compose.onNodeWithText("Invitation for Mina").performScrollTo().assertExists()
        compose.onNodeWithText("Switch to the invited account before joining.").performScrollTo().assertExists()
        compose.onNodeWithText("Join conversation").performScrollTo().assertIsNotEnabled()
        assertFalse(client.state.value.working("channel:setup"))
        capture("qa-channel-invitation-account-review")
    }

    @Test fun offlineChannelCreationKeepsItsOriginalSealedAttempt() {
        compose.runOnIdle { client.navigation.open("messages"); client.channels.openSetup(false) }
        compose.onNodeWithText("Conversation name").performTextInput("Original creation attempt")
        compose.onNodeWithText("Announcements").performClick()
        compose.onNodeWithText("Create conversation").performScrollTo().assertIsDisplayed().assertIsEnabled().performClick()
        try { compose.waitUntil(15_000) { !client.state.value.working("channel:setup") && client.channels.creations.value()?.size == 1 } }
        catch (error: Throwable) {
            capturePermissionFailure("qa-channel-creation-failure")
            throw AssertionError("Creation setup=${client.channels.setup.value}; operations=${client.state.value.operations}; creations=${client.channels.creations.state.value}", error)
        }
        val original = client.channels.creations.value()!!.single()
        compose.onNodeWithText("Retry original creation").performScrollTo().performClick()
        compose.waitUntil(15_000) { !client.state.value.working("channel:setup") }
        assertEquals(original, client.channels.creations.value()!!.single())
        assertEquals("distribution", original.kind)
        capture("qa-channel-original-creation-retry")
        compose.onNodeWithText("Discard if not yet signed").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.channels.creations.value()?.isEmpty() == true }
        assertNull(client.channels.setup.value.request)
    }

    @Test fun folderNavigationAndComposerRetainTheAuthoredInput() {
        compose.onAllNodesWithText("Files").onLast().performClick()
        compose.waitUntil(10_000) { client.library.listing.value()?.folders?.isNotEmpty() == true }
        compose.onNodeWithText("Project materials").performClick()
        compose.waitUntil(10_000) { client.library.folder != "root" }
        compose.onNodeWithContentDescription("All files").performClick()
        compose.waitUntil(10_000) { client.library.folder == "root" }
        assertFalse(client.navigation.state.value.canGoBack)
        compose.onNodeWithText("Project materials").assertExists()
        compose.onAllNodesWithText("Messages").onLast().performClick()
        compose.waitUntil(10_000) { client.messages.channels.value()?.isNotEmpty() == true }
        compose.onNodeWithText("test channel").performClick()
        compose.onNodeWithText("Continue the conversation").assertExists()
        compose.runOnIdle { client.messages.edit("ab".repeat(32), client.messages.composer("ab".repeat(32)).copy(text = "Still here")) }
        compose.runOnIdle { client.navigation.back() }
        compose.onNodeWithText("test channel").performClick()
        compose.onNodeWithText("Still here").assertExists()
        capture("qa-conversation")
    }

    @Test fun fieldReferencesAndRotationKeepSecretsConcealedAndRequireAChangedRevision() {
        compose.onNodeWithText("Studio account").performClick()
        compose.waitUntil(10_000) { client.vault.detail.state.value is ReadState.Ready }
        val item = client.vault.detail.value()!!
        compose.onNodeWithContentDescription("Field actions").performClick()
        compose.onNodeWithText("Field reference and rotation").performClick()
        compose.waitUntil(10_000) { client.secretTools.reference.state.value is ReadState.Ready }
        compose.onNodeWithText("Sample-only password").assertDoesNotExist()
        compose.onNodeWithText("No dependencies recorded on this device.").assertExists()
        capture("qa-secret-reference")
        compose.onNodeWithText("Start a rotation checklist").performClick()
        compose.onNodeWithText("Start checklist").performClick()
        compose.waitUntil(10_000) { client.secretTools.rotation.state.value is ReadState.Ready }
        val rotation = client.secretTools.rotation.value()!!.rotation
        // No provider action has happened. The core refuses the owner's first step
        // until the actual field has changed, even if another metadata revision exists.
        assertTrue(runCatching { runBlocking { native.secretRotationUpdate(rotation.id, "created", null) } }.isFailure)
        assertFalse(client.secretTools.rotation.value()!!.rotation.created)
        val changed = runBlocking {
            native.saveItem(item.id, item.revision, VaultEdit(item.title, item.kind, item.notes, item.favorite, item.tags,
                item.fields.map { FieldEdit(it.id, it.kind, it.label, it.section, if (it.id == "password") "Rotated sample-only password" else null) }, item.files))
        }
        runBlocking { native.secretRotationUpdate(rotation.id, "created", null); native.secretRotationUpdate(rotation.id, "tested", null) }
        compose.runOnIdle { client.secretTools.rotation(rotation.id) }
        compose.waitUntil(10_000) { client.secretTools.rotation.value()?.rotation?.tested == true }
        assertEquals(changed.item.revision, client.secretTools.rotation.value()!!.rotation.next)
        assertFalse(client.secretTools.rotation.value()!!.rotation.referencesChecked)
        capture("qa-secret-rotation")
        compose.runOnIdle { client.navigation.open("secret-health") }
        compose.waitUntil(10_000) { client.secretTools.health.state.value is ReadState.Ready }
        assertEquals(rotation.id, client.secretTools.health.value()!!.rotations.single().id)
        compose.onNodeWithText("Rotated sample-only password").assertDoesNotExist()
        capture("qa-secret-health")
    }

    @Test fun selectedSnapshotPreparationStaysLocalAndReusesItsOriginalApproval() {
        compose.onNodeWithText("Studio account").performClick()
        compose.waitUntil(10_000) { client.vault.detail.state.value is ReadState.Ready }
        val item = client.vault.detail.value()!!
        compose.onNodeWithText("Share secret snapshot").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.secretSharing.draft.value != null }
        assertFalse(client.secretSharing.draft.value!!.selected)
        val label = item.fields.first { it.id == "password" }.label.ifBlank { net.dmsg.android.platform.str(R.string.text_password_347d7fc) }
        compose.onNodeWithText(label, substring = false).performClick()
        assertEquals(setOf("password"), client.secretSharing.draft.value!!.fields)
        assertFalse(client.secretSharing.draft.value!!.notes)
        capture("qa-secret-compose")
        compose.onNodeWithText("Prepare selected snapshot").performScrollTo().performClick()
        compose.waitUntil(15_000) { client.secretSharing.job.state.value is ReadState.Ready }
        val job = client.secretSharing.job.value()!!
        assertEquals("prepared", job.state)
        assertNull(job.request)
        assertNull(client.secretSharing.link.value)
        compose.onNodeWithText("Review publication or resume").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.approvals.detail.state.value is ReadState.Ready }
        val approval = client.approvals.detail.value()!!
        assertEquals("awaiting_user", approval.state)
        assertEquals(listOf("password"), approval.secret!!.fields.map { it.id })
        assertFalse(approval.secret!!.notes)
        compose.onNodeWithText("Sample-only password").assertDoesNotExist()
        capture("qa-secret-approval")
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithText("Approve").assertIsEnabled(); true }.getOrDefault(false) }
        compose.onNodeWithText("Approve").performClick()
        compose.onNodeWithText("Approve the reviewed request?").assertExists()
        compose.onNodeWithText("Anyone with the link").assertExists()
        compose.onNodeWithText(approval.digest).assertExists()
        val idBounds = compose.onNodeWithText(approval.id).fetchSemanticsNode().boundsInRoot
        val digestBounds = compose.onNodeWithText(approval.digest).fetchSemanticsNode().boundsInRoot
        assertTrue("Reviewed request and digest must not overlap", idBounds.bottom <= digestBounds.top)
        capture("qa-approval-final-review")
        compose.onNodeWithText("Cancel").performClick()
        assertEquals("awaiting_user", net.dmsg.android.core.parseApproval(kotlinx.serialization.json.Json.parseToJsonElement(runBlocking { native.approval(approval.id).json })).state)
        val original = runBlocking { native.secretShareRequest(job.id).json }
        assertTrue(original.contains(approval.id))
        val unchanged = runBlocking { native.secretShare(job.id, false).json }
        assertTrue(unchanged.contains("awaiting_user"))
        assertFalse(unchanged.contains("Sample-only password"))
        // Approval has not been accepted: no cloud upload/publication is attempted.
        compose.runOnIdle { client.navigation.back(); client.navigation.back() }
        compose.onNodeWithText("Open prepared snapshot").performScrollTo().performClick()
        compose.waitUntil(10_000) { client.secretSharing.job.value()?.id == job.id }
        assertEquals(job.id, client.secretSharing.draft.value!!.prepared)
    }

    @Test fun externalPickerResultWaitsForForegroundAndCannotCrossLock() {
        var deliveries = 0
        val stamp = client.currentStamp()!!
        compose.runOnIdle {
            client.beginExternalInteraction(); client.background()
            client.deliverExternalResult("test-picker", stamp) { deliveries += 1 }
        }
        compose.waitForIdle()
        assertEquals(0, deliveries)
        compose.runOnIdle { client.foreground() }
        compose.waitUntil(10_000) { deliveries == 1 }
        compose.runOnIdle { client.lock() }
        compose.waitUntil(10_000) { !client.state.value.unlocked && !client.state.value.busy }
        compose.runOnIdle { client.deliverExternalResult("late-picker", stamp) { deliveries += 1 } }
        compose.waitUntil(10_000) { !client.state.value.operations.contains("external-result:late-picker") }
        assertEquals(1, deliveries)
    }

    @Test fun passwordImportChecksTheStagedDigestAndWritesOnlySelectedRecords() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(app.cacheDir, "dmsg-shared/import-fixture/" + UUID.randomUUID() + ".csv")
        source.parentFile!!.mkdirs()
        source.writeText("name,url,username,password\nChosen record,https://example.test,reader,import-only secret\nExcluded record,https://example.net,other,not imported\n")
        val uri = androidx.core.content.FileProvider.getUriForFile(app, app.packageName + ".files", source)
        compose.runOnIdle { client.navigation.open("vault-import"); assertTrue(client.vaultImport.begin()); client.vaultImport.result(uri) }
        compose.waitUntil(15_000) { client.vaultImport.preview.state.value is ReadState.Ready }
        val preview = client.vaultImport.preview.value()!!
        assertEquals(2, preview.items.size)
        assertTrue(client.vaultImport.form.value!!.selected.isEmpty())
        compose.onNodeWithText("import-only secret").assertDoesNotExist()
        compose.onNodeWithText("Chosen record").performScrollTo().performClick()
        val handle = client.vaultImport.form.value!!.handle
        val staged = File(stagingDirectory, handle)
        val original = staged.readBytes()
        staged.appendText("\n")
        net.dmsg.core.OperationToken().use { token ->
            assertTrue(runCatching { runBlocking { native.vaultImportSelected(handle, preview.digest, listOf(0u), token) } }.isFailure)
        }
        staged.writeBytes(original); original.fill(0)
        capture("qa-password-import")
        compose.onNodeWithText("Import selected records").assertIsDisplayed().performClick()
        compose.waitUntil(15_000) { client.vaultImport.form.value?.completed != null }
        assertEquals(1, client.vaultImport.form.value!!.completed!!.size)
        val items = runBlocking { native.vaultPage("", null, false, 0u, 100u).items }
        assertTrue(items.any { it.title == "Chosen record" })
        assertFalse(items.any { it.title == "Excluded record" })
        compose.waitUntil(10_000) { !staged.exists() }
        source.delete()
    }
}
