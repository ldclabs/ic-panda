package net.dmsg.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CheckBox
import androidx.compose.material.icons.outlined.CheckBoxOutlineBlank
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.core.*
import net.dmsg.android.platform.str
import net.dmsg.android.ui.kit.Notice

/** Native Compose renderer for the safe, bounded Markdown AST produced by Rust. */
@Composable internal fun MarkdownDocumentView(
    document: MarkdownDocument,
    client: ClientController,
    browser: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var pendingUrl by remember { mutableStateOf<String?>(null) }
    val onLink: (String) -> Unit = { url ->
        if (url.startsWith("mailto:", ignoreCase = true)) {
            client.clipboard.copy(url.substringAfter(':'))
        } else if (url.startsWith("https://", ignoreCase = true) || url.startsWith("http://", ignoreCase = true)) {
            pendingUrl = url
        }
    }
    SelectionContainer {
        Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (document.truncated) {
                Notice(str(R.string.preview_truncated))
            }
            document.blocks.forEach { block ->
                MarkdownBlockView(block, client, onLink)
            }
        }
    }
    pendingUrl?.let { url ->
        AlertDialog(
            onDismissRequest = { pendingUrl = null },
            title = { Text(str(R.string.preview_open_external_link)) },
            text = { Text(url, style = MaterialTheme.typography.bodyMedium.code()) },
            confirmButton = {
                TextButton(onClick = { pendingUrl = null; browser(url) }) {
                    Text(str(R.string.text_continue_fd19155))
                }
            },
            dismissButton = {
                TextButton(onClick = { pendingUrl = null }) {
                    Text(str(R.string.text_cancel_998b9c4))
                }
            },
        )
    }
}

