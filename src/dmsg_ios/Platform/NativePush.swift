import Foundation
import Observation
import OSLog
import UIKit
import UserNotifications
@preconcurrency import DMsgCore

extension Notification.Name {
    static let dmsgPushToken = Notification.Name("dmsg.push-token")
    static let dmsgPushRegistrationFailed = Notification.Name("dmsg.push-registration-failed")
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
        let value = token.map { String(format: "%02x", $0) }.joined()
        // UIKit also returns the existing token when registration is requested
        // again. Only a changed token needs another service registration.
        guard Self.token != value else { return }
        Self.token = value
        NotificationCenter.default.post(name: .dmsgPushToken, object: nil)
    }
    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        NotificationCenter.default.post(name: .dmsgPushRegistrationFailed, object: nil)
    }
    // Keep the completion handlers explicit: the async delegate bridge can
    // finish on a cooperative worker, where UIKit's snapshot update aborts.
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping @Sendable () -> Void) {
        let route = NotificationRoute.parse(response.notification.request.content.userInfo["dmsg"])
        DispatchQueue.main.async {
            if let route {
                Self.pendingRoute = route
                NotificationCenter.default.post(name: .dmsgPushOpen, object: route)
            }
            completionHandler()
        }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping @Sendable (UNNotificationPresentationOptions) -> Void) {
        let test = notification.request.identifier == "dmsg-test"
        DispatchQueue.main.async {
            if !test { NotificationCenter.default.post(name: .dmsgPushReceived, object: nil) }
            completionHandler(test ? [.banner] : [])
        }
    }
}

@MainActor @Observable
final class NativePush {
    struct Registered: Decodable { let route: String; let expiresAt: UInt64; let revision: UInt64? }

    /// `enabled` is read once for migration; core preferences are the authority.
    struct Subscription: Codable {
        var enabled: Bool?; var route: String?; var token: String?; var expires: UInt64?; var revision: UInt64?; var unified: Bool?
        /// A failed notification-settings sync does not invalidate the APNs lease.
        func current(token: String, now: UInt64) -> Bool {
            self.token == token && route != nil && revision != nil && (expires ?? 0) > now + 86_400_000
        }

        /// Automatic refresh reuses the lease. An explicit retry also repairs a
        /// lease removed or superseded remotely before its local expiry.
        @MainActor
        mutating func renew(token: String, now: UInt64, retry: Bool,
                            register: () async throws -> Registered) async throws -> Bool {
            guard retry || !current(token: token, now: now) else { return false }
            let registered = try await register()
            try Task.checkCancellation()
            route = registered.route; self.token = token; expires = registered.expiresAt
            revision = registered.revision; unified = false
            return true
        }
    }
    enum Registration: Equatable {
        case idle, waiting, enabled, denied, unsupported, failed(String)

