import SwiftUI
import DMsgCore
import VisionKit
import AVFoundation

/// A single invitation follows the login through account selection and both approvals.
struct DesktopPairingView: View {
    struct Preview: Decodable {
        var stage: String
        var account: String
        var currentAccount: String?
        var principal: String?
        var deviceId: String?
        var code: String?
        var expiresAt: UInt64
    }
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.openURL) private var openURL
    @State private var invitation = ""
    @State private var preview: Preview?
    @State private var scanning = false
    @State private var working = false
    @State private var paused = false
    @State private var expired = false
    @State private var retryContinuation = false
    @State private var cameraDenied = false
    @State private var failure: String?
    private var token: String { invitation.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var waiting: Bool { ["waiting_login", "waiting_device", "authorized"].contains(preview?.stage ?? "") }

    var body: some View {
        Form {
            if let preview {
                Section("Account") {
                    FingerprintRow(title: "Account ID", value: preview.account)
                    if preview.stage == "bind_login", let principal = preview.principal {
                        FingerprintRow(title: "Login identity", value: principal)
                    }
                    if let code = preview.code { FingerprintRow(title: "Comparison code", value: code); Text("Check that both devices show the same code before approving.") }
                    if let device = preview.deviceId {
                        FingerprintRow(title: "Device ID", value: groupedFingerprint(device))
                            .accessibilityIdentifier("pair.desktop.device")
                    }
                }
                switch preview.stage {
                case "sign_in", "other_account", "linked_login_required":
                    if preview.stage == "other_account" {
                        NoticeBar(text: String(localized: "This login belongs to another account. Choose a different login."))
                        if let account = preview.currentAccount { FingerprintRow(title: "Account ID", value: account) }
                    } else if preview.stage == "linked_login_required" {
                        NoticeBar(text: String(localized: "Use a login already linked to the account you want to open."))
                    }
                    if model.session?.passkeyAvailable == true {
                        Button("Use an existing passkey") { Task { await signIn(.login) } }
                        if preview.stage != "linked_login_required" { Button("Create a passkey") { Task { await signIn(.register) } } }
                    }
                    Button("Continue with Internet Identity") { Task { if await model.login(usingInternetIdentity: true) { await check() } } }
                case "bind_login", "device":
                    Button(LocalizedStringKey(preview.stage == "bind_login" ? "Use this login for this account and add this device" : "Send device request")) {
                        Task { await check(continueJoin: true) }
                    }.accessibilityIdentifier("pair.desktop.confirm")
                default:
                    if preview.stage == "waiting_device" { LabeledContent("Login identity") { Label("Completed", systemImage: "checkmark") } }
                    Text(LocalizedStringKey(model.session?.state == "root_required" ? "Approved. Preparing your vault…" : preview.stage == "waiting_login" ? "Waiting for login approval" : "Waiting for device approval"))
                }
            } else {
                NoticeBar(text: String(localized: "Scan the code on an authorized device to open its account."))
                if DataScannerViewController.isSupported {
                    Button("Scan invitation", systemImage: "qrcode.viewfinder") { Task { await scan() } }
                }
                if cameraDenied {
                    NoticeBar(text: String(localized: "Camera access is unavailable. Paste the invitation or allow camera access in Settings."))
                    Button("Open Settings") { if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) } }
                }
                Section("Paste invitation") {
                    TextField("Paste invitation", text: $invitation, axis: .vertical).lineLimit(3...6)
                        .textInputAutocapitalization(.never).autocorrectionDisabled().accessibilityIdentifier("pair.desktop.invitation")
                    Button("Review") { Task { await check() } }.disabled(token.isEmpty).accessibilityIdentifier("pair.desktop.send")
                }
                if model.session?.state == "no_account" {
                    NavigationLink("Create binding request") { LoginBindingRequestView() }
                }
            }
            if expired { NoticeBar(text: String(localized: "This request has expired")) }
            if let failure { NoticeBar(text: failure) }
            if (paused && !expired) || failure != nil {
                Button("Retry") { Task { paused = false; await check(continueJoin: retryContinuation) } }
            }
            if preview != nil || expired {
                Button("Scan invitation") { preview = nil; failure = nil; expired = false; paused = false; invitation = ""; Task { await scan() } }
            }
            if working { ProgressView().accessibilityIdentifier("pair.desktop.working") }
        }.disabled(working || model.busy).privateScreen().navigationTitle("Open an existing account")
            .onChange(of: model.session?.principal) { _, principal in
                if let previous = preview?.principal, previous != principal, !working && !model.busy {
                    paused = true
                    Task { await check() }
                }
            }
            .sheet(isPresented: $scanning) {
                QRCodeScanner { text in invitation = text; scanning = false; Task { await check() } }
            }
            .task {
                if let saved = try? await model.engine?.savedAccountInvitation() { invitation = saved; await check() }
                else { await scan() }
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .seconds(4)) } catch { return }
                    if scenePhase == .active, waiting, !paused, !working, !model.busy { await check(continueJoin: true) }
                }
            }
    }

    private func signIn(_ operation: PasskeyOperation) async {
        if await model.login(passkeyOperation: operation) { await check() }
    }

    private func scan() async {
        guard DataScannerViewController.isSupported else { return }
        let allowed = await AVCaptureDevice.requestAccess(for: .video)
        guard !Task.isCancelled else { return }
        cameraDenied = !allowed || !DataScannerViewController.isAvailable
        scanning = !cameraDenied
    }

    private func check(continueJoin: Bool = false) async {
        guard !working, !token.isEmpty else { return }
        retryContinuation = continueJoin
        working = true; defer { working = false }
        let input = token
        failure = nil
        expired = false
        do {
            #if DEBUG
            if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-pairing-expiry") {
                if preview == nil { preview = Preview(stage: "waiting_device", account: "040g2081040g2081040g", deviceId: String(repeating: "aa", count: 32), expiresAt: UInt64(Date.now.timeIntervalSince1970 * 1000) + 3000) }
                else { expired = true; preview = nil }
                return
            }
            #endif
            guard let engine = model.engine else { return }
            if continueJoin, model.session?.principal == preview?.principal, ["authorized", "waiting_device"].contains(preview?.stage ?? "") {
                do { _ = try await engine.refreshAccount() }
                catch { await model.updateSession(); throw error }
                await model.updateSession()
                guard input == token, !Task.isCancelled, model.showingJoinAccount else { return }
                if model.unlocked && model.session?.account == preview?.account { dismiss(); return }
                if model.session?.state == "root_required" { return }
            }
            let result = try await engine.accountInvitation(invitation: input, principal: continueJoin ? preview?.principal : nil, bindLogin: continueJoin && preview?.stage == "bind_login")
            guard input == token, !Task.isCancelled, model.showingJoinAccount else { return }
            let next: Preview = try model.decode(result)
            preview = next
            paused = false
            await model.updateSession()
            if next.stage == "authorized", model.unlocked, model.session?.account == next.account { dismiss() }
        } catch {
            guard !Task.isCancelled, token == input, model.showingJoinAccount else { return }
            if let err = error as? MobileError, case .Failure(let code) = err {
                if code == "EXPIRED" { expired = true; preview = nil; paused = true; return }
                if ["AUTH_REQUIRED", "LOGIN_EXPIRED"].contains(code), preview != nil {
                    preview?.stage = "sign_in"; preview?.principal = nil; preview?.code = nil; preview?.deviceId = nil
                    paused = true
                    return
                }
                paused = !["UNAVAILABLE", "NETWORK_UNAVAILABLE", "EXECUTION_UNKNOWN", "POLICY_STALE", "SYNC_BUSY"].contains(code)
            } else { paused = true }
            model.report(error)
            failure = model.error
        }
    }
}
