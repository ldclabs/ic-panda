import Foundation
import Observation
import UIKit
import UserNotifications
@preconcurrency import DMsgCore

extension Notification.Name {
    static let dmsgPushToken = Notification.Name("dmsg.push-token")
    static let dmsgPushOpen = Notification.Name("dmsg.push-open")
    static let dmsgPushReceived = Notification.Name("dmsg.push-received")
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
        if let route = NotificationRoute.parse(response.notification.request.content.userInfo["dmsg"]) {
            await MainActor.run {
                Self.pendingRoute = route
                NotificationCenter.default.post(name: .dmsgPushOpen, object: route)
            }
        }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        if notification.request.identifier == "dmsg-test" { return [.banner] }
        await MainActor.run { NotificationCenter.default.post(name: .dmsgPushReceived, object: nil) }
        return []
    }
}

@MainActor @Observable
final class NativePush {
    /// `enabled` is read once for migration; core preferences are the authority.
    struct Subscription: Codable { var enabled: Bool?; var route: String?; var token: String?; var expires: UInt64?; var revision: UInt64?; var unified: Bool? }
    private var subscriptions: [String: Subscription] = [:]
    private var enabledAccounts: [String: Bool] = [:]
    private var root: URL?
    private(set) var working = false
    private var idleWaiters: [CheckedContinuation<Void, Never>] = []
    private(set) var status: String?
    func forget(_ account: String) throws { subscriptions.removeValue(forKey: account); enabledAccounts.removeValue(forKey: account); try save() }
    func enabled(_ account: String?) -> Bool { account.flatMap { enabledAccounts[$0] } ?? false }
    func account(for route: String) -> String? { subscriptions.first { $0.value.route == route }?.key }
    func load(_ files: PrivateFiles) throws {
        guard root != files.root else { return }; root = files.root; subscriptions = [:]; enabledAccounts = [:]
        let path = files.root.appending(path: "push-subscriptions.json")
        if FileManager.default.fileExists(atPath: path.path) { subscriptions = try JSONDecoder().decode([String: Subscription].self, from: Data(contentsOf: path)) }
    }
    private func save() throws {
        guard let root else { return }
        try JSONEncoder().encode(subscriptions).write(to: root.appending(path: "push-subscriptions.json"), options: [.atomic, .completeFileProtection])
    }
    func requestPermission() async throws -> Bool {
        let center = UNUserNotificationCenter.current()
        let current = await center.notificationSettings()
        let allowed: Bool
        if current.authorizationStatus == .notDetermined { allowed = try await center.requestAuthorization(options: [.alert, .sound, .badge]) }
        else { allowed = [.authorized, .provisional, .ephemeral].contains(current.authorizationStatus) }
        if allowed { UIApplication.shared.registerForRemoteNotifications() }
        else { status = String(localized: "Notifications are disabled in system settings.") }
        return allowed
    }
    private func preferences(_ model: SessionModel, account: String) async throws -> NoticePreferences {
        guard let engine = model.engine else { throw CocoaError(.userCancelled) }
        if let legacy = subscriptions[account]?.enabled {
            try await engine.notificationImportLegacy(selected: account, enabled: legacy)
            subscriptions[account]?.enabled = nil; try save()
        }
        let value: NoticePreferences = try model.decode(try await engine.notificationPreferences(selected: account))
        enabledAccounts[account] = value.account?.enabled == true
        return value
    }
    func setEnabled(_ enabled: Bool, model: SessionModel) async {
        guard let account = model.session?.account, let files = model.files, let engine = model.engine else { return }
        do {
            try load(files)
            var value = try await preferences(model, account: account)
            if enabled { _ = try await requestPermission() }
            if var settings = value.account {
                settings.enabled = enabled
                _ = try await engine.notificationSetAccount(selected: account, expected: settings.revision, settingsJson: NoticeEncoding.json(settings))
            }
            if enabled && value.global.revision == 0 {
                value.global.enabled = true
                _ = try await engine.notificationSetGlobal(expected: 0, settingsJson: NoticeEncoding.json(value.global))
            }
            enabledAccounts[account] = enabled
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
            let preference = try await preferences(model, account: account)
            var subscription = subscriptions[account] ?? Subscription()
            if preference.account?.enabled != true {
                if subscription.route != nil {
                    _ = try await engine.unregisterPush(installation: files.installation)
                    subscription.route = nil; subscription.token = nil; subscription.expires = nil
                    subscription.revision = nil; subscription.unified = nil
                    subscriptions[account] = subscription; try save()
                }
                _ = try? await engine.notificationRefresh()
                return
            }
            let permission = await UNUserNotificationCenter.current().notificationSettings()
            guard [.authorized, .provisional, .ephemeral].contains(permission.authorizationStatus) else {
                status = String(localized: "Notifications are disabled in system settings."); return
            }
            UIApplication.shared.registerForRemoteNotifications()
            guard let token = DMsgAppDelegate.token else { status = String(localized: "Waiting for notification registration…"); return }
            if subscription.token == token, subscription.unified == true, let expiry = subscription.expires, Double(expiry) > Date.now.timeIntervalSince1970 * 1000 + 86_400_000 {
                _ = try? await engine.notificationRefresh(); return
            }
            let environment = Bundle.main.object(forInfoDictionaryKey: "DMsgAPNSEnvironment") as? String == "development" ? "sandbox" : "production"
            struct Registration: Decodable { let route: String; let expiresAt: UInt64; let revision: UInt64? }
            let result = try await model.apple.registerPush(engine: engine, files: files, account: account, token: token, environment: environment)
            guard epoch == model.epoch else { return }
            let registered: Registration = try model.decode(result)
            // A settings change can arrive while App Attest/network work waits.
            // Preserve that newer preference, then unregister before sign-out proceeds.
            subscription.route = registered.route; subscription.token = token; subscription.expires = registered.expiresAt
            subscription.revision = registered.revision
            subscriptions[account] = subscription; try save()
            let current = try await preferences(model, account: account)
            if current.account?.enabled != true {
                _ = try await engine.unregisterPush(installation: files.installation)
                subscription.route = nil; subscription.token = nil; subscription.expires = nil
                subscription.revision = nil; subscription.unified = nil
                subscriptions[account] = subscription; try save()
            } else if let revision = registered.revision {
                let result = try await engine.notificationRegisterApple(installation: files.installation, revision: revision, language: Bundle.main.preferredLocalizations.first ?? "en")
                guard epoch == model.epoch else { return }
                subscription.unified = result.json != "null"
                subscriptions[account] = subscription; try save()
                let confirmed = try await preferences(model, account: account)
                status = subscription.unified == true ? (confirmed.pending || confirmed.conflict ? NoticeCopy.text("noticePending") : String(localized: "Notifications enabled")) : NoticeCopy.text("noticeRemote")
            } else { status = NoticeCopy.text("noticeRemote") }
            await model.notifications.refresh(model, receive: true)
        } catch { if epoch == model.epoch { status = String(localized: "Notification registration needs retrying. Foreground sync remains available.") } }
    }
}
