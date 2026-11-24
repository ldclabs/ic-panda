//! Unified in-memory fullscreen file preview classification, bounded safe
//! Markdown AST parsing, and preview payload types shared across Desktop,
//! iOS, and Android.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroizing;

/// Largest raster image decoded in full for fullscreen preview (20 MiB).
pub const MAX_PREVIEW_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// Largest Markdown document parsed for fullscreen preview (2 MiB).
pub const MAX_PREVIEW_MARKDOWN_BYTES: u64 = 2 * 1024 * 1024;
/// Largest plain-text document returned for fullscreen preview (1 MiB).
pub const MAX_PREVIEW_TEXT_BYTES: u64 = 1024 * 1024;

const MAX_MARKDOWN_DEPTH: usize = 16;
const MAX_MARKDOWN_BLOCKS: usize = 5_000;

/// Preview category resolved from a file's MIME type and filename extension.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewClass {
    /// Supported raster image (excluding SVG).
    Image,
    /// Markdown document.
    Markdown,
    /// Plain text or structured text file.
    Text,
    /// Not natively previewable in-app; displays file metadata.
    Unsupported,
}

/// Why a file fell back to metadata display instead of rendering content.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    /// File type is not supported for in-app preview.
    UnsupportedType,
    /// File exceeds the size budget for its preview category.
    TooLarge,
    /// Library version has no local chunks (cloud-only).
    CloudOnly,
    /// Text or image bytes could not be decoded safely.
    DecodeFailed,
    /// File was trashed, deleted, or retracted.
    Unavailable,
}

/// Column alignment for GFM tables.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TableAlignment {
    /// Default alignment.
    None,
    /// Left-aligned column.
    Left,
    /// Center-aligned column.
    Center,
    /// Right-aligned column.
    Right,
}

/// Safe, bounded Markdown document tree produced from untrusted input.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MarkdownDocument {
    /// Top-level blocks in reading order.
    pub blocks: Vec<MarkdownBlock>,
    /// True when parsing stopped early due to block or nesting limits.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// A block-level Markdown node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarkdownBlock {
    /// Heading (`#`..`######`).
    Heading {
        /// Heading level 1..=6.
        level: u8,
        /// Inline spans inside the heading.
        inlines: Vec<MarkdownInline>,
    },
    /// Paragraph block.
    Paragraph {
        /// Inline spans inside the paragraph.
        inlines: Vec<MarkdownInline>,
    },
    /// Fenced or indented code block.
    CodeBlock {
        /// Optional language info string (first token).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        /// Literal code text.
        code: String,
    },
    /// Block quote containing nested blocks.
    BlockQuote {
        /// Nested blocks inside the quote.
        blocks: Vec<MarkdownBlock>,
    },
    /// Ordered or unordered list.
    List {
        /// True for ordered lists (`1.`), false for bullet lists (`-`).
        ordered: bool,
        /// Starting number for ordered lists (1 for unordered).
        start: u64,
        /// List items.
        items: Vec<MarkdownListItem>,
    },
    /// GFM table.
    Table {
        /// Per-column alignments.
        alignments: Vec<TableAlignment>,
        /// Header cells.
        headers: Vec<Vec<MarkdownInline>>,
        /// Body rows.
        rows: Vec<Vec<Vec<MarkdownInline>>>,
    },
    /// Horizontal rule (`---`).
    ThematicBreak,
}

/// One list item, optionally carrying a GFM task checkbox state.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MarkdownListItem {
    /// Task list checkbox state, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// Blocks inside the list item.
    pub blocks: Vec<MarkdownBlock>,
}

/// An inline Markdown node.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarkdownInline {
    /// Plain text run.
    Text {
        /// Unescaped text content.
        text: String,
    },
    /// Bold / strong emphasis.
    Strong {
        /// Child inline nodes.
        children: Vec<MarkdownInline>,
    },
    /// Italic / emphasis.
    Emphasis {
        /// Child inline nodes.
        children: Vec<MarkdownInline>,
    },
    /// GFM strikethrough.
    Strikethrough {
        /// Child inline nodes.
        children: Vec<MarkdownInline>,
    },
    /// Inline code span.
    Code {
        /// Literal code text.
        code: String,
    },
    /// Validated link (`https:`, `http:`, or `mailto:`).
    Link {
        /// Validated destination URL.
        url: String,
        /// Child inline nodes.
        children: Vec<MarkdownInline>,
    },
    /// Blocked external image placeholder (never fetched automatically).
    BlockedImage {
        /// Alt text describing the blocked image.
        alt: String,
    },
    /// Soft line break.
    SoftBreak,
    /// Hard line break.
    HardBreak,
}

