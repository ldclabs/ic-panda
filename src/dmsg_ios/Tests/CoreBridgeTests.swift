import XCTest
import CryptoKit
@preconcurrency import DMsgCore

final class CoreBridgeTests: XCTestCase {
    class NoHardware: Platform, @unchecked Sendable {
        func get(name: String) throws -> Data? { nil }
        func set(name: String, secret: Data) throws { throw MobileError.Failure(code: "UNAVAILABLE") }
        func delete(name: String) throws {}
        func presence() -> UInt8 { 0 }
        func createSecret(biometryOnly: Bool) throws -> HardwareSecret { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func openSecret(handle: Data, context: Data, biometryOnly: Bool) throws -> Data { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func supportsBiometryOnly() -> Bool { false }
        func authorizeKeys(reason: String, biometryOnly: Bool, handles: [Data]) throws -> UInt64 { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func prepareSignature(reason: String, biometryOnly: Bool, handle: Data, message: Data) throws -> UInt64 { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func signPrepared(preparation: UInt64) throws -> Data { throw MobileError.Failure(code: "CANCELLED") }
        func releasePrepared(preparation: UInt64) {}
        func biometryState() -> Data? { nil }
        func createSigning(biometryOnly: Bool) throws -> SigningKey { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func deleteSigning(handle: Data) throws {}
        func authorize(reason: String, biometryOnly: Bool) throws -> UInt64 { throw MobileError.Failure(code: "CANCELLED") }
        func signAuthorized(authorization: UInt64, handle: Data, message: Data) throws -> Data { throw MobileError.Failure(code: "CANCELLED") }
        func releaseAuthorization(authorization: UInt64) {}
        func createAnchor() throws -> SigningKey { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func signAnchor(handle: Data, message: Data) throws -> Data { throw MobileError.Failure(code: "HARDWARE_UNAVAILABLE") }
        func confirm(requestId: String, digest: String, reason: String) throws { throw MobileError.Failure(code: "CANCELLED") }
        func cancelPresence() {}
    }

    /// The simulator fixture supplies only the platform callbacks. Rust owns real keys, storage and session transitions.
    final class FixtureHardware: NoHardware, @unchecked Sendable {
        private let lock = NSLock()
        private var slots: [String: Data] = [:]
        private let anchor = P256.Signing.PrivateKey()
        private let secret = SymmetricKey(size: .bits256).withUnsafeBytes { Data($0) }
        override func presence() -> UInt8 { 1 }
        override func get(name: String) throws -> Data? { lock.withLock { slots[name] } }
        override func set(name: String, secret: Data) throws { lock.withLock { slots[name] = secret } }
        override func delete(name: String) throws { _ = lock.withLock { slots.removeValue(forKey: name) } }
        override func createSecret(biometryOnly: Bool) throws -> HardwareSecret {
            HardwareSecret(handle: Data([1]), context: Data([2]), secret: secret)
        }
        override func openSecret(handle: Data, context: Data, biometryOnly: Bool) throws -> Data {
            guard handle == Data([1]), context == Data([2]) else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            return secret
        }
        override func createAnchor() throws -> SigningKey { SigningKey(publicKey: anchor.publicKey.x963Representation, handle: Data([3])) }
        override func signAnchor(handle: Data, message: Data) throws -> Data {
            guard handle == Data([3]) else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            return try anchor.signature(for: message).rawRepresentation
        }
    }

    func testOfflineAccountRestartsWithItsOwnVault() async throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: base) }
        let hardware = FixtureHardware()
        let directory = base.appending(path: "account").path, staging = base.appending(path: "staging").path
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        let itemID: String
        do {
            let root = try await MobileEngine.open(directory: directory, staging: staging, platform: hardware)
            let created = try await root.createLocalAccount()
            XCTAssertEqual(created.accountKind, "local"); XCTAssertEqual(created.account, "00000000000000000000")
            let engine = try root.scoped(generation: created.generation)
            let detail = try await engine.saveItem(id: nil, revision: nil, edit: VaultEdit(title: "Local fixture", kind: "note", notes: "Device only", favorite: false, tags: [], fields: [], files: nil))
            let name = "00000000000000000000/dmsg-import-fixture/sample.txt"
            let file = URL(fileURLWithPath: staging).appending(path: name)
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("offline file".utf8).write(to: file)
            _ = try await engine.importFile(name: name, folder: "root", sharedId: nil, operation: OperationToken())
            itemID = detail.item.id
            XCTAssertEqual(detail.item.saveState, "local_only"); XCTAssertFalse(detail.item.synced)
            _ = try await engine.lock()
        }
        let root = try await MobileEngine.open(directory: directory, staging: staging, platform: hardware)
        let locked = try await root.sessionState()
        XCTAssertEqual(locked.state, "locked")
        let reopened = try await root.unlockFast()
        let engine = try root.scoped(generation: reopened.generation)
        let item = try await engine.vaultItem(id: itemID, revision: nil)
        XCTAssertEqual(item.item.notes, "Device only"); XCTAssertEqual(reopened.pendingSync, 0)
        let listing = try await engine.files(folder: "root")
        let result = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(listing.json.utf8)) as? [String: Any])
        let files = try XCTUnwrap(result["files"] as? [[String: Any]])
        XCTAssertEqual(files.first?["name"] as? String, "sample.txt")
        XCTAssertEqual(files.first?["size"] as? Int, 12)
    }

