import XCTest
import StoreKit
import StoreKitTest
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

    private func storeFixture() async throws -> (SKTestSession, PurchaseModel.Store, Product) {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".storekit")
        // The local StoreKit catalogue supplies a real Product without contacting
        // Apple or making a purchase. Account/status reads are controlled below.
        let configuration = #"""
        {"identifier":"SubscriptionLoading","version":{"major":3,"minor":0},
         "settings":{"_locale":"en_US","_storefront":"USA"},"products":[],"nonRenewingSubscriptions":[],
         "subscriptionGroups":[{"id":"fixture-annual","name":"Annual","localizations":[],"subscriptions":[
           {"adHocOffers":[],"codeOffers":[],"displayPrice":"9.99","familyShareable":false,"groupNumber":1,
            "internalID":"fixture-plus","introductoryOffer":null,
            "localizations":[{"description":"Test annual subscription","displayName":"Plus","locale":"en_US"}],
            "productID":"net.dmsg.ios.plus.annual","recurringSubscriptionPeriod":"P1Y","referenceName":"Plus Annual",
            "subscriptionGroupID":"fixture-annual","type":"RecurringSubscription"}]}]}
        """#
        try Data(configuration.utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let session = try SKTestSession(contentsOf: url)
        session.disableDialogs = true
        let products = try await Product.products(for: ["net.dmsg.ios.plus.annual"])
        let product = try XCTUnwrap(products.first)
        XCTAssertEqual(product.type, .autoRenewable)
        let token = UUID()
        var store = PurchaseModel.Store()
        store.catalogue = { _ in
            .init(configuration: .init(bundleId: "net.dmsg.ios", appAppleId: 6827050680, products: [.init(productId: product.id)]), accountToken: token.uuidString)
        }
        store.products = { _ in [product] }
        store.appEnvironment = { "Sandbox" }
        store.refreshAppEnvironment = { XCTFail("Background reads must not authenticate Apple"); throw StoreKitError.userCancelled }
        store.subscriptions = { _, _, _ in
            [.init(id: product.subscription!.subscriptionGroupID, ownership: .available)]
        }
        return (session, store, product)
    }

    func testAppleAccountReadFailureKeepsProductsAndAuthenticatesOnlyOnRetry() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, refreshes = 0
        store.appEnvironment = { throw StoreKitError.notEntitled }
        store.refreshAppEnvironment = { refreshes += 1; return "Sandbox" }
        let purchases = PurchaseModel(enabled: true, store: store)
        for _ in 0..<2 {
            await purchases.load(model)
            XCTAssertEqual(purchases.products.map(\.id), [product.id])
            XCTAssertEqual(purchases.loadIssue, .appStore)
            XCTAssertNil(purchases.subscriptionMessage)
            XCTAssertNil(purchases.feedback)
            XCTAssertNil(model.error)
            XCTAssertFalse(purchases.canPurchase(product))
        }
        XCTAssertEqual(refreshes, 0)
        await purchases.retry(model)
        XCTAssertEqual(refreshes, 1)
        XCTAssertNil(purchases.loadIssue)
        XCTAssertEqual(purchases.storeEnvironment, "Sandbox")
        XCTAssertTrue(purchases.canPurchase(product))
        XCTAssertFalse(purchases.loading)
        XCTAssertFalse(purchases.working)
    }

    func testUnavailableSubscriptionStatusIsNotAnEmptyNewAccount() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, reads = 0
        store.subscriptions = { _, token, _ in
            reads += 1
            if reads == 1 { throw URLError(.notConnectedToInternet) }
            return [PurchaseModel.subscription(group: product.subscription!.subscriptionGroupID, statuses: [], expected: token)]
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertEqual(purchases.products.map(\.id), [product.id])
        XCTAssertEqual(purchases.loadIssue, .subscriptions)
        XCTAssertNil(purchases.subscriptionMessage)
        XCTAssertNil(model.error)
        XCTAssertFalse(purchases.canPurchase(product))
        await purchases.load(model)
        XCTAssertNil(purchases.loadIssue)
        XCTAssertTrue(purchases.canPurchase(product))
    }

    func testCatalogueAndProductErrorsNeverReportAnAccountAssociationProblem() async throws {
        let (session, initial, _) = try await storeFixture()
        defer { session.resetToDefaultState() }
        for stage in [PurchaseModel.LoadIssue.catalogue, .products] {
            let model = SessionModel(); model.loadFixtures()
            var store = initial
            if stage == .catalogue { store.catalogue = { _ in throw URLError(.notConnectedToInternet) } }
            else { store.products = { _ in throw URLError(.notConnectedToInternet) } }
            store.appEnvironment = { XCTFail("An earlier failure must not request Apple credentials"); return "Sandbox" }
            let purchases = PurchaseModel(enabled: true, store: store)
            await purchases.load(model)
            XCTAssertEqual(purchases.loadIssue, stage)
            XCTAssertTrue(purchases.products.isEmpty)
            XCTAssertNil(purchases.subscriptionMessage)
            XCTAssertNil(purchases.feedback)
            XCTAssertNil(model.error)
        }
    }

    func testAccountSwitchRejectsALateCredentialFailureWithoutPrompting() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        let original = model.session?.account, firstToken = UUID(), nextToken = UUID()
        var store = initial, reads = 0, checkedTokens: [UUID] = []
        var suspended: CheckedContinuation<String, Error>?
        let started = expectation(description: "Old account's App Store read started")
        store.catalogue = { model in
            .init(configuration: .init(bundleId: "net.dmsg.ios", appAppleId: 6827050680, products: [.init(productId: product.id)]),
                accountToken: (model.session?.account == original ? firstToken : nextToken).uuidString)
        }
        store.appEnvironment = {
            reads += 1
            if reads == 1 {
                return try await withCheckedThrowingContinuation { suspended = $0; started.fulfill() }
            }
            return "Sandbox"
        }
        store.subscriptions = { _, token, _ in
            checkedTokens.append(token)
            return [PurchaseModel.subscription(group: product.subscription!.subscriptionGroupID, statuses: [], expected: token)]
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        let oldRead = Task { await purchases.retry(model) }
        await fulfillment(of: [started], timeout: 2)
        let old = try XCTUnwrap(model.session)
        model.installFixtures(.init(generation: old.generation + 1, sequence: old.sequence + 1, state: old.state,
            account: "040g2082040g2082040g", deviceId: old.deviceId, pendingSync: old.pendingSync,
            conflicts: old.conflicts, approvalsWaiting: old.approvalsWaiting, fastUnlock: old.fastUnlock,
            loginExpiresAtMs: old.loginExpiresAtMs, loginRenewable: old.loginRenewable, passkeyAvailable: old.passkeyAvailable,
            setupPending: old.setupPending, lastSyncAtMs: old.lastSyncAtMs, syncError: old.syncError), items: [])
        await purchases.load(model)
        suspended?.resume(throwing: StoreKitError.notEntitled)
        await oldRead.value
        XCTAssertEqual(checkedTokens, [nextToken])
        XCTAssertNil(purchases.loadIssue)
        XCTAssertNil(purchases.subscriptionMessage)
        XCTAssertNil(model.error)
        XCTAssertTrue(purchases.canPurchase(product))
    }

}
