import Foundation

enum ReminderCategory: String, Codable, CaseIterable {
    case security, action, message, service
    var title: String {
        switch self {
        case .security: String(localized: "Account security")
        case .action: String(localized: "Approvals")
        case .message: NoticeCopy.text("noticeMessages")
        case .service: NoticeCopy.text("noticeServices")
        }
    }
    var bodyKey: String {
        switch self { case .security: "noticeSecurityBody"; case .action: "noticeActionBody"; case .message: "noticeMessageBody"; case .service: "noticeServiceBody" }
    }
}

enum ReminderTarget: Hashable, Codable {
    case channel(String), inbox(String?), approval(String), recovery(String), agentMail(String)
    case task(String), plan(String), service(String), wallet(String, String), summary(ReminderCategory)
    private enum Keys: String, CodingKey { case kind, id }
    private struct Wallet: Codable { let wallet: String; let proposal: String }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "wallet":
            let value = try c.decode(Wallet.self, forKey: .id)
            guard UInt64(value.proposal).map(String.init) == value.proposal else { throw Self.invalid(decoder) }
            self = .wallet(value.wallet, value.proposal)
        case "summary": self = .summary(try c.decode(ReminderCategory.self, forKey: .id))
        case "inbox":
            let id = try c.decodeIfPresent(String.self, forKey: .id)
            guard id == nil || Self.hash(id!) else { throw Self.invalid(decoder) }; self = .inbox(id)
        default:
            let id = try c.decode(String.self, forKey: .id)
            switch kind {
            case "channel", "approval", "recovery", "plan":
                guard Self.hash(id) else { throw Self.invalid(decoder) }
                switch kind { case "channel": self = .channel(id); case "approval": self = .approval(id); case "recovery": self = .recovery(id); default: self = .plan(id) }
            case "agent_mail": guard id.hasPrefix("did:agent:") && id.count <= 256 else { throw Self.invalid(decoder) }; self = .agentMail(id)
            case "task": guard id.hasPrefix("conflicts:") || id.hasPrefix("sync_failed:") else { throw Self.invalid(decoder) }; self = .task(id)
            case "service": guard id.hasPrefix("network:") || id.hasPrefix("registration:") else { throw Self.invalid(decoder) }; self = .service(id)
            default: throw Self.invalid(decoder)
            }
        }
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        let kind: String, id: String?
        switch self {
        case .channel(let v): kind = "channel"; id = v
        case .inbox(let v): kind = "inbox"; id = v
        case .approval(let v): kind = "approval"; id = v
        case .recovery(let v): kind = "recovery"; id = v
        case .agentMail(let v): kind = "agent_mail"; id = v
        case .task(let v): kind = "task"; id = v
        case .plan(let v): kind = "plan"; id = v
        case .service(let v): kind = "service"; id = v
        case .summary(let v): kind = "summary"; id = v.rawValue
        case .wallet(let wallet, let proposal):
            try c.encode("wallet", forKey: .kind); try c.encode(Wallet(wallet: wallet, proposal: proposal), forKey: .id); return
        }
        try c.encode(kind, forKey: .kind); try c.encode(id, forKey: .id)
    }
    var category: ReminderCategory {
        switch self {
        case .recovery: .security
        case .approval, .wallet, .task: .action
        case .plan, .service: .service
        case .summary(let category): category
        default: .message
        }
    }
    static func hash(_ value: String) -> Bool { value.utf8.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) } }
    private static func invalid(_ decoder: Decoder) -> DecodingError { .dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "Unsupported notification target")) }
}

struct ReminderItem: Decodable, Identifiable {
    let id: String; let account: String; let target: ReminderTarget; let version: String
    let open: Bool; let actionable: Bool; let unread: UInt64?
    let firstAt: UInt64; let updatedAt: UInt64; let deadline: UInt64?
    let observed: String; let stage: UInt8; let snoozedUntil: UInt64?
}
struct ReminderPage: Decodable { let items: [ReminderItem]; let next: UInt32? }
struct ReminderCounts: Decodable { let actionable: UInt64; let unread: UInt64; let unverified: Bool }
struct ReminderStatus: Decodable {
    struct Account: Decodable, Identifiable { let account: String; let enabled: Bool; let counts: ReminderCounts; var id: String { account } }
    let counts: ReminderCounts; let accounts: [Account]; let badge: UInt64; let enabled: Bool
}
struct ReminderPlan: Decodable, Identifiable {
    struct Covered: Decodable { let id: String; let version: String; let stage: UInt8 }
    let systemId: String; let deliveryId: String; let account: String
    let category: ReminderCategory; let target: ReminderTarget; let covered: [Covered]
    let count: UInt64?; let sound: Bool; let inApp: Bool; let update: Bool
    var id: String { deliveryId }
    var body: String { NoticeCopy.text(category != .security && count == nil ? "noticeUpdatesBody" : category.bodyKey) }
}
struct ReminderBatch: Decodable { let plans: [ReminderPlan]; let remove: [String]; let nextDue: UInt64? }
struct ReminderRoute: Decodable { let account: String; let target: ReminderTarget; let ended: Bool }
struct ReminderEntry: Identifiable { let id = UUID(); var account: String?; var target: ReminderTarget?; var ended = false; var retryRoute: String? }

enum NotificationRoute {
    static func parse(_ value: Any?) -> String? {
        guard let payload = value as? [String: Any], payload["version"] as? Int == 1 else { return nil }
        if let route = payload["route"] as? String {
            // Keep old account-only routes readable during the one-time migration.
            guard route.count == 64 || route.count == 128,
                  route.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { return nil }
            return route
        }
        if let local = payload["local"] as? String, ReminderTarget.hash(local) { return "local:" + local }
        return nil
    }
}

