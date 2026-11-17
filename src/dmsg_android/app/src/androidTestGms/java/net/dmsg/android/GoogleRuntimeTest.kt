package net.dmsg.android

import android.content.ComponentName
import android.content.pm.PackageManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.google.firebase.FirebaseApp
import net.dmsg.android.platform.GoogleConfiguration
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class GoogleRuntimeTest {
    @Test fun unconfiguredGmsPackageHasAProtectedReceiverAndDoesNotInitializeFirebaseOrClient() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as DMsgApplication
        assertTrue(BuildConfig.HAS_FCM)
        assertTrue(BuildConfig.GOOGLE_CONFIG.isEmpty())
        assertNull(GoogleConfiguration.load(app))
        assertTrue(FirebaseApp.getApps(app).isEmpty())
        assertFalse(app.hasMainClient)
        val service = app.packageManager.getServiceInfo(ComponentName(app, "net.dmsg.android.platform.GoogleMessagingService"), 0)
        assertFalse(service.exported)
        try {
            app.packageManager.getProviderInfo(ComponentName(app, "com.google.firebase.provider.FirebaseInitProvider"), 0)
            fail("Firebase must be initialized only after explicit consent")
        } catch (_: PackageManager.NameNotFoundException) { }
    }
}