/// Metadata summary describing a previewed file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PreviewMeta {
    /// Resolved preview class.
    pub kind: PreviewClass,
    /// Effective MIME type.
    pub mime: String,
    /// File name.
    pub name: String,
    /// File byte size.
    pub size: u64,
    /// Verified SHA-256 hex digest when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Full verified file preview returned in memory (never written to temp files).
#[derive(Clone, Debug)]
pub struct FullPreview {
    /// Metadata describing the file.
    pub meta: PreviewMeta,
    /// Decoded preview payload.
    pub content: PreviewContent,
}

/// Decoded in-memory preview body.
#[derive(Clone, Debug)]
pub enum PreviewContent {
    /// Verified raw image bytes (zeroized on drop).
    Image(Zeroizing<Vec<u8>>),
    /// Parsed safe Markdown AST and original UTF-8 text.
    Markdown {
        /// Parsed safe AST.
        document: MarkdownDocument,
        /// Original UTF-8 source text.
        raw: String,
    },
    /// Verified UTF-8 plain text.
    Text {
        /// Decoded text.
        text: String,
    },
    /// Not rendered; show metadata card with reason.
    Fallback {
        /// Why the file fell back to metadata.
        reason: FallbackReason,
    },
}

/// JSON envelope returned for non-image previews (Markdown, Text, Fallback).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PreviewJson {
    /// Preview metadata.
    #[serde(flatten)]
    pub meta: PreviewMeta,
    /// Parsed Markdown document when `kind == Markdown` and rendered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<MarkdownDocument>,
    /// Raw UTF-8 text for Markdown or Text previews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Fallback reason when content is not rendered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<FallbackReason>,
}

/// Lightweight entry for stepping through attachments inside a channel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChannelAttachmentItem {
    /// Message sequence number inside the channel.
    pub seq: u64,
    /// Message digest.
    pub digest: String,
    /// Sender account ID.
    pub account: String,
    /// Message creation timestamp (ms).
    pub created_at: u64,
    /// Relay storage timestamp (ms).
    pub stored_at: u64,
    /// Attachment file name.
    pub name: String,
    /// Effective MIME type.
    pub mime: String,
    /// Byte length.
    pub size: u64,
    /// Resolved preview class.
    pub preview_class: PreviewClass,
    /// True when the attachment is within its in-app preview size limit.
    pub previewable: bool,
}

impl FullPreview {
    /// Build a metadata-only fallback response.
    pub fn fallback(
        mime: String,
        name: String,
        size: u64,
        sha256: Option<String>,
        reason: FallbackReason,
    ) -> Self {
        let mime = effective_mime(&mime, &name);
        let kind = classify(&mime, &name);
        Self {
            meta: PreviewMeta {
                kind,
                mime,
                name,
                size,
                sha256,
            },
            content: PreviewContent::Fallback { reason },
        }
    }

    /// Convert non-image content into a serializable JSON payload.
    pub fn to_json(&self) -> PreviewJson {
        match &self.content {
            PreviewContent::Markdown { document, raw } => PreviewJson {
                meta: self.meta.clone(),
                document: Some(document.clone()),
                text: Some(raw.clone()),
                reason: None,
            },
            PreviewContent::Text { text } => PreviewJson {
                meta: self.meta.clone(),
                document: None,
                text: Some(text.clone()),
                reason: None,
            },
            PreviewContent::Fallback { reason } => PreviewJson {
                meta: self.meta.clone(),
                document: None,
                text: None,
                reason: Some(*reason),
            },
            PreviewContent::Image(_) => PreviewJson {
                meta: self.meta.clone(),
                document: None,
                text: None,
                reason: None,
            },
        }
    }
}

