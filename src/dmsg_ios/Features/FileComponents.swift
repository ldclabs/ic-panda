import SwiftUI

struct LibraryItemTarget: Identifiable {
    let id: String
    let name: String
    let folder: Bool
}

struct FileActionResult: Identifiable {
    let id = UUID()
    let name: String
    let message: String
    var failed = false
}

enum FileLibrarySheet: Identifiable {
    case trash, help, create
    case rename(LibraryItemTarget), move([LibraryItemTarget]), backup([LibraryFile]), link(FileLinkTarget)
    case results([FileActionResult]), imports([URL], String)
    var id: String {
        switch self {
        case .trash: "trash"
        case .help: "help"
        case .create: "create"
        case .rename(let item): "rename:" + item.id
        case .move: "move"
        case .backup: "backup"
        case .link: "link"
        case .results: "results"
        case .imports: "imports"
        }
    }
}

struct FileBrowseBar: View {
    var showsScopes = true
    @Binding var scope: FileScope
    @Binding var sort: FileSort
    let openTransfers: () -> Void
    @Environment(SessionModel.self) private var model
    var body: some View {
        let active = model.fileTransfers.activeCount
        HStack(spacing: 4) {
            if showsScopes && model.localAccount {
                Label("On this device", systemImage: "internaldrive").font(.subheadline.weight(.medium))
                    .accessibilityIdentifier("files.scope.local")
                Spacer()
            } else if showsScopes {
                BrowseScopes(title: "Files", selection: $scope, options: [
                    .init(value: .local, title: "On this device", icon: Image(systemName: "internaldrive"), identifier: "files.scope.local"),
                    .init(value: .cloud, title: "Cloud backups", icon: AppGlyph.sync.image, identifier: "files.scope.cloud"),
                ])
            } else { Spacer() }
            Button(action: openTransfers) {
                // Icon buttons stop growing where they would outgrow their 44 pt targets, as bar buttons do.
                Image(systemName: "arrow.left.arrow.right").dynamicTypeSize(...DynamicTypeSize.xxxLarge)
                    .frame(width: 44, height: 44).contentShape(Rectangle())
                    .overlay(alignment: .topTrailing) {
                        if active > 0 {
                            Text(active.formatted()).font(.caption2.bold().monospacedDigit()).foregroundStyle(Theme.paper)
                                .padding(.horizontal, 4).background(Theme.forest, in: Capsule())
                        }
                    }
            }.buttonStyle(.plain).accessibilityLabel("File transfers")
                .accessibilityValue(active > 0 ? active.formatted() : "").accessibilityIdentifier("files.transfers")
            if !showsScopes || scope == .local {
                let filtered = model.filesFavoritesOnly || !model.filesTag.isEmpty
                Menu {
                    Button {
                        model.filesFavoritesOnly.toggle()
                    } label: {
                        if model.filesFavoritesOnly { Label("Favorites", systemImage: "checkmark") }
                        else { Label("Favorites", systemImage: "star") }
                    }
                    if !model.fileTags.isEmpty || !model.filesTag.isEmpty {
                        Section("Tags") {
                            Button {
                                model.filesTag = ""
                            } label: {
                                if model.filesTag.isEmpty { Label("All", systemImage: "checkmark") }
                                else { Text("All") }
                            }
                            ForEach(model.fileTags) { entry in
                                Button {
                                    model.filesTag = model.filesTag == entry.tag ? "" : entry.tag
                                } label: {
                                    if model.filesTag == entry.tag {
                                        Label { Text(verbatim: "#" + entry.tag) } icon: { Image(systemName: "checkmark") }
                                    } else {
                                        Text(verbatim: "#" + entry.tag)
                                    }
                                }
                            }
                        }
                    }
                    if filtered {
                        Divider()
                        Button("Clear filters") {
                            model.filesFavoritesOnly = false
                            model.filesTag = ""
                        }
                    }
                } label: {
                    Image(systemName: filtered ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease")
                        .font(.body).dynamicTypeSize(...DynamicTypeSize.xxxLarge).frame(width: 44, height: 44)
                }.accessibilityLabel("Filters").accessibilityValue(model.filesTag).accessibilityIdentifier("files.filters")
            }
            Menu {
                Picker("Sort by", selection: $sort) {
                    ForEach(FileSort.allCases, id: \.self) { order in
                        let ranked = order == .name && scope == .local && !model.filesSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        Text(ranked ? String(localized: "Best match") : order.title).tag(order)
                    }
                }
            } label: {
                Image(systemName: "arrow.up.arrow.down").font(.body).dynamicTypeSize(...DynamicTypeSize.xxxLarge).frame(width: 44, height: 44)
            }.accessibilityLabel("Sort by").accessibilityValue(Text(sort.title))
                .accessibilityIdentifier("files.options")
        }.font(.subheadline).padding(.bottom, 8)
    }
}

struct FileSummaryRow: View {
    let name: String
    let mime: String
    let subtitle: String
    var expanded = false
    var thumbnail: LibraryFile? = nil
    var nameSpans: [HighlightSpan]? = nil
    var subtitleSpans: [HighlightSpan]? = nil
    @Environment(\.dynamicTypeSize) private var textSize

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            if mime == "inode/directory" { ObjectIcon(glyph: .folder) }
            else {
                ObjectIcon(name: fileSymbol(mime)).overlay {
                    if let thumbnail, ThumbnailStore.supports(thumbnail) {
                        // The clear square keeps a filled image to the tile's scaled size.
                        Color.clear.overlay { LibraryThumbnail(file: thumbnail) }
                            .clipShape(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
                    }
                }
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(highlightedText(name, spans: nameSpans))
                    .font(.body.weight(.medium)).lineLimit(expanded || textSize.isAccessibilitySize ? nil : 2).truncationMode(.middle)
                    .accessibilityLabel(name)
                if !subtitle.isEmpty {
                    Text(highlightedText(subtitle, spans: subtitleSpans))
                        .rowSubtitle().lineLimit(expanded || textSize.isAccessibilitySize ? nil : 2).fixedSize(horizontal: false, vertical: true)
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }.accessibilityElement(children: .combine)
    }
}

struct FileRow: View {
    let file: LibraryFile
    var showsLocation = false
    @Environment(SessionModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            let meta = showsLocation ? model.fileSearchMeta[file.id] : nil
            let details = fileSize(file.size) + " · " + file.statusLabel
            let location = fileFolderPath(file.path)
            let tagHint = (meta?.matchField == "tag" && !(meta?.matchValue ?? "").isEmpty) ? " · #" + (meta?.matchValue ?? "") : ""
            let subtitle = showsLocation ? location + tagHint + " · " + details : details
            FileSummaryRow(name: file.name, mime: file.mime, subtitle: subtitle, thumbnail: file, nameSpans: meta?.nameSpans)
            if let job = model.fileTransfers.backups.first(where: { $0.file == file.id && $0.version == file.current }) {
                FileTransferProgressView(id: job.id, stage: job.stage, bytes: job.bytes, total: job.totalBytes, failure: job.error, retry: job.retryAt, compact: true)
            }
        }
    }
}

struct FileInlineFailure: View {
    let message: String
    var working = false
    let retry: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(message, systemImage: "exclamationmark.circle").font(.footnote)
            Button("Retry", action: retry).buttonStyle(.bordered).disabled(working)
        }.padding(.vertical, 4).accessibilityIdentifier("files.failure")
    }
}

