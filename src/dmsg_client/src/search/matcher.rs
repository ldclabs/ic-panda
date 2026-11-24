//! Unified text normalization, Chinese pinyin (full/initials/hybrid/heteronym/fuzzy),
//! Traditional-to-Simplified folding, multi-token matching, typo tolerance, relevance scoring,
//! and pre-sliced highlight span generation.

use super::t2s::fold_traditional;
use icu_normalizer::DecomposingNormalizerBorrowed;
use pinyin::{PinyinMulti, ToPinyinMulti};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A contiguous slice of display text marked as matched or unmatched for UI highlighting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    /// Text slice in original casing and script.
    pub text: String,
    /// Whether this slice matched the user's query.
    pub matched: bool,
}

/// Match result of a single field against a single token or full query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldMatch {
    /// Relevance score (higher is better; 0 means no match).
    pub score: u32,
    /// Matched UTF-8 byte ranges `[start, end)` in the original field text.
    pub ranges: Vec<(usize, usize)>,
}

/// Weighted field input for multi-field document matching.
#[derive(Clone, Copy, Debug)]
pub struct WeightedField<'a> {
    /// Logical field identifier (e.g. `"title"`, `"tag"`, `"notes"`).
    pub name: &'static str,
    /// Relative weight multiplier (e.g. 100 for title, 60 for notes).
    pub weight: u32,
    /// Original field text.
    pub text: &'a str,
}

/// Aggregated result of matching a parsed [`Query`] across a document's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentMatch {
    /// Total relevance score across all query tokens.
    pub score: u32,
    /// Merged matched byte ranges per field index (aligned with input slice).
    pub field_ranges: Vec<Vec<(usize, usize)>>,
    /// Highest-priority non-primary field that contributed a match (when primary fields did not cover all tokens).
    pub matched_field: Option<&'static str>,
}

/// Parsed, normalized multi-token user query.
#[derive(Clone, Debug)]
pub struct Query {
    raw: String,
    tokens: Vec<QueryToken>,
}

#[derive(Clone, Debug)]
struct QueryToken {
    norm: String,
    compact: String,
    ascii_alpha: bool,
    ascii_alnum: bool,
    char_count: usize,
    req_mask: u32,
}

/// Compact inline ASCII pinyin syllable (longest Mandarin syllable is 6 bytes, e.g. `zhuang`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Syl {
    buf: [u8; 6],
    len: u8,
}

impl Syl {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > 6 {
            return None;
        }
        let mut buf = [0u8; 6];
        buf[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            buf,
            len: bytes.len() as u8,
        })
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buf[..usize::from(self.len)]
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).unwrap_or("")
    }
}

/// Inline list of unique ASCII syllables for one Han character (up to 8 variants).
#[derive(Clone, Copy, Debug, Default)]
struct SylList {
    items: [Syl; 8],
    len: u8,
}

impl SylList {
    fn push(&mut self, syl: Syl) {
        let count = usize::from(self.len);
        if count >= self.items.len() {
            return;
        }
        if self.items[..count].contains(&syl) {
            return;
        }
        self.items[count] = syl;
        self.len += 1;
    }

    fn as_slice(&self) -> &[Syl] {
        &self.items[..usize::from(self.len)]
    }
}

/// One normalized character unit with exact byte offsets in the original text.
#[derive(Clone, Copy, Debug)]
struct TextUnit {
    orig_start: usize,
    orig_end: usize,
    folded: char,
    is_word_start: bool,
    is_separator: bool,
    pinyin: Option<PinyinMulti>,
}

/// Pre-analyzed text ready for fast repeated matching (used by in-memory indexes).
#[derive(Clone, Debug)]
pub struct PreparedText {
    units: Vec<TextUnit>,
    norm: String,
    norm_spans: Vec<(usize, usize)>,
    compact: String,
    compact_spans: Vec<(usize, usize)>,
    mask: u32,
    has_han: bool,
}

/// Score constants ordered by match quality.
const SCORE_EXACT: u32 = 1000;
const SCORE_PREFIX: u32 = 920;
const SCORE_WORD_PREFIX: u32 = 860;
const SCORE_LITERAL_SUBSTR: u32 = 780;
const SCORE_COMPACT: u32 = 740;
const SCORE_PINYIN_EXACT: u32 = 820;
const SCORE_PINYIN_PREFIX: u32 = 760;
const SCORE_PINYIN_FULL: u32 = 710;
const SCORE_PINYIN_HYBRID: u32 = 650;
const SCORE_PINYIN_INITIALS: u32 = 610;
const SCORE_ACRONYM: u32 = 520;
const SCORE_FUZZY_PINYIN_FULL: u32 = 470;
const SCORE_FUZZY_PINYIN_HYBRID: u32 = 430;
const SCORE_FUZZY_PINYIN_INITIALS: u32 = 390;
const SCORE_TYPO: u32 = 310;

const MAX_PINYIN_QUERY_BYTES: usize = 64;
const MAX_SKIP_SEPARATORS: usize = 4;

