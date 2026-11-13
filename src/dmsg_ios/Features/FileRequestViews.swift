import SwiftUI

/// Links that take files from people with no account: the saved list first, then the relay's counts.
struct FileRequestsView: View {
    @Environment(SessionModel.self) private var model
    @State private var links: [FileRequest] = []
    @State private var capacity: FileBackupCapacity?
    @State private var loading = true
    @State private var failure: String?
    @State private var composing = false
    private var allowed: UInt64? { capacity?.fileRequestLimit }

    var body: some View {
        List {
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            if allowed == 0 {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Collecting files needs a paid plan").font(.headline)
                        Text("Plus and above can ask anyone for files by link. People need no account, see only what they sent, and only you can open the files.")
                            .font(.footnote).foregroundStyle(.secondary)
                    }.padding(.vertical, 4).accessibilityIdentifier("file.request.upgrade")
                    NavigationLink("See plans") { SubscriptionView() }.accessibilityIdentifier("file.request.plans")
                }
            } else if let allowed {
                Section {
                    LabeledContent("Active links", value: "\(links.filter(\.active).count.formatted()) / \(allowed.formatted())")
                        .accessibilityIdentifier("file.request.usage")
                    if let largest = capacity?.backupMaxFileBytes { LabeledContent("Largest file", value: fileLimit(largest)) }
                }
            }
            ForEach(links) { link in
                NavigationLink { FileRequestDetailView(link: link) { changed in replace(changed) } } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(link.title ?? String(link.id.prefix(12)) + "…").lineLimit(2)
                        if let label = link.label { Text(label).font(.footnote).foregroundStyle(.secondary).lineLimit(1) }
                        (Text(link.stateTitle) + Text(verbatim: " · ") + Text("\(Int(clamping: link.files)) files · \(fileSize(link.bytes))"))
                            .font(.caption).foregroundStyle(link.active ? Color.secondary : Color.orange)
                        if link.unread { Text("New files").font(.caption.weight(.medium)).foregroundStyle(Theme.forest).accessibilityIdentifier("file.request.unread") }
                    }.padding(.vertical, 2)
                }.accessibilityIdentifier("file.request.item.\(link.id)")
            }
            if loading && links.isEmpty { ProgressView("Loading…") }
            else if !loading && failure == nil && links.isEmpty && allowed != 0 {
                ContentUnavailableView("Collect files", systemImage: "tray.and.arrow.down",
                    description: Text("No links yet. Make one and send it to whoever should send you files."))
            }
        }.privateScreen().navigationTitle("Collect files").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("New link", systemImage: "plus") { composing = true }
                        .disabled(allowed == 0).accessibilityIdentifier("file.request.new")
                }
            }
            .sheet(isPresented: $composing, onDismiss: { Task { await load() } }) {
                NavigationStack { FileRequestComposer(capacity: capacity) }
            }
            .task { await load() }.refreshable { await load() }
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        if let saved = try? await model.fileRequests(refresh: false), stamp == model.epoch { links = saved }
        async let usage = try? model.fileRequestCapacity()
        do {
            let fresh = try await model.fileRequests(refresh: true)
            guard stamp == model.epoch, !Task.isCancelled else { return }
            links = fresh
        } catch {
            guard stamp == model.epoch, !Task.isCancelled else { return }
            failure = model.fileFailureMessage(error)
        }
        if let value = await usage, stamp == model.epoch { capacity = value }
    }

    private func replace(_ link: FileRequest) {
        if let index = links.firstIndex(where: { $0.id == link.id }) { links[index] = link }
    }
}

/// A link, or one for each name; a failure keeps the names still to do.
struct FileRequestComposer: View {
    private static let maxNames = 50
    private static let mib: UInt64 = 1024 * 1024
    let capacity: FileBackupCapacity?
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var instructions = ""
    @State private var names = ""
    @State private var days = 14
    @State private var total: UInt64 = 0
    @State private var working = false
    @State private var progress = (done: 0, total: 0)
    @State private var failure: String?
    @State private var created: [FileRequest] = []
    @State private var work: Task<Void, Never>?
    @FocusState private var editing: Bool

