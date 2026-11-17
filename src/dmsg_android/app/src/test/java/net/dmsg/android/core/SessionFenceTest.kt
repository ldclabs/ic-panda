package net.dmsg.android.core
import org.junit.Assert.*
import org.junit.Test

class SessionFenceTest {
    @Test fun returningToSameAccountDoesNotReviveOldWork() {
        val fence = SessionFence()
        assertTrue(fence.adopt(1u, 1u, "a")); val old = fence.stamp()!!
        fence.adopt(2u, 2u, "b"); fence.adopt(3u, 3u, "a")
        assertFalse(fence.current(old))
        assertFalse(fence.adopt(1u, 99u, "a"))
    }
    @Test fun lateReadAndBackgroundCompletionCannotRestoreAView() {
        val fence = SessionFence()
        fence.adopt(1u, 9u, "a"); val old = fence.stamp()!!
        assertFalse(fence.adopt(1u, 8u, "a"))
        fence.invalidate()
        assertFalse(fence.current(old))
        assertTrue(fence.adopt(1u, 10u, "a"))
        assertFalse(fence.current(old))
    }
}