impl Query {
    /// Parse and normalize a user search string into whitespace-separated tokens.
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let mut tokens = Vec::new();
        for part in trimmed.split_whitespace() {
            let norm = normalize_string(part);
            if norm.is_empty() {
                continue;
            }
            let compact: String = norm.chars().filter(|c| c.is_alphanumeric()).collect();
            let ascii_alpha = norm.bytes().all(|b| b.is_ascii_lowercase());
            let ascii_alnum = !compact.is_empty()
                && compact
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
            let char_count = norm.chars().count();
            let req_mask = query_required_mask(&norm);
            tokens.push(QueryToken {
                norm,
                compact,
                ascii_alpha,
                ascii_alnum,
                char_count,
                req_mask,
            });
            if tokens.len() >= 12 {
                break;
            }
        }
        Self {
            raw: trimmed.to_string(),
            tokens,
        }
    }

    /// Whether the query has no searchable tokens.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// The original trimmed query text.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Number of parsed tokens.
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Match this query against a single raw text string.
    pub fn match_text(&self, text: &str) -> Option<FieldMatch> {
        if self.tokens.is_empty() {
            return Some(FieldMatch {
                score: 1,
                ranges: Vec::new(),
            });
        }
        if text.is_empty() {
            return None;
        }
        let prepared = PreparedText::new(text);
        self.match_prepared(&prepared)
    }

    /// Match this query against a pre-analyzed [`PreparedText`].
    pub fn match_prepared(&self, prepared: &PreparedText) -> Option<FieldMatch> {
        if self.tokens.is_empty() {
            return Some(FieldMatch {
                score: 1,
                ranges: Vec::new(),
            });
        }
        if prepared.units.is_empty() {
            return None;
        }
        let mut total = 0u32;
        let mut ranges = Vec::new();
        for token in &self.tokens {
            let hit = match_token(prepared, token)?;
            total = total.saturating_add(hit.score);
            ranges.extend(hit.ranges);
        }
        merge_ranges(&mut ranges);
        Some(FieldMatch {
            score: total / self.tokens.len() as u32,
            ranges,
        })
    }

    /// Match this query across multiple weighted fields using AND semantics across tokens.
    /// Every token must match at least one field; highlights are collected for every matching field.
    pub fn match_fields(&self, fields: &[WeightedField<'_>]) -> Option<DocumentMatch> {
        if self.tokens.is_empty() {
            return Some(DocumentMatch {
                score: 1,
                field_ranges: vec![Vec::new(); fields.len()],
                matched_field: None,
            });
        }
        let prepared: Vec<PreparedText> =
            fields.iter().map(|f| PreparedText::new(f.text)).collect();
        self.match_prepared_fields(
            &prepared
                .iter()
                .zip(fields.iter())
                .map(|(p, f)| (f.name, f.weight, p))
                .collect::<Vec<_>>(),
        )
    }

    /// Match this query across pre-analyzed weighted fields `(name, weight, prepared)`.
    pub fn match_prepared_fields(
        &self,
        fields: &[(&'static str, u32, &PreparedText)],
    ) -> Option<DocumentMatch> {
        if self.tokens.is_empty() {
            return Some(DocumentMatch {
                score: 1,
                field_ranges: vec![Vec::new(); fields.len()],
                matched_field: None,
            });
        }
        let mut field_ranges = vec![Vec::new(); fields.len()];
        let mut total_score = 0u32;
        let mut primary_hits = 0usize;
        let mut secondary_best: Option<(u32, &'static str)> = None;
        for token in &self.tokens {
            let mut token_best: Option<(u32, usize)> = None;
            let mut matched_primary = false;
            for (idx, &(name, weight, prepared)) in fields.iter().enumerate() {
                if prepared.units.is_empty() {
                    continue;
                }
                if let Some(hit) = match_token(prepared, token) {
                    let weighted = hit.score.saturating_mul(weight) / 100;
                    field_ranges[idx].extend(hit.ranges);
                    if idx == 0 {
                        matched_primary = true;
                    }
                    if token_best.is_none_or(|(best, _)| weighted > best) {
                        token_best = Some((weighted, idx));
                    }
                    let _ = name;
                }
            }
            let (best_score, best_idx) = token_best?;
            total_score = total_score.saturating_add(best_score);
            if matched_primary {
                primary_hits += 1;
            } else if best_idx > 0 {
                let candidate = (best_score, fields[best_idx].0);
                if secondary_best.is_none_or(|(s, _)| candidate.0 > s) {
                    secondary_best = Some(candidate);
                }
            }
        }
        for ranges in &mut field_ranges {
            merge_ranges(ranges);
        }
        let mut score = total_score / self.tokens.len() as u32;
        if primary_hits == self.tokens.len() && self.tokens.len() > 1 {
            score = score.saturating_add(80);
        }
        Some(DocumentMatch {
            score,
            field_ranges,
            matched_field: secondary_best.map(|(_, name)| name),
        })
    }
}

impl PreparedText {
    /// Analyze and normalize `text` once for fast repeated queries.
    pub fn new(text: &str) -> Self {
        let mut units = Vec::with_capacity(text.len().min(256));
        let mut norm = String::with_capacity(text.len());
        let mut norm_spans = Vec::with_capacity(text.len().min(256));
        let mut compact = String::with_capacity(text.len());
        let mut compact_spans = Vec::with_capacity(text.len().min(256));
        let mut mask = 0u32;
        let mut has_han = false;
        let nfkd = DecomposingNormalizerBorrowed::new_nfkd();

        let mut prev_was_sep = true;
        let mut prev_was_ascii_lower = false;
        let mut prev_was_han = false;
        let mut prev_was_digit = false;

        for (orig_start, ch) in text.char_indices() {
            let orig_end = orig_start + ch.len_utf8();
            let orig_is_ascii_upper = ch.is_ascii_uppercase();
            let orig_is_ascii_lower = ch.is_ascii_lowercase();

            let mut produced_any = false;
            for decomposed in nfkd.normalize_iter(std::iter::once(ch)) {
                if is_ignorable_mark(decomposed) {
                    continue;
                }
                let mapped = fold_script_char(decomposed);
                for lower in mapped.to_lowercase() {
                    let folded = fold_traditional(lower);
                    let pinyin = ch.to_pinyin_multi().or_else(|| folded.to_pinyin_multi());
                    let is_han = pinyin.is_some();
                    let is_sep = !folded.is_alphanumeric() && !is_han;
                    let is_digit = folded.is_ascii_digit();

                    let is_word_start = !is_sep
                        && (!produced_any
                            && (prev_was_sep
                                || (prev_was_ascii_lower && orig_is_ascii_upper)
                                || (prev_was_han != is_han)
                                || (prev_was_digit != is_digit)));

                    if is_han {
                        has_han = true;
                        if let Some(pm) = pinyin {
                            for syl in syllables_of(pm).as_slice() {
                                mask |= ascii_letters_mask(syl.as_bytes());
                            }
                        }
                    } else if folded.is_ascii_lowercase() {
                        mask |= letter_bit(folded as u8);
                    } else if is_digit {
                        mask |= 1 << 26;
                    }

                    units.push(TextUnit {
                        orig_start,
                        orig_end,
                        folded,
                        is_word_start,
                        is_separator: is_sep,
                        pinyin,
                    });
                    norm.push(folded);
                    norm_spans.push((orig_start, orig_end));
                    if !is_sep {
                        compact.push(folded);
                        compact_spans.push((orig_start, orig_end));
                    }

                    prev_was_sep = is_sep;
                    prev_was_han = is_han;
                    prev_was_digit = is_digit;
                    produced_any = true;
                }
            }
            if produced_any {
                prev_was_ascii_lower = orig_is_ascii_lower;
            }
        }

        Self {
            units,
            norm,
            norm_spans,
            compact,
            compact_spans,
            mask,
            has_han,
        }
    }

    /// Whether this prepared text has no searchable units.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

/// Convert original text and sorted/merged byte ranges into pre-sliced [`HighlightSpan`] segments.
pub fn highlight_spans(text: &str, ranges: &[(usize, usize)]) -> Vec<HighlightSpan> {
    if text.is_empty() {
        return Vec::new();
    }
    if ranges.is_empty() {
        return vec![HighlightSpan {
            text: text.to_string(),
            matched: false,
        }];
    }
    let mut merged = ranges.to_vec();
    merge_ranges(&mut merged);
    let mut out = Vec::with_capacity(merged.len() * 2 + 1);
    let mut cursor = 0usize;
    for (start, end) in merged {
        let start = clamp_char_boundary(text, start);
        let end = clamp_char_boundary(text, end);
        if start >= end || start < cursor {
            continue;
        }
        if start > cursor {
            out.push(HighlightSpan {
                text: text[cursor..start].to_string(),
                matched: false,
            });
        }
        out.push(HighlightSpan {
            text: text[start..end].to_string(),
            matched: true,
        });
        cursor = end;
    }
    if cursor < text.len() {
        out.push(HighlightSpan {
            text: text[cursor..].to_string(),
            matched: false,
        });
    }
    out
}

/// Build a centered snippet window around the first matched range, with pre-sliced [`HighlightSpan`]s.
pub fn snippet_spans(
    text: &str,
    ranges: &[(usize, usize)],
    max_chars: usize,
) -> Vec<HighlightSpan> {
    let clean = collapse_whitespace(text);
    if clean.is_empty() {
        return Vec::new();
    }
    if ranges.is_empty() {
        return trim_spans(highlight_spans(&clean, &[]), max_chars);
    }
    let total_chars = clean.chars().count();
    if total_chars <= max_chars {
        if clean == text {
            return highlight_spans(text, ranges);
        }
        // Re-map ranges cleanly if whitespace was collapsed by returning direct span when short.
        let mut out = highlight_spans(text, ranges);
        for span in &mut out {
            span.text = collapse_whitespace_keep_edges(&span.text);
        }
        out.retain(|s| !s.text.is_empty());
        return trim_spans(out, max_chars);
    }

    let mut merged = ranges.to_vec();
    merge_ranges(&mut merged);
    let (first_start, first_end) = merged[0];
    let first_start = clamp_char_boundary(text, first_start);
    let first_end = clamp_char_boundary(text, first_end);

    let lead_chars = (max_chars / 3).min(28);
    let win_start = step_back_chars(text, first_start, lead_chars);
    let win_end = step_forward_chars(text, win_start, max_chars).max(first_end);
    let win_end = win_end.min(text.len());

    let slice = &text[win_start..win_end];
    let local_ranges: Vec<(usize, usize)> = merged
        .into_iter()
        .filter_map(|(s, e)| {
            let s = s.max(win_start);
            let e = e.min(win_end);
            (s < e).then_some((s - win_start, e - win_start))
        })
        .collect();

    let mut spans = highlight_spans(slice, &local_ranges);
    for span in &mut spans {
        span.text = collapse_whitespace_keep_edges(&span.text);
    }
    spans.retain(|s| !s.text.is_empty());

    if win_start > 0 {
        if let Some(first) = spans.first_mut()
            && !first.matched
        {
            first.text.insert(0, '…');
        } else {
            spans.insert(
                0,
                HighlightSpan {
                    text: "…".to_string(),
                    matched: false,
                },
            );
        }
    }
    if win_end < text.len() {
        if let Some(last) = spans.last_mut()
            && !last.matched
        {
            last.text.push('…');
        } else {
            spans.push(HighlightSpan {
                text: "…".to_string(),
                matched: false,
            });
        }
    }
    spans
}

/// Generate compact lowercase search keys (normalized text, full pinyin, initials, fuzzy keys)
/// for out-of-process pickers such as OS Password AutoFill extensions.
pub fn search_keys(texts: &[&str]) -> Vec<String> {
    fn push_unique(keys: &mut Vec<String>, value: String) {
        let trimmed = value.trim();
        if trimmed.is_empty() || keys.iter().any(|k| k == trimmed) {
            return;
        }
        keys.push(trimmed.to_string());
    }

    let mut keys = Vec::new();
    for &text in texts {
        if text.trim().is_empty() {
            continue;
        }
        let prepared = PreparedText::new(text);
        if !prepared.norm.is_empty() {
            push_unique(&mut keys, prepared.norm.clone());
        }
        if !prepared.has_han {
            continue;
        }
        let mut fulls = vec![String::new()];
        let mut initials = vec![String::new()];
        let mut fuzzy_full = String::new();
        let mut fuzzy_initials = String::new();

        for unit in &prepared.units {
            if let Some(pm) = unit.pinyin {
                let syls = syllables_of(pm);
                let slice = syls.as_slice();
                if let Some(primary) = slice.first() {
                    let s = primary.as_str();
                    fuzzy_full.push_str(&fuzzy_canonical_syllable(s));
                    if let Some(first) = s.chars().next() {
                        fuzzy_initials.push(fuzzy_initial_char(first));
                    }
                    // Expand heteronyms bounded to at most 6 variants so common multi-reading
                    // names like 银行 (yinhang / yinxing) are always covered.
                    let mut next_fulls = Vec::with_capacity(fulls.len().min(6));
                    'outer: for base in &fulls {
                        for syl in slice.iter().take(2) {
                            let mut combined = base.clone();
                            combined.push_str(syl.as_str());
                            if !next_fulls.contains(&combined) {
                                next_fulls.push(combined);
                                if next_fulls.len() >= 6 {
                                    break 'outer;
                                }
                            }
                        }
                    }
                    fulls = next_fulls;

                    let mut next_inits = Vec::with_capacity(initials.len().min(6));
                    'outer_i: for base in &initials {
                        for syl in slice.iter().take(2) {
                            let Some(first) = syl.as_str().chars().next() else {
                                continue;
                            };
                            let mut combined = base.clone();
                            combined.push(first);
                            if !next_inits.contains(&combined) {
                                next_inits.push(combined);
                                if next_inits.len() >= 6 {
                                    break 'outer_i;
                                }
                            }
                        }
                    }
                    initials = next_inits;
                }
            } else if !unit.is_separator {
                for f in &mut fulls {
                    f.push(unit.folded);
                }
                for init in &mut initials {
                    init.push(unit.folded);
                }
                fuzzy_full.push(unit.folded);
                fuzzy_initials.push(unit.folded);
            }
        }
        for f in fulls {
            push_unique(&mut keys, f);
        }
        for init in initials {
            push_unique(&mut keys, init);
        }
        push_unique(&mut keys, fuzzy_full);
        push_unique(&mut keys, fuzzy_initials);
        if keys.len() >= 16 {
            break;
        }
    }
    keys
}

/// Localized/bilingual aliases for vault item kinds so users can search by type in English or Chinese/pinyin.
pub fn vault_kind_aliases(kind: &str) -> &'static str {
    match kind {
        "login" => "login password 密码 登录 账号",
        "api" => "api token credential 凭据 令牌 接口",
        "key" => "key secret 密钥 秘钥",
        "ssh" => "ssh key 密钥 终端",
        "wallet" => "wallet crypto seed mnemonic 钱包 助记词",
        "card" => "card credit debit bank 银行卡 信用卡 支付卡",
        "identity" => "identity passport id 身份 证件 护照 身份证",
        "server" => "server host vps 服务器 主机",
        "database" => "database db sql 数据库",
        "license" => "license software serial 许可证 序列号 激活码",
        "note" => "note memo 笔记 备忘录",
        _ => "",
    }
}

/// Lightweight media category aliases for file library search.
pub fn mime_aliases(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image photo picture 图片 照片 图像"
    } else if mime == "application/pdf" {
        "pdf document 文档"
    } else if mime.starts_with("video/") {
        "video movie 视频 影片"
    } else if mime.starts_with("audio/") {
        "audio music voice 音频 音乐 录音"
    } else if mime == "text/csv" {
        "sheet excel csv text code document 表格 文本 代码 文档"
    } else if mime.contains("spreadsheet") || mime.contains("excel") {
        "sheet excel csv 表格"
    } else if mime.starts_with("text/")
        || matches!(
            mime,
            "application/json" | "application/xml" | "application/toml" | "application/yaml"
        )
    {
        "text code document 文本 代码 文档"
    } else if mime.contains("zip")
        || mime.contains("tar")
        || mime.contains("gzip")
        || mime.contains("rar")
        || mime.contains("7z")
    {
        "archive zip 压缩包 归档"
    } else {
        ""
    }
}

