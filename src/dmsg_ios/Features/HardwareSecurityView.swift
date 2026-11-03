import SwiftUI
@preconcurrency import DMsgCore

struct HardwareSecurityStatus: Decodable {
    struct Key: Decodable {
        let keyId: String
        let registeredAt: UInt64
        let registeredBy: String
        let here: Bool
        let biometryOnly: Bool?
        let fingerprintsChanged: Bool?
    }
    struct Device: Decodable, Identifiable {
        let deviceId: String
        let thisDevice: Bool
        let keyId: String?
        var id: String { deviceId }
    }
    struct Unprotected: Decodable { let backgroundOnline: Bool; let residentAgents: UInt64 }
    let available: Bool
    let canManagePolicy: Bool
    let deviceBound: Bool
    let boundKeyAvailable: Bool?
    let biometry: Bool
    let key: Key?
    let waiting: [String]
    let policy: [String]
    let devices: [Device]
    let boundBiometryOnly: Bool?
    let unprotected: Unprotected
}

struct HardwareSecurityView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var status: HardwareSecurityStatus?
    @State private var request: ApprovalView?
    @State private var busy = false
    @State private var loading = true
    @State private var biometryOnly = false
    @State private var policy: Set<String> = []
    @State private var bindingChange: Bool?
    @State private var enrollmentToken: String?
    @State private var incomingToken = ""
    @State private var incomingDevice = ""
    @State private var local: LocalOnly?
    @State private var scope: UInt64 = 0
    struct Draft: Codable { let policy: Set<String>; let biometry: Bool; let token: String; let device: String }
    private var dirty: Bool { status != nil && (policy != Set(status?.policy ?? []) || !incomingToken.isEmpty || !incomingDevice.isEmpty) }
    private let controlPurposes: Set<String> = ["account_control", "device_control", "principal_control"]

    var body: some View {
        Form {
            NoticeBar(text: String(localized: "Quick unlock opens this workspace. Device binding limits which device can unlock it. Hardware approval protects sensitive account actions."))
            if let status {
                Section("This device") {
                    LabeledContent("Hardware approval key", value: status.key?.here == true ? String(localized: "Registered on this device") : String(localized: "Not registered"))
                    if let key = status.key {
                        FingerprintRow(title: "Key fingerprint", value: key.keyId)
                        if key.fingerprintsChanged == true { NoticeBar(text: String(localized: "Enrolled biometrics changed. Register a replacement key before relying on this key.")) }
                    }
                    if !status.available { NoticeBar(text: String(localized: "Secure Enclave is unavailable on this device.")) }
                    Toggle("Biometrics only", isOn: $biometryOnly).disabled(!status.biometry || busy)
                    Text("Biometrics-only keys cannot use the device passcode and may stop working when enrolled biometrics change.")
                        .font(.footnote).foregroundStyle(.secondary)
                    SettingsAction(LocalizedStringKey(status.key?.here == true ? "Review replacement key" : "Review key registration")) { Task { await register() } }
                        .disabled(!status.available || busy)
                    SettingsAction("Create an administrator enrollment request") { Task { await enrollment() } }
                        .disabled(!status.available || busy)
                    if status.key != nil {
                        SettingsAction("Review retiring this approval key", role: .destructive) { Task { await retire(nil) } }.disabled(busy)
                    }
                }
                if let enrollmentToken {
                    Section("Send to an administrator") {
                        Text("Share this request with an authorized administrator. It does not register or approve the key by itself.")
                            .font(.footnote).foregroundStyle(.secondary)
                        ShareLink(item: enrollmentToken) { SettingsActionLabel("Share enrollment request", systemImage: "square.and.arrow.up") }
                        SettingsAction("Copy enrollment request") { model.clipboard.copy(enrollmentToken) }
                        FingerprintRow(title: "Device ID", value: model.session?.deviceId ?? "")
                    }
                }
                Section("Device-bound workspace") {
                    Toggle("Require this device to unlock", isOn: Binding(get: { status.deviceBound }, set: { bindingChange = $0 })).disabled(busy || !status.available)
                    LocalOnlyWarning(value: local)
                    if status.deviceBound {
                        LabeledContent("Unlock policy", value: status.boundBiometryOnly == true ? String(localized: "Biometrics only") : String(localized: "System authentication"))
                        if status.boundKeyAvailable == false {
                            NoticeBar(text: String(localized: "The local binding key is unavailable. Keep this workspace and review your recovery options before locking it."))
                        }
                    }
                    Text("Binding protects this local workspace even after a new login. Keep another authorized device or a recovery path before changing it.")
                        .font(.footnote).foregroundStyle(.secondary)
                    if status.unprotected.backgroundOnline || status.unprotected.residentAgents > 0 {
                        NoticeBar(text: String(localized: "Background signing and resident agent capabilities are separate from this workspace lock."))
                    }
                }
                Section("Require hardware approval") {
                    Toggle("Account, devices and identities", isOn: Binding(
                        get: { !policy.isDisjoint(with: controlPurposes) },
                        set: { if $0 { policy.formUnion(controlPurposes) } else { policy.removeAll() } }))
                    purposeToggle("Payments", purpose: "payment").disabled(policy.isDisjoint(with: controlPurposes))
                    purposeToggle("Attestations", purpose: "attestation").disabled(policy.isDisjoint(with: controlPurposes))
                    SettingsAction("Review hardware policy changes") { Task { await changePolicy() } }
                        .disabled(busy || policy == Set(status.policy))
                    Text("Account, device and identity protection changes together. A required approval cannot fall back to an ordinary confirmation dialog.")
                        .font(.footnote).foregroundStyle(.secondary)
                }.disabled(busy || !status.canManagePolicy)
                Section("Other devices") {
                    ForEach(status.devices) { device in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(LocalizedStringKey(device.thisDevice ? "This iPhone" : "Authorized device")).font(.headline)
                            FingerprintRow(title: "Device ID", value: device.deviceId)
                            if let key = device.keyId {
                                FingerprintRow(title: "Key fingerprint", value: key)
                                if !device.thisDevice { Button("Review retiring this approval key", role: .destructive) { Task { await retire(device.deviceId) } }.disabled(busy || !status.canManagePolicy) }
                            } else { Text("No registered approval key").font(.caption).foregroundStyle(.secondary) }
                        }.padding(.vertical, 4)
                    }
                }
                Section("Review an enrollment request") {
                    SettingsField("Paste enrollment request") { TextField("Paste enrollment request", text: $incomingToken, axis: .vertical).lineLimit(2...5).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                    SettingsField("Device ID") { TextField("Device ID", text: $incomingDevice, axis: .vertical).font(.caption.monospaced()).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                    SettingsAction("Review key enrollment") { Task { await enrollOther() } }
                        .disabled(busy || !status.canManagePolicy || incomingToken.isEmpty || incomingDevice.count != 64)
                }
            } else if loading { ProgressView() }
            else { SettingsAction("Retry loading hardware security") { Task { await load() } } }
        }.disabled(busy).privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Hardware security").navigationBarTitleDisplayMode(.inline).confirmLeavingDraft(dirty)
            .task { scope = model.viewGeneration; await load() }
            .onDisappear { if dirty { model.keepFormDraft("hardware", value: Draft(policy: policy, biometry: biometryOnly, token: incomingToken, device: incomingDevice), generation: scope) } }.refreshable { await load() }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
            .confirmationDialog(LocalizedStringKey(bindingChange == true ? "Bind this workspace to this iPhone?" : "Remove device binding?"),
                isPresented: Binding(get: { bindingChange != nil }, set: { if !$0 { bindingChange = nil } })) {
                Button(LocalizedStringKey(bindingChange == true ? "Bind workspace" : "Remove device binding"), role: bindingChange == true ? nil : .destructive) {
                    if let enabled = bindingChange { Task { await bindWorkspace(enabled) } }
                }
            } message: { Text("This changes local unlock protection. System authentication may be required. No account recovery or other device is changed.") }
    }

    private func purposeToggle(_ title: LocalizedStringKey, purpose: String) -> some View {
        Toggle(title, isOn: Binding(get: { policy.contains(purpose) }, set: { if $0 { policy.insert(purpose) } else { policy.remove(purpose) } }))
    }

    private func load() async {
        let editing = dirty, previousPolicy = policy, previousBiometry = biometryOnly
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let value = try await model.engine?.hardwareSecurity(), epoch == model.epoch && visible, !Task.isCancelled else { return }
            let next: HardwareSecurityStatus = try model.decode(value)
            status = next
            if !editing && policy == previousPolicy && biometryOnly == previousBiometry { policy = Set(next.policy); biometryOnly = next.key?.biometryOnly ?? next.boundBiometryOnly ?? false }
            if let draft = model.formDraft("hardware", as: Draft.self) {
                policy = draft.policy; biometryOnly = draft.biometry; incomingToken = draft.token; incomingDevice = draft.device
                model.settingsDrafts.removeValue(forKey: "hardware")
            }
        } catch { if epoch == model.epoch && visible { model.report(error) } }
        local = await LocalOnly.load(model)
    }

    private func approval(_ action: @MainActor (MobileEngine) async throws -> ObjectSnapshot) async {
        guard let engine = model.engine, !busy else { return }
        let epoch = model.epoch; busy = true; model.activity()
        defer { busy = false }
        do {
            let result = try await action(engine)
            guard epoch == model.epoch && visible else { return }; request = try model.decode(result)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func register() async { await approval { try await $0.registerApprovalKey(biometryOnly: biometryOnly) } }
    private func retire(_ device: String?) async { await approval { try await $0.retireApprovalKey(device: device) } }
    private func changePolicy() async { await approval { try await $0.setHardwarePolicy(purposes: policy.sorted()) } }
    private func enrollOther() async { await approval { try await $0.approveApprovalKey(token: incomingToken, device: incomingDevice) } }

    private func enrollment() async {
        struct Token: Decodable { let token: String }
        guard let engine = model.engine, !busy else { return }
        let epoch = model.epoch; busy = true; model.activity()
        defer { busy = false }
        do {
            let result = try await engine.approvalKeyRequest(biometryOnly: biometryOnly)
            guard epoch == model.epoch && visible else { return }
            let token: Token = try model.decode(result); enrollmentToken = token.token
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func bindWorkspace(_ enabled: Bool) async {
        bindingChange = nil
        guard let engine = model.engine, !busy else { return }
        let epoch = model.epoch; busy = true; model.activity()
        defer { busy = false }
        do {
            model.invalidateSystemExports()
            _ = try await engine.setDeviceBound(enabled: enabled, biometryOnly: enabled && biometryOnly)
            guard epoch == model.epoch && visible else { return }; await model.updateSession(); await load()
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }
}

struct FingerprintRow: View {
    let title: LocalizedStringKey
    let value: String
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            Text(value).font(.caption.monospaced()).textSelection(.enabled)
                .environment(\.layoutDirection, .leftToRight).fixedSize(horizontal: false, vertical: true)
        }
    }
}
