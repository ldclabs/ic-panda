import SwiftUI
@preconcurrency import DMsgCore

struct ApprovalsView: View {
    var showsDone = false
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var verified: Bool?
    @State private var readID: UUID?
    @State private var selected: ApprovalView?

    var body: some View {
        List {
            if verified == false {
                NoticeBar(text: model.approvals.isEmpty ? String(localized: "The operation could not be completed. Please try again.") : String(localized: "Showing saved content. The latest sync did not finish."))
                Button("Retry") { Task { await reload() } }.accessibilityIdentifier("approvals.retry")
            }
            let waiting = model.approvals.filter { $0.state == "awaiting_user" }
            let processing = model.approvals.filter { $0.state != "awaiting_user" }
            if !waiting.isEmpty {
                Section("Waiting for approval") { rows(waiting.sorted { $0.expiresAt < $1.expiresAt }) }
            }
            if !processing.isEmpty { Section("In progress") { rows(processing) } }
            Section {
                if model.approvals.isEmpty {
                    if verified == true {
                        ContentUnavailableView("All caught up", image: AppGlyph.approvals.assetName, description: Text("Requests that need your decision appear here."))
                            .accessibilityIdentifier("approvals.empty")
                    } else if verified == nil { ProgressView("Syncing…") }
                }
            } footer: {
                Text("Desktop-local command approvals stay on that desktop. Only requests available to this device appear here.")
            }
        }.privateScreen().navigationTitle("Approvals")
            .navigationDestination(isPresented: Binding(get: { selected != nil }, set: { if !$0 { selected = nil } })) {
                if let selected { ApprovalDetailView(request: selected) }
            }
            .toolbar { if showsDone { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } } }
            .task(id: model.contentRevision) { await reload(showCached: true) }
            .refreshable { await reload() }
    }

    @ViewBuilder private func rows(_ requests: [ApprovalView]) -> some View {
        ForEach(requests) { request in
            Button { selected = request } label: {
                HStack(alignment: .top, spacing: 10) {
                    ObjectIcon(glyph: request.type == "device_pair" ? .device : .signature)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(approvalTitle(request.type)).font(.headline)
                        Text(approvalSource(request.source)).font(.subheadline).foregroundStyle(.secondary)
                        if request.state == "awaiting_user" {
                            HStack(spacing: 4) {
                                Text("Request expires")
                                Text(Date(timeIntervalSince1970: Double(request.expiresAt) / 1000), format: .dateTime.month().day().hour().minute())
                            }.font(.footnote).foregroundStyle(.secondary)
                        } else {
                            Text(secretStateTitle(request.state)).font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                    Spacer(minLength: 4)
                    Image(systemName: "chevron.forward").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                        .accessibilityHidden(true)
                }.contentShape(Rectangle())
            }.buttonStyle(.plain).compactObjectRow()
        }
    }

    private func reload(showCached: Bool = false) async {
        let attempt = UUID()
        readID = attempt
        verified = nil
        if showCached { await model.loadApprovals() }
        guard readID == attempt, !Task.isCancelled else { return }
        let success = await model.loadApprovals(refresh: true)
        if readID == attempt && !Task.isCancelled { verified = success }
    }
}

private func approvalSource(_ source: String) -> String {
    if source == "gui" { return "dMsg" }
    if source == "cli" { return "dMsg CLI" }
    if source.hasPrefix("connect:https://") || source.hasPrefix("connect:http://") { return String(source.dropFirst("connect:".count)) }
    return source
}

