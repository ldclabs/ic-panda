import SwiftUI

struct SecretFieldReferenceView: View {
    let itemID: String
    let fieldID: String
    @Environment(SessionModel.self) private var model
    @State private var reference: String?
    @State private var rotation: SecretRotation?
    @State private var confirming = false
    @State private var working = false
    @State private var loading = true

    var body: some View {
        List {
            Section("Field reference") {
                if let reference {
                    Text(reference).font(.caption.monospaced()).textSelection(.enabled)
                        .environment(\.layoutDirection, .leftToRight)
                    Button("Copy reference", systemImage: "link") { model.clipboard.copy(reference) }
                    NavigationLink("Dependencies on this device") { SecretDependenciesView(reference: reference) }
                    Button("Start a rotation checklist", systemImage: "arrow.triangle.2.circlepath") { confirming = true }.disabled(working)
                } else if loading { ProgressView() }
                else { Button("Retry") { Task { await load() } } }
            }
            NoticeBar(text: String(localized: "A reference identifies a field; it does not contain its secret value or grant permission to read it."))
        }.privateScreen().navigationTitle("Field reference").navigationBarTitleDisplayMode(.inline).task { await load() }
            .confirmationDialog("Start a rotation checklist?", isPresented: $confirming) {
                Button("Start checklist") { Task { await createRotation() } }
            } message: { Text("This tracks your work on this device. It does not create, test or revoke credentials at their provider.") }
            .navigationDestination(isPresented: Binding(get: { rotation != nil }, set: { if !$0 { rotation = nil } })) {
                if let rotation { SecretRotationView(initial: rotation) }
            }
    }

    private func load() async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let snapshot = try await model.engine?.secretReference(itemId: itemID, fieldId: fieldID), epoch == model.epoch else { return }
            let result: SecretReferenceResult = try model.decode(snapshot); reference = result.uri
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func createRotation() async {
        guard let reference, let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            let result = try await engine.secretRotationCreate(reference: reference)
            guard epoch == model.epoch else { return }; rotation = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretItemAttachmentsView: View {
    let itemID: String
    @Environment(SessionModel.self) private var model
    @State private var listing: SecretAttachmentListing?
    @State private var loading = true

    var body: some View {
        List {
            if let listing {
                ForEach(listing.files) { entry in
                    if let file = entry.details?.file {
                        NavigationLink { FileDetailView(file: file) } label: { FileRow(file: file) }
                    } else {
                        VStack(alignment: .leading, spacing: 4) {
                            Label("Attachment unavailable on this device", systemImage: "doc.badge.ellipsis")
                            Text(entry.id).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                }
                if listing.files.isEmpty { ContentUnavailableView("No item attachments", systemImage: "paperclip") }
            } else if loading { ProgressView() }
            else { Button("Retry") { Task { await load() } } }
            NoticeBar(text: String(localized: "Item references can sync before their files. Only files available on this device can be added to a new snapshot."))
        }.privateScreen().navigationTitle("Item attachments").navigationBarTitleDisplayMode(.inline).task { await load() }.refreshable { await load() }
    }

    private func load() async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretItemAttachments(itemId: itemID), epoch == model.epoch else { return }
            listing = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretDependenciesView: View {
    let reference: String
    @Environment(SessionModel.self) private var model
    @State private var index: SecretReferenceIndex?
    @State private var loading = true
    var body: some View {
        List {
            NoticeBar(text: String(localized: "This inventory covers references and delivery records on this device. It does not search other devices or arbitrary project files."))
            if let index { SecretDependencyRows(index: index) }
            else if loading { ProgressView() }
            else { Button("Retry") { Task { await load() } } }
        }.privateScreen().navigationTitle("Local dependencies").navigationBarTitleDisplayMode(.inline).task { await load() }.refreshable { await load() }
    }
    private func load() async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretReferences(reference: reference), epoch == model.epoch else { return }
            index = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

struct SecretDependencyRows: View {
    let index: SecretReferenceIndex
    var body: some View {
        ForEach(index.dependencies, id: \.listID) { dependency in
            VStack(alignment: .leading, spacing: 5) {
                Text(dependency.label.isEmpty ? secretDependencyKind(dependency.kind) : dependency.label).font(.headline)
                Text(secretStateTitle(dependency.state)).font(.caption).foregroundStyle(.secondary)
                Text(secretNextAction(dependency.action)).font(.subheadline)
                Text(dependency.id).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(2)
            }.padding(.vertical, 4)
        }
        if index.dependencies.isEmpty { Text("No dependencies recorded on this device.").foregroundStyle(.secondary) }
    }
}
