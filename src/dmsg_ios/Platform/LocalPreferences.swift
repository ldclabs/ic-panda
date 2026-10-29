import Foundation

struct LocalPreferences: Codable, Equatable {
    var immediateBackgroundLock = false
    /// Opt-in is scoped to an account; signing into another account grants nothing.
    var automaticSyncAccounts: Set<String> = []
    /// Accounts whose security-alert prompt was set aside. Optional, so files
    /// written before it existed still decode.
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
