package net.dmsg.android.platform

import android.content.Context

class GooglePush(private val context: Context) {
    val configuration: GoogleConfiguration? get() = null
    val available get() = false
    suspend fun token(): String = error("GOOGLE_UNAVAILABLE")
    suspend fun integrity(requestHash: String): String = error("GOOGLE_UNAVAILABLE")
    companion object { fun initializeConsented(context: Context) = Unit }
}
