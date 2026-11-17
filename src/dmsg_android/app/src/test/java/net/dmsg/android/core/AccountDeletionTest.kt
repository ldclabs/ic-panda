package net.dmsg.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class AccountDeletionTest {
    @Test fun deletionRequiresExactIdentityAndEveryDependencyReview() {
        val account = "040g2081040g2081040g"
        val review = AccountDeletionPreview("ab".repeat(32), account, 3u, false, emptyList(), emptyList(), true)
        assertTrue(review.canRequest(account, false))
        assertFalse(review.canRequest(account + " ", true))
        assertFalse(review.copy(pendingOperation = true).canRequest(account, true))
        assertFalse(review.copy(channels = listOf(AccountDeletionChannel("cd".repeat(32), "Owned"))).canRequest(account, true))
        assertFalse(review.copy(walletsAvailable = false).canRequest(account, false))
        assertTrue(review.copy(walletsAvailable = false).canRequest(account, true))
        assertFalse(review.copy(wallets = listOf(WalletSummary("aaaaa-aa", "Wallet"))).canRequest(account, false))
    }
    @Test fun unknownStatusAndMissingDependenciesCannotMeanDeleted() {
        assertNull(parseAccountDeletionStatus(Json.parseToJsonElement("""{"deletion":null}""")).deletion)
        assertEquals("purging", parseAccountDeletionStatus(Json.parseToJsonElement("""{"deletion":{"id":"original","state":"purging","requested_at":9007199254740993}}""")).deletion!!.state)
        assertTrue(runCatching { parseAccountDeletionStatus(Json.parseToJsonElement("{}")) }.isFailure)
        assertTrue(runCatching { parseAccountDeletionPreview(Json.parseToJsonElement("""{"digest":"ab","details":{}}""")) }.isFailure)
    }
}
