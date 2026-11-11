import XCTest
import StoreKit
import DeviceCheck
@testable import dMsg

@MainActor
final class PurchaseDeliveryTests: XCTestCase {
    @MainActor private final class Finishes {
        var ids: [UInt64] = []
        func record(_ id: UInt64) { ids.append(id) }
    }

    @MainActor private final class FailureControl { var enabled = true }

    private func receipt(_ token: UUID?, id: UInt64 = 1, jws: String = "signed-test-observation", finishes: Finishes) -> PurchaseModel.Receipt {
        .init(id: id, product: "net.dmsg.ios.plus.annual", token: token, environment: "Sandbox", jws: jws,
            finish: { await finishes.record(id) })
    }

    private func stream(_ receipts: [PurchaseModel.Receipt]) -> AsyncStream<Result<PurchaseModel.Receipt, Error>> {
        AsyncStream { continuation in
            receipts.forEach { continuation.yield(.success($0)) }
            continuation.finish()
        }
    }

    private func store(_ token: UUID) -> PurchaseModel.Store {
        var store = PurchaseModel.Store()
        store.catalogue = { _ in
            .init(configuration: .init(bundleId: "net.dmsg.ios", appAppleId: 6827050680, products: []), accountToken: token.uuidString)
        }
        store.products = { _ in [] }
        store.subscriptions = { _, _ in [] }
        store.appEnvironment = { "Sandbox" }
        store.sync = {}
        store.prepare = { _, _ in XCTFail("Replay never presents or prepares a new payment") }
        store.submit = { _, _, _ in XCTFail("Unexpected submission") }
        store.currentEntitlements = { self.stream([]) }
        store.unfinished = { self.stream([]) }
        return store
    }