func approvalTitle(_ type: String) -> String {
    switch type {
    case "wallet": String(localized: "Review wallet action")
    case "delegation": String(localized: "Review agent delegation")
    case "grant": String(localized: "Review agent permissions")
    case "device_pair": String(localized: "Authorize a new device")
    case "device_transfer": String(localized: "Send to a device")
    case "vault_backup": String(localized: "Export encrypted backup")
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
    @State private var readFailed = false
    @State private var readID: UUID?
    @State private var reviewedScope: String?
    @State private var state: String?
    @State private var observedVersion: UInt64?
    @State private var supported = false
    @State private var readyAt = Date.distantFuture
    @State private var decisionTask: Task<Void, Never>?
    @AccessibilityFocusState private var statusFocused: Bool
    #if DEBUG
    @State private var fixtureState: String?
    @State private var fixtureReadFailed = false
    #endif


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

    private func recordField(_ key: String) -> DisplayValue? {
        guard case .object(let fields)? = payload, let value = fields[key], !value.isNull else { return nil }
        return value
    }

    private var envelope: DisplayValue? {
        guard case .object(var fields)? = payload, fields["detail"] != nil else { return nil }
        for key in ["detail", "result", "delivery_report", "state"] { fields.removeValue(forKey: key) }
        return fields.isEmpty ? nil : .object(fields)
    }

    private var summary: String? {
        guard let reviewed else { return nil }
        return approvalSummary(type: request.type, detail: reviewed)
    }

    private var readKey: String {
        let current = model.approvals.first { $0.id == request.id }
        return "\(request.id):\(model.epoch):\(model.concealed):\(current?.state ?? ""):\(current?.updatedAt ?? 0)"
    }

    var body: some View {
        content.sensoryFeedback(.success, trigger: state) { (_: String?, outcome: String?) in outcome == "done" || outcome == "rejected" }
    }

    private var content: some View {
        ScrollViewReader { reader in
            List {
                HStack(alignment: .top, spacing: 12) {
                    ObjectIcon(glyph: .approvals)
                    VStack(alignment: .leading, spacing: 5) {
                        Text(approvalTitle(request.type)).font(.title2.bold())
                        Text(approvalSource(request.source)).font(.subheadline).foregroundStyle(.secondary).textSelection(.enabled)
                            .accessibilityIdentifier("approval.source")
                        if request.risk == "high", state == "awaiting_user" {
                            Label("High risk", systemImage: "exclamationmark.triangle").font(.footnote.weight(.semibold))
                                .foregroundStyle(.orange)
                        }
                        if let state, state != "awaiting_user" {
                            Text(secretStateTitle(state)).font(.headline)
                                .accessibilityIdentifier("approval.status")
                                .accessibilityFocused($statusFocused)
                        }
                    }
                }.listRowBackground(Color.clear).listRowSeparator(.hidden)
                    .reminderVisible(.approval(request.id), watermark: observedVersion, ready: observedVersion != nil)
                    .id("approval.heading")
                if let summary {
                    Text(summary).font(.body.weight(.medium)).fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("approval.summary")
                }
                if state == "awaiting_user", request.type == "device_pair" {
                    NoticeBar(text: String(localized: "This device may receive your private content. The content root must be redistributed after approval."))
                        .listRowBackground(Color.clear)
                }
                if state == "awaiting_user", let recoveryNotice { NoticeBar(text: recoveryNotice).listRowBackground(Color.clear) }
                if state == "awaiting_user", let notice = approvalNotice(request.type) { NoticeBar(text: notice).listRowBackground(Color.clear) }
                if let reviewed, let check = approvalFileCheck(type: request.type, detail: reviewed) {
                    Text(check).font(.subheadline).accessibilityIdentifier("approval.file-check")
                }
                if state == "awaiting_user" && payload != nil && readID == nil && !readFailed && !supported {
                    NoticeBar(text: String(localized: "This request requires a supported desktop client."))
                }
                LabeledContent("Account ID") {
                    Text(model.session?.account ?? "").font(.footnote.monospaced()).textSelection(.enabled)
                        .environment(\.layoutDirection, .leftToRight)
                }
                Section("Review the complete request") {
                    if readFailed {
                        NoticeBar(text: String(localized: "The operation could not be completed. Please try again."))
                        Button("Retry") { Task { await load() } }.accessibilityIdentifier("approval.detail-retry")
                    }
                    if let reviewed { CanonicalDetails(value: reviewed) }
                    else if !readFailed { ProgressView() }
                }
                // Bookkeeping around the request, not part of what is being decided.
                if let envelope { DisclosureGroup("Technical details") { CanonicalDetails(value: envelope) } }
                if let result = recordField("result") { Section("Result") { CanonicalDetails(value: result) } }
                if let delivery = recordField("delivery_report") { Section("Delivery report") { CanonicalDetails(value: delivery) } }
            }.privateScreen().navigationTitle("Review request").navigationBarTitleDisplayMode(.inline)
                .safeAreaInset(edge: .bottom) {
                    if state == "awaiting_user" {
                        TimelineView(.periodic(from: .now, by: 1)) { context in
                            let expired = context.date.timeIntervalSince1970 * 1000 >= Double(request.expiresAt)
                            VStack(spacing: 8) {
                                Text(LocalizedStringKey(working ? "Waiting for confirmation" : expired ? "This request has expired" : "Your decision applies only to this request."))
                                    .font(.footnote).foregroundStyle(.secondary)
                                    .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
                                    .accessibilityIdentifier("approval.decision-hint")
                                if expired {
                                    Button("Refresh") { Task { await load() } }
                                } else { HStack {
                                    Button(role: .destructive) { decision = false } label: {
                                        Text("Reject").frame(maxWidth: .infinity)
                                    }.buttonStyle(.bordered).accessibilityIdentifier("approval.reject")
                                    Button { decision = true } label: {
                                        HStack(spacing: 8) {
                                            if working { ProgressView().controlSize(.small).accessibilityHidden(true) }
                                            Text("Approve")
                                        }.frame(maxWidth: .infinity)
                                    }.buttonStyle(.borderedProminent).frame(maxWidth: .infinity)
                                        .accessibilityIdentifier("approval.approve")
                                        .disabled(expired || context.date < readyAt || !supported)
                                }.controlSize(.large).disabled(payload == nil || working || readID != nil || readFailed) }
                            }
                        }.padding(16).background(.bar)
                    } else if state == "executing" || state == "unknown" {
                        VStack(spacing: 8) {
                            Text("The result is not confirmed. Check it again before retrying.")
                                .font(.footnote).foregroundStyle(.secondary)
                            Button("Refresh") { Task { await reconcile() } }.disabled(working || readID != nil)
                        }.padding(16).background(.bar)
                    }
                }
                .task(id: readKey) { await load() }
                .alert(LocalizedStringKey(decision == true ? "Approve the reviewed request?" : "Reject this request?"),
                    isPresented: Binding(get: { decision != nil }, set: { if !$0 { decision = nil } }), presenting: decision) { approve in
                    Button(LocalizedStringKey(approve ? "Approve this request" : "Reject"), role: approve ? nil : .destructive) {
                        decisionTask = Task { await decide(approve) }
                    }
                    Button("Cancel", role: .cancel) { decision = nil }
                } message: { _ in
                    if let summary { Text(verbatim: approvalSource(request.source) + "\n" + summary) }
                    else { Text(verbatim: approvalTitle(request.type) + "\n" + approvalSource(request.source)) }
                }
                .task(id: state == "awaiting_user" ? request.expiresAt : nil) {
                    guard state == "awaiting_user" else { return }
                    let remaining = Date(timeIntervalSince1970: Double(request.expiresAt) / 1000).timeIntervalSinceNow
                    if remaining > 0 { try? await Task.sleep(for: .seconds(remaining)) }
                    guard !Task.isCancelled, !working, !model.concealed else { return }
                    decision = nil
                }
                .onDisappear { decisionTask?.cancel() }
                .onChange(of: state) { previous, next in
                    if previous != nil, let next, next != "awaiting_user" {
                        reader.scrollTo("approval.heading", anchor: .top)
                        statusFocused = true
                    }
                }
        }
    }

    private func load() async {
        guard !working else { return }
        let attempt = UUID()
        readID = attempt
        defer { if readID == attempt { readID = nil } }
        let epoch = model.epoch
        let scope = "\(request.id):\(request.digest):\(epoch)"
        if reviewedScope != scope || model.concealed { state = nil; payload = nil }
        reviewedScope = scope
        supported = false; decision = nil; readFailed = false; observedVersion = nil; readyAt = .distantFuture
        guard !model.concealed else { return }
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-approval-detail-retry"), !fixtureReadFailed {
                fixtureReadFailed = true; readFailed = true; return
            }
            if ProcessInfo.processInfo.arguments.contains("--ui-approval-result-reread-failure"), fixtureState != nil {
                readFailed = true; return
            }
            state = fixtureState ?? request.state
            if state == "awaiting_user" && request.expiresAt <= NotificationTime.now { state = "expired" }
            supported = request.mobileSupported == true; readyAt = .now
            if request.id == "web-review" {
                payload = .object(["detail": .object(["method": .string("authenticate"), "origin": .string("https://studio.example.test"),
                    "content": .object(["purpose": .string("Login"), "receiver": .string("Studio notes"),
                        "expires_at": .number(Decimal(request.expiresAt + 3_600_000))])])])
                return
            }
            if request.type == "attest" {
                payload = .object(["detail": .object(["statement": .object(["profile": .string("text"),
                    "content": .object(["text": .string("Publish the reviewed release notes")])])])])
                return
            }
            if request.type == "grant", let grant = model.fixtureGrantReview { payload = .object(["detail": grant]); return }
            payload = .object(["detail": .object([
                "capabilities": .array([.string("ContentSign"), .string("VaultUnlock"), .string("RootManage")]),
                "device_id": .string(String(repeating: "a", count: 64)), "role": .string("member")
            ])])
            return
        }
        #endif
        do {
            guard let engine = model.engine else { throw MobileError.Failure(code: "UNAVAILABLE") }
            let snapshot = try await engine.approval(id: request.id)
            guard readID == attempt, epoch == model.epoch, !Task.isCancelled, !model.concealed else { return }
            let fresh: ApprovalView = try model.decode(snapshot)
            guard fresh.digest == request.digest else { throw CocoaError(.coderInvalidValue) }
            let reviewed: DisplayValue = try model.decode(snapshot)
            state = fresh.state
            observedVersion = fresh.updatedAt
            supported = fresh.mobileSupported == true
            payload = reviewed; readyAt = .now.addingTimeInterval(1)
        } catch {
            if readID == attempt, epoch == model.epoch, !Task.isCancelled, !model.concealed {
                readFailed = true; decision = nil; model.reportBackground(error)
            }
        }
    }

    private func reconcile() async {
        guard !working, readID == nil else { return }
        working = true; model.activity()
        let epoch = model.epoch
        do {
            if let engine = model.engine { _ = try await coreOperation { try await engine.reconcile(operation: $0) } }
            #if DEBUG
            if model.uiTesting { fixtureState = "done"; model.approvals.removeAll { $0.id == request.id } }
            #endif
        } catch { if epoch == model.epoch { model.report(error) } }
        working = false
        if epoch == model.epoch && !model.concealed { await load(); await model.loadApprovals() }
    }

    private func decide(_ approve: Bool) async {
        guard model.engine != nil || model.uiTesting, !working, readID == nil, !readFailed, !model.concealed, state == "awaiting_user",
              request.expiresAt > NotificationTime.now, !approve || (supported && Date.now >= readyAt) else { return }
        decision = nil; working = true; model.activity()
        let epoch = model.epoch
        defer { working = false }
        do {
            let value = try await decideRequest(approve)
            guard epoch == model.epoch else { return }
            if case .object(let fields) = value, case .string(let outcome)? = fields["state"] {
                state = outcome
                if fields["detail"] != nil { payload = value }
            }
            await model.updateSession(); await model.loadApprovals()
        } catch { if epoch == model.epoch { supported = false; state = nil; model.report(error) } }
        working = false
        if epoch == model.epoch && !model.concealed { await load() }
    }

    private func decideRequest(_ approve: Bool) async throws -> DisplayValue {
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-slow-approval") {
                model.fixtureApprovalInFlight = true
                defer { model.fixtureApprovalInFlight = false }
                model.inactive()
                // The OS has dismissed its prompt while the account operation
                // continues. Reuse the real foreground lifecycle path.
                let foreground = Task { await model.foreground() }
                defer { foreground.cancel() }
                try await Task.sleep(for: .seconds(6))
                if ProcessInfo.processInfo.arguments.contains("--ui-approval-failure") {
                    fixtureState = "unknown"
                    throw MobileError.Failure(code: "EXECUTION_UNKNOWN")
                }
            }
            let outcome = approve ? "done" : "rejected"
            fixtureState = outcome
            model.approvals.removeAll { $0.id == request.id }
            return .object(["state": .string(outcome)])
        }
        #endif
        guard let engine = model.engine else { throw MobileError.Failure(code: "LOCKED") }
        let snapshot = try await coreOperation { try await engine.decide(id: request.id, digest: request.digest, approve: approve, operation: $0) }
        return try model.decode(snapshot)
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