fn match_token(prepared: &PreparedText, token: &QueryToken) -> Option<FieldMatch> {
    // Fast bitmask pre-filter: reject immediately if >= 2 unambiguous ASCII letters are missing.
    if token.req_mask != 0 && (token.req_mask & !prepared.mask).count_ones() > 1 {
        return None;
    }

    // Tier 1: Normalized literal match (covers exact, case-insensitive, NFKC, diacritics, Arabic, T2S).
    if let Some(hit) = match_literal(prepared, &token.norm) {
        return Some(hit);
    }

    // Tier 2: Separator-insensitive compact match (e.g. `13800138000` -> `138-0013-8000`).
    if token.compact.chars().count() >= 2
        && let Some(hit) = match_compact(prepared, &token.compact)
    {
        return Some(hit);
    }

    // Tier 3: Exact syllable-aligned Chinese pinyin match (full / hybrid / initials / mixed Han+pinyin).
    if prepared.has_han
        && token.norm.len() <= MAX_PINYIN_QUERY_BYTES
        && token.norm.bytes().any(|b| b.is_ascii_lowercase())
        && let Some(hit) = match_pinyin(prepared, &token.norm, false)
    {
        return Some(hit);
    }

    // Tier 4: Word-initial / CamelCase acronym match (e.g. `gh` -> `GitHub`, `dmsg` -> `dMsg`).
    if token.ascii_alpha
        && (2..=12).contains(&token.char_count)
        && let Some(hit) = match_acronym(prepared, &token.norm)
    {
        return Some(hit);
    }

    // Tier 5: Fuzzy Chinese pinyin match (`zh/z`, `ch/c`, `sh/s`, `l/n`, `in/ing`, `en/eng`, `an/ang`).
    if prepared.has_han
        && token.ascii_alpha
        && token.norm.len() <= MAX_PINYIN_QUERY_BYTES
        && let Some(hit) = match_pinyin(prepared, &token.norm, true)
    {
        return Some(hit);
    }

    // Tier 6: 1-edit typo tolerance for ASCII words and Han-run full pinyin (len >= 4).
    if token.ascii_alnum
        && (4..=32).contains(&token.char_count)
        && let Some(hit) = match_typo(prepared, &token.compact)
    {
        return Some(hit);
    }

    None
}

