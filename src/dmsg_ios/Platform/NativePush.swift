import Foundation
import Observation
import UIKit
import UserNotifications
@preconcurrency import DMsgCore

extension Notification.Name {
    static let dmsgPushToken = Notification.Name("dmsg.push-token")
    static let dmsgPushOpen = Notification.Name("dmsg.push-open")
}

@MainActor
final class DMsgAppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    static var token: String?
    static var pendingRoute: String?
    private static var backgroundCompletion: (() -> Void)?
    static func finishBackgroundEvents() { let completion = backgroundCompletion; backgroundCompletion = nil; completion?() }
    func application(_ application: UIApplication, handleEventsForBackgroundURLSession identifier: String, completionHandler: @escaping () -> Void) {
        guard identifier == BackgroundTransfers.identifier else { completionHandler(); return }
        Self.backgroundCompletion = completionHandler; BackgroundTransfers.shared.reconnect()
    }
    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        BackgroundTransfers.shared.reconnect()
        return true
    }
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken token: Data) {
        Self.token = token.map { String(format: "%02x", $0) }.joined()
        NotificationCenter.default.post(name: .dmsgPushToken, object: nil)
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        if let payload = response.notification.request.content.userInfo["dmsg"] as? [String: Any], let route = payload["route"] as? String {
            await MainActor.run {
                Self.pendingRoute = route
                NotificationCenter.default.post(name: .dmsgPushOpen, object: route)
            }
        }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions { [.banner, .sound] }
}

@MainActor @Observable
final class NativePush {
    struct Subscription: Codable { var enabled: Bool; var route: String?; var token: String?; var expires: UInt64? }
    private var subscriptions: [String: Subscription] = [:]
    private var root: URL?
    private(set) var working = false
    private var idleWaiters: [CheckedContinuation<Void, Never>] = []
    private(set) var status: String?
    func forget(_ account: String) throws { subscriptions.removeValue(forKey: account); try save() }
    func enabled(_ account: String?) -> Bool { account.flatMap { subscriptions[$0]?.enabled } ?? false }
    func account(for route: String) -> String? { subscriptions.first { $0.value.route == route && $0.value.enabled }?.key }
    func load(_ files: PrivateFiles) throws {
        guard root != files.root else { return }; root = files.root
        let path = files.root.appending(path: "push-subscriptions.json")
        if FileManager.default.fileExists(atPath: path.path) { subscriptions = try JSONDecoder().decode([String: Subscription].self, from: Data(contentsOf: path)) }
    }
    private func save() throws {
        guard let root else { return }
        try JSONEncoder().encode(subscriptions).write(to: root.appending(path: "push-subscriptions.json"), options: [.atomic, .completeFileProtection])
    }
    func setEnabled(_ enabled: Bool, model: SessionModel) async {
        guard let account = model.session?.account, let files = model.files else { return }
        do {
            try load(files)
            if enabled {
                guard try await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) else {
                    status = String(localized: "Notifications are disabled in system settings."); return
                }
                UIApplication.shared.registerForRemoteNotifications()
            }
            var subscription = subscriptions[account] ?? Subscription(enabled: false)
            subscription.enabled = enabled; subscriptions[account] = subscription; try save()
            await refresh(model)
        } catch { model.report(error) }
    }
    func refresh(_ model: SessionModel) async {
        while working { await withCheckedContinuation { idleWaiters.append($0) } }
        guard model.unlocked, let engine = model.engine, let files = model.files, let account = model.session?.account else { return }
        let epoch = model.epoch
        working = true; defer {
            working = false; let waiting = idleWaiters; idleWaiters.removeAll(); for waiter in waiting { waiter.resume() }
        }
        do {
            try load(files)
            guard var subscription = subscriptions[account] else { return }
            if !subscription.enabled {
                if subscription.route != nil {
                    _ = try await engine.unregisterPush(installation: files.installation)
                    subscription.enabled = subscriptions[account]?.enabled == true
                    subscription.route = nil; subscription.token = nil; subscription.expires = nil
                    subscriptions[account] = subscription; try save()
                }
                return
            }
            UIApplication.shared.registerForRemoteNotifications()
            guard let token = DMsgAppDelegate.token else { status = String(localized: "Waiting for notification registration…"); return }
            if subscription.token == token, let expiry = subscription.expires, Double(expiry) > Date.now.timeIntervalSince1970 * 1000 + 86_400_000 { return }
            let environment = Bundle.main.object(forInfoDictionaryKey: "DMsgAPNSEnvironment") as? String == "development" ? "sandbox" : "production"
            struct Registration: Decodable { let route: String; let expiresAt: UInt64 }
            let result = try await model.apple.registerPush(engine: engine, files: files, account: account, token: token, environment: environment)
            guard epoch == model.epoch else { return }
            let registered: Registration = try model.decode(result)
            // A settings change can arrive while App Attest/network work waits.
            // Preserve that newer preference, then unregister before sign-out proceeds.
            subscription.enabled = subscriptions[account]?.enabled == true
            subscription.route = registered.route; subscription.token = token; subscription.expires = registered.expiresAt
            subscriptions[account] = subscription; try save()
            if !subscription.enabled {
                _ = try await engine.unregisterPush(installation: files.installation)
                subscription.enabled = subscriptions[account]?.enabled == true
                subscription.route = nil; subscription.token = nil; subscription.expires = nil
                subscriptions[account] = subscription; try save()
            } else { status = String(localized: "Notifications enabled") }
        } catch { if epoch == model.epoch { status = String(localized: "Notification registration needs retrying. Foreground sync remains available.") } }
    }
}
