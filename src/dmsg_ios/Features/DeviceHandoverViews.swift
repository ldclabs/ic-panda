import SwiftUI
import VisionKit
import AVFoundation
@preconcurrency import DMsgCore

/// What only this device holds: file versions without a cloud backup,
/// changes not yet synced and the last successful sync.
struct LocalOnly: Decodable {
    let versions: Int
    let bytes: Int64
    let unbackedVersions: Int
    let unbackedBytes: Int64
    let pendingSync: Int
    let lastSyncOkAt: UInt64?

    /// Versions without a backup, unsynced changes, or no successful sync for a week.
    var atRisk: Bool {
        guard let last = lastSyncOkAt else { return true }
        return unbackedVersions > 0 || pendingSync > 0 || Date.now.timeIntervalSince1970 * 1000 - Double(last) > 7 * 86_400_000
    }

    @MainActor static func load(_ model: SessionModel) async -> LocalOnly? {
        guard let engine = model.engine else { return nil }
        return try? model.decode(try await engine.localOnly())
    }

    static func size(_ bytes: Int64) -> String { ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file) }
}

/// The figures of what only this device holds, where giving it up would
/// lose them; `always` shows them even when nothing is at risk.
struct LocalOnlySection: View {
    let value: LocalOnly?
    var always = false

    var body: some View {
        if let value, value.atRisk || always {
            Section {
                LabeledContent("File versions", value: "\(value.versions) · \(LocalOnly.size(value.bytes))")
                LabeledContent("Without a cloud backup", value: "\(value.unbackedVersions) · \(LocalOnly.size(value.unbackedBytes))")
                LabeledContent("Changes not yet synced", value: String(value.pendingSync))
                LabeledContent("Last successful sync") {
                    if let last = value.lastSyncOkAt { Text(Date(timeIntervalSince1970: TimeInterval(last) / 1000), style: .date) }
                    else { Text("Never") }
                }
            } header: { Text("Only on this device") } footer: {
                if value.atRisk { Text("Revoking a device loses whatever exists only on it, such as files never synced or backed up.") }
            }.accessibilityIdentifier("localOnly")
        }
    }
}

/// One line warning that some files exist only on this device.
struct LocalOnlyWarning: View {
    let value: LocalOnly?

    var body: some View {
        if let value, value.atRisk {
            Label("Some files exist only on this iPhone, without a cloud backup or a recent sync.", systemImage: "exclamationmark.triangle")
                .font(.footnote).foregroundStyle(.orange).accessibilityIdentifier("localOnly.warning")
        }
    }
}

/// One device's record of a handover, as the core shows it.
struct Succession: Decodable, Identifiable {
    struct Step: Decodable, Identifiable {
        let step: String
        let settled: Bool
        let evidence: DisplayValue?
        let skipped: [String]
        var id: String { step }
    }
    enum State { case done, left, ready, waiting }

    let id: String
    let side: String
    let account: String
    let predecessor: String
    let successor: String
    let steps: [Step]
    let available: [String]
    let cancelledAt: UInt64?
    let createdAt: UInt64

    /// A step as this device sees it: settled without evidence was left behind.
    func state(_ step: Step) -> State {
        if step.settled { return step.evidence == nil ? .left : .done }
        return cancelledAt == nil && available.contains(step.step) ? .ready : .waiting
    }

    var finished: Bool { side == "successor" && steps.allSatisfy(\.settled) }

    /// What was left behind, as the revocation lists it.
    var leftBehind: [String] {
        steps.filter { state($0) == .left }.flatMap { step in step.skipped.isEmpty ? [Self.title(step.step)] : Self.items(step.skipped) }
    }

    static func items(_ skipped: [String]) -> [String] { skipped.map { $0 == "library" ? String(localized: "Files") : $0 } }

