import Observation
import OSLog
import StoreKit
@preconcurrency import DMsgCore

@MainActor @Observable
final class PurchaseModel {
    private static let logger = Logger(subsystem: "net.dmsg.ios", category: "StoreKit")
    // Missing configuration fails closed, including transaction replay.
    let enabled: Bool

    init(enabled: Bool = (Bundle.main.object(forInfoDictionaryKey: "DMsgInAppPurchasesEnabled") as? String) == "YES") {
        #if DEBUG
        // Presentation fixtures must never start real StoreKit operations.
        self.enabled = ProcessInfo.processInfo.arguments.contains("--ui-testing") ? false : enabled
        #else
        self.enabled = enabled
        #endif
    }

    var showsControls: Bool {
        #if DEBUG
        let args = ProcessInfo.processInfo.arguments
        if args.contains("--ui-testing") && (args.contains("--ui-purchases") || args.contains("--ui-sandbox-subscription") || args.contains("--ui-max-subscription") || args.contains("--ui-foreign-subscription") || args.contains("--ui-scheduled-subscription")) { return true }
        #endif
        return enabled
    }

    struct Catalogue: Decodable {
        struct Configuration: Decodable {
            struct ProductID: Decodable { let productId: String }
            let bundleId: String; let appAppleId: UInt64; let products: [ProductID]
        }
        let configuration: Configuration?; let accountToken: String
    }
    private(set) var products: [Product] = []
    private(set) var loading = false
    private(set) var working = false
    private(set) var pending = false
    private(set) var storeEnvironment: String?
    private(set) var revision = 0
    enum Ownership: Equatable { case available, current, otherAccount, unverified }
    struct Subscription: Identifiable {
        let id: String
        let ownership: Ownership
        var nextProduct: String? = nil
        var renewalDate: Date? = nil
    }
    struct SubscriptionStatus {
        let token: UUID?
        let state: Product.SubscriptionInfo.RenewalState
        var nextProduct: String? = nil
        var renewalDate: Date? = nil

        var active: Bool { state != .expired && state != .revoked }
    }
    enum Delivery: Equatable { case applied, otherAccount, unverified, stale }
    private(set) var subscriptions: [Subscription] = []
    private(set) var feedback: String?
    private var loadRevision = 0
    private var account: String?
    private var token: UUID?

    static var otherAccountMessage: String {
        String(localized: "This Apple subscription belongs to another dMsg account. Switch accounts to manage it. Restoring purchases does not transfer it.")
    }
    static var unverifiedMessage: String {
        String(localized: "Unable to verify the account linked to this Apple subscription. Try again before purchasing.")
    }
    var subscriptionMessage: String? {
        if subscriptions.contains(where: { $0.ownership == .otherAccount }) { return Self.otherAccountMessage }
        if subscriptions.contains(where: { $0.ownership == .unverified }) { return Self.unverifiedMessage }
        return nil
    }
    var hasCurrentSubscription: Bool { subscriptions.contains { $0.ownership == .current } }

    func canPurchase(_ product: Product) -> Bool {
        canPurchase(group: product.subscription?.subscriptionGroupID)
    }

    func canPurchase(group: String?) -> Bool {
        guard let group else { return subscriptionMessage == nil }
        return subscriptions.first(where: { $0.id == group })?.ownership == .available
    }

    // Apple's historical token is not commerce's durable original-subscription
    // binding. It can differ from the latest token after an earlier purchase or
    // account-token update. Only the service can accept or reject that binding.
    static func subscription(group: String, statuses: [SubscriptionStatus], expected: UUID) -> Subscription {
        let active = statuses.filter(\.active)
        // A fully expired group is a new purchase, with the new account's token.
        // Billing retry/grace and revocation do not count as full expiry here.
        if statuses.allSatisfy({ $0.state == .expired }) { return Subscription(id: group, ownership: .available) }
        let relevant = active.isEmpty ? statuses : active
        if let owned = relevant.first(where: { $0.token == expected }) {
            return Subscription(id: group, ownership: owned.active ? .current : .available,
                nextProduct: owned.active ? owned.nextProduct : nil, renewalDate: owned.active ? owned.renewalDate : nil)
        }
        if relevant.contains(where: { $0.token == nil }) { return Subscription(id: group, ownership: .unverified) }
        return Subscription(id: group, ownership: relevant.isEmpty ? .available : .otherAccount)
    }

    private func current(_ model: SessionModel, epoch: UInt64, account: String?) -> Bool {
        !Task.isCancelled && model.epoch == epoch && model.unlocked && model.session?.account == account
    }

