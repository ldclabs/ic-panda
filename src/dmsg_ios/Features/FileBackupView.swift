import SwiftUI
@preconcurrency import DMsgCore

struct FileBackupView: View {
    let files: [LibraryFile]
    var explicitRevisions = false
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
            if files.count > 1 {
                LabeledContent("Selected", value: "\(files.count.formatted()) · \(fileSize(files.reduce(0) { $0 + $1.size }))")
            }
            ForEach(files, id: \.backupKey) { file in
                VStack(alignment: .leading, spacing: 8) {
                    FileSummaryRow(name: file.name, mime: file.mime, subtitle: fileSize(file.size))
                    if explicitRevisions {
                        VStack(alignment: .leading, spacing: 4) {
                            let path = file.path ?? []
                            LabeledContent("Folder", value: path.isEmpty ? String(localized: "Files") : path.joined(separator: " / "))
                            if let created = file.createdAt {
                                LabeledContent("Created") { Text(fileDate(created), format: .dateTime) }
                            }
                            LabeledContent("Version", value: file.current).textSelection(.enabled)
                                .accessibilityIdentifier("backup.revision." + file.backupKey)
                        }.font(.caption)
                    }
                    if let outcome = outcomes[file.backupKey] {
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
        for file in files where !completed.contains(file.backupKey) {
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            do {
                let result = try await model.queueFileBackup(file, explicitRevision: explicitRevisions)
                guard stamp == model.epoch, visible, !Task.isCancelled else { return }
                outcomes[file.backupKey] = FileActionResult(name: file.name, message: result.message, failed: !result.accepted)
                if result.accepted { completed.insert(file.backupKey) }
            } catch { if stamp == model.epoch, visible, !Task.isCancelled { guard let message = model.fileFailureMessage(error) else { return }; outcomes[file.backupKey] = FileActionResult(name: file.name, message: message, failed: true) } }
        }
    }
}

private extension LibraryFile {
    var backupKey: String { id + ":" + current }
}

struct UnbackedVersionsView: View {
    @Environment(SessionModel.self) private var model
    @State private var groups: [LibraryDetails] = []
    @State private var selection: Set<String> = []
    private struct Review: Identifiable { let id = UUID(); let files: [LibraryFile] }
    @State private var review: Review?
    @State private var loading = true
    @State private var failure: String?
    private var versions: [LibraryFile] { groups.flatMap { group in group.versions.map { $0.file(in: group.file) } } }
    private var selected: [LibraryFile] { versions.filter { selection.contains($0.backupKey) } }

    var body: some View {
        List {
            if loading { ProgressView("Loading…") }
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            if !loading && failure == nil && groups.isEmpty { ContentUnavailableView("All caught up", systemImage: "checkmark.icloud") }
            ForEach(groups, id: \.file.id) { group in
                Section {
                    ForEach(group.versions.reversed()) { version in
                        let key = version.file(in: group.file).backupKey
                        Toggle(isOn: Binding(get: { selection.contains(key) }, set: { value in
                            if value { selection.insert(key) } else { selection.remove(key) }
                        })) {
                            VStack(alignment: .leading) {
                                Text(fileDate(version.createdAt), format: .dateTime)
                                Text(fileSize(version.size) + " · " + String(localized: version.current ? "Current version" : "Historical version")).font(.caption)
                                if version.status == "backing_up" { Text("Backing up…").font(.caption) }
                            }
                        }.accessibilityIdentifier("backup.choose." + key)
                    }
                } header: { Text(((group.file.path ?? []) + [group.file.name]).joined(separator: " / ")).textCase(nil) }
            }
        }.toggleStyle(SelectionRowToggleStyle()).privateScreen().navigationTitle("Without a cloud backup")
            .safeAreaInset(edge: .bottom) {
                if !groups.isEmpty {
                    Button {
                        review = Review(files: selected)
                    } label: {
                        VStack {
                            Text("Back up to cloud")
                            if !selection.isEmpty { Text(verbatim: "\(selection.count.formatted()) · \(fileSize(selected.reduce(0) { $0 + $1.size }))").font(.caption) }
                        }.frame(maxWidth: .infinity)
                    }.buttonStyle(.borderedProminent).disabled(selection.isEmpty || loading).padding().background(.bar)
                        .accessibilityIdentifier("backup.review-selected")
                }
            }
            .task { await load() }.refreshable { await load() }
            .sheet(item: $review, onDismiss: { Task { await load() } }) { review in
                NavigationStack { FileBackupView(files: review.files, explicitRevisions: true) }
            }
    }

    private func load() async {
        let epoch = model.epoch
        loading = true; failure = nil
        defer { if epoch == model.epoch { loading = false } }
        do {
            let next = try await model.unbackedFileVersions()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            groups = next; selection.formIntersection(versions.map(\.backupKey))
        } catch { if epoch == model.epoch && !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }
}
