import SwiftUI
import PhotosUI
import UniformTypeIdentifiers
@preconcurrency import DMsgCore

struct FeedbackView: View {
    @Environment(SessionModel.self) private var model
    @State private var photos: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var choosingFile = false
    @State private var importing = false
    @State private var discarding = false
    @State private var importFailure: String?
    @State private var active = false
    @FocusState private var writing: Bool
    private var form: FeedbackWorkspace { model.feedback }
    private var frozen: Bool { form.busy || importing || !form.loaded || !form.editable }

    var body: some View {
        Form {
            Section {
                Text("Send a private, free letter to dMsg support. Review your description, screenshots and app information before sending.").font(.footnote).foregroundStyle(.secondary)
                Text(FeedbackWorkspace.recipient).font(.caption.monospaced()).textSelection(.enabled).environment(\.layoutDirection, .leftToRight)
            }
            if !form.loaded {
                SettingsAction("Retry", working: form.busy) { Task { await form.load(model) } }
            } else {
                Section {
                    SettingsField("Message", counter: "\(form.text.utf8.count) / 4500", invalid: form.text.utf8.count > 4500) {
                        TextField("Message", text: Binding(get: { form.text }, set: { form.update($0, model: model) }), axis: .vertical)
                            .lineLimit(5...12).focused($writing).disabled(frozen).accessibilityIdentifier("feedback.text")
                    }
                    if form.text.utf8.count > 4500 { Text("Use at most 4500 UTF-8 bytes for the description.").font(.footnote).foregroundStyle(.red) }
                }
                Section {
                    ForEach(form.screenshots) { screenshot in
                        VStack(alignment: .leading, spacing: 8) {
                            LibraryThumbnail(file: screenshot.libraryFile, contentMode: .fit).frame(height: 130).clipped()
                            Text(screenshot.name).font(.caption).lineLimit(3)
                            if !frozen {
                                Button("Remove", role: .destructive) { form.update(form.text, screenshots: form.screenshots.filter { $0.id != screenshot.id }, model: model) }
                                    .accessibilityIdentifier("feedback.remove")
                            }
                        }.accessibilityIdentifier("feedback.screenshot")
                    }
                    if importing { ProgressView() }
                    SettingsAction("Choose screenshots", systemImage: "photo.on.rectangle") { writing = false; choosingPhotos = true }
                        .disabled(frozen || form.screenshots.count >= 3).accessibilityIdentifier("feedback.photos")
                    SettingsAction("Import file", systemImage: "folder") { writing = false; choosingFile = true }
                        .disabled(frozen || form.screenshots.count >= 3).accessibilityIdentifier("feedback.file")
                    #if DEBUG
                    if model.uiTesting {
                        Button {
                            writing = false
                            let file = FeedbackScreenshot(file: NativeApple.requestID(), version: NativeApple.requestID(), name: "Screenshot.png", size: 100, mime: "image/png")
                            form.update(form.text, screenshots: form.screenshots + [file], model: model)
                        } label: { Text(verbatim: "Add test screenshot") }
                        .disabled(frozen || form.screenshots.count >= 3).accessibilityIdentifier("feedback.fixture-image")
                    }
                    #endif
                } header: { Text("Screenshots") } footer: {
                    Text("Up to 3 PNG, JPEG, WebP or GIF images, 10 MiB each. Remove secrets first. Copies stay in Files; sending creates encrypted links valid for 30 days and uses your storage quota. Anyone with a complete link can open it.")
                }
                Section {
                    Text(form.context).font(.footnote).foregroundStyle(.secondary)
                    if form.saving || form.dirty { Text("Saving encrypted draft…").font(.footnote) }
                    else if form.record?.state == "sent" { Label("Feedback delivered. This does not mean it has been read.", systemImage: "checkmark.circle").accessibilityIdentifier("feedback.sent") }
                    else if form.record?.state == "pending" { Text("Delivery is unconfirmed. Retry keeps the original report.").font(.footnote) }
                    else if form.record != nil { Text("Draft saved encrypted on this device.").font(.footnote).foregroundStyle(.secondary) }
                    if let failure = importFailure ?? form.failure ?? form.record?.error.map({ letterSendFailure(MobileError.Failure(code: $0)) }) {
                        NoticeBar(text: failure).accessibilityIdentifier("feedback.failure")
                    }
                    if form.dirty, form.failure != nil { SettingsAction("Retry") { Task { await form.flush(model) } } }
                    if form.record?.state == "sent" {
                        SettingsAction("New feedback", prominent: true) { Task { await form.clear(model) } }.accessibilityIdentifier("feedback.new")
                    } else {
                        SettingsAction(form.record?.state == "pending" ? "Retry" : "Send", prominent: true, working: form.busy) { writing = false; form.send(model) }
                            .disabled(importing || !form.valid).accessibilityIdentifier("feedback.send")
                        if form.record?.state == "pending", form.record?.editable == true {
                            SettingsAction("Edit draft") { Task { await form.edit(model) } }.disabled(form.busy)
                        }
                        if form.editable { SettingsAction("Discard", role: .destructive) { discarding = true }.disabled(form.busy || importing) }
                    }
                }
            }
        }.privateScreen().scrollDismissesKeyboard(.interactively).navigationTitle("Report a problem").navigationBarTitleDisplayMode(.inline)
            .onAppear { active = true }
            .onDisappear { active = false }
            .toolbar { ToolbarItemGroup(placement: .keyboard) { Spacer(); Button("Done") { writing = false }.accessibilityIdentifier("feedback.keyboard-done") } }
            .task(id: model.epoch) { await form.load(model) }
            .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: max(1, 3 - form.screenshots.count), matching: .images, preferredItemEncoding: .compatible)
            .onChange(of: photos) { _, values in if !values.isEmpty { Task { await importPhotos(values) } } }
            .fileImporter(isPresented: $choosingFile, allowedContentTypes: [.png, .jpeg, .webP, .gif]) { result in
                if case .success(let url) = result { Task { await importFiles([url]) } }
                else if case .failure(let error) = result { importFailure = InboxLoading.message(error) }
            }
            .confirmationDialog("Discard feedback draft?", isPresented: $discarding, titleVisibility: .visible) {
                Button("Discard", role: .destructive) { Task { await form.clear(model) } }
            } message: { Text("Only the draft is removed. Screenshot copies and existing share links remain in Files.") }
    }

    private func append(_ url: URL, epoch: UInt64) async throws {
        guard active, epoch == model.epoch, !Task.isCancelled else { throw CancellationError() }
        let file = try await model.importLibraryDocument(url, folder: "root")
        guard active, epoch == model.epoch, !Task.isCancelled else { throw CancellationError() }
        guard form.screenshots.count < 3, file.size <= 10 * 1024 * 1024, ["image/png", "image/jpeg", "image/webp", "image/gif"].contains(file.mime) else { throw MobileError.Failure(code: "INVALID_INPUT") }
        if !form.screenshots.contains(where: { $0.file == file.id }) {
            form.update(form.text, screenshots: form.screenshots + [FeedbackScreenshot(file: file.id, version: file.current, name: file.name, size: file.size, mime: file.mime)], model: model)
        }
    }

    private func importPhotos(_ values: [PhotosPickerItem]) async {
        guard !frozen else { return }
        let epoch = model.epoch; importing = true; importFailure = nil
        defer { if epoch == model.epoch { importing = false; photos = [] } }
        do {
            for value in values {
                guard epoch == model.epoch, !Task.isCancelled else { return }
                guard let copy = try await value.loadTransferable(type: PhotoImport.self) else { throw CocoaError(.fileReadUnknown) }
                defer { copy.finish() }
                guard epoch == model.epoch else { return }
                try await append(copy.file, epoch: epoch)
            }
        } catch { if epoch == model.epoch, !(error is CancellationError) { importFailure = InboxLoading.message(error) } }
    }

    private func importFiles(_ urls: [URL]) async {
        guard !frozen else { return }
        let epoch = model.epoch; importing = true; importFailure = nil
        defer { if epoch == model.epoch { importing = false } }
        do { for url in urls { try await append(url, epoch: epoch) } }
        catch { if epoch == model.epoch, !(error is CancellationError) { importFailure = InboxLoading.message(error) } }
    }
}
