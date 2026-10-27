import SwiftUI
import CoreImage.CIFilterBuiltins
import VisionKit
import AVFoundation

struct AccountView: View {
    @Environment(SessionModel.self) private var model
    @State private var signingOut = false
    @State private var confirmAutomaticSync = false
    @State private var profile: AccountProfile?
    @State private var avatar: UIImage?
    @State private var loadingProfile = false
    @State private var profileFailed = false
    @State private var profileLoadedAt: Date?
    @State private var profileAccount: String?
    @State private var loadedProfileRevision: UInt64 = 0
    var body: some View {
        @Bindable var model = model
        List {
            HStack(spacing: 12) {
                Group {
                    if let avatar {
                        Image(uiImage: avatar).resizable().scaledToFill()
                    } else {
                        Image(systemName: "person.crop.circle.fill").resizable().scaledToFit()
                            .padding(8).foregroundStyle(Theme.forest)
                    }
                }.frame(width: 60, height: 60)
                    .background(Theme.forest.opacity(0.08))
                    .clipShape(RoundedRectangle(cornerRadius: 16))
                    .accessibilityIdentifier("account.avatar")
                VStack(alignment: .leading, spacing: 4) {
                    Text(profile?.displayName ?? String(localized: "My account"))
                        .font(.title2.bold()).fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("account.name")
                    Text(model.session?.account ?? "").font(.caption.monospaced())
                        .foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                }.frame(maxWidth: .infinity, alignment: .leading)
                if loadingProfile { ProgressView().controlSize(.small) }
            }.listRowBackground(Color.clear).listRowSeparator(.hidden)
            if profileFailed {
                Button("Retry loading profile") { Task { await loadProfile(force: true) } }.disabled(loadingProfile)
            }
            Section {
                NavigationLink { PublicProfileView() } label: { Label("Public profile", systemImage: "person.text.rectangle") }
                NavigationLink { ApprovalsView() } label: { Label("Approvals", systemImage: "checkmark.shield") }
                NavigationLink { AgentsView() } label: { Label("Agents", systemImage: "person.crop.square.filled.and.at.rectangle") }
                NavigationLink { WalletsView() } label: { Label("Team wallets", systemImage: "wallet.pass") }
                NavigationLink { DevicesView() } label: { Label("Authorized devices", systemImage: "laptopcomputer.and.iphone") }
                NavigationLink { RecoveryView() } label: { Label("Account recovery", systemImage: "arrow.counterclockwise.shield") }
                NavigationLink { AccountSecurityView() } label: { Label("Login identities", systemImage: "person.badge.key") }
                NavigationLink("Try the local demo") { LocalDemoView() }
                NavigationLink { BrowserConnectionsView() } label: { Label("Connected browsers", systemImage: "globe") }
                NavigationLink { SystemExportsView() } label: { Label("System integrations", systemImage: "square.stack.3d.up") }
                NavigationLink { HardwareSecurityView() } label: { Label("Hardware security", systemImage: "cpu") }
                    .accessibilityIdentifier("account.hardware")
                NavigationLink { SecretActivityView() } label: { Label("Activity on this device", systemImage: "clock.arrow.circlepath") }
                NavigationLink { SafetySettingsView() } label: { Label("Safety and support", systemImage: "hand.raised") }
                NavigationLink { AccountDeletionView() } label: { Label("Delete account", systemImage: "person.crop.circle.badge.minus") }
                NavigationLink { AccountsView() } label: { Label("Switch account", systemImage: "person.2") }
            }
            Section("Privacy") {
                Toggle("Quick unlock with system authentication", isOn: Binding(get: { model.session?.fastUnlock ?? false }, set: { enabled in Task {
                    await model.setFastUnlock(enabled: enabled)
                } })).disabled(model.busy)
                Toggle("Automatic foreground sync", isOn: Binding(get: { model.automaticSyncEnabled }, set: { enabled in
                    if enabled { confirmAutomaticSync = true } else { model.setAutomaticSync(false) }
                }))
                Toggle("Lock immediately in background", isOn: $model.immediateBackgroundLock)
                Text("Otherwise, dMsg locks after 3 minutes away. Device lock always locks your workspace immediately.")
                    .font(.footnote).foregroundStyle(.secondary)
                NavigationLink { SubscriptionView() } label: { Label("Plan and usage", systemImage: "creditcard") }
                    .accessibilityIdentifier("account.plan")
                NavigationLink("Inbox settings") { InboxSettingsView() }
                NavigationLink { NotificationSettingsView() } label: { Label("Notifications", systemImage: "bell") }
            }
            Section {
                Button { Task { await model.synchronize() } } label: {
                    Label {
                        if model.synchronizing { Text("Syncing…") } else { Text("Sync now") }
                    } icon: {
                        if model.synchronizing {
                            ProgressView().controlSize(.small).tint(Theme.forest).accessibilityHidden(true)
                        } else { Image(systemName: "arrow.triangle.2.circlepath") }
                    }
                }.disabled(model.busy).accessibilityIdentifier("account.sync")
                Button("Lock workspace", systemImage: "lock") { Task { await model.lock() } }
                Button("Sign out", role: .destructive) { signingOut = true }
            }
            NoticeBar(text: String(localized: "This device keeps its own encrypted workspace. Content that has not synced cannot be recovered from another device."))
                .listRowBackground(Color.clear)
        }.privateScreen().navigationTitle("Account")
            .task(id: "\(model.session?.generation ?? 0):\(model.session?.lastSyncAtMs ?? 0):\(model.profileRevision)") { await loadProfile(force: loadedProfileRevision != model.profileRevision) }
            .confirmationDialog("Enable automatic sync for this account?", isPresented: $confirmAutomaticSync) {
                Button("Enable automatic sync") { model.setAutomaticSync(true) }
            } message: {
                Text("While dMsg is open and unlocked, encrypted edits will upload and messages and approvals will refresh. File backups still require a separate confirmation. You can turn this off at any time.")
            }
            .toolbar { Button("Lock workspace", systemImage: "lock") { Task { await model.lock() } }.accessibilityIdentifier("account.lock") }
            .confirmationDialog("Sign out of this account?", isPresented: $signingOut) {
                Button("Sign out", role: .destructive) { Task { await logout() } }
            } message: { Text("Your encrypted files remain here. Saved login and quick unlock are removed.") }
    }