fn match_literal(prepared: &PreparedText, needle: &str) -> Option<FieldMatch> {
    if needle.is_empty() || prepared.norm.is_empty() {
        return None;
    }
    let needle_chars = needle.chars().count();
    let mut best: Option<(u32, (usize, usize))> = None;
    let mut search_from = 0usize;
    let mut char_offset = 0usize;

    while let Some(rel) = prepared.norm[search_from..].find(needle) {
        let byte_pos = search_from + rel;
        char_offset += prepared.norm[search_from..byte_pos].chars().count();
        let end_char = char_offset + needle_chars;
        if end_char <= prepared.norm_spans.len() {
            let orig_start = prepared.norm_spans[char_offset].0;
            let orig_end = prepared.norm_spans[end_char - 1].1;
            let score = if char_offset == 0 && end_char == prepared.norm_spans.len() {
                SCORE_EXACT
            } else if char_offset == 0 {
                SCORE_PREFIX
            } else if prepared
                .units
                .get(char_offset)
                .is_some_and(|u| u.is_word_start)
            {
                SCORE_WORD_PREFIX
            } else {
                SCORE_LITERAL_SUBSTR
            };
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, (orig_start, orig_end)));
                if score == SCORE_EXACT {
                    break;
                }
            }
        }
        let step = prepared.norm[byte_pos..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        search_from = byte_pos + step;
        char_offset += 1;
    }

    best.map(|(score, range)| FieldMatch {
        score,
        ranges: vec![range],
    })
}

