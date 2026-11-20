import SwiftUI
import DMsgCore

struct LocalImportView: View {
    private struct Plan: Decodable {
        struct Entry: Decodable { let index: UInt32; let title: String; let state: String; let attachments: [UInt32?] }
        struct File: Decodable {
            let index: UInt32
            let name, version: String
            let path: [String]
            let local, current: Bool
            let size: Int64
            let createdAt: UInt64
        }
        let id: String; let target: String; let items: [Entry]; let files: [File]
    }
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var plan: Plan?
    @State private var items = Set<UInt32>()
    @State private var files = Set<UInt32>()
    private struct ProgressState: Decodable { let bytes: Int64? }
    @State private var copied: Int64 = 0
    @State private var busy = false
    @State private var visible = false

    var body: some View {
        Form {
            if let plan {
                let required = Set(plan.items.filter { items.contains($0.index) }.flatMap { $0.attachments.compactMap { $0 } })
                Section("Account") {
                    Text(verbatim: plan.target).font(.body.monospaced())
                    Text("Vault entries follow normal sync. Attachments are saved in Files on this device.")
                }
                Section("Vault") {
                    ForEach(plan.items, id: \.index) { entry in
                        Toggle(isOn: selection($items, entry.index)) {
                            Text(entry.title)
                            Text(verbatim: entry.state == "same" ? String(localized: "Already here") : entry.state == "deleted" ? String(localized: "Deleted here · save a copy") : entry.state == "changed" ? String(localized: "Different content · save a copy") : String(localized: "New items"))
                                .font(.caption).foregroundStyle(.secondary)
                            let omitted = entry.attachments.filter { $0 == nil }.count
                            if omitted > 0 { LabeledContent("Omitted attachments", value: omitted.formatted()).font(.footnote) }
                        }.disabled(entry.state == "same")
                    }
                }
                Section("Files") {
                    ForEach(plan.files, id: \.index) { file in
                        Toggle(isOn: required.contains(file.index) ? .constant(true) : selection($files, file.index)) {
                            Text(verbatim: (file.path + [file.name]).joined(separator: "/"))
                            Text(verbatim: ByteCountFormatter.string(fromByteCount: file.size, countStyle: .file)
                                 + " · " + Date(timeIntervalSince1970: Double(file.createdAt) / 1000).formatted()
                                 + " · " + file.version.prefix(8))
                            if file.current { Text("Current version").font(.caption).foregroundStyle(.secondary) }
                        }.disabled(!file.local || required.contains(file.index))
                    }
                }
                Button("Import") { Task { await commit(plan) } }.disabled(items.isEmpty && files.isEmpty)
            } else {
                Button("Import from local account") { Task { await preview() } }
            }
            if busy {
                ProgressView()
                if plan != nil { Text(ByteCountFormatter.string(fromByteCount: copied, countStyle: .file)) }
            }
        }.disabled(busy).task(id: busy) {
            guard busy, let id = plan?.id, let engine = model.engine else { return }
            while !Task.isCancelled {
                if let value = try? await engine.localImportStatus(id: id),
                   let status: ProgressState = try? model.decode(value), let bytes = status.bytes { copied = bytes }
                do { try await Task.sleep(for: .milliseconds(750)) } catch { return }
            }
        }.navigationTitle("Import from local account").privateScreen()
            .onAppear { visible = true }
            .onDisappear {
                visible = false
                if let id = plan?.id, let engine = model.engine { Task { try? await engine.localImportCancel(id: id) } }
            }
    }

    private func selection(_ values: Binding<Set<UInt32>>, _ index: UInt32) -> Binding<Bool> {
        Binding(get: { values.wrappedValue.contains(index) }, set: { yes in
            if yes { values.wrappedValue.insert(index) } else { values.wrappedValue.remove(index) }
        })
    }

    private func preview() async {
        guard !busy, let engine = model.engine else { return }
        busy = true; defer { busy = false }
        do {
            let received: Plan = try model.decode(try await engine.localImportPreview())
            guard visible else { try? await engine.localImportCancel(id: received.id); return }
            plan = received; items = Set(received.items.filter { $0.state == "new" && !$0.attachments.contains(nil) }.map(\.index))
        } catch { if visible { model.report(error) } }
    }

    private func commit(_ reviewed: Plan) async {
        guard !busy, let engine = model.engine else { return }
        busy = true; defer { busy = false }
        do {
            _ = try await engine.localImportCommit(id: reviewed.id, items: items.sorted(), files: files.sorted(), folder: "root")
            plan = nil; await model.refresh(); if visible { dismiss() }
        } catch {
            if case MobileError.Failure(let code) = error, ["EXPIRED", "VERSION_CONFLICT"].contains(code) {
                try? await engine.localImportCancel(id: reviewed.id)
                plan = nil; files = []; copied = 0
            }
            if visible { model.report(error) }
        }
    }
}