        var message: String? {
            switch self {
            case .idle: nil
            case .waiting: String(localized: "Waiting for notification registration…")
            case .enabled: String(localized: "Notifications enabled")
            case .denied: String(localized: "Notifications are disabled in system settings.")
            case .unsupported: NoticeCopy.text("noticeRemote")
            case .failed(let message): message
            }
        }
    }
    private var subscriptions: [String: Subscription] = [:]
    /// Keep an explicit retry across the asynchronous UIKit token callback.
    private var pendingRetries: Set<String> = []
    private var enabledAccounts: [String: Bool] = [:]
    private var root: URL?
    private(set) var working = false
    private var idleWaiters: [CheckedContinuation<Void, Never>] = []
    private(set) var registration = Registration.idle
    private static let logger = Logger(subsystem: "net.dmsg.ios", category: "notifications")
    private static var retryMessage: String { String(localized: "Notification registration needs retrying. Foreground sync remains available.") }
    func registrationFailed() {
        guard DMsgAppDelegate.token == nil else { return }
        registration = .failed(Self.retryMessage)
    }
    func forget(_ account: String) throws { subscriptions.removeValue(forKey: account); enabledAccounts.removeValue(forKey: account); pendingRetries.remove(account); try save() }
    func enabled(_ account: String?) -> Bool { account.flatMap { enabledAccounts[$0] } ?? false }
    func account(for route: String) -> String? { subscriptions.first { $0.value.route == route }?.key }
    func load(_ files: PrivateFiles) throws {
        guard root != files.root else { return }; root = files.root; subscriptions = [:]; enabledAccounts = [:]; pendingRetries = []
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
        if allowed, DMsgAppDelegate.token == nil { UIApplication.shared.registerForRemoteNotifications() }
        if !allowed { registration = .denied }
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
    func refresh(_ model: SessionModel, retryRegistration: Bool = false) async {
        #if DEBUG
        if model.uiTesting { await refreshFixture(model, retryRegistration: retryRegistration); return }
        #endif
        while working { await withCheckedContinuation { idleWaiters.append($0) } }
        guard !Task.isCancelled, model.unlocked, let engine = model.engine, let files = model.files, let account = model.session?.account else { return }
        let epoch = model.epoch
        working = true; registration = .idle; defer {
            working = false; let waiting = idleWaiters; idleWaiters.removeAll(); for waiter in waiting { waiter.resume() }
        }
        var stage = "preferences"
        do {
            try load(files)
            if retryRegistration { pendingRetries.insert(account) }
            let preference = try await preferences(model, account: account)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            var subscription = subscriptions[account] ?? Subscription()
            if preference.account?.enabled != true {
                pendingRetries.remove(account)
                if subscription.route != nil {
                    _ = try await engine.unregisterPush(installation: files.installation)
                    subscription.route = nil; subscription.token = nil; subscription.expires = nil
                    subscription.revision = nil; subscription.unified = nil
                    subscriptions[account] = subscription; try save()
                }
                await model.notifications.refresh(model, receive: true)
                return
            }
            let permission = await UNUserNotificationCenter.current().notificationSettings()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            guard [.authorized, .provisional, .ephemeral].contains(permission.authorizationStatus) else {
                registration = .denied; return
            }
            guard let token = DMsgAppDelegate.token else {
                registration = .waiting
                UIApplication.shared.registerForRemoteNotifications()
                return
            }
            stage = "apple_registration"
            let retryApple = pendingRetries.remove(account) != nil
            let renewed = try await subscription.renew(token: token, now: NotificationTime.now, retry: retryApple) {
                let environment = Bundle.main.object(forInfoDictionaryKey: "DMsgAPNSEnvironment") as? String == "development" ? "sandbox" : "production"
                let result = try await model.apple.registerPush(engine: engine, files: files, account: account, token: token, environment: environment)
                guard epoch == model.epoch else { throw CancellationError() }
                return try model.decode(result)
            }
            if renewed {
                subscriptions[account] = subscription; try save()
            }
            // A settings change can arrive while App Attest/network work waits.
            // Preserve that newer preference, then unregister before sign-out proceeds.
            stage = "preferences"
            let current = try await preferences(model, account: account)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            if current.account?.enabled != true {
                _ = try await engine.unregisterPush(installation: files.installation)
                subscription.route = nil; subscription.token = nil; subscription.expires = nil
                subscription.revision = nil; subscription.unified = nil
                subscriptions[account] = subscription; try save()
            } else if let revision = subscription.revision {
                if subscription.unified != true || retryRegistration || current.registrationRequired == true {
                    stage = "notification_registration"
                    let result = try await engine.notificationRegisterApple(installation: files.installation, revision: revision, language: Bundle.main.preferredLocalizations.first ?? "en")
                    guard epoch == model.epoch else { return }
                    subscription.unified = result.json != "null"
                    subscriptions[account] = subscription; try save()
                }
                registration = subscription.unified == true ? .enabled : .unsupported
            } else { registration = .unsupported }
            await model.notifications.refresh(model, receive: true)
        } catch {
            guard epoch == model.epoch, !Task.isCancelled else { return }
            // Fixed stage names and allowlisted codes only; never log tokens,
            // account IDs, assertions or the error's potentially private text.
            let codes: Set<String> = ["APP_INTEGRITY_UNAVAILABLE", "AUTH_REQUIRED", "DEVICE_NOT_APPROVED", "FORBIDDEN", "VERSION_CONFLICT", "POLICY_STALE", "NETWORK_UNAVAILABLE", "UNAVAILABLE", "INTEGRITY_FAILED", "INVALID_INPUT", "QUOTA_EXCEEDED", "UNSUPPORTED_PROTOCOL", "LOCAL_STORAGE_UNAVAILABLE"]
            let code: String
            if case MobileError.Failure(let value) = error { code = codes.contains(value) ? value : "CORE_ERROR" }
            else { code = error is DecodingError ? "INVALID_RESPONSE" : "PLATFORM_ERROR" }
            Self.logger.error("registration_failed stage=\(stage, privacy: .public) code=\(code, privacy: .public)")
            if case MobileError.Failure(code: "APP_INTEGRITY_UNAVAILABLE") = error {
                registration = .failed(String(localized: "App verification is unavailable on this device. Your saved content remains available."))
            } else { registration = .failed(Self.retryMessage) }
            model.reportBackground(error)
        }
    }

    #if DEBUG
    private func refreshFixture(_ model: SessionModel, retryRegistration: Bool) async {
        let service = ProcessInfo.processInfo.arguments.contains("--ui-reminder-services")
        guard service || ProcessInfo.processInfo.arguments.contains("--ui-push-retry") else { return }
        guard !working else { return }
        if service && !retryRegistration { registration = .enabled; return }
        if !retryRegistration {
            if registration == .idle {
                registration = .failed(Self.retryMessage)
                if var value = model.notifications.preferences {
                    value.globalPendingSync = true
                    model.notifications.saveFixture(value)
                }
            }
            return
        }
        working = true; registration = .idle
        defer { working = false }
        try? await Task.sleep(for: .seconds(4))
        guard !Task.isCancelled else { return }
        if var value = model.notifications.preferences {
            value.globalPendingSync = false
            value.registrationRequired = false
            let revision = value.account?.revision ?? 0
            value.account?.remoteRevision = revision
            model.notifications.saveFixture(value)
        }
        registration = .enabled
    }
    #endif
}
