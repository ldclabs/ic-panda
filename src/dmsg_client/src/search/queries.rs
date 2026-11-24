//! Unified high-recall search queries across Vault, Files, Channels, Messages, Letters,
//! and cross-workspace Global Search.

use super::cache::sync_vault_cache;
use super::matcher::{
    HighlightSpan, PreparedText, Query, WeightedField, highlight_spans, mime_aliases, snippet_spans,
};
use crate::{
    Client,
    channel::{ChannelView, Message},
    channel_log::ChannelType,
    hold::Hold,
    library::{
        SearchFilter,
        catalog::{FileView, FolderView, ROOT},
    },
    vault::reading::{BrowsePage, BrowseQuery, Session, Summary},
};
use dmsg_types::{Result, ensure_valid};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// A matching folder in [`FileSearchListing`].
#[derive(Clone, Debug, Serialize)]
pub struct FolderSearchHit {
    /// Standard folder fields.
    #[serde(flatten)]
    pub folder: FolderView,
    /// Highlighted folder name segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<HighlightSpan>,
    /// Highlighted path segments (joined with `" / "`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_spans: Vec<HighlightSpan>,
    /// Relevance score.
    pub score: u32,
}

/// A matching file in [`FileSearchListing`].
#[derive(Clone, Debug, Serialize)]
pub struct FileSearchHit {
    /// Standard file fields.
    #[serde(flatten)]
    pub file: FileView,
    /// Highlighted file name segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<HighlightSpan>,
    /// Highlighted folder path segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_spans: Vec<HighlightSpan>,
    /// Secondary field that matched when the name alone did not cover all tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_field: Option<String>,
    /// Safe preview of the matched secondary value (e.g., matched tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_value: Option<String>,
    /// Relevance score.
    pub score: u32,
}

/// Combined folder and file search results returned in one call.
#[derive(Clone, Debug, Serialize)]
pub struct FileSearchListing {
    /// Matching folders in the requested scope.
    pub folders: Vec<FolderSearchHit>,
    /// Matching files in the requested scope.
    pub files: Vec<FileSearchHit>,
}

/// A matching channel in directory search (including matches via member display names/handles).
#[derive(Clone, Debug, Serialize)]
pub struct ChannelSearchHit {
    /// Standard channel view fields.
    #[serde(flatten)]
    pub channel: ChannelView,
    /// Highlighted channel name segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<HighlightSpan>,
    /// Secondary field that matched (e.g. `"member"`, `"draft"`, `"preview"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_field: Option<String>,
    /// Display name or short preview explaining why this channel matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_value: Option<String>,
    /// Relevance score.
    pub score: u32,
}

/// One matching message with centered highlight snippet and attachment highlights.
#[derive(Clone, Debug, Serialize)]
pub struct MessageSearchHit {
    /// Channel ID owning this message.
    pub channel: String,
    /// Public message fields.
    pub message: Message,
    /// Cached sender display name, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    /// Centered snippet spans of the message text with highlighted matches.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippet_spans: Vec<HighlightSpan>,
    /// Highlighted attachment filename segments, if matched or present.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_spans: Vec<HighlightSpan>,
    /// Relevance score.
    pub score: u32,
}

/// One page of single-channel message search hits.
#[derive(Clone, Debug, Serialize)]
pub struct ChannelMessageSearchPage {
    /// Matching hits (newest first for timeline consistency, or ranked when searching).
    pub hits: Vec<MessageSearchHit>,
    /// Whether earlier local matches exist before `next_before`.
    pub has_more: bool,
    /// Exclusive sequence cursor for fetching earlier matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_before: Option<u64>,
}

/// Grouped cross-channel message hits for one channel.
#[derive(Clone, Debug, Serialize)]
pub struct ChannelSearchGroup {
    /// Channel ID.
    pub channel_id: String,
    /// Channel display name.
    pub channel_name: String,
    /// Matching messages in this channel (newest first).
    pub hits: Vec<MessageSearchHit>,
    /// Exclusive sequence cursor if more earlier matches exist in this channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_before: Option<u64>,
    /// Highest relevance score in this group.
    pub top_score: u32,
}

/// One matching letter across inbox, archived, sent, pending, or drafts.
#[derive(Clone, Debug, Serialize)]
pub struct LetterSearchHit {
    /// Letter/draft identifier (`order_id` or `draft.id`).
    pub id: String,
    /// Box category: `"inbox"`, `"archive"`, `"sent"`, `"pending"`, or `"draft"`.
    pub box_kind: String,
    /// Counterparty account ID (sender for incoming, recipient for outgoing/draft).
    pub account: String,
    /// Cached counterparty display name, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Highlighted counterparty display name or account ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<HighlightSpan>,
    /// Centered highlighted snippet of letter text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippet_spans: Vec<HighlightSpan>,
    /// Timestamp in Unix milliseconds.
    pub created_at: u64,
    /// Whether the letter has been read (incoming only).
    pub read: bool,
    /// Whether the letter is archived (incoming only).
    pub archived: bool,
    /// Delivery state (`visible`, `queued`, `failed`, `draft`, etc.).
    pub state: String,
    /// Relevance score.
    pub score: u32,
}

