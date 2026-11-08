import SwiftUI
@preconcurrency import DMsgCore

struct ApprovalsView: View {
    var showsDone = false
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        List {
            if model.approvalsFailed { NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish.")) }
            Section {
                ForEach(model.approvals) { request in
                    NavigationLink { ApprovalDetailView(request: request) } label: {
                        HStack(alignment: .top, spacing: 10) {
                            ObjectIcon(glyph: request.type == "device_pair" ? .device : .signature)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(approvalTitle(request.type)).font(.headline)
                                Text(request.source).font(.caption).foregroundStyle(.secondary)
                                Text(Date(timeIntervalSince1970: Double(request.expiresAt) / 1000), style: .relative)
                                    .font(.caption).foregroundStyle(.orange)
                            }
                        }
                    }.compactObjectRow()
                }
                if model.approvals.isEmpty { ContentUnavailableView("All caught up", image: AppGlyph.approvals.assetName, description: Text("Requests that need your decision appear here.")) }
            } footer: {
                Text("Desktop-local command approvals stay on that desktop. Only requests available to this device appear here.")
            }
        }.privateScreen().navigationTitle("Approvals")
            .toolbar { if showsDone { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } } }
            .task(id: model.contentRevision) { await model.loadApprovals(); await model.loadApprovals(refresh: true) }
            .refreshable { await model.loadApprovals(refresh: true) }
    }
}

