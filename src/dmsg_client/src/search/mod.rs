//! Local encrypted-workspace search engine shared by Desktop, Android, and iOS.
//!
//! All matching, normalization, Chinese pinyin (full/initials/hybrid/heteronym/fuzzy),
//! Traditional-to-Simplified folding, multi-token AND matching, typo tolerance,
//! relevance ranking, and highlight slicing live here so every client shares one source
//! of truth while respecting SEC01 (no plaintext index on disk; cleared on lock) and
//! V01 (hidden vault fields and OTP secrets never indexed or matched).

pub(crate) mod cache;
mod matcher;
mod queries;
mod t2s;

#[cfg(test)]
mod tests;

pub(crate) use cache::SearchCache;
pub use matcher::{
    DocumentMatch, FieldMatch, HighlightSpan, PreparedText, Query, WeightedField, highlight_spans,
    mime_aliases, search_keys, snippet_spans, vault_kind_aliases,
};
pub use queries::{
    ChannelMessageSearchPage, ChannelSearchGroup, ChannelSearchHit, FileSearchHit,
    FileSearchListing, FolderSearchHit, GlobalChannelHit, GlobalFileHit, GlobalSearchRequest,
    GlobalSearchResponse, GlobalVaultHit, LetterSearchHit, MessageSearchHit, browse_vault_cached,
    global_search,
};