    static func title(_ step: String) -> String {
        switch step {
        case "authorize": String(localized: "Approve the new device")
        case "approval_key": String(localized: "Approval key")
        case "cosigners": String(localized: "Cosigner keys")
        case "content": String(localized: "Local content")
        case "wallets": String(localized: "Wallet bindings")
        case "native_addresses": String(localized: "Native addresses")
        case "revoke": String(localized: "Revoke the old device")
        default: step
        }
    }

    static func hint(_ step: String) -> String {
        switch step {
        case "authorize": String(localized: "Approving enrolls the new device and moves the content root to it.")
        case "approval_key": String(localized: "Registers the new device's approval key.")
        case "cosigners": String(localized: "The old device moves its wallet cosigner keys to the new device's key; the new device confirms once that is done.")
        case "content": String(localized: "Move the file library over the local network, import it from a copied workspace, or leave it behind.")
        case "wallets": String(localized: "The new device takes over the old device's wallet bindings, one approval per wallet.")
        case "native_addresses": String(localized: "Move each native Taproot address to this device's key, or leave them for a later sweep through L2.")
        default: String(localized: "Revokes the old device once nothing is left behind unnoticed.")
        }
    }
}

/// A workspace copied here from another iPhone and set aside.
struct MovedCopy: Decodable, Identifiable {
    let id: String
    let account: String?
    let deviceId: String
    let movedAt: UInt64
}

/// Handing a device over to a new one. The old device reviews the new
/// device's request and approves it; then each side runs its steps in
/// order. Steps that ask create approval requests; local content moves with
/// a device transfer or an import from a copied workspace, and it and
/// native addresses can be left behind, which the revocation lists.
struct SuccessionsView: View {
    private struct Leaving { let task: Succession; let step: String; let items: [String] }
    private struct Address: Decodable, Identifiable { let wallet: String; let from: UInt32; var id: String { "\(wallet)#\(from)" } }
    private struct Outcome: Decodable { let kind: String?; let requests: [ApprovalView]?; let addresses: [Address]?; let rotations: [DisplayValue]? }

    @Environment(SessionModel.self) private var model
    @State private var tasks: [Succession] = []
    @State private var copies: [MovedCopy] = []
    @State private var found: [String: [Address]] = [:]
    @State private var loaded = false
    @State private var working = false
    @State private var notice: String?
    @State private var request: ApprovalView?
    @State private var leaving: Leaving?
    @State private var stopping: Succession?
    @State private var revoking: Succession?

    var body: some View {
        List {
            Section {
                Text("Moving to a new device: the new device asks to replace an old one, the old device approves, then both hand over step by step.")
                    .font(.footnote).foregroundStyle(.secondary)
                NavigationLink("Review a request") { SuccessionReviewView() }.accessibilityIdentifier("succession.review")
            }
            if let notice { NoticeBar(text: notice).listRowBackground(Color.clear) }
            ForEach(tasks) { task in section(task) }
            if loaded && tasks.isEmpty { Text("No handovers on this device.").foregroundStyle(.secondary) }
        }.privateScreen().navigationTitle("Replace a device").navigationBarTitleDisplayMode(.inline)
            .task(id: model.session?.generation) { await load() }
            .refreshable { await load() }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
            .confirmationDialog("Leave this step behind?", isPresented: Binding(get: { leaving != nil }, set: { if !$0 { leaving = nil } }), titleVisibility: .visible) {
                Button("Leave behind", role: .destructive) { if let leaving { Task { await skip(leaving) } } }
                Button("Cancel", role: .cancel) { leaving = nil }
            } message: {
                Text(LocalizedStringKey(leaving?.step == "content"
                    ? "The file library stays only on the old device. Whatever was never synced or backed up there is lost once it is revoked."
                    : "These native addresses keep needing the old device for L1. The team can still sweep them through L2 once each output is old enough."))
            }
            .confirmationDialog("Stop handover", isPresented: Binding(get: { stopping != nil }, set: { if !$0 { stopping = nil } }), titleVisibility: .visible) {
                Button("Stop handover", role: .destructive) { if let stopping { Task { await cancel(stopping) } } }
                Button("Cancel", role: .cancel) { stopping = nil }
            } message: { Text("Steps already taken stay. To undo the new device's enrollment, revoke it.") }
            .confirmationDialog("Revoke the old device", isPresented: Binding(get: { revoking != nil }, set: { if !$0 { revoking = nil } }), titleVisibility: .visible) {
                Button("Revoke the old device", role: .destructive) { if let revoking { Task { await run(revoking, "revoke") } } }
                Button("Cancel", role: .cancel) { revoking = nil }
            } message: {
                let text = String(localized: "The old device stops working for this account, and anything only on it is lost.")
                let left = revoking?.leftBehind ?? []
                Text(left.isEmpty ? text : text + "\n\n" + String(localized: "Left behind") + ": " + left.joined(separator: ", "))
            }
    }

