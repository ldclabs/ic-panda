package net.dmsg.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import net.dmsg.android.core.*
import net.dmsg.android.platform.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.KeyStore
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class PlayPurchaseStorageTest {
    @Test fun platformReceiptKeySurvivesReopenAndDoesNotOpenAnAccountOrReplaceALostKey() {
        val app=InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        val initialized=app.hasMainClient
        val root=File(app.noBackupFilesDir,"play-qa-"+UUID.randomUUID())
        var alias:String?=null
        try {
            val home="f7crg-kabae";val id="0c1g60o30c1g60o30c1g"
            val account=PlayAccount(home,id,playDigest("dmsg/google-play/account/1\n$home\n$id"))
            val store=PlayPurchaseStore(root,PlayPurchaseKeys(app,root))
            alias=app.packageName+".play.receipts."+File(root,"installation").readText()
            val purchase=PlayPurchase("synthetic-keystore-purchase-token","example.plus",PlayKind.SUBSCRIPTION,account.binding,null,false,System.currentTimeMillis(),null)
            store.bind(account);store.enqueue(purchase)
            val file=File(root,"purchases.sealed")
            val sealedBytes=file.readBytes()
            assertFalse(sealedBytes.toString(Charsets.UTF_8).contains(purchase.token))
            val reopened=PlayPurchaseStore(root,PlayPurchaseKeys(app,root))
            assertEquals(purchase.token,reopened.pending(account).single().purchase.token)
            assertEquals(initialized,app.hasMainClient)
            val keys=KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            keys.deleteEntry(alias)
            assertTrue(runCatching { PlayPurchaseStore(root,PlayPurchaseKeys(app,root)).pending(account) }.isFailure)
            assertArrayEquals(sealedBytes,file.readBytes())
            assertFalse(keys.containsAlias(alias))
        } finally {
            alias?.let { KeyStore.getInstance("AndroidKeyStore").apply { load(null);deleteEntry(it) } }
            root.deleteRecursively()
        }
    }
}
