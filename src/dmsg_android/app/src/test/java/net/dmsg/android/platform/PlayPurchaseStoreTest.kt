package net.dmsg.android.platform

import net.dmsg.android.core.*
import org.junit.Assert.*
import org.junit.Test
import java.nio.file.Files
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

class PlayPurchaseStoreTest {
    private val cipher = object : PurchaseSealer {
        val key = SecretKeySpec(ByteArray(32) { 7 }, "AES")
        override fun seal(bytes: ByteArray): ByteArray = Cipher.getInstance("AES/GCM/NoPadding").run { init(Cipher.ENCRYPT_MODE,key); iv + doFinal(bytes) }
        override fun open(bytes: ByteArray): ByteArray = Cipher.getInstance("AES/GCM/NoPadding").run { init(Cipher.DECRYPT_MODE,key,GCMParameterSpec(128,bytes.copyOfRange(0,12))); doFinal(bytes,12,bytes.size-12) }
    }
    private fun account(value: String) = PlayAccount("f7crg-kabae", value, playDigest("dmsg/google-play/account/1\nf7crg-kabae\n$value"))
    private val a = account("0c1g60o30c1g60o30c1g")
    private val b = account("0".repeat(20))
    private fun purchase(binding: String? = a.binding) = PlayPurchase("secret-original-purchase-token", "configured.product", PlayKind.SUBSCRIPTION, binding, null, false, 1_800_000_000_000, "GPA.example..0")
    private fun receipt(p: PlayPurchase, delivered: Boolean, confirming: Boolean = false, error: String? = null) = PlayReceipt(p.id,p.product,p.kind,"Production",if(error == null) "Active" else "Verifying",1_900_000_000_000uL,delivered,!delivered,confirming,null,error)

    @Test fun lateOriginalAccountReceiptSurvivesSwitchRestartAndNeverAppearsInOtherAccount() {
        val root=Files.createTempDirectory("dmsg-play-test").toFile()
        try {
            val store=PlayPurchaseStore(root,cipher); store.bind(a); store.bind(b)
            val late=purchase(null); store.enqueue(late,a)
            assertEquals(1,store.pending(a).size); assertTrue(store.pending(b,true).isEmpty())
            assertFalse(store.canLaunch(a)); assertTrue(store.canLaunch(b))
            assertFalse(root.resolve("purchases.sealed").readBytes().toString(Charsets.UTF_8).contains(late.token))
            val restarted=PlayPurchaseStore(root,cipher)
            assertEquals(late.token,restarted.pending(a).single().purchase.token)
            assertEquals(a,restarted.pending(a).single().owner)
            assertFalse(late.toString().contains(late.token))
        } finally { root.deleteRecursively() }
    }

    @Test fun acceptedAndDeliveredButUnconfirmedStayPendingUntilAuthorityFinishes() {
        val root=Files.createTempDirectory("dmsg-play-test").toFile()
        try {
            val store=PlayPurchaseStore(root,cipher);store.bind(a);val purchase=purchase();store.enqueue(purchase)
            store.reconcile(a,listOf(receipt(purchase,false)));assertFalse(store.canLaunch(a))
            store.reconcile(a,listOf(receipt(purchase,true,true)));assertFalse(store.canLaunch(a))
            store.reconcile(a,listOf(receipt(purchase,true)));assertTrue(store.canLaunch(a));assertTrue(store.pending(a).isEmpty())
            store.enqueue(purchase);assertTrue(store.pending(a).isEmpty())
            store.enqueue(purchase,force=true);assertEquals(1,store.pending(a).size)
            store.submitted(purchase.id,a,100);store.submitted(purchase.id,a,1)
            assertEquals(100,store.pending(a).single().submittedAt)
        } finally { root.deleteRecursively() }
    }

    @Test fun unknownPurchaseCannotBeReassignedByUiAndVerifiedMismatchDoesNotBlockOtherAccounts() {
        val root=Files.createTempDirectory("dmsg-play-test").toFile()
        try {
            val store=PlayPurchaseStore(root,cipher);store.bind(a);store.bind(b);val purchase=purchase(null);store.enqueue(purchase)
            assertTrue(store.pending(a).isEmpty());assertEquals(1,store.pending(a,true).size)
            assertFalse(store.canLaunch(a));assertFalse(store.canLaunch(b))
            store.reconcile(a,listOf(receipt(purchase,false,error="account_mismatch")))
            assertTrue(store.canLaunch(a));assertEquals(1,store.pending(b,true).size)
            store.reconcile(b,listOf(receipt(purchase,false)))
            assertEquals(b,store.pending(b).single().owner);assertTrue(store.pending(a,true).isEmpty())
            assertThrows(IllegalStateException::class.java) { store.submitted(purchase.id,a,200) }
        } finally { root.deleteRecursively() }
    }

    @Test fun corruptedReceiptIsNeverSilentlyReplacedWithAnEmptyLedger() {
        val root=Files.createTempDirectory("dmsg-play-test").toFile()
        try {
            val store=PlayPurchaseStore(root,cipher);store.bind(a);store.enqueue(purchase())
            val file=root.resolve("purchases.sealed");val bytes=file.readBytes();bytes[bytes.lastIndex]=(bytes.last().toInt() xor 1).toByte();file.writeBytes(bytes)
            assertThrows(Exception::class.java) { PlayPurchaseStore(root,cipher).pending(a) }
            assertArrayEquals(bytes,file.readBytes())
        } finally { root.deleteRecursively() }
    }
}
