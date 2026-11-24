import SwiftUI

/// Renders pre-sliced highlight spans using palette-backed tokens (`Theme.fill` + semibold).
func highlightedText(_ fallback: String, spans: [HighlightSpan]?) -> AttributedString {
    guard let spans, !spans.isEmpty else { return AttributedString(fallback) }
    var output = AttributedString()
    for span in spans {
        var piece = AttributedString(span.text)
        if span.matched {
            piece.backgroundColor = Theme.fill
            piece.inlinePresentationIntent = .stronglyEmphasized
        }
        output.append(piece)
    }
    return output
}

/// Renders matched term highlights inside a conversation bubble while keeping own-bubble contrast high.
/// The bubble always shows the full message: spans that slice a shortened snippet only name the terms to mark.
func bubbleHighlightedText(_ text: String, spans: [HighlightSpan]?, own: Bool) -> AttributedString {
    guard let spans, !spans.isEmpty else { return AttributedString(text) }
    func marked(_ value: Substring) -> AttributedString {
        var piece = AttributedString(String(value))
        piece.inlinePresentationIntent = .stronglyEmphasized
        if own {
            piece.underlineStyle = .single
        } else {
            piece.backgroundColor = Theme.fill
        }
        return piece
    }
    if spans.map(\.text).joined() == text {
        var output = AttributedString()
        for span in spans {
            output.append(span.matched ? marked(Substring(span.text)) : AttributedString(span.text))
        }
        return output
    }
    var ranges: [Range<String.Index>] = []
    for term in Set(spans.filter { $0.matched && !$0.text.isEmpty }.map(\.text)) {
        var from = text.startIndex
        while from < text.endIndex,
              let found = text.range(of: term, options: .caseInsensitive, range: from..<text.endIndex) {
            ranges.append(found)
            from = found.upperBound
        }
    }
    guard !ranges.isEmpty else { return AttributedString(text) }
    var merged: [Range<String.Index>] = []
    for range in ranges.sorted(by: { $0.lowerBound < $1.lowerBound }) {
        if let last = merged.last, range.lowerBound <= last.upperBound {
            merged[merged.count - 1] = last.lowerBound..<max(last.upperBound, range.upperBound)
        } else {
            merged.append(range)
        }
    }
    var output = AttributedString()
    var position = text.startIndex
    for range in merged {
        output.append(AttributedString(String(text[position..<range.lowerBound])))
        output.append(marked(text[range]))
        position = range.upperBound
    }
    output.append(AttributedString(String(text[position...])))
    return output
}

/// Safe secondary match explanation for Vault items (preserves V01: never exposes notes plaintext).
func vaultMatchHint(matchField: String?, matchValue: String?, subtitle: String) -> String? {
    guard let field = matchField, field != "title", field != "username" else { return nil }
    if field == "notes" { return String(localized: "Matched in notes") }
    if field == "tag", let value = matchValue, !value.isEmpty { return "#" + value }
    if let value = matchValue, !value.isEmpty, value != subtitle { return value }
    return nil
}

/// Owns the async global search state for a workspace `List` or sheet.
/// Attaching `.task` on the enclosing `List` (via `.globalSearch(...)`) avoids SwiftUI's
/// `List` quirk where `.task` on an initially empty `Group` never fires.
@Observable @MainActor
final class GlobalSearchStore {
    var result: GlobalSearchResponse?
    var loading = false
    var failed = false
    var retry = 0

    func clear() {
        result = nil
        loading = false
        failed = false
    }

    func run(query: String, model: SessionModel) async {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty, model.unlocked, !model.concealed else {
            clear()
            return
        }
        let stamp = model.epoch
        loading = true
        failed = false
        defer { if stamp == model.epoch { loading = false } }
        do {
            try await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled, stamp == model.epoch else { return }
            let value = try await model.searchGlobal(query: term)
            guard !Task.isCancelled, stamp == model.epoch else { return }
            result = value
        } catch {
            if !Task.isCancelled, stamp == model.epoch { failed = true }
        }
    }
}

