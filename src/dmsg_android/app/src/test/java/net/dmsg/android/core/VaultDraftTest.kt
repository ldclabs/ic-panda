package net.dmsg.android.core

import net.dmsg.core.VaultField
import org.junit.Assert.*
import org.junit.Test

class VaultDraftTest {
    @Test fun storedFormsRetainIncompleteInputHiddenFieldIntentAndBaseRevision() {
        val draft = VaultDraft("a".repeat(64), "b".repeat(64), "", "login", "unfinished notes", false, "typing tag,",
            listOf(DraftField("password", "concealed", "", "", "", false, true),
                DraftField("otp", "otp", "", "", "incomplete otp", true, true)), listOf("c".repeat(64)))
        val stored = kotlinx.serialization.json.Json.parseToJsonElement(vaultDraftForm(draft, draft))
            as kotlinx.serialization.json.JsonObject
        assertEquals(draft, parseVaultDraft(stored.getValue("draft")))
        assertEquals(draft, parseVaultDraft(stored.getValue("initial")))
        assertNull(parseVaultDraft(stored.getValue("draft")).fields.first().edit().value)
    }
    @Test fun preservingHiddenValueIsDifferentFromExplicitlyClearingIt() {
        val original = DraftField("password", "concealed", "", "", "", false, true)
        assertNull(original.edit().value)
        assertEquals("", original.copy(changed = true).edit().value)
        assertEquals("new", original.copy(changed = true, value = "new").edit().value)
    }
    @Test fun switchingTemplatesKeepsAuthoredFieldsAndReorderingKeepsStableIds() {
        val login = listOf(VaultField("username", "text", "", "", "", false), VaultField("password", "concealed", "", "", "", true))
        val draft = VaultDraft.from(null, login).field("username") { it.copy(value = "alice", changed = true) }
        val changed = draft.switchKind("api") { kind -> if (kind == "login") login else listOf(VaultField("token", "concealed", "", "", "", true)) }
        assertEquals(listOf("username", "token"), changed.fields.map { it.id })
        assertEquals("alice", changed.fields.first().value)
        assertEquals(listOf("token", "username"), changed.move("token", -1).fields.map { it.id })
    }
    @Test fun invalidDatesAndOtpInputsFailBeforeSavingAndPasswordsContainSelectedGroups() {
        val field = DraftField("date", "date", "", "", "2026-02-30", true, false)
        assertEquals("INVALID_DATE", validateDraftField(field))
        assertNull(validateDraftField(field.copy(value = "2028-02-29")))
        assertEquals("INVALID_OTP", validateDraftField(field.copy(kind = "otp")) { false })
        repeat(8) {
            val value = generatedPassword(20, true)
            assertEquals(20, value.length)
            assertTrue(value.any(Char::isUpperCase)); assertTrue(value.any(Char::isLowerCase)); assertTrue(value.any(Char::isDigit))
            assertTrue(value.any { it in "!@#$%&*-_+=?" })
        }
    }
}