fn match_compact(prepared: &PreparedText, needle: &str) -> Option<FieldMatch> {
    let pos = prepared.compact.find(needle)?;
    let start_char = prepared.compact[..pos].chars().count();
    let needle_chars = needle.chars().count();
    let end_char = start_char + needle_chars;
    if end_char > prepared.compact_spans.len() || start_char >= end_char {
        return None;
    }
    let orig_start = prepared.compact_spans[start_char].0;
    let orig_end = prepared.compact_spans[end_char - 1].1;
    let bonus = if start_char == 0 { 20 } else { 0 };
    Some(FieldMatch {
        score: SCORE_COMPACT + bonus,
        ranges: vec![(orig_start, orig_end)],
    })
}

fn match_acronym(prepared: &PreparedText, needle: &str) -> Option<FieldMatch> {
    let target: Vec<char> = needle.chars().collect();
    let starts: Vec<usize> = prepared
        .units
        .iter()
        .enumerate()
        .filter_map(|(i, u)| (u.is_word_start && u.pinyin.is_none()).then_some(i))
        .collect();
    if starts.len() < target.len() {
        return None;
    }
    for window in starts.windows(target.len()) {
        if window
            .iter()
            .zip(&target)
            .all(|(&idx, &ch)| prepared.units[idx].folded == ch)
        {
            let ranges = window
                .iter()
                .map(|&idx| (prepared.units[idx].orig_start, prepared.units[idx].orig_end))
                .collect();
            let bonus = if window[0] == 0 { 20 } else { 0 };
            return Some(FieldMatch {
                score: SCORE_ACRONYM + bonus,
                ranges,
            });
        }
    }
    None
}

#[derive(Clone, Copy, Debug)]
struct PinyinHit {
    end_unit: usize,
    full_steps: u16,
    initial_steps: u16,
    skips: u16,
}

fn match_pinyin(prepared: &PreparedText, query: &str, fuzzy: bool) -> Option<FieldMatch> {
    let q = query.as_bytes();
    if q.is_empty() || q.len() > MAX_PINYIN_QUERY_BYTES {
        return None;
    }
    let units = &prepared.units;
    let mut best: Option<(u32, (usize, usize))> = None;

    for start in 0..units.len() {
        if units[start].is_separator {
            continue;
        }
        if !can_start_pinyin_match(units[start], query, fuzzy) {
            continue;
        }
        let mut memo = HashMap::new();
        if let Some(hit) = dfs_pinyin(units, start, q, 0, 0, fuzzy, &mut memo) {
            let orig_start = units[start].orig_start;
            let orig_end = units[hit.end_unit].orig_end;
            let base = if fuzzy {
                if hit.initial_steps == 0 {
                    SCORE_FUZZY_PINYIN_FULL
                } else if hit.full_steps > 0 {
                    SCORE_FUZZY_PINYIN_HYBRID
                } else {
                    SCORE_FUZZY_PINYIN_INITIALS
                }
            } else if hit.initial_steps == 0 && start == 0 && hit.end_unit + 1 == units.len() {
                SCORE_PINYIN_EXACT
            } else if hit.initial_steps == 0 && start == 0 {
                SCORE_PINYIN_PREFIX
            } else if hit.initial_steps == 0 {
                SCORE_PINYIN_FULL
            } else if hit.full_steps > 0 {
                SCORE_PINYIN_HYBRID
            } else {
                SCORE_PINYIN_INITIALS
            };
            let start_bonus = if start == 0 {
                30
            } else if units[start].is_word_start {
                15
            } else {
                0
            };
            let penalty = u32::from(hit.skips) * 15;
            let score = base.saturating_add(start_bonus).saturating_sub(penalty);
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, (orig_start, orig_end)));
                if !fuzzy && score >= SCORE_PINYIN_EXACT {
                    break;
                }
            }
        }
    }

    best.map(|(score, range)| FieldMatch {
        score,
        ranges: vec![range],
    })
}

fn can_start_pinyin_match(unit: TextUnit, query: &str, fuzzy: bool) -> bool {
    let Some(first_q_char) = query.chars().next() else {
        return false;
    };
    if unit.folded == first_q_char {
        return true;
    }
    let Some(pm) = unit.pinyin else {
        return false;
    };
    if !first_q_char.is_ascii_lowercase() {
        return false;
    }
    let q_byte = first_q_char as u8;
    for syl in syllables_of(pm).as_slice() {
        let Some(&s0) = syl.as_bytes().first() else {
            continue;
        };
        if s0 == q_byte {
            return true;
        }
        if fuzzy && fuzzy_initial_byte(s0) == fuzzy_initial_byte(q_byte) {
            return true;
        }
    }
    false
}

