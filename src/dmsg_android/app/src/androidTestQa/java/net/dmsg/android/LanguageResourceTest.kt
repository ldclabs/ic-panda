package net.dmsg.android

import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.platform.AppLanguage
import net.dmsg.android.platform.localizedNumber
import net.dmsg.android.platform.quantity
import org.junit.Assert.*
import org.junit.Test
import java.math.BigInteger
import java.text.NumberFormat
import java.util.Locale

class LanguageResourceTest {
    @Test fun androidSpecificVocabularyUsesTheRequestedLanguage() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        // Exercise Android's compiled locale matching, including legacy id/in,
        // region-specific Portuguese and Chinese scripts, rather than JSON lookup.
        val confirmations = mapOf(
            "en" to "Confirm in dMsg", "zh-Hans" to "在 dMsg 中确认", "zh-Hant" to "在 dMsg 中確認",
            "ar" to "التأكيد في dMsg", "de" to "In dMsg bestätigen", "es" to "Confirmar en dMsg",
            "fr" to "Confirmer dans dMsg", "id" to "Konfirmasi di dMsg", "it" to "Conferma in dMsg",
            "ja" to "dMsg で確認", "ms" to "Sahkan dalam dMsg", "nl" to "Bevestigen in dMsg",
            "pt-BR" to "Confirmar no dMsg", "pl" to "Potwierdź w dMsg", "ru" to "Подтвердить в dMsg",
            "tr" to "dMsg'de onayla", "vi" to "Xác nhận trong dMsg", "ko" to "dMsg에서 확인",
            "fil" to "Kumpirmahin sa dMsg",
            "hi" to "dMsg में पुष्टि करें", "bn" to "dMsg-এ নিশ্চিত করুন",
            "ur" to "dMsg میں تصدیق کریں", "th" to "ยืนยันใน dMsg",
        )
        assertEquals(AppLanguage.supported.keys, confirmations.keys)
        for ((tag, expected) in confirmations) {
            val config = Configuration(app.resources.configuration).apply { setLocales(LocaleList.forLanguageTags(tag)) }
            assertEquals(tag, expected, app.createConfigurationContext(config).getString(R.string.security_confirm))
        }
    }

    @Test fun customFieldLabelsAreNeverTranslatedOrReplaced() {
        assertEquals("Username", net.dmsg.android.ui.vaultFieldTitle("username", "text", "Username"))
        assertEquals("API 密钥 / Primary", net.dmsg.android.ui.vaultFieldTitle("password", "concealed", "API 密钥 / Primary"))
    }

    @Test fun allSupportedLanguagesResolveSharedVocabularyAndCountFormats() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        assertEquals(23, AppLanguage.supported.size)
        for (tag in AppLanguage.supported.keys) {
            val locale = Locale.forLanguageTag(tag)
            val config = Configuration(app.resources.configuration).apply { setLocales(LocaleList(locale)) }
            val resources = app.createConfigurationContext(config).resources
            assertTrue(tag, resources.getString(R.string.shortcut_vault).isNotBlank())
            assertTrue(tag, resources.getString(R.string.notice_body_generic).isNotBlank())
            for (count in listOf(0, 1, 2, 3, 5, 11, 21, 101, 1_000_000)) {
                val number = NumberFormat.getIntegerInstance(locale).format(count)
                for (id in listOf(R.plurals.items, R.plurals.days, R.plurals.create_links, R.plurals.draft_attachments)) {
                    val value = resources.getQuantityString(id, count, number)
                    assertTrue("$tag / $count", number in value)
                    assertFalse(value, "%1\$s" in value)
                }
            }
        }
        assertTrue(quantity(R.plurals.items, ULong.MAX_VALUE).contains(localizedNumber(ULong.MAX_VALUE)))
        assertEquals(NumberFormat.getIntegerInstance(AppLanguage.locale()).format(BigInteger("18446744073709551615")), localizedNumber(ULong.MAX_VALUE))
    }

    @Test fun scriptSpecificChineseAndSlavicPluralFormsAreDistinct() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext
        fun resources(tag: String) = app.createConfigurationContext(Configuration(app.resources.configuration).apply { setLocales(LocaleList.forLanguageTags(tag)) }).resources
        assertEquals("账户", resources("zh-Hans").getString(R.string.text_account_4ab56e9))
        assertEquals("帳戶", resources("zh-Hant").getString(R.string.text_account_4ab56e9))
        assertEquals("保险库", resources("zh-Hans").getString(R.string.shortcut_vault))
        assertEquals("保險庫", resources("zh-Hant").getString(R.string.shortcut_vault))
        assertEquals("1 элемент", resources("ru").getQuantityString(R.plurals.items, 1, "1"))
        assertEquals("2 элемента", resources("ru").getQuantityString(R.plurals.items, 2, "2"))
        assertEquals("11 элементов", resources("ru").getQuantityString(R.plurals.items, 11, "11"))
        assertEquals("21 элемент", resources("ru").getQuantityString(R.plurals.items, 21, "21"))
    }
}