func approvalTitle(_ type: String) -> String {
    switch type {
    case "wallet": String(localized: "Review wallet action")
    case "delegation": String(localized: "Review agent delegation")
    case "grant": String(localized: "Review agent permissions")
    case "device_pair": String(localized: "Authorize a new device")
    case "device_transfer": String(localized: "Send to a device")
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
    @State private var observedVersion: UInt64?
    @State private var supported = false
    @State private var readyAt = Date.distantFuture
    @State private var decisionTask: Task<Void, Never>?


    private var recoveryNotice: String? {
        guard request.type == "account", let payload, case .object(let fields) = payload,
              case .object(let detail)? = fields["detail"],
              case .string("recovery_policy")? = detail["change"],
              case .boolean(let enabled)? = detail["enabled"] else { return nil }
        return String(localized: enabled
            ? "If every device is lost, a bound login can recover uploaded content after the waiting period. A stolen login can do the same unless an authorized device cancels in time."
            : "Keep an authorized device you can unlock to move to a new device. If all authorized devices are lost, recovering your login will not recover cloud content.")
    }

    /// The action itself, every field of it, in the terms of the request.
    private var reviewed: DisplayValue? {
        if case .object(let fields)? = payload, let detail = fields["detail"], !detail.isNull { return detail }
        return payload
    }

    private var envelope: DisplayValue? {
        guard case .object(var fields)? = payload, let detail = fields["detail"], !detail.isNull else { return nil }
        fields.removeValue(forKey: "detail")
        return .object(fields)
    }

    var body: some View {
        List {
            HStack(alignment: .top, spacing: 12) {
                ObjectIcon(glyph: .approvals)
                VStack(alignment: .leading, spacing: 5) {
                    Text(approvalTitle(request.type)).font(.title2.bold())
                    Text(request.source).font(.subheadline).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }.listRowBackground(Color.clear).listRowSeparator(.hidden)
                .reminderVisible(.approval(request.id), watermark: observedVersion, ready: observedVersion != nil)
            if request.type == "device_pair" {
                NoticeBar(text: String(localized: "This device may receive your private content. The content root must be redistributed after approval."))
                    .listRowBackground(Color.clear)
            }
            if let recoveryNotice { NoticeBar(text: recoveryNotice).listRowBackground(Color.clear) }
            if let notice = approvalNotice(request.type) { NoticeBar(text: notice).listRowBackground(Color.clear) }
            if payload != nil && !supported { NoticeBar(text: String(localized: "This request requires a supported desktop client.")) }
            LabeledContent("Account ID") {
                Text(model.session?.account ?? "").font(.caption.monospaced()).textSelection(.enabled)
                    .environment(\.layoutDirection, .leftToRight)
            }
            Section("Review the complete request") {
                if let reviewed { CanonicalDetails(value: reviewed) } else { ProgressView() }
            }
            // Bookkeeping around the request, not part of what is being decided.
            if let envelope { DisclosureGroup("Technical details") { CanonicalDetails(value: envelope) } }
            if let result { Section("Result") { CanonicalDetails(value: result) } }
        }.privateScreen().navigationTitle("Review request").navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom) {
                if result == nil && state == "awaiting_user" {
                    VStack(spacing: 8) {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            let expired = context.date.timeIntervalSince1970 * 1000 >= Double(request.expiresAt)
                            Text(LocalizedStringKey(expired ? "This request has expired" : "Your decision applies only to this request."))
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
                        Text("The result is not confirmed. Check it again before retrying.")
                            .font(.caption).foregroundStyle(.secondary)
                        Button("Refresh") { Task { await reconcile() } }.disabled(working)
                    }.padding(16).background(.bar)
                }
            }
            .task(id: request.id + ":\(model.epoch):\(model.concealed)") { await load() }
            .confirmationDialog(LocalizedStringKey(decision == true ? "Approve the reviewed request?" : "Reject this request?"),
                isPresented: Binding(get: { decision != nil }, set: { if !$0 { decision = nil } })) {
                Button(LocalizedStringKey(decision == true ? "Approve this request" : "Reject"), role: decision == true ? nil : .destructive) {
                    if let approve = decision { decisionTask = Task { await decide(approve) } }
                }
            }
            .onDisappear { decisionTask?.cancel() }
    }

    private func load() async {
        guard !working else { return }
        supported = false; state = nil; payload = nil; observedVersion = nil; readyAt = .distantFuture
        guard !model.concealed else { return }
        let epoch = model.epoch
        do {
            guard let snapshot = try await model.engine?.approval(id: request.id), epoch == model.epoch else { return }
            let fresh: ApprovalView = try model.decode(snapshot)
            guard fresh.digest == request.digest else { throw CocoaError(.coderInvalidValue) }
            state = fresh.state
            observedVersion = fresh.updatedAt
            supported = fresh.mobileSupported == true
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
            supported = fresh.mobileSupported == true
            await model.loadApprovals()
        } catch { if epoch == model.epoch { supported = false; state = nil; model.report(error) } }
        working = false
        if epoch == model.epoch && !model.concealed { await load() }
    }

    private func decide(_ approve: Bool) async {
        guard let engine = model.engine, !working, !approve || supported else { return }
        decision = nil; working = true; model.activity()
        let epoch = model.epoch
        defer { working = false }
        do {
            let snapshot = try await coreOperation { try await engine.decide(id: request.id, digest: request.digest, approve: approve, operation: $0) }
            guard epoch == model.epoch else { return }
            result = try model.decode(snapshot); Haptics.success(); await model.updateSession(); await model.loadApprovals()
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
    private let rows: [Row]
    init(value: DisplayValue) { self.value = value; rows = Self.flatten(value, path: "", label: "") }
    var body: some View {
        ForEach(rows) { row in
            VStack(alignment: .leading, spacing: 4) {
                if !row.label.isEmpty { Text(row.label).font(.caption).foregroundStyle(.secondary) }
                Text(row.text).font(.subheadline).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }.padding(.vertical, 4).frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private static func flatten(_ value: DisplayValue, path: String, label: String, key: String = "") -> [Row] {
        switch value {
        case .object(let values):
            return values.keys.sorted().flatMap { child in
                let title = approvalFieldLabel(child)
                return flatten(values[child]!, path: path + "/" + child, label: label.isEmpty ? title : label + " · " + title, key: child)
            }
        case .array(let values):
            if values.isEmpty { return [Row(id: path, label: label, text: "—")] }
            // A plain list reads as one line, not one numbered row per item.
            if values.allSatisfy(\.isScalar) {
                return [Row(id: path, label: label, text: values.map { approvalFieldValue($0.text, key: key) }.joined(separator: ", "))]
            }
            return values.enumerated().flatMap { index, value in flatten(value, path: path + "/\(index)", label: label + " \(index + 1)", key: key) }
        default: return [Row(id: path, label: label, text: approvalFieldValue(value.text, key: key))]
        }
    }
}

/// Words for a request's field names. The name itself is never hidden from the
/// reader: a field without a translation still reads as its own words.
func approvalFieldLabel(_ key: String) -> String {
    var words = ""
    for character in key {
        if character == "_" || character == "-" { words.append(" "); continue }
        if character.isUppercase && !words.isEmpty { words.append(" ") }
        words.append(character.lowercased())
    }
    switch words {
    case "device id", "device": return String(localized: "Device ID")
    case "to device": return String(localized: "To device")
    case "role": return String(localized: "Role")
    case "capabilities": return String(localized: "Permissions")
    case "origin": return String(localized: "Website")
    case "name": return String(localized: "Name")
    case "type", "kind": return String(localized: "Type")
    case "size", "bytes": return String(localized: "Size")
    case "recipient": return String(localized: "Recipient")
    case "expires at": return String(localized: "Expires")
    case "version": return String(localized: "Version")
    case "file": return String(localized: "File")
    case "state": return String(localized: "Status")
    case "created at": return String(localized: "Created")
    case "updated at": return String(localized: "Updated")
    case "source": return String(localized: "Source")
    case "purpose": return String(localized: "Purpose")
    case "statement": return String(localized: "Statement")
    default: return words.prefix(1).uppercased() + words.dropFirst()
    }
}

/// Role and permission names arrive as code identifiers such as `ManageAccount`.
func approvalFieldValue(_ text: String, key: String) -> String {
    guard ["role", "capabilities", "purpose", "purposes"].contains(key) else { return text }
    return approvalFieldLabel(text)
}

/// Approval requests a screen created and has not seen end, by the change each
/// makes. As on the desktop, that change stays unavailable and asking again shows
/// the same request; a receipt clears only once its request reaches a final state,
/// and one whose request cannot be read is kept.
@MainActor @Observable final class ApprovalReceipts {
    static let finalStates: Set<String> = ["done", "rejected", "cancelled", "expired", "failed", "elsewhere"]
    private var requests: [String: ApprovalView] = [:]
    private var generation: UInt64 = 0

    subscript(key: String) -> ApprovalView? { requests[key] }

    /// The kept request for `key`, or the one `create` makes, kept before it is shown.
    func request(_ key: String, generation: UInt64, create: () async throws -> ApprovalView) async throws -> ApprovalView {
        adopt(generation)
        if let kept = requests[key] { return kept }
        let created = try await create()
        if self.generation == generation { requests[key] = created }
        return created
    }

    /// Re-reads each kept request; `read` gives its state, or nil when it cannot be read.
    func settle(generation: UInt64, read: (String) async -> String?) async {
        adopt(generation)
        for (key, kept) in requests {
            guard let state = await read(kept.id), self.generation == generation, requests[key]?.id == kept.id else { continue }
            if Self.finalStates.contains(state) { requests[key] = nil }
        }
    }

    /// Receipts last while the account's views do: locking or switching accounts
    /// clears them, a trip to the background does not.
    private func adopt(_ generation: UInt64) {
        if generation != self.generation { requests = [:]; self.generation = generation }
    }

    /// `request(_:generation:create:)` through the engine; nil without one or after the views were reset.
    func request(_ key: String, model: SessionModel, create: @MainActor (MobileEngine) async throws -> ObjectSnapshot) async throws -> ApprovalView? {
        guard let engine = model.engine else { return nil }
        let generation = model.viewGeneration
        let request = try await request(key, generation: generation) {
            let created: ApprovalView = try model.decode(try await create(engine))
            return created
        }
        return generation == model.viewGeneration ? request : nil
    }

    func settle(_ model: SessionModel) async {
        guard let engine = model.engine else { return }
        await settle(generation: model.viewGeneration) { id in
            guard let value = try? await engine.approval(id: id), let view: ApprovalView = try? model.decode(value) else { return nil }
            return view.state
        }
    }
}

/// A kept request's notice, with the way back to it.
struct ApprovalReceiptRow: View {
    let receipts: ApprovalReceipts
    let key: String
    let open: (ApprovalView) -> Void
    var body: some View {
        if let request = receipts[key] {
            VStack(alignment: .leading, spacing: 4) {
                Label("Waiting for approval", systemImage: "clock").font(.subheadline).foregroundStyle(.secondary)
                Button("View approval") { open(request) }.buttonStyle(.borderless).frame(minHeight: 44)
                    .accessibilityIdentifier("approval.receipt.\(key)")
            }
        }
    }
}

extension View {
    /// Shows `request`; closing it re-reads the screen's kept requests.
    func approvalDestination(_ request: Binding<ApprovalView?>, receipts: ApprovalReceipts, model: SessionModel) -> some View {
        navigationDestination(isPresented: Binding(get: { request.wrappedValue != nil }, set: {
            if !$0 { request.wrappedValue = nil; Task { await receipts.settle(model) } }
        })) {
            if let shown = request.wrappedValue { ApprovalDetailView(request: shown) }
        }
    }
}