    /// Each step is a row, and the actions of a step ready here are rows of their own below it.
    @ViewBuilder private func section(_ task: Succession) -> some View {
        Section {
            ForEach(task.steps) { step in
                row(task, step)
                if task.state(step) == .ready { actions(task, step.step) }
            }
            if task.finished {
                Label("Handover complete. Consider registering an approval key and a cosigner key on a second device as well.", systemImage: "checkmark.circle")
                    .font(.footnote).foregroundStyle(.green)
            } else if task.cancelledAt == nil {
                Button("Stop handover", role: .destructive) { stopping = task }.disabled(working)
            }
        } header: {
            VStack(alignment: .leading, spacing: 2) {
                Text(LocalizedStringKey(task.side == "successor" ? "This device replaces" : "This device is replaced by"))
                Text(verbatim: String((task.side == "successor" ? task.predecessor : task.successor).prefix(16))).font(.caption.monospaced())
                if task.cancelledAt != nil { Text("Stopped").foregroundStyle(.orange) }
            }
        }
    }

    @ViewBuilder private func row(_ task: Succession, _ step: Succession.Step) -> some View {
        let state = task.state(step)
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                Image(systemName: state == .done ? "checkmark.circle.fill" : state == .left ? "arrow.uturn.left.circle" : state == .ready ? "arrow.right.circle" : "circle.dotted")
                    .foregroundStyle(state == .done ? Color.green : state == .left ? Color.orange : state == .ready ? Theme.forest : Color.secondary)
                    .accessibilityHidden(true)
                Text(Succession.title(step.step)).font(.subheadline)
                Spacer()
                Text(LocalizedStringKey(state == .done ? "Done" : state == .left ? "Left behind" : state == .ready ? "Ready" : "Waiting"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            if state == .ready {
                Text(Succession.hint(step.step)).font(.footnote).foregroundStyle(.secondary)
            } else if state == .left && !step.skipped.isEmpty {
                Text(Succession.items(step.skipped).joined(separator: " · ")).font(.caption).foregroundStyle(.secondary)
            }
        }.padding(.vertical, 4).accessibilityElement(children: .contain).accessibilityIdentifier("succession.step.\(step.step)")
    }

    @ViewBuilder private func actions(_ task: Succession, _ step: String) -> some View {
        switch step {
        case "content":
            NavigationLink(LocalizedStringKey(task.side == "successor" ? "Receive here" : "Send to a device")) {
                DeviceTransferView(succession: task)
            }
            if task.side == "successor" {
                ForEach(copies.filter { $0.deviceId == task.predecessor && $0.account == task.account }) { copy in
                    Button("Import from copy") { Task { await importCopy(copy, into: task) } }.disabled(working)
                }
            }
            Button("Leave behind") { leaving = Leaving(task: task, step: step, items: ["library"]) }.disabled(working)
        case "native_addresses":
            Button("Check addresses") { Task { await run(task, step) } }.disabled(working)
            ForEach(found[task.id] ?? []) { address in
                NavigationLink { WalletNativeMoveView(wallet: address.wallet, from: address.from) } label: {
                    LabeledContent { Text("Move to this device") } label: {
                        Text(verbatim: "\(address.wallet.prefix(11))… #\(address.from)").font(.caption.monospaced())
                    }
                }
            }
            Button("Leave behind") {
                leaving = Leaving(task: task, step: step, items: (found[task.id] ?? []).map { "\($0.wallet) #\($0.from)" })
            }.disabled(working)
        case "revoke":
            Button("Revoke the old device", role: .destructive) { revoking = task }.disabled(working)
        default:
            Button(LocalizedStringKey(step == "authorize" ? "Approve" : step == "cosigners" && task.side == "successor" ? "Confirm it is done" : "Run")) {
                Task { await run(task, step) }
            }.disabled(working)
        }
    }

    private func load() async {
        guard let engine = model.engine else { return }
        do {
            let list: [Succession] = try model.decode(try await engine.successions())
            tasks = list.reversed()
            copies = (try? model.decode(try await engine.movedCopies())) ?? []
        } catch { model.report(error) }
        loaded = true
    }

    /// Run `step` here: an approval to decide, wallet approvals, addresses to move, or the step recorded.
    private func run(_ task: Succession, _ step: String) async {
        guard let engine = model.engine, !working else { return }
        working = true; notice = nil; revoking = nil
        defer { working = false }
        do {
            let result = try await engine.successionRun(id: task.id, step: step)
            let outcome: Outcome = try model.decode(result)
            if outcome.kind != nil { request = try model.decode(result) }
            else if let requests = outcome.requests {
                notice = String(localized: "Wallet approvals are waiting in Approvals.")
                request = requests.first
            } else if let addresses = outcome.addresses { found[task.id] = addresses }
            else if outcome.rotations != nil { notice = String(localized: "Some wallets are not done yet. Run the step again to finish.") }
        } catch { model.report(error) }
        await load()
    }

    private func skip(_ chosen: Leaving) async {
        guard let engine = model.engine, !working else { return }
        working = true; leaving = nil
        defer { working = false }
        do { _ = try await engine.successionSkip(id: chosen.task.id, step: chosen.step, skipped: chosen.items) }
        catch { model.report(error) }
        await load()
    }

    private func cancel(_ task: Succession) async {
        guard let engine = model.engine, !working else { return }
        working = true; stopping = nil
        defer { working = false }
        do { _ = try await engine.successionCancel(id: task.id) }
        catch { model.report(error) }
        await load()
    }

    private func importCopy(_ copy: MovedCopy, into task: Succession) async {
        guard let engine = model.engine, !working else { return }
        working = true; notice = nil
        defer { working = false }
        do { _ = try await engine.movedImport(id: copy.id, succession: task.id); notice = String(localized: "Files imported.") }
        catch { model.report(error) }
        await load()
    }
}

/// The old device reviews a new device's request: the device, its role and
/// permissions, the keys made there and the team wallets the handover
/// covers, then approves it once the person confirms its device ID.
struct SuccessionReviewView: View {
    private struct Review: Decodable {
        struct Wallet: Decodable { let wallet: String; let name: String? }
        let deviceId: String
        let role: String
        let capabilities: [String]
        let approvalKey: Bool
        let cosigner: Bool
        let expiresAt: UInt64
        let wallets: [Wallet]
    }

