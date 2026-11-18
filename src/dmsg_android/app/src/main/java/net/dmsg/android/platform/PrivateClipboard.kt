package net.dmsg.android.platform

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import android.os.SystemClock
import java.util.UUID

/** Best effort expiry; background restrictions can prevent timely cleanup. */
class PrivateClipboard(context: Context) {
    private val clipboard = context.getSystemService(ClipboardManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var owned: String? = null
    private var expiresAt = 0L

    fun copy(value: String, expiresInMs: Long = 30_000) {
        val nonce = UUID.randomUUID().toString()
        val clip = ClipData.newPlainText("dMsg", value)
        clip.description.extras = PersistableBundle().apply {
            putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
            putString(MARKER, nonce)
        }
        clipboard.setPrimaryClip(clip)
        owned = nonce
        val delay = expiresInMs.coerceIn(1, 30_000)
        expiresAt = SystemClock.elapsedRealtime() + delay
        main.postDelayed(::clearExpired, delay)
    }

    fun clearExpired() {
        if (SystemClock.elapsedRealtime() >= expiresAt) clearOwned()
    }

    fun clearOwned() {
        val nonce = owned ?: return
        expiresAt = 0
        runCatching {
            // Android can deny clipboard reads while the app lacks focus. Retry on return.
            val description = clipboard.primaryClipDescription ?: return
            if (description.extras?.getString(MARKER) == nonce) clipboard.clearPrimaryClip()
            owned = null
        }
    }

    private companion object { const val MARKER = "net.dmsg.clipboard-owner" }
}