struct NoticeCategoryPreference: Codable, Equatable { var enabled: Bool; var sound: Bool }
struct NoticeAccountPreference: Codable, Equatable {
    var revision: UInt64; var enabled: Bool; var categories: [String: NoticeCategoryPreference]
    var mergeDevices: Bool; var agentMail: [String: Bool]; var remoteRevision: UInt64
}
struct NoticeQuietHours: Codable, Equatable { var start: Int; var end: Int; var timezone: String }
struct NoticeGlobalPreference: Codable, Equatable {
    var revision: UInt64; var enabled: Bool; var pausedUntil: UInt64?; var quietHours: NoticeQuietHours?
    var securityException: Bool; var badge: String
}
struct NoticePreferences: Decodable, Equatable {
    var global: NoticeGlobalPreference; var account: NoticeAccountPreference?; var accountId: String?
    var remoteSupported: Bool; var globalPendingSync: Bool?; var globalConflict: Bool?; var accountConflict: Bool?
    var pending: Bool { globalPendingSync == true || (account.map { $0.revision > $0.remoteRevision } ?? false) }
    var conflict: Bool { globalConflict == true || accountConflict == true }
}

enum NoticeEncoding {
    static func json<T: Encodable>(_ value: T) throws -> String {
        let encoder = JSONEncoder(); encoder.keyEncodingStrategy = .convertToSnakeCase
        return String(decoding: try encoder.encode(value), as: UTF8.self)
    }
}

enum NotificationTime {
    static var now: UInt64 { UInt64(max(0, Date.now.timeIntervalSince1970 * 1000)) }
    static func quietUntil(_ preference: NoticeGlobalPreference, at now: Date = .now) -> UInt64? {
        guard let hours = preference.quietHours, let zone = TimeZone(identifier: hours.timezone), hours.start != hours.end else { return nil }
        var calendar = Calendar(identifier: .gregorian); calendar.timeZone = zone
        let components = calendar.dateComponents([.hour, .minute], from: now)
        let minute = (components.hour ?? 0) * 60 + (components.minute ?? 0)
        let inside = hours.start < hours.end ? minute >= hours.start && minute < hours.end : minute >= hours.start || minute < hours.end
        guard inside, let end = calendar.nextDate(after: now, matching: DateComponents(hour: hours.end / 60, minute: hours.end % 60), matchingPolicy: .nextTime, repeatedTimePolicy: .last) else { return nil }
        return UInt64(end.timeIntervalSince1970 * 1000)
    }
    static func tomorrow(_ now: Date = .now) -> UInt64 {
        let calendar = Calendar.current
        let day = calendar.date(byAdding: .day, value: 1, to: calendar.startOfDay(for: now))!
        return UInt64(calendar.date(bySettingHour: 8, minute: 0, second: 0, of: day)!.timeIntervalSince1970 * 1000)
    }
}

enum NoticeCopy {
    static func text(_ key: String) -> String { Bundle.main.localizedString(forKey: "dmsg." + key, value: fallback[key] ?? key, table: nil) }
    // Kept with the shared desktop notification vocabulary by sync-notifications.mjs.
    static let fallback: [String: String] = [
        "noticeTitle": "Reminders",
        "noticeDetail": "Pending actions and recent updates",
        "noticeNeedsAction": "Needs action",
        "noticeRecent": "Recent reminders",
        "noticeSettings": "Notifications",
        "noticeEnable": "System notifications on this device",
        "noticeAccount": "Receive notifications for this account",
        "noticeSound": "Sound",
        "noticeMessages": "Messages",
        "noticeServices": "Services",
        "noticePause15": "Pause for 15 minutes",
        "noticePauseHour": "Pause for 1 hour",
        "noticePauseTomorrow": "Pause until tomorrow at 08:00",
        "noticeResume": "Resume notifications",
        "noticeQuiet": "Daily quiet hours",
        "noticeFrom": "From",
        "noticeUntil": "Until",
        "noticeZone": "Time zone",
        "noticeSecurityException": "Allow security alerts during quiet hours",
        "noticeBadge": "App icon badge",
        "noticeBadgeAll": "Actions and unread messages",
        "noticeBadgeActions": "Actions only",
        "noticeBadgeOff": "Off",
        "noticeMerge": "Merge duplicate alerts across devices",
        "noticeAgent": "Agent Mail alerts",
        "noticeTest": "Test notification",
        "noticeSystem": "Open system settings",
        "noticeManaged": "Managed by the operating system",
        "noticeUnsupported": "Unavailable on this system",
        "noticePending": "Saved here; remote settings pending",
        "noticeRemote": "Remote notification filtering is not available yet",
        "noticeClear": "Clear finished reminders",
        "noticeLater": "Remind me later",
        "noticeLate": "This time is after the deadline",
        "noticeEmpty": "No reminders",
        "noticeSwitch": "Switch to this account to open the item",
        "noticeEnded": "This item has ended or is unavailable",
        "noticeSecurityBody": "An account security change needs your review.",
        "noticeActionBody": "An item needs your attention.",
        "noticeMessageBody": "You have new messages.",
        "noticeServiceBody": "A service needs your attention.",
        "noticeTestSent": "Test notification submitted to the system",
        "noticeReadOnlyHint": "Viewing reminders does not complete actions or mark messages as read.",
        "noticePendingHint": "Your settings apply here immediately. Remote alerts may use earlier settings until the update is confirmed.",
        "noticeScopeHint": "Quiet hours and sounds apply to this device. Channel mutes follow your account.",
        "noticeUpdatesBody": "Open dMsg to check for new activity."
    ]
}
