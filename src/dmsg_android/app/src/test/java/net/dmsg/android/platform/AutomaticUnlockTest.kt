package net.dmsg.android.platform

import org.junit.Assert.*
import org.junit.Test

class AutomaticUnlockTest {
    @Test fun cancellationDoesNotReopenAuthenticationWithinTheSameVisit() {
        val policy = AutomaticUnlockPolicy()
        assertTrue(policy.consume(locked = true, fastUnlock = true, busy = false))
        policy.suppress()
        repeat(3) { assertFalse(policy.consume(locked = true, fastUnlock = true, busy = false)) }
        policy.rearm()
        assertTrue(policy.consume(locked = true, fastUnlock = true, busy = false))
    }
    @Test fun unavailableOrBusySessionsDoNotConsumeTheSingleAttempt() {
        val policy = AutomaticUnlockPolicy()
        assertFalse(policy.consume(locked = true, fastUnlock = false, busy = false))
        assertFalse(policy.consume(locked = false, fastUnlock = true, busy = false))
        assertFalse(policy.consume(locked = true, fastUnlock = true, busy = true))
        assertTrue(policy.consume(locked = true, fastUnlock = true, busy = false))
    }
}