fn dfs_pinyin(
    units: &[TextUnit],
    idx: usize,
    q: &[u8],
    q_pos: usize,
    skips: usize,
    fuzzy: bool,
    memo: &mut HashMap<(usize, usize), Option<PinyinHit>>,
) -> Option<PinyinHit> {
    if q_pos == q.len() {
        return None;
    }
    if idx >= units.len() {
        return None;
    }
    if let Some(hit) = memo.get(&(idx, q_pos)) {
        return *hit;
    }

    let unit = units[idx];

    // Allow skipping up to MAX_SKIP_SEPARATORS punctuation/space separators after matching >= 1 char.
    if unit.is_separator {
        let hit = if q_pos > 0 && skips < MAX_SKIP_SEPARATORS {
            dfs_pinyin(units, idx + 1, q, q_pos, skips + 1, fuzzy, memo)
        } else {
            None
        };
        memo.insert((idx, q_pos), hit);
        return hit;
    }

    let rem = &q[q_pos..];
    let mut best: Option<PinyinHit> = None;
    let consider = |best: &mut Option<PinyinHit>, candidate: PinyinHit| {
        let better = match best {
            None => true,
            Some(prev) => {
                (
                    candidate.full_steps,
                    std::cmp::Reverse(candidate.initial_steps),
                    std::cmp::Reverse(candidate.skips),
                ) > (
                    prev.full_steps,
                    std::cmp::Reverse(prev.initial_steps),
                    std::cmp::Reverse(prev.skips),
                )
            }
        };
        if better {
            *best = Some(candidate);
        }
    };

    // 1) Literal character match (supports mixed Han/ASCII/digits like `zs银行` or `招商2026`).
    let mut buf = [0u8; 4];
    let folded_bytes = unit.folded.encode_utf8(&mut buf).as_bytes();
    if rem.starts_with(folded_bytes) {
        let next_q = q_pos + folded_bytes.len();
        if next_q == q.len() {
            consider(
                &mut best,
                PinyinHit {
                    end_unit: idx,
                    full_steps: 1,
                    initial_steps: 0,
                    skips: skips as u16,
                },
            );
        } else if let Some(mut tail) = dfs_pinyin(units, idx + 1, q, next_q, skips, fuzzy, memo) {
            tail.full_steps += 1;
            consider(&mut best, tail);
        }
    }

    // 2) Pinyin syllable match for Han units.
    if let Some(pm) = unit.pinyin
        && rem[0].is_ascii_lowercase()
    {
        for syl in syllables_of(pm).as_slice() {
            let steps = match_syllable_steps(syl.as_bytes(), rem, fuzzy);
            for (consumed, is_full) in steps.into_iter().flatten() {
                let next_q = q_pos + consumed;
                if next_q == q.len() {
                    consider(
                        &mut best,
                        PinyinHit {
                            end_unit: idx,
                            full_steps: u16::from(is_full),
                            initial_steps: u16::from(!is_full),
                            skips: skips as u16,
                        },
                    );
                } else if let Some(mut tail) =
                    dfs_pinyin(units, idx + 1, q, next_q, skips, fuzzy, memo)
                {
                    if is_full {
                        tail.full_steps += 1;
                    } else {
                        tail.initial_steps += 1;
                    }
                    consider(&mut best, tail);
                }
            }
        }
    }

    memo.insert((idx, q_pos), best);
    best
}

/// Return up to 4 candidate `(bytes_consumed, is_full_or_prefix)` transitions for one syllable against `rem`.
fn match_syllable_steps(syl: &[u8], rem: &[u8], fuzzy: bool) -> [Option<(usize, bool)>; 4] {
    let mut out: [Option<(usize, bool)>; 4] = [None; 4];
    let mut count = 0usize;
    let mut push = |consumed: usize, is_full: bool| {
        if consumed == 0 || consumed > rem.len() || count >= out.len() {
            return;
        }
        for existing in out[..count].iter_mut().flatten() {
            if existing.0 == consumed {
                existing.1 |= is_full;
                return;
            }
        }
        out[count] = Some((consumed, is_full));
        count += 1;
    };

    if !fuzzy {
        // Full syllable match
        if rem.starts_with(syl) {
            push(syl.len(), true);
        }
        // End-of-query syllable prefix match (e.g. `zhaosh` matching `招商`)
        if rem.len() < syl.len() && syl.starts_with(rem) {
            push(rem.len(), rem.len() >= 2);
        }
        // Two-letter initial (`zh`, `ch`, `sh`)
        if syl.len() >= 2
            && syl[1] == b'h'
            && matches!(syl[0], b'z' | b'c' | b's')
            && rem.starts_with(&syl[..2])
        {
            push(2, false);
        }
        // Single-letter initial
        if rem[0] == syl[0] {
            push(1, false);
        }
        return out;
    }

    // Fuzzy pinyin: match fuzzy initial (`zh<->z`, `ch<->c`, `sh<->s`, `l<->n`),
    // then fuzzy final (`ang<->an`, `eng<->en`, `ing<->in`).
    let (init_len, init_matches) = match_fuzzy_initial(syl, rem);
    let syl_rest = &syl[init_len..];

    for consumed_init in init_matches.into_iter().flatten() {
        let rest_rem = &rem[consumed_init..];
        if syl_rest.is_empty() {
            push(consumed_init, true);
            continue;
        }
        if consumed_init > 0 {
            push(consumed_init, false);
        }
        if rest_rem.is_empty() {
            continue;
        }
        for consumed_final in match_fuzzy_final(syl_rest, rest_rem).into_iter().flatten() {
            push(consumed_init + consumed_final, true);
        }
    }

    out
}

fn match_fuzzy_initial(syl: &[u8], rem: &[u8]) -> (usize, [Option<usize>; 2]) {
    let mut matched = [None; 2];
    if syl.len() >= 2 && syl[1] == b'h' && matches!(syl[0], b'z' | b'c' | b's') {
        let base = syl[0];
        if rem.starts_with(&[base, b'h']) {
            matched[0] = Some(2);
            matched[1] = Some(1);
        } else if rem[0] == base {
            matched[0] = Some(1);
        }
        return (2, matched);
    }
    if matches!(syl[0], b'z' | b'c' | b's') {
        let base = syl[0];
        if rem.starts_with(&[base, b'h']) {
            matched[0] = Some(2);
            matched[1] = Some(1);
        } else if rem[0] == base {
            matched[0] = Some(1);
        }
        return (1, matched);
    }
    if matches!(syl[0], b'l' | b'n') {
        if matches!(rem[0], b'l' | b'n') {
            matched[0] = Some(1);
        }
        return (1, matched);
    }
    if matches!(syl[0], b'a' | b'e' | b'o') {
        // Zero-initial syllable (e.g. `an`, `ang`, `en`, `eng`, `ai`, `ou`)
        matched[0] = Some(0);
        return (0, matched);
    }
    if rem[0] == syl[0] {
        matched[0] = Some(1);
    }
    (1, matched)
}