    func testPendingDeliveryRetainsTheReceiptUntilAnAppliedRetry() async throws {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let receipt = receipt(token, finishes: finishes)
        var store = store(token), submitted: [String] = []
        store.submit = { _, _, jws in
            submitted.append(jws)
            try PurchaseModel.requireApplied(Data((submitted.count == 1 ? #"{"state":"pending"}"# : #"{"state":"applied"}"#).utf8))
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        do { _ = try await purchases.deliver(receipt, model: model); XCTFail("Pending is not applied") } catch {}
        XCTAssertTrue(purchases.pending)
        XCTAssertEqual(purchases.revision, 0)
        XCTAssertTrue(finishes.ids.isEmpty)
        let result = try await purchases.deliver(receipt, model: model)
        XCTAssertEqual(result, .applied)
        XCTAssertEqual(submitted, [receipt.jws, receipt.jws])
        XCTAssertEqual(finishes.ids, [1])
        XCTAssertFalse(purchases.pending)
        XCTAssertEqual(purchases.revision, 1)
    }

    func testRestoreContinuesAfterOneFailureAndPreservesOtherPendingTransactions() async {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let first = receipt(token, id: 1, jws: "first", finishes: finishes)
        let second = receipt(token, id: 2, jws: "second", finishes: finishes)
        var store = store(token), submitted: [String] = [], syncs = 0
        let failure = FailureControl()
        store.sync = { syncs += 1 }
        store.currentEntitlements = { self.stream([first, second]) }
        store.unfinished = { self.stream([first, second]) }
        store.submit = { _, _, jws in
            submitted.append(jws)
            if failure.enabled && jws == "first" { throw URLError(.notConnectedToInternet) }
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.restore(model)
        XCTAssertEqual(submitted, ["first", "second"], "Current and unfinished copies are deduplicated within replay")
        XCTAssertEqual(finishes.ids, [2])
        XCTAssertTrue(purchases.pending, "Finishing the second transaction must not hide the first failure")
        XCTAssertNil(purchases.feedback, "Partial delivery must not claim all purchases were applied")
        XCTAssertNil(model.error)
        failure.enabled = false
        await purchases.restore(model)
        XCTAssertEqual(syncs, 2)
        XCTAssertEqual(finishes.ids, [2, 1, 2])
        XCTAssertFalse(purchases.pending)
        XCTAssertNotNil(purchases.feedback)
    }

    func testLaunchListenerDoesNotWaitForCatalogueAndRecoveryReplaysTheMissedReceipt() async throws {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let receipt = receipt(token, finishes: finishes)
        var store = store(token), submitted = 0
        let listening = expectation(description: "Listening without a catalogue or account token")
        let loading = expectation(description: "Catalogue request suspended")
        var events: AsyncStream<Result<PurchaseModel.Receipt, Error>>.Continuation?
        var resume: CheckedContinuation<PurchaseModel.Catalogue, Error>?
        let catalogue = store.catalogue
        store.catalogue = { _ in try await withCheckedThrowingContinuation { resume = $0; loading.fulfill() } }
        store.updates = { AsyncStream { events = $0; listening.fulfill() } }
        store.unfinished = { self.stream([receipt]) }
        store.submit = { _, _, _ in submitted += 1 }
        let purchases = PurchaseModel(enabled: true, store: store)
        let listener = Task { await purchases.listen(model) }
        defer { listener.cancel(); events?.finish() }
        let refresh = Task { await purchases.refresh(model) }
        await fulfillment(of: [listening, loading], timeout: 2)
        XCTAssertEqual(submitted, 0)
        // Nothing is lost if StoreKit reports the purchase before token lookup.
        events?.yield(.success(receipt))
        await Task.yield()
        resume?.resume(returning: try await catalogue(model))
        await refresh.value
        XCTAssertGreaterThanOrEqual(submitted, 1)
        XCTAssertFalse(finishes.ids.isEmpty)
        XCTAssertFalse(purchases.pending)
    }

    func testRefreshRecoversUnfinishedPurchasesAfterAnInitialCatalogueFailure() async {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let receipt = receipt(token, finishes: finishes)
        var store = store(token), reads = 0, submitted = 0
        let catalogue = store.catalogue
        store.catalogue = { model in
            reads += 1
            if reads == 1 { throw URLError(.notConnectedToInternet) }
            return try await catalogue(model)
        }
        store.unfinished = { self.stream([receipt]) }
        store.submit = { _, _, _ in submitted += 1 }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.refresh(model)
        XCTAssertEqual(purchases.loadIssue, .catalogue)
        XCTAssertEqual(submitted, 0)
        await purchases.refresh(model)
        XCTAssertNil(purchases.loadIssue)
        XCTAssertEqual(submitted, 1)
        XCTAssertEqual(finishes.ids, [1])
    }

    func testRestoreKeepsANewerObservationOfTheSameTransaction() async {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let first = receipt(token, jws: "first", finishes: finishes)
        let refund = receipt(token, jws: "refunded", finishes: finishes)
        var store = store(token), submitted: [String] = []
        store.currentEntitlements = { self.stream([first]) }
        store.unfinished = { self.stream([first, refund]) }
        store.submit = { _, _, jws in submitted.append(jws) }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.restore(model)
        XCTAssertEqual(submitted, ["first", "refunded"])
        XCTAssertEqual(finishes.ids, [1, 1])
        XCTAssertFalse(purchases.pending)
    }

    func testNetworkRecoveryRetriesDeliveryWithoutAuthenticatingOrChargingAgain() async {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes(), failure = FailureControl()
        let receipt = receipt(token, finishes: finishes)
        var store = store(token), submitted = 0
        store.sync = { XCTFail("Network recovery must not request Apple sign-in") }
        store.unfinished = { self.stream([receipt]) }
        store.submit = { _, _, _ in
            submitted += 1
            if failure.enabled { throw URLError(.notConnectedToInternet) }
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.refresh(model)
        XCTAssertTrue(purchases.pending)
        XCTAssertTrue(finishes.ids.isEmpty)
        failure.enabled = false
        await purchases.networkReturned(model)
        XCTAssertEqual(submitted, 2)
        XCTAssertEqual(finishes.ids, [1])
        XCTAssertFalse(purchases.pending)
        XCTAssertNil(model.error)
    }

    func testDuplicateDeliveryCoalescesButANewerObservationIsNotDropped() async throws {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let original = receipt(token, jws: "old", finishes: finishes)
        let refund = receipt(token, jws: "refunded", finishes: finishes)
        var store = store(token), submitted: [String] = []
        var resume: CheckedContinuation<Void, Never>?
        let started = expectation(description: "Original observation submitting")
        let duplicateStarted = expectation(description: "Duplicate waiting")
        let refundStarted = expectation(description: "Refund waiting")
        store.submit = { _, _, jws in
            submitted.append(jws)
            if submitted.count == 1 { await withCheckedContinuation { resume = $0; started.fulfill() } }
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        let first = Task { try await purchases.deliver(original, model: model) }
        await fulfillment(of: [started], timeout: 2)
        let duplicate = Task { duplicateStarted.fulfill(); return try await purchases.deliver(original, model: model) }
        let later = Task { refundStarted.fulfill(); return try await purchases.deliver(refund, model: model) }
        await fulfillment(of: [duplicateStarted, refundStarted], timeout: 2)
        resume?.resume()
        let results = try await [first.value, duplicate.value, later.value]
        XCTAssertEqual(results, [.applied, .applied, .applied])
        XCTAssertEqual(submitted, ["old", "refunded"])
        XCTAssertEqual(finishes.ids, [1, 1])
        XCTAssertEqual(purchases.revision, 2)
        XCTAssertFalse(purchases.pending)
    }

    func testForeignMissingAndLocalTestReceiptsNeverReachDeliveryOrFinish() async throws {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        let purchases = PurchaseModel(enabled: true, store: store(token))
        await purchases.load(model)
        let foreign = try await purchases.deliver(receipt(UUID(), finishes: finishes), model: model)
        let missing = try await purchases.deliver(receipt(nil, finishes: finishes), model: model)
        XCTAssertEqual(foreign, .otherAccount)
        XCTAssertEqual(missing, .unverified)
        let local = PurchaseModel.Receipt(id: 1, product: "plus", token: token, environment: "Xcode", jws: "local", finish: {})
        do { _ = try await purchases.deliver(local, model: model); XCTFail("Local test transactions must be refused") } catch {}
        XCTAssertTrue(finishes.ids.isEmpty)
        XCTAssertFalse(purchases.pending)
    }

    func testAccountSwitchDuringSubmissionCannotFinishOrChangeTheNewAccount() async throws {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID(), finishes = Finishes()
        var store = store(token)
        let started = expectation(description: "Submission started")
        var resume: CheckedContinuation<Void, Never>?
        store.submit = { _, _, _ in await withCheckedContinuation { resume = $0; started.fulfill() } }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        let task = Task { try await purchases.deliver(receipt(token, finishes: finishes), model: model) }
        await fulfillment(of: [started], timeout: 2)
        let old = try XCTUnwrap(model.session)
        model.installFixtures(.init(generation: old.generation + 1, sequence: old.sequence + 1, state: old.state,
            account: "040g2082040g2082040g", deviceId: old.deviceId, pendingSync: old.pendingSync,
            conflicts: old.conflicts, approvalsWaiting: old.approvalsWaiting, fastUnlock: old.fastUnlock,
            loginExpiresAtMs: old.loginExpiresAtMs, loginRenewable: old.loginRenewable, passkeyAvailable: old.passkeyAvailable,
            setupPending: old.setupPending, lastSyncAtMs: old.lastSyncAtMs, syncError: old.syncError), items: [])
        await purchases.load(model)
        resume?.resume()
        let result = try await task.value
        XCTAssertEqual(result, .stale)
        XCTAssertTrue(finishes.ids.isEmpty)
        XCTAssertEqual(purchases.revision, 0)
        XCTAssertFalse(purchases.pending)
        XCTAssertNil(purchases.feedback)
        XCTAssertNil(model.error)
    }

    func testInvalidAttestationKeyRetiresOnlyItsOwnRecord() throws {
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let key = directory.appending(path: "key.json")
        let saved = Data(#"{"id":"registered-key","device":"device"}"#.utf8)
        try saved.write(to: key)
        let invalid = NSError(domain: DCErrorDomain, code: DCError.Code.invalidKey.rawValue)
        try NativeApple.discardInvalidKey(URLError(.notConnectedToInternet), id: "registered-key", path: key)
        XCTAssertEqual(try Data(contentsOf: key), saved, "A network failure must not rotate a valid key")
        try NativeApple.discardInvalidKey(invalid, id: "older-key", path: key)
        XCTAssertEqual(try Data(contentsOf: key), saved, "A late failure must not delete a replacement key")
        try NativeApple.discardInvalidKey(invalid, id: "registered-key", path: key)
        XCTAssertFalse(FileManager.default.fileExists(atPath: key.path), "The next attempt must register a new usable key")
    }
}
