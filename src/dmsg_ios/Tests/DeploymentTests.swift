import XCTest
import StoreKit
@testable import dMsg

@MainActor
final class DeploymentTests: XCTestCase {
    func testDefaultDeploymentDoesNotRequireAStagingConfiguration() throws {
        XCTAssertFalse(MobileDeployment.wantsStaging)
        XCTAssertEqual(try MobileDeployment.load().name, "production")
    }

    func testDisabledStoreKitRestoreAndListenersReturnWithoutStartingWork() async {
        let model = SessionModel()
        model.loadFixtures()
        XCTAssertTrue(model.unlocked)
        let purchases = PurchaseModel(enabled: false)
        XCTAssertFalse(purchases.enabled)
        let finished = expectation(description: "Disabled StoreKit paths return without sync or listeners")
        let task = Task {
            await purchases.load(model)
            await purchases.restore(model)
            await purchases.listen(model)
            finished.fulfill()
        }
        defer { task.cancel() }
        await fulfillment(of: [finished], timeout: 2)
        XCTAssertFalse(purchases.working)
        XCTAssertFalse(purchases.loading)
        XCTAssertFalse(purchases.pending)
        XCTAssertTrue(purchases.products.isEmpty)
        XCTAssertNil(model.error)
    }
    func testAppleEnvironmentIsIndependentOfProductionBusinessServices() throws {
        XCTAssertEqual(try MobileDeployment.load().name, "production")
        XCTAssertEqual(PurchaseModel.environmentName(.sandbox), "Sandbox")
        XCTAssertEqual(PurchaseModel.environmentName(.production), "Production")
        XCTAssertNil(PurchaseModel.environmentName(.xcode))
    }

    func testPendingOrMalformedDeliveryCannotFinishTheAppleTransaction() throws {
        XCTAssertNoThrow(try PurchaseModel.requireApplied(Data(#"{"state":"applied"}"#.utf8)))
        XCTAssertThrowsError(try PurchaseModel.requireApplied(Data(#"{"state":"pending"}"#.utf8)))
        XCTAssertThrowsError(try PurchaseModel.requireApplied(Data(#"{"accepted":true}"#.utf8)))
    }

    func testActiveSubscriptionManagementDoesNotBlockARealPurchaseAfterSandbox() {
        let now = Date(timeIntervalSince1970: 1000)
        let limits = SubscriptionView.Plan.Limits(storageBytes: 10, activeChannels: 5, monthlyExecutionUnits: 5)
        let active = SubscriptionView.Plan(plan: "Pro", source: "Active", limits: limits,
            current: .init(endsAtMs: 2000000, method: "AppStore", storeEnvironment: "Sandbox"))
        XCTAssertTrue(active.managesSubscription(environment: "Sandbox", at: now))
        XCTAssertTrue(active.managesSubscription(environment: nil, at: now))
        XCTAssertFalse(active.managesSubscription(environment: "Production", at: now))
        XCTAssertFalse(active.managesSubscription(environment: "Sandbox", at: Date(timeIntervalSince1970: 2000)))
        let external = SubscriptionView.Plan(plan: "Pro", source: "Active", limits: limits,
            current: .init(endsAtMs: 2000000, method: "Cash", storeEnvironment: nil))
        XCTAssertFalse(external.managesSubscription(environment: "Production", at: now))
    }

    func testLatestAppleTransactionIdentifiesOneAccountWithoutRequiringHistoricalTokens() {
        let a = UUID(), b = UUID()
        // Apple can retain A on the original transaction while the latest uses B.
        // Requiring both tokens previously reported "another account" for A and B.
        for active in [true, false] {
            let latest = PurchaseModel.SubscriptionStatus(token: b, state: active ? .subscribed : .expired)
            XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: [latest], expected: a).ownership, active ? .otherAccount : .available)
            XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: [latest], expected: b).ownership, active ? .current : .available)
        }
    }

    func testSubscriptionStatusDoesNotLetAnotherHistoryHideTheMatchingAccount() {
        let a = UUID(), b = UUID()
        for active in [true, false] {
            let first = PurchaseModel.SubscriptionStatus(token: a, state: active ? .subscribed : .expired)
            let second = PurchaseModel.SubscriptionStatus(token: b, state: active ? .subscribed : .expired)
            for statuses in [[first, second], [second, first]] {
                for account in [a, b] {
                    XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: statuses, expected: account).ownership, active ? .current : .available)
                }
            }
        }
    }

    func testActiveForeignSubscriptionStillBlocksAnExpiredMatchingHistory() {
        let a = UUID(), b = UUID()
        let statuses = [PurchaseModel.SubscriptionStatus(token: a, state: .expired), .init(token: b, state: .subscribed)]
        XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: statuses, expected: a).ownership, .otherAccount)
        XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: statuses, expected: b).ownership, .current)
    }

    func testMissingTransactionTokenDoesNotInventAnOwner() {
        let account = UUID()
        let unknown = PurchaseModel.SubscriptionStatus(token: nil, state: .subscribed)
        XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: [unknown], expected: account).ownership, .unverified)
        XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: [], expected: account).ownership, .available)
    }

    func testExpiredSubscriptionAllowsRepurchaseButRestoreCannotMoveItsReceipt() {
        let a = UUID(), b = UUID()
        let expired = PurchaseModel.SubscriptionStatus(token: a, state: .expired)
        XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: [expired], expected: b).ownership, .available)
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: a, expected: b), .otherAccount)
        for state in [Product.SubscriptionInfo.RenewalState.subscribed, .inGracePeriod, .inBillingRetryPeriod, .revoked] {
            let unfinished = PurchaseModel.SubscriptionStatus(token: a, state: state)
            XCTAssertEqual(PurchaseModel.subscription(group: "annual", statuses: [unfinished], expected: b).ownership, .otherAccount)
        }
    }

    func testNextRenewalIsShownOnlyForTheMatchingActiveSubscription() {
        let account = UUID(), next = Date(timeIntervalSince1970: 2000)
        let current = PurchaseModel.SubscriptionStatus(token: account, state: .subscribed, nextProduct: "Plus", renewalDate: next)
        let result = PurchaseModel.subscription(group: "annual", statuses: [current], expected: account)
        XCTAssertEqual(result.nextProduct, "Plus")
        XCTAssertEqual(result.renewalDate, next)
        XCTAssertNil(PurchaseModel.subscription(group: "annual", statuses: [current], expected: UUID()).nextProduct)
        let expired = PurchaseModel.SubscriptionStatus(token: account, state: .expired, nextProduct: "Plus", renewalDate: next)
        XCTAssertNil(PurchaseModel.subscription(group: "annual", statuses: [expired], expected: account).nextProduct)
    }

    func testForeignRestoreCannotDeliverOrFinishTheTransaction() {
        let a = UUID(), b = UUID()
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: a, expected: b), .otherAccount)
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: nil, expected: b), .unverified)
        // A group can be foreign or unavailable while an unfinished transaction
        // belongs to B. Only that transaction is submitted; the service must apply it.
        XCTAssertNil(PurchaseModel.deliveryIssue(token: b, expected: b))
    }

    func testPurchaseRequiresACompletedSubscriptionGroupCheck() {
        let model = PurchaseModel(enabled: false)
        XCTAssertFalse(model.canPurchase(group: "not-yet-checked"))
    }

}
