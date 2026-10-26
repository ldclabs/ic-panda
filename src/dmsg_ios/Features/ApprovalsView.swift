import SwiftUI

struct ApprovalsView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Desktop-local command approvals stay on that desktop. Only requests available to this device appear here."))
            Section("Approvals") {
                ForEach(model.approvals) { request in
                    NavigationLink { ApprovalDetailView(request: request) } label: {
                        HStack(alignment: .top, spacing: 12) {
                            ObjectIcon(name: request.type == "device_pair" ? "laptopcomputer.and.iphone" : "signature")
                            VStack(alignment: .leading, spacing: 5) {
                                Text(approvalTitle(request.type)).font(.headline)
                                Text(request.source).font(.subheadline).foregroundStyle(.secondary)
                                Text(Date(timeIntervalSince1970: Double(request.expiresAt) / 1000), style: .relative)
                                    .font(.caption).foregroundStyle(.orange)
                            }
                        }.padding(.vertical, 6)
                    }
                }
                if model.approvals.isEmpty { ContentUnavailableView("All caught up", systemImage: "checkmark.shield", description: Text("Requests that need your decision appear here.")) }
            }
        }.privateScreen().navigationTitle("Approvals")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .task { await model.loadApprovals() }
            .refreshable { await model.loadApprovals() }
    }
}

func approvalTitle(_ type: String) -> String {
    switch type {
    case "device_pair": String(localized: "Authorize a new device")
    case "device_revoke": String(localized: "Revoke a device")
    case "device_capabilities": String(localized: "Change device permissions")
    case "attest": String(localized: "Sign a statement")
    case "secret_share": String(localized: "Publish a secret snapshot")
    case "secret_package_import": String(localized: "Import selected secrets")
    case "approval_key": String(localized: "Register a hardware approval key")
    case "approval_key_retire": String(localized: "Retire a hardware approval key")
    case "hardware_policy": String(localized: "Change hardware approval policy")
    case "secret_discovery_import": String(localized: "Import desktop-discovered secrets")
    case "secret_mount_config": String(localized: "Configure a desktop file mapping")
    case "secret_mount_read": String(localized: "Read from a desktop file mapping")
    case "secret_ssh": String(localized: "Use an SSH signing key")
    case "secret_ssh_binding": String(localized: "Register an SSH binding")
    case "secret_process": String(localized: "Deliver a secret to a local process")
    case "secret_policy": String(localized: "Change a device secret policy")
    case "secret_use": String(localized: "Use a secret through a device policy")
    case "account": String(localized: "Change account security")
    case "principal": String(localized: "Change identity settings")
    case "connect": String(localized: "Review a web connection request")
    default: type.replacingOccurrences(of: "_", with: " ")
    }
}

struct ApprovalDetailView: View {
    let request: ApprovalView
    @Environment(SessionModel.self) private var model
    @State private var payload: DisplayValue?
    @State private var working = false
    @State private var decision: Bool?
    @State private var result: DisplayValue?
    @State private var state: String?
    @State private var supported = false
    @State private var readyAt = Date.distantFuture
    @State private var decisionTask: Task<Void, Never>?
    private let supportedTypes: Set<String> = ["device_pair", "device_revoke", "device_capabilities", "attest", "connect", "account", "principal", "approval_key", "approval_key_retire", "hardware_policy", "secret_share", "secret_package_import"]

