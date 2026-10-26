import Foundation
@preconcurrency import DMsgCore

/// The receipt path selects an isolated test realm; it never grants an
/// entitlement. Apple JWS verification and canister authority remain mandatory.
enum MobileDeployment {
    case production
    case staging(StagingDeployment)

    var name: String { if case .staging = self { "staging" } else { "production" } }
    static var wantsStaging: Bool {
        #if DEBUG
        return (Bundle.main.object(forInfoDictionaryKey: "DMsgUseStaging") as? String)?.uppercased() == "YES"
        #else
        return Bundle.main.appStoreReceiptURL?.lastPathComponent == "sandboxReceipt"
        #endif
    }
    static func load() throws -> Self {
        guard wantsStaging else { return .production }
        guard let url = Bundle.main.url(forResource: "StagingDeployment", withExtension: "json") else { throw MobileError.Failure(code: "TEST_DEPLOYMENT_UNAVAILABLE") }
        guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 16 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let data = try Data(contentsOf: url)
        guard let fields = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(fields.keys).isSubset(of: ["relay","handle","user_homes","wallet_hub","connect_url"]) else { throw CocoaError(.fileReadCorruptFile) }
        struct Settings: Decodable { let relay: String; let handle: String; let userHomes: [String]; let walletHub: String?; let connectUrl: String }
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let config = try decoder.decode(Settings.self, from: data)
        return .staging(StagingDeployment(relay: config.relay, handle: config.handle, userHomes: config.userHomes, walletHub: config.walletHub, connectUrl: config.connectUrl))
    }
    /// Only a public realm marker; credentials/preferences live in separate roots.
    func recordSelection() throws -> Bool {
        let parent = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        var path = parent.appending(path: "dMsg-deployment")
        let previous = try? String(contentsOf: path, encoding: .utf8)
        try Data(name.utf8).write(to: path, options: [.atomic, .completeFileProtection])
        var flags = URLResourceValues(); flags.isExcludedFromBackup = true; try path.setResourceValues(flags)
        return previous != nil && previous != name
    }
}