private func reviewedStatement(type: String, fields: [String: DisplayValue]) -> [String: DisplayValue]? {
    let statement: DisplayValue?
    switch type {
    case "attest": statement = fields["statement"]
    case "connect":
        guard case .object(let content)? = fields["content"] else { return nil }
        switch fields["method"]?.text {
        case "signDocument": statement = content["statement"]
        case "signAction": statement = fields["content"]
        default: return nil
        }
    default: return nil
    }
    guard case .object(let value)? = statement else { return nil }
    return value
}

func approvalFileCheck(type: String, detail: DisplayValue) -> String? {
    guard case .object(let fields) = detail, let statement = reviewedStatement(type: type, fields: fields),
          case .object(let content)? = statement["content"], case .string(let digest)? = content["sha256"], !digest.isEmpty else { return nil }
    if case .boolean(true)? = fields["file_checked"] { return String(localized: "File digest checked") }
    if case .object? = fields["file"] { return String(localized: "The file changed or cannot be read") }
    return String(localized: "Only the digest is confirmed; the file was not checked")
}

/// Summarize only the verified action. The complete payload remains available below it.
func approvalSummary(type: String, detail: DisplayValue) -> String? {
    guard case .object(let fields) = detail else { return nil }
    func string(_ key: String) -> String? {
        guard case .string(let value)? = fields[key], !value.isEmpty else { return nil }
        return value
    }
    switch type {
    case "attest", "connect":
        var lines = [string("app_id"), string("origin")].compactMap { $0 }
        if let statement = reviewedStatement(type: type, fields: fields) {
            if case .string(let subject)? = statement["subject"], !subject.isEmpty { lines.append(subject) }
            if case .object(let file)? = fields["file"], case .string(let name)? = file["name"] { lines.append(name) }
            if case .object(let content)? = statement["content"] {
                if case .string(let text)? = content["text"], !text.isEmpty {
                    lines.append(text.count > 200 ? String(text.prefix(200)) + "…" : text)
                }
                if case .string(let digest)? = content["sha256"] { lines.append("SHA-256 · " + digest) }
                if case .string(let app)? = content["app_id"], case .string(let command)? = content["command"] { lines.append(app + " · " + command) }
            }
        } else if type == "connect", string("method") == "authenticate", case .object(let content)? = fields["content"] {
            if case .string(let purpose)? = content["purpose"] {
                let action = switch purpose {
                case "Login": String(localized: "Sign in")
                case "Link": String(localized: "Link to an existing account")
                case "Reauthenticate": String(localized: "Confirm it is still you")
                default: purpose
                }
                lines.insert(action, at: 0)
            }
            if case .string(let receiver)? = content["receiver"] { lines.append(String(localized: "Receiver") + " · " + receiver) }
            if case .number(let milliseconds)? = content["expires_at"] {
                let date = Date(timeIntervalSince1970: NSDecimalNumber(decimal: milliseconds).doubleValue / 1000)
                lines.append(String(localized: "Valid until") + " · " + date.formatted())
            }
        } else if let method = string("method") { lines.append(method) }
        return lines.isEmpty ? nil : lines.joined(separator: "\n")
    case "vault_backup":
        let items = fields["items"]?.text ?? ""
        let files = fields["files"]?.text ?? ""
        let omitted = fields["omitted"]?.text ?? ""
        var lines = [String(localized: "Items") + " · " + items, String(localized: "Attachments") + " · " + files,
                     String(localized: "Omitted attachments") + " · " + omitted]
        if case .boolean(true)? = fields["wallet"] { lines.append(String(localized: "Includes secrets saved in wallet items.")) }
        return lines.joined(separator: "\n")
    case "device_pair":
        guard let device = string("device_id"), let role = string("role") else { return nil }
        let fingerprint = device.count > 20 ? String(device.prefix(8)) + "…" + String(device.suffix(8)) : device
        var target = String(localized: "Authorize \(fingerprint) as \(deviceRoleTitle(role)).")
        if case .boolean(true)? = fields["succession"] { target = String(localized: "Replace a device") + "\n" + target }
        guard case .array(let values)? = fields["capabilities"] else { return target }
        let permissions = values.map { deviceCapabilityTitle($0.text) }.formatted(.list(type: .and))
        return target + (permissions.isEmpty ? "" : "\n" + permissions)
    case "secret_share":
        guard let title = string("title") else { return nil }
        let recipient: String
        if case .object(let channel)? = fields["channel"], case .string(let id)? = channel["channel_id"] {
            if case .string(let name)? = channel["name"], !name.isEmpty { recipient = name + " · " + id }
            else { recipient = id }
        }
        else if fields["channel"]?.isNull != false, case .boolean(let protected)? = fields["password"] {
            recipient = String(localized: protected ? "Anyone with the link and password" : "Anyone with the link")
        }
        else { return nil }
        var lines = [String(localized: "Share \(title) with \(recipient).")]
        if case .array(let selected)? = fields["fields"] {
            for case .object(let field) in selected {
                guard case .string(let id)? = field["id"], case .string(let label)? = field["label"] else { continue }
                let name = label.isEmpty ? vaultFieldName(id) : label
                if case .string(let section)? = field["section"], !section.isEmpty { lines.append(section + " · " + name) }
                else { lines.append(name) }
            }
        }
        if case .object(let input)? = fields["input"], case .boolean(true)? = input["include_notes"] {
            lines.append(String(localized: "Notes"))
        }
        if case .array(let attachments)? = fields["attachments"] {
            lines += attachments.compactMap { file in
                guard case .object(let metadata) = file, case .string(let name)? = metadata["name"] else { return nil }
                return name
            }
        }
        if case .number(let expiry)? = fields["expires_at"], case .number(let sessions)? = fields["max_download_sessions"] {
            lines.append(String(localized: "Expires") + " · " + Date(timeIntervalSince1970: NSDecimalNumber(decimal: expiry).doubleValue / 1000).formatted())
            lines.append(String(localized: "Download sessions") + " · " + sessions.formatted())
        }
        return lines.joined(separator: "\n")
    case "grant":
        guard let grantee = string("grantee"), case .array(let capabilities)? = fields["capabilities"] else { return nil }
        var parts = [grantee] + capabilities.map { AgentPermissionName.describe($0.text) }
        if let days = fields["days"].flatMap({ Int($0.text) }) { parts.append(String(localized: "\(days) days")) }
        return parts.joined(separator: "\n")
    case "wallet":
        guard let action = string("action"), let name = string("name") ?? string("wallet") else { return nil }
        let collectOnly: Bool? = if case .boolean(let value)? = fields["collect_only"] { value } else { nil }
        guard let prefix = walletSummaryAction(action, decision: string("decision"), collectOnly: collectOnly) else { return nil }
        var parts = [prefix, name]
        if let wallet = string("wallet"), wallet != name { parts.append(wallet) }
        if action == "create", let price = string("price_e8s"), let fee = string("ledger_fee_e8s") {
            parts += [AtomicAmount.display(price, decimals: 8) + " ICP",
                      String(localized: "Fee") + " " + AtomicAmount.display(fee, decimals: 8) + " ICP"]
        }
        if case .object(let proposal)? = fields["proposal"], let id = proposal["id"], id.isScalar { parts.append(String(localized: "Proposal") + " " + id.text) }
        return parts.joined(separator: " · ")
    default: return nil
    }
}

