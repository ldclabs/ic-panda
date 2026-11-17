package net.dmsg.android.core

/** Include queue and read latency; never extend a native OTP's remaining life. */
fun secretRemaining(duration: ULong, requestedAt: Long, now: Long): ULong {
    val elapsed = (now - requestedAt).coerceAtLeast(0).toULong()
    return if (elapsed >= duration) 0u else duration - elapsed
}
