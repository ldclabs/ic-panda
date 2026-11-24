import XCTest
@testable import dMsg

final class PreviewModelsTests: XCTestCase {
    private func makeItem(
        id: String,
        name: String,
        mime: String,
        size: UInt64,
        canOpen: Bool = true
    ) -> FullscreenPreviewItem {
        FullscreenPreviewItem(
            id: id,
            source: .library(id: id, version: nil),
            name: name,
            mime: mime,
            size: size,
            updatedAt: nil,
            sha256: nil,
            canOpen: canOpen,
            libraryFile: nil,
            versionLabel: nil,
            explicitClass: nil,
            explicitPreviewable: nil
        )
    }

    func testClassificationMatchesRustRules() {
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "image/png", name: "a.png"), .image)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "application/octet-stream", name: "b.WEBP"), .image)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "image/svg+xml", name: "c.svg"), .unsupported)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "text/plain", name: "README.md"), .markdown)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "application/json", name: "data.json"), .text)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "application/pdf", name: "doc.pdf"), .unsupported)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "application/vnd.dmsg.secret-package", name: "k.md"), .unsupported)
        // Same rules as the core: no ICO or TIFF rendering, TypeScript is text.
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "image/tiff", name: "scan.tiff"), .unsupported)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "application/octet-stream", name: "favicon.ico"), .unsupported)
        XCTAssertEqual(PreviewFilterLogic.classify(mime: "application/typescript", name: "main.ts"), .text)
    }

    func testFilterCountsAndNearestNeighborFocus() {
        let list = [
            makeItem(id: "1", name: "a.zip", mime: "application/zip", size: 100),
            makeItem(id: "2", name: "b.png", mime: "image/png", size: 200),
            makeItem(id: "3", name: "c.pdf", mime: "application/pdf", size: 300),
            makeItem(id: "4", name: "d.md", mime: "text/markdown", size: 400),
            makeItem(id: "5", name: "e.txt", mime: "text/plain", size: 500),
            makeItem(id: "6", name: "f.png", mime: "image/png", size: PreviewFilterLogic.maxImageBytes + 1),
            makeItem(id: "7", name: "g.png", mime: "image/png", size: 100, canOpen: false),
        ]
        let counts = PreviewFilterLogic.counts(for: list)
        XCTAssertEqual(counts[.all], 7)
        XCTAssertEqual(counts[.previewable], 3)
        XCTAssertEqual(counts[.images], 3)
        XCTAssertEqual(counts[.documents], 2)

        XCTAssertEqual(PreviewFilterLogic.pickNearest(items: list, currentID: "4", filter: .documents).activeID, "4")
        XCTAssertEqual(PreviewFilterLogic.pickNearest(items: list, currentID: "3", filter: .images).activeID, "2")
        XCTAssertEqual(PreviewFilterLogic.pickNearest(items: list, currentID: "3", filter: .previewable).activeID, "4")
    }

    func testDecodesSafeMarkdownAstJson() throws {
        let json = """
        {
          "blocks": [
            {"type": "heading", "level": 1, "inlines": [{"type": "text", "text": "Title"}]},
            {"type": "paragraph", "inlines": [
              {"type": "strong", "children": [{"type": "text", "text": "Bold"}]},
              {"type": "blocked_image", "alt": "pixel"}
            ]},
            {"type": "code_block", "language": "swift", "code": "let x = 1"},
            {"type": "thematic_break"}
          ],
          "truncated": true
        }
        """
        let doc = try JSONDecoder().decode(MarkdownDocument.self, from: Data(json.utf8))
        XCTAssertTrue(doc.truncated)
        XCTAssertEqual(doc.blocks.count, 4)
        if case .heading(let level, _) = doc.blocks[0] {
            XCTAssertEqual(level, 1)
        } else {
            XCTFail("Expected heading block")
        }
    }
}