@Composable private fun MarkdownBlockView(
    block: MarkdownBlock,
    client: ClientController,
    onLink: (String) -> Unit,
) {
    when (block) {
        is MarkdownBlock.Heading -> {
            val style = when (block.level) {
                1 -> MaterialTheme.typography.headlineMedium
                2 -> MaterialTheme.typography.headlineSmall
                3 -> MaterialTheme.typography.titleLarge
                4 -> MaterialTheme.typography.titleMedium
                5 -> MaterialTheme.typography.titleSmall
                else -> MaterialTheme.typography.labelLarge
            }
            Text(
                text = buildMarkdownInlines(block.inlines, onLink),
                style = style,
                modifier = Modifier.padding(top = if (block.level <= 2) 6.dp else 2.dp),
            )
        }

        is MarkdownBlock.Paragraph -> {
            Text(
                text = buildMarkdownInlines(block.inlines, onLink),
                style = MaterialTheme.typography.bodyLarge,
            )
        }

        is MarkdownBlock.CodeBlock -> {
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = MaterialTheme.shapes.medium,
                color = MaterialTheme.colorScheme.surfaceContainerHighest,
            ) {
                Column(Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(
                            text = block.language.orEmpty(),
                            style = MaterialTheme.typography.labelSmall.code(),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        IconButton(onClick = { client.clipboard.copy(block.code) }) {
                            Icon(Icons.Outlined.ContentCopy, str(R.string.text_copy_d6e9651), Modifier.size(18.dp))
                        }
                    }
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
                        Text(
                            text = block.code,
                            style = MaterialTheme.typography.bodySmall.code(),
                            modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(12.dp),
                        )
                    }
                }
            }
        }

        is MarkdownBlock.BlockQuote -> {
            QuoteBar(Modifier.fillMaxWidth()) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    block.blocks.forEach { nested -> MarkdownBlockView(nested, client, onLink) }
                }
            }
        }

        is MarkdownBlock.ListBlock -> {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                block.items.forEachIndexed { index, item ->
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(start = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.Top,
                    ) {
                        when {
                            item.checked != null -> Icon(
                                imageVector = if (item.checked) Icons.Outlined.CheckBox else Icons.Outlined.CheckBoxOutlineBlank,
                                contentDescription = str(if (item.checked) R.string.text_completed_6ff49bc else R.string.text_incomplete_022e64c),
                                tint = if (item.checked) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.padding(top = 2.dp).size(18.dp),
                            )
                            block.ordered -> Text(
                                text = "${block.start + index.toULong()}.",
                                style = MaterialTheme.typography.bodyLarge.tabular(),
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            else -> Text(
                                text = "•",
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            item.blocks.forEach { child -> MarkdownBlockView(child, client, onLink) }
                        }
                    }
                }
            }
        }

        is MarkdownBlock.Table -> {
            val colCount = maxOf(block.headers.size, block.rows.maxOfOrNull { it.size } ?: 0)
            if (colCount > 0) {
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    shape = MaterialTheme.shapes.medium,
                    color = MaterialTheme.colorScheme.card,
                ) {
                    BoxWithConstraints(Modifier.fillMaxWidth()) {
                        val colWidth = maxOf(108.dp, maxWidth / colCount)
                        val tableWidth = colWidth * colCount
                        Column(Modifier.horizontalScroll(rememberScrollState()).width(tableWidth)) {
                            if (block.headers.isNotEmpty()) {
                                Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainerHighest)) {
                                    for (c in 0 until colCount) {
                                        val align = columnAlignment(block.alignments.getOrNull(c))
                                        Text(
                                            text = buildMarkdownInlines(block.headers.getOrElse(c) { emptyList() }, onLink),
                                            style = MaterialTheme.typography.labelLarge,
                                            textAlign = align,
                                            modifier = Modifier.width(colWidth).padding(horizontal = 12.dp, vertical = 8.dp),
                                        )
                                    }
                                }
                                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                            }
                            block.rows.forEachIndexed { rowIndex, row ->
                                if (rowIndex > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                                Row(Modifier.fillMaxWidth()) {
                                    for (c in 0 until colCount) {
                                        val align = columnAlignment(block.alignments.getOrNull(c))
                                        Text(
                                            text = buildMarkdownInlines(row.getOrElse(c) { emptyList() }, onLink),
                                            style = MaterialTheme.typography.bodyMedium,
                                            textAlign = align,
                                            modifier = Modifier.width(colWidth).padding(horizontal = 12.dp, vertical = 8.dp),
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        MarkdownBlock.ThematicBreak -> HorizontalDivider(
            modifier = Modifier.padding(vertical = 4.dp),
            color = MaterialTheme.colorScheme.outlineVariant,
        )
    }
}

private fun columnAlignment(alignment: MarkdownTableAlignment?): TextAlign = when (alignment) {
    MarkdownTableAlignment.CENTER -> TextAlign.Center
    MarkdownTableAlignment.RIGHT -> TextAlign.End
    else -> TextAlign.Start
}

@Composable private fun buildMarkdownInlines(
    inlines: List<MarkdownInline>,
    onLink: (String) -> Unit,
): AnnotatedString {
    val colors = MaterialTheme.colorScheme
    val codeStyle = MaterialTheme.typography.bodyMedium.code().toSpanStyle().copy(
        background = colors.surfaceContainerHighest,
    )
    val linkStyles = TextLinkStyles(
        style = SpanStyle(color = colors.primary, textDecoration = TextDecoration.Underline),
    )
    val blockedLabel = str(R.string.preview_external_image_blocked)
    return buildAnnotatedString {
        fun appendList(nodes: List<MarkdownInline>) {
            for (node in nodes) {
                when (node) {
                    is MarkdownInline.Text -> append(node.text)
                    is MarkdownInline.Strong -> {
                        pushStyle(SpanStyle(fontWeight = FontWeight.Bold))
                        appendList(node.children)
                        pop()
                    }
                    is MarkdownInline.Emphasis -> {
                        pushStyle(SpanStyle(fontStyle = FontStyle.Italic))
                        appendList(node.children)
                        pop()
                    }
                    is MarkdownInline.Strikethrough -> {
                        pushStyle(SpanStyle(textDecoration = TextDecoration.LineThrough))
                        appendList(node.children)
                        pop()
                    }
                    is MarkdownInline.Code -> {
                        pushStyle(codeStyle)
                        append(" ${node.code} ")
                        pop()
                    }
                    is MarkdownInline.Link -> {
                        pushLink(LinkAnnotation.Clickable(tag = "md-link", styles = linkStyles) { onLink(node.url) })
                        appendList(node.children)
                        pop()
                    }
                    is MarkdownInline.BlockedImage -> {
                        pushStyle(SpanStyle(color = colors.onSurfaceVariant, background = colors.surfaceContainerHighest))
                        val label = if (node.alt.isNotBlank()) "[$blockedLabel: ${node.alt}]" else "[$blockedLabel]"
                        append(label)
                        pop()
                    }
                    MarkdownInline.SoftBreak -> append(" ")
                    MarkdownInline.HardBreak -> append("\n")
                }
            }
        }
        appendList(inlines)
    }
}