private struct GlobalSearchModifier: ViewModifier {
    @Environment(SessionModel.self) private var model
    let store: GlobalSearchStore
    let query: String
    let active: Bool

    private var trimmed: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var taskKey: String {
        guard active, !trimmed.isEmpty, model.unlocked, !model.concealed else { return "" }
        return "\(model.epoch):\(model.contentRevision):\(trimmed):\(store.retry)"
    }

    func body(content: Content) -> some View {
        content
            .task(id: taskKey) {
                guard !taskKey.isEmpty else {
                    store.clear()
                    return
                }
                await store.run(query: trimmed, model: model)
            }
            .onChange(of: model.epoch) { _, _ in store.clear() }
            .onChange(of: active) { _, isActive in if !isActive { store.clear() } }
    }
}

extension View {
    /// Drives a `GlobalSearchStore` from the enclosing `List` lifecycle.
    func globalSearch(store: GlobalSearchStore, query: String, active: Bool) -> some View {
        modifier(GlobalSearchModifier(store: store, query: query, active: active))
    }
}

/// Grouped cross-workspace results rendered when the user selects the "All" search scope
/// inside Vault, Messages, or Files, or inside the hardware-keyboard global search sheet.
struct GlobalSearchResultsContent: View {
    let store: GlobalSearchStore
    let query: String
    var onSelect: (() -> Void)? = nil
    @Environment(SessionModel.self) private var model
    @Environment(\.horizontalSizeClass) private var sizeClass