    private func loadProfile(force: Bool = false) async {
        #if DEBUG
        if model.uiTesting {
            let name = ProcessInfo.processInfo.arguments.contains("--ui-long-profile")
                ? "Studio Research · Design, Security and Collaboration" : "Studio"
            profile = AccountProfile(name: name, description: nil, avatarUrl: nil, updatedAt: 1)
            avatar = UIImage(named: "BrandMark")
            return
        }
        #endif
        guard let engine = model.engine, model.unlocked else { return }
        let epoch = model.epoch, account = model.session?.account
        if !force, profileAccount == account, let at = profileLoadedAt, Date.now.timeIntervalSince(at) < 300 { return }
        loadingProfile = true; profileFailed = false
        defer { if !Task.isCancelled, epoch == model.epoch { loadingProfile = false } }
        do {
            let result = try await engine.profile()
            guard !Task.isCancelled, epoch == model.epoch, account == model.session?.account else { return }
            let incoming: AccountProfile = try model.decode(result)
            if incoming.avatarUrl != profile?.avatarUrl { avatar = nil }
            profile = incoming; profileAccount = account; profileLoadedAt = .now; loadedProfileRevision = model.profileRevision
            if let url = incoming.avatarUrl, avatar == nil {
                let bytes = try await engine.profileAvatar(url: url)
                guard !Task.isCancelled, epoch == model.epoch, account == model.session?.account else { return }
                avatar = await AvatarImage.decode(bytes)
            }
        } catch {
            if !Task.isCancelled, epoch == model.epoch, account == model.session?.account { profileFailed = true; profileLoadedAt = nil }
        }
    }

    private func logout() async {
        let account = model.session?.account
        if let account { await model.cancelBackgroundForAccount(account) }
        model.invalidateSystemExports()
        await model.push.setEnabled(false, model: model)
        await model.lock()
        guard account == model.session?.account else { return }
        do { _ = try await model.engine?.logout(); await model.updateSession() } catch { model.report(error) }
    }
}

