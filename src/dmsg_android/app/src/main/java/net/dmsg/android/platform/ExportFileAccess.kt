package net.dmsg.android.platform

import android.app.KeyguardManager
import android.content.Context
import android.os.SystemClock
import android.provider.DocumentsContract

/** RAM only. Process death, device lock, workspace lock and replacement end it. */
object ExportFileAccess {
    private var session: OpenExport? = null
    private var deadline = 0L
    @Synchronized fun grant(context: Context, store: SystemExportStore, opened: OpenExport) {
        check(store.purpose == ExportPurpose.files && store.current(opened.header) && !context.getSystemService(KeyguardManager::class.java).isDeviceLocked) { "LOCKED" }
        lock(); session = OpenExport(opened.header, opened.key.copyOf()); deadline = SystemClock.elapsedRealtime() + 300_000
        changed(context)
    }
    @Synchronized fun lock() { session?.close(); session = null; deadline = 0 }
    @Synchronized fun <T> read(context: Context, store: SystemExportStore, snapshot: String? = null, block: (OpenExport) -> T): T {
        val value = session ?: error("EXPORT_LOCKED")
        if (context.getSystemService(KeyguardManager::class.java).isDeviceLocked || SystemClock.elapsedRealtime() >= deadline || !store.current(value.header)) {
            lock(); error("EXPORT_LOCKED")
        }
        check(snapshot == null || snapshot == value.header.id) { "EXPORT_EXPIRED" }
        return block(value)
    }
    fun changed(context: Context) {
        context.contentResolver.notifyChange(DocumentsContract.buildRootsUri(context.packageName + ".documents"), null)
        context.contentResolver.notifyChange(DocumentsContract.buildChildDocumentsUri(context.packageName + ".documents", "root"), null)
    }
}