    /// Reads current verified transactions, including billing retry and renewal.
    /// Historical transactions must not veto a matching current transaction.
    private func inspect(_ products: [Product], token: UUID, environment: String) async throws -> [Subscription] {
        let groups = Set(products.compactMap { $0.subscription?.subscriptionGroupID })
        var subscriptions: [Subscription] = []
        for group in groups.sorted() {
            let values = try await Product.SubscriptionInfo.status(for: group)
            try Task.checkCancellation()
            let statuses = try values.map { value -> SubscriptionStatus in
                guard case .verified(let transaction) = value.transaction,
                      case .verified(let renewal) = value.renewalInfo,
                      Self.environmentName(transaction.environment) == environment else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
                // Renewal metadata describes a future transaction, not ownership
                // of the current one. Only show its plan for the same account.
                let sameRenewalAccount = renewal.appAccountToken == nil || renewal.appAccountToken == transaction.appAccountToken
                return SubscriptionStatus(token: transaction.appAccountToken, state: value.state,
                    nextProduct: sameRenewalAccount && renewal.willAutoRenew && renewal.autoRenewPreference != transaction.productID ? renewal.autoRenewPreference : nil,
                    renewalDate: sameRenewalAccount ? renewal.renewalDate : nil)
            }
            subscriptions.append(Self.subscription(group: group, statuses: statuses, expected: token))
        }
        return subscriptions
    }

    func load(_ model: SessionModel, reportErrors: Bool = true) async {
        guard enabled else { return }
        guard let engine = model.engine, model.unlocked else { return }; let epoch = model.epoch
        loadRevision &+= 1; let read = loadRevision
        loading = true
        defer { if read == loadRevision { loading = false } }
        if account != model.session?.account { products = []; pending = false; feedback = nil; subscriptions = []; storeEnvironment = nil; account = model.session?.account }
        token = nil
        let expectedAccount = account
        do {
            guard case .verified(let app) = try await AppTransaction.shared,
                  let environment = Self.environmentName(app.environment) else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            storeEnvironment = environment
            let catalogue: Catalogue = try model.decode(try await engine.storeProducts())
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            guard let config = catalogue.configuration else {
                Self.logger.error("Commerce has no App Store product configuration")
                products = []; token = nil; subscriptions = []; return
            }
            guard config.bundleId == "net.dmsg.ios", config.appAppleId == 6827050680,
                  let token = UUID(uuidString: catalogue.accountToken) else { throw CocoaError(.coderInvalidValue) }
            // Restore needs the account token, even if Apple's product/status
            // lookup fails. Each submitted transaction is verified independently.
            self.token = token
            let found = try await Product.products(for: config.products.map(\.productId))
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            let missing = Set(config.products.map(\.productId)).subtracting(found.map(\.id)).sorted()
            if !missing.isEmpty {
                // Product identifiers are public. Never log account tokens or receipts.
                Self.logger.warning("App Store products unavailable in \(environment, privacy: .public): \(missing.joined(separator: ","), privacy: .public)")
            }
            // Sandbox admits annual subscriptions only; commerce refuses its storage packs.
            let offered = found.filter { environment != "Sandbox" || $0.type == .autoRenewable }.sorted { $0.price < $1.price }
            let checked = try await inspect(offered, token: token, environment: environment)
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            self.token = token; products = offered; subscriptions = checked
            if feedback == Self.unverifiedMessage { feedback = nil }
        } catch {
            if current(model, epoch: epoch, account: expectedAccount), read == loadRevision {
                products = []; subscriptions = []; feedback = Self.unverifiedMessage
                if reportErrors { model.report(error) }
            }
        }
    }

    func purchase(_ product: Product, model: SessionModel) async {
        guard enabled, !working, let token, let storeEnvironment, account == model.session?.account else { return }
        working = true; feedback = nil; defer { working = false }
        let epoch = model.epoch, expectedAccount = account
        do {
            struct Check: Decodable { let eligible: Bool; let reason: String?; let kind: String }
            guard let engine = model.engine else { return }
            let check: Check = try model.decode(try await engine.purchasePreflight(product: product.id, environment: storeEnvironment))
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            guard check.eligible else {
                switch check.reason {
                case "external_plan": model.notice = String(localized: "Your current plan is managed outside the App Store. Wait until it ends before starting an App Store subscription.")
                case "subscription_required": model.notice = String(localized: "Storage purchases require an active App Store subscription.")
                default: model.notice = String(localized: "Purchases are not currently available.")
                }
                return
            }
            guard (check.kind == "subscription" && product.type == .autoRenewable) || (check.kind == "storage" && product.type == .consumable) else {
                throw MobileError.Failure(code: "UNSUPPORTED_PROTOCOL")
            }
            // Recheck immediately before Apple's sheet, not only when the page loaded.
            let checked = try await inspect(products, token: token, environment: storeEnvironment)
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            subscriptions = checked
            guard canPurchase(product) else {
                feedback = subscriptionMessage ?? String(localized: "Change your plan or cancel renewal in Manage subscriptions.")
                return
            }
            let result = try await product.purchase(options: [.appAccountToken(token)])
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            switch result {
            case .success(let verification):
                show(try await deliver(verification, model: model), model: model, epoch: epoch, account: expectedAccount)
            case .pending: pending = true
            case .userCancelled: break
            @unknown default: break
            }
            if current(model, epoch: epoch, account: expectedAccount) { await load(model, reportErrors: false) }
        } catch { if current(model, epoch: epoch, account: expectedAccount) { model.report(error) } }
    }

