import SwiftUI
import DMsgCore

struct FileImportView: View {
    let urls: [URL]
    let folder: String
    let open: (LibraryFile) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var results: [String: FileActionResult] = [:]
    @State private var imported: [LibraryFile] = []
    @State private var working = false
    @State private var started = false
    @State private var visible = true

    var body: some View {
        List {
            TransferStatusView()
            ForEach(Array(urls.enumerated()), id: \.offset) { _, url in
                VStack(alignment: .leading, spacing: 6) {
                    Text(url.lastPathComponent).font(.body.weight(.medium))
                    if let result = results[url.absoluteString] {
                        Label(result.message, systemImage: result.failed ? "exclamationmark.circle" : "checkmark.circle").font(.footnote)
                    } else { Text("Waiting").font(.caption).foregroundStyle(.secondary) }
                }
            }
            if !working {
                if let file = imported.last {
                    SettingsAction("Open local file", systemImage: "doc") { open(file); dismiss() }
                }
                if results.values.contains(where: \.failed) {
                    if model.filesPendingImports == 0 { SettingsAction("Retry") { Task { await run() } } }
                    NavigationLink("Pending imports") { PendingImportsView() }
                }
            }
        }.privateScreen().navigationTitle("Import file")
            .toolbar { ToolbarItem(placement: .confirmationAction) {
                Button(working ? "Pause transfer" : "Done") { if working { model.transfer.cancel() } else { dismiss() } }
            } }
            .task { if !started { started = true; await run() } }
            .onAppear { visible = true }.onDisappear { visible = false }.interactiveDismissDisabled(working)
    }

    private func run() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; defer { working = false }
        for url in urls where results[url.absoluteString]?.failed != false {
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            do {
                let file = try await model.importLibraryDocument(url, folder: folder)
                guard stamp == model.epoch, visible, !Task.isCancelled else { return }
                imported.append(file)
                results[url.absoluteString] = FileActionResult(name: file.name, message: String(localized: "Imported to this device. Cloud backup is a separate action."))
            } catch {
                guard stamp == model.epoch, visible, !Task.isCancelled else { return }
                if (error as? MobileError)?.fileCode == "CANCELLED" {
                    results[url.absoluteString] = FileActionResult(name: url.lastPathComponent, message: String(localized: "Transfer paused"), failed: true); break
                }
                guard let message = model.fileFailureMessage(error) else { return }
                results[url.absoluteString] = FileActionResult(name: url.lastPathComponent, message: message, failed: true)
            }
        }
        guard stamp == model.epoch else { return }; await model.loadFiles()
    }
}
