package net.dmsg.android.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str
import net.dmsg.android.ui.kit.*

/** Opens a cross-workspace search hit by switching to its workspace and destination route. */
internal fun openGlobalVaultHit(client: ClientController, id: String) {
    client.navigation.open("vault")
    client.navigation.push(Route("vault", id = id))
}

internal fun openGlobalChannelHit(client: ClientController, channelId: String, sequence: ULong? = null) {
    client.navigation.open("messages")
    client.navigation.push(Route("messages", id = channelId, sequence = sequence))
}

internal fun openGlobalLetterHit(client: ClientController, hit: LetterSearchHit) {
    client.navigation.open("messages")
    val field = when (hit.boxKind) {
        "sent", "pending" -> "sent"
        "draft" -> "draft"
        else -> null
    }
    client.navigation.push(Route("inbox", id = hit.id, field = field))
}

internal fun openGlobalFileHit(client: ClientController, id: String, isFolder: Boolean, parent: String = "root") {
    client.navigation.open("files")
    if (isFolder) client.navigation.folder(id) else client.navigation.push(Route("files", id = id, folder = parent))
}

/** Shared grouped results for the "All" search scope across all four workspaces. */
internal fun LazyListScope.globalSearchResults(
    client: ClientController,
    query: String,
    read: ReadState<GlobalSearchResult>,
    onRetry: () -> Unit,
) {
    val trimmed = query.trim()
    if (trimmed.isEmpty()) {
        item(key = "global-prompt") {
            EmptyState(str(R.string.search_everything), icon = Icons.Outlined.Search)
        }
        return
    }
    item(key = "global-feedback") { ReadFeedback(read, onRetry) }
    val result = read.valueOrNull()?.takeIf { it.query == trimmed } ?: return
    if (result.isEmpty && (read is ReadState.Ready || read is ReadState.Empty)) {
        item(key = "global-empty") { EmptyState(str(R.string.no_matches), icon = Icons.Outlined.Search) }
        return
    }
    if (result.vault.isNotEmpty()) {
        item(key = "global-h-vault") { SectionTitle(str(R.string.text_vault_ede2a06)) }
        items(result.vault, key = { "gv:" + it.id }) { item ->
            val hint = vaultSearchHint(item)
            BrowseRow(
                buildHighlightedText(item.title, item.titleSpans),
                Modifier.clickable { openGlobalVaultHit(client, item.id) },
                leading = { SecretIcon(item.kind) },
                supporting = {
                    Column {
                        Text(buildHighlightedText(item.subtitle.ifEmpty { vaultKindName(item.kind) }, item.subtitleSpans), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (hint.isNotEmpty()) Text(hint, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                },
            )
        }
    }
    if (result.channels.isNotEmpty()) {
        item(key = "global-h-channels") { SectionTitle(str(R.string.text_conversations_23a085e)) }
        items(result.channels, key = { "gc:" + it.id }) { ch ->
            BrowseRow(
                buildHighlightedText(ch.name, ch.nameSpans),
                Modifier.clickable { openGlobalChannelHit(client, ch.id) },
                leading = { Avatar(ch.name, ch.id) },
                supporting = {
                    ch.matchedMember?.let { member ->
                        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(str(R.string.search_member_label) + ":", maxLines = 1)
                            Text(buildHighlightedText(member, ch.memberSpans), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                },
            )
        }
    }
    if (result.messages.isNotEmpty()) {
        item(key = "global-h-messages") { SectionTitle(str(R.string.text_messages_3a9ab2d)) }
        items(result.messages, key = { "gm:${it.channelId}:${it.sequence}" }) { hit ->
            BrowseRow(
                buildHighlightedText(hit.previewText, hit.previewSpans),
                Modifier.clickable { openGlobalChannelHit(client, hit.channelId, hit.sequence) },
                leading = { BrowseIcon(Icons.Outlined.Forum) },
                supporting = {
                    val sender = hit.senderName?.takeIf(String::isNotBlank)
                    val meta = listOfNotNull(hit.channelName.takeIf(String::isNotBlank), sender, messageTime(hit.createdAtMs)).joinToString(" · ")
                    Text(meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
                },
            )
        }
    }
    if (result.letters.isNotEmpty()) {
        item(key = "global-h-letters") { SectionTitle(str(R.string.text_letters_cfbff5c)) }
        items(result.letters, key = { "gl:${it.boxKind}:${it.id}" }) { hit ->
            val who = hit.displayName?.takeIf(String::isNotBlank) ?: hit.account
            BrowseRow(
                buildHighlightedText(who, hit.nameSpans),
                Modifier.clickable { openGlobalLetterHit(client, hit) },
                leading = { BrowseIcon(Icons.Outlined.MailOutline) },
                supporting = {
                    if (hit.snippetSpans.isNotEmpty()) {
                        Text(buildHighlightedText("", hit.snippetSpans), maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                },
            )
        }
    }
    if (result.folders.isNotEmpty() || result.files.isNotEmpty()) {
        item(key = "global-h-files") { SectionTitle(str(R.string.text_files_c79ddc2)) }
        items(result.folders, key = { "gfd:" + it.id }) { folder ->
            BrowseRow(
                buildHighlightedText(folder.name, folder.nameSpans),
                Modifier.clickable { openGlobalFileHit(client, folder.id, isFolder = true) },
                leading = { BrowseIcon(Icons.Outlined.Folder) },
                supporting = {
                    if (folder.path.isNotEmpty()) {
                        Text(buildHighlightedText(folder.path.joinToString(" / "), folder.pathSpans), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                },
            )
        }
        items(result.files, key = { "gf:" + it.id }) { file ->
            BrowseRow(
                buildHighlightedText(file.name, file.nameSpans),
                Modifier.clickable { openGlobalFileHit(client, file.id, isFolder = false, parent = file.folder) },
                leading = { FileThumbnail(client, file) },
                supporting = {
                    val pathText = (listOf(str(R.string.text_files_c79ddc2)) + file.path).joinToString(" / ")
                    Text(buildHighlightedText(pathText, file.pathSpans), maxLines = 1, overflow = TextOverflow.Ellipsis)
                },
            )
        }
    }
}

internal fun vaultSearchHint(item: VaultSummary): String {
    val field = item.matchField ?: return ""
    if (field == "title" || field == "username") return ""
    if (field == "notes") return str(R.string.search_matched_notes)
    if (field == "tag" && !item.matchValue.isNullOrBlank()) return "#" + item.matchValue
    if (!item.matchValue.isNullOrBlank() && item.matchValue != item.subtitle) return item.matchValue
    return ""
}
