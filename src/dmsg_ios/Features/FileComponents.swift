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
    @Binding var scope: FileScope
    @Binding var sort: FileSort
    @Binding var showingTransfers: Bool
    @Environment(SessionModel.self) private var model
    var body: some View {
        let active = model.fileTransfers.activeCount
        HStack(spacing: 4) {
            BrowseScopes(title: "Files", selection: $scope, options: [
                .init(value: .local, title: "On this device", icon: Image(systemName: "internaldrive"), identifier: "files.scope.local"),
                .init(value: .cloud, title: "Cloud backups", icon: Image(systemName: "icloud"), identifier: "files.scope.cloud"),
            ])
            Button { showingTransfers = true } label: {
                Image(systemName: "arrow.left.arrow.right").font(.system(size: 18)).frame(width: 44, height: 44)
                    .overlay(alignment: .topTrailing) {
                        if active > 0 {
                            Text(active.formatted()).font(.caption2.bold().monospacedDigit()).foregroundStyle(Theme.paper)
                                .padding(.horizontal, 4).background(Theme.forest, in: Capsule())
                        }
                    }
            }.accessibilityLabel("File transfers").accessibilityValue(active > 0 ? active.formatted() : "")
                .accessibilityIdentifier("files.transfers")
            Menu {
                Picker("Sort by", selection: $sort) {
                    ForEach(FileSort.allCases, id: \.self) { Text($0.title).tag($0) }
                }
            } label: { Image(systemName: "arrow.up.arrow.down").font(.system(size: 18)).frame(width: 44, height: 44) }
                .accessibilityLabel("Sort by").accessibilityValue(sort.title).accessibilityIdentifier("files.sort")
        }.font(.subheadline).padding(.bottom, 8).background(Theme.paper)
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
