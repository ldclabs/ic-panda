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
    case rename(LibraryItemTarget), move([LibraryItemTarget]), backup([LibraryFile])
    case results([FileActionResult]), imports([URL], String)
    var id: String {
        switch self {
        case .trash: "trash"
        case .help: "help"
        case .create: "create"
        case .rename(let item): "rename:" + item.id
        case .move: "move"
        case .backup: "backup"
        case .results: "results"
        case .imports: "imports"
        }
    }
}

struct FileBrowseBar: View {
    @Binding var scope: FileScope
    @Binding var sort: FileSort
    @Environment(\.dynamicTypeSize) private var textSize
    var body: some View {
        HStack(spacing: 4) {
            if textSize.isAccessibilitySize { scopeMenu }
            else {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 6) {
                        button(.local, title: "On this device", symbol: "internaldrive")
                        button(.cloud, title: "Cloud backups", symbol: "icloud")
                    }.padding(.leading, 16).fixedSize(horizontal: true, vertical: false)
                    scopeMenu
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            Menu {
                Picker("Sort by", selection: $sort) {
                    ForEach(FileSort.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            } label: { Image(systemName: "arrow.up.arrow.down").font(.system(size: 18)).frame(width: 44, height: 44) }
                .accessibilityLabel("Sort by").accessibilityValue(sort.title).accessibilityIdentifier("files.sort")
        }.font(.subheadline).background(Theme.paper)
    }

    private var scopeMenu: some View {
        Menu {
            Picker("Files", selection: $scope) {
                Text("On this device").tag(FileScope.local)
                Text("Cloud backups").tag(FileScope.cloud)
            }
        } label: {
            HStack(spacing: 8) {
                Text(scope == .local ? "On this device" : "Cloud backups").fixedSize(horizontal: false, vertical: true)
                Image(systemName: "chevron.down").font(.system(size: 16)).accessibilityHidden(true)
            }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 8)
        }.accessibilityIdentifier("files.scope.\(scope.rawValue)")
    }

    private func button(_ value: FileScope, title: LocalizedStringKey, symbol: String) -> some View {
        Button { scope = value } label: {
            Label(title, systemImage: symbol).padding(.horizontal, 12).frame(minHeight: 44)
                .foregroundStyle(scope == value ? Theme.forest : Color.secondary)
                .background(scope == value ? Theme.forest.opacity(0.10) : .clear, in: Capsule())
        }.buttonStyle(.plain).accessibilityAddTraits(scope == value ? .isSelected : [])
            .accessibilityIdentifier("files.scope.\(value.rawValue)")
    }
}

struct FileSummaryRow: View {
    let name: String
    let mime: String
    let subtitle: String
    var expanded = false
    @Environment(\.dynamicTypeSize) private var textSize

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            if mime == "inode/directory" { ObjectIcon(glyph: .folder) }
            else { ObjectIcon(name: fileSymbol(mime)) }
            VStack(alignment: .leading, spacing: 3) {
                Text(name).font(.body.weight(.medium)).lineLimit(expanded ? nil : textSize.isAccessibilitySize ? 3 : 2).truncationMode(.middle)
                    .accessibilityLabel(name)
                if !subtitle.isEmpty { Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(expanded ? nil : 2).fixedSize(horizontal: false, vertical: true) }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }.accessibilityElement(children: .combine)
    }
}

struct FileRow: View {
    let file: LibraryFile
    @Environment(SessionModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            FileSummaryRow(name: file.name, mime: file.mime, subtitle: fileSize(file.size) + " · " + file.statusLabel)
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
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List {
            Section("On this device") {
                Text("Import a document to encrypt it on this device. Cloud backup is a separate action.")
                Text("Local files are not backed up automatically. Uninstalling dMsg removes files that have no cloud backup.")
                LabeledContent("Import file", value: "1000 MiB")
                LabeledContent("Conversation attachments", value: "100 MiB")
            }
            Section("Cloud backups") {
                Text("Refreshing this list does not back up local files. Choose Back up to cloud for the files you want to protect.")
                Text("Restoring downloads and verifies the selected version before it can be opened.")
                NavigationLink("Current plan and limits") { SubscriptionView() }
            }
            Section("Background ciphertext transfer") {
                Text("Only the selected encrypted file can transfer while dMsg is locked. iOS controls scheduling; force quitting stops delivery until you open the app again. Open the original account to verify and finish.")
            }
            Section("Trash") { Text("Cloud backups are kept. This only changes the copy on this device.") }
        }.privateScreen().navigationTitle("About files and backups").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
    }
}