struct DevicesView: View {
    @Environment(SessionModel.self) private var model
    @State private var devices: [DeviceInfo] = []
    struct AccountDevices: Decodable { let devices: [DeviceInfo] }
    var body: some View {
        List {
            Section { NavigationLink("Authorize another device") { PairApprovalView() } }
            Section("Authorized devices") {
                ForEach(devices) { device in
                    NavigationLink { DeviceDetail(device: device) } label: {
                        HStack(spacing: 12) {
                            ObjectIcon(name: device.current ? "iphone" : "laptopcomputer.and.iphone")
                            VStack(alignment: .leading, spacing: 5) {
                                Text(device.current ? String(localized: "This iPhone") : device.role).font(.headline)
                                Text(device.deviceId).font(.caption.monospaced()).lineLimit(1).foregroundStyle(.secondary)
                                if device.revokedAt != nil { Text("Revoked").font(.caption).foregroundStyle(.red) }
                            }.padding(.vertical, 5)
                        }
                    }
                }
            }
        }.privateScreen().navigationTitle("Devices").task {
            do { if let result = try await model.engine?.devices() { let account: AccountDevices = try model.decode(result); devices = account.devices } }
            catch { model.report(error) }
        }
    }
}

struct DeviceInfo: Decodable, Identifiable {
    let deviceId: String; let role: String; let capabilities: [String]
    let addedAt: UInt64; let addedBy: String?; let revokedAt: UInt64?; let current: Bool
    var id: String { deviceId }
}

struct DeviceDetail: View {
    let device: DeviceInfo
    @Environment(SessionModel.self) private var model
    @State private var request: ApprovalView?
    @State private var confirming = false
    var body: some View {
        List {
            Section("Device") {
                LabeledContent("Role", value: device.role)
                LabeledContent("Device ID") { Text(device.deviceId).font(.caption.monospaced()).textSelection(.enabled) }
                LabeledContent("Added") { Text(Date(timeIntervalSince1970: Double(device.addedAt) / 1000), style: .date) }
                if let source = device.addedBy { LabeledContent("Approved by") { Text(source).font(.caption.monospaced()).textSelection(.enabled) } }
            }
            Section("Permissions") { ForEach(device.capabilities, id: \.self) { Text($0) } }
            if device.revokedAt == nil {
                Button("Revoke this device", role: .destructive) { confirming = true }
            }
            NoticeBar(text: String(localized: "Revocation stops new access after security state is refreshed and keys are rotated. Previously copied content cannot be recalled."))
        }.privateScreen().navigationTitle(device.current ? "This iPhone" : "Device")
            .confirmationDialog("Create a revocation request?", isPresented: $confirming) {
                Button("Review revocation", role: .destructive) { Task {
                    do { if let result = try await model.engine?.revokeDevice(device: device.deviceId) { request = try model.decode(result) } }
                    catch { model.report(error) }
                } }
            }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
    }
}

struct PairView: View {
    @Environment(SessionModel.self) private var model
    @State private var token: String?
    @State private var expires = Date.distantPast
    @State private var qrImage: UIImage?
    var body: some View {
        List {
            HStack(spacing: 12) {
                ObjectIcon(name: "iphone")
                VStack(alignment: .leading, spacing: 4) {
                    Text("Authorize this iPhone").font(.title2.bold())
                    Text("Your existing device makes the final decision.").font(.subheadline).foregroundStyle(.secondary)
                }
            }.listRowBackground(Color.clear)
            if let token {
                if let image = qrImage { Image(uiImage: image).interpolation(.none).resizable().scaledToFit().frame(maxWidth: 240).frame(maxWidth: .infinity).accessibilityLabel("Device pairing request QR code") }
                Text(model.session?.deviceId ?? "").font(.caption.monospaced()).textSelection(.enabled)
                ShareLink(item: token) { Label("Share pairing request", systemImage: "square.and.arrow.up") }
                Button("Copy pairing request") { model.clipboard.copy(token) }
                Text(expires, style: .relative).font(.caption).foregroundStyle(.secondary)
                Button("Check authorization") { Task { await refresh() } }
            }
            Button("Create a new pairing request") { Task { await request() } }
            NoticeBar(text: String(localized: "On the authorized device, open Approvals and review this device's full fingerprint, role and permissions. Scanning never approves automatically."))
            NavigationLink("All devices lost?") { RecoveryView() }
        }.privateScreen().navigationTitle("Join account").task { await request() }
    }

    private func request() async {
        do {
            if let result = try await model.engine?.pairRequest() {
                // Pairing is available before a content root is opened.
                token = try JSONDecoder().decode(String.self, from: Data(result.json.utf8)); expires = .now.addingTimeInterval(270)
                qrImage = token.flatMap(QRCode.image)
            }
        } catch { model.report(error) }
    }

    private func refresh() async {
        do { _ = try await model.engine?.refreshAccount(); await model.updateSession() }
        catch { model.report(error) }
    }
}

