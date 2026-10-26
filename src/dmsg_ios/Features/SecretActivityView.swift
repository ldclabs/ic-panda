import SwiftUI

struct SecretActivityView: View {
    @Environment(SessionModel.self) private var model
    @State private var entries: [ApprovalView] = []
    @State private var next: String?
    @State private var loaded = false
    @State private var failed = false
    @State private var working = false
    @State private var cancelling: ApprovalView?
    @State private var reviewing: ApprovalView?
    var body: some View {
        List {
            NoticeBar(text: String(localized: "This history belongs to this device. It does not mirror desktop-local CLI, SSH or file-mount approvals from another device."))
            ForEach(entries) { entry in
                VStack(alignment: .leading, spacing: 8) {
                    NavigationLink { ApprovalDetailView(request: entry) } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(approvalTitle(entry.type)).font(.headline)
                            Text(secretStateTitle(entry.state)).font(.caption).foregroundStyle(.secondary)
                            Text(entry.source).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                    if entry.state == "awaiting_user" { Button("Cancel pending request", role: .destructive) { cancelling = entry }.disabled(working) }
                    if entry.state == "lost" { Button("Record manual review") { reviewing = entry }.disabled(working) }
                }.padding(.vertical, 4)
            }
            if !loaded { ProgressView() }
            else if entries.isEmpty && failed { Button("Retry loading activity") { Task { await load(reset: true) } } }
            else if entries.isEmpty { ContentUnavailableView("No activity on this device", systemImage: "clock") }
            if next != nil { Button("Load earlier activity") { Task { await load(reset: false) } }.disabled(working) }
        }.privateScreen().navigationTitle("Activity on this device").navigationBarTitleDisplayMode(.inline).task { await load(reset: true) }.refreshable { await load(reset: true) }
            .confirmationDialog("Cancel this pending request?", isPresented: Binding(get: { cancelling != nil }, set: { if !$0 { cancelling = nil } })) {
                Button("Cancel request", role: .destructive) { if let entry = cancelling { Task { await act(entry, reviewed: false) } } }
            } message: { Text("Cancellation applies to this undecided request. It does not undo a completed delivery.") }
            .confirmationDialog("Have you reviewed the missing outcome?", isPresented: Binding(get: { reviewing != nil }, set: { if !$0 { reviewing = nil } })) {
                Button("Record manual review") { if let entry = reviewing { Task { await act(entry, reviewed: true) } } }
            } message: { Text("This records that you checked the outcome manually. It does not mark the operation successful or run it again.") }
    }

    private func load(reset: Bool) async {
        #if DEBUG
        if model.uiTesting && model.engine == nil { loaded = true; failed = true; return }
        #endif
        guard let engine = model.engine else { return }; let epoch = model.epoch
        do {
            let snapshot = try await engine.secretActivity(before: reset ? nil : next, limit: 40)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            let page: SecretActivityPage = try model.decode(snapshot)
            if reset { entries = page.items } else {
                let existing = Set(entries.map(\.id)); entries += page.items.filter { !existing.contains($0.id) }
            }
            next = page.next; loaded = true; failed = false
        } catch { if epoch == model.epoch { loaded = true; failed = true; model.report(error) } }
    }
    private func act(_ entry: ApprovalView, reviewed: Bool) async {
        cancelling = nil; reviewing = nil
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            if reviewed { _ = try await engine.secretActivityReviewed(id: entry.id) }
            else { _ = try await engine.secretActivityCancel(id: entry.id) }
            if epoch == model.epoch { await load(reset: true); await model.loadApprovals() }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}
