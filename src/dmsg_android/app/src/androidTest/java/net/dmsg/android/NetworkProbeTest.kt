package net.dmsg.android

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import net.dmsg.core.MobileException
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.net.URL
import android.content.Context
import javax.net.ssl.HttpsURLConnection

/** Explicit live, read-only deployment probe. Ordinary offline CI does not run it. */
@RunWith(AndroidJUnit4::class)
class NetworkProbeTest {
    @Test fun platformTlsAndRustDeploymentPreflight() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("networkProbe") == "true")
        val connection = URL("https://icp-api.io/api/v2/status").openConnection() as HttpsURLConnection
        connection.connectTimeout = 10_000; connection.readTimeout = 10_000
        var certificates = emptyArray<ByteArray>()
        try {
            assertEquals(200, connection.responseCode)
            connection.inputStream.use { assertTrue(it.read() >= 0) }
            certificates = connection.serverCertificates.map { it.encoded }.toTypedArray()
        } finally { connection.disconnect() }
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        // Exercise the published Kotlin verifier using the very same valid server
        // chain, separating provider initialization from Rust/JNI transport wiring.
        val verifier = Class.forName("org.rustls.platformverifier.CertificateVerifier")
        val verify = verifier.getDeclaredMethod("verifyCertificateChain", Context::class.java, String::class.java,
            String::class.java, Array<String>::class.java, ByteArray::class.java, java.lang.Long.TYPE, Array<ByteArray>::class.java)
        verify.isAccessible = true
        val verification = verify.invoke(null, app, "icp-api.io", "RSA", arrayOf("1.3.6.1.5.5.7.3.1"), null, System.currentTimeMillis(), certificates)
        val code = verification.javaClass.getDeclaredField("code").apply { isAccessible = true }.getInt(verification)
        val message = verification.javaClass.getDeclaredField("message").apply { isAccessible = true }.get(verification) as? String
        assertEquals("Published verifier: " + (message?.take(400) ?: "no detail"), 0, code)
        ActivityScenario.launch(MainActivity::class.java).use {
            val deadline = android.os.SystemClock.elapsedRealtime() + 15_000
            while (app.client.state.value.session == null) {
                check(android.os.SystemClock.elapsedRealtime() < deadline)
                Thread.sleep(50)
            }
            val core = checkNotNull(app.client.scopedHandle())
            try {
                val started = runBlocking { core.beginLogin() }
                assertTrue(started.url.startsWith("https://dmsg.net/_/login#"))
            } catch (error: MobileException.Failure) {
                fail("Android platform TLS passed; native public-deployment preflight failed with " + error.code)
            } finally { core.cancelLogin(); core.destroy() }
        }
    }
}
