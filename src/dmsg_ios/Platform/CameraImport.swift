import AVFoundation
import SwiftUI

extension SessionModel {
    var cameraAvailable: Bool {
        #if DEBUG
        if uiTesting { return !Set(ProcessInfo.processInfo.arguments).isDisjoint(with: ["--ui-camera", "--ui-camera-denied"]) }
        #endif
        return CameraCapture.isAvailable
    }
}

extension View {
    /// Asks for the camera when `requested` turns on, and hands back what was captured.
    func cameraImport(requested: Binding<Bool>, finish: @escaping (Result<PhotoImport, Error>) async -> Void) -> some View {
        modifier(CameraImport(requested: requested, finish: finish))
    }
}

private struct CameraImport: ViewModifier {
    @Binding var requested: Bool
    let finish: (Result<PhotoImport, Error>) async -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.openURL) private var openURL
    @State private var capturing = false
    @State private var denied = false

    func body(content: Content) -> some View {
        content
            .onChange(of: requested) { _, value in
                guard value else { return }
                requested = false
                Task { await open() }
            }
            .fullScreenCover(isPresented: $capturing) {
                CameraCapture { result in
                    capturing = false
                    if let result { Task { await finish(result) } }
                }.ignoresSafeArea().task {
                    // The camera's controls never reach the app, so a recording would run into
                    // the idle lock and be lost. Bounded so a forgotten camera still locks.
                    for _ in 0..<60 {
                        model.activity()
                        do { try await Task.sleep(for: .seconds(30)) } catch { return }
                    }
                }
            }
            .alert("Camera access is unavailable. Allow camera access in Settings to take photos and videos.", isPresented: $denied) {
                Button("Open Settings") { if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) } }
                Button("Cancel", role: .cancel) {}
            }
    }

    private func open() async {
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-camera-denied") { denied = true }
            else { await finish(Result { try model.cameraCaptureFixture() }) }
            return
        }
        #endif
        if await AVCaptureDevice.requestAccess(for: .video) { capturing = true } else { denied = true }
    }
}
