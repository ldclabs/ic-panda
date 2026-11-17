package net.dmsg.android.core

import java.net.URI

/** Public system entries navigate only. No object ID, authority or action parameter. */
data class WorkspaceLink(val destination: String) {
    init { require(destination in pages) }
    val page get() = pages.getValue(destination)
    fun url() = "dmsg://open/" + destination
    companion object {
        val pages = linkedMapOf("vault" to "vault", "messages" to "messages", "files" to "files", "approvals" to "approvals", "imports" to "incoming-shares", "transfers" to "file-transfers", "autofill" to "system-autofill", "file-copies" to "system-files", "notifications" to "notifications")
        fun parse(raw: String): WorkspaceLink? = runCatching {
            if (raw.length > 128) return null
            val uri = URI(raw)
            if (uri.scheme != "dmsg" || uri.rawAuthority != "open" || uri.rawQuery != null || uri.rawFragment != null) return null
            val destination = uri.rawPath?.removePrefix("/") ?: return null
            if (uri.rawPath != "/" + destination || destination !in pages) return null
            WorkspaceLink(destination)
        }.getOrNull()
    }
}
