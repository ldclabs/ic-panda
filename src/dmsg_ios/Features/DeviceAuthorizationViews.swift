import SwiftUI
@preconcurrency import DMsgCore
import CoreImage.CIFilterBuiltins
import VisionKit
import AVFoundation

/// Four-character groups are easier to compare between two screens.
func groupedFingerprint(_ value: String) -> String {
    stride(from: 0, to: value.count, by: 4).map { start in
        let from = value.index(value.startIndex, offsetBy: start)
        return String(value[from..<(value.index(from, offsetBy: 4, limitedBy: value.endIndex) ?? value.endIndex)])
    }.joined(separator: " ")
}

/// The request stays current and the screen notices the approval by itself:
/// nothing here asks the person to check or to start again.
struct PairView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var token: String?
    @State private var expires = Date.distantPast
    @State private var qrImage: UIImage?
    @State private var needsLogin = false
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
                FingerprintRow(title: "Device ID", value: groupedFingerprint(model.session?.deviceId ?? ""))
                LabeledContent("Expires") { Text(expires, style: .relative) }
                ShareLink(item: token) { SettingsActionLabel("Share pairing request", systemImage: "square.and.arrow.up") }
                SettingsAction("Copy pairing request") { model.clipboard.copy(token); model.notice = String(localized: "Copied for 30 seconds") }
            } else if model.engine != nil { ProgressView().frame(maxWidth: .infinity) }
            if needsLogin {
                NoticeBar(text: String(localized: "Sign in again to unlock your encrypted workspace on this iPhone."))
                SettingsAction("Continue with Internet Identity") { Task { await model.login(); needsLogin = false } }.disabled(model.busy)
            }
            NoticeBar(text: String(localized: "On the authorized device, open Approvals and review this device's full fingerprint, role and permissions. Scanning never approves automatically."))
            NavigationLink("All devices lost?") { RecoveryView() }
        }.privateScreen().navigationTitle("Join account")
            .task(id: model.epoch) { await watch() }
            .onChange(of: model.session?.state) {
                if let state = model.session?.state, ["logged_out", "locked"].contains(state) { dismiss() }
            }
    }

    private func watch() async {
        guard model.engine != nil else { return }
        await request()
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled else { return }
            if expires <= .now { await request() }
            if !needsLogin { await refresh() }
        }
    }

    private func request() async {
        do {
            if let result = try await model.engine?.pairRequest() {
                // Pairing is available before a content root is opened.
                token = try JSONDecoder().decode(String.self, from: Data(result.json.utf8)); expires = .now.addingTimeInterval(270)
                qrImage = token.flatMap(QRCode.image)
            }
        } catch { model.reportBackground(error) }
    }

    private func refresh() async {
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
            } else { model.reportBackground(error) }
        }
        // An identity transition invalidates the old engine handle even on
        // failure. Adopt its current state so retry/sign-in can work again.
        if epoch == model.epoch { await model.updateSession() }
    }
}

/// Scan or paste a request, then decide on the approval screen, which shows the
/// device's full fingerprint, role and permissions. Nothing is authorized before that.
struct PairApprovalView: View {
    @Environment(SessionModel.self) private var model
    @State private var token = ""
    @State private var created: ApprovalView?
    @State private var scanning = false
    @State private var working = false
    struct PairPreview: Decodable { let deviceId: String }
    private var clean: String { token.trimmingCharacters(in: .whitespacesAndNewlines) }
    var body: some View {
        Form {
            Section("Device request") { TextField("Paste pairing request", text: $token, axis: .vertical).lineLimit(3...6).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
            if DataScannerViewController.isSupported {
                SettingsAction("Scan device request", systemImage: "qrcode.viewfinder") { Task {
                    if await AVCaptureDevice.requestAccess(for: .video) { scanning = true }
                } }
            }
            SettingsAction("Review request", prominent: true) { Task { await review() } }.disabled(clean.isEmpty)
            if working { ProgressView() }
            NoticeBar(text: String(localized: "This creates a pending request. It does not authorize the device."))
        }.disabled(working).privateScreen().navigationTitle("Add device")
            .sheet(isPresented: $scanning) { QRCodeScanner { text in token = text; scanning = false; Task { await review() } } }
            .navigationDestination(isPresented: Binding(get: { created != nil }, set: { if !$0 { created = nil } })) {
                if let created { ApprovalDetailView(request: created) }
            }
    }

    private func review() async {
        guard !working, let engine = model.engine else { return }
        let epoch = model.epoch, input = clean
        working = true; defer { working = false }
        do {
            let inspected: PairPreview = try model.decode(try await engine.inspectPairingRequest(token: input))
            guard epoch == model.epoch, input == clean else { return }
            let result = try await engine.approvePairingRequest(token: input, device: inspected.deviceId)
            guard epoch == model.epoch, input == clean else { return }
            created = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
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
