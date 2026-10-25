import XCTest
@preconcurrency import DMsgCore

final class CoreBridgeTests: XCTestCase {
    final class NoHardware: Platform, @unchecked Sendable {
        func get(name: String) throws -> Data? { nil }
        func set(name: String, secret: Data) throws { throw MobileError.Failure(code: "UNAVAILABLE") }
        func delete(name: String) throws {}
        func presence() -> UInt8 { 0 }
        func createAgreement() throws -> Agreement { throw MobileError.Failure(code: "UNAVAILABLE") }
        func openAgreement(handle: Data, peer: Data) throws -> Data { throw MobileError.Failure(code: "UNAVAILABLE") }
        func confirm(requestId: String, digest: String, reason: String) throws { throw MobileError.Failure(code: "CANCELLED") }
        func cancelPresence() {}
    }

    func testRealRustBridgeStartsLockedAndRejectsContentAndForeignCallbacks() async throws {
        let base = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: base) }
        let engine = try await MobileEngine.open(directory: base.appending(path: "account").path,
            staging: base.appending(path: "imports").path, platform: NoHardware())
        let initial = try await engine.sessionState()
        XCTAssertNotEqual(initial.state, "unlocked")
        do { _ = try await engine.vaultPage(query: "", offset: 0, limit: 20); XCTFail("Locked content read") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        do { _ = try await engine.completeLogin(callback: "https://evil.example/#abc"); XCTFail("Foreign callback") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "INVALID_INPUT") }
        let locked = try await engine.lock()
        XCTAssertGreaterThan(locked.generation, initial.generation)
        let operation = OperationToken()
        operation.cancel()
        do { _ = try await engine.sync(operation: operation); XCTFail("Cancelled operation ran") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "CANCELLED") }
        engine.suspend()
        do { _ = try await engine.sessionState(); XCTFail("Suspended operation") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        let resumed = try await engine.resume()
        XCTAssertNotEqual(resumed.state, "unlocked")
        do { _ = try await engine.initializeAccount(); XCTFail("Stale UI handle started a mutation") }
        catch MobileError.Failure(let code) { XCTAssertEqual(code, "LOCKED") }
        let current = try engine.scoped(generation: resumed.generation)
        let currentState = try await current.sessionState()
        XCTAssertEqual(currentState.generation, resumed.generation)
        try await engine.close()
    }
}
