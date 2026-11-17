package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class WalletModelsTest {
    @Test fun onlyFreshUnblockedPreviewsCanBeSubmitted() {
        val preview = WalletTransferPreview("10000", "10000", "ICP", 8, "200000000", "200000000", "0", 2u, false, 0u, 1000u, emptyList())
        assertFalse(preview.canSubmit(999u)); assertTrue(preview.canSubmit(1000u)); assertTrue(preview.canSubmit(61_000u)); assertFalse(preview.canSubmit(61_001u))
        assertFalse(preview.copy(blockers = listOf("INSUFFICIENT_ASSET_BALANCE")).canSubmit(1000u))
    }
    @Test fun atomicAmountsNeverRoundAndRespectAssetPrecisionAndUint128() {
        assertEquals("100000001", AtomicAmount.parse("1.00000001", 8, '.'))
        assertEquals("123400", AtomicAmount.parse("١٢,٣٤", 4, ','))
        assertEquals("9007199254740993", AtomicAmount.parse("9007199254740993", 0, '.'))
        val max = "340282366920938463463374607431768211455"
        assertEquals(max, AtomicAmount.parse(max, 0, '.'))
        assertNull(AtomicAmount.parse("340282366920938463463374607431768211456", 0, '.'))
        for (value in listOf("-1", "+1", "1e8", "1.", ".1", "1,000", "1.2.3", "0.000000001")) assertNull(AtomicAmount.parse(value, 8, '.'))
        assertEquals("1.00000001", AtomicAmount.display("100000001", 8))
        assertEquals("0.00000001", AtomicAmount.display("1", 8))
    }
    @Test fun localizedAmountsRetainAllAtomsAcrossDecimalScripts() {
        val maximum = "340282366920938463463374607431768211455"
        for (tag in listOf("en", "de", "ar-EG", "bn", "fa")) {
            val locale = java.util.Locale.forLanguageTag(tag)
            val separator = java.text.DecimalFormatSymbols.getInstance(locale).decimalSeparator
            for (decimals in listOf(0, 8, 18, 38)) {
                val display = AtomicAmount.display(maximum, decimals, locale)
                assertEquals(tag, maximum, AtomicAmount.parse(display, decimals, separator))
            }
        }
        assertEquals("1,00000001", AtomicAmount.display("100000001", 8, java.util.Locale.GERMAN))
    }
    @Test fun onlyDefiniteAbsenceOrRejectionAllowsANewProposalIdentity() {
        val request = WalletSubmission("ab".repeat(32), attempted = true)
        for (state in listOf(null, "pending", "completed", "unknown")) {
            val retained = request.failed(state, "EXECUTION_UNKNOWN")
            assertEquals(request.request, retained.request); assertTrue(retained.attempted); assertNull(retained.result)
        }
        for (state in listOf("absent", "rejected")) {
            val editable = request.failed(state, "FORBIDDEN")
            assertNotEquals(request.request, editable.request); assertFalse(editable.attempted)
        }
    }
    @Test fun unknownBalancesAndMissingPrecisionAreNotZero() {
        val value = parseWalletBalances(Json.parseToJsonElement("""{"wallet":"aaaaa-aa","read_at":9007199254740993,"addresses":[{"address":0,"text":"account","balances":[{"key":"icp","asset":"ICP","balance":{"Err":"RPC unavailable"}},{"key":"btc","asset":"BTC","balance":{"Ok":"9007199254740993"},"metadata":{"key":"btc","symbol":"BTC","decimals":8}}]}]}"""))
        assertNull(value.values[0].atoms); assertNull(value.values[0].asset.decimals); assertEquals("RPC unavailable", value.values[0].error)
        assertEquals("9007199254740993", value.values[1].atoms)
        assertTrue(runCatching { parseWalletBalances(Json.parseToJsonElement("""{"wallet":"aaaaa-aa","read_at":0,"addresses":[{"address":0,"text":"account","balances":[{"key":"icp","balance":{}}]}]}""")) }.isFailure)
    }
    @Test fun walletRolesAndNativeMoveFieldsRemainExplicit() {
        val value = parseWalletDetails(Json.parseToJsonElement("""{"wallet":"aaaaa-aa","name":"Team","frozen":false,"me":{"id":"owner","role":"Viewer","status":"Active"},"members":[],"addresses":[],"cosigners":[],"policy":{"approver_threshold":2,"admin_threshold":2}}"""))
        assertFalse(value.canPropose); assertFalse(value.canDecide)
        assertTrue(value.copy(me = value.me!!.copy(role = "Admin")).canDecide)
        assertFalse(value.copy(me = value.me!!.copy(role = "Admin", status = "Invited")).canDecide)
        val root = parseNativeWalletKeys(Json.parseToJsonElement("""{"created":false,"protection":null,"hardware":null,"key":null,"protectors":[]}"""))
        assertFalse(root.created); assertNull(root.hardware)
        assertTrue(runCatching { parseWalletAsset(Json.parseToJsonElement("""{"key":"btc","decimals":39}""").objectValue()) }.isFailure)
    }
}