struct PairApprovalView: View {
    @Environment(SessionModel.self) private var model
    @State private var token = ""
    @State private var device = ""
    @State private var created: ApprovalView?
    @State private var scanning = false
    @State private var preview: DisplayValue?
    struct PairPreview: Decodable { let deviceId: String }
    var body: some View {
        Form {
            Section("Device request") { TextField("Paste pairing request", text: $token, axis: .vertical).lineLimit(3...6).textInputAutocapitalization(.never) }
            if DataScannerViewController.isSupported {
                Button("Scan device request", systemImage: "qrcode.viewfinder") { Task {
                    if await AVCaptureDevice.requestAccess(for: .video) { scanning = true }
                } }
            }
            Button("Inspect pairing request") { Task { await inspect() } }.disabled(token.isEmpty)
            if let preview { Section("Requested permissions") { CanonicalDetails(value: preview) } }
            Section("Verify the full device fingerprint") { TextField("Device ID", text: $device, axis: .vertical).font(.body.monospaced()).textInputAutocapitalization(.never) }
            Button("Review in approval center") { Task { await create() } }.disabled(token.isEmpty || device.count != 64)
            NoticeBar(text: String(localized: "This creates a pending request. It does not authorize the device."))
        }.privateScreen().navigationTitle("Add device")
            .sheet(isPresented: $scanning) { PairingScanner { text in token = text; scanning = false; Task { await inspect() } } }
            .navigationDestination(isPresented: Binding(get: { created != nil }, set: { if !$0 { created = nil } })) {
                if let created { ApprovalDetailView(request: created) }
            }
    }

    private func inspect() async {
        do {
            guard let result = try await model.engine?.inspectPairingRequest(token: token) else { return }
            let request: PairPreview = try model.decode(result)
            device = request.deviceId; preview = try model.decode(result)
        } catch { preview = nil; device = ""; model.report(error) }
    }

    private func create() async {
        do { if let result = try await model.engine?.approvePairingRequest(token: token, device: device) { created = try model.decode(result) } }
        catch { model.report(error) }
    }
}

struct RecoveryView: View {
    @Environment(SessionModel.self) private var model
    @State private var value: DisplayValue?
    @State private var confirming = false
    var body: some View {
        List {
            NoticeBar(text: String(localized: "Recovery uses your bound login and a waiting period. An authorized device can dispute it. Unsynced local content cannot be recovered."))
            if let value { Section("Recovery status") { CanonicalDetails(value: value) } }
            Button("Request account recovery") { confirming = true }
            Button("Complete recovery after waiting period") { Task { await complete() } }
        }.privateScreen().navigationTitle("Recovery").task { await refresh() }
            .confirmationDialog("Start account recovery?", isPresented: $confirming) {
                Button("Request recovery") { Task {
                    do { _ = try await model.engine?.requestRecovery(); await refresh() }
                    catch { model.report(error) }
                } }
            }
    }
    private func refresh() async {
        do { if let result = try await model.engine?.recoveryStatus() { value = try JSONDecoder().decode(DisplayValue.self, from: Data(result.json.utf8)) } }
        catch { model.report(error) }
    }
    private func complete() async {
        do { _ = try await model.engine?.completeRecovery(); await model.updateSession() }
        catch { model.report(error) }
    }
}

struct AccountsView: View {
    @Environment(SessionModel.self) private var model
    @State private var accounts: [LocalAccount] = []
    struct LocalAccount: Decodable, Identifiable { let account: String; let current: Bool; var id: String { account } }
    var body: some View {
        List {
            ForEach(accounts) { account in
                Button { Task { await choose(account.account) } } label: {
                    HStack { Text(account.account).font(.body.monospaced()); Spacer(); if account.current { Image(systemName: "checkmark") } }
                }
            }
            Button("Sign in to another account") { Task { await model.lock(); await model.login() } }
        }.privateScreen().navigationTitle("Accounts").task {
            do { if let result = try await model.engine?.accounts() { accounts = try model.decode(result) } }
            catch { model.report(error) }
        }
    }
    private func choose(_ account: String) async {
        model.invalidateSystemExports()
        await model.lock()
        do { _ = try await model.engine?.switchAccount(account: account); await model.updateSession() }
        catch { model.report(error) }
    }
}

enum QRCode {
    static func image(_ text: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator(); filter.message = Data(text.utf8); filter.correctionLevel = "M"
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 5, y: 5)),
              let image = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: image)
    }
}