    private var labels: [String] { names.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty } }
    private var tooMany: Bool { labels.count > Self.maxNames }
    private var totals: [UInt64] {
        let room = capacity?.storageLimit ?? .max
        let fits = ([100, 500, 1024, 2048, 5120, 10240] as [UInt64]).filter { $0 * Self.mib <= room }
        return fits.isEmpty ? [1] : fits
    }
    private var preferred: UInt64 {
        let wanted: UInt64 = (capacity?.backupMaxFileBytes ?? 0) > 100 * Self.mib ? 2048 : 500
        return totals.contains(wanted) ? wanted : totals.last ?? 1
    }
    private var finished: Bool { !created.isEmpty && failure == nil }

    var body: some View {
        Form {
            if !created.isEmpty { made }
            if !finished { terms }
        }.privateScreen().navigationTitle("New link").navigationBarTitleDisplayMode(.inline)
            // Return adds a line in these fields, so scrolling is what puts the keyboard away.
            .scrollDismissesKeyboard(.immediately)
            #if DEBUG
            .onAppear {
                guard model.storePreview, title.isEmpty else { return }
                let draft = model.storePreviewRequestDraft()
                title = draft.title; instructions = draft.instructions; names = draft.names
            }
            #endif
            .toolbar {
                if finished {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.accessibilityIdentifier("file.request.done") }
                } else {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { work?.cancel(); dismiss() } }
                }
            }
            .interactiveDismissDisabled(working)
            .onAppear { if total == 0 { total = preferred } }
            .onDisappear { work?.cancel() }
    }

    @ViewBuilder private var made: some View {
        Section {
            if finished { Text("Links ready. Send each one only to its person.").font(.footnote).padding(.vertical, 4).accessibilityIdentifier("file.request.ready") }
            ForEach(created) { link in
                VStack(alignment: .leading, spacing: 8) {
                    Text(link.label ?? link.title ?? "").lineLimit(2)
                    if let url = link.url {
                        HStack(spacing: 16) {
                            ShareLink(item: url) { Label("Share link", systemImage: "square.and.arrow.up") }.buttonStyle(.borderless)
                            Button("Copy link", systemImage: "doc.on.doc") { copy(url) }.buttonStyle(.borderless)
                        }.font(.footnote).labelStyle(.titleOnly)
                    }
                }.padding(.vertical, 2).accessibilityElement(children: .contain).accessibilityIdentifier("file.request.created")
            }
            if created.count > 1 {
                Button("Copy all", systemImage: "doc.on.doc") {
                    copy(created.compactMap { link in link.url.map { url in link.label.map { "\($0),\(url)" } ?? url } }.joined(separator: "\n"))
                }.accessibilityIdentifier("file.request.copy.all")
            }
        }
    }

    @ViewBuilder private var terms: some View {
        Section {
            TextField("What are you asking for?", text: $title, axis: .vertical).lineLimit(1...3)
                .focused($editing).accessibilityIdentifier("file.request.title")
            VStack(alignment: .leading, spacing: 4) {
                TextField("Instructions (optional)", text: $instructions, axis: .vertical).lineLimit(2...6)
                    .focused($editing).accessibilityIdentifier("file.request.instructions")
                Text("Whoever has the link sees these, as plain text.").font(.caption).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 4) {
                TextField("Who is it for? (optional)", text: $names, axis: .vertical).lineLimit(2...8)
                    .textInputAutocapitalization(.words).focused($editing).accessibilityIdentifier("file.request.names")
                Text("One name per line makes a separate link for each. Only you see the names.").font(.caption).foregroundStyle(.secondary)
                if tooMany { Text("At most \(Self.maxNames) names at a time.").font(.caption).foregroundStyle(.red).accessibilityIdentifier("file.request.toomany") }
            }
        }.disabled(working)
        Section {
            Picker("Open for", selection: $days) {
                ForEach([7, 14, 30, 90], id: \.self) { value in Text("\(value) days").tag(value) }
            }
            Picker("Total size", selection: $total) {
                ForEach(totals, id: \.self) { value in Text(fileLimit(value * Self.mib)).tag(value) }
            }
        }.disabled(working)
        if working { ProgressView("Creating \(progress.done) of \(progress.total)…").accessibilityIdentifier("file.request.working") }
        if let failure {
            Section {
                if !created.isEmpty { Text("Some links were not made. The names left are below to try again.").font(.footnote) }
                Text(failure).font(.footnote).foregroundStyle(.red).accessibilityIdentifier("file.request.failure")
            }
        }
        SettingsAction(labels.count > 1 ? "Create \(labels.count) links" : "Create link", systemImage: "link", prominent: true) { submit() }
            .disabled(working || tooMany || title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .accessibilityIdentifier("file.request.create")
    }

    private func copy(_ text: String) { model.clipboard.copy(text); model.notice = String(localized: "Copied") }

    private func submit() {
        guard !working else { return }
        let stamp = model.epoch, pending = labels.isEmpty ? [""] : labels
        let heading = title.trimmingCharacters(in: .whitespacesAndNewlines), notes = instructions.trimmingCharacters(in: .whitespacesAndNewlines)
        let expires = UInt64(days) * 86_400_000, bytes = total * Self.mib
        editing = false
        working = true; failure = nil; progress = (0, pending.count); model.activity()
        work = Task {
            defer { working = false }
            var remaining = pending
            do {
                while let next = remaining.first {
                    let link = try await model.createFileRequest(title: heading, instructions: notes, label: next, expiresInMs: expires, maxTotalBytes: bytes)
                    guard stamp == model.epoch, !Task.isCancelled else { return }
                    created.append(link); remaining.removeFirst(); progress.done += 1
                }
                names = ""
            } catch {
                guard stamp == model.epoch, !Task.isCancelled else { return }
                failure = model.fileFailureMessage(error); names = remaining.joined(separator: "\n")
            }
        }
    }
}