fn match_fuzzy_final(syl_rest: &[u8], rem: &[u8]) -> [Option<usize>; 2] {
    let mut out: [Option<usize>; 2] = [None; 2];
    let mut count = 0usize;
    let mut push = |len: usize| {
        if len == 0 || len > rem.len() || count >= out.len() {
            return;
        }
        if out[..count].contains(&Some(len)) {
            return;
        }
        out[count] = Some(len);
        count += 1;
    };

    // Exact final match or end-of-query prefix
    if rem.starts_with(syl_rest) {
        push(syl_rest.len());
    } else if rem.len() < syl_rest.len() && syl_rest.starts_with(rem) {
        push(rem.len());
    }

    // Nasal fuzzy pairs: `ang <-> an`, `eng <-> en`, `ing <-> in` (also covers `iang`/`uang`)
    if syl_rest.len() >= 3
        && syl_rest.ends_with(b"ng")
        && matches!(syl_rest[syl_rest.len() - 3], b'a' | b'e' | b'i')
    {
        let short = &syl_rest[..syl_rest.len() - 1];
        if rem.starts_with(short) {
            push(short.len());
        } else if rem.len() < short.len() && short.starts_with(rem) {
            push(rem.len());
        }
    } else if syl_rest.len() >= 2
        && syl_rest.ends_with(b"n")
        && matches!(syl_rest[syl_rest.len() - 2], b'a' | b'e' | b'i')
    {
        let mut long = [0u8; 6];
        if syl_rest.len() < 6 {
            long[..syl_rest.len()].copy_from_slice(syl_rest);
            long[syl_rest.len()] = b'g';
            let long_slice = &long[..=syl_rest.len()];
            if rem.starts_with(long_slice) {
                push(long_slice.len());
            } else if rem.len() < long_slice.len() && long_slice.starts_with(rem) {
                push(rem.len());
            }
        }
    }

    out
}

fn match_typo(prepared: &PreparedText, needle: &str) -> Option<FieldMatch> {
    let nb = needle.as_bytes();
    let nlen = nb.len();
    if nlen < 4 {
        return None;
    }

    // 1) Check contiguous ASCII alphanumeric words (and camelCase subwords).
    let mut i = 0usize;
    while i < prepared.units.len() {
        let unit = &prepared.units[i];
        if unit.is_separator || unit.pinyin.is_some() || !unit.folded.is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        let mut word = Vec::with_capacity(32);
        while i < prepared.units.len()
            && !prepared.units[i].is_separator
            && prepared.units[i].pinyin.is_none()
            && prepared.units[i].folded.is_ascii_alphanumeric()
        {
            word.push(prepared.units[i].folded as u8);
            i += 1;
        }
        if word.len() >= 3 && (word.len().abs_diff(nlen) <= 1 || word.len() > nlen) {
            let slice = if word.len() > nlen + 1 {
                &word[..nlen]
            } else {
                &word
            };
            if damerau_distance_at_most_one(nb, slice) {
                let end_unit = if word.len() > nlen + 1 {
                    start + nlen - 1
                } else {
                    i - 1
                };
                return Some(FieldMatch {
                    score: SCORE_TYPO,
                    ranges: vec![(
                        prepared.units[start].orig_start,
                        prepared.units[end_unit].orig_end,
                    )],
                });
            }
        }
    }

    // 2) Check contiguous Han runs converted to primary full pinyin (e.g. `zhaoshnag` -> `招商`).
    if prepared.has_han {
        let mut i = 0usize;
        while i < prepared.units.len() {
            if prepared.units[i].pinyin.is_none() {
                i += 1;
                continue;
            }
            let run_start = i;
            let mut py_bytes = Vec::with_capacity(64);
            let mut unit_bounds: Vec<(usize, usize, usize)> = Vec::new();
            while i < prepared.units.len()
                && let Some(pm) = prepared.units[i].pinyin
                && unit_bounds.len() < 24
            {
                let syls = syllables_of(pm);
                if let Some(first) = syls.as_slice().first() {
                    let b_start = py_bytes.len();
                    py_bytes.extend_from_slice(first.as_bytes());
                    unit_bounds.push((i, b_start, py_bytes.len()));
                }
                i += 1;
            }
            // Test Han sub-slices starting at each char whose pinyin byte length is within nlen ± 1.
            for s in 0..unit_bounds.len() {
                let base_b = unit_bounds[s].1;
                for &(u_idx, _, end_b) in &unit_bounds[s..] {
                    let span_len = end_b - base_b;
                    if span_len + 1 < nlen {
                        continue;
                    }
                    if span_len > nlen + 1 {
                        break;
                    }
                    if damerau_distance_at_most_one(nb, &py_bytes[base_b..end_b]) {
                        let u_start = unit_bounds[s].0;
                        let bonus = if u_start == run_start && run_start == 0 {
                            15
                        } else {
                            0
                        };
                        return Some(FieldMatch {
                            score: SCORE_TYPO + bonus,
                            ranges: vec![(
                                prepared.units[u_start].orig_start,
                                prepared.units[u_idx].orig_end,
                            )],
                        });
                    }
                }
            }
        }
    }

    None
}

/// True iff Damerau-Levenshtein distance (insert, delete, substitute, adjacent transpose) <= 1.
fn damerau_distance_at_most_one(a: &[u8], b: &[u8]) -> bool {
    let al = a.len();
    let bl = b.len();
    if al.abs_diff(bl) > 1 {
        return false;
    }
    let mut i = 0usize;
    let min_len = al.min(bl);
    while i < min_len && a[i] == b[i] {
        i += 1;
    }
    if i == min_len {
        return true;
    }
    if al == bl {
        // Substitution
        if a[i + 1..] == b[i + 1..] {
            return true;
        }
        // Adjacent transposition
        i + 1 < al && a[i] == b[i + 1] && a[i + 1] == b[i] && a[i + 2..] == b[i + 2..]
    } else if al > bl {
        a[i + 1..] == b[i..]
    } else {
        a[i..] == b[i + 1..]
    }
}

