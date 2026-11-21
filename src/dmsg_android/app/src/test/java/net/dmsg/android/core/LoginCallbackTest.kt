package net.dmsg.android.core

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LoginCallbackTest {
    @Test fun onlyTheExactAndroidEndpointWithABoundedBase64UrlFragmentIsAccepted() {
        val target = "https://dmsg.net/_/android/auth/callback"
        assertTrue(LoginCallback.valid("$target#AZaz09_-"))
        for (link in listOf(target, "$target#", "$target/#abc", "$target?x=1#abc", "$target#abc=",
            "$target#%41", "$target#非", "https://dmsg.net:443/_/android/auth/callback#abc",
            "https://dmsg.net/_/ios/auth/callback#abc", "dmsg://open/vault", "$target#" + "A".repeat(96 * 1024))) {
            assertFalse(LoginCallback.valid(link))
        }
    }
}
