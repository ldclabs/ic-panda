import SwiftUI

@main
struct DMsgApp: App {
    @UIApplicationDelegateAdaptor(DMsgAppDelegate.self) private var appDelegate
    @State private var model = SessionModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView().environment(model).tint(Theme.forest)
                #if DEBUG
                .preferredColorScheme(model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-dark") ? .dark : nil)
                #endif
                .task { await model.start() }
                .onOpenURL { url in
                    if let link = ConnectLink(url) { ConnectNavigation.shared.pending = link }
                    if url.scheme == "dmsg", url.host == "open", let destination = WorkspaceDestination(rawValue: url.lastPathComponent), url.query == nil, url.fragment == nil { IntentNavigation.shared.destination = destination }
                }
                .task(id: "push:\(model.session?.account ?? ""):\(model.session?.generation ?? 0)") {
                    if model.unlocked { await model.push.refresh(model) }
                }
                .onReceive(NotificationCenter.default.publisher(for: .dmsgPushToken)) { _ in Task { await model.push.refresh(model) } }
                .onReceive(NotificationCenter.default.publisher(for: .dmsgPushOpen)) { notification in
                    if let route = notification.object as? String { Task { await model.openPushRoute(route) } }
                }
                .task(id: "store:\(model.session?.account ?? ""):\(model.unlocked)") {
                    if model.unlocked { await model.purchases.listen(model) }
                }
                .task(id: "\(scenePhase == .active):\(model.session?.account ?? ""):\(model.session?.generation ?? 0)") {
                    guard scenePhase == .active, model.session?.account != nil else { return }
                    while !Task.isCancelled {
                        await model.refreshRecoveryWarning()
                        do { try await Task.sleep(for: .seconds(60)) } catch { break }
                    }
                }
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
                    case .background: model.background()
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
