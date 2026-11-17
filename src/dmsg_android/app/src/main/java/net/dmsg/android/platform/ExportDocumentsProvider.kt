package net.dmsg.android.platform

import android.database.Cursor
import android.database.MatrixCursor
import android.os.*
import android.os.storage.StorageManager
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.system.ErrnoException
import android.system.OsConstants
import net.dmsg.android.R
import java.io.FileNotFoundException

/** Read-only immutable copies; it cannot create or unlock a MobileEngine. */
class ExportDocumentsProvider : DocumentsProvider() {
    private val app get() = checkNotNull(context)
    private val store by lazy { systemExports(app, ExportPurpose.files) }
    private val ioThread = lazy { HandlerThread("dmsg-export-reads").apply { start() } }
    private val io by ioThread
    override fun onCreate() = true
    override fun shutdown() { if (ioThread.isInitialized()) io.quitSafely(); super.shutdown() }
    override fun queryRoots(projection: Array<out String>?): Cursor = MatrixCursor(projection ?: ROOT_COLUMNS).apply {
        newRow().add(Root.COLUMN_ROOT_ID, "copies").add(Root.COLUMN_DOCUMENT_ID, "root").add(Root.COLUMN_TITLE, "dMsg")
            .add(Root.COLUMN_SUMMARY, str(R.string.text_selected_file_copies_8c3cd79)).add(Root.COLUMN_FLAGS, Root.FLAG_SUPPORTS_IS_CHILD)
            .add(Root.COLUMN_ICON, R.drawable.dmsg_mark).add(Root.COLUMN_MIME_TYPES, "*/*")
        setNotificationUri(app.contentResolver, DocumentsContract.buildRootsUri(app.packageName + ".documents"))
    }
    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor = MatrixCursor(projection ?: DOCUMENT_COLUMNS).apply {
        if (documentId == "root") directory(this)
        else guarded { ExportFileAccess.read(app, store, snapshot(documentId)) { opened -> row(this, opened.header, store.files(opened).firstOrNull { it.id == item(documentId) } ?: throw FileNotFoundException()) } }
    }
    override fun queryChildDocuments(parentDocumentId: String, projection: Array<out String>?, sortOrder: String?): Cursor = MatrixCursor(projection ?: DOCUMENT_COLUMNS).apply {
        if (parentDocumentId != "root") throw FileNotFoundException()
        val result = runCatching { ExportFileAccess.read(app, store) { opened -> store.files(opened).sortedBy { it.name }.forEach { row(this, opened.header, it) } } }
        result.exceptionOrNull()?.let { failure ->
            extras = Bundle().apply {
                if (failure.message in setOf("EXPORT_LOCKED", "EXPORT_EXPIRED", "EXPORT_NOT_CONFIGURED"))
                    putString(DocumentsContract.EXTRA_INFO, str(R.string.text_open_dmsg_account_system_integrations_to_authorize_file_access_fo_20a1082))
                else putString(DocumentsContract.EXTRA_ERROR, str(R.string.text_the_selected_copies_could_not_be_verified_update_them_in_dmsg_340040e))
            }
        }
        setNotificationUri(app.contentResolver, DocumentsContract.buildChildDocumentsUri(app.packageName + ".documents", "root"))
    }
    override fun isChildDocument(parentDocumentId: String, documentId: String): Boolean = parentDocumentId == "root" && runCatching {
        ExportFileAccess.read(app, store, snapshot(documentId)) { opened -> store.files(opened).any { it.id == item(documentId) } }
    }.getOrDefault(false)
    override fun openDocument(documentId: String, mode: String, signal: CancellationSignal?): ParcelFileDescriptor = guarded {
        if (mode != "r") throw FileNotFoundException("Read-only")
        signal?.throwIfCanceled()
        val snapshot = snapshot(documentId)
        val file = ExportFileAccess.read(app, store, snapshot) { opened -> store.files(opened).firstOrNull { it.id == item(documentId) } ?: throw FileNotFoundException() }
        app.getSystemService(StorageManager::class.java).openProxyFileDescriptor(ParcelFileDescriptor.MODE_READ_ONLY, object : ProxyFileDescriptorCallback() {
            private var released = false
            private fun <T> access(block: (OpenExport) -> T): T {
                try { check(!released); signal?.throwIfCanceled(); return ExportFileAccess.read(app, store, snapshot, block) }
                catch (_: Exception) { throw ErrnoException("read", OsConstants.EACCES) }
            }
            override fun onGetSize(): Long = access { file.size }
            override fun onRead(offset: Long, size: Int, data: ByteArray): Int = access { opened ->
                if (offset < 0 || size < 0 || size > data.size) throw ErrnoException("read", OsConstants.EINVAL)
                var written = 0
                while (written < size && offset + written < file.size) {
                    signal?.throwIfCanceled()
                    val at = offset + written; val index = (at / SystemExportStore.PART).toInt(); val start = (at % SystemExportStore.PART).toInt()
                    val part = store.plain(opened, file.id + "." + index)
                    try {
                        check(part.size == minOf(SystemExportStore.PART.toLong(), file.size - index.toLong() * SystemExportStore.PART).toInt())
                        val count = minOf(size - written, part.size - start)
                        part.copyInto(data, written, start, start + count); written += count
                    } finally { part.fill(0) }
                }
                written
            }
            override fun onRelease() { released = true }
        }, Handler(io.looper))
    }
    private fun directory(cursor: MatrixCursor) { cursor.newRow().add(Document.COLUMN_DOCUMENT_ID, "root").add(Document.COLUMN_DISPLAY_NAME, str(R.string.text_dmsg_file_copies_588b452)).add(Document.COLUMN_MIME_TYPE, Document.MIME_TYPE_DIR).add(Document.COLUMN_FLAGS, Document.FLAG_SUPPORTS_SETTINGS) }
    private fun row(cursor: MatrixCursor, header: ExportHeader, file: ExportFile) { cursor.newRow().add(Document.COLUMN_DOCUMENT_ID, header.id + ":" + file.id).add(Document.COLUMN_DISPLAY_NAME, file.name).add(Document.COLUMN_MIME_TYPE, file.mime).add(Document.COLUMN_SIZE, file.size).add(Document.COLUMN_LAST_MODIFIED, file.modified).add(Document.COLUMN_FLAGS, 0) }
    private fun snapshot(id: String): String { check(id.length == 73 && id[36] == ':'); return id.substring(0, 36) }
    private fun item(id: String) = id.substring(37)
    private fun <T> guarded(block: () -> T): T = try { block() } catch (_: Exception) { throw FileNotFoundException("Copy unavailable or access ended") }
    companion object {
        private val ROOT_COLUMNS = arrayOf(Root.COLUMN_ROOT_ID, Root.COLUMN_DOCUMENT_ID, Root.COLUMN_TITLE, Root.COLUMN_SUMMARY, Root.COLUMN_FLAGS, Root.COLUMN_ICON, Root.COLUMN_MIME_TYPES)
        private val DOCUMENT_COLUMNS = arrayOf(Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE, Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED, Document.COLUMN_FLAGS)
    }
}
