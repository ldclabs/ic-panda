package net.dmsg.android.platform

import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.security.SecureRandom

class SystemExportStoreTest {
    @get:Rule val temporary = TemporaryFolder()
    private class TestKeys : ExportKeys {
        private val keys = mutableMapOf<String, ByteArray>()
        override fun create(header: ExportHeader): ExportWrappedKey {
            val key = ByteArray(32).also(SecureRandom()::nextBytes); keys[header.id] = key
            return ExportWrappedKey(key.copyOf(), byteArrayOf(1, 2, 3))
        }
        override fun open(header: ExportHeader, valid: () -> Boolean): ByteArray { check(valid()); return checkNotNull(keys[header.id]).copyOf() }
        override fun remove(header: ExportHeader) { keys.remove(header.id)?.fill(0) }
    }
    private val credential = ExportCredentialInput("Private title", "private-user", "private-password", "https://example.com")

    @Test fun aLoginCanBeFoundByItsWebsiteWhenItIsIneligibleForSiteTrust() {
        val http = AutofillTrust.displayOrigin("http://example.com/sign-in")
        val alternatePort = AutofillTrust.displayOrigin("https://account@example.net:8443/sign-in?token=fixture")
        assertEquals("http://example.com", http)
        assertEquals("https://example.net:8443", alternatePort)
        assertNull(AutofillTrust.host(http))
        assertNull(AutofillTrust.host(alternatePort))
    }

    @Test fun onlyAuthenticatedCurrentCopiesCanOpenAndTheirMetadataStaysEncrypted() {
        var time = 1000L; val keys = TestKeys(); val root = temporary.newFolder()
        val store = SystemExportStore(root, ExportPurpose.credentials, keys) { time }
        val prepared = store.prepareCredentials(listOf(credential), "private-account", true)
        store.publish(prepared); prepared.close()
        root.walkTopDown().filter(File::isFile).forEach { file ->
            val content = file.readBytes().toString(Charsets.ISO_8859_1)
            listOf(credential.title, credential.username, credential.password, credential.origin, "private-account").forEach { assertFalse(content.contains(it)) }
        }
        store.open().use { opened ->
            val login = store.credentials(opened).single()
            assertEquals(credential.title, login.title)
            assertEquals(credential.password, store.plain(opened, login.id).toString(Charsets.UTF_8))
            time += SystemExportStore.DAY
            assertTrue(runCatching { store.credentials(opened) }.isFailure)
        }
        assertTrue(runCatching { store.open() }.isFailure)
        time = 999
        assertTrue(runCatching { store.open() }.isFailure)
    }
    @Test fun rememberedAccountsSurviveRefreshAndStayScopedToTheServiceAndVault() {
        var now = 1000L
        val store = SystemExportStore(temporary.newFolder(), ExportPurpose.credentials, TestKeys()) { now }
        val items = (0..60).map { credential.copy(username = "user$it", itemId = "item$it") }
        store.prepareCredentials(items, "A", false, "first").use { store.publish(it) }
        store.open().use { opened ->
            listOf("item1", "item2", "item1").forEach { store.remember(it, "app:one", opened) }
        }
        now += 2 * SystemExportStore.DAY
        store.open().use { assertEquals(61, store.credentials(it).size) }
        store.prepareCredentials(items.reversed(), "A", false, "second").use { store.publish(it) }
        store.open().use { opened ->
            assertEquals(listOf("item1", "item2"), store.recentLogins(opened.header.account, "app:one"))
            assertTrue(store.recentLogins(opened.header.account, "app:two").isEmpty())
            assertTrue(store.recentLogins("B", "app:one").isEmpty())
            assertTrue(store.recentLogins(opened.header.account, null).isEmpty())
            assertEquals("item60", store.credentials(opened).first().itemId)
        }
        store.open().use { opened ->
            store.withdrawCredentials(opened.header)
            assertFalse(store.current(opened.header))
            assertEquals(2, store.recentLogins(opened.header.account, "app:one").size)
        }
        store.invalidate()
        assertTrue(store.recentLogins(SystemExportStore.digest("A".toByteArray()), "app:one").isEmpty())
    }

    @Test fun removingOrReplacingASelectionRejectsDelayedPublicationAndOldReferences() {
        val root = temporary.newFolder(); val keys = TestKeys()
        val store = SystemExportStore(root, ExportPurpose.credentials, keys)
        val first = store.prepareCredentials(listOf(credential), "A", false); store.publish(first)
        val old = store.open(); val oldLogin = store.credentials(old).single()
        val delayed = store.prepareCredentials(listOf(credential.copy(password = "new")), "A", false)
        SystemExportStore(root, ExportPurpose.credentials, keys).invalidate("A")
        assertTrue(runCatching { store.publish(delayed) }.isFailure)
        assertTrue(runCatching { store.plain(old, oldLogin.id) }.isFailure)
        assertNull(store.header())
        store.discard(delayed); old.close(); first.close()
        val second = store.prepareCredentials(listOf(credential), "B", true); store.publish(second); second.close()
        store.invalidate("A")
        assertNotNull(store.header())
        assertTrue(runCatching { SystemExportStore(root, ExportPurpose.files, keys).header() }.isFailure)
    }
    @Test fun fileCopiesAreBoundedVerifiedAndCannotAcceptAlteredChunks() {
        val root = temporary.newFolder(); val keys = TestKeys()
        val store = SystemExportStore(root, ExportPurpose.files, keys)
        val bytes = ByteArray(SystemExportStore.PART + 3) { (it % 251).toByte() }
        val source = temporary.newFile().apply { writeBytes(bytes) }
        val input = ExportFileInput(source, "Private file.bin", "application/octet-stream", bytes.size.toLong(), SystemExportStore.digest(bytes))
        assertTrue(runCatching { store.prepareFiles(listOf(input.copy(sha256 = "0".repeat(64))), "A", false) }.isFailure)
        assertNull(store.header())
        val prepared = store.prepareFiles(listOf(input), "A", false); store.publish(prepared); prepared.close()
        store.open().use { opened ->
            val file = store.files(opened).single(); assertEquals(2, file.parts)
            assertArrayEquals(bytes, store.plain(opened, file.id + ".0") + store.plain(opened, file.id + ".1"))
            val chunk = File(root, opened.header.id + "/" + file.id + ".1")
            chunk.writeBytes(chunk.readBytes().also { it[it.lastIndex] = (it.last() + 1).toByte() })
            assertTrue(runCatching { store.plain(opened, file.id + ".1") }.isFailure)
        }
    }
    @Test fun aFailedPublicationNeverChangesTheCurrentSnapshot() {
        val store = SystemExportStore(temporary.newFolder(), ExportPurpose.credentials, TestKeys())
        val old = store.prepareCredentials(listOf(credential), "A", false); store.publish(old)
        val next = store.prepareCredentials(listOf(credential.copy(password = "next")), "A", true)
        assertTrue(runCatching { store.publish(next) { false } }.isFailure)
        store.discard(next)
        assertEquals(old.opened.header.id, store.header()!!.id)
        store.open().use { assertEquals(credential.password, store.plain(it, store.credentials(it).single().id).toString(Charsets.UTF_8)) }
        old.close()
    }
}
