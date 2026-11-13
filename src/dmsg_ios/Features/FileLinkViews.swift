import SwiftUI

/// Sets the terms of a new link, then hands the finished link over.
struct FileLinkComposer: View {
    let target: FileLinkTarget
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var duration: UInt64 = 604_800_000
    @State private var maxDownloads: UInt64 = 0
    @State private var password = ""
    @State private var showProfile = false
    @State private var working = false
    @State private var failure: String?
    @State private var link: FileLink?
    @State private var work: Task<Void, Never>?

    var body: some View {
        Form {
            FileSummaryRow(name: target.name, mime: target.mime, subtitle: fileSize(target.size), expanded: true).listRowBackground(Color.clear)
            if let link, let url = link.url { ready(link, url: url) } else { terms }
        }.privateScreen().navigationTitle("Share by link").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if link != nil {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                } else {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { work?.cancel(); dismiss() } }
                }
            }
            .interactiveDismissDisabled(working)
            .onChange(of: model.concealed) { _, concealed in if concealed { password = "" } }
            .onDisappear { work?.cancel(); password = "" }
    }

    @ViewBuilder private var terms: some View {
        Section {
            Group {
                Text("Anyone with the link can download this file until it expires or you revoke it. They need no dMsg account or app.")
                if target.remote {
                    Text("This file is not on this device. dMsg downloads it only long enough to encrypt the new copy, and your library does not change.")
                        .accessibilityIdentifier("file.link.remote")
                }
                Text("The link uses a new encrypted copy that counts toward your cloud storage until it is revoked or expires.").foregroundStyle(.secondary)
            }.padding(.vertical, 4)
        }.font(.footnote)
        Section("Link limits") {
            Picker("Expires after", selection: $duration) {
                Text("1 day").tag(UInt64(86_400_000)); Text("7 days").tag(UInt64(604_800_000)); Text("30 days").tag(UInt64(2_592_000_000))
            }
            Picker("Downloads allowed", selection: $maxDownloads) {
                Text("Unlimited").tag(UInt64(0))
                ForEach([1, 5, 10, 50, 100] as [UInt64], id: \.self) { value in Text(value.formatted()).tag(value) }
            }
            SecureField("Optional password", text: $password).textInputAutocapitalization(.never).autocorrectionDisabled()
                .accessibilityIdentifier("file.link.password")
            Toggle("Show my public profile", isOn: $showProfile).accessibilityIdentifier("file.link.profile")
            Text("Anyone with this link can see your public profile, even before entering a password.").font(.footnote).foregroundStyle(.secondary)
        }.disabled(working)
        if working { ProgressView("Encrypting and uploading a new copy…").accessibilityIdentifier("file.link.working") }
        if let failure { FileInlineFailure(message: failure, working: working) { submit() } }
        SettingsAction("Create link", systemImage: "link", prominent: true) { submit() }
            .disabled(working || target.tooLarge).accessibilityIdentifier("file.link.create")
    }

    @ViewBuilder private func ready(_ link: FileLink, url: String) -> some View {
        Section {
            Text(link.password ? "The link opens only with its password. Send the password another way." : "The link is ready. It contains the key to this file, so send it over a private channel.")
                .font(.footnote).padding(.vertical, 4).accessibilityIdentifier("file.link.ready")
            ShareLink(item: url) { Label("Share link", systemImage: "square.and.arrow.up") }.accessibilityIdentifier("file.link.share")
            Button("Copy link", systemImage: "doc.on.doc") { model.clipboard.copy(url); model.notice = String(localized: "Copied") }
                .accessibilityIdentifier("file.link.copy")
        }
    }

    private func submit() {
        guard !working else { return }
        let stamp = model.epoch, secret = password.isEmpty ? nil : password
        working = true; failure = nil; model.activity()
        work = Task {
            defer { working = false }
            do {
                let made = try await model.createFileLink(target, expiresInMs: duration, maxDownloads: maxDownloads, password: secret, showProfile: showProfile)
                guard stamp == model.epoch, !Task.isCancelled else { return }
                link = made; password = ""
            } catch {
                if stamp == model.epoch, !Task.isCancelled { failure = model.fileFailureMessage(error) }
            }
        }
    }
}

/// The account's file links, the saved list first and then the relay's counts.
struct FileLinksView: View {
    var file: String? = nil
    @Environment(SessionModel.self) private var model
    @State private var links: [FileLink] = []
    @State private var loading = true
    @State private var failure: String?
    private var shown: [FileLink] { file.map { id in links.filter { $0.file == id } } ?? links }

