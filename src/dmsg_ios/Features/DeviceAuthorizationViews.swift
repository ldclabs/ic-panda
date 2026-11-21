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
    @State private var paused = false
    @State private var checking = false
    private var preparing: Bool { model.session?.state == "root_required" }
    var body: some View {
        List {
            HStack(spacing: 12) {
                ObjectIcon(name: "iphone")
                VStack(alignment: .leading, spacing: 4) {
                    Text("Authorize this iPhone").font(.title2.bold())
                    Text("Your existing device makes the final decision.").font(.subheadline).foregroundStyle(.secondary)
                }
            }.listRowBackground(Color.clear)
            if preparing {
                NoticeBar(text: String(localized: "Approved. Preparing your vault…"))
            } else {
                NavigationLink { DesktopPairingView() } label: {
                    Label("Scan desktop invitation", systemImage: "qrcode.viewfinder")
                }.accessibilityIdentifier("pair.desktop")
                if let token {
                    if let image = qrImage { Image(uiImage: image).interpolation(.none).resizable().scaledToFit().frame(maxWidth: 240).frame(maxWidth: .infinity).accessibilityLabel("Device pairing request QR code") }
                    FingerprintRow(title: "Device ID", value: groupedFingerprint(model.session?.deviceId ?? ""))
                    LabeledContent("Expires") { Text(expires, style: .relative) }
                    ShareLink(item: token) { SettingsActionLabel("Share pairing request", systemImage: "square.and.arrow.up") }
                    SettingsAction("Refresh") { Task { await request() } }
                    SettingsAction("Copy pairing request") { model.clipboard.copy(token); model.notice = String(localized: "Copied for 30 seconds") }
                } else if model.engine != nil { ProgressView().frame(maxWidth: .infinity) }
            }
            if paused {
                SettingsAction("Unlock workspace") { Task { paused = false; await refresh() } }.disabled(checking)
            }
            if needsLogin {
                NoticeBar(text: String(localized: "Sign in again to unlock your encrypted workspace on this iPhone."))
                SettingsAction("Continue with Internet Identity") { Task { await model.login(usingInternetIdentity: true); needsLogin = false } }.disabled(model.busy)
            }
            if !preparing { NoticeBar(text: String(localized: "On the authorized device, open Approvals and review this device's full fingerprint, role and permissions. Scanning never approves automatically.")) }
            NavigationLink("All devices lost?") { RecoveryView() }
        }.privateScreen().navigationTitle("Join account")
            .task(id: model.epoch) { await watch() }
            .onChange(of: model.session?.state) {
                if model.session?.state == "logged_out" { dismiss() }
            }
    }

    private func watch() async {
        guard model.engine != nil else { return }
        if model.session?.state == "pairing_required" { await request() }
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled else { return }
            if model.session?.state == "pairing_required" && !paused && expires <= .now { await request() }
            if !needsLogin && !paused && ["pairing_required", "root_required"].contains(model.session?.state ?? "") { await refresh() }
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
        guard !checking else { return }
        checking = true; defer { checking = false }
        let epoch = model.epoch
        do {
            _ = try await model.engine?.refreshAccount()
            guard epoch == model.epoch else { return }
            needsLogin = false
        }
        catch {
            guard epoch == model.epoch else { return }
            if let failure = error as? MobileError, case .Failure(let code) = failure, ["AUTH_REQUIRED", "LOGIN_EXPIRED"].contains(code) {
                needsLogin = true
            } else if pairingNeedsManualUnlock(error) { paused = true; model.report(error) }
            else { model.reportBackground(error) }
        }
        // An identity transition invalidates the old engine handle even on
        // failure. Adopt its current state so retry/sign-in can work again.
        if epoch == model.epoch { await model.updateSession() }
    }
}

func pairingNeedsManualUnlock(_ error: Error) -> Bool {
    guard let failure = error as? MobileError, case .Failure(let code) = failure else { return false }
    return ["CANCELLED", "LOCKED", "FORBIDDEN", "INTEGRITY_FAILED", "AUTHENTICATION_TIMEOUT"].contains(code)
        || code.hasPrefix("HARDWARE_") || code.hasPrefix("BIOMETRIC_") || code.hasPrefix("DEVICE_CREDENTIAL_")
}

struct EnrollmentRequest: Decodable {
    let token, deviceId, keyId: String
}

struct DeviceRequestCode: View {
    let token: String
    let deviceId: String
    var keyId: String?
    var body: some View {
        if let image = QRCode.image(token) {
            Image(uiImage: image).interpolation(.none).resizable().scaledToFit().frame(maxWidth: 240)
                .frame(maxWidth: .infinity).accessibilityLabel("Device pairing request QR code")
        }
        FingerprintRow(title: "Device ID", value: groupedFingerprint(deviceId))
        if let keyId { FingerprintRow(title: "Key fingerprint", value: groupedFingerprint(keyId)) }
        ShareLink(item: token) { SettingsActionLabel("Share request", systemImage: "square.and.arrow.up") }
    }
}