    func testRealRustBridgeStartsLockedAndRejectsContentAndForeignCallbacks() async throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: base) }
        var engine = try await MobileEngine.open(directory: base.appending(path: "account").path,
            staging: base.appending(path: "imports").path, platform: NoHardware())
        let initial = try await engine.sessionState()
        XCTAssertNotEqual(initial.state, "unlocked")
        XCTAssertFalse(initial.passkeyAvailable)
        XCTAssertFalse(initial.loginRenewable)
        XCTAssertEqual(initial.accountKind, "none")
        do { _ = try await engine.createLocalAccount(); XCTFail("Created a hardware workspace without hardware") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCAL_HARDWARE_UNAVAILABLE") }
        let denied = try await engine.sessionState()
        XCTAssertEqual(denied.state, "logged_out"); XCTAssertNil(denied.account)
        engine = try engine.scoped(generation: denied.generation)
        do { _ = try await engine.beginPasskey(operation: .login, credentialId: nil); XCTFail("Passkey without hardware/configuration") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "UNSUPPORTED_PROTOCOL") }
        do { _ = try await engine.beginPasskey(operation: .resetSessions, credentialId: nil); XCTFail("A capacity reset needs its failed credential") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "INVALID_INPUT") }
        let recovery = try await engine.recoveryWarning()
        XCTAssertEqual(recovery.json, "null")
        do { _ = try await engine.hardwareSecurity(); XCTFail("Locked hardware management") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        do { _ = try await engine.secretHealth(); XCTFail("Locked secret health read") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        do { _ = try await engine.vaultPage(query: "", kind: nil, favoritesOnly: false, offset: 0, limit: 20); XCTFail("Locked content read") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        do { _ = try await engine.completeLogin(callback: "https://evil.example/#abc"); XCTFail("Foreign callback") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "INVALID_INPUT") }
        let locked = try await engine.lock()
        XCTAssertGreaterThan(locked.generation, initial.generation)
        let operation = OperationToken()
        operation.cancel()
        do { _ = try await engine.sync(automatic: false, operation: operation); XCTFail("Cancelled operation ran") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "CANCELLED") }
        let oldForeground = engine.controlGeneration()
        engine.suspend()
        do { _ = try await engine.resume(expectedGeneration: oldForeground); XCTFail("Late foreground resumed background work") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        do { _ = try await engine.configureInbox(mode: "paid", allow: [], operation: OperationToken()); XCTFail("Paid policy enabled through the free iOS API") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "INVALID_INPUT") }
        engine.lockNow()
        do { _ = try await engine.sessionState(); XCTFail("Suspended operation") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        let resumed = try await engine.resume(expectedGeneration: engine.controlGeneration())
        XCTAssertNotEqual(resumed.state, "unlocked")
        do { _ = try await engine.initializeAccount(); XCTFail("Stale UI handle started a mutation") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        let current = try engine.scoped(generation: resumed.generation)
        let currentState = try await current.sessionState()
        XCTAssertEqual(currentState.generation, resumed.generation)
        do { _ = try await engine.recoveryWarning(); XCTFail("Stale security observation") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        do { _ = try await engine.profileAvatar(url: "https://assets.dmsg.net/avatar.png"); XCTFail("Stale avatar fetch") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        try await engine.shutdown()
    }
}