fn syllables_of(pm: PinyinMulti) -> SylList {
    let mut list = SylList::default();
    for p in pm {
        let raw = p.plain();
        if raw.is_ascii() {
            if let Some(s) = Syl::from_bytes(raw.as_bytes()) {
                list.push(s);
            }
            continue;
        }
        // Handle `ü` -> both `v` and `u`, and `ê` -> `e`.
        let mut with_v = [0u8; 6];
        let mut with_u = [0u8; 6];
        let mut len = 0usize;
        let mut has_umlaut = false;
        for ch in raw.chars() {
            if len >= 6 {
                len = 0;
                break;
            }
            match ch {
                'ü' => {
                    has_umlaut = true;
                    with_v[len] = b'v';
                    with_u[len] = b'u';
                    len += 1;
                }
                'ê' => {
                    with_v[len] = b'e';
                    with_u[len] = b'e';
                    len += 1;
                }
                c if c.is_ascii_lowercase() => {
                    with_v[len] = c as u8;
                    with_u[len] = c as u8;
                    len += 1;
                }
                _ => {}
            }
        }
        if let Some(s) = Syl::from_bytes(&with_v[..len]) {
            list.push(s);
        }
        if has_umlaut && let Some(s) = Syl::from_bytes(&with_u[..len]) {
            list.push(s);
        }
    }
    list
}

fn normalize_string(input: &str) -> String {
    let nfkd = DecomposingNormalizerBorrowed::new_nfkd();
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        for d in nfkd.normalize_iter(std::iter::once(ch)) {
            if is_ignorable_mark(d) {
                continue;
            }
            let mapped = fold_script_char(d);
            for lower in mapped.to_lowercase() {
                out.push(fold_traditional(lower));
            }
        }
    }
    out
}

fn is_ignorable_mark(ch: char) -> bool {
    let code = ch as u32;
    matches!(
        code,
        0x0300..=0x036F
            | 0x0610..=0x061A
            | 0x0640
            | 0x064B..=0x065F
            | 0x0670
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

fn fold_script_char(ch: char) -> char {
    match ch {
        // Arabic letter normalization (preserves readability while boosting search recall)
        'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
        'ة' => 'ه',
        'ى' => 'ي',
        'ؤ' => 'و',
        'ئ' => 'ي',
        // Katakana -> Hiragana folding
        '\u{30A1}'..='\u{30F6}' => char::from_u32(ch as u32 - 0x60).unwrap_or(ch),
        _ => ch,
    }
}

fn letter_bit(b: u8) -> u32 {
    if b.is_ascii_lowercase() {
        1u32 << (b - b'a')
    } else {
        0
    }
}

fn ascii_letters_mask(bytes: &[u8]) -> u32 {
    let mut mask = 0u32;
    for &b in bytes {
        mask |= letter_bit(b);
    }
    mask
}

/// Mask of unambiguous ASCII letters in a query token (excluding fuzzy-interchangeable `l/n/h/g`).
fn query_required_mask(norm: &str) -> u32 {
    let mut mask = 0u32;
    for b in norm.bytes() {
        if b.is_ascii_lowercase() && !matches!(b, b'l' | b'n' | b'h' | b'g' | b'v' | b'u') {
            mask |= letter_bit(b);
        } else if b.is_ascii_digit() {
            mask |= 1 << 26;
        }
    }
    mask
}

fn fuzzy_initial_byte(b: u8) -> u8 {
    match b {
        b'l' => b'n',
        _ => b,
    }
}

fn fuzzy_initial_char(ch: char) -> char {
    match ch {
        'l' => 'n',
        _ => ch,
    }
}

fn fuzzy_canonical_syllable(syl: &str) -> String {
    let mut s = if let Some(rest) = syl.strip_prefix("zh") {
        format!("z{rest}")
    } else if let Some(rest) = syl.strip_prefix("ch") {
        format!("c{rest}")
    } else if let Some(rest) = syl.strip_prefix("sh") {
        format!("s{rest}")
    } else if let Some(rest) = syl.strip_prefix('l') {
        format!("n{rest}")
    } else {
        syl.to_string()
    };
    if s.ends_with("ang") || s.ends_with("eng") || s.ends_with("ing") {
        s.pop();
    }
    s
}

fn merge_ranges(ranges: &mut Vec<(usize, usize)>) {
    if ranges.len() <= 1 {
        return;
    }
    ranges.sort_unstable();
    let mut write = 0usize;
    for read in 1..ranges.len() {
        if ranges[read].0 <= ranges[write].1 {
            ranges[write].1 = ranges[write].1.max(ranges[read].1);
        } else {
            write += 1;
            ranges[write] = ranges[read];
        }
    }
    ranges.truncate(write + 1);
}

fn clamp_char_boundary(text: &str, mut pos: usize) -> usize {
    if pos >= text.len() {
        return text.len();
    }
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn step_back_chars(text: &str, byte_pos: usize, count: usize) -> usize {
    text[..byte_pos]
        .char_indices()
        .rev()
        .nth(count.saturating_sub(1))
        .map_or(0, |(idx, _)| idx)
}

fn step_forward_chars(text: &str, byte_pos: usize, count: usize) -> usize {
    text[byte_pos..]
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(rel, _)| byte_pos + rel)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collapse_whitespace_keep_edges(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_ws {
                out.push(' ');
                last_ws = true;
            }
        } else {
            out.push(ch);
            last_ws = false;
        }
    }
    out
}

fn trim_spans(spans: Vec<HighlightSpan>, max_chars: usize) -> Vec<HighlightSpan> {
    let mut out = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    for span in spans {
        let remaining = max_chars.saturating_sub(used);
        if remaining == 0 {
            truncated = true;
            break;
        }
        let len = span.text.chars().count();
        if len <= remaining {
            used += len;
            out.push(span);
        } else {
            let cut: String = span.text.chars().take(remaining).collect();
            out.push(HighlightSpan {
                text: cut,
                matched: span.matched,
            });
            truncated = true;
            break;
        }
    }
    if truncated {
        if let Some(last) = out.last_mut()
            && !last.matched
        {
            last.text.push('…');
        } else {
            out.push(HighlightSpan {
                text: "…".to_string(),
                matched: false,
            });
        }
    }
    out
}
