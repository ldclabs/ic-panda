import SwiftUI

struct FileManagementView: View {
    let file: LibraryFile
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var moving = false
    @State private var working = false
    var body: some View {
        Form {
            TextField("File name", text: $name)
            Button("Save name") { Task { await rename() } }.disabled(working || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            Button("Move to another folder") { moving = true }.disabled(working)
        }.privateScreen().navigationTitle("Manage file").onAppear { name = file.name }
            .sheet(isPresented: $moving) { NavigationStack { SecretFolderPicker { id, _ in Task { await move(id) } } } }
    }
    private func rename() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do { try await engine.renameFile(id: file.id, name: name); if epoch == model.epoch { await model.refresh(); dismiss() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
    private func move(_ folder: String) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do { try await engine.moveFile(id: file.id, folder: folder); if epoch == model.epoch { await model.refresh(); dismiss() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct FileVersionsView: View {
    let file: LibraryFile
    @Environment(SessionModel.self) private var model
    @State private var versions: [Version] = []
    struct Version: Decodable, Identifiable { let id: String; let size: UInt64; let sha256: String; let createdAt: UInt64; let current: Bool; let local: Bool }
    struct Details: Decodable { let versions: [Version] }
    var body: some View {
        List(versions) { version in
            NavigationLink {
                FileDetailView(file: LibraryFile(id: file.id, name: file.name, mime: file.mime, size: version.size,
                    current: version.id, sha256: version.sha256, status: version.local ? "local" : "backed_up", versions: file.versions), historical: true)
                    .id(version.id)
            } label: {
                VStack(alignment: .leading, spacing: 4) {
                    Text(Date(timeIntervalSince1970: Double(version.createdAt) / 1000), format: .dateTime)
                    Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: version.size), countStyle: .file)).font(.caption).foregroundStyle(.secondary)
                    if version.current { Text("Current version").font(.caption) }
                    if !version.local { Text("Restore this version from cloud backups before opening it.").font(.caption).foregroundStyle(.secondary) }
                }
            }.disabled(!version.local)
        }.privateScreen().navigationTitle("File versions").task {
            do { if let result = try await model.engine?.fileDetails(id: file.id) { let result: Details = try model.decode(result); versions = result.versions.reversed() } }
            catch { model.report(error) }
        }
    }
}
