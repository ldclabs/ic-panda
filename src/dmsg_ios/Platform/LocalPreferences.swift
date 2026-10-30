import Foundation

struct LocalPreferences: Codable, Equatable {
    var immediateBackgroundLock = false
    /// Accounts that turned automatic sync off; every other account syncs
    /// while the app is open and unlocked. Optional, as are the fields
    /// below, so files written before it existed still decode.
    var manualSyncAccounts: Set<String>?
    /// Accounts already told that they sync automatically.
    var syncIntroduced: Set<String>?
    /// Accounts whose security-alert prompt was set aside.
    var notificationSetupDismissed: Set<String>?
    static func read(at root: URL) throws -> Self {
        let file = root.appending(path: "privacy-preferences.json")
        guard FileManager.default.fileExists(atPath: file.path) else { return Self() }
        return try JSONDecoder().decode(Self.self, from: Data(contentsOf: file))
    }
    func write(at root: URL) throws {
        try JSONEncoder().encode(self).write(to: root.appending(path: "privacy-preferences.json"),
                                           options: [.atomic, .completeFileProtection])
    }
}