private func walletSummaryAction(_ action: String, decision: String?, collectOnly: Bool?) -> String? {
    switch action {
    case "create": String(localized: "Create wallet")
    case "join": String(localized: "Review joining wallet")
    case "unbind": String(localized: "Remove device binding")
    case "cosign" where collectOnly == true: String(localized: "Cosign on this device")
    case "cosign" where collectOnly == false:
        String(localized: "Cosign on this device") + " · " + String(localized: "Execute with collected signatures")
    case "decide" where decision == "approve": String(localized: "Review approval")
    case "decide" where decision == "reject": String(localized: "Review rejection")
    case "decide" where decision == "veto": String(localized: "Veto proposal")
    default: nil
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
                if !row.label.isEmpty { Text(row.label).font(.footnote).foregroundStyle(.secondary) }
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
    case "key id": return String(localized: "Key fingerprint")
    case "this device": return String(localized: "This device")
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
    switch key {
    case "capabilities":
        let title = deviceCapabilityTitle(text)
        if text.contains(".") { return text }
        return title == text ? approvalFieldLabel(text) : title
    case "role":
        let title = deviceRoleTitle(text)
        return title == text ? approvalFieldLabel(text) : title
    case "purpose", "purposes": return approvalFieldLabel(text)
    default: return text
    }
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
