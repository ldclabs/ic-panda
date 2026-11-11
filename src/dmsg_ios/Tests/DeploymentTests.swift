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
        store.sync = { XCTFail("Background reads must not authenticate Apple") }
        store.preflight = { _, _, _ in XCTFail("Unexpected purchase preflight"); throw CancellationError() }
        store.purchase = { _, _ in XCTFail("Unexpected purchase sheet"); return .userCancelled }
        store.prepare = { _, _ in }
        store.submit = { _, _, _ in XCTFail("Unexpected receipt delivery") }
        store.currentEntitlements = { AsyncStream { $0.finish() } }
        store.unfinished = { AsyncStream { $0.finish() } }
        store.subscriptions = { _, _ in
            [.init(id: product.subscription!.subscriptionGroupID, ownership: .available)]
        }
        return (session, store, product)
    }

    func testUnavailableAppDownloadDoesNotPromptOrReportAnAccountFailure() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, syncs = 0
        store.appEnvironment = { throw StoreKitError.notEntitled }
        store.sync = { syncs += 1 }
        let purchases = PurchaseModel(enabled: true, store: store)
        for _ in 0..<2 {
            await purchases.load(model)
            XCTAssertEqual(purchases.products.map(\.id), [product.id])
            XCTAssertNil(purchases.loadIssue)
            XCTAssertNil(purchases.subscriptionMessage)
            XCTAssertNil(purchases.feedback)
            XCTAssertNil(model.error)
            XCTAssertTrue(purchases.canPurchase(product))
        }
        await purchases.retry(model)
        XCTAssertEqual(syncs, 0, "Missing app download information needs no account authentication")
        XCTAssertNil(purchases.loadIssue)
        XCTAssertNil(purchases.storeEnvironment, "An unavailable download must not invent a verified purchase environment")
        XCTAssertTrue(purchases.canPurchase(product))
        XCTAssertFalse(purchases.loading)
        XCTAssertFalse(purchases.working)
    }

    func testFreshAppleAccountDoesNotNeedAnAppDownloadTransaction() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, checkedEnvironments: [String] = [], purchasedTokens: [UUID] = []
        store.appEnvironment = { throw StoreKitError.notEntitled }
        let catalogue = try await store.catalogue(model)
        let token = try XCTUnwrap(UUID(uuidString: catalogue.accountToken))
        store.preflight = { _, id, environment in
            XCTAssertEqual(id, product.id)
            checkedEnvironments.append(environment)
            return .init(eligible: true, reason: nil, kind: "subscription")
        }
        store.purchase = { selected, token in
            XCTAssertEqual(selected.id, product.id)
            purchasedTokens.append(token)
            return .userCancelled
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertNil(purchases.loadIssue, "App download validation is not subscription validation")
        XCTAssertNil(purchases.subscriptionMessage)
        XCTAssertNil(purchases.feedback)
        XCTAssertTrue(purchases.canPurchase(product), "A checked empty subscription group permits the first purchase")
        await purchases.purchase(product, model: model)
        XCTAssertEqual(checkedEnvironments, ["Production"], "Unknown context uses the stricter preflight rules")
        XCTAssertEqual(purchasedTokens, [token], "The first purchase reaches StoreKit with this dMsg account's token")
        XCTAssertNil(purchases.loadIssue)
        XCTAssertNil(model.error)
        XCTAssertFalse(purchases.working)
        XCTAssertFalse(purchases.pending, "Cancelling Apple's sheet grants no entitlement")
    }

    func testUnknownEnvironmentStillEnforcesExternalPlanPreflight() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, checks = 0
        store.appEnvironment = { throw StoreKitError.notEntitled }
        store.preflight = { _, _, environment in
            checks += 1
            XCTAssertEqual(environment, "Production")
            return .init(eligible: false, reason: "external_plan", kind: "subscription")
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        await purchases.purchase(product, model: model)
        XCTAssertEqual(checks, 1)
        XCTAssertNotNil(model.notice)
        XCTAssertFalse(purchases.pending)
    }

    func testFirstPurchaseReachesLocalStoreKitWithoutAnAppDownload() async throws {
        let (session, initial, product) = try await storeFixture()
        session.clearTransactions()
        defer { session.clearTransactions(); session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        store.appEnvironment = { throw StoreKitError.notEntitled }
        // Exercise StoreKit's actual empty group and purchase APIs. The only
        // substituted service call is commerce preflight; no real funds move.
        store.subscriptions = PurchaseModel.Store().subscriptions
        store.purchase = PurchaseModel.Store().purchase
        store.preflight = { _, _, environment in
            XCTAssertEqual(environment, "Production")
            return .init(eligible: true, reason: nil, kind: "subscription")
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertNil(purchases.loadIssue)
        XCTAssertTrue(purchases.canPurchase(product))
        await purchases.purchase(product, model: model)
        XCTAssertEqual(session.allTransactions().map(\.productIdentifier), [product.id])
        if case .verified(let transaction) = await product.latestTransaction {
            XCTAssertEqual(transaction.environment, .xcode)
        } else { XCTFail("StoreKit should have produced a local test transaction") }
        // Xcode-signed purchases must still be refused by production delivery.
        XCTAssertNotNil(model.error)
        XCTAssertFalse(purchases.pending)
        XCTAssertEqual(purchases.revision, 0)
    }

    func testKnownSandboxKeepsSandboxPreflight() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, purchasesStarted = 0
        let environment = expectation(description: "Optional Sandbox hint loaded")
        var readEnvironment = false
        store.appEnvironment = {
            if !readEnvironment { readEnvironment = true; environment.fulfill() }
            return "Sandbox"
        }
        store.preflight = { _, _, environment in
            XCTAssertEqual(environment, "Sandbox")
            return .init(eligible: true, reason: nil, kind: "subscription")
        }
        store.purchase = { _, _ in purchasesStarted += 1; return .userCancelled }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        await fulfillment(of: [environment], timeout: 2)
        await purchases.purchase(product, model: model)
        XCTAssertEqual(purchasesStarted, 1)
        XCTAssertNil(model.error)
    }

    func testOptionalAppDownloadDoesNotKeepRestoreWorking() async throws {
        let (session, initial, _) = try await storeFixture()
        session.clearTransactions()
        defer { session.clearTransactions(); session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        var suspended: CheckedContinuation<String, Error>?
        let started = expectation(description: "Optional download lookup started")
        let restored = expectation(description: "Restore completes independently of the app download")
        store.sync = {}
        store.appEnvironment = {
            try await withCheckedThrowingContinuation { suspended = $0; started.fulfill() }
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        let restore = Task { await purchases.restore(model); restored.fulfill() }
        await fulfillment(of: [started, restored], timeout: 2)
        XCTAssertFalse(purchases.working)
        suspended?.resume(throwing: StoreKitError.notEntitled)
        await restore.value
    }

    func testPurchaseStatusFailureInvalidatesPreviousAvailability() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, reads = 0
        store.preflight = { _, _, _ in .init(eligible: true, reason: nil, kind: "subscription") }
        store.subscriptions = { _, _ in
            reads += 1
            if reads > 1 { throw URLError(.notConnectedToInternet) }
            return [.init(id: product.subscription!.subscriptionGroupID, ownership: .available)]
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        await purchases.purchase(product, model: model)
        XCTAssertEqual(purchases.loadIssue, .subscriptions)
        XCTAssertFalse(purchases.canPurchase(product))
        XCTAssertNil(model.error, "A status failure should use the inline retry, not a generic alert")
    }

    func testAppVerificationFailureStopsBeforePayment() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, preparations = 0
        store.preflight = { _, _, _ in .init(eligible: true, reason: nil, kind: "subscription") }
        store.prepare = { _, _ in preparations += 1; throw URLError(.notConnectedToInternet) }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        await purchases.purchase(product, model: model)
        XCTAssertEqual(preparations, 1)
        XCTAssertFalse(purchases.working)
        XCTAssertFalse(purchases.pending)
        XCTAssertNotNil(model.error)
    }

    func testUndeliveredPaymentDisablesRepurchaseOfTheSameProduct() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        let catalogue = try await store.catalogue(model)
        let token = try XCTUnwrap(UUID(uuidString: catalogue.accountToken))
        store.submit = { _, _, _ in throw URLError(.notConnectedToInternet) }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        let receipt = PurchaseModel.Receipt(id: 1, product: product.id, token: token, environment: "Sandbox", jws: "paid", finish: { XCTFail("Unapplied purchase must remain unfinished") })
        do { _ = try await purchases.deliver(receipt, model: model); XCTFail("Delivery should wait for recovery") } catch {}
        await purchases.load(model)
        XCTAssertTrue(purchases.pending)
        XCTAssertFalse(purchases.canPurchase(product))
        await purchases.purchase(product, model: model)
        XCTAssertNil(model.error)
    }

    func testPartialCatalogueOffersRetryWhileKeepingAvailableProductsPurchasable() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        store.catalogue = { _ in
            .init(configuration: .init(bundleId: "net.dmsg.ios", appAppleId: 6827050680,
                products: [.init(productId: product.id), .init(productId: "net.dmsg.ios.pro.annual")]), accountToken: UUID().uuidString)
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertTrue(purchases.hasUnavailableProducts)
        XCTAssertEqual(purchases.products.map(\.id), [product.id])
        XCTAssertTrue(purchases.canPurchase(product))
        XCTAssertNil(purchases.loadIssue)
    }

    func testSignedSubscriptionEnvironmentDoesNotRequireAnAppDownloadLookup() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        store.subscriptions = { _, _ in [.init(id: product.subscription!.subscriptionGroupID, ownership: .current, environment: "Sandbox")] }
        store.appEnvironment = { XCTFail("Signed subscription already supplies the environment"); return "Production" }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertEqual(purchases.storeEnvironment, "Sandbox")
        XCTAssertTrue(purchases.hasCurrentSubscription)
        XCTAssertFalse(purchases.canPurchase(product))
    }

    func testCancellingAppleAuthenticationDoesNotReportAPurchaseFailure() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        store.preflight = { _, _, _ in .init(eligible: true, reason: nil, kind: "subscription") }
        store.purchase = { _, _ in throw StoreKitError.userCancelled }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        await purchases.purchase(product, model: model)
        XCTAssertNil(model.error)
        XCTAssertNil(purchases.feedback)
        XCTAssertFalse(purchases.pending)
        XCTAssertFalse(purchases.working)
        XCTAssertTrue(purchases.canPurchase(product))
    }

    func testCancellingSubscriptionRetryKeepsTheInlineFailureWithoutAnAlert() async throws {
        let (session, initial, _) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        store.subscriptions = { _, _ in throw URLError(.notConnectedToInternet) }
        store.sync = { throw StoreKitError.userCancelled }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        await purchases.retry(model)
        XCTAssertEqual(purchases.loadIssue, .subscriptions)
        XCTAssertFalse(purchases.working)
        XCTAssertNil(model.error)
    }

    func testDeferredPurchaseCannotBeRepeatedAndItsApprovalIsDeliveredByTheListener() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, sheets = 0
        let catalogue = try await store.catalogue(model)
        let token = try XCTUnwrap(UUID(uuidString: catalogue.accountToken))
        let listening = expectation(description: "Updates listener ready")
        let finished = expectation(description: "Approved transaction finished")
        var events: AsyncStream<Result<PurchaseModel.Receipt, Error>>.Continuation?
        store.preflight = { _, _, _ in .init(eligible: true, reason: nil, kind: "subscription") }
        store.purchase = { _, _ in sheets += 1; return .pending }
        store.updates = { AsyncStream { events = $0; listening.fulfill() } }
        store.submit = { _, _, _ in }
        let purchases = PurchaseModel(enabled: true, store: store)
        let listener = Task { await purchases.listen(model) }
        defer { listener.cancel(); events?.finish() }
        await fulfillment(of: [listening], timeout: 2)
        await purchases.load(model)
        await purchases.purchase(product, model: model)
        XCTAssertTrue(purchases.pending)
        XCTAssertFalse(purchases.canPurchase(product))
        await purchases.purchase(product, model: model)
        XCTAssertEqual(sheets, 1)
        events?.yield(.success(.init(id: 1, product: product.id, token: token, environment: "Sandbox", jws: "approved", finish: { finished.fulfill() })))
        await fulfillment(of: [finished], timeout: 2)
        events?.finish()
        await listener.value
        XCTAssertFalse(purchases.pending)
        XCTAssertEqual(purchases.revision, 1)
    }

    func testSlowAppDownloadReadDoesNotDelayFirstPurchaseReadiness() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial
        var suspended: CheckedContinuation<String, Error>?
        let started = expectation(description: "Optional app download lookup started")
        store.appEnvironment = {
            try await withCheckedThrowingContinuation { suspended = $0; started.fulfill() }
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        let read = Task { await purchases.load(model) }
        await fulfillment(of: [started], timeout: 2)
        XCTAssertFalse(purchases.loading)
        XCTAssertTrue(purchases.canPurchase(product))
        XCTAssertNil(purchases.loadIssue)
        suspended?.resume(throwing: StoreKitError.notEntitled)
        await read.value
        XCTAssertTrue(purchases.canPurchase(product))
        XCTAssertNil(model.error)
    }

    func testMissingAppDownloadDoesNotBypassForeignOrUnverifiedSubscriptions() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        for owner in [UUID?.some(UUID()), nil] {
            let model = SessionModel(); model.loadFixtures()
            var store = initial
            store.appEnvironment = { throw StoreKitError.notEntitled }
            store.subscriptions = { _, token in
                return [PurchaseModel.subscription(group: product.subscription!.subscriptionGroupID,
                    statuses: [.init(token: owner, state: .subscribed)], expected: token)]
            }
            let purchases = PurchaseModel(enabled: true, store: store)
            await purchases.load(model)
            XCTAssertNil(purchases.loadIssue)
            XCTAssertNotNil(purchases.subscriptionMessage)
            XCTAssertFalse(purchases.canPurchase(product))
            await purchases.purchase(product, model: model)
        }
    }

    func testPurchaseRechecksOwnershipAfterPreflightWithoutAnAppDownload() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, reads = 0
        store.appEnvironment = { throw StoreKitError.notEntitled }
        store.preflight = { _, _, _ in .init(eligible: true, reason: nil, kind: "subscription") }
        store.subscriptions = { _, token in
            reads += 1
            let statuses: [PurchaseModel.SubscriptionStatus] = reads == 1 ? [] : [.init(token: UUID(), state: .subscribed)]
            return [PurchaseModel.subscription(group: product.subscription!.subscriptionGroupID, statuses: statuses, expected: token)]
        }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertTrue(purchases.canPurchase(product))
        await purchases.purchase(product, model: model)
        XCTAssertEqual(reads, 2)
        XCTAssertEqual(purchases.feedback, PurchaseModel.otherAccountMessage)
        XCTAssertFalse(purchases.canPurchase(product))
        XCTAssertFalse(purchases.pending)
    }

    func testUnavailableSubscriptionStatusIsNotAnEmptyNewAccount() async throws {
        let (session, initial, product) = try await storeFixture()
        defer { session.resetToDefaultState() }
        let model = SessionModel(); model.loadFixtures()
        var store = initial, reads = 0, syncs = 0
        store.appEnvironment = { throw StoreKitError.notEntitled }
        store.sync = { syncs += 1 }
        store.subscriptions = { _, token in
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
        await purchases.purchase(product, model: model)
        XCTAssertEqual(syncs, 0)
        await purchases.retry(model)
        XCTAssertEqual(syncs, 1, "Only explicit retry may synchronize subscription credentials")
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
            store.appEnvironment = { XCTFail("An earlier failure must not request the app download"); return "Sandbox" }
            let purchases = PurchaseModel(enabled: true, store: store)
            await purchases.load(model)
            XCTAssertEqual(purchases.loadIssue, stage)
            XCTAssertTrue(purchases.products.isEmpty)
            XCTAssertNil(purchases.subscriptionMessage)
            XCTAssertNil(purchases.feedback)
            XCTAssertNil(model.error)
        }
    }

    func testAccountSwitchRejectsALateAppDownloadFailureWithoutPrompting() async throws {
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
        store.subscriptions = { _, token in
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
        XCTAssertEqual(checkedTokens, [firstToken, nextToken])
        XCTAssertNil(purchases.loadIssue)
        XCTAssertNil(purchases.subscriptionMessage)
        XCTAssertNil(model.error)
        XCTAssertTrue(purchases.canPurchase(product))
    }

}