/// Extract a lowercase ASCII extension (`1..=16` alphanumeric characters).
pub fn extension_of(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    if ext.is_empty() || ext.len() > 16 || !ext.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Normalize a MIME string by stripping parameters and lowercasing.
pub fn normalize_mime(mime: &str) -> String {
    mime.split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// Infer a concrete MIME type when metadata is empty or `application/octet-stream`.
pub fn effective_mime(mime: &str, name: &str) -> String {
    let base = normalize_mime(mime);
    if !base.is_empty() && base != "application/octet-stream" {
        return base;
    }
    match extension_of(name).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        Some("heic") => "image/heic",
        Some("heif") => "image/heif",
        Some("svg") => "image/svg+xml",
        Some("md" | "markdown") => "text/markdown",
        Some("txt" | "log") => "text/plain",
        Some("csv") => "text/csv",
        Some("json") => "application/json",
        Some("yaml" | "yml") => "application/yaml",
        Some("toml") => "application/toml",
        Some("xml") => "application/xml",
        Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("ts" | "tsx" | "jsx") => "text/typescript",
        Some("rs") => "text/x-rust",
        Some("py") => "text/x-python",
        Some("sh") => "text/x-shellscript",
        Some("sql") => "application/sql",
        _ if !base.is_empty() => return base,
        _ => "application/octet-stream",
    }
    .into()
}

/// Classify a file into its fullscreen preview category.
///
/// SVG (`image/svg+xml`) is intentionally excluded from image preview to prevent
/// untrusted script or external resource execution.
pub fn classify(mime: &str, name: &str) -> PreviewClass {
    let base = normalize_mime(mime);
    let ext = extension_of(name);
    if base == "image/svg+xml" || (ext.as_deref() == Some("svg") && base.is_empty()) {
        return PreviewClass::Unsupported;
    }
    if matches!(
        base.as_str(),
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/bmp"
            | "image/avif"
            | "image/heic"
            | "image/heif"
    ) {
        return PreviewClass::Image;
    }
    if matches!(base.as_str(), "" | "application/octet-stream")
        && matches!(
            ext.as_deref(),
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif" | "heic" | "heif")
        )
    {
        return PreviewClass::Image;
    }
    if matches!(base.as_str(), "text/markdown" | "text/x-markdown") {
        return PreviewClass::Markdown;
    }
    if matches!(ext.as_deref(), Some("md" | "markdown"))
        && matches!(
            base.as_str(),
            "" | "application/octet-stream" | "text/plain" | "text/markdown" | "text/x-markdown"
        )
    {
        return PreviewClass::Markdown;
    }
    if base.starts_with("text/")
        || matches!(
            base.as_str(),
            "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/toml"
                | "application/javascript"
                | "application/typescript"
                | "application/sql"
        )
    {
        return PreviewClass::Text;
    }
    if matches!(base.as_str(), "" | "application/octet-stream")
        && matches!(
            ext.as_deref(),
            Some(
                "txt"
                    | "log"
                    | "csv"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "toml"
                    | "xml"
                    | "html"
                    | "htm"
                    | "css"
                    | "js"
                    | "mjs"
                    | "ts"
                    | "tsx"
                    | "jsx"
                    | "rs"
                    | "py"
                    | "sh"
                    | "sql"
                    | "ini"
                    | "conf"
                    | "go"
                    | "java"
                    | "kt"
                    | "swift"
                    | "c"
                    | "h"
                    | "cpp"
                    | "hpp"
            )
        )
    {
        return PreviewClass::Text;
    }
    PreviewClass::Unsupported
}

/// Maximum byte size allowed for in-app preview of `class`.
pub const fn size_limit(class: PreviewClass) -> Option<u64> {
    match class {
        PreviewClass::Image => Some(MAX_PREVIEW_IMAGE_BYTES),
        PreviewClass::Markdown => Some(MAX_PREVIEW_MARKDOWN_BYTES),
        PreviewClass::Text => Some(MAX_PREVIEW_TEXT_BYTES),
        PreviewClass::Unsupported => None,
    }
}

/// Whether a file with `(mime, name, size)` can be previewed in-app.
pub fn is_previewable(mime: &str, name: &str, size: u64) -> bool {
    let class = classify(mime, name);
    size_limit(class).is_some_and(|limit| size <= limit)
}

/// Decode UTF-8 (optional BOM) or UTF-16LE/BE (with BOM) text, rejecting NUL bytes.
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    let text = if bytes.starts_with(&[0xFF, 0xFE]) {
        let body = &bytes[2..];
        if !body.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16(&units).ok()?
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let body = &bytes[2..];
        if !body.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        String::from_utf16(&units).ok()?
    } else {
        let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        std::str::from_utf8(body).ok()?.to_string()
    };
    if text.contains('\0') {
        return None;
    }
    Some(text)
}

/// Validate a Markdown link URL against the allowed external schemes.
pub fn safe_link_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = Url::parse(trimmed).ok()?;
    match parsed.scheme() {
        "https" | "http" | "mailto" => Some(parsed.to_string()),
        _ => None,
    }
}