    @Environment(SessionModel.self) private var model
    @State private var token = ""
    @State private var shown = ""
    @State private var review: Review?
    @State private var local: LocalOnly?
    @State private var scanning = false
    @State private var working = false
    @State private var created: ApprovalView?

    private var device: String { shown.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }

    var body: some View {
        Form {
            Text("Paste the request the new device shows, check what it asks for, then confirm its device ID.")
                .font(.footnote).foregroundStyle(.secondary)
            Section("Request from the new device") {
                TextField("Request from the new device", text: $token, axis: .vertical).lineLimit(3...6)
                    .textInputAutocapitalization(.never).autocorrectionDisabled().onChange(of: token) { review = nil }
                    .accessibilityIdentifier("succession.token")
                if DataScannerViewController.isSupported {
                    Button("Scan device request", systemImage: "qrcode.viewfinder") { Task {
                        if await AVCaptureDevice.requestAccess(for: .video) { scanning = true }
                    } }
                }
                Button("Check request") { Task { await check() } }.disabled(token.isEmpty || working)
            }
            if let review {
                Section("Device") {
                    FingerprintRow(title: "Device ID", value: review.deviceId)
                    LabeledContent("Role", value: review.role)
                    LabeledContent("Permissions", value: review.capabilities.joined(separator: ", "))
                    LabeledContent("Approval key", value: review.approvalKey ? String(localized: "made") : String(localized: "not available"))
                    LabeledContent("Cosigner key", value: review.cosigner ? String(localized: "made") : String(localized: "not available"))
                    LabeledContent("Team wallets", value: review.wallets.isEmpty ? "—" : review.wallets.map { $0.name ?? $0.wallet }.joined(separator: " · "))
                    LabeledContent("Request expires") { Text(Date(timeIntervalSince1970: TimeInterval(review.expiresAt) / 1000), style: .relative) }
                }
                LocalOnlySection(value: local, always: true)
                Section {
                    TextField("Device ID", text: $shown, axis: .vertical).font(.body.monospaced())
                        .textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("succession.device")
                    if device.count == 64 && device != review.deviceId {
                        Text("This is not the device in the request.").font(.footnote).foregroundStyle(.red)
                    }
                    Button("Approve replacement") { Task { await approve() } }.disabled(device != review.deviceId || working)
                } header: { Text("Device ID shown on the new device") } footer: {
                    Text("Once approved, the new device joins with this device's role and the content root moves to it; this device then only finishes the handover.")
                }
            }
        }.privateScreen().navigationTitle("Review a request").navigationBarTitleDisplayMode(.inline)
            .task(id: model.session?.generation) { local = await LocalOnly.load(model) }
            .sheet(isPresented: $scanning) { PairingScanner { text in token = text; scanning = false; Task { await check() } } }
            .navigationDestination(isPresented: Binding(get: { created != nil }, set: { if !$0 { created = nil } })) {
                if let created { ApprovalDetailView(request: created) }
            }
    }

