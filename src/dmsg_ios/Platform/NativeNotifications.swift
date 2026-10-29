import Foundation
import Observation
import UIKit
import UserNotifications
@preconcurrency import DMsgCore

/// Native presentation only. Classification, phases, preferences and budgets
/// come from the shared core. There is no second Swift notification policy.
@MainActor @Observable
final class NativeNotifications {
    private(set) var status: ReminderStatus?
    private(set) var preferences: NoticePreferences?
    private(set) var permission: UNAuthorizationStatus = .notDetermined
    private(set) var working = false
    private(set) var fixtureItems: [ReminderItem] = []
    var presented: ReminderPlan?
    private var wake: Task<Void, Never>?
    private var retry = false
    private var receiveAgain = false
    private var receiving = false
    private struct Viewport { let value: VisibleReminder; let active: @MainActor () -> Bool }
    private var visible: [UUID: Viewport] = [:]

    func clearVisibility() { visible.removeAll(); wake?.cancel(); wake = nil; presented = nil }

    func refresh(_ model: SessionModel, receive: Bool = false) async {
        guard !model.uiTesting else { return }
        guard let engine = model.engine else { return }
        if working { retry = true; receiveAgain = receiveAgain || receive; return }
        working = true
        let epoch = model.epoch
        if receive && !receiving {
            receiving = true
            Task {
                _ = try? await engine.notificationRefresh()
                receiving = false
                await refresh(model, receive: model.epoch != epoch)
            }
        }
        defer {
            working = false
            if retry { let receive = receiveAgain; retry = false; receiveAgain = false; Task { await self.refresh(model, receive: receive) } }
        }
        do {
            permission = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
            let nextPreferences: NoticePreferences = try model.decode(try await engine.notificationPreferences(selected: model.session?.account))
            let nextStatus: ReminderStatus = try model.decode(try await engine.notificationStatus())
            guard model.epoch == epoch, !Task.isCancelled else { return }
            preferences = nextPreferences; status = nextStatus
            let allowed = [.authorized, .provisional, .ephemeral].contains(permission)
            let foreground = UIApplication.shared.applicationState == .active && !model.concealed
            let batch: ReminderBatch = try model.decode(try await engine.notificationNext(context: NotificationPresentationContext(
                allowed: allowed, foreground: foreground, quietUntil: NotificationTime.quietUntil(nextPreferences.global), visible: foreground ? visible.values.filter { $0.active() }.map(\.value) : [])))
            guard model.epoch == epoch, !Task.isCancelled else { return }
            if !nextStatus.counts.unverified || nextStatus.badge > 0 || nextPreferences.global.badge == "off" {
                try? await UNUserNotificationCenter.current().setBadgeCount(Int(min(nextStatus.badge, UInt64(Int32.max))))
            }
            let center = UNUserNotificationCenter.current()
            center.removePendingNotificationRequests(withIdentifiers: batch.remove)
            center.removeDeliveredNotifications(withIdentifiers: batch.remove)
            for plan in batch.plans {
                guard model.epoch == epoch, !Task.isCancelled else { return }
                let current: NoticePreferences = try model.decode(try await engine.notificationPreferences(selected: plan.account))
                guard model.epoch == epoch, !Task.isCancelled else { return }
                if !current.global.enabled || current.account?.enabled != true || current.account?.categories[plan.category.rawValue]?.enabled != true {
                    try await engine.notificationResult(selected: plan.account, systemId: plan.systemId, deliveryId: plan.deliveryId, result: .suppressed)
                    continue
                }
                if plan.inApp {
                    let active = UIApplication.shared.applicationState == .active && !model.concealed
                    if active && (presented == nil || plan.category == .security || presented?.category != .security) {
                        model.notice = nil; presented = plan
                    }
                    try await engine.notificationResult(selected: plan.account, systemId: plan.systemId, deliveryId: plan.deliveryId, result: active ? .submitted : .suppressed)
                    continue
                }
                let content = UNMutableNotificationContent()
                content.title = "dMsg"; content.body = plan.body
                content.categoryIdentifier = "dmsg." + plan.category.rawValue
                content.threadIdentifier = plan.systemId
                content.userInfo = ["dmsg": ["version": 1, "local": plan.systemId]]
                if plan.sound { content.sound = .default }
                do {
                    try await center.add(UNNotificationRequest(identifier: plan.systemId, content: content, trigger: nil))
                    try await engine.notificationResult(selected: plan.account, systemId: plan.systemId, deliveryId: plan.deliveryId, result: .submitted)
                } catch {
                    try? await engine.notificationResult(selected: plan.account, systemId: plan.systemId, deliveryId: plan.deliveryId, result: .failed)
                }
            }
            wake?.cancel(); wake = nil
            if let due = batch.nextDue, foreground {
                wake = Task { [weak self, weak model] in
                    do { try await Task.sleep(for: .milliseconds(max(1, Int64(clamping: due) - Int64(clamping: NotificationTime.now)))) } catch { return }
                    guard let self, let model, model.epoch == epoch else { return }
                    await self.refresh(model)
                }
            }
        } catch {
            // A failed read preserves the last trusted counts and reminder state.
            // Explicit settings/registration actions show their own retry state.
        }
    }

