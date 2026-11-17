package net.dmsg.android.platform

/** Monotonic policy, independent of UI recreation and the wall clock. */
class SessionPolicy(private val now: () -> Long) {
    var immediateBackgroundLock: Boolean = false
    private var lastActivity = now()
    private var backgroundAt: Long? = null

    fun activity() { lastActivity = now() }
    fun background() { backgroundAt = now() }
    fun shouldLock(): Boolean {
        val current = now()
        if (current - lastActivity >= 15 * 60_000L) return true
        return backgroundAt?.let { current - it >= if (immediateBackgroundLock) 0 else 180_000L } ?: false
    }
    fun foreground(): Boolean {
        val expired = shouldLock()
        backgroundAt = null
        return expired
    }
}

/** One attempt per foreground visit; cancellation consumes it until a real background. */
class AutomaticUnlockPolicy {
    private var pending = true
    fun rearm() { pending = true }
    fun suppress() { pending = false }
    fun consume(locked: Boolean, fastUnlock: Boolean, busy: Boolean): Boolean {
        if (!pending || !locked || !fastUnlock || busy) return false
        pending = false
        return true
    }
}
