package net.dmsg.android.core

import org.junit.Assert.*
import org.junit.Test

class SecretLifetimeTest {
    @Test fun delayedOtpReadCannotExtendExpiryOrCopyAnExpiredValue() {
        assertEquals(1200uL, secretRemaining(2000u, 1000, 1800))
        assertEquals(0uL, secretRemaining(2000u, 1000, 3000))
        assertEquals(0uL, secretRemaining(2000u, 1000, 4500))
    }
    @Test fun independentNativeDurationsRetainUnsignedPrecision() {
        assertEquals(ULong.MAX_VALUE - 500u, secretRemaining(ULong.MAX_VALUE, 10, 510))
    }
}