    func observed(_ model: SessionModel, owner: UUID, target: ReminderTarget, watermark: UInt64?, active: @escaping @MainActor () -> Bool) async {
        guard model.unlocked, !model.concealed, UIApplication.shared.applicationState == .active,
              active(), let engine = model.engine, let account = model.session?.account else { return }
        let epoch = model.epoch
        do {
            let item: ReminderItem? = try model.decode(try await engine.notificationReference(selected: account, targetJson: NoticeEncoding.json(target)))
            guard let item, active(), model.epoch == epoch, model.unlocked, !model.concealed, UIApplication.shared.applicationState == .active else { return }
            if let watermark, UInt64(item.version).map({ $0 > watermark }) ?? true { return }
            if case .channel = target, watermark == nil { return }
            try await engine.notificationObserved(selected: account, id: item.id, expectedVersion: item.version)
            guard active(), model.epoch == epoch, UIApplication.shared.applicationState == .active, !model.concealed else { return }
            visible[owner] = Viewport(value: VisibleReminder(id: item.id, version: item.version), active: active)
            await refresh(model, receive: true)
        } catch { }
    }
    func hide(_ owner: UUID) { visible.removeValue(forKey: owner) }

    func installFixtures(account: String) {
        // --ui-notification-setup: this account has not opted in; another local account has.
        let setup = ProcessInfo.processInfo.arguments.contains("--ui-notification-setup")
        let categories = Dictionary(uniqueKeysWithValues: ReminderCategory.allCases.map { ($0.rawValue, NoticeCategoryPreference(enabled: true, sound: $0 == .security)) })
        preferences = NoticePreferences(global: NoticeGlobalPreference(revision: 1, enabled: true, pausedUntil: nil, quietHours: nil, securityException: false, badge: "all"), account: NoticeAccountPreference(revision: setup ? 0 : 1, enabled: !setup, categories: categories, mergeDevices: true, agentMail: [:], remoteRevision: setup ? 0 : 1), accountId: account, remoteSupported: true)
        let now = NotificationTime.now
        fixtureItems = [ReminderItem(id: String(repeating: "a", count: 64), account: account, target: .recovery(String(repeating: "b", count: 64)), version: "1", open: true, actionable: true, unread: nil, firstAt: now, updatedAt: now, deadline: now + 86_400_000, observed: "0", stage: 1, snoozedUntil: nil),
            ReminderItem(id: String(repeating: "c", count: 64), account: account, target: .task("conflicts:1"), version: "1", open: true, actionable: true, unread: nil, firstAt: now, updatedAt: now, deadline: nil, observed: "0", stage: 1, snoozedUntil: nil)]
        let other: [ReminderStatus.Account] = setup ? [.init(account: "040g2082040g2082040g", enabled: true, counts: .init(actionable: 0, unread: 0, unverified: false))] : []
        status = ReminderStatus(counts: ReminderCounts(actionable: 2, unread: 0, unverified: false), accounts: [.init(account: account, enabled: !setup, counts: .init(actionable: 2, unread: 0, unverified: false))] + other, badge: 2, enabled: true)
    }
    func saveFixture(_ value: NoticePreferences) { preferences = value }

    func test() async throws {
        let center = UNUserNotificationCenter.current()
        let content = UNMutableNotificationContent(); content.title = "dMsg"; content.body = NoticeCopy.text("noticeTest")
        try await center.add(UNNotificationRequest(identifier: "dmsg-test", content: content, trigger: nil))
    }
}
