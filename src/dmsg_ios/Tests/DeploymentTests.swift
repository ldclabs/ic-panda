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

    func testAppleSubscriptionCannotMoveAccountsByChangingPurchaseToken() {
        let a = UUID(), b = UUID()
        for active in [true, false] {
            XCTAssertEqual(PurchaseModel.ownership(originalToken: a, transactionToken: a, renewalToken: a, expected: b, active: active), .otherAccount)
            // Even if Apple accepted B's token during a change, the original belongs to A.
            XCTAssertEqual(PurchaseModel.ownership(originalToken: a, transactionToken: b, renewalToken: b, expected: b, active: active), .otherAccount)
        }
        XCTAssertEqual(PurchaseModel.ownership(originalToken: a, transactionToken: a, renewalToken: b, expected: a, active: true), .unverified)
    }

    func testSubscriptionOwnershipRequiresVerifiedOriginalAndCurrentTokens() {
        let a = UUID()
        XCTAssertEqual(PurchaseModel.ownership(originalToken: nil, transactionToken: a, renewalToken: a, expected: a, active: true), .unverified)
        XCTAssertEqual(PurchaseModel.ownership(originalToken: a, transactionToken: nil, renewalToken: a, expected: a, active: true), .unverified)
        XCTAssertEqual(PurchaseModel.ownership(originalToken: a, transactionToken: a, renewalToken: nil, expected: a, active: true), .current)
        XCTAssertEqual(PurchaseModel.ownership(originalToken: a, transactionToken: a, renewalToken: a, expected: a, active: false), .available)
    }

    func testForeignRestoreCannotDeliverOrFinishTheTransaction() {
        let a = UUID(), b = UUID()
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: a, expected: b, subscription: .otherAccount), .otherAccount)
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: b, expected: b, subscription: .otherAccount), .otherAccount)
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: nil, expected: b, subscription: nil), .unverified)
        XCTAssertEqual(PurchaseModel.deliveryIssue(token: b, expected: b, subscription: .unverified), .unverified)
        XCTAssertNil(PurchaseModel.deliveryIssue(token: b, expected: b, subscription: .available))
    }

    func testPurchaseRequiresACompletedSubscriptionGroupCheck() {
        let model = PurchaseModel(enabled: false)
        XCTAssertFalse(model.canPurchase(group: "not-yet-checked"))
    }

}
