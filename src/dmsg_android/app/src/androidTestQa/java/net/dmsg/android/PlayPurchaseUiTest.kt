package net.dmsg.android

import android.graphics.Bitmap
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.*
import net.dmsg.android.ui.DMsgTheme
import net.dmsg.android.ui.PlayPurchaseContent
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Presentation fixtures only. No Play account, SDK payment or entitlement is created. */
@RunWith(AndroidJUnit4::class)
class PlayPurchaseUiTest {
    @get:Rule val compose = createComposeRule()
    private val offer = PlayOffer(PlaySelection("example.plus","annual",null,PlayKind.SUBSCRIPTION,"Plus",null,true),"Sample annual subscription","Sample price for UI validation only.",listOf(PlayPricePhase("€12.00","P1Y",0,true)),System.currentTimeMillis())
    private fun capture(name:String) {
        compose.waitForIdle()
        val automation=InstrumentationRegistry.getInstrumentation().uiAutomation
        val bitmap=checkNotNull(automation.takeScreenshot())
        val directory=File(InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir(null),"play-qa").apply { mkdirs() }
        File(directory,"$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG,100,it) };bitmap.recycle()
    }
    @Test fun reviewShowsActualOfferPhasesAndCannotDoubleStartWhilePreparing() {
        var view by mutableStateOf(PurchaseView(beneficiary="040g2081040g2081040g",offers=listOf(offer),canBuy=true))
        var purchases=0
        compose.setContent { DMsgTheme(false) { Surface { Column(Modifier.safeDrawingPadding().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("UI fixture · no payment")
            PlayPurchaseContent(view,null,true,{view=view.copy(selected=it)},{},{},{view=view.copy(selected=null)}, {purchases++;view=view.copy(phase=PurchasePhase.PREPARING)}, {},{})
        } } } }
        compose.onNodeWithTag("play-offer-${offer.selection.id}").performScrollTo().performClick()
        compose.onNodeWithText("040g2081040g2081040g").assertExists()
        compose.onNodeWithText("Each year").assertIsDisplayed()
        compose.runOnIdle { view = view.copy(beneficiary = null) }
        compose.onNodeWithTag("play-buy").assertIsNotEnabled()
        compose.runOnIdle { view = view.copy(beneficiary = "040g2081040g2081040g", cancelled = true) }
        compose.onNodeWithText("Cancelled").assertExists()
        compose.onNodeWithTag("play-buy").performScrollTo().assertIsEnabled()
        capture("qa-play-review")
        compose.onNodeWithTag("play-buy").performClick()
        compose.onNodeWithTag("play-buy").assertIsNotEnabled()
        compose.onNodeWithText("1 · Preparing your purchase").assertExists()
        compose.runOnIdle { assertEquals(1,purchases);view=view.copy(phase=PurchasePhase.FAILED,error="PLAY_SERVICE_UNAVAILABLE",canBuy=false) }
        compose.onNodeWithText("Check the original purchase").performScrollTo().assertIsDisplayed()
        capture("qa-play-recovery")
        compose.onNodeWithText("Close").performClick()
        compose.onNodeWithTag("play-review").assertDoesNotExist()
    }
    @Test fun pendingDeliveryBlocksAnotherPurchaseAndShowsScheduledChangeWithoutEarlyTierChange() {
        val receipt=PlayReceipt("11".repeat(32),"example.plus",PlayKind.SUBSCRIPTION,"Test","Pending",null,false,true,false,"example.pro",null)
        var attempts=0
        var view by mutableStateOf(PurchaseView(PurchasePhase.PENDING,offers=listOf(offer),receipts=listOf(receipt),pending=1,canBuy=true,selected=offer))
        compose.setContent { DMsgTheme(true) { Surface { Column(Modifier.safeDrawingPadding().padding(20.dp).verticalScroll(rememberScrollState())) {
            Text("UI fixture · no payment")
            PlayPurchaseContent(view,null,true,{},{},{},{view=view.copy(selected=null)},{attempts++},{},{})
        } } } }
        compose.onNodeWithTag("play-buy").performScrollTo().assertIsNotEnabled()
        capture("qa-play-pending")
        compose.onNodeWithText("Close").performScrollTo().performClick()
        compose.onNodeWithText("Test purchase").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("A change is scheduled for the next term. Current benefits remain until then.").performScrollTo().assertIsDisplayed()
        compose.runOnIdle { assertEquals(0,attempts) }
    }
}
