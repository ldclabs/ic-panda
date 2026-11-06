import SwiftUI
import DMsgCore
import VisionKit
import AVFoundation

/// Scanning transports the request to the desktop. Only its approval center
/// can grant authority; this screen never treats delivery as enrollment.
struct DesktopPairingView: View {
    private struct Delivery: Decodable { let deviceId: String; let expiresAt: UInt64 }
    @Environment(SessionModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var invitation = ""
    @State private var scanning = false
    @State private var attempt: UUID?
    @State private var working = false
    @State private var delivery: Delivery?
    @State private var failed = false
    @State private var cameraDenied = false
    @State private var needsLogin = false

    var body: some View {
        Form {
            NoticeBar(text: String(localized: "Sign in to the same account on the new device, scan the desktop QR code, then review and approve on the desktop."))
            if DataScannerViewController.isSupported {
                Button("Scan desktop invitation", systemImage: "qrcode.viewfinder") { Task { await scan() } }
                    .disabled(working).accessibilityIdentifier("pair.desktop.scan")
            }
            if cameraDenied {
                NoticeBar(text: String(localized: "Camera access is unavailable. Paste the invitation or allow camera access in Settings."))
                Button("Open Settings") { if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) } }
            }
            Section("Paste invitation") {
                TextField("Paste invitation", text: $invitation, axis: .vertical)
                    .lineLimit(3...6).textInputAutocapitalization(.never).autocorrectionDisabled()
                    .disabled(working).accessibilityIdentifier("pair.desktop.invitation")
                Button("Send device request") { attempt = UUID() }
                    .disabled(invitation.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || working || delivery != nil)
                    .accessibilityIdentifier("pair.desktop.send")
            }
            if working { ProgressView().accessibilityIdentifier("pair.desktop.working") }
            if needsLogin {
                NoticeBar(text: String(localized: "Sign in again to unlock your encrypted workspace on this iPhone."))
                Button("Continue with Internet Identity") { Task { await model.login() } }.disabled(working || model.busy)
            }
            if failed {
                NoticeBar(text: String(localized: "Device request delivery is unconfirmed. Check your connection and the desktop before retrying."))
                Button("Check authorization") { Task { await refresh() } }.disabled(working)
            }
            if let delivery {
                Section("Waiting for approval") {
                    Text("Compare the device ID on both screens and approve on your desktop.")
                    Text(groupedFingerprint(delivery.deviceId)).font(.caption.monospaced()).textSelection(.enabled)
                        .environment(\.layoutDirection, .leftToRight).accessibilityIdentifier("pair.desktop.device")
                    LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(delivery.expiresAt) / 1000), style: .relative) }
                }
            }
        }.privateScreen().navigationTitle("Scan desktop invitation").navigationBarTitleDisplayMode(.inline)
            .onChange(of: invitation) { delivery = nil; failed = false }
            .sheet(isPresented: $scanning) {
                QRCodeScanner { text in
                    invitation = text; delivery = nil; scanning = false; attempt = UUID()
                }
            }
            .task { await scan() }
            // Notices the desktop's decision without being asked.
            .task(id: delivery?.deviceId) {
                guard delivery != nil else { return }
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(5))
                    guard !Task.isCancelled, !needsLogin else { return }
                    await check()
                }
            }
            .task(id: attempt) {
                guard attempt != nil, let engine = model.engine else { return }
                let epoch = model.epoch
                let token = invitation.trimmingCharacters(in: .whitespacesAndNewlines)
                working = true; failed = false; delivery = nil
                defer { working = false }
                do {
                    let result: Delivery = try model.decode(try await engine.joinDeviceInvitation(invitation: token))
                    guard !Task.isCancelled, epoch == model.epoch else { return }
                    delivery = result
                } catch {
                    guard !Task.isCancelled, epoch == model.epoch else { return }
                    failed = true
                }
            }
    }

    private func scan() async {
        guard DataScannerViewController.isSupported else { return }
        let allowed = await AVCaptureDevice.requestAccess(for: .video)
        guard !Task.isCancelled else { return }
        cameraDenied = !allowed || !DataScannerViewController.isAvailable
        if !cameraDenied { scanning = true }
    }

    private func refresh() async {
        guard !working else { return }
        working = true; defer { working = false }
        await check()
    }

    private func check() async {
        guard let engine = model.engine else { return }
        let epoch = model.epoch
        do {
            _ = try await engine.refreshAccount()
            guard epoch == model.epoch else { return }
            needsLogin = false
        }
        catch {
            guard epoch == model.epoch else { return }
            if let failure = error as? MobileError, case .Failure(let code) = failure, code == "AUTH_REQUIRED" {
                needsLogin = true
            } else { model.reportBackground(error) }
        }
        if epoch == model.epoch { await model.updateSession() }
    }
}