    private func check() async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do { review = try model.decode(try await engine.successionReview(token: token.trimmingCharacters(in: .whitespacesAndNewlines))) }
        catch { review = nil; model.report(error) }
    }

    private func approve() async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do { created = try model.decode(try await engine.successionApprove(token: token.trimmingCharacters(in: .whitespacesAndNewlines), device: device)) }
        catch { model.report(error) }
    }
}

/// A new iPhone not yet joined asks to replace an administrator device; the
/// old device reviews the request and approves this iPhone in its place.
struct SuccessionRequestView: View {
    private struct Made: Decodable { let token: String; let deviceId: String; let approvalKey: Bool; let cosigner: Bool }

    @Environment(SessionModel.self) private var model
    @State private var predecessor = ""
    @State private var made: Made?
    @State private var image: UIImage?
    @State private var working = false

    private var device: String { predecessor.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }

    var body: some View {
        Form {
            Text("Moving from an old device? This device takes over its role, keys and wallets, then the old device is revoked.")
                .font(.footnote).foregroundStyle(.secondary)
            Section {
                TextField("ID of the device to replace", text: $predecessor, axis: .vertical).font(.body.monospaced())
                    .textInputAutocapitalization(.never).autocorrectionDisabled().onChange(of: predecessor) { made = nil; image = nil }
                    .accessibilityIdentifier("succession.predecessor")
                Button("Create request") { Task { await create() } }
                    .disabled(working || made != nil || device.count != 64 || !device.allSatisfy(\.isHexDigit))
                    .accessibilityIdentifier("succession.create")
            } header: { Text("ID of the device to replace") } footer: {
                Text("Shown in Devices on that device, which must be an administrator.")
            }
            if let made {
                Section {
                    if let image {
                        Image(uiImage: image).interpolation(.none).resizable().scaledToFit().frame(maxWidth: 240).frame(maxWidth: .infinity)
                            .accessibilityLabel("Device replacement request QR code")
                    }
                    Text("On the device being replaced, open Devices → Replace a device → Review a request, scan or paste this request and enter this device's ID. Then check authorization here.")
                        .font(.footnote).foregroundStyle(.secondary)
                    FingerprintRow(title: "Device ID", value: made.deviceId)
                    LabeledContent("Approval key", value: made.approvalKey ? String(localized: "made") : String(localized: "not available"))
                    LabeledContent("Cosigner key", value: made.cosigner ? String(localized: "made") : String(localized: "not available"))
                    ShareLink(item: made.token) { Label("Share request", systemImage: "square.and.arrow.up") }
                    Button("Copy request") { model.clipboard.copy(made.token) }
                    Button("Check authorization") { Task { await refresh() } }.disabled(working)
                }
            }
        }.privateScreen().navigationTitle("Replace a device").navigationBarTitleDisplayMode(.inline)
    }

