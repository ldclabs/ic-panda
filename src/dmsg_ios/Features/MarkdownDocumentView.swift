import SwiftUI

/// Native SwiftUI renderer for the bounded, safe `MarkdownDocument` AST produced by Rust.
/// Uses system text styles and shared `Theme` tokens exclusively (no WebView, no raw HTML).
struct MarkdownDocumentView: View {
    let document: MarkdownDocument
    @Environment(SessionModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var pendingURL: URL?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            renderBlocks(document.blocks)
            if document.truncated {
                NoticeBar(text: String(localized: "Long document truncated for preview. Download or open externally to view all content."))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .textSelection(.enabled)
        .environment(\.openURL, OpenURLAction { url in
            if let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" {
                pendingURL = url
                return .handled
            }
            if url.scheme?.lowercased() == "mailto" {
                let address = url.absoluteString.replacingOccurrences(of: "mailto:", with: "", options: [.anchored, .caseInsensitive])
                model.clipboard.copy(address)
                model.notice = String(localized: "Copied")
                return .handled
            }
            return .discarded
        })
        .alert(
            String(localized: "Open external link?"),
            isPresented: Binding(get: { pendingURL != nil }, set: { if !$0 { pendingURL = nil } })
        ) {
            Button(String(localized: "Cancel"), role: .cancel) { pendingURL = nil }
            Button(String(localized: "Open")) {
                if let url = pendingURL {
                    pendingURL = nil
                    openURL(url)
                }
            }
        } message: {
            Text(pendingURL?.absoluteString ?? "")
        }
    }

