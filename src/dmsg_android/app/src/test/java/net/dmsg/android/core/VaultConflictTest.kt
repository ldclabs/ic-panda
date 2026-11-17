package net.dmsg.android.core

import org.junit.Assert.*
import org.junit.Test

class VaultConflictTest {
    @Test fun concealedDifferencesAreReviewableWithoutReturningEitherValue() {
        val field = ConflictField("password", "concealed", "Password", "Login", "first secret")
        val current = ConflictItem("item", "current", "Account", "", emptyList(), emptyList(), false, false, listOf(field))
        val copy = current.copy(revision = "other", fields = listOf(field.copy(value = "second secret")))
        val parts = conflictParts(VaultConflict(copy, current, listOf("password")))
        assertEquals(listOf("field:password"), parts.map { it.id })
        assertTrue(parts.single().hidden)
        assertFalse(parts.toString().contains("first secret")); assertFalse(parts.toString().contains("second secret"))
    }
    @Test fun fieldMetadataAndRemovalAreNotMistakenForIdenticalValues() {
        val field = ConflictField("site", "url", "Website", "Login", "https://example.test")
        val current = ConflictItem("item", "current", "Account", "", emptyList(), emptyList(), false, false, listOf(field))
        val copy = current.copy(revision = "other", fields = listOf(field.copy(label = "Production site")))
        assertEquals("field:site", conflictParts(VaultConflict(copy, current, emptyList())).single().id)
        assertEquals("—", conflictParts(VaultConflict(copy.copy(fields = emptyList()), current, emptyList())).single().other)
    }
}