/// What one link received. A file comes down only when asked for, one at a time.
struct FileRequestDetailView: View {
    let link: FileRequest
    let changed: (FileRequest) -> Void
    @Environment(SessionModel.self) private var model
    @State private var current: FileRequest?
    @State private var files: [ReceivedFile] = []
    @State private var loading = true
    @State private var saving: String?
    @State private var working = false
    @State private var failure: String?
    @State private var closing = false
    @State private var deleting: ReceivedFile?
    private var shown: FileRequest { current ?? link }

    var body: some View {
        List {
            Section("Details") {
                if let title = shown.title { Text(title).font(.headline).textSelection(.enabled) }
                if let label = shown.label { Text(label).foregroundStyle(.secondary) }
                LabeledContent("Status", value: shown.stateTitle).accessibilityIdentifier("file.request.state")
                LabeledContent("Expires") { Text(fileDate(shown.expiresAt), format: .dateTime) }
                if let instructions = shown.instructions, !instructions.isEmpty { Text(instructions).font(.footnote).foregroundStyle(.secondary) }
            }
            Section {
                if shown.active {
                    if let url = shown.url {
                        ShareLink(item: url) { Label("Share link", systemImage: "square.and.arrow.up") }.accessibilityIdentifier("file.request.share")
                        Button("Copy link", systemImage: "doc.on.doc") { model.clipboard.copy(url); model.notice = String(localized: "Copied") }
                            .accessibilityIdentifier("file.request.copy")
                    } else {
                        Text("This device does not have this link’s key yet.").font(.footnote).foregroundStyle(.secondary)
                    }
                    SettingsAction("Close link", systemImage: "xmark.circle", role: .destructive) { closing = true }
                        .disabled(working).accessibilityIdentifier("file.request.close")
                }
                if working && saving == nil { ProgressView("Loading…") }
                if let failure { Text(failure).font(.footnote).foregroundStyle(.red).accessibilityIdentifier("files.action.failure") }
            }
            Section("Files") {
                ForEach(files) { file in row(file) }
                if loading && files.isEmpty { ProgressView("Loading…") }
                else if !loading && files.isEmpty { Text("Nothing has arrived yet.").font(.footnote).foregroundStyle(.secondary).accessibilityIdentifier("file.request.empty") }
            }
        }.privateScreen().navigationTitle("Collect files").navigationBarTitleDisplayMode(.inline)
            .task { await load() }.refreshable { await load() }
            // The save runs in the foreground, so it must outlast the idle lock.
            .task(id: saving) {
                guard saving != nil else { return }
                for _ in 0..<240 {
                    model.activity()
                    do { try await Task.sleep(for: .seconds(30)) } catch { return }
                }
            }
            .confirmationDialog("Close link", isPresented: $closing, titleVisibility: .visible) {
                Button("Close link", role: .destructive) { Task { await close() } }.accessibilityIdentifier("file.request.close.confirm")
            } message: { Text("Nobody can send more files through it. Files already received stay.") }
    }