/// Scan or paste a request, then decide on the approval screen, which shows the
/// device's full fingerprint, role and permissions. Nothing is authorized before that.
struct PairApprovalView: View {
    var enrollmentOnly = false
    @Environment(SessionModel.self) private var model
    @State private var token = ""
    @State private var created: ApprovalView?
    @State private var receipts = ApprovalReceipts()
    @State private var scanning = false
    @State private var working = false
    @State private var inspected: PairPreview?
    @State private var enrollment: EnrollmentRequest?
    @State private var visible = false
    struct PairPreview: Decodable {
        let deviceId, kind, route: String
        let approvers: [String]
        let canEnroll: Bool
        let keyId: String?
    }
    private func edit(_ value: String) {
        token = value; inspected = nil; enrollment = nil
    }
    private var clean: String { token.trimmingCharacters(in: .whitespacesAndNewlines) }
    var body: some View {
        Form {
            if inspected?.route != "handoff" {
                Section("Device request") { TextField("Paste pairing request", text: Binding(get: { token }, set: edit), axis: .vertical).lineLimit(3...6).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                if DataScannerViewController.isSupported {
                    SettingsAction("Scan device request", systemImage: "qrcode.viewfinder") { Task {
                        if await AVCaptureDevice.requestAccess(for: .video) { scanning = true }
                    } }
                }
                SettingsAction("Review request", prominent: true) { Task { await review() } }.disabled(clean.isEmpty || receipts["pair.\(clean)"] != nil)
                ApprovalReceiptRow(receipts: receipts, key: "pair.\(clean)") { created = $0 }
            }
            if let inspected {
                if inspected.route == "handoff" {
                    if enrollment == nil {
                        Section("Continue on another device") {
                            Text("Scan this code with an administrator device to review the request.")
                            DeviceRequestCode(token: clean, deviceId: inspected.deviceId, keyId: inspected.keyId)
                            if inspected.approvers.isEmpty {
                                Text("No eligible administrator device was found.")
                                NavigationLink("Account recovery") { RecoveryView() }
                            }
                            ForEach(inspected.approvers, id: \.self) { FingerprintRow(title: "Authorized device", value: [model.deviceLabel($0), groupedFingerprint($0)].compactMap { $0 }.joined(separator: "\n")) }
                        }
                    }
                    if let enrollment {
                        Section("Send to an administrator") {
                            Text("Scan this code with an administrator device to review the request.")
                            DeviceRequestCode(token: enrollment.token, deviceId: enrollment.deviceId, keyId: enrollment.keyId)
                            if inspected.kind == "device_pair" { Text("After key registration, refresh the request on the joining device and scan it again.") }
                        }
                    } else if inspected.canEnroll {
                        SettingsAction("Let this device approve future requests") { Task { await enroll() } }
                    }
                    SettingsAction("Back") { edit("") }
                } else if inspected.route == "refresh" { Text("Refresh the request on the joining device and scan it again.") }
                else if inspected.route == "done" { Label("Completed", systemImage: "checkmark.circle") }
            }
            if working { ProgressView() }
            if inspected?.route != "handoff" { NoticeBar(text: String(localized: "This creates a pending request. It does not authorize the device.")) }
        }.disabled(working).privateScreen().navigationTitle(LocalizedStringKey(inspected?.route == "handoff" ? "Continue on another device" : enrollmentOnly ? "Review an enrollment request" : "Add device"))
            .onAppear { visible = true }.onDisappear { visible = false }
            .sheet(isPresented: $scanning) { QRCodeScanner { text in edit(text); scanning = false; Task { await review() } } }
            .approvalDestination($created, receipts: receipts, model: model)
    }

    private func review() async {
        guard !working, let engine = model.engine else { return }
        let epoch = model.epoch, input = clean
        working = true; defer { working = false }
        do {
            // The same pairing request opens its existing approval again, as on the desktop.
            if let kept = receipts["pair.\(input)"] { created = kept; return }
            let inspected: PairPreview = try model.decode(try await engine.inspectPairingRequest(token: input))
            guard visible, epoch == model.epoch, input == clean else { return }
            guard !enrollmentOnly || inspected.kind == "approval_key" else { throw MobileError.Failure(code: "UNSUPPORTED_PROTOCOL") }
            self.inspected = inspected
            guard inspected.route == "approve" else { return }
            let value = try await receipts.request("pair.\(input)", model: model) { engine in
                if inspected.kind == "approval_key" { return try await engine.approveApprovalKey(token: input, device: inspected.deviceId) }
                return try await engine.approvePairingRequest(token: input, device: inspected.deviceId)
            }
            guard let value, visible, epoch == model.epoch, input == clean else { return }
            created = value
        } catch {
            guard visible, epoch == model.epoch, input == clean else { return }
            if let failure = error as? MobileError, case .Failure(let code) = failure, ["EXPIRED", "VERSION_CONFLICT"].contains(code) {
                model.error = String(localized: "Refresh the request on the joining device and scan it again.")
            } else { model.report(error) }
        }
    }
    private func enroll() async {
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch, input = clean
        working = true; defer { working = false }
        do {
            let result = try await engine.approvalKeyRequest(biometryOnly: false)
            guard visible, epoch == model.epoch, input == clean else { return }
            enrollment = try model.decode(result)
        } catch { if visible && epoch == model.epoch { model.report(error) } }
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