/// Request parameters for unified cross-workspace global search.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GlobalSearchRequest {
    /// User search query.
    pub query: String,
    /// Optional subset of scopes (`"vault"`, `"channels"`, `"messages"`, `"letters"`, `"files"`).
    /// Empty means search all scopes available to the current account.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Maximum hits returned per group (default 8, max 50).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Compact vault hit inside [`GlobalSearchResponse`].
#[derive(Clone, Debug, Serialize)]
pub struct GlobalVaultHit {
    /// Safe summary (never includes concealed/OTP values or note plaintext).
    #[serde(flatten)]
    pub summary: Summary,
    /// Relevance score.
    pub score: u32,
}

/// Compact channel hit inside [`GlobalSearchResponse`].
#[derive(Clone, Debug, Serialize)]
pub struct GlobalChannelHit {
    /// Channel ID.
    pub id: String,
    /// Channel name.
    pub name: String,
    /// Highlighted channel name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<HighlightSpan>,
    /// Unread count.
    pub unread: u64,
    /// Whether this is a direct conversation.
    pub direct: bool,
    /// Secondary match explanation (`"member"`, `"draft"`, `"preview"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_field: Option<String>,
    /// Matched member name or hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_value: Option<String>,
    /// Relevance score.
    pub score: u32,
}

/// Compact file or folder hit inside [`GlobalSearchResponse`].
#[derive(Clone, Debug, Serialize)]
pub struct GlobalFileHit {
    /// File or folder ID.
    pub id: String,
    /// `"file"` or `"folder"`.
    pub kind: &'static str,
    /// Parent folder ID (for files, the containing folder; for folders, parent).
    pub folder: String,
    /// Display name.
    pub name: String,
    /// Highlighted name segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<HighlightSpan>,
    /// Breadcrumb folder names from root.
    pub path: Vec<String>,
    /// Highlighted path segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_spans: Vec<HighlightSpan>,
    /// File MIME type (empty for folders).
    pub mime: String,
    /// File size in bytes (0 for folders).
    pub size: u64,
    /// File sync status (`local`, `backed_up`, `cloud_only`, etc.; `"folder"` for folders).
    pub status: String,
    /// Secondary match field (e.g. `"tag"`, `"path"`, `"note"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_field: Option<String>,
    /// Secondary match preview (e.g. matched tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_value: Option<String>,
    /// Relevance score.
    pub score: u32,
}

/// Unified cross-workspace global search response shared by Desktop, Android, and iOS.
#[derive(Clone, Debug, Default, Serialize)]
pub struct GlobalSearchResponse {
    /// Original trimmed query.
    pub query: String,
    /// Matching vault entries.
    pub vault: Vec<GlobalVaultHit>,
    /// Matching channels.
    pub channels: Vec<GlobalChannelHit>,
    /// Matching messages across channels.
    pub messages: Vec<MessageSearchHit>,
    /// Matching letters across inbox/archive/sent/drafts.
    pub letters: Vec<LetterSearchHit>,
    /// Matching files and folders.
    pub files: Vec<GlobalFileHit>,
}