    @ViewBuilder private func row(_ file: ReceivedFile) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            FileSummaryRow(name: file.name ?? String(file.id.prefix(12)) + "…", mime: file.mime ?? "", subtitle: fileSize(file.size ?? file.bytes), expanded: true)
            if let sender = file.sender, !sender.name.isEmpty {
                (Text("From (not verified):") + Text(verbatim: " \(sender.name)")).font(.footnote).foregroundStyle(.secondary).accessibilityIdentifier("file.request.sender")
            }
            if let note = file.sender?.note, !note.isEmpty { Text(note).font(.footnote).foregroundStyle(.secondary) }
            if let old = file.replaces {
                Text("Replaces \(files.first { $0.id == old }?.name ?? String(old.prefix(8)))").font(.caption).foregroundStyle(.secondary)
            }
            switch file.state {
            case "unreadable": Text("This file cannot be opened. You can delete it.").font(.footnote).foregroundStyle(.orange).accessibilityIdentifier("file.request.unreadable")
            case "withdrawn": Text("Withdrawn by the sender").font(.footnote).foregroundStyle(.secondary)
            case "saved": Text("Saved to your library").font(.footnote).foregroundStyle(.green)
            default: EmptyView()
            }
            if saving == file.id {
                ProgressView("Saving…").accessibilityIdentifier("file.request.saving")
                Text("Keep dMsg open until this finishes.").font(.caption).foregroundStyle(.secondary)
            }
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 16) { actions(file) }
                VStack(alignment: .leading, spacing: 8) { actions(file) }
            }.font(.footnote).buttonStyle(.borderless).labelStyle(.titleOnly)
                .confirmationDialog("Delete cloud copy", isPresented: Binding(get: { deleting?.id == file.id }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
                    Button("Delete cloud copy", role: .destructive) { Task { await remove(file) } }.accessibilityIdentifier("file.request.delete.confirm")
                } message: { Text("This frees its storage. A copy saved to your library stays.") }
        }.padding(.vertical, 2).accessibilityElement(children: .contain).accessibilityIdentifier("file.request.file.\(file.id)")
    }

    @ViewBuilder private func actions(_ file: ReceivedFile) -> some View {
        if file.state == "received" {
            Button("Save to library", systemImage: "square.and.arrow.down") { Task { await save(file) } }
                .disabled(saving != nil || working).accessibilityIdentifier("file.request.save")
        }
        if let saved = file.savedFile {
            NavigationLink { FileDetailDestination(id: saved) } label: { Text("Show in library") }
                .fixedSize().accessibilityIdentifier("file.request.show")
        }
        if file.state != "withdrawn" {
            Button("Delete cloud copy", systemImage: "trash", role: .destructive) { deleting = file }
                .disabled(saving != nil || working).accessibilityIdentifier("file.request.delete")
        }
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        do {
            let value = try await model.fileRequestFiles(link.id)
            guard stamp == model.epoch, !Task.isCancelled else { return }
            files = value
            if shown.unread { var seen = shown; seen.unread = false; current = seen; changed(seen) }
        } catch { if stamp == model.epoch, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }

    private func save(_ file: ReceivedFile) async {
        guard saving == nil, !working else { return }; let stamp = model.epoch
        saving = file.id; failure = nil; defer { saving = nil }
        do {
            try await model.saveRequestFile(file.id)
            guard stamp == model.epoch else { return }
            await load(); await model.loadFiles()
        } catch { if stamp == model.epoch { failure = model.fileFailureMessage(error) } }
    }

    private func remove(_ file: ReceivedFile) async {
        guard !working else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        do {
            try await model.deleteRequestFile(file.id)
            guard stamp == model.epoch else { return }
            files.removeAll { $0.id == file.id }
        } catch { if stamp == model.epoch { failure = model.fileFailureMessage(error) } }
    }

    private func close() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        do {
            let value = try await model.revokeFileRequest(link.id)
            guard stamp == model.epoch else { return }
            current = value; changed(value)
        } catch { if stamp == model.epoch { failure = model.fileFailureMessage(error) } }
    }
}
