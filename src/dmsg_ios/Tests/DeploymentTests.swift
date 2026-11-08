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

}
