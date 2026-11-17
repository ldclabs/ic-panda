package net.dmsg.android.core

import net.dmsg.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

data class FileBrowsePreferences(val query: String = "", val sort: String = "name", val favorites: Boolean = false, val limit: Int = 100, val first: Int = 0, val offset: Int = 0)
data class FileActionTarget(val id: String, val name: String, val folder: Boolean)
data class FileActionResult(val id: String, val name: String, val action: String, val error: String?)
data class LibraryUsage(val files: ULong, val trashed: ULong, val versions: ULong, val localVersions: ULong, val bytes: ULong, val openCopies: ULong, val trashDays: ULong)
data class FileMetadataDraft(val file: LibraryFile, val tags: String = file.tags.joinToString(", "), val note: String = file.note, val starred: Boolean = file.starred)

/** A folder refresh cannot replace a different folder or a file's verified details. */
class FileCoordinator(private val client: ClientController) {
    data class Listing(val folder: String, val query: String, val trash: Boolean, val folders: List<LibraryFolder>, val files: List<LibraryFile>)
    val listing = ReadStore<Listing> { it.folders.isEmpty() && it.files.isEmpty() }
    val detail = ReadStore<FileDetails>()
    val destinations = ReadStore<Listing>()
    val tree = ReadStore<List<LibraryFolder>> { it.isEmpty() }
    val usage = ReadStore<LibraryUsage>()
    private val resultsValue = MutableStateFlow<List<FileActionResult>>(emptyList())
    val results = resultsValue.asStateFlow()
    private val editorValue = MutableStateFlow<FileMetadataDraft?>(null)
    val editor = editorValue.asStateFlow()
    private val editorFailure = MutableStateFlow<String?>(null)
    val editorError = editorFailure.asStateFlow()
    private val selectionValue = MutableStateFlow<Set<String>>(emptySet())
    val selection = selectionValue.asStateFlow()
    private val cache = linkedMapOf<String, Listing>()
    private val preferences = mutableMapOf<String, FileBrowsePreferences>()
    var folder = "root"
        private set
    var query = ""
        private set
    var trash = false
        private set
    private var selected: String? = null
    fun preferences(folder: String, trash: Boolean) = preferences["$trash:$folder"] ?: FileBrowsePreferences()
    fun remember(folder: String, trash: Boolean, value: FileBrowsePreferences, owner: SessionStamp?) {
        if (owner != null && client.accepts(owner)) preferences["$trash:$folder"] = value
    }
    fun choose(id: String) { client.activity(); selectionValue.value = if (id in selectionValue.value) selectionValue.value - id else selectionValue.value + id }
    fun clearSelection() { selectionValue.value = emptySet() }
    fun beginMetadata(file: LibraryFile) { if (editorValue.value?.file?.id != file.id) { editorValue.value = FileMetadataDraft(file); editorFailure.value = null } }
    fun editMetadata(value: FileMetadataDraft) { client.activity(); editorValue.value = value }
    fun discardMetadata() { editorValue.value = null; editorFailure.value = null }
    fun reviewNewMetadata(file: LibraryFile) { editorValue.value?.takeIf { it.file.id == file.id }?.let {
        val tags = it.tags.split(',').map(String::trim).filter(String::isNotEmpty).distinct()
        editorValue.value = it.copy(file = file, note = if (it.note == it.file.note) file.note else it.note,
            tags = if (tags == it.file.tags) file.tags.joinToString(", ") else it.tags,
            starred = if (it.starred == it.file.starred) file.starred else it.starred)
        editorFailure.value = null
    } }
    fun browse(next: String = folder, search: String = query, discarded: Boolean = trash) {
        val same = next == folder && search == query && discarded == trash
        val key = "$discarded:$next:$search"
        if (!same) {
            selectionValue.value = emptySet()
            listing.reset()
            cache[key]?.let { listing.accept(listing.begin(false), it) }
        }
        val request = listing.begin()
        folder = next; query = search; trash = discarded
        client.perform({ core ->
            val base = if (discarded) core.fileTrash() else core.files(next)
            val (folders, files) = parseFiles(base.json)
            if (search.isBlank()) Listing(next, search, discarded, folders, files)
            else if (discarded) Listing(next, search, discarded, folders.filter { it.name.contains(search, true) }, files.filter { it.name.contains(search, true) || it.note.contains(search, true) || it.tags.any { tag -> tag.contains(search, true) } })
            else Listing(next, search, discarded, emptyList(), core.searchFiles(search, next.takeIf { it != "root" }).decode().arrayRows().map(::parseFile))
        }, key = "files:list", replace = true, onError = { listing.fail(request, it) }) {
            listing.accept(request, it)
            selectionValue.value = selectionValue.value.intersect((it.files.map { it.id } + it.folders.map { it.id }).toSet())
            cache.remove(key); cache[key] = it
            while (cache.size > 8) cache.remove(cache.keys.first())
        }
        if (tree.value() == null) loadTree()
    }
    fun select(id: String) {
        val request = detail.begin(keep = selected == id); selected = id
        client.perform({ parseFileDetails(it.fileDetails(id).decode()) }, key = "files:detail", replace = true,
            onError = { detail.fail(request, it) }) { detail.accept(request, it) }
    }
    fun destination(parent: String) {
        val request = destinations.begin(false)
        client.perform({ parseFiles(it.files(parent).json).let { Listing(parent, "", false, it.first, it.second) } }, key = "files:destinations", replace = true,
            onError = { destinations.fail(request, it) }) { destinations.accept(request, it) }
    }
    fun loadTree() {
        val request = tree.begin()
        client.perform({ it.fileFolders().decode().arrayRows().map { row -> LibraryFolder(row.required("id"), row.required("name"), row.required("parent"), row.strings("path")) } },
            key = "files:tree", replace = true, quiet = true, onError = { tree.fail(request, it) }) { tree.accept(request, it) }
    }
    fun loadUsage() {
        val request = usage.begin()
        client.perform({ it.fileLibraryUsage().decode().objectValue().let { row -> LibraryUsage(row.requiredUnsigned("files"), row.requiredUnsigned("trashed"), row.requiredUnsigned("versions"), row.requiredUnsigned("local_versions"), row.requiredUnsigned("blob_bytes"), row.requiredUnsigned("open_copies"), row.requiredUnsigned("trash_days")) } },
            key = "files:usage", replace = true, quiet = true, onError = { usage.fail(request, it) }) { usage.accept(request, it) }
    }
    fun changed() { cache.clear(); client.messageAttachments.invalidate(); browse(); loadTree(); selected?.let(::select) }
    fun rename(id: String, name: String, done: () -> Unit) = client.perform({ it.renameFile(id, name) }, key = "files:rename:" + id) { changed(); done() }
    fun move(id: String, folder: String, done: () -> Unit) = client.perform({ it.moveFile(id, folder) }, key = "files:move:" + id) { changed(); done() }
    fun remove(id: String, done: () -> Unit) = client.perform({ it.trashFile(id) }, key = "files:trash:" + id) { changed(); done() }
    fun restore(id: String, done: () -> Unit) = client.perform({ it.restoreFile(id) }, key = "files:restore:" + id) { changed(); done() }
    fun saveMetadata(done: () -> Unit) {
        val value = editorValue.value ?: return
        val tags = value.tags.split(',').map(String::trim).filter(String::isNotEmpty).distinct()
        editorFailure.value = null
        client.perform({ it.updateFileMetadata(value.file.id, FileMetadata(value.file.tags, value.file.note, value.file.starred), FileMetadata(tags, value.note, value.starred)) }, key = "files:metadata:${value.file.id}", quiet = true, onError = { editorFailure.value = it; select(value.file.id) }) {
            if (editorValue.value == value) editorValue.value = null
            changed(); done()
        }
    }
    fun useVersion(file: LibraryFile, version: String) = client.perform({ it.useFileVersion(file.id, version, file.version) }, key = "files:version:${file.id}") { changed() }
    fun deleteVersion(file: LibraryFile, version: String) = client.perform({ it.deleteFileVersion(file.id, version, file.version) }, key = "files:version:${file.id}") { changed() }
    fun batch(action: String, targets: List<FileActionTarget>, destination: String? = null, done: () -> Unit = {}) {
        if (targets.isEmpty() || client.isWorking("files:batch")) return
        val stamp = client.currentStamp() ?: return
        resultsValue.value = emptyList()
        client.perform({ core ->
            for (target in targets) {
                val error = try {
                    when (action) {
                        "trash" -> core.trashFile(target.id)
                        "restore" -> core.restoreFile(target.id)
                        "purge" -> core.purgeFile(target.id)
                        "move" -> core.moveFile(target.id, checkNotNull(destination))
                        else -> error("INVALID_INPUT")
                    }
                    null
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Throwable) { client.errorCode(error) }
                withContext(Dispatchers.Main.immediate) {
                    if (client.accepts(stamp)) {
                        resultsValue.value += FileActionResult(target.id, target.name, action, error)
                        if (error == null) selectionValue.value -= target.id
                    }
                }
            }
        }, key = "files:batch", onError = { changed(); loadUsage() }) { changed(); loadUsage(); done() }
    }
    fun cancelBatch() { client.cancelOperation("files:batch"); changed(); loadUsage() }
    fun clear(retainForms: Boolean = false) {
        listing.reset(); detail.reset(); destinations.reset(); tree.reset(); usage.reset(); resultsValue.value = emptyList(); selectionValue.value = emptySet(); cache.clear(); preferences.clear()
        if (!retainForms) { editorValue.value = null; editorFailure.value = null }
        folder = "root"; query = ""; trash = false; selected = null
    }
}