    private func create() async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do {
            let next: Made = try model.decode(try await engine.successionRequest(predecessor: device))
            made = next; image = QRCode.image(next.token)
        } catch { model.report(error) }
    }

    private func refresh() async {
        working = true; defer { working = false }
        do { _ = try await model.engine?.refreshAccount(); await model.updateSession() }
        catch { model.report(error) }
    }
}

/// The file library moving between two devices of the account on the same
/// network: this iPhone receives and shows the session code the sender
/// enters, or sends to a receiving session. For a handover, the transfer
/// goes to its other device and settles its content step.
struct DeviceTransferView: View {
    var succession: Succession?
    @Environment(SessionModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var session = ""
    @State private var to = ""
    @State private var address = ""

    private var sends: Bool { succession?.side == "predecessor" }
    private var sessionShown: String { session.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
    private var toShown: String { to.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
    private var canSend: Bool {
        sessionShown.count == 32 && sessionShown.allSatisfy(\.isHexDigit)
            && (succession != nil || toShown.count == 64 && toShown.allSatisfy(\.isHexDigit))
            && (address.isEmpty || DeviceTransferModel.parse(address) != nil)
    }

    var body: some View {
        let transfer = model.deviceTransfer
        Form {
            Text("Moves the file library to another device of this account on the same network, encrypted end to end. Keep both devices unlocked until it finishes.")
                .font(.footnote).foregroundStyle(.secondary)
            if let status = transfer.status { statusSection(status, transfer: transfer) }
            if transfer.localNetworkDenied {
                Section {
                    NoticeBar(text: String(localized: "Local network access is off for dMsg. Turn it on in Settings, or enter the other device's address."))
                    Button("Open Settings") { if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) } }
                }
            }
            if transfer.bonjourFailed {
                NoticeBar(text: String(localized: "The other device was not found on the local network. Enter its address."))
            }
            if !transfer.running {
                if !sends {
                    Button("Receive here", systemImage: "square.and.arrow.down") { Task { await transfer.receive(model, succession: succession?.id) } }
                        .accessibilityIdentifier("transfer.receive")
                }
                if succession == nil || sends {
                    Section {
                        TextField("Session code", text: $session).font(.body.monospaced())
                            .textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("transfer.session")
                        if succession == nil {
                            TextField("Receiving device ID", text: $to, axis: .vertical).font(.body.monospaced())
                                .textInputAutocapitalization(.never).autocorrectionDisabled()
                        }
                        TextField("Address", text: $address).font(.body.monospaced()).keyboardType(.URL)
                            .textInputAutocapitalization(.never).autocorrectionDisabled()
                        Text("Optional. Only when the other device is not found automatically, for example 192.168.1.20:52000.")
                            .font(.footnote).foregroundStyle(.secondary)
                        Button("Start sending") { Task {
                            await transfer.send(model, session: sessionShown, to: succession == nil ? toShown : nil,
                                                address: address.isEmpty ? nil : DeviceTransferModel.parse(address), succession: succession?.id)
                        } }.disabled(!canSend)
                    } header: { Text("Send to a device") } footer: { Text("Enter the session code the receiving device shows.") }
                }
            }
        }.privateScreen().navigationTitle("Transfer between devices").navigationBarTitleDisplayMode(.inline)
    }

    @ViewBuilder private func statusSection(_ status: DeviceTransferModel.Status, transfer: DeviceTransferModel) -> some View {
        Section {
            LabeledContent(LocalizedStringKey(status.role == "receive" ? "Receiving" : "Sending")) {
                Text(LocalizedStringKey(Self.state(status.state)))
            }
            if status.role == "receive" && status.running {
                LabeledContent("Session code") { Text(verbatim: status.session).font(.caption.monospaced()).textSelection(.enabled) }
                Button("Copy") { model.clipboard.copy(status.session) }
                FingerprintRow(title: "Device ID", value: status.to)
                if !status.addresses.isEmpty {
                    FingerprintRow(title: "Address", value: status.addresses.joined(separator: "\n"))
                }
            }
            if status.state == "running" {
                ProgressView(value: Double(status.report.bytes), total: Double(max(1, status.report.totalBytes)))
                Text(verbatim: "\(LocalOnly.size(Int64(clamping: status.report.bytes))) / \(LocalOnly.size(Int64(clamping: status.report.totalBytes)))")
                    .font(.caption).foregroundStyle(.secondary)
            } else if status.state == "waiting" { ProgressView() }
            if !status.running && status.report.peer != nil {
                LabeledContent("Records added", value: String(status.report.records))
                LabeledContent("Already here", value: String(status.report.kept))
                LabeledContent("Left out", value: String(status.report.skipped))
                LabeledContent("Transferred", value: LocalOnly.size(Int64(clamping: status.report.bytes)))
            }
            if let failure = status.error { Text(verbatim: failure.code).font(.footnote).foregroundStyle(.red) }
            if transfer.running { Button("Cancel", role: .destructive) { transfer.cancel(model) } }
            else if status.state == "failed" || status.state == "cancelled" {
                Button("Resume") { Task { await transfer.resume(model) } }.accessibilityIdentifier("transfer.resume")
            }
        }
    }

    private static func state(_ state: String) -> String {
        switch state {
        case "waiting": "Waiting for the other device"
        case "running": "Transferring"
        case "done": "Complete"
        case "failed": "Interrupted"
        default: "Cancelled"
        }
    }
}

/// This account's copies set aside here, whose file libraries import; an
/// open handover from the copy's device takes the import as its content.
struct MovedImportSection: View {
    let copies: [MovedCopy]
    let handovers: [Succession]
    let reload: @MainActor () async -> Void
    @Environment(SessionModel.self) private var model
    @State private var working = false
    @State private var imported = false

    var body: some View {
        if !copies.isEmpty {
            Section {
                ForEach(copies) { copy in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(verbatim: String(copy.deviceId.prefix(16))).font(.caption.monospaced())
                            Text(Date(timeIntervalSince1970: TimeInterval(copy.movedAt) / 1000), style: .date).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Import files") { Task { await importFiles(copy) } }.buttonStyle(.borderless).disabled(working)
                    }
                }
                if imported { Text("Files imported.").font(.footnote).foregroundStyle(.secondary) }
            } header: { Text("Copies from other iPhones") }
        }
    }

    private func importFiles(_ copy: MovedCopy) async {
        guard let engine = model.engine, !working else { return }
        working = true; imported = false
        defer { working = false }
        let handover = handovers.first { $0.side == "successor" && $0.predecessor == copy.deviceId && $0.available.contains("content") }
        do { _ = try await engine.movedImport(id: copy.id, succession: handover?.id); imported = true }
        catch { model.report(error) }
        await reload()
    }
}
