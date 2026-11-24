package net.dmsg.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FullscreenPreviewModelsTest {
    private fun item(id: String, name: String, mime: String, size: ULong = 1024uL, canOpen: Boolean = true) =
        FullscreenPreviewItem(
            id = id,
            source = PreviewSource.Library(id, null),
            name = name,
            mime = mime,
            size = size,
            updatedAt = 1uL,
            sha256 = null,
            canOpen = canOpen,
        )

    @Test fun classificationMatchesRustContract() {
        assertEquals(PreviewClass.IMAGE, PreviewFilterLogic.classify("image/png", "photo.png"))
        assertEquals(PreviewClass.IMAGE, PreviewFilterLogic.classify("application/octet-stream", "photo.WEBP"))
        assertEquals(PreviewClass.UNSUPPORTED, PreviewFilterLogic.classify("image/svg+xml", "vector.svg"))
        assertEquals(PreviewClass.UNSUPPORTED, PreviewFilterLogic.classify("application/octet-stream", "icon.svg"))
        assertEquals(PreviewClass.MARKDOWN, PreviewFilterLogic.classify("text/markdown; charset=utf-8", "readme.txt"))
        assertEquals(PreviewClass.MARKDOWN, PreviewFilterLogic.classify("application/octet-stream", "notes.md"))
        assertEquals(PreviewClass.TEXT, PreviewFilterLogic.classify("application/json", "config.json"))
        assertEquals(PreviewClass.UNSUPPORTED, PreviewFilterLogic.classify("application/pdf", "report.pdf"))
        assertEquals(PreviewClass.UNSUPPORTED, PreviewFilterLogic.classify("application/vnd.dmsg.secret-package", "secret.dmsg"))
    }

    @Test fun filterCountsAndNearestNeighborPreserveFocus() {
        val items = listOf(
            item("1", "archive.zip", "application/zip", 500uL),
            item("2", "photo.png", "image/png", 1024uL),
            item("3", "huge.png", "image/png", PreviewFilterLogic.MAX_IMAGE_BYTES + 1uL),
            item("4", "notes.md", "text/markdown", 2048uL),
            item("5", "cloud.txt", "text/plain", 512uL, canOpen = false),
        )
        val counts = PreviewFilterLogic.counts(items)
        assertEquals(5, counts[PreviewFilter.ALL])
        assertEquals(2, counts[PreviewFilter.PREVIEWABLE])
        assertEquals(2, counts[PreviewFilter.IMAGES])
        assertEquals(2, counts[PreviewFilter.DOCUMENTS])

        val kept = PreviewFilterLogic.pickNearest(items, "2", PreviewFilter.IMAGES)
        assertEquals("2", kept.activeId)
        assertEquals(listOf("2", "3"), kept.filtered.map { it.id })

        val moved = PreviewFilterLogic.pickNearest(items, "1", PreviewFilter.PREVIEWABLE)
        assertEquals("2", moved.activeId)
        assertEquals(listOf("2", "4"), moved.filtered.map { it.id })

        val backward = PreviewFilterLogic.pickNearest(items, "5", PreviewFilter.IMAGES)
        assertEquals("3", backward.activeId)
    }

    @Test fun parsesMarkdownAstAndChannelAttachments() {
        val mdJson = """
            {
              "blocks": [
                {"type":"heading","level":2,"inlines":[{"type":"text","text":"Title"}]},
                {"type":"paragraph","inlines":[
                  {"type":"strong","children":[{"type":"text","text":"bold"}]},
                  {"type":"link","url":"https://example.com","children":[{"type":"text","text":"site"}]},
                  {"type":"blocked_image","alt":"logo"}
                ]},
                {"type":"list","ordered":false,"start":1,"items":[
                  {"checked":true,"blocks":[{"type":"paragraph","inlines":[{"type":"text","text":"done"}]}]}
                ]},
                {"type":"code_block","language":"rust","code":"fn main() {}"}
              ],
              "truncated": true
            }
        """.trimIndent()
        val doc = parseMarkdownDocument(mdJson)
        assertNotNull(doc)
        assertTrue(doc!!.truncated)
        assertEquals(4, doc.blocks.size)
        assertTrue(doc.blocks[0] is MarkdownBlock.Heading)
        assertTrue(doc.blocks[2] is MarkdownBlock.ListBlock)

        val attJson = """
            [{"seq":7,"digest":"abc","account":"acc1","created_at":100,"stored_at":200,"name":"doc.md","mime":"text/markdown","size":42,"preview_class":"markdown","previewable":true}]
        """.trimIndent()
        val rows = parseChannelAttachments(attJson)
        assertEquals(1, rows.size)
        assertEquals(7uL, rows[0].seq)
        assertEquals(PreviewClass.MARKDOWN, rows[0].previewClass)
        assertTrue(rows[0].previewable)
        assertFalse(PreviewFilterLogic.matches(FullscreenPreviewItem.fromAttachment("c1", rows[0]), PreviewFilter.IMAGES))
    }
}
