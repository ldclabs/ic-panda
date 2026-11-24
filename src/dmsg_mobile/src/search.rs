//! Unified mobile search APIs backed by `dmsg_client::search`.

use crate::*;
use dmsg_client::library::SearchFilter;
use dmsg_client::search::{GlobalSearchRequest, global_search};

#[uniffi::export]
impl MobileEngine {
    /// Unified cross-workspace global search across Vault, Channels, Messages, Letters, and Files.
    pub async fn global_search(
        &self,
        query: String,
        scopes: Vec<String>,
        limit: u32,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(query.chars().count() <= 200, "query")?;
        let req = GlobalSearchRequest {
            query,
            scopes,
            limit: Some((limit as usize).clamp(1, 50)),
        };
        self.held_object(move |h| Box::pin(async move { global_search(h, &req).await }))
            .await
    }

    /// Search both folders and files in one call with pinyin, multi-token, path, tag, and trash support.
    pub async fn search_files_listing(
        &self,
        query: String,
        folder: Option<String>,
        starred: bool,
        tag: Option<String>,
        trash: bool,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(query.chars().count() <= 200, "query")?;
        self.object(move |c| {
            Box::pin(async move {
                let filter = SearchFilter {
                    folder: if trash { None } else { folder },
                    starred,
                    tag,
                };
                c.files_search_listing(&query, &filter, trash)
            })
        })
        .await
    }

    /// Tags present across live library files with counts.
    pub async fn file_tags(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.files_tags() })).await
    }

    /// Search channels by channel name, member display names, member IDs, drafts, and previews.
    pub async fn search_channels(&self, query: String) -> Result<ObjectSnapshot> {
        ensure_valid(query.chars().count() <= 200, "query")?;
        self.object(move |c| Box::pin(async move { c.channel_list_search(&query) }))
            .await
    }

    /// Rich single-channel message search with snippet highlights, attachment/sender filters, and pagination.
    pub async fn search_channel_messages(
        &self,
        channel: String,
        query: String,
        before: Option<u64>,
        limit: u32,
        has_file: bool,
        sender: Option<String>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(query.chars().count() <= 200, "query")?;
        self.object(move |c| {
            Box::pin(async move {
                c.channel_search_rich(
                    &channel,
                    &query,
                    before,
                    (limit as usize).clamp(1, 200),
                    has_file,
                    sender.as_deref(),
                )
            })
        })
        .await
    }

    /// Cross-channel message search in a single FFI call using LDK-sealed buckets.
    pub async fn search_messages_all(
        &self,
        query: String,
        limit_per_channel: u32,
        max_channels: u32,
        has_file: bool,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(query.chars().count() <= 200, "query")?;
        self.object(move |c| {
            Box::pin(async move {
                c.channel_search_all(
                    &query,
                    (limit_per_channel as usize).clamp(1, 50),
                    (max_channels as usize).clamp(1, 100),
                    has_file,
                )
            })
        })
        .await
    }

    /// Bidirectional local timeline window around a jumped message sequence.
    pub async fn messages_around(
        &self,
        channel: String,
        seq: u64,
        before: u32,
        after: u32,
    ) -> Result<MessageWindowPage> {
        let generation = self.host.generation();
        let window = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    c.channel_messages_around(&channel, seq, before as usize, after as usize)
                })
            })
            .await?;
        Ok(MessageWindowPage {
            generation,
            items: window.items.into_iter().map(MessageItem::from).collect(),
            anchor: window.anchor,
            has_before: window.has_before,
            has_after: window.has_after,
            next_before: window.next_before,
            next_after: window.next_after,
        })
    }

    /// Load newer messages after `after`, filling a missing page on demand.
    pub async fn messages_after(
        &self,
        channel: String,
        after: u64,
        limit: u32,
    ) -> Result<MessagePage> {
        ensure_valid(limit > 0 && limit <= 200, "page")?;
        let generation = self.host.generation();
        let turn = self.read_turns.lane(&format!("channel:{channel}"));
        let page = self
            .held(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::channel::newer_messages(h, &channel, after, limit as usize).await
                })
            })
            .await?;
        Ok(MessagePage {
            generation,
            items: page.items.into_iter().map(MessageItem::from).collect(),
            has_more: page.has_more,
            next_before: page.next_before,
        })
    }

    /// Search incoming, archived, sent, pending, and draft letters by display name, account ID, or text.
    pub async fn search_inbox(
        &self,
        query: String,
        box_kind: Option<String>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(query.chars().count() <= 200, "query")?;
        self.object(move |c| Box::pin(async move { c.inbox_search(&query, box_kind.as_deref()) }))
            .await
    }
}
