package net.dmsg.android.core

import kotlinx.serialization.json.*
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.dmsg.core.*

data class VaultQuery(val text: String = "", val kind: String? = null, val favorites: Boolean = false,
    val tag: String? = null, val sort: String = "title", val trash: Boolean = false) {
    val filtered get() = text.isNotBlank() || kind != null || favorites || tag != null
}
data class VaultSummary(val id: String, val revision: String, val kind: String, val title: String, val subtitle: String,
    val favorite: Boolean, val tags: List<String>, val synced: Boolean, val conflict: Boolean, val reserved: Boolean,
    val deleted: Boolean, val createdAt: ULong, val updatedAt: ULong, val saveState: String? = null)
data class VaultListing(val items: List<VaultSummary>, val total: ULong, val all: ULong, val favorites: ULong,
    val kinds: Map<String, ULong>, val tags: List<String>, val hasMore: Boolean)

fun parseVaultListing(value: JsonElement): VaultListing {
    val root = value.objectValue()
    val items = root.rows("items").map { row -> VaultSummary(row.required("id"), row.required("revision"), row.required("kind"), row.required("title"), row.required("subtitle"),
        row.requiredFlag("favorite"), row.strings("tags"), row.requiredFlag("synced"), row.requiredFlag("conflict"), row.requiredFlag("reserved"), row.requiredFlag("deleted"), row.requiredUnsigned("createdAt"), row.requiredUnsigned("updatedAt"), row.text("saveState")) }
    val kinds = root["kinds"]?.objectValue() ?: error("INTEGRITY_FAILED")
    return VaultListing(items, root.requiredUnsigned("total"), root.requiredUnsigned("all"), root.requiredUnsigned("favorites"),
        kinds.keys.associateWith { kinds.requiredUnsigned(it) }, root.strings("tags"), root.requiredFlag("hasMore"))
}

class VaultBrowser(private val client: ClientController, private val requestKey: String, private val loaded: (VaultListing) -> Unit = {}) {
    val listing = ReadStore<VaultListing> { it.items.isEmpty() }
    private val criteria = MutableStateFlow(VaultQuery())
    val filters = criteria.asStateFlow()
    val query get() = criteria.value

    fun browse(next: VaultQuery = query, more: Boolean = false) {
        if (!client.state.value.unlocked) return
        if (more && (listing.state.value is ReadState.Loading || listing.value()?.hasMore != true)) return
        val changed = next != query
        val typing = next.text.isNotBlank() && next.text != query.text
        val previous = if (more && !changed) listing.value() else null
        criteria.value = next
        val request = listing.begin(keep = !changed)
        val offset = previous?.items?.size?.toUInt() ?: 0u
        client.perform({ core ->
            if (typing) delay(180)
            val snapshot = core.browseVault(VaultBrowseRequest(next.text, next.kind, next.favorites, next.tag, next.sort, offset, 100u, next.trash))
            check(snapshot.generation == core.controlGeneration()) { "LOCKED" }
            parseVaultListing(snapshot.decode())
        }, key = requestKey, replace = true, quiet = true, onError = { listing.fail(request, it) }) { page ->
            val joined = if (previous == null) page else page.copy(items = (previous.items + page.items).distinctBy { it.id })
            if (listing.accept(request, joined)) loaded(joined)
        }
    }

    fun reset(retainQuery: Boolean = false) {
        listing.reset()
        if (!retainQuery) criteria.value = VaultQuery()
    }
}

class VaultCoordinator(private val client: ClientController) {
    val detail = ReadStore<VaultItem>()
    val attachmentFiles = ReadStore<List<LibraryFile>> { it.isEmpty() }
    private var beforeTrash = VaultQuery()
    var selected: String? = null
        private set
    private val browser = VaultBrowser(client, "vault:list") { page ->
        val current = detail.value()
        if (current != null && page.items.any { it.id == current.id && it.revision != current.revision }) select(current.id)
    }
    val listing get() = browser.listing
    val filters get() = browser.filters
    val query get() = browser.query
    fun browse(next: VaultQuery = query, more: Boolean = false) = browser.browse(next, more)

    fun toggleTrash() {
        if (query.trash) browse(beforeTrash.copy(sort = query.sort))
        else { beforeTrash = query; browse(VaultQuery(sort = query.sort, trash = true)) }
    }

    fun loadAttachmentFiles() {
        val request = attachmentFiles.begin()
        client.perform({ it.searchFiles("", null).decode().arrayRows().map(::parseFile) }, key = "vault:attachment-files", replace = true, quiet = true,
            onError = { attachmentFiles.fail(request, it) }) { attachmentFiles.accept(request, it) }
    }

    fun select(id: String) {
        val same = selected == id
        selected = id
        val request = detail.begin(keep = same)
        client.perform({ core ->
            val value = core.vaultItem(id, null)
            check(value.generation == core.controlGeneration()) { "LOCKED" }
            value.item
        }, key = "vault:detail", replace = true, quiet = true, onError = { detail.fail(request, it) }) { detail.accept(request, it) }
    }

    fun changed() {
        client.systemExports.refreshCredentials()
        browse()
    }

    fun favorite(item: VaultItem) {
        client.perform({ it.favoriteVaultItem(item.id, item.revision, !item.favorite) }, key = "vault:edit:" + item.id) { browse(); select(item.id) }
    }

    fun clear(retainQuery: Boolean = false) {
        browser.reset(retainQuery); detail.reset(); attachmentFiles.reset()
        if (!retainQuery) { selected = null; beforeTrash = VaultQuery() }
    }
}