    var body: some View {
        List {
            if let failure { FileInlineFailure(message: failure, working: loading) { Task { await load() } } }
            ForEach(shown) { link in
                NavigationLink { FileLinkDetailView(link: link) { revoked in replace(revoked) } } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(link.name ?? String(link.id.prefix(12)) + "…").lineLimit(2)
                        (Text(link.stateTitle) + Text(verbatim: " · ") + Text("Downloads") + Text(verbatim: " \(link.downloadsText)"))
                            .font(.caption).foregroundStyle(link.active ? Color.secondary : Color.orange)
                    }.padding(.vertical, 2)
                }.accessibilityIdentifier("file.link.item.\(link.id)")
            }
            if loading && shown.isEmpty { ProgressView("Loading…") }
            else if !loading && failure == nil && shown.isEmpty {
                ContentUnavailableView("No share links yet", systemImage: "link",
                    description: Text("Choose Share by link on a file to send it to anyone. They need no dMsg account or app."))
            }
        }.privateScreen().navigationTitle("Share links").navigationBarTitleDisplayMode(.inline)
            .task { await load() }.refreshable { await load() }
    }

    private func load() async {
        let stamp = model.epoch; loading = true; failure = nil; defer { loading = false }
        if let saved = try? await model.fileLinks(refresh: false), stamp == model.epoch { links = saved }
        do {
            let fresh = try await model.fileLinks(refresh: true)
            guard stamp == model.epoch, !Task.isCancelled else { return }
            links = fresh
        } catch {
            guard stamp == model.epoch, !Task.isCancelled, model.fileFailureMessage(error) != nil else { return }
            failure = String(localized: "Share links could not be refreshed. Showing saved links.")
        }
    }

    private func replace(_ link: FileLink) {
        if let index = links.firstIndex(where: { $0.id == link.id }) { links[index] = link }
    }
}

struct FileLinkDetailView: View {
    let link: FileLink
    let revoked: (FileLink) -> Void
    @Environment(SessionModel.self) private var model
    @State private var current: FileLink?
    @State private var confirming = false
    @State private var working = false
    @State private var failure: String?
    private var shown: FileLink { current ?? link }

    var body: some View {
        List {
            if let name = shown.name {
                FileSummaryRow(name: name, mime: "", subtitle: shown.size.map(fileSize) ?? "", expanded: true).listRowBackground(Color.clear)
            }
            Section("Details") {
                LabeledContent("Status", value: shown.stateTitle).accessibilityIdentifier("file.link.state")
                LabeledContent("Downloads", value: shown.downloadsText)
                LabeledContent("Expires") { Text(fileDate(shown.expiresAt), format: .dateTime) }
                if shown.password { Label("Password protected", systemImage: "lock") }
            }
            Section {
                if shown.active {
                    if let url = shown.url {
                        ShareLink(item: url) { Label("Share link", systemImage: "square.and.arrow.up") }
                        Button("Copy link", systemImage: "doc.on.doc") { model.clipboard.copy(url); model.notice = String(localized: "Copied") }
                            .accessibilityIdentifier("file.link.copy")
                    } else {
                        Text("The full link is not available on this device.").font(.footnote).foregroundStyle(.secondary)
                    }
                }
                if shown.state != "revoked" {
                    SettingsAction("Revoke", systemImage: "xmark.circle", role: .destructive) { confirming = true }
                        .disabled(working).accessibilityIdentifier("file.link.revoke")
                }
                if working { ProgressView("Loading…") }
                if let failure { Text(failure).font(.footnote).foregroundStyle(.red).accessibilityIdentifier("files.action.failure") }
            }
        }.privateScreen().navigationTitle("Share link").navigationBarTitleDisplayMode(.inline)
            .confirmationDialog("Revoke this link?", isPresented: $confirming, titleVisibility: .visible) {
                Button("Revoke", role: .destructive) { Task { await revoke() } }
            } message: { Text("Revoking stops new downloads through this link. Local files stay; downloads already started and downloaded copies cannot be recalled.") }
    }

    private func revoke() async {
        guard !working else { return }; let stamp = model.epoch
        working = true; failure = nil; defer { working = false }
        do {
            let value = try await model.revokeFileLink(link.id)
            guard stamp == model.epoch, !Task.isCancelled else { return }
            current = value; revoked(value)
        } catch { if stamp == model.epoch, !Task.isCancelled { failure = model.fileFailureMessage(error) } }
    }
}
