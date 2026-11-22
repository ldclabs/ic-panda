import XCTest
@testable import dMsg

@MainActor
final class MembershipPlansTests: XCTestCase {
    @MainActor private final class CatalogueFailure { var enabled = false }
    func testHostedAgentCountComesFromTheAccount() throws {
        let decoder = JSONDecoder()
        let listing = try decoder.decode(AgentsView.Listing.self, from: Data(#"{"agents":[]}"#.utf8))
        let quota = try decoder.decode(AgentsView.Quota.self, from: Data(#"{"used":2,"limit":3}"#.utf8))
        XCTAssertTrue(listing.agents.isEmpty)
        XCTAssertEqual(quota.used, 2)
        XCTAssertEqual(quota.limit, 3)
        let unknown = try decoder.decode(AgentsView.Quota.self, from: Data(#"{"used":2,"limit":null}"#.utf8))
        XCTAssertNil(unknown.limit)
    }

    func testHostedAgentLimitsInComparison() {
        XCTAssertEqual(MembershipCatalogue.fixture.ordered.map { MembershipBenefit.agentIds.value($0) }, [1, 3, 5, 10].map { $0.formatted() })
    }

    func testCatalogueKeepsVerifiedLimitsAndUnknownWalletQuotaDistinctFromZero() throws {
        let data = Data(#"{"plans":[{"plan_id":"Pro","price_cents":999999,"limits":{"storage_bytes":9876543210,"active_channels":73,"monthly_execution_units":129,"active_devices":5,"backup_max_file_bytes":104857600,"file_request_links":50,"team_wallets":null}}]}"#.utf8)
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let catalogue = try decoder.decode(MembershipCatalogue.self, from: data)
        let pro = try XCTUnwrap(catalogue.ordered.first)
        XCTAssertEqual(pro.tier, .pro)
        XCTAssertEqual(pro.limits.storageBytes, 9876543210)
        XCTAssertEqual(pro.limits.activeChannels, 73)
        XCTAssertEqual(pro.limits.monthlyExecutionUnits, 129)
        XCTAssertNil(pro.limits.teamWallets)
        XCTAssertEqual(MembershipBenefit.wallets.value(pro), String(localized: "Unavailable"))
        XCTAssertNotEqual(MembershipBenefit.wallets.value(pro), "0")
    }

    func testGovernedProductMappingDoesNotGuessTierFromProductName() async throws {
        let model = SessionModel(); model.loadFixtures()
        let token = UUID()
        let data = Data("""
        {"configuration":{"bundle_id":"net.dmsg.ios","app_apple_id":6827050680,"products":[
        {"product_id":"legacy-pro-name","kind":{"Plan":"Plus"}},
        {"product_id":"new-annual","kind":{"Plan":"Max"}},
        {"product_id":"storage","kind":{"Storage":[1,2,3]}}]},"account_token":"\(token.uuidString)"}
        """.utf8)
        var store = PurchaseModel.Store()
        let failure = CatalogueFailure()
        store.catalogue = { _ in
            if failure.enabled { throw CocoaError(.fileReadUnknown) }
            let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(PurchaseModel.Catalogue.self, from: data)
        }
        store.products = { _ in [] }; store.subscriptions = { _, _ in [] }
        store.appEnvironment = { "Sandbox" }
        let purchases = PurchaseModel(enabled: true, store: store)
        await purchases.load(model)
        XCTAssertEqual(purchases.productPlans, ["legacy-pro-name": "Plus", "new-annual": "Max"])
        failure.enabled = true
        await purchases.load(model)
        XCTAssertTrue(purchases.productPlans.isEmpty)
    }

    func testManagedPlansOfferChangesRatherThanAnotherSubscription() {
        XCTAssertEqual(MembershipTier.max.actionTitle(current: "Plus", managed: true), String.localizedStringWithFormat(String(localized: "Upgrade to %@"), "Max"))
        XCTAssertEqual(MembershipTier.plus.actionTitle(current: "Max", managed: true), String(localized: "Change plan"))
        XCTAssertEqual(MembershipTier.pro.actionTitle(current: nil, managed: false), String.localizedStringWithFormat(String(localized: "Subscribe to %@"), "Pro"))
    }

    func testStoreKitPriceAndPeriodArePreservedWithoutCashPriceConversion() {
        let result = SubscriptionView.price("HK$388.00", period: "1年")
        XCTAssertTrue(result.contains("HK$388.00"))
        XCTAssertTrue(result.contains("1年"))
        XCTAssertEqual(SubscriptionView.price("€49,99", period: nil), "€49,99")
    }
}
