import Observation
import StoreKit
@preconcurrency import DMsgCore

@MainActor @Observable
final class PurchaseModel {
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

    // A changed token cannot claim an existing original subscription. Expired
    // originals keep their account binding, just as commerce does.
    static func ownership(originalToken: UUID?, transactionToken: UUID?, renewalToken: UUID?, expected: UUID, active: Bool) -> Ownership {
        guard let originalToken, let transactionToken else { return .unverified }
        guard originalToken == expected, transactionToken == expected else { return .otherAccount }
        if let renewalToken, renewalToken != expected { return .unverified }
        return active ? .current : .available
    }

    private func current(_ model: SessionModel, epoch: UInt64, account: String?) -> Bool {
        !Task.isCancelled && model.epoch == epoch && model.unlocked && model.session?.account == account
    }

    /// Reads verified subscription-group status, including billing retry and the
    /// next renewal. The original transaction prevents re-binding after a tier change.
    private func inspect(_ products: [Product], token: UUID, environment: String) async throws -> [Subscription] {
        let groups = Set(products.compactMap { $0.subscription?.subscriptionGroupID })
        var statuses: [String: [Product.SubscriptionInfo.Status]] = [:]
        var originals = Set<UInt64>()
        for group in groups {
            let values = try await Product.SubscriptionInfo.status(for: group)
            try Task.checkCancellation()
            statuses[group] = values
            for value in values {
                guard case .verified(let transaction) = value.transaction,
                      Self.environmentName(transaction.environment) == environment else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
                originals.insert(transaction.originalID)
            }
        }
        var originalTokens: [UInt64: UUID] = [:]
        if !originals.isEmpty {
            for await value in Transaction.all {
                try Task.checkCancellation()
                guard case .verified(let transaction) = value,
                      Self.environmentName(transaction.environment) == environment,
                      transaction.id == transaction.originalID, originals.contains(transaction.id) else { continue }
                originalTokens[transaction.id] = transaction.appAccountToken
                originals.remove(transaction.id)
                if originals.isEmpty { break }
            }
        }
        return try groups.sorted().map { group in
            var summary = Subscription(id: group, ownership: .available)
            let values = statuses[group] ?? []
            let activeValues = values.filter { $0.state != .expired && $0.state != .revoked }
            for status in activeValues.isEmpty ? values : activeValues {
                guard case .verified(let transaction) = status.transaction,
                      case .verified(let renewal) = status.renewalInfo else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
                let active = status.state != .expired && status.state != .revoked
                let owner = Self.ownership(originalToken: originalTokens[transaction.originalID], transactionToken: transaction.appAccountToken,
                    renewalToken: renewal.appAccountToken, expected: token, active: active)
                if owner == .otherAccount || owner == .unverified { return Subscription(id: group, ownership: owner) }
                if owner == .current {
                    summary = Subscription(id: group, ownership: owner,
                        nextProduct: renewal.willAutoRenew && renewal.autoRenewPreference != transaction.productID ? renewal.autoRenewPreference : nil,
                        renewalDate: renewal.renewalDate)
                }
            }
            return summary
        }
    }

    func load(_ model: SessionModel, reportErrors: Bool = true) async {
        guard enabled else { return }
        guard let engine = model.engine, model.unlocked else { return }; let epoch = model.epoch
        loadRevision &+= 1; let read = loadRevision
        if account != model.session?.account { products = []; token = nil; pending = false; feedback = nil; subscriptions = []; account = model.session?.account }
        let expectedAccount = account
        do {
            guard case .verified(let app) = try await AppTransaction.shared,
                  let environment = Self.environmentName(app.environment) else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            storeEnvironment = environment
            let catalogue: Catalogue = try model.decode(try await engine.storeProducts())
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            guard let config = catalogue.configuration else { products = []; token = nil; subscriptions = []; return }
            guard config.bundleId == "net.dmsg.ios", config.appAppleId == 6827050680,
                  let token = UUID(uuidString: catalogue.accountToken) else { throw CocoaError(.coderInvalidValue) }
            let found = try await Product.products(for: config.products.map(\.productId))
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            // Sandbox admits annual subscriptions only; commerce refuses its storage packs.
            let offered = found.filter { environment != "Sandbox" || $0.type == .autoRenewable }.sorted { $0.price < $1.price }
            let checked = try await inspect(offered, token: token, environment: environment)
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            self.token = token; products = offered; subscriptions = checked
            if feedback == Self.unverifiedMessage { feedback = nil }
        } catch {
            if current(model, epoch: epoch, account: expectedAccount), read == loadRevision {
                token = nil; products = []; subscriptions = []; feedback = Self.unverifiedMessage
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
            await load(model)
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
        for await value in Transaction.unfinished {
            if Task.isCancelled { return }
            let epoch = model.epoch
            do { _ = try await deliver(value, model: model) } catch { if !Task.isCancelled, epoch == model.epoch { pending = true } }
        }
        for await value in Transaction.updates {
            if Task.isCancelled { return }
            let epoch = model.epoch
            do { _ = try await deliver(value, model: model) } catch { if !Task.isCancelled, epoch == model.epoch { pending = true } }
        }
    }

    static func deliveryIssue(token: UUID?, expected: UUID, subscription: Ownership?) -> Delivery? {
        guard let token else { return .unverified }
        if token != expected || subscription == .otherAccount { return .otherAccount }
        if subscription == .unverified { return .unverified }
        return nil
    }

    private func deliver(_ value: VerificationResult<Transaction>, model: SessionModel) async throws -> Delivery {
        guard enabled else { return .stale }
        guard case .verified(let transaction) = value, Self.environmentName(transaction.environment) != nil else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        guard let token, let account, account == model.session?.account,
              model.unlocked, !Task.isCancelled, let engine = model.engine, let files = model.files else { return .stale }
        let owner = subscriptions.first { $0.id == transaction.subscriptionGroupID }?.ownership
        if let issue = Self.deliveryIssue(token: transaction.appAccountToken, expected: token, subscription: owner) { return issue }
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
