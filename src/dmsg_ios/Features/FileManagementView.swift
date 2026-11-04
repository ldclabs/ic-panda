import SwiftUI

struct FileManagementView: View {
    let file: LibraryFile
    var body: some View {
        List {
            NavigationLink("Rename") { FileNameEditor(target: LibraryItemTarget(id: file.id, name: file.name, folder: false), parent: file.folder ?? "root") }
            NavigationLink("Move to another folder") { FileMoveView(items: [LibraryItemTarget(id: file.id, name: file.name, folder: false)]) }
        }.privateScreen().navigationTitle("Manage file")
    }
}

struct FileNameEditor: View {
    let target: LibraryItemTarget?
    let parent: String
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var working = false
    @State private var failure: String?
    @State private var visible = true
    @State private var initialized = false
    @State private var generation: UInt64 = 0
    private var draftKey: String { "file.name:" + (target?.id ?? "new:" + parent) }
    private var clean: String { name.trimmingCharacters(in: .whitespacesAndNewlines) }

    static func valid(_ name: String) -> Bool {
        let value = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return !value.isEmpty && value.unicodeScalars.count <= 255 && value != "." && value != ".."
            && !value.contains("/") && !value.contains("\\") && value.rangeOfCharacter(from: .controlCharacters) == nil
    }

    var body: some View {
        Form {
            SettingsField(target?.folder == false ? "File name" : "Folder name", invalid: !name.isEmpty && !Self.valid(name)) {
                TextField("Name", text: $name).textInputAutocapitalization(.never).autocorrectionDisabled()
                    .submitLabel(.done).onSubmit { if Self.valid(name) && !working { Task { await save() } } }.disabled(working)
                    .accessibilityIdentifier("files.name")
            }
            if !name.isEmpty && !Self.valid(name) { Text("Use a name of 1–255 characters without slashes or control characters.").font(.footnote).foregroundStyle(.red) }
            if let failure { Text(failure).font(.footnote).foregroundStyle(.red).accessibilityIdentifier("files.action.failure") }
            if working { ProgressView("Saving…") }
            SettingsAction(target == nil ? "Create" : "Save name", prominent: true) { Task { await save() } }
                .disabled(working || !Self.valid(name) || clean == target?.name).accessibilityIdentifier("files.name.save")
        }.privateScreen().navigationTitle(target == nil ? "New folder" : "Rename")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(working) } }
            .onAppear {
                visible = true
                if !initialized { generation = model.viewGeneration; name = model.formDraft(draftKey, as: String.self) ?? target?.name ?? ""; initialized = true }
            }
            .onDisappear { visible = false }
            .onChange(of: name) { _, value in failure = nil; model.keepFormDraft(draftKey, value: value, generation: generation) }
            .interactiveDismissDisabled(working)
    }

    private func save() async {
        guard !working, Self.valid(name) else { return }; let stamp = model.epoch, value = clean
        working = true; failure = nil; defer { working = false }
        do {
            if let target { try await model.renameLibraryItem(target.id, name: value) }
            else {
                #if DEBUG
                if model.uiTesting { try model.createFileFolderFixture(parent: parent, name: value) }
                else { try await create(value) }
                #else
                try await create(value)
                #endif
            }
            guard stamp == model.epoch, visible, !Task.isCancelled else { return }
            model.settingsDrafts.removeValue(forKey: draftKey)
            await model.loadFiles(); dismiss()
        } catch { if stamp == model.epoch, visible, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func create(_ name: String) async throws {
        guard let engine = model.engine else { throw CancellationError() }
        _ = try await engine.createFolder(parent: parent, name: name)
    }
}

struct FileMoveView: View {
    let items: [LibraryItemTarget]
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var folder = "root"
    @State private var folderName = String(localized: "Files")
    @State private var choosing = false
    @State private var working = false
    @State private var completed: Set<String> = []
    @State private var outcomes: [String: FileActionResult] = [:]
    @State private var moveTask: Task<Void, Never>?
    @State private var destinationChosen = false
    var body: some View {
        List {
            Section {
                if destinationChosen { LabeledContent("Destination folder", value: folderName) }
                Button("Choose folder") { choosing = true }.disabled(working)
            }
            ForEach(items) { item in
                VStack(alignment: .leading, spacing: 6) {
                    Text(item.name)
                    if let result = outcomes[item.id] { Label(result.message, systemImage: result.failed ? "exclamationmark.circle" : "checkmark.circle").font(.footnote) }
                }
            }
            if working { ProgressView("Moving…") }
            if outcomes.values.contains(where: \.failed) {
                SettingsAction("Retry", prominent: true) { startMove() }.disabled(working)
                    .accessibilityIdentifier("files.move.retry")
            }
        }.privateScreen().navigationTitle("Move to another folder")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button(outcomes.isEmpty ? "Cancel" : "Done") { dismiss() }.disabled(working) } }
            .sheet(isPresented: $choosing) { NavigationStack {
                FileFolderPicker(excluding: Set(items.filter(\.folder).map(\.id)), confirmationTitle: "Move here") { id, name in
                    if folder != id { completed = []; outcomes = [:] }
                    folder = id; folderName = name; destinationChosen = true
                    startMove()
                }
            } }
            // Moving starts with the picker confirmation, while its sheet closes.
            // Cancel actual work on departure instead of gating it on visibility.
            .onDisappear { moveTask?.cancel(); moveTask = nil }
            .interactiveDismissDisabled(working)
    }

    private func startMove() {
        guard !working, destinationChosen else { return }
        working = true
        moveTask = Task { await move() }
    }

    private func move() async {
        let stamp = model.epoch, destination = folder
        defer { working = false }
        for item in items where !completed.contains(item.id) {
            guard stamp == model.epoch, !Task.isCancelled else { return }
            do {
                try await model.moveLibraryItem(item.id, folder: destination)
                guard stamp == model.epoch, !Task.isCancelled else { return }
                completed.insert(item.id); outcomes[item.id] = FileActionResult(name: item.name, message: String(localized: "Moved"))
            } catch { if stamp == model.epoch, !Task.isCancelled { guard let message = model.fileFailureMessage(error) else { return }; outcomes[item.id] = FileActionResult(name: item.name, message: message, failed: true) } }
        }
        if stamp == model.epoch, !Task.isCancelled { await model.loadFiles() }
    }
}