    var body: some View {
        List {
            HStack(alignment: .top, spacing: 12) {
                ObjectIcon(name: "checkmark.shield")
                VStack(alignment: .leading, spacing: 5) {
                    Text(approvalTitle(request.type)).font(.title2.bold())
                    Text(request.source).font(.subheadline).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }.listRowBackground(Color.clear).listRowSeparator(.hidden)
            if request.type == "device_pair" {
                NoticeBar(text: String(localized: "This device may receive your private content. The content root must be redistributed after approval."))
                    .listRowBackground(Color.clear)
            }
            if let notice = approvalNotice(request.type) { NoticeBar(text: notice).listRowBackground(Color.clear) }
            if payload != nil && !supported { NoticeBar(text: String(localized: "This request requires a supported desktop client.")) }
            LabeledContent("Account ID") {
                Text(model.session?.account ?? "").font(.caption.monospaced()).textSelection(.enabled)
                    .environment(\.layoutDirection, .leftToRight)
            }
            Section("Review the complete request") {
                if let payload { CanonicalDetails(value: payload) }
                else { ProgressView() }
            }
            if let result { Section("Result") { CanonicalDetails(value: result) } }
        }.privateScreen().navigationTitle("Review request").navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom) {
                if result == nil && state == "awaiting_user" {
                    VStack(spacing: 8) {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            let expired = context.date.timeIntervalSince1970 * 1000 >= Double(request.expiresAt)
                            Text(expired ? "This request has expired" : "Your decision applies only to this request.")
                                .font(.caption).foregroundStyle(.secondary)
                            HStack {
                                Button("Reject", role: .destructive) { decision = false }.buttonStyle(.bordered).frame(maxWidth: .infinity)
                                Button { decision = true } label: {
                                    if working { ProgressView() } else { Text("Approve").frame(maxWidth: .infinity) }
                                }.buttonStyle(.borderedProminent).frame(maxWidth: .infinity)
                                    .disabled(expired || context.date < readyAt || !supported)
                            }.controlSize(.large).disabled(payload == nil || working)
                        }
                    }.padding(16).background(.bar)
                } else if result == nil && (state == "executing" || state == "unknown") {
                    VStack(spacing: 8) {
                        Text("The result is not confirmed. Reconcile this request before retrying.")
                            .font(.caption).foregroundStyle(.secondary)
                        Button("Refresh") { Task { await reconcile() } }.disabled(working)
                    }.padding(16).background(.bar)
                }
            }
            .task(id: request.id + ":\(model.epoch):\(model.concealed)") { await load() }
            .confirmationDialog(decision == true ? "Approve the reviewed request?" : "Reject this request?",
                isPresented: Binding(get: { decision != nil }, set: { if !$0 { decision = nil } })) {
                Button(decision == true ? "Approve this request" : "Reject", role: decision == true ? nil : .destructive) {
                    if let approve = decision { decisionTask = Task { await decide(approve) } }
                }
            }
            .onDisappear { decisionTask?.cancel() }
    }

    private func load() async {
        guard !working else { return }
        supported = false; state = nil; payload = nil; readyAt = .distantFuture
        guard !model.concealed else { return }
        let epoch = model.epoch
        do {
            guard let snapshot = try await model.engine?.approval(id: request.id), epoch == model.epoch else { return }
            let fresh: ApprovalView = try model.decode(snapshot)
            guard fresh.digest == request.digest else { throw CocoaError(.coderInvalidValue) }
            state = fresh.state
            supported = fresh.mobileSupported == true && supportedTypes.contains(fresh.type)
            payload = try model.decode(snapshot); readyAt = .now.addingTimeInterval(1)
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func reconcile() async {
        guard let engine = model.engine, !working else { return }
        working = true; model.activity(); defer { working = false }
        let epoch = model.epoch
        do {
            _ = try await coreOperation { try await engine.reconcile(operation: $0) }
            let snapshot = try await engine.approval(id: request.id)
            guard epoch == model.epoch else { return }
            let fresh: ApprovalView = try model.decode(snapshot)
            guard fresh.digest == request.digest else { throw CocoaError(.coderInvalidValue) }
            state = fresh.state; payload = try model.decode(snapshot)
            supported = fresh.mobileSupported == true && supportedTypes.contains(fresh.type)
            await model.loadApprovals()
        } catch { if epoch == model.epoch { supported = false; state = nil; model.report(error) } }
        working = false
        if epoch == model.epoch && !model.concealed { await load() }
    }

    private func decide(_ approve: Bool) async {
        guard let engine = model.engine, !working, !approve || supported else { return }
        decision = nil; working = true; model.authenticating = true; model.activity()
        let epoch = model.epoch
        defer { working = false; if epoch == model.epoch { model.authenticating = false } }
        do {
            let snapshot = try await coreOperation { try await engine.decide(id: request.id, digest: request.digest, approve: approve, operation: $0) }
            guard epoch == model.epoch else { return }
            result = try model.decode(snapshot); await model.updateSession(); await model.loadApprovals()
        } catch { if epoch == model.epoch { supported = false; state = nil; model.report(error) } }
        working = false
        if epoch == model.epoch && !model.concealed { await load() }
    }

    private func approvalNotice(_ type: String) -> String? {
        switch type {
        case "secret_share": String(localized: "Approval publishes exactly the reviewed fields and attachments as an independent snapshot. Verify the destination and limits; recipients can retain copies.")
        case "secret_package_import": String(localized: "Approval imports only your selected fields and attachments as new independent items. Files remain local and inherit the destination folder's access grants.")
        case "approval_key": String(localized: "Verify the full device and key fingerprints. This key will prove hardware approval for the protected actions of this account.")
        case "approval_key_retire": String(localized: "Retiring a key removes its ability to approve protected actions. Check that another authorized approval path remains available.")
        case "hardware_policy": String(localized: "This changes which account actions require a hardware signature. Disabling protection permits those actions without that hardware proof.")
        default: nil
        }
    }
}

struct CanonicalDetails: View {
    let value: DisplayValue
    struct Row: Identifiable { let id: String; let label: String; let text: String }
    private var rows: [Row] { flatten(value, path: "", label: "") }
    var body: some View {
        ForEach(rows) { row in
            VStack(alignment: .leading, spacing: 4) {
                if !row.label.isEmpty { Text(row.label).font(.caption).foregroundStyle(.secondary) }
                Text(row.text).font(.subheadline).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }.padding(.vertical, 4).frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func flatten(_ value: DisplayValue, path: String, label: String) -> [Row] {
        switch value {
        case .object(let values):
            return values.keys.sorted().flatMap { key in flatten(values[key]!, path: path + "/" + key,
                label: label.isEmpty ? key : label + " · " + key) }
        case .array(let values):
            if values.isEmpty { return [Row(id: path, label: label, text: "—")] }
            return values.enumerated().flatMap { index, value in flatten(value, path: path + "/\(index)", label: label + " \(index + 1)") }
        default: return [Row(id: path, label: label, text: value.text)]
        }
    }
}
