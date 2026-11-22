package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class PlayModelsTest {
    @Test fun accountBindingMatchesPublishedRustVectorAndProductsUseGovernanceNotSkuNames() {
        val owner=PlayAccount("f7crg-kabae","0c1g60o30c1g60o30c1g","28292fa7cbf3c3be83c07d8aa35cf67946d6becd710d2d3b70ace7386639f75f")
        owner.validate()
        val value=Json.parseToJsonElement("""{"home":"${owner.home}","account":"${owner.account}","account_token":"${owner.binding}","configuration":{"package_name":"net.dmsg.android","environment":"Production","accept_test":false,"products":[{"product_id":"plus","offer_id":null,"enabled":true,"kind":{"Plan":{"plan":"Max","base_plan_id":"annual"}}}]}}""")
        val catalog=parsePlayCatalog(value)!!
        assertEquals("Max",catalog.products.single().plan);assertEquals("plus",catalog.products.single().product)
        assertThrows(IllegalStateException::class.java) { owner.copy(account="0".repeat(20)).validate() }
    }
    @Test fun receiptPaginationPreservesExactLargeTimesAndRejectsFalseCompletion() {
        val id="11".repeat(32)
        val json="""{"items":[{"token_hash":"$id","product_id":"plus","kind":"subscription","environment":"Test","state":"Pending","expires_at_ms":9007199254740993,"delivered":false,"pending":true,"confirmation_pending":false,"next_product":null,"error":null}],"next_cursor":"$id","end":false}"""
        val page=parsePlayReceipts(Json.parseToJsonElement(json))
        assertEquals(9007199254740993uL,page.items.single().expires);assertFalse(page.end)
        assertThrows(IllegalStateException::class.java) { parsePlayReceipts(Json.parseToJsonElement(json.replace("\"end\":false","\"end\":true"))) }
        assertThrows(IllegalStateException::class.java) { parsePlayReceipts(Json.parseToJsonElement(json.replace("\"delivered\":false","\"delivered\":true"))) }
    }
    @Test fun playReplacementAppliesOnlyToActiveGooglePlaySubscriptions() {
        val pro = PlaySelection("pro", "annual", null, PlayKind.SUBSCRIPTION, "Pro", null, true)
        val plus = PlaySelection("plus", "annual", null, PlayKind.SUBSCRIPTION, "Plus", null, true)
        val existing = PlayPurchase("tok-plus", "plus", PlayKind.SUBSCRIPTION, null, null, false, 1L, "GPA.1")
        for (method in listOf("Promo", "Compensation")) {
            val current = Json.parseToJsonElement("""{"method":"$method","plan":"Plus","store_token_hash":null}""").objectValue()
            assertNull(playReplacement(plus, current, emptyList()))
            assertNull(playReplacement(pro, current, emptyList()))
        }
        val playCurrent = Json.parseToJsonElement("""{"method":"GooglePlay","plan":"Plus","store_token_hash":"${existing.id}"}""").objectValue()
        val upgrade = playReplacement(pro, playCurrent, listOf(existing))!!
        assertEquals(existing.id, upgrade.purchase.id)
        assertFalse(upgrade.deferred)
        assertThrows(IllegalStateException::class.java) { playReplacement(plus, playCurrent, listOf(existing)) }
    }
}
