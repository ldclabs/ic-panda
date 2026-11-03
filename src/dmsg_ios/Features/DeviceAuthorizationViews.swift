import SwiftUI
@preconcurrency import DMsgCore
import CoreImage.CIFilterBuiltins
import VisionKit
import AVFoundation

struct PairView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var token: String?
    @State private var expires = Date.distantPast
    @State private var qrImage: UIImage?
    @State private var needsLogin = false
    @State private var refreshing = false
    var body: some View {
        List {
            HStack(spacing: 12) {
                ObjectIcon(name: "iphone")
                VStack(alignment: .leading, spacing: 4) {
                    Text("Authorize this iPhone").font(.title2.bold())
                    Text("Your existing device makes the final decision.").font(.subheadline).foregroundStyle(.secondary)
                }
            }.listRowBackground(Color.clear)
            NavigationLink { DesktopPairingView() } label: {
                Label("Scan desktop invitation", systemImage: "qrcode.viewfinder")
            }.accessibilityIdentifier("pair.desktop")
            if let token {
                if let image = qrImage { Image(uiImage: image).interpolation(.none).resizable().scaledToFit().frame(maxWidth: 240).frame(maxWidth: .infinity).accessibilityLabel("Device pairing request QR code") }
                Text(model.session?.deviceId ?? "").font(.caption.monospaced()).textSelection(.enabled)
                ShareLink(item: token) { SettingsActionLabel("Share pairing request", systemImage: "square.and.arrow.up") }
                SettingsAction("Copy pairing request") { model.clipboard.copy(token) }
                Text(expires, style: .relative).font(.caption).foregroundStyle(.secondary)
            }
            SettingsAction("Check authorization") { Task { await refresh() } }.disabled(refreshing || model.busy)
            if needsLogin {
                NoticeBar(text: String(localized: "Sign in again to unlock your encrypted workspace on this iPhone."))
                SettingsAction("Continue with Internet Identity") { Task { await model.login() } }.disabled(refreshing || model.busy)
            }
            SettingsAction("Create a new pairing request") { Task { await request() } }
            NoticeBar(text: String(localized: "On the authorized device, open Approvals and review this device's full fingerprint, role and permissions. Scanning never approves automatically."))
            NavigationLink("All devices lost?") { RecoveryView() }
        }.privateScreen().navigationTitle("Join account")
            .onChange(of: model.session?.state) {
                if let state = model.session?.state, ["logged_out", "locked"].contains(state) { dismiss() }
            }
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
        guard !refreshing else { return }
        refreshing = true; defer { refreshing = false }
        let epoch = model.epoch
        do {
            _ = try await model.engine?.refreshAccount()
            guard epoch == model.epoch else { return }
            needsLogin = false
        }
        catch {
            guard epoch == model.epoch else { return }
            if let failure = error as? MobileError, case .Failure(let code) = failure, code == "AUTH_REQUIRED" {
                needsLogin = true
            } else { model.report(error) }
        }
        // An identity transition invalidates the old engine handle even on
        // failure. Adopt its current state so retry/sign-in can work again.
        if epoch == model.epoch { await model.updateSession() }
    }
}

struct PairApprovalView: View {
    @Environment(SessionModel.self) private var model
    @State private var token = ""
    @State private var device = ""
    @State private var created: ApprovalView?
    @State private var scanning = false
    @State private var preview: DisplayValue?
    @State private var working = false
    @State private var inspected = ""
    struct PairPreview: Decodable { let deviceId: String }
    var body: some View {
        Form {
            Section("Device request") { TextField("Paste pairing request", text: $token, axis: .vertical).lineLimit(3...6).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
            if DataScannerViewController.isSupported {
                SettingsAction("Scan device request", systemImage: "qrcode.viewfinder") { Task {
                    if await AVCaptureDevice.requestAccess(for: .video) { scanning = true }
                } }
            }
            SettingsAction("Inspect pairing request") { Task { await inspect() } }.disabled(token.isEmpty)
            if let preview { Section("Requested permissions") { CanonicalDetails(value: preview) } }
            Section("Verify the full device fingerprint") { TextField("Device ID", text: $device, axis: .vertical).font(.body.monospaced()).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
            SettingsAction("Review in approval center", prominent: true) { Task { await create() } }.disabled(token.isEmpty || device.count != 64 || preview == nil || inspected != token)
            NoticeBar(text: String(localized: "This creates a pending request. It does not authorize the device."))
        }.disabled(working).privateScreen().navigationTitle("Add device")
            .onChange(of: token) { _, _ in preview = nil; device = ""; inspected = "" }
            .sheet(isPresented: $scanning) { QRCodeScanner { text in token = text; scanning = false; Task { await inspect() } } }
            .navigationDestination(isPresented: Binding(get: { created != nil }, set: { if !$0 { created = nil } })) {
                if let created { ApprovalDetailView(request: created) }
            }
    }

    private func inspect() async {
        guard !working else { return }; working = true; defer { working = false }
        let epoch = model.epoch, input = token
        do {
            guard let result = try await model.engine?.inspectPairingRequest(token: input) else { return }
            guard epoch == model.epoch, input == token else { return }
            inspected = input
            let request: PairPreview = try model.decode(result)
            device = request.deviceId; preview = try model.decode(result)
        } catch { if epoch == model.epoch { preview = nil; device = ""; model.report(error) } }
    }

    private func create() async {
        guard !working, preview != nil, inspected == token else { return }
        let epoch = model.epoch; working = true; defer { working = false }
        do { if let result = try await model.engine?.approvePairingRequest(token: token, device: device), epoch == model.epoch { created = try model.decode(result) } }
        catch { if epoch == model.epoch { model.report(error) } }
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