/// Browse and search the vault using the unlocked incremental cache, relevance ranking,
/// pinyin/multi-token/fuzzy matching, and pre-sliced highlight spans.
pub async fn browse_vault_cached<H: Hold>(h: &H, query: &BrowseQuery) -> Result<BrowsePage> {
    ensure_valid(query.limit > 0 && query.limit <= 10_000, "page")?;
    ensure_valid(
        matches!(
            query.sort.as_str(),
            "title" | "updated" | "created" | "relevance"
        ),
        "sort",
    )?;
    ensure_valid(
        query
            .kind
            .as_ref()
            .is_none_or(|k| crate::vault::TYPES.contains(&k.as_str())),
        "kind",
    )?;
    let session = h.with(|c| Session::start(c)).await?;
    let (docs, conflicts) = sync_vault_cache(h, session).await?;
    h.with(|c| session.check(c)).await?;

    let parsed = query
        .filter
        .query
        .as_deref()
        .map(Query::parse)
        .unwrap_or_else(|| Query::parse(""));

    let mut matched: Vec<(u32, Summary)> = Vec::new();
    let mut all = 0usize;
    let mut favorites = 0usize;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut tags: BTreeSet<String> = BTreeSet::new();

    for doc in docs {
        if doc.deleted != query.filter.trash {
            continue;
        }
        let Some(hit) = doc.match_query(&parsed) else {
            continue;
        };
        tags.extend(doc.summary.tags.iter().cloned());
        if query
            .tag
            .as_ref()
            .is_some_and(|tag| !doc.summary.tags.contains(tag))
        {
            continue;
        }
        all += 1;
        favorites += usize::from(doc.summary.favorite);
        *kinds.entry(doc.summary.kind.clone()).or_default() += 1;
        if query
            .kind
            .as_ref()
            .is_none_or(|kind| *kind == doc.summary.kind)
            && (!query.favorites || doc.summary.favorite)
        {
            let mut summary = doc.summary;
            summary.conflict |= conflicts.contains(&summary.id);
            summary.title_spans = hit.title_spans;
            summary.subtitle_spans = hit.subtitle_spans;
            summary.match_field = hit.match_field;
            summary.match_value = hit.match_value;
            matched.push((hit.score, summary));
        }
    }

    let use_relevance = query.sort == "relevance" && !parsed.is_empty();
    if use_relevance {
        matched.sort_by(|(sa, a), (sb, b)| {
            sb.cmp(sa)
                .then_with(|| b.favorite.cmp(&a.favorite))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.id.cmp(&b.id))
        });
    } else if query.sort == "title" {
        matched.sort_by_cached_key(|(_, e)| (e.title.to_lowercase(), e.id.clone()));
    } else if query.sort == "created" {
        matched.sort_by(|(_, a), (_, b)| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
    } else {
        matched.sort_by(|(_, a), (_, b)| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    let total = matched.len();
    let has_more = query.offset.saturating_add(query.limit) < total;
    let items = matched
        .into_iter()
        .skip(query.offset)
        .take(query.limit)
        .map(|(_, s)| s)
        .collect();

    Ok(BrowsePage {
        items,
        total,
        all,
        favorites,
        kinds,
        tags: tags.into_iter().collect(),
        has_more,
    })
}

/// Unified cross-workspace global search across Vault, Channels, Messages, Letters, and Files.
pub async fn global_search<H: Hold>(
    h: &H,
    req: &GlobalSearchRequest,
) -> Result<GlobalSearchResponse> {
    let parsed = Query::parse(&req.query);
    ensure_valid(req.query.chars().count() <= 200, "query")?;
    if parsed.is_empty() {
        return Ok(GlobalSearchResponse {
            query: String::new(),
            ..Default::default()
        });
    }
    let limit = req.limit.unwrap_or(8).clamp(1, 50);
    let want =
        |scope: &str| req.scopes.is_empty() || req.scopes.iter().any(|s| s == "all" || s == scope);

    let session = h.with(|c| Session::start(c)).await?;
    let is_local = h.with(|c| Ok(c.is_local())).await?;

    let mut response = GlobalSearchResponse {
        query: parsed.raw().to_string(),
        ..Default::default()
    };

    // 1. Vault (uses incremental VaultSearchCache, releasing client lock between any decrypts)
    if want("vault") {
        let (docs, conflicts) = sync_vault_cache(h, session).await?;
        let mut vault_hits: Vec<GlobalVaultHit> = Vec::new();
        for doc in docs {
            if doc.deleted {
                continue;
            }
            if let Some(hit) = doc.match_query(&parsed) {
                let mut summary = doc.summary;
                summary.conflict |= conflicts.contains(&summary.id);
                summary.title_spans = hit.title_spans;
                summary.subtitle_spans = hit.subtitle_spans;
                summary.match_field = hit.match_field;
                summary.match_value = hit.match_value;
                vault_hits.push(GlobalVaultHit {
                    summary,
                    score: hit.score,
                });
            }
        }
        vault_hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.summary.favorite.cmp(&a.summary.favorite))
                .then_with(|| b.summary.updated_at.cmp(&a.summary.updated_at))
                .then_with(|| a.summary.id.cmp(&b.summary.id))
        });
        vault_hits.truncate(limit);
        response.vault = vault_hits;
    }

    // 2. Files & folders (from in-memory Catalog)
    if want("files") {
        response.files = h
            .with(|c| {
                session.check(c)?;
                let listing =
                    c.files_search_listing(parsed.raw(), &SearchFilter::default(), false)?;
                let mut items: Vec<GlobalFileHit> =
                    Vec::with_capacity(listing.folders.len() + listing.files.len());
                for f in listing.folders {
                    items.push(GlobalFileHit {
                        id: f.folder.id,
                        kind: "folder",
                        folder: f.folder.parent,
                        name: f.folder.name,
                        name_spans: f.name_spans,
                        path: f.folder.path,
                        path_spans: f.path_spans,
                        mime: String::new(),
                        size: 0,
                        status: "folder".into(),
                        match_field: None,
                        match_value: None,
                        score: f.score,
                    });
                }
                for f in listing.files {
                    items.push(GlobalFileHit {
                        id: f.file.id,
                        kind: "file",
                        folder: f.file.folder,
                        name: f.file.name,
                        name_spans: f.name_spans,
                        path: f.file.path,
                        path_spans: f.path_spans,
                        mime: f.file.mime,
                        size: f.file.size,
                        status: f.file.status.to_string(),
                        match_field: f.match_field,
                        match_value: f.match_value,
                        score: f.score,
                    });
                }
                items.sort_by(|a, b| {
                    b.score
                        .cmp(&a.score)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                items.truncate(limit);
                Ok(items)
            })
            .await?;
    }

    // 3. Channels, Messages, and Letters (cloud accounts only)
    if !is_local {
        if want("channels") {
            response.channels = h
                .with(|c| {
                    session.check(c)?;
                    let hits = c.channel_list_search(parsed.raw())?;
                    Ok(hits
                        .into_iter()
                        .take(limit)
                        .map(|h| GlobalChannelHit {
                            id: h.channel.id,
                            name: h.channel.name,
                            name_spans: h.name_spans,
                            unread: h.channel.unread,
                            direct: h
                                .channel
                                .ledger
                                .as_ref()
                                .is_some_and(|l| matches!(l.kind, ChannelType::Direct)),
                            match_field: h.match_field,
                            match_value: h.match_value,
                            score: h.score,
                        })
                        .collect())
                })
                .await?;
        }

        if want("messages") {
            response.messages = h
                .with(|c| {
                    session.check(c)?;
                    let groups = c.channel_search_all(parsed.raw(), 3, 20, false)?;
                    let mut flat: Vec<MessageSearchHit> =
                        groups.into_iter().flat_map(|g| g.hits).collect();
                    flat.sort_by(|a, b| {
                        b.score
                            .cmp(&a.score)
                            .then_with(|| b.message.stored_at.cmp(&a.message.stored_at))
                    });
                    flat.truncate(limit);
                    Ok(flat)
                })
                .await?;
        }

        if want("letters") {
            response.letters = h
                .with(|c| {
                    session.check(c)?;
                    let mut hits = c.inbox_search(parsed.raw(), None)?;
                    hits.truncate(limit);
                    Ok(hits)
                })
                .await?;
        }
    }

    h.with(|c| session.check(c)).await?;
    Ok(response)
}

impl Client {
    /// Cached account display names keyed by stringified account ID.
    pub(crate) fn cached_display_names(&self) -> HashMap<String, String> {
        self.public_names()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(id, info)| info.name.map(|name| (id.to_string(), name)))
            .collect()
    }

    /// Search both folders and files in one call using the unified pinyin/multi-token matcher,
    /// supporting trash scope, folder subtree, starred, tag filters, path matching, and highlights.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` for an unknown live folder.
    pub fn files_search_listing(
        &mut self,
        query: &str,
        filter: &SearchFilter,
        trash: bool,
    ) -> Result<FileSearchListing> {
        struct RawFileMatch {
            id: String,
            score: u32,
            name_ranges: Vec<(usize, usize)>,
            path_ranges: Vec<(usize, usize)>,
            match_field: Option<String>,
            match_value: Option<String>,
        }
        let parsed = Query::parse(query);
        let (folder_hits, file_ids) = {
            let catalog = self.catalog()?;
            let within: Option<BTreeSet<String>> = if trash {
                None
            } else {
                match &filter.folder {
                    Some(folder) if folder != ROOT => {
                        catalog.live_folder(folder)?;
                        Some(catalog.subtree(folder).into_iter().collect())
                    }
                    Some(folder) => {
                        catalog.live_folder(folder)?;
                        None
                    }
                    None => None,
                }
            };

            let mut matched_folders: Vec<FolderSearchHit> = Vec::new();
            if !filter.starred && filter.tag.is_none() {
                for (id, folder) in &catalog.folders {
                    let in_trash = catalog.folder_trashed(id);
                    if in_trash != trash {
                        continue;
                    }
                    if trash && catalog.folder_trashed(&folder.parent) {
                        continue;
                    }
                    if !trash
                        && let Some(scope) = &within
                        && (!scope.contains(id) || filter.folder.as_deref() == Some(id.as_str()))
                    {
                        continue;
                    }
                    let view = catalog.folder_view(id, folder);
                    if parsed.is_empty() {
                        matched_folders.push(FolderSearchHit {
                            folder: view,
                            name_spans: Vec::new(),
                            path_spans: Vec::new(),
                            score: 1,
                        });
                        continue;
                    }
                    let path_joined = view.path.join(" / ");
                    let fields = [
                        WeightedField {
                            name: "name",
                            weight: 100,
                            text: &view.name,
                        },
                        WeightedField {
                            name: "path",
                            weight: 70,
                            text: &path_joined,
                        },
                    ];
                    if let Some(m) = parsed.match_fields(&fields) {
                        let name_spans = if m.field_ranges[0].is_empty() {
                            Vec::new()
                        } else {
                            highlight_spans(&view.name, &m.field_ranges[0])
                        };
                        let path_spans = if m.field_ranges[1].is_empty() {
                            Vec::new()
                        } else {
                            highlight_spans(&path_joined, &m.field_ranges[1])
                        };
                        matched_folders.push(FolderSearchHit {
                            folder: view,
                            name_spans,
                            path_spans,
                            score: m.score,
                        });
                    }
                }
            }

            let mut matched_files: Vec<RawFileMatch> = Vec::new();
            for (id, file) in &catalog.files {
                let in_trash = catalog.file_trashed(file);
                if in_trash != trash {
                    continue;
                }
                if trash && catalog.folder_trashed(&file.folder) {
                    continue;
                }
                if !trash
                    && let Some(scope) = &within
                    && !scope.contains(&file.folder)
                {
                    continue;
                }
                if filter.starred && !file.starred {
                    continue;
                }
                if filter
                    .tag
                    .as_ref()
                    .is_some_and(|tag| !file.tags.contains(tag))
                {
                    continue;
                }
                if parsed.is_empty() {
                    matched_files.push(RawFileMatch {
                        id: id.clone(),
                        score: 1,
                        name_ranges: Vec::new(),
                        path_ranges: Vec::new(),
                        match_field: None,
                        match_value: None,
                    });
                    continue;
                }
                let path_vec = catalog.path(&file.folder);
                let path_joined = path_vec.join(" / ");
                let mime_hint = mime_aliases(&file.mime);
                let mut fields: Vec<WeightedField<'_>> = Vec::with_capacity(4 + file.tags.len());
                fields.push(WeightedField {
                    name: "name",
                    weight: 100,
                    text: &file.name,
                });
                fields.push(WeightedField {
                    name: "path",
                    weight: 70,
                    text: &path_joined,
                });
                let tag_start = fields.len();
                for tag in &file.tags {
                    fields.push(WeightedField {
                        name: "tag",
                        weight: 85,
                        text: tag,
                    });
                }
                fields.push(WeightedField {
                    name: "note",
                    weight: 60,
                    text: &file.note,
                });
                fields.push(WeightedField {
                    name: "type",
                    weight: 45,
                    text: mime_hint,
                });
                if let Some(m) = parsed.match_fields(&fields) {
                    let mut score = m.score;
                    if file.starred {
                        score = score.saturating_add(12);
                    }
                    let mut match_value = None;
                    if m.matched_field == Some("tag") {
                        for (i, tag) in file.tags.iter().enumerate() {
                            if !m.field_ranges[tag_start + i].is_empty() {
                                match_value = Some(tag.clone());
                                break;
                            }
                        }
                    } else if m.matched_field == Some("path") && !path_joined.is_empty() {
                        match_value = Some(path_joined);
                    }
                    matched_files.push(RawFileMatch {
                        id: id.clone(),
                        score,
                        name_ranges: m.field_ranges[0].clone(),
                        path_ranges: m.field_ranges[1].clone(),
                        match_field: m.matched_field.map(str::to_string),
                        match_value,
                    });
                }
            }
            (matched_folders, matched_files)
        };

        let mut folders = folder_hits;
        if parsed.is_empty() {
            folders.sort_by_key(|a| a.folder.name.to_lowercase());
        } else {
            folders.sort_by(|a, b| {
                b.score.cmp(&a.score).then_with(|| {
                    a.folder
                        .name
                        .to_lowercase()
                        .cmp(&b.folder.name.to_lowercase())
                })
            });
        }

        let mut files = Vec::with_capacity(file_ids.len());
        for RawFileMatch {
            id,
            score,
            name_ranges,
            path_ranges,
            match_field,
            match_value,
        } in file_ids
        {
            let view = self.file_view(&id)?;
            let name_spans = if name_ranges.is_empty() {
                Vec::new()
            } else {
                highlight_spans(&view.name, &name_ranges)
            };
            let path_joined = view.path.join(" / ");
            let path_spans = if path_ranges.is_empty() {
                Vec::new()
            } else {
                highlight_spans(&path_joined, &path_ranges)
            };
            files.push(FileSearchHit {
                file: view,
                name_spans,
                path_spans,
                match_field,
                match_value,
                score,
            });
        }
        if parsed.is_empty() {
            files.sort_by(|a, b| {
                b.file
                    .updated_at
                    .cmp(&a.file.updated_at)
                    .then_with(|| a.file.name.cmp(&b.file.name))
            });
        } else {
            files.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| b.file.updated_at.cmp(&a.file.updated_at))
                    .then_with(|| a.file.name.cmp(&b.file.name))
            });
        }

        Ok(FileSearchListing { folders, files })
    }

    /// Search known channels by channel name, member display names, member IDs, drafts, and previews.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn channel_list_search(&self, query: &str) -> Result<Vec<ChannelSearchHit>> {
        let parsed = Query::parse(query);
        let channels = self.channel_list()?;
        if parsed.is_empty() {
            return Ok(channels
                .into_iter()
                .map(|channel| ChannelSearchHit {
                    channel,
                    name_spans: Vec::new(),
                    match_field: None,
                    match_value: None,
                    score: 1,
                })
                .collect());
        }
        let names = self.cached_display_names();
        let me = self.session_state().account;
        let mut hits = Vec::new();
        for channel in channels {
            let mut member_texts: Vec<(String, String)> = Vec::new();
            if let Some(ledger) = &channel.ledger {
                for account in ledger.members.keys() {
                    if me.as_deref() == Some(account.as_str()) && ledger.members.len() > 1 {
                        continue;
                    }
                    let display = names.get(account).cloned().unwrap_or_default();
                    member_texts.push((account.clone(), display));
                }
            }
            let last_text = channel
                .last
                .as_ref()
                .map(|l| l.text.as_str())
                .unwrap_or_default();
            let mut fields: Vec<WeightedField<'_>> = Vec::with_capacity(3 + member_texts.len() * 2);
            fields.push(WeightedField {
                name: "name",
                weight: 100,
                text: &channel.name,
            });
            let member_start = fields.len();
            for (acc, disp) in &member_texts {
                fields.push(WeightedField {
                    name: "member",
                    weight: 88,
                    text: disp,
                });
                fields.push(WeightedField {
                    name: "member_id",
                    weight: 60,
                    text: acc,
                });
            }
            fields.push(WeightedField {
                name: "draft",
                weight: 50,
                text: &channel.draft,
            });
            fields.push(WeightedField {
                name: "preview",
                weight: 45,
                text: last_text,
            });

            if let Some(m) = parsed.match_fields(&fields) {
                let name_spans = if m.field_ranges[0].is_empty() {
                    Vec::new()
                } else {
                    highlight_spans(&channel.name, &m.field_ranges[0])
                };
                let mut match_field = m.matched_field.map(str::to_string);
                let mut match_value = None;
                if matches!(m.matched_field, Some("member" | "member_id")) {
                    match_field = Some("member".into());
                    for (idx, (acc, disp)) in member_texts.iter().enumerate() {
                        let d_idx = member_start + idx * 2;
                        if !m.field_ranges[d_idx].is_empty()
                            || !m.field_ranges[d_idx + 1].is_empty()
                        {
                            match_value = Some(if disp.is_empty() {
                                acc.clone()
                            } else {
                                disp.clone()
                            });
                            break;
                        }
                    }
                }
                let score = m.score.saturating_add(if channel.hidden { 0 } else { 15 });
                hits.push(ChannelSearchHit {
                    channel,
                    name_spans,
                    match_field,
                    match_value,
                    score,
                });
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.channel.active_at.cmp(&a.channel.active_at))
                .then_with(|| a.channel.id.cmp(&b.channel.id))
        });
        Ok(hits)
    }

    /// Rich single-channel message search using LDK-sealed search buckets, pinyin/fuzzy matching,
    /// sender display names, optional attachment/sender filters, centered snippet highlights,
    /// and `before` pagination.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked or `Error::InvalidInput` for an invalid query/channel.
    pub fn channel_search_rich(
        &self,
        channel: &str,
        query: &str,
        before: Option<u64>,
        limit: usize,
        has_file: bool,
        sender: Option<&str>,
    ) -> Result<ChannelMessageSearchPage> {
        let trimmed = query.trim();
        ensure_valid(
            (!trimmed.is_empty() || has_file || sender.is_some()) && trimmed.chars().count() <= 200,
            "query",
        )?;
        let limit = limit.clamp(1, 200);
        let parsed = Query::parse(trimmed);
        let cursor = before.unwrap_or(u64::MAX);
        let names = self.cached_display_names();
        let blocked: HashSet<String> = self
            .blocked_accounts()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let indexed = self.ensure_channel_search_index(channel)?;

        let mut hits = Vec::with_capacity(limit + 1);
        for cached in indexed {
            if cached.entry.seq >= cursor {
                continue;
            }
            if blocked.contains(&cached.entry.account) {
                continue;
            }
            if has_file && cached.entry.file_name.is_none() {
                continue;
            }
            if let Some(target_sender) = sender
                && cached.entry.account != target_sender
            {
                continue;
            }
            let sender_name = names.get(&cached.entry.account).cloned();

            let (score, text_ranges, file_ranges, text_matched) = if parsed.is_empty() {
                (100, Vec::new(), Vec::new(), true)
            } else {
                let prepared_sender = sender_name
                    .as_deref()
                    .map(PreparedText::new)
                    .unwrap_or_else(|| PreparedText::new(""));
                let empty_file = PreparedText::new("");
                let file_ref = cached.prepared_file.as_ref().unwrap_or(&empty_file);
                let fields = [
                    ("text", 100u32, &cached.prepared_text),
                    ("file", 95u32, file_ref),
                    ("sender", 75u32, &prepared_sender),
                ];
                let Some(doc_match) = parsed.match_prepared_fields(&fields) else {
                    continue;
                };
                let text_matched =
                    !doc_match.field_ranges[0].is_empty() || !doc_match.field_ranges[2].is_empty();
                (
                    doc_match.score,
                    doc_match.field_ranges[0].clone(),
                    doc_match.field_ranges[1].clone(),
                    text_matched,
                )
            };

            let Some(message) = self.channel_message_public(channel, cached.entry.seq)? else {
                continue;
            };
            if message.retracted || message.hidden || message.unreadable {
                continue;
            }
            if !text_matched && message.file_deleted {
                continue;
            }
            if has_file && (message.file.is_none() || message.file_deleted) {
                continue;
            }

            let snippet = if !message.text.is_empty() {
                snippet_spans(&message.text, &text_ranges, 110)
            } else {
                Vec::new()
            };
            let file_spans = message
                .file
                .as_ref()
                .filter(|_| !message.file_deleted)
                .map(|f| highlight_spans(&f.name, &file_ranges))
                .unwrap_or_default();

            hits.push(MessageSearchHit {
                channel: channel.to_string(),
                message,
                sender_name,
                snippet_spans: snippet,
                file_spans,
                score,
            });
            if hits.len() > limit {
                break;
            }
        }

        let has_more = hits.len() > limit;
        if has_more {
            hits.truncate(limit);
        }
        let next_before = has_more.then(|| hits.last().map_or(0, |h| h.message.seq));
        Ok(ChannelMessageSearchPage {
            hits,
            has_more,
            next_before,
        })
    }

    /// Search messages across all non-hidden channels in a single call using LDK-sealed buckets.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked or `Error::InvalidInput` for an empty/oversized query.
    pub fn channel_search_all(
        &self,
        query: &str,
        limit_per_channel: usize,
        max_channels: usize,
        has_file: bool,
    ) -> Result<Vec<ChannelSearchGroup>> {
        let trimmed = query.trim();
        ensure_valid(
            (!trimmed.is_empty() || has_file) && trimmed.chars().count() <= 200,
            "query",
        )?;
        let limit_per_channel = limit_per_channel.clamp(1, 50);
        let max_channels = max_channels.clamp(1, 100);
        let mut channels = self.channel_list()?;
        channels.retain(|c| !c.hidden);
        channels.sort_by_key(|a| std::cmp::Reverse(a.active_at));

        let mut groups = Vec::new();
        for ch in channels {
            let page =
                self.channel_search_rich(&ch.id, trimmed, None, limit_per_channel, has_file, None)?;
            if page.hits.is_empty() {
                continue;
            }
            let top_score = page.hits.iter().map(|h| h.score).max().unwrap_or(0);
            groups.push(ChannelSearchGroup {
                channel_id: ch.id,
                channel_name: ch.name,
                hits: page.hits,
                next_before: page.next_before,
                top_score,
            });
            if groups.len() >= max_channels {
                break;
            }
        }
        groups.sort_by(|a, b| {
            b.top_score.cmp(&a.top_score).then_with(|| {
                let ta = a.hits.first().map_or(0, |h| h.message.stored_at);
                let tb = b.hits.first().map_or(0, |h| h.message.stored_at);
                tb.cmp(&ta)
            })
        });
        Ok(groups)
    }

    /// Search incoming, archived, sent, pending, and draft letters by display name, Account ID,
    /// and body text using the unified pinyin/multi-token matcher.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn inbox_search(
        &self,
        query: &str,
        box_kind: Option<&str>,
    ) -> Result<Vec<LetterSearchHit>> {
        let parsed = Query::parse(query);
        let names = self.cached_display_names();
        let blocked: HashSet<String> = self
            .blocked_accounts()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let mut hits = Vec::new();

        struct LetterMatch {
            score: u32,
            display: Option<String>,
            name_spans: Vec<HighlightSpan>,
            snippet: Vec<HighlightSpan>,
        }
        let match_letter = |account: &str, text: &str| -> Option<LetterMatch> {
            let display = names.get(account).cloned();
            let shown_name = display.as_deref().unwrap_or(account);
            if parsed.is_empty() {
                return Some(LetterMatch {
                    score: 1,
                    display,
                    name_spans: Vec::new(),
                    snippet: if text.is_empty() {
                        Vec::new()
                    } else {
                        snippet_spans(text, &[], 100)
                    },
                });
            }
            let fields = [
                WeightedField {
                    name: "name",
                    weight: 100,
                    text: shown_name,
                },
                WeightedField {
                    name: "account",
                    weight: 70,
                    text: account,
                },
                WeightedField {
                    name: "text",
                    weight: 85,
                    text,
                },
            ];
            let m = parsed.match_fields(&fields)?;
            let name_spans = if !m.field_ranges[0].is_empty() {
                highlight_spans(shown_name, &m.field_ranges[0])
            } else if display.is_none() && !m.field_ranges[1].is_empty() {
                highlight_spans(account, &m.field_ranges[1])
            } else {
                Vec::new()
            };
            let snippet = if !text.is_empty() {
                snippet_spans(text, &m.field_ranges[2], 100)
            } else {
                Vec::new()
            };
            Some(LetterMatch {
                score: m.score,
                display,
                name_spans,
                snippet,
            })
        };

        // Incoming (inbox + archive)
        if matches!(box_kind, None | Some("all" | "inbox" | "archive")) {
            for letter in self.inbox_cached()? {
                if blocked.contains(&letter.sender) {
                    continue;
                }
                let kind = if letter.archived { "archive" } else { "inbox" };
                if let Some(filter_box) = box_kind
                    && filter_box != "all"
                    && filter_box != kind
                {
                    continue;
                }
                if let Some(m) = match_letter(&letter.sender, &letter.text) {
                    hits.push(LetterSearchHit {
                        id: letter.order_id,
                        box_kind: kind.into(),
                        account: letter.sender,
                        display_name: m.display,
                        name_spans: m.name_spans,
                        snippet_spans: m.snippet,
                        created_at: letter.created_at,
                        read: letter.read,
                        archived: letter.archived,
                        state: letter.state,
                        score: m.score,
                    });
                }
            }
        }

        // Sent & pending history
        if matches!(box_kind, None | Some("all" | "sent" | "pending")) {
            for row in self.letter_history()? {
                let pending = row["pending"].as_bool().unwrap_or(false);
                let kind = if pending { "pending" } else { "sent" };
                if let Some(filter_box) = box_kind
                    && filter_box != "all"
                    && filter_box != kind
                {
                    continue;
                }
                let id = row["id"].as_str().unwrap_or_default().to_string();
                let recipient = row["recipient"].as_str().unwrap_or_default().to_string();
                let text = row["text"].as_str().unwrap_or_default();
                let created_at = row["created_at"].as_u64().unwrap_or(0);
                let state = row["state"].as_str().unwrap_or("unknown").to_string();
                if let Some(m) = match_letter(&recipient, text) {
                    hits.push(LetterSearchHit {
                        id,
                        box_kind: kind.into(),
                        account: recipient,
                        display_name: m.display,
                        name_spans: m.name_spans,
                        snippet_spans: m.snippet,
                        created_at,
                        read: true,
                        archived: false,
                        state,
                        score: m.score,
                    });
                }
            }
        }

        // Local drafts
        if matches!(box_kind, None | Some("all" | "draft" | "drafts")) {
            for draft in self.letter_drafts()? {
                if let Some(m) = match_letter(&draft.recipient, &draft.text) {
                    hits.push(LetterSearchHit {
                        id: draft.id,
                        box_kind: "draft".into(),
                        account: draft.recipient,
                        display_name: m.display,
                        name_spans: m.name_spans,
                        snippet_spans: m.snippet,
                        created_at: draft.updated_at,
                        read: true,
                        archived: false,
                        state: "draft".into(),
                        score: m.score,
                    });
                }
            }
        }

        if parsed.is_empty() {
            hits.sort_by(|a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
        } else {
            hits.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| b.created_at.cmp(&a.created_at))
                    .then_with(|| a.id.cmp(&b.id))
            });
        }
        Ok(hits)
    }
}