    func restore(_ model: SessionModel) async {
        guard enabled, !working, model.unlocked else { return }
        working = true; feedback = nil; defer { working = false }
        let epoch = model.epoch, expectedAccount = model.session?.account
        do {
            try await AppStore.sync()
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            await load(model, reportErrors: false)
            guard current(model, epoch: epoch, account: expectedAccount), token != nil else { return }
            var applied = false, foreign = false, unverified = false
            var seen = Set<UInt64>()
            for stream in [Transaction.currentEntitlements, Transaction.unfinished] {
                for await value in stream {
                    guard current(model, epoch: epoch, account: expectedAccount) else { return }
                    if case .verified(let transaction) = value, !seen.insert(transaction.id).inserted { continue }
                    switch try await deliver(value, model: model) {
                    case .applied: applied = true
                    case .otherAccount: foreign = true
                    case .unverified: unverified = true
                    case .stale: return
                    }
                }
            }
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            if applied { feedback = String(localized: "Your purchase has been applied to this dMsg account.") }
            else if foreign { feedback = Self.otherAccountMessage }
            else if unverified { feedback = Self.unverifiedMessage }
            else { feedback = subscriptionMessage ?? String(localized: "No purchases were found for this dMsg account.") }
        } catch { if current(model, epoch: epoch, account: expectedAccount) { model.report(error) } }
    }

    private func show(_ result: Delivery, model: SessionModel, epoch: UInt64, account: String?) {
        guard current(model, epoch: epoch, account: account) else { return }
        switch result {
        case .applied: feedback = String(localized: "Your purchase has been applied to this dMsg account.")
        case .otherAccount: feedback = Self.otherAccountMessage
        case .unverified: feedback = Self.unverifiedMessage
        case .stale: break
        }
    }

    func listen(_ model: SessionModel) async {
        guard enabled else { return }
        await load(model, reportErrors: false)
        // A finished purchase can still need server-side reconciliation after a
        // missed renewal or repaired account token. Finishing is not entitlement.
        var seen = Set<UInt64>()
        for stream in [Transaction.currentEntitlements, Transaction.unfinished] {
            for await value in stream {
                if Task.isCancelled { return }
                if case .verified(let transaction) = value, !seen.insert(transaction.id).inserted { continue }
                let epoch = model.epoch
                do { _ = try await deliver(value, model: model) } catch { if !Task.isCancelled, epoch == model.epoch { pending = true } }
            }
        }
        for await value in Transaction.updates {
            if Task.isCancelled { return }
            let epoch = model.epoch
            do { _ = try await deliver(value, model: model) } catch { if !Task.isCancelled, epoch == model.epoch { pending = true } }
        }
    }

    static func deliveryIssue(token: UUID?, expected: UUID) -> Delivery? {
        guard let token else { return .unverified }
        return token == expected ? nil : .otherAccount
    }

    private func deliver(_ value: VerificationResult<Transaction>, model: SessionModel) async throws -> Delivery {
        guard enabled else { return .stale }
        guard case .verified(let transaction) = value, Self.environmentName(transaction.environment) != nil else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        guard let token, let account, account == model.session?.account,
              model.unlocked, !Task.isCancelled, let engine = model.engine, let files = model.files else { return .stale }
        if let issue = Self.deliveryIssue(token: transaction.appAccountToken, expected: token) { return issue }
        let epoch = model.epoch
        pending = true
        try await model.apple.submitPurchase(engine: engine, files: files, account: account, jws: value.jwsRepresentation)
        guard current(model, epoch: epoch, account: account) else { return .stale }
        // Apple payment is finished only after the authoritative service applied it.
        await transaction.finish()
        guard current(model, epoch: epoch, account: account) else { return .stale }
        pending = false; revision &+= 1; await model.updateSession()
        return .applied
    }

    #if DEBUG
    func installSubscriptionFixture() {
        let args = ProcessInfo.processInfo.arguments
        working = args.contains("--ui-purchase-working")
        loading = args.contains("--ui-products-loading")
        if args.contains("--ui-foreign-subscription") {
            subscriptions = [Subscription(id: "fixture", ownership: .otherAccount)]
        } else if args.contains("--ui-scheduled-subscription") {
            subscriptions = [Subscription(id: "fixture", ownership: .current, nextProduct: "Plus", renewalDate: Date.now.addingTimeInterval(86400))]
        }
    }
    #endif

    static func environmentName(_ value: AppStore.Environment) -> String? {
        switch value {
        case .production: return "Production"
        case .sandbox: return "Sandbox"
        default: return nil
        }
    }

    static func requireApplied(_ data: Data) throws {
        struct Result: Decodable { let state: String }
        guard try JSONDecoder().decode(Result.self, from: data).state == "applied" else {
            throw MobileError.Failure(code: "PURCHASE_PENDING")
        }
    }
}
