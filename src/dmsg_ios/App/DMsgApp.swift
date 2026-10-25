import SwiftUI

@main
struct DMsgApp: App {
    @State private var model = SessionModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView().environment(model).tint(Theme.forest)
                .task { await model.start() }
                .task(id: model.unlocked && model.automaticSyncEnabled && scenePhase == .active) {
                    guard model.automaticSyncEnabled, model.unlocked, scenePhase == .active else { return }
                    while !Task.isCancelled {
                        await model.optedInSyncPass()
                        do { try await Task.sleep(for: .seconds(30)) } catch { break }
                    }
                }
                .onChange(of: scenePhase) { _, phase in
                    switch phase {
                    case .inactive: model.inactive()
                    case .background: Task { await model.background() }
                    case .active: Task { await model.foreground() }
                    @unknown default: model.inactive()
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.protectedDataWillBecomeUnavailableNotification)) { _ in
                    model.protectedDataUnavailable()
                }
                .onReceive(Timer.publish(every: 30, on: .main, in: .common).autoconnect()) { _ in
                    Task { await model.idleTick() }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.userDidTakeScreenshotNotification)) { _ in
                    if model.unlocked { model.notice = String(localized: "Screenshots may contain private information.") }
                }
        }
    }
}