    private var trimmed: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        Group {
            if trimmed.isEmpty {
                ContentUnavailableView("Search everything", systemImage: "magnifyingglass")
            } else if store.failed {
                NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                Button("Retry") { store.retry += 1 }
            } else if let result = store.result, result.query == trimmed {
                if result.isEmpty {
                    ContentUnavailableView.search(text: trimmed)
                } else {
                    if !result.vault.isEmpty {
                        Section("Vault") {
                            ForEach(result.vault, id: \.id) { summary in
                                Button {
                                    model.openGlobalVaultHit(summary.id)
                                    onSelect?()
                                } label: {
                                    HStack(spacing: 10) {
                                        ObjectIcon(glyph: .vaultItem(summary.kind))
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(highlightedText(summary.title, spans: summary.titleSpans))
                                                .font(.body.weight(.medium)).lineLimit(1)
                                            let hint = vaultMatchHint(matchField: summary.matchField, matchValue: summary.matchValue, subtitle: summary.subtitle)
                                            if summary.subtitle.isEmpty {
                                                if let hint {
                                                    (Text(vaultKindName(summary.kind)) + Text(verbatim: " · " + hint))
                                                        .rowSubtitle().lineLimit(1)
                                                } else {
                                                    Text(vaultKindName(summary.kind)).rowSubtitle().lineLimit(1)
                                                }
                                            } else if let hint {
                                                Text(highlightedText(summary.subtitle, spans: summary.subtitleSpans) + AttributedString(" · " + hint))
                                                    .rowSubtitle().lineLimit(1)
                                            } else {
                                                Text(highlightedText(summary.subtitle, spans: summary.subtitleSpans))
                                                    .rowSubtitle().lineLimit(1)
                                            }
                                        }.frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                }.buttonStyle(.plain).compactObjectRow().accessibilityIdentifier("global.vault.\(summary.id)")
                            }
                        }
                    }
                    if !model.localAccount && !result.channels.isEmpty {
                        Section("Conversations") {
                            ForEach(result.channels) { hit in
                                Button {
                                    model.openGlobalChannelHit(hit.id)
                                    onSelect?()
                                } label: {
                                    HStack(spacing: 10) {
                                        Avatar(hit.name, seed: hit.id, kind: .conversation)
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(highlightedText(hit.name, spans: hit.nameSpans))
                                                .font(.body.weight(.medium)).lineLimit(1)
                                            if hit.matchField == "member", let member = hit.matchValue, !member.isEmpty {
                                                Text(String(localized: "Member") + ": " + member).rowSubtitle().lineLimit(1)
                                            }
                                        }.frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                }.buttonStyle(.plain).compactObjectRow()
                            }
                        }
                    }
                    if !model.localAccount && !result.messages.isEmpty {
                        Section("Messages") {
                            ForEach(result.messages) { hit in
                                Button {
                                    model.openGlobalChannelHit(hit.channel, sequence: hit.sequence)
                                    onSelect?()
                                } label: {
                                    let channelName = model.channels.first(where: { $0.id == hit.channel })?.name
                                    let bodyFallback = hit.text.isEmpty ? (hit.attachmentName ?? "") : hit.text
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(highlightedText(bodyFallback, spans: hit.snippetSpans)).lineLimit(2)
                                        let meta = [channelName, hit.senderName].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
                                        HStack(spacing: 6) {
                                            if !meta.isEmpty { Text(meta).lineLimit(1) }
                                            Spacer(minLength: 4)
                                            if hit.message.timestampMs > 0 {
                                                Text(ConversationTimeline.date(hit.message.timestampMs), style: .date)
                                            }
                                        }.font(.footnote).foregroundStyle(.secondary)
                                    }.frame(maxWidth: .infinity, alignment: .leading)
                                }.buttonStyle(.plain)
                            }
                        }
                    }
                    if !model.localAccount && !result.letters.isEmpty {
                        Section("Letters") {
                            ForEach(result.letters) { hit in
                                Button {
                                    model.openGlobalLetterHit(hit)
                                    onSelect?()
                                } label: {
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(highlightedText(hit.displayName ?? hit.account, spans: hit.nameSpans))
                                            .font(.body.weight(.medium)).lineLimit(1)
                                        if let spans = hit.snippetSpans, !spans.isEmpty {
                                            Text(highlightedText("", spans: spans)).rowSubtitle().lineLimit(2)
                                        }
                                    }.frame(maxWidth: .infinity, alignment: .leading)
                                }.buttonStyle(.plain)
                            }
                        }
                    }
                    if !result.files.isEmpty {
                        Section("Files") {
                            ForEach(result.files) { hit in
                                Button {
                                    model.openGlobalFileHit(hit, compact: sizeClass == .compact)
                                    onSelect?()
                                } label: {
                                    let pathText = fileFolderPath(hit.path)
                                    let detail: String = {
                                        if hit.isFolder { return pathText.isEmpty ? String(localized: "Folder") : pathText }
                                        if hit.matchField == "tag", let tag = hit.matchValue, !tag.isEmpty {
                                            return pathText.isEmpty ? "#" + tag : pathText + " · #" + tag
                                        }
                                        return pathText
                                    }()
                                    HStack(alignment: .top, spacing: 10) {
                                        if hit.isFolder { ObjectIcon(glyph: .folder) }
                                        else { ObjectIcon(name: fileSymbol(hit.mime ?? "")) }
                                        VStack(alignment: .leading, spacing: 3) {
                                            Text(highlightedText(hit.name, spans: hit.nameSpans))
                                                .font(.body.weight(.medium)).lineLimit(2).truncationMode(.middle)
                                            if !detail.isEmpty {
                                                Text(highlightedText(detail, spans: hit.pathSpans))
                                                    .rowSubtitle().lineLimit(2)
                                            }
                                        }.frame(maxWidth: .infinity, alignment: .leading)
                                    }
                                }.buttonStyle(.plain).compactObjectRow()
                            }
                        }
                    }
                }
            } else {
                ProgressView("Searching…").frame(maxWidth: .infinity)
            }
        }
    }
}

/// Hardware-keyboard (`⌘K`) global search sheet for iPad and external keyboards.
struct GlobalSearchSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var globalSearch = GlobalSearchStore()

    var body: some View {
        NavigationStack {
            List {
                GlobalSearchResultsContent(store: globalSearch, query: query) { dismiss() }
            }
            .listStyle(.plain)
            .globalSearch(store: globalSearch, query: query, active: true)
            .privateScreen(.plain)
            .navigationTitle("Search")
            .navigationBarTitleDisplayMode(.inline)
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search everything")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