/// Build a [`FullPreview`] from verified in-memory bytes.
pub fn from_verified_bytes(
    mime: String,
    name: String,
    size: u64,
    sha256: Option<String>,
    bytes: Zeroizing<Vec<u8>>,
) -> FullPreview {
    let mime = effective_mime(&mime, &name);
    let kind = classify(&mime, &name);
    let content = match kind {
        PreviewClass::Image => PreviewContent::Image(bytes),
        PreviewClass::Markdown => match decode_text(&bytes) {
            Some(raw) => {
                let document = parse_markdown(&raw);
                PreviewContent::Markdown { document, raw }
            }
            None => PreviewContent::Fallback {
                reason: FallbackReason::DecodeFailed,
            },
        },
        PreviewClass::Text => match decode_text(&bytes) {
            Some(text) => PreviewContent::Text { text },
            None => PreviewContent::Fallback {
                reason: FallbackReason::DecodeFailed,
            },
        },
        PreviewClass::Unsupported => PreviewContent::Fallback {
            reason: FallbackReason::UnsupportedType,
        },
    };
    FullPreview {
        meta: PreviewMeta {
            kind,
            mime,
            name,
            size,
            sha256,
        },
        content,
    }
}

enum BlockFrame {
    Root(Vec<MarkdownBlock>),
    Heading {
        level: u8,
        inlines: Vec<MarkdownInline>,
    },
    Paragraph(Vec<MarkdownInline>),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    BlockQuote(Vec<MarkdownBlock>),
    List {
        ordered: bool,
        start: u64,
        items: Vec<MarkdownListItem>,
    },
    ListItem {
        checked: Option<bool>,
        blocks: Vec<MarkdownBlock>,
        pending_inlines: Vec<MarkdownInline>,
    },
    Table {
        alignments: Vec<TableAlignment>,
        headers: Vec<Vec<MarkdownInline>>,
        rows: Vec<Vec<Vec<MarkdownInline>>>,
        in_head: bool,
    },
    TableRow(Vec<Vec<MarkdownInline>>),
    TableCell(Vec<MarkdownInline>),
}

enum InlineFrame {
    Strong(Vec<MarkdownInline>),
    Emphasis(Vec<MarkdownInline>),
    Strikethrough(Vec<MarkdownInline>),
    Link {
        url: Option<String>,
        children: Vec<MarkdownInline>,
    },
    Image(String),
}

fn push_inline_to_block(frame: &mut BlockFrame, node: MarkdownInline) {
    let target = match frame {
        BlockFrame::Heading { inlines, .. }
        | BlockFrame::Paragraph(inlines)
        | BlockFrame::TableCell(inlines) => inlines,
        BlockFrame::ListItem {
            pending_inlines, ..
        } => pending_inlines,
        _ => return,
    };
    if let (Some(MarkdownInline::Text { text: prev }), MarkdownInline::Text { text: next }) =
        (target.last_mut(), &node)
    {
        prev.push_str(next);
    } else {
        target.push(node);
    }
}

fn push_inline(blocks: &mut [BlockFrame], inlines: &mut [InlineFrame], node: MarkdownInline) {
    if let Some(top) = inlines.last_mut() {
        match top {
            InlineFrame::Image(alt) => {
                collect_alt_text(alt, &node);
                return;
            }
            InlineFrame::Strong(children)
            | InlineFrame::Emphasis(children)
            | InlineFrame::Strikethrough(children)
            | InlineFrame::Link { children, .. } => {
                if let (
                    Some(MarkdownInline::Text { text: prev }),
                    MarkdownInline::Text { text: next },
                ) = (children.last_mut(), &node)
                {
                    prev.push_str(next);
                } else {
                    children.push(node);
                }
                return;
            }
        }
    }
    if let Some(frame) = blocks.last_mut() {
        push_inline_to_block(frame, node);
    }
}

fn collect_alt_text(alt: &mut String, node: &MarkdownInline) {
    match node {
        MarkdownInline::Text { text } => alt.push_str(text),
        MarkdownInline::Code { code } => alt.push_str(code),
        MarkdownInline::SoftBreak | MarkdownInline::HardBreak => alt.push(' '),
        MarkdownInline::Strong { children }
        | MarkdownInline::Emphasis { children }
        | MarkdownInline::Strikethrough { children }
        | MarkdownInline::Link { children, .. } => {
            for child in children {
                collect_alt_text(alt, child);
            }
        }
        MarkdownInline::BlockedImage { alt: inner } => alt.push_str(inner),
    }
}

fn flush_item_inlines(frame: &mut BlockFrame, block_count: &mut usize) {
    if let BlockFrame::ListItem {
        blocks,
        pending_inlines,
        ..
    } = frame
        && !pending_inlines.is_empty()
    {
        blocks.push(MarkdownBlock::Paragraph {
            inlines: std::mem::take(pending_inlines),
        });
        *block_count += 1;
    }
}

