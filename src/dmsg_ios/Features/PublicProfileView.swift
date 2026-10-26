import SwiftUI
import PhotosUI

struct PublicProfileView: View {
    @Environment(SessionModel.self) private var model
    @State private var name = ""
    @State private var about = ""
    @State private var updated: Int64?
    @State private var status = ""
    @State private var confirming = false
    @State private var working = false
    @State private var pending = false
    @State private var photo: PhotosPickerItem?
    @State private var avatarBytes: Data?
    @State private var avatarPreview: UIImage?
    @State private var uploadedAvatar: String?
    @State private var changingAvatar = false
    struct Publication: Decodable { let published: Bool; let updatedAt: Int64?; let pending: Bool? }
    var body: some View {
        Form {
            NoticeBar(text: String(localized: "This profile is public. Do not include secrets or private contact information."))
            Section("Public profile") {
                TextField("Name", text: $name)
                TextField("About", text: $about, axis: .vertical).lineLimit(3...8)
            }.disabled(working || pending)
            Section("Public avatar") {
                if let avatarPreview { Image(uiImage: avatarPreview).resizable().scaledToFit().frame(height: 96) }
                PhotosPicker(selection: $photo, matching: .images) { Label("Choose avatar", systemImage: "photo") }.disabled(working || pending || changingAvatar)
                if changingAvatar { ProgressView() }
                Text("The selected image stays on this device until you publish. Published avatars are public and copies may remain outside dMsg.").font(.footnote).foregroundStyle(.secondary)
            }
            if !status.isEmpty { Text(status).font(.footnote).foregroundStyle(.secondary) }
            if working { ProgressView() }
            if pending { Button("Retry publication") { Task { await publish(retry: true) } }.disabled(working) }
            else { Button("Publish profile") { confirming = true }.disabled(updated == nil || working || changingAvatar) }
            if updated == nil && !working { Button("Retry loading profile") { Task { await load() } } }
        }.privateScreen().navigationTitle("Public profile").task { await load() }
            .onChange(of: photo) { _, selected in Task {
                let epoch = model.epoch
                changingAvatar = true; defer { if selected == photo { changingAvatar = false } }
                do {
                    guard let selected, let imported = try await selected.loadTransferable(type: PhotoImport.self) else { return }
                    defer { imported.finish() }
                    let bytes = try await AvatarImage.prepare(imported.file)
                    guard epoch == model.epoch, selected == photo else { return }
                    avatarBytes = bytes; uploadedAvatar = nil; avatarPreview = await AvatarImage.decode(bytes)
                } catch { if epoch == model.epoch { model.report(error) } }
            } }
            .confirmationDialog("Publish these details publicly?", isPresented: $confirming) {
                Button("Publish") { Task { await publish(retry: false) } }
            }
    }
    private func load() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            let result = try await engine.profileEditor()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let profile: AccountProfile = try model.decode(result)
            name = profile.name ?? ""; about = profile.description ?? ""; updated = profile.updatedAt
            let publication = try await engine.profilePublication()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            accept(try model.decode(publication))
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func accept(_ result: Publication) {
        pending = result.pending == true || !result.published
        if let version = result.updatedAt { updated = version }
        status = pending ? String(localized: "Saved. Publication is pending.") : String(localized: "Published")
    }
    private func publish(retry: Bool) async {
        guard let engine = model.engine, let updated, !working else { return }
        let epoch = model.epoch, submittedName = name, submittedAbout = about
        working = true; model.activity(); defer { working = false }
        do {
            if !retry, let avatarBytes, uploadedAvatar == nil {
                uploadedAvatar = try await coreOperation { try await engine.uploadProfileAvatar(bytes: avatarBytes, operation: $0) }
                guard epoch == model.epoch else { return }
            }
            let result = try await (retry ? engine.retryProfilePublication() : engine.publishProfile(
                name: submittedName, description: submittedAbout, expectedUpdatedAt: updated, avatarUrl: uploadedAvatar))
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let publication: Publication = try model.decode(result)
            accept(publication)
            if publication.published { model.profileDidChange() }
        } catch {
            if epoch == model.epoch {
                if let result = try? await engine.profilePublication(), let state: Publication = try? model.decode(result) { accept(state) }
                model.report(error)
            }
        }
    }
}