struct FileResultsView: View {
    let results: [FileActionResult]
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List(results) { result in
            VStack(alignment: .leading, spacing: 6) {
                Text(result.name).font(.body.weight(.medium))
                Label(result.message, systemImage: result.failed ? "exclamationmark.circle" : "checkmark.circle")
                    .font(.footnote).foregroundStyle(result.failed ? Color.primary : Color.secondary)
            }.accessibilityElement(children: .combine)
        }.privateScreen().navigationTitle("Results")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
    }
}

struct FileLibraryHelpView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List {
            Section("On this device") {
                if !model.localAccount {
                    Text("Import a document to encrypt it on this device. Cloud backup is a separate action.")
                    Text("Local files are not backed up automatically. Uninstalling dMsg removes files that have no cloud backup.")
                }
                LabeledContent("Import file", value: "1000 MiB")
                    .accessibilityIdentifier("files.importLimit")
                if !model.localAccount { LabeledContent("Conversation attachments", value: "100 MiB") }
            }
            if !model.localAccount {
                Section("Cloud backups") {
                    Text("Refreshing this list does not back up local files. Choose Back up to cloud for the files you want to protect.")
                    Text("Restoring downloads and verifies the selected version before it can be opened.")
                    NavigationLink("Current plan and limits") { SubscriptionView() }
                }
                Section("Background transfer") {
                    Text("Only the selected encrypted file can transfer while dMsg is locked. iOS controls scheduling; force quitting stops delivery until you open the app again. Open the original account to verify and finish.")
                }
                Section("Trash") { Text("Cloud backups are kept. This only changes the copy on this device.") }
            }
        }.privateScreen().navigationTitle(model.localAccount ? String(localized: "Files") : String(localized: "About files and backups")).navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
    }
}
