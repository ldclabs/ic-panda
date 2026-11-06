import SwiftUI
@preconcurrency import DMsgCore

struct FileBackupView: View {
    let files: [LibraryFile]
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var capacity: FileBackupCapacity?
    @State private var loading = true
    @State private var failure: String?
    @State private var working = false
    @State private var outcomes: [String: FileActionResult] = [:]
    @State private var completed: Set<String> = []
    @State private var visible = true

    var body: some View {
        List {
            Section {
                Text("The encrypted version will use cloud storage. Your local copy stays on this device.").font(.footnote).foregroundStyle(.secondary)
                if loading { ProgressView("Loading…") }
                if let capacity {
                    if let limit = capacity.backupMaxFileBytes { LabeledContent("Cloud backup file limit", value: fileLimit(limit)) }
                    if let remaining = capacity.remaining { LabeledContent("Remaining", value: fileSize(remaining)) }
                }
                if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
                NavigationLink("Current plan and limits") { SubscriptionView() }
            }
            ForEach(files) { file in
                VStack(alignment: .leading, spacing: 8) {
                    FileSummaryRow(name: file.name, mime: file.mime, subtitle: fileSize(file.size))
                    if let outcome = outcomes[file.id] {
                        Label(outcome.message, systemImage: outcome.failed ? "exclamationmark.circle" : "checkmark.circle").font(.footnote)
                    } else if let limit = capacity?.backupMaxFileBytes, file.size > limit {
                        Text("This file exceeds your current cloud backup file limit.").font(.footnote).foregroundStyle(.secondary)
                    }
                }
            }
            if working { ProgressView("Preparing backups…") }
            if completed.count < files.count {
                SettingsAction("Back up encrypted copy", glyph: .cloudUp, prominent: true) { Task { await submit() } }
                    .disabled(working || loading || capacity?.backupMaxFileBytes == nil).accessibilityIdentifier("files.backup.submit")
            }
            if !outcomes.isEmpty { NavigationLink("File transfers") { TransfersView() } }
        }.privateScreen().navigationTitle("Back up to cloud").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.disabled(working) } }
            .task { await load() }.onAppear { visible = true }.onDisappear { visible = false }.interactiveDismissDisabled(working)
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        do {
            let value: FileBackupCapacity
            #if DEBUG
            if model.uiTesting { value = FileBackupCapacity(backupMaxFileBytes: 100 * 1024 * 1024, storageLimit: 1024 * 1024 * 1024, committed: 0, reserved: 0) }
            else { value = try await readCapacity() }
            #else
            value = try await readCapacity()
            #endif
            guard stamp == model.epoch, !Task.isCancelled else { return }
            capacity = value
            if value.backupMaxFileBytes == nil { failure = String(localized: "Your current entitlement could not be refreshed. Retry when the verification service is available.") }
        } catch { if stamp == model.epoch, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func readCapacity() async throws -> FileBackupCapacity {
        guard let engine = model.engine else { throw CancellationError() }
        return try model.decode(try await coreOperation { try await engine.fileBackupCapacity(operation: $0) })
    }

    private func submit() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; defer { working = false }
        for file in files where !completed.contains(file.id) {
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            do {
                let result = try await model.queueFileBackup(file)
                guard stamp == model.epoch, visible, !Task.isCancelled else { return }
                outcomes[file.id] = FileActionResult(name: file.name, message: result.message, failed: !result.accepted)
                if result.accepted { completed.insert(file.id) }
            } catch { if stamp == model.epoch, visible, !Task.isCancelled { guard let message = model.fileFailureMessage(error) else { return }; outcomes[file.id] = FileActionResult(name: file.name, message: message, failed: true) } }
        }
    }
}
