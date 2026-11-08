import SwiftUI
@preconcurrency import DMsgCore

struct DevicesView: View {
    @Environment(SessionModel.self) private var model
    @State private var display = DisplayLoad()
    @State private var devices: [DeviceInfo] = []
    @State private var copies: [MovedCopy] = []
    @State private var handovers: [Succession] = []
    @State private var auxiliaryFailed = false
    struct AccountDevices: Decodable { let devices: [DeviceInfo] }
    private var canManage: Bool { devices.contains { $0.current && $0.managesAccount } }
    var body: some View {
        List {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Section {
                NavigationLink("Authorize another device") { PairApprovalView() }.disabled(!canManage)
                NavigationLink("Replace a device") { SuccessionsView() }.accessibilityIdentifier("devices.succession")
                NavigationLink("Transfer between devices") { DeviceTransferView() }.accessibilityIdentifier("devices.transfer")
            }
            if display.loaded && !canManage { Text("An administrator device is required to change account security.").font(.footnote).foregroundStyle(.secondary) }
            SettingsLoadStatus(loading: false, failed: auxiliaryFailed) { Task { await load(force: true) } }
            MovedImportSection(copies: copies, handovers: handovers) { await load() }
            Section("Authorized devices") {
                ForEach(devices) { device in
                    NavigationLink { DeviceDetail(device: device, canManage: canManage) } label: {
                        HStack(spacing: 10) {
                            ObjectIcon(name: device.current ? "iphone" : "laptopcomputer.and.iphone")
                            VStack(alignment: .leading, spacing: 2) {
                                Text(model.deviceLabel(device.id) ?? (device.current ? String(localized: "This iPhone") : deviceRoleTitle(device.role))).font(.headline)
                                Text(device.deviceId).font(.caption.monospaced()).lineLimit(1).foregroundStyle(.secondary)
                                if device.revokedAt != nil { Text("Revoked").font(.caption).foregroundStyle(.red) }
                            }
                        }
                    }.compactObjectRow()
                }
            }
        }.privateScreen().navigationTitle("Devices").task(id: model.contentRevision) { await load(force: true) }.refreshable { await load(force: true) }
    }

    private func load(force: Bool = false) async {
        guard let engine = model.engine else { return }
        await display.load(model, kind: "account-summary", force: force) { (account: AccountDevices) in devices = account.devices }
        let account = model.session?.account, epoch = model.epoch
        do {
            let moved: [MovedCopy] = try model.decode(try await engine.movedCopies())
            let next: [Succession] = try model.decode(try await engine.successions())
            guard epoch == model.epoch, !Task.isCancelled else { return }
            copies = moved.filter { $0.account != nil && $0.account == account }; handovers = next; auxiliaryFailed = false
        } catch { if epoch == model.epoch, !Task.isCancelled { auxiliaryFailed = true } }
    }
}

struct DeviceInfo: Decodable, Identifiable {
    let deviceId: String; let role: String; let capabilities: [String]
    let addedAt: UInt64; let addedBy: String?; let revokedAt: UInt64?; let current: Bool
    var id: String { deviceId }
    var managesAccount: Bool { revokedAt == nil && role.lowercased() == "administrator" && capabilities.contains("RootManage") }
}

struct DeviceDetail: View {
    let device: DeviceInfo
    var canManage = false
    @Environment(SessionModel.self) private var model
    @State private var request: ApprovalView?
    @State private var receipts = ApprovalReceipts()
    @State private var confirming = false
    @State private var working = false
    @State private var label = ""
    @State private var current: DeviceInfo?
    @State private var permission: Bool?
    @State private var failed = false
    var body: some View {
        let device = current ?? self.device
        let canManage = permission ?? self.canManage
        List {
            SettingsLoadStatus(loading: false, failed: failed) { Task { await refresh() } }
            Section("Only on this device") {
                SettingsField("Local label") { TextField("Local label", text: $label) }
                SettingsAction("Save settings", prominent: true) { model.setDeviceLabel(device.id, label: label); model.notice = String(localized: "Saved") }
                    .disabled(label == (model.deviceLabel(device.id) ?? ""))
            }
            Section("Device") {
                LabeledContent("Role", value: deviceRoleTitle(device.role))
                FingerprintRow(title: "Device ID", value: device.deviceId)
                SettingsAction("Copy device ID") { model.clipboard.copy(device.deviceId); model.notice = String(localized: "Copied") }
                if let revoked = device.revokedAt { LabeledContent("Revoked") { Text(Date(timeIntervalSince1970: Double(revoked) / 1000), style: .date) } }
                LabeledContent("Added") { Text(Date(timeIntervalSince1970: Double(device.addedAt) / 1000), style: .date) }
                if let source = device.addedBy { LabeledContent("Approved by") { Text(source).font(.caption.monospaced()).textSelection(.enabled) } }
            }
            Section("Permissions") { ForEach(device.capabilities, id: \.self) { Text(deviceCapabilityTitle($0)) } }
            if device.revokedAt == nil {
                SettingsAction("Revoke this device", role: .destructive) { confirming = true }.disabled(working || !canManage || receipts["device.revoke"] != nil)
                ApprovalReceiptRow(receipts: receipts, key: "device.revoke") { request = $0 }
                if !canManage { Text("An administrator device is required to change account security.").font(.footnote).foregroundStyle(.secondary) }
            }
            NoticeBar(text: String(localized: "Revocation stops new access after security state is refreshed and keys are rotated. Previously copied content cannot be recalled."))
        }.privateScreen().navigationTitle(LocalizedStringKey(device.current ? "This iPhone" : "Device"))
            .onAppear { label = model.deviceLabel(device.id) ?? "" }
            .task(id: model.contentRevision) { await refresh() }
            .confirmationDialog("Create a revocation request?", isPresented: $confirming) {
                Button("Review revocation", role: .destructive) { Task {
                    guard !working, canManage else { return }; working = true; defer { working = false }
                    let epoch = model.epoch
                    do {
                        let value = try await receipts.request("device.revoke", model: model) { try await $0.revokeDevice(device: device.deviceId) }
                        if let value, epoch == model.epoch { request = value }
                    } catch { if epoch == model.epoch { model.report(error) } }
                } }
            }
            .approvalDestination($request, receipts: receipts, model: model)
    }

    private func refresh() async {
        let epoch = model.epoch
        do {
            guard let value = try await model.engine?.devices(), epoch == model.epoch, !Task.isCancelled else { return }
            let account: DevicesView.AccountDevices = try model.decode(value)
            current = account.devices.first { $0.id == device.id }
            permission = account.devices.contains { $0.current && $0.managesAccount }; failed = false
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
        await receipts.settle(model)
    }

}