fn push_block(blocks: &mut [BlockFrame], block: MarkdownBlock, block_count: &mut usize) {
    *block_count += 1;
    if let Some(parent) = blocks.last_mut() {
        match parent {
            BlockFrame::Root(list) | BlockFrame::BlockQuote(list) => list.push(block),
            BlockFrame::ListItem {
                blocks: list,
                pending_inlines,
                ..
            } => {
                if !pending_inlines.is_empty() {
                    list.push(MarkdownBlock::Paragraph {
                        inlines: std::mem::take(pending_inlines),
                    });
                    *block_count += 1;
                }
                list.push(block);
            }
            _ => {}
        }
    }
}

/// Parse untrusted Markdown into a bounded, safe [`MarkdownDocument`].
///
/// Raw HTML is stripped, external images become [`MarkdownInline::BlockedImage`]
/// placeholders without making network requests, and links are restricted to
/// `https:`, `http:`, and `mailto:`.
pub fn parse_markdown(input: &str) -> MarkdownDocument {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(input, options);
    let mut blocks = vec![BlockFrame::Root(Vec::new())];
    let mut inlines: Vec<InlineFrame> = Vec::new();
    let mut block_count = 0usize;
    let mut truncated = false;
    let mut ignored_depth = 0usize;

    for event in parser {
        if block_count >= MAX_MARKDOWN_BLOCKS {
            truncated = true;
            break;
        }
        if ignored_depth > 0 {
            match event {
                Event::Start(_) => ignored_depth += 1,
                Event::End(_) => ignored_depth -= 1,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        blocks.push(BlockFrame::Paragraph(Vec::new()));
                    }
                }
                Tag::Heading { level, .. } => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        let lvl = match level {
                            HeadingLevel::H1 => 1,
                            HeadingLevel::H2 => 2,
                            HeadingLevel::H3 => 3,
                            HeadingLevel::H4 => 4,
                            HeadingLevel::H5 => 5,
                            HeadingLevel::H6 => 6,
                        };
                        blocks.push(BlockFrame::Heading {
                            level: lvl,
                            inlines: Vec::new(),
                        });
                    }
                }
                Tag::CodeBlock(kind) => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        let language = match kind {
                            CodeBlockKind::Fenced(info) => {
                                let token = info.split_whitespace().next().unwrap_or_default();
                                (!token.is_empty()).then(|| token.to_string())
                            }
                            CodeBlockKind::Indented => None,
                        };
                        blocks.push(BlockFrame::CodeBlock {
                            language,
                            code: String::new(),
                        });
                    }
                }
                Tag::BlockQuote(_) => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        blocks.push(BlockFrame::BlockQuote(Vec::new()));
                    }
                }
                Tag::List(start) => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        if let Some(top) = blocks.last_mut() {
                            flush_item_inlines(top, &mut block_count);
                        }
                        blocks.push(BlockFrame::List {
                            ordered: start.is_some(),
                            start: start.unwrap_or(1),
                            items: Vec::new(),
                        });
                    }
                }
                Tag::Item => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        blocks.push(BlockFrame::ListItem {
                            checked: None,
                            blocks: Vec::new(),
                            pending_inlines: Vec::new(),
                        });
                    }
                }
                Tag::Table(aligns) => {
                    if blocks.len() >= MAX_MARKDOWN_DEPTH {
                        truncated = true;
                        ignored_depth = 1;
                    } else {
                        let alignments = aligns
                            .into_iter()
                            .map(|a| match a {
                                Alignment::None => TableAlignment::None,
                                Alignment::Left => TableAlignment::Left,
                                Alignment::Center => TableAlignment::Center,
                                Alignment::Right => TableAlignment::Right,
                            })
                            .collect();
                        blocks.push(BlockFrame::Table {
                            alignments,
                            headers: Vec::new(),
                            rows: Vec::new(),
                            in_head: false,
                        });
                    }
                }
                Tag::TableHead => {
                    if let Some(BlockFrame::Table { in_head, .. }) = blocks.last_mut() {
                        *in_head = true;
                    }
                    blocks.push(BlockFrame::TableRow(Vec::new()));
                }
                Tag::TableRow => {
                    blocks.push(BlockFrame::TableRow(Vec::new()));
                }
                Tag::TableCell => {
                    blocks.push(BlockFrame::TableCell(Vec::new()));
                }
                Tag::Strong => {
                    if inlines.len() < MAX_MARKDOWN_DEPTH {
                        inlines.push(InlineFrame::Strong(Vec::new()));
                    }
                }
                Tag::Emphasis => {
                    if inlines.len() < MAX_MARKDOWN_DEPTH {
                        inlines.push(InlineFrame::Emphasis(Vec::new()));
                    }
                }
                Tag::Strikethrough => {
                    if inlines.len() < MAX_MARKDOWN_DEPTH {
                        inlines.push(InlineFrame::Strikethrough(Vec::new()));
                    }
                }
                Tag::Link { dest_url, .. } => {
                    if inlines.len() < MAX_MARKDOWN_DEPTH {
                        inlines.push(InlineFrame::Link {
                            url: safe_link_url(&dest_url),
                            children: Vec::new(),
                        });
                    }
                }
                Tag::Image { .. } => {
                    if inlines.len() < MAX_MARKDOWN_DEPTH {
                        inlines.push(InlineFrame::Image(String::new()));
                    }
                }
                Tag::HtmlBlock | Tag::MetadataBlock(_) => {
                    ignored_depth = 1;
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if let Some(BlockFrame::Paragraph(inlines)) = blocks.pop() {
                        push_block(
                            &mut blocks,
                            MarkdownBlock::Paragraph { inlines },
                            &mut block_count,
                        );
                    }
                }
                TagEnd::Heading(_) => {
                    if let Some(BlockFrame::Heading { level, inlines }) = blocks.pop() {
                        push_block(
                            &mut blocks,
                            MarkdownBlock::Heading { level, inlines },
                            &mut block_count,
                        );
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some(BlockFrame::CodeBlock { language, code }) = blocks.pop() {
                        let code = code.strip_suffix('\n').unwrap_or(&code).to_string();
                        push_block(
                            &mut blocks,
                            MarkdownBlock::CodeBlock { language, code },
                            &mut block_count,
                        );
                    }
                }
                TagEnd::BlockQuote(_) => {
                    if let Some(BlockFrame::BlockQuote(inner)) = blocks.pop() {
                        push_block(
                            &mut blocks,
                            MarkdownBlock::BlockQuote { blocks: inner },
                            &mut block_count,
                        );
                    }
                }
                TagEnd::List(_) => {
                    if let Some(BlockFrame::List {
                        ordered,
                        start,
                        items,
                    }) = blocks.pop()
                    {
                        push_block(
                            &mut blocks,
                            MarkdownBlock::List {
                                ordered,
                                start,
                                items,
                            },
                            &mut block_count,
                        );
                    }
                }
                TagEnd::Item => {
                    if let Some(mut item_frame) = blocks.pop() {
                        flush_item_inlines(&mut item_frame, &mut block_count);
                        if let BlockFrame::ListItem {
                            checked,
                            blocks: item_blocks,
                            ..
                        } = item_frame
                            && let Some(BlockFrame::List { items, .. }) = blocks.last_mut()
                        {
                            items.push(MarkdownListItem {
                                checked,
                                blocks: item_blocks,
                            });
                        }
                    }
                }
                TagEnd::TableCell => {
                    if let Some(BlockFrame::TableCell(cell)) = blocks.pop()
                        && let Some(BlockFrame::TableRow(cells)) = blocks.last_mut()
                    {
                        cells.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(BlockFrame::TableRow(cells)) = blocks.pop()
                        && let Some(BlockFrame::Table {
                            headers, in_head, ..
                        }) = blocks.last_mut()
                    {
                        *headers = cells;
                        *in_head = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(BlockFrame::TableRow(cells)) = blocks.pop()
                        && let Some(BlockFrame::Table { rows, .. }) = blocks.last_mut()
                    {
                        rows.push(cells);
                    }
                }
                TagEnd::Table => {
                    if let Some(BlockFrame::Table {
                        alignments,
                        headers,
                        rows,
                        ..
                    }) = blocks.pop()
                    {
                        push_block(
                            &mut blocks,
                            MarkdownBlock::Table {
                                alignments,
                                headers,
                                rows,
                            },
                            &mut block_count,
                        );
                    }
                }
                TagEnd::Strong => {
                    if let Some(InlineFrame::Strong(children)) = inlines.pop() {
                        push_inline(
                            &mut blocks,
                            &mut inlines,
                            MarkdownInline::Strong { children },
                        );
                    }
                }
                TagEnd::Emphasis => {
                    if let Some(InlineFrame::Emphasis(children)) = inlines.pop() {
                        push_inline(
                            &mut blocks,
                            &mut inlines,
                            MarkdownInline::Emphasis { children },
                        );
                    }
                }
                TagEnd::Strikethrough => {
                    if let Some(InlineFrame::Strikethrough(children)) = inlines.pop() {
                        push_inline(
                            &mut blocks,
                            &mut inlines,
                            MarkdownInline::Strikethrough { children },
                        );
                    }
                }
                TagEnd::Link => {
                    if let Some(InlineFrame::Link { url, children }) = inlines.pop() {
                        match url {
                            Some(url) => push_inline(
                                &mut blocks,
                                &mut inlines,
                                MarkdownInline::Link { url, children },
                            ),
                            None => {
                                for child in children {
                                    push_inline(&mut blocks, &mut inlines, child);
                                }
                            }
                        }
                    }
                }
                TagEnd::Image => {
                    if let Some(InlineFrame::Image(alt)) = inlines.pop() {
                        push_inline(
                            &mut blocks,
                            &mut inlines,
                            MarkdownInline::BlockedImage {
                                alt: alt.trim().to_string(),
                            },
                        );
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(BlockFrame::CodeBlock { code, .. }) = blocks.last_mut() {
                    code.push_str(&text);
                } else {
                    push_inline(
                        &mut blocks,
                        &mut inlines,
                        MarkdownInline::Text {
                            text: text.into_string(),
                        },
                    );
                }
            }
            Event::Code(code) => {
                push_inline(
                    &mut blocks,
                    &mut inlines,
                    MarkdownInline::Code {
                        code: code.into_string(),
                    },
                );
            }
            Event::SoftBreak => {
                push_inline(&mut blocks, &mut inlines, MarkdownInline::SoftBreak);
            }
            Event::HardBreak => {
                push_inline(&mut blocks, &mut inlines, MarkdownInline::HardBreak);
            }
            Event::Rule => {
                push_block(&mut blocks, MarkdownBlock::ThematicBreak, &mut block_count);
            }
            Event::TaskListMarker(checked) => {
                if let Some(BlockFrame::ListItem { checked: slot, .. }) = blocks.last_mut() {
                    *slot = Some(checked);
                }
            }
            // Strip raw HTML and inline HTML completely.
            Event::Html(_) | Event::InlineHtml(_) => {}
            _ => {}
        }
    }

    // Drain any frames left open when truncation occurred mid-document.
    while blocks.len() > 1 {
        let frame = blocks.pop().expect("len > 1");
        match frame {
            BlockFrame::Paragraph(inlines) => {
                push_block(
                    &mut blocks,
                    MarkdownBlock::Paragraph { inlines },
                    &mut block_count,
                );
            }
            BlockFrame::Heading { level, inlines } => {
                push_block(
                    &mut blocks,
                    MarkdownBlock::Heading { level, inlines },
                    &mut block_count,
                );
            }
            BlockFrame::CodeBlock { language, code } => {
                push_block(
                    &mut blocks,
                    MarkdownBlock::CodeBlock { language, code },
                    &mut block_count,
                );
            }
            BlockFrame::BlockQuote(inner) => {
                push_block(
                    &mut blocks,
                    MarkdownBlock::BlockQuote { blocks: inner },
                    &mut block_count,
                );
            }
            BlockFrame::List {
                ordered,
                start,
                items,
            } => {
                push_block(
                    &mut blocks,
                    MarkdownBlock::List {
                        ordered,
                        start,
                        items,
                    },
                    &mut block_count,
                );
            }
            BlockFrame::ListItem {
                checked,
                blocks: mut item_blocks,
                pending_inlines,
            } => {
                if !pending_inlines.is_empty() {
                    item_blocks.push(MarkdownBlock::Paragraph {
                        inlines: pending_inlines,
                    });
                }
                if let Some(BlockFrame::List { items, .. }) = blocks.last_mut() {
                    items.push(MarkdownListItem {
                        checked,
                        blocks: item_blocks,
                    });
                }
            }
            _ => {}
        }
    }

    let root = match blocks.pop() {
        Some(BlockFrame::Root(list)) => list,
        _ => Vec::new(),
    };
    MarkdownDocument {
        blocks: root,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_types_and_excludes_svg() {
        assert_eq!(classify("image/png", "photo.png"), PreviewClass::Image);
        assert_eq!(
            classify("application/octet-stream", "shot.webp"),
            PreviewClass::Image
        );
        assert_eq!(classify("image/heic", "IMG_001.HEIC"), PreviewClass::Image);
        assert_eq!(
            classify("image/svg+xml", "vector.svg"),
            PreviewClass::Unsupported
        );
        assert_eq!(classify("", "vector.svg"), PreviewClass::Unsupported);
        assert_eq!(
            classify("text/markdown", "README.md"),
            PreviewClass::Markdown
        );
        assert_eq!(classify("text/plain", "notes.md"), PreviewClass::Markdown);
        assert_eq!(
            classify("application/octet-stream", "guide.markdown"),
            PreviewClass::Markdown
        );
        assert_eq!(
            classify("application/json", "config.json"),
            PreviewClass::Text
        );
        assert_eq!(
            classify("application/pdf", "report.pdf"),
            PreviewClass::Unsupported
        );
        assert_eq!(
            classify("application/zip", "archive.zip"),
            PreviewClass::Unsupported
        );
    }

    #[test]
    fn respects_size_limits() {
        assert!(is_previewable(
            "image/png",
            "a.png",
            MAX_PREVIEW_IMAGE_BYTES
        ));
        assert!(!is_previewable(
            "image/png",
            "a.png",
            MAX_PREVIEW_IMAGE_BYTES + 1
        ));
        assert!(is_previewable(
            "text/markdown",
            "doc.md",
            MAX_PREVIEW_MARKDOWN_BYTES
        ));
        assert!(!is_previewable(
            "text/markdown",
            "doc.md",
            MAX_PREVIEW_MARKDOWN_BYTES + 1
        ));
        assert!(is_previewable(
            "text/plain",
            "a.txt",
            MAX_PREVIEW_TEXT_BYTES
        ));
        assert!(!is_previewable(
            "text/plain",
            "a.txt",
            MAX_PREVIEW_TEXT_BYTES + 1
        ));
    }

    #[test]
    fn decodes_utf8_and_utf16_and_rejects_nul() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFhello").as_deref(), Some("hello"));
        assert_eq!(
            decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]).as_deref(),
            Some("hi")
        );
        assert_eq!(
            decode_text(&[0xFE, 0xFF, 0, b'o', 0, b'k']).as_deref(),
            Some("ok")
        );
        assert!(decode_text(b"bin\0data").is_none());
    }

    #[test]
    fn parses_markdown_safely_without_html_or_external_images() {
        let md = r#"# Title

Hello **bold** *italic* ~~gone~~ `code` [safe](https://dmsg.net) [evil](javascript:alert(1)) ![secret alt](https://tracker.example/pixel.png).

<script>alert("xss")</script>

> Quoted line

- [x] Done
- [ ] Todo

```rust
fn main() {}
```

| Col A | Col B |
| :--- | ---: |
| 1 | 2 |

---
"#;
        let doc = parse_markdown(md);
        assert!(!doc.truncated);
        assert!(matches!(
            &doc.blocks[0],
            MarkdownBlock::Heading { level: 1, .. }
        ));
        if let MarkdownBlock::Paragraph { inlines } = &doc.blocks[1] {
            assert!(inlines.iter().any(|n| matches!(
                n,
                MarkdownInline::Link { url, .. } if url == "https://dmsg.net/"
            )));
            assert!(!inlines.iter().any(
                |n| matches!(n, MarkdownInline::Link { url, .. } if url.starts_with("javascript:"))
            ));
            assert!(inlines.iter().any(|n| matches!(
                n,
                MarkdownInline::BlockedImage { alt } if alt == "secret alt"
            )));
            let serialized = serde_json::to_string(&doc).unwrap();
            assert!(!serialized.contains("<script>"));
            assert!(!serialized.contains("tracker.example"));
        } else {
            panic!("expected paragraph");
        }
        assert!(doc.blocks.iter().any(|b| matches!(
            b,
            MarkdownBlock::List { items, .. }
                if items.len() == 2 && items[0].checked == Some(true) && items[1].checked == Some(false)
        )));
        assert!(doc.blocks.iter().any(|b| matches!(
            b,
            MarkdownBlock::CodeBlock { language: Some(lang), code } if lang == "rust" && code == "fn main() {}"
        )));
        assert!(doc.blocks.iter().any(|b| matches!(
            b,
            MarkdownBlock::Table { alignments, headers, rows }
                if alignments == &[TableAlignment::Left, TableAlignment::Right]
                    && headers.len() == 2
                    && rows.len() == 1
        )));
        assert!(matches!(
            doc.blocks.last(),
            Some(MarkdownBlock::ThematicBreak)
        ));
    }

    #[test]
    fn bounds_deeply_nested_markdown() {
        let deep = "> ".repeat(40) + "leaf";
        let doc = parse_markdown(&deep);
        assert!(doc.truncated);
    }
}
