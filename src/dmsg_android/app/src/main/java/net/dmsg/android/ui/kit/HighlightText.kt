package net.dmsg.android.ui.kit

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import net.dmsg.android.core.HighlightSpan

/** Renders pre-sliced highlight spans using theme color roles only. */
@Composable
internal fun buildHighlightedText(text: String, spans: List<HighlightSpan>): AnnotatedString {
    if (spans.isEmpty()) return AnnotatedString(text)
    val bg = MaterialTheme.colorScheme.primaryContainer
    val fg = MaterialTheme.colorScheme.onPrimaryContainer
    val mark = SpanStyle(background = bg, color = fg, fontWeight = FontWeight.SemiBold)
    return buildAnnotatedString {
        for (span in spans) {
            if (span.matched) withStyle(mark) { append(span.text) }
            else append(span.text)
        }
    }
}

/** Highlights matched terms inside full message text without truncating the message body. */
@Composable
internal fun buildBubbleHighlightedText(text: String, spans: List<HighlightSpan>): AnnotatedString {
    if (spans.isEmpty()) return AnnotatedString(text)
    if (spans.joinToString("") { it.text } == text) return buildHighlightedText(text, spans)
    val terms = spans.filter { it.matched && it.text.isNotEmpty() }.map { it.text }.distinct()
    if (terms.isEmpty()) return AnnotatedString(text)
    val ranges = mutableListOf<IntRange>()
    for (term in terms) {
        var from = 0
        while (from < text.length) {
            val at = text.indexOf(term, from)
            if (at < 0) break
            ranges += at until (at + term.length)
            from = at + term.length
        }
    }
    if (ranges.isEmpty()) return AnnotatedString(text)
    ranges.sortBy { it.first }
    val merged = mutableListOf<IntRange>()
    for (r in ranges) {
        val last = merged.lastOrNull()
        if (last != null && r.first <= last.last + 1) {
            merged[merged.lastIndex] = last.first..maxOf(last.last, r.last)
        } else {
            merged += r
        }
    }
    val bg = MaterialTheme.colorScheme.primaryContainer
    val fg = MaterialTheme.colorScheme.onPrimaryContainer
    val mark = SpanStyle(background = bg, color = fg, fontWeight = FontWeight.SemiBold)
    return buildAnnotatedString {
        var pos = 0
        for (r in merged) {
            if (r.first > pos) append(text.substring(pos, r.first))
            withStyle(mark) { append(text.substring(r.first, r.last + 1)) }
            pos = r.last + 1
        }
        if (pos < text.length) append(text.substring(pos))
    }
}