    @ViewBuilder
    private func renderBlocks(_ blocks: [MarkdownBlock]) -> some View {
        ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
            renderBlock(block)
        }
    }

    @ViewBuilder
    private func renderBlock(_ block: MarkdownBlock) -> some View {
        switch block {
        case .heading(let level, let inlines):
            VStack(alignment: .leading, spacing: 6) {
                Text(attributed(from: inlines))
                    .font(headingFont(level))
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                if level <= 2 { Divider() }
            }
            .padding(.top, level <= 2 ? 6 : 2)

        case .paragraph(let inlines):
            Text(attributed(from: inlines))
                .font(.body)
                .foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)

        case .codeBlock(let language, let code):
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 8) {
                    Text(verbatim: language?.isEmpty == false ? language! : "text")
                        .font(.footnote.monospaced())
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button {
                        model.clipboard.copy(code)
                        model.notice = String(localized: "Copied")
                    } label: {
                        Label(String(localized: "Copy"), systemImage: "doc.on.doc")
                            .font(.footnote)
                            .foregroundStyle(Theme.forest)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                Divider()
                ScrollView(.horizontal, showsIndicators: true) {
                    Text(verbatim: code)
                        .font(.footnote.monospaced())
                        .foregroundStyle(.primary)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .environment(\.layoutDirection, .leftToRight)
            }
            .background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: Theme.radius, style: .continuous)
                    .strokeBorder(Theme.fill, lineWidth: 1)
            }

        case .blockQuote(let nested):
            HStack(alignment: .top, spacing: 10) {
                RoundedRectangle(cornerRadius: Theme.radiusSmall)
                    .fill(Theme.forest)
                    .frame(width: 3)
                VStack(alignment: .leading, spacing: 8) {
                    AnyView(renderBlocks(nested))
                }
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 8)
            .background(Theme.fill, in: RoundedRectangle(cornerRadius: Theme.radiusSmall, style: .continuous))

        case .list(let ordered, let start, let items):
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(items.enumerated()), id: \.offset) { idx, item in
                    HStack(alignment: .top, spacing: 8) {
                        if let checked = item.checked {
                            Image(systemName: checked ? "checkmark.square.fill" : "square")
                                .font(.body)
                                .foregroundStyle(checked ? Theme.forest : Color.secondary)
                                .accessibilityLabel(checked ? String(localized: "Completed") : String(localized: "Incomplete"))
                        } else if ordered {
                            Text(verbatim: "\(start + UInt64(idx)).")
                                .font(.body.monospacedDigit())
                                .foregroundStyle(.secondary)
                        } else {
                            Text(verbatim: "•")
                                .font(.body)
                                .foregroundStyle(.secondary)
                        }
                        VStack(alignment: .leading, spacing: 6) {
                            AnyView(renderBlocks(item.blocks))
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .padding(.leading, 4)

        case .table(let alignments, let headers, let rows):
            ScrollView(.horizontal, showsIndicators: true) {
                Grid(alignment: .leading, horizontalSpacing: 0, verticalSpacing: 0) {
                    if !headers.isEmpty {
                        GridRow {
                            ForEach(Array(headers.enumerated()), id: \.offset) { col, cell in
                                Text(attributed(from: cell))
                                    .font(.subheadline.weight(.semibold))
                                    .multilineTextAlignment(textAlignment(alignments, col))
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 8)
                                    .frame(maxWidth: .infinity, alignment: frameAlignment(alignments, col))
                            }
                        }
                        .background(Theme.fill)
                        Divider()
                    }
                    ForEach(Array(rows.enumerated()), id: \.offset) { rowIdx, row in
                        GridRow {
                            ForEach(Array(row.enumerated()), id: \.offset) { col, cell in
                                Text(attributed(from: cell))
                                    .font(.subheadline)
                                    .multilineTextAlignment(textAlignment(alignments, col))
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 8)
                                    .frame(maxWidth: .infinity, alignment: frameAlignment(alignments, col))
                            }
                        }
                        if rowIdx < rows.count - 1 { Divider() }
                    }
                }
                .background(Theme.card, in: RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: Theme.radius, style: .continuous)
                        .strokeBorder(Theme.fill, lineWidth: 1)
                }
            }

        case .thematicBreak:
            Divider().padding(.vertical, 4)
        }
    }

    private func headingFont(_ level: Int) -> Font {
        switch level {
        case 1: .title.bold()
        case 2: .title2.bold()
        case 3: .title3.weight(.semibold)
        default: .headline
        }
    }

    private func textAlignment(_ alignments: [MarkdownTableAlignment], _ index: Int) -> TextAlignment {
        guard alignments.indices.contains(index) else { return .leading }
        switch alignments[index] {
        case .center: return .center
        case .right: return .trailing
        case .left, .none: return .leading
        }
    }

    private func frameAlignment(_ alignments: [MarkdownTableAlignment], _ index: Int) -> Alignment {
        guard alignments.indices.contains(index) else { return .leading }
        switch alignments[index] {
        case .center: return .center
        case .right: return .trailing
        case .left, .none: return .leading
        }
    }

    private func attributed(from inlines: [MarkdownInline]) -> AttributedString {
        var result = AttributedString()
        for node in inlines {
            result.append(renderInline(node))
        }
        return result
    }

    private func renderInline(_ node: MarkdownInline) -> AttributedString {
        switch node {
        case .text(let value):
            return AttributedString(value)
        case .strong(let children):
            var sub = attributed(from: children)
            sub.inlinePresentationIntent = .stronglyEmphasized
            return sub
        case .emphasis(let children):
            var sub = attributed(from: children)
            sub.inlinePresentationIntent = .emphasized
            return sub
        case .strikethrough(let children):
            var sub = attributed(from: children)
            sub.strikethroughStyle = .single
            return sub
        case .code(let value):
            var sub = AttributedString(value)
            sub.inlinePresentationIntent = .code
            sub.backgroundColor = Theme.fill
            return sub
        case .link(let rawURL, let children):
            var sub = attributed(from: children)
            if let url = URL(string: rawURL) {
                sub.link = url
                sub.foregroundColor = Theme.forest
                sub.underlineStyle = .single
            }
            return sub
        case .blockedImage(let alt):
            let label = alt.isEmpty
                ? "[\(String(localized: "External image blocked"))]"
                : "[\(String(localized: "External image blocked")): \(alt)]"
            var sub = AttributedString(label)
            sub.foregroundColor = .secondary
            return sub
        case .softBreak:
            return AttributedString(" ")
        case .hardBreak:
            return AttributedString("\n")
        }
    }
}