struct FileFolderPicker: View {
    var excluding: Set<String> = []
    var confirmationTitle: LocalizedStringKey = "Use this folder"
    let selected: (String, String) -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var folder = "root"
    @State private var listing: LibraryListing?
    @State private var loading = true
    @State private var failure: String?
    var body: some View {
        List {
            if loading { ProgressView("Loading…") }
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            if let listing, !loading {
                if let parent = listing.folder { Button("Parent folder", systemImage: "arrow.up") { folder = parent.parent ?? "root" } }
                ForEach(listing.folders) { child in
                    Button { folder = child.id } label: { FileSummaryRow(name: child.name, mime: "inode/directory", subtitle: "") }
                        .disabled(excluding.contains(child.id)).compactObjectRow().accessibilityIdentifier("files.folder.\(child.id)")
                }
                SettingsAction(confirmationTitle, prominent: true) { selected(folder, listing.folder?.name ?? String(localized: "Files")); dismiss() }
                    .disabled(excluding.contains(folder)).accessibilityIdentifier("files.folder.choose")
            }
        }.privateScreen().navigationTitle(listing?.folder?.name ?? String(localized: "Files"))
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task(id: folder) { await load() }
    }

    private func load() async {
        let stamp = model.epoch, target = folder; loading = true; failure = nil; listing = nil
        defer { if folder == target { loading = false } }
        do {
            let value: LibraryListing
            #if DEBUG
            if model.uiTesting { value = try await model.fileListingFixture(folder: target, query: "") }
            else { value = try await read(target) }
            #else
            value = try await read(target)
            #endif
            guard stamp == model.epoch, target == folder, !Task.isCancelled else { return }; listing = value
        } catch { if stamp == model.epoch, target == folder, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func read(_ folder: String) async throws -> LibraryListing {
        guard let engine = model.engine else { throw CancellationError() }
        return try model.decode(try await engine.files(folder: folder))
    }
}

struct FileVersionsView: View {
    let file: LibraryFile
    @Environment(SessionModel.self) private var model
    @State private var details: LibraryDetails?
    @State private var loading = true
    @State private var failure: String?
    var body: some View {
        List {
            if loading { ProgressView("Loading…") }
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            ForEach((details?.versions ?? []).reversed()) { version in
                NavigationLink { FileDetailDestination(id: file.id, version: version.id) } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(fileDate(version.createdAt), format: .dateTime).font(.body.weight(.medium))
                        Text(fileSize(version.size) + " · " + String(localized: version.local ? "On this device" : "Only in cloud")).font(.caption).foregroundStyle(.secondary)
                        if version.current { Text("Current version").font(.caption).foregroundStyle(Theme.forest) }
                        if !version.local { Label("Restore to this device", systemImage: "icloud.and.arrow.down").font(.footnote) }
                    }
                }.compactObjectRow().accessibilityIdentifier("file.version.\(version.id)")
            }
            if !loading && failure == nil && details?.versions.isEmpty == true { ContentUnavailableView("No file versions", systemImage: "doc") }
        }.privateScreen().navigationTitle("File versions").task(id: model.filesRevision) { await load() }.refreshable { await load() }
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        do {
            let value = try await model.fileDetails(file.id)
            guard stamp == model.epoch, !Task.isCancelled else { return }; details = value
        } catch { if stamp == model.epoch, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }
}
