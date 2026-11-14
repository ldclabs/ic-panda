import Observation
import OSLog
import StoreKit
@preconcurrency import DMsgCore

@MainActor @Observable
final class PurchaseModel {
    private static let logger = Logger(subsystem: "net.dmsg.ios", category: "StoreKit")
    // Missing configuration fails closed, including transaction replay.
    let enabled: Bool
    @ObservationIgnored private let store: Store

    init(enabled: Bool = (Bundle.main.object(forInfoDictionaryKey: "DMsgInAppPurchasesEnabled") as? String) == "YES", store: Store = Store()) {
        self.store = store
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
        if args.contains("--ui-testing") && (args.contains("--ui-purchases") || args.contains("--ui-sandbox-subscription") || args.contains("--ui-max-subscription") || args.contains("--ui-foreign-subscription") || args.contains("--ui-scheduled-subscription") || args.contains("--ui-subscription-status-unavailable") || args.contains("--ui-stale-subscription") || args.contains("--ui-expired-subscription")) { return true }
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
    struct PurchaseCheck: Decodable {
        let eligible: Bool; let reason: String?; let kind: String
    }
    // Keep the external reads replaceable so account switches and partial
    // StoreKit failures can be exercised without real payments or credentials.
    struct Store {
        var catalogue: @MainActor (SessionModel) async throws -> Catalogue = { model in
            guard let engine = model.engine else { throw CancellationError() }
            return try model.decode(try await engine.storeProducts())
        }
        var products: @MainActor ([String]) async throws -> [Product] = { try await Product.products(for: $0) }
        // AppTransaction describes the app download, not an in-app purchase.
        // Its environment is only a hint; a first-time buyer may not have it.
        var appEnvironment: @MainActor () async throws -> String = { try verifiedEnvironment(try await AppTransaction.shared) }
        var subscriptions: @MainActor ([Product], UUID) async throws -> [Subscription] = { try await inspect($0, token: $1) }
        var sync: @MainActor () async throws -> Void = { try await AppStore.sync() }
        var preflight: @MainActor (SessionModel, String, String) async throws -> PurchaseCheck = { model, product, environment in
            guard let engine = model.engine else { throw CancellationError() }
            return try model.decode(try await engine.purchasePreflight(product: product, environment: environment))
        }
        var purchase: @MainActor (Product, UUID) async throws -> Product.PurchaseResult = {
            try await $0.purchase(options: [.appAccountToken($1)])
        }
        var prepare: @MainActor (SessionModel, String) async throws -> Void = { model, account in
            guard let engine = model.engine, let files = model.files else { throw CancellationError() }
            try await model.apple.preparePurchase(engine: engine, files: files, account: account)
        }
        var submit: @MainActor (SessionModel, String, String) async throws -> Void = { model, account, jws in
            guard let engine = model.engine, let files = model.files else { throw CancellationError() }
            try await model.apple.submitPurchase(engine: engine, files: files, account: account, jws: jws)
        }
        var currentEntitlements: @MainActor () -> AsyncStream<Result<Receipt, Error>> = { receipts(Transaction.currentEntitlements) }
        var unfinished: @MainActor () -> AsyncStream<Result<Receipt, Error>> = { receipts(Transaction.unfinished) }
        var updates: @MainActor () -> AsyncStream<Result<Receipt, Error>> = { receipts(Transaction.updates) }
    }

    // The production adapter constructs this only from a verified Apple JWS.
    // Keeping delivery separate lets tests exercise outages and duplicate events
    // without accepting Xcode-signed purchases in a distribution build.
    struct Receipt: Sendable {
        let id: UInt64
        let product: String
        let token: UUID?
        let environment: String
        let jws: String
        let finish: @Sendable () async -> Void

        var key: String { "\(environment):\(id)" }
    }

    private static func receipt(_ value: VerificationResult<Transaction>) throws -> Receipt {
        guard case .verified(let transaction) = value,
              let environment = environmentName(transaction.environment) else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        return Receipt(id: transaction.id, product: transaction.productID, token: transaction.appAccountToken,
            environment: environment, jws: value.jwsRepresentation, finish: { await transaction.finish() })
    }

    private static func receipts(_ values: Transaction.Transactions) -> AsyncStream<Result<Receipt, Error>> {
        AsyncStream { continuation in
            let task = Task {
                for await value in values {
                    if Task.isCancelled { break }
                    continuation.yield(Result { try receipt(value) })
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
    enum LoadIssue: String {
        case catalogue, products, subscriptions

        var message: String {
            switch self {
            case .catalogue, .products: String(localized: "Purchases are not currently available.")
            case .subscriptions: String(localized: "Your Apple subscription status could not be checked. Retry before purchasing.")
            }
        }
    }
    private(set) var products: [Product] = []
    private(set) var hasUnavailableProducts = false
    private(set) var loadIssue: LoadIssue?
    private(set) var loading = false
    private(set) var working = false
    var pending: Bool { deliveryPending || approvalPending || refreshPending }
    var deliveryPending: Bool { !pendingTransactions.isEmpty }
    var approvalPending: Bool { !awaitingApproval.isEmpty }
    var refreshPending: Bool { !pendingRefreshes.isEmpty }
    private(set) var storeEnvironment: String?
    private(set) var revision = 0
    enum Ownership: Equatable { case available, current, otherAccount, unverified }
    struct Subscription: Identifiable {
        let id: String
        let ownership: Ownership
        var nextProduct: String? = nil
        var renewalDate: Date? = nil
        var environment: String? = nil
        var state: Product.SubscriptionInfo.RenewalState? = nil
        var expiresAt: Date? = nil
    }
    struct SubscriptionStatus {
        let token: UUID?
        let state: Product.SubscriptionInfo.RenewalState
        var nextProduct: String? = nil
        var renewalDate: Date? = nil
        var environment: String? = nil
        var expiresAt: Date? = nil

        var active: Bool { state != .expired && state != .revoked }
    }
    enum Delivery: Equatable { case applied, otherAccount, unverified, stale }
    private(set) var subscriptions: [Subscription] = []
    private(set) var feedback: String?
    private var loadRevision = 0
    private var account: String?
    private var token: UUID?
    private var environmentTask: Task<Void, Never>?
    private var pendingTransactions: [String: String] = [:]
    private var pendingRefreshes = Set<String>()
    private var awaitingApproval = Set<String>()
    private struct InFlight {
        let jws: String
        let task: Task<Delivery, Error>
    }
    private var deliveries: [String: InFlight] = [:]

    static var otherAccountMessage: String {
        String(localized: "This Apple subscription belongs to another dMsg account. Switch accounts to manage it. Restoring purchases does not transfer it.")
    }
    static var unverifiedMessage: String {
        String(localized: "Unable to verify the account linked to this Apple subscription. Try again before purchasing.")
    }
    static var purchaseNotStartedMessage: String {
        String(localized: "The purchase could not be started. No payment request was sent to Apple.")
    }
    var subscriptionMessage: String? {
        if subscriptions.contains(where: { $0.ownership == .otherAccount }) { return Self.otherAccountMessage }
        if subscriptions.contains(where: { $0.ownership == .unverified }) { return Self.unverifiedMessage }
        return nil
    }
    var hasCurrentSubscription: Bool { subscriptions.contains { $0.ownership == .current } }

    func canPurchase(_ product: Product) -> Bool {
        guard !pendingTransactions.values.contains(product.id), !awaitingApproval.contains(product.id),
              products.contains(where: { $0.id == product.id }) else { return false }
        if let group = product.subscription?.subscriptionGroupID,
           products.contains(where: { $0.subscription?.subscriptionGroupID == group && awaitingApproval.contains($0.id) }) { return false }
        return canPurchase(group: product.subscription?.subscriptionGroupID)
    }

    func canPurchase(group: String?) -> Bool {
        guard !loading, loadIssue == nil, token != nil, !deliveryPending else { return false }
        guard let group else { return subscriptionMessage == nil }
        return subscriptions.first(where: { $0.id == group })?.ownership == .available
    }

    // Apple's historical token is not commerce's durable original-subscription
    // binding. It can differ from the latest token after an earlier purchase or
    // account-token update. Only the service can accept or reject that binding.
    static func subscription(group: String, statuses: [SubscriptionStatus], expected: UUID) -> Subscription {
        let active = statuses.filter(\.active)
        let environments = Set(statuses.compactMap(\.environment))
        let environment = environments.count == 1 ? environments.first : nil
        // A fully expired group is a new purchase, with the new account's token.
        // Billing retry/grace and revocation do not count as full expiry here.
        if statuses.allSatisfy({ $0.state == .expired }) {
            let ended = statuses.filter { $0.token == expected }.max { ($0.expiresAt ?? .distantPast) < ($1.expiresAt ?? .distantPast) }
            return Subscription(id: group, ownership: .available, environment: environment, state: ended?.state, expiresAt: ended?.expiresAt)
        }
        let relevant = active.isEmpty ? statuses : active
        if let owned = relevant.first(where: { $0.token == expected }) {
            return Subscription(id: group, ownership: owned.active ? .current : .available,
                nextProduct: owned.active ? owned.nextProduct : nil, renewalDate: owned.active ? owned.renewalDate : nil,
                environment: environment, state: owned.state, expiresAt: owned.expiresAt)
        }
        if relevant.contains(where: { $0.token == nil }) { return Subscription(id: group, ownership: .unverified, environment: environment) }
        return Subscription(id: group, ownership: relevant.isEmpty ? .available : .otherAccount, environment: environment)
    }

    private func current(_ model: SessionModel, epoch: UInt64, account: String?) -> Bool {
        !Task.isCancelled && model.epoch == epoch && model.unlocked && model.session?.account == account
    }

    /// Reads current verified transactions, including billing retry and renewal.
    /// Historical transactions must not veto a matching current transaction.
    private static func inspect(_ products: [Product], token: UUID) async throws -> [Subscription] {
        let groups = Set(products.compactMap { $0.subscription?.subscriptionGroupID })
        var subscriptions: [Subscription] = []
        for group in groups.sorted() {
            let values = try await Product.SubscriptionInfo.status(for: group)
            try Task.checkCancellation()
            let statuses = try values.map { value -> SubscriptionStatus in
                guard case .verified(let transaction) = value.transaction,
                      case .verified(let renewal) = value.renewalInfo,
                      Self.environmentName(transaction.environment) != nil else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
                // Renewal metadata describes a future transaction, not ownership
                // of the current one. Only show its plan for the same account.
                let sameRenewalAccount = renewal.appAccountToken == nil || renewal.appAccountToken == transaction.appAccountToken
                return SubscriptionStatus(token: transaction.appAccountToken, state: value.state,
                    nextProduct: sameRenewalAccount && renewal.willAutoRenew && renewal.autoRenewPreference != transaction.productID ? renewal.autoRenewPreference : nil,
                    renewalDate: sameRenewalAccount ? renewal.renewalDate : nil, environment: Self.environmentName(transaction.environment), expiresAt: transaction.expirationDate)
            }
            subscriptions.append(Self.subscription(group: group, statuses: statuses, expected: token))
        }
        return subscriptions
    }

    private static func verifiedEnvironment(_ value: VerificationResult<AppTransaction>) throws -> String {
        guard case .verified(let app) = value, let environment = environmentName(app.environment) else {
            throw MobileError.Failure(code: "INTEGRITY_FAILED")
        }
        return environment
    }

    func load(_ model: SessionModel) async {
        guard enabled else { return }
        guard model.unlocked else { return }; let epoch = model.epoch
        loadRevision &+= 1; let read = loadRevision
        environmentTask?.cancel(); environmentTask = nil
        loading = true
        defer { if read == loadRevision { loading = false } }
        if account != model.session?.account {
            products = []; token = nil; pendingTransactions = [:]; pendingRefreshes = []; awaitingApproval = []; feedback = nil; account = model.session?.account
        }
        if feedback == Self.otherAccountMessage || feedback == Self.unverifiedMessage { feedback = nil }
        subscriptions = []; storeEnvironment = nil; loadIssue = nil; hasUnavailableProducts = false
        let expectedAccount = account
        var stage = LoadIssue.catalogue
        do {
            let catalogue = try await store.catalogue(model)
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
            stage = .products
            let found = try await store.products(config.products.map(\.productId))
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            // Keep offers visible even when the subscription status read fails.
            products = found.sorted { $0.price < $1.price }
            let missing = Set(config.products.map(\.productId)).subtracting(found.map(\.id)).sorted()
            hasUnavailableProducts = !missing.isEmpty
            if !missing.isEmpty {
                // Product identifiers are public. Never log account tokens or receipts.
                Self.logger.warning("App Store products unavailable: \(missing.joined(separator: ","), privacy: .public)")
            }
            stage = .subscriptions
            let checked = try await store.subscriptions(products, token)
            guard current(model, epoch: epoch, account: expectedAccount), read == loadRevision else { return }
            subscriptions = checked
            let environments = Set(checked.compactMap(\.environment))
            storeEnvironment = environments.count == 1 ? environments.first : nil
            // This optional hint must not hold loading, restore, or transaction
            // replay open. Signed subscription transactions take precedence.
            if storeEnvironment == nil {
                let appEnvironment = store.appEnvironment
                environmentTask = Task { [weak self, weak model] in
                    do {
                        let environment = try await appEnvironment()
                        guard let self, let model, self.current(model, epoch: epoch, account: expectedAccount), read == self.loadRevision else { return }
                        self.storeEnvironment = environment
                    } catch {
                        guard !Task.isCancelled else { return }
                        let failure = error as NSError
                        Self.logger.info("App download environment unavailable: \(failure.domain, privacy: .public) (\(failure.code))")
                    }
                }
            }
        } catch {
            if current(model, epoch: epoch, account: expectedAccount), read == loadRevision {
                loadIssue = stage
                if stage == .catalogue { token = nil; products = [] }
                if stage == .products { products = [] }
                let failure = error as NSError
                Self.logger.warning("App Store load failed at \(stage.rawValue, privacy: .public): \(failure.domain, privacy: .public) (\(failure.code))")
                model.reportBackground(error)
            }
        }
    }

    func retry(_ model: SessionModel) async {
        guard enabled, !working, model.unlocked else { return }
        working = true; defer { working = false }
        let epoch = model.epoch, expectedAccount = model.session?.account
        do {
            // Only a user-requested retry of a failed subscription read may ask
            // StoreKit to authenticate. Never refresh the app download receipt.
            if loadIssue == .subscriptions || subscriptionMessage == Self.unverifiedMessage || feedback == Self.unverifiedMessage { try await store.sync() }
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            await refresh(model)
        } catch { if current(model, epoch: epoch, account: expectedAccount), !Self.cancelled(error) { model.report(error) } }
    }

    func purchase(_ product: Product, model: SessionModel) async {
        guard enabled, !working, model.unlocked, canPurchase(product), let token, let account, account == model.session?.account else { return }
        working = true; feedback = nil; defer { working = false }
        let epoch = model.epoch, expectedAccount = account
        // An unavailable app download must not prevent a free account's first
        // purchase. Use the stricter paid-production rules for unknown context;
        // the verified transaction determines the environment at delivery.
        let environment = storeEnvironment
        var stage = "preflight"
        do {
            let check = try await store.preflight(model, product.id, environment ?? "Production")
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
            // Register the installation before asking Apple to charge. A device
            // unable to deliver an attested purchase must fail before payment.
            stage = "attestation"
            try await store.prepare(model, account)
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            // Recheck immediately before Apple's sheet, not only when the page loaded.
            stage = "subscription_status"
            let checked: [Subscription]
            do { checked = try await store.subscriptions(products, token) }
            catch {
                if current(model, epoch: epoch, account: expectedAccount) {
                    subscriptions = []; loadIssue = .subscriptions; model.reportBackground(error)
                }
                return
            }
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            subscriptions = checked
            guard canPurchase(product) else {
                feedback = subscriptionMessage ?? String(localized: "Change your plan or cancel renewal in Manage subscriptions.")
                return
            }
            stage = "payment"
            let result = try await store.purchase(product, token)
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            switch result {
            case .success(let verification):
                // Even when delivery fails, refresh ownership so a paid customer
                // sees recovery/management instead of another purchase offer.
                do { show(try await deliver(Self.receipt(verification), model: model), model: model, epoch: epoch, account: expectedAccount) }
                catch {
                    if current(model, epoch: epoch, account: expectedAccount) {
                        if pending { model.reportBackground(error) } else { model.report(error) }
                    }
                }
            case .pending: awaitingApproval.insert(product.id)
            case .userCancelled: break
            @unknown default: break
            }
            if current(model, epoch: epoch, account: expectedAccount) { await load(model) }
        } catch {
            if current(model, epoch: epoch, account: expectedAccount), !Self.cancelled(error) {
                let failure = error as NSError
                let code = (error as? MobileError).map { if case .Failure(let code) = $0 { return code }; return "unknown" } ?? String(failure.code)
                Self.logger.warning("Purchase failed at \(stage, privacy: .public): \(failure.domain, privacy: .public) (\(code, privacy: .public))")
                if stage == "payment" { model.report(error) }
                else {
                    if code == "EXECUTION_UNKNOWN" {
                        // An uncertain preparation result cannot mean a charge.
                        feedback = Self.purchaseNotStartedMessage
                    } else {
                        model.report(error)
                        guard current(model, epoch: epoch, account: expectedAccount), let reason = model.error else { return }
                        feedback = reason + "\n\n" + Self.purchaseNotStartedMessage
                    }
                    // The selected price can be above the inline status. Keep
                    // explicit purchase failures visible without requiring a scroll.
                    model.error = feedback
                }
            }
        }
    }

    func restore(_ model: SessionModel) async {
        guard enabled, !working, model.unlocked else { return }
        working = true; feedback = nil; defer { working = false }
        let epoch = model.epoch, expectedAccount = model.session?.account
        do {
            try await store.sync()
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            await load(model)
            guard current(model, epoch: epoch, account: expectedAccount), token != nil else { return }
            let result = await reconcile(model)
            guard current(model, epoch: epoch, account: expectedAccount) else { return }
            if result.unverified { feedback = Self.unverifiedMessage }
            else if result.failed { feedback = nil }
            else if result.applied { feedback = String(localized: "Your purchase has been applied to this dMsg account.") }
            else if result.foreign { feedback = Self.otherAccountMessage }
            else { feedback = subscriptionMessage ?? (loadIssue == nil ? String(localized: "No purchases were found for this dMsg account.") : nil) }
        } catch { if current(model, epoch: epoch, account: expectedAccount), !Self.cancelled(error) { model.report(error) } }
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
        // Subscribe at launch, independently of login, catalogue reads or replay.
        // Events seen while locked stay unfinished and replay after unlock.
        for await value in store.updates() {
            if Task.isCancelled { return }
            let epoch = model.epoch, account = model.session?.account
            do { _ = try await deliver(value.get(), model: model) }
            catch {
                logDeliveryFailure(error)
                if current(model, epoch: epoch, account: account) { model.reportBackground(error) }
            }
        }
    }

    func refresh(_ model: SessionModel) async {
        guard enabled, model.unlocked else { return }
        let epoch = model.epoch, account = model.session?.account
        await load(model)
        guard current(model, epoch: epoch, account: account) else { return }
        _ = await reconcile(model)
    }

    func networkReturned(_ model: SessionModel) async {
        guard !working, pending || loadIssue != nil || products.isEmpty else { return }
        await refresh(model)
    }

    private struct Reconciliation {
        var applied = false, foreign = false, unverified = false, failed = false
    }

    private func reconcile(_ model: SessionModel) async -> Reconciliation {
        var result = Reconciliation(), seen = Set<String>()
        let epoch = model.epoch, account = model.session?.account
        guard enabled, model.unlocked, token != nil, self.account == account else { return result }
        // Unfinished purchases take precedence over routine entitlement replay.
        // Failure to refresh an already delivered subscription is not a new payment.
        for (stream, refreshing) in [(store.unfinished, false), (store.currentEntitlements, true)] {
            let previous = refreshing ? pendingRefreshes : Set(pendingTransactions.keys)
            var present = Set<String>(), verified = true
            for await value in stream() {
                guard current(model, epoch: epoch, account: account) else { return result }
                let receipt: Receipt
                do { receipt = try value.get() }
                catch { verified = false; result.unverified = true; logDeliveryFailure(error); continue }
                present.insert(receipt.key)
                guard seen.insert(receipt.key + ":" + receipt.jws).inserted else { continue }
                do {
                    switch try await deliver(receipt, model: model, refreshing: refreshing) {
                    case .applied: result.applied = true
                    case .otherAccount: result.foreign = true
                    case .unverified: result.unverified = true
                    case .stale: return result
                    }
                } catch {
                    result.failed = true; logDeliveryFailure(error)
                    if current(model, epoch: epoch, account: account) { model.reportBackground(error) }
                }
            }
            guard current(model, epoch: epoch, account: account) else { return result }
            // Clear only old failures absent from a complete verified snapshot.
            // Expired finished entitlements must not leave a permanent retry banner.
            if verified {
                for key in previous.subtracting(present) {
                    if refreshing { pendingRefreshes.remove(key) }
                    else { pendingTransactions[key] = nil }
                }
            }
        }
        return result
    }

    private func logDeliveryFailure(_ error: Error) {
        let failure = error as NSError
        Self.logger.warning("Purchase delivery deferred: \(failure.domain, privacy: .public) (\(failure.code))")
    }

    private static func cancelled(_ error: Error) -> Bool {
        if error is CancellationError { return true }
        if case StoreKitError.userCancelled = error { return true }
        return false
    }

    static func deliveryIssue(token: UUID?, expected: UUID) -> Delivery? {
        guard let token else { return .unverified }
        return token == expected ? nil : .otherAccount
    }

    func deliver(_ receipt: Receipt, model: SessionModel, refreshing: Bool = false) async throws -> Delivery {
        guard enabled else { return .stale }
        guard receipt.environment == "Production" || receipt.environment == "Sandbox" else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        guard let token, let account, account == model.session?.account,
              model.unlocked, !Task.isCancelled else { return .stale }
        if let issue = Self.deliveryIssue(token: receipt.token, expected: token) { return issue }
        let epoch = model.epoch
        let key = "\(epoch):\(account):\(receipt.key)"
        func result(_ task: Task<Delivery, Error>) async throws -> Delivery {
            do { return try await task.value }
            catch {
                if current(model, epoch: epoch, account: account) {
                    if refreshing { pendingRefreshes.insert(receipt.key) }
                    else { pendingTransactions[receipt.key] = receipt.product }
                }
                throw error
            }
        }
        if let delivery = deliveries[key] {
            if delivery.jws == receipt.jws { return try await result(delivery.task) }
            // A refund or renewal observation of the same transaction must not
            // disappear behind an older in-flight JWS.
            _ = await delivery.task.result
            guard current(model, epoch: epoch, account: account) else { return .stale }
            return try await deliver(receipt, model: model, refreshing: refreshing)
        }
        let task = Task { @MainActor in
            defer { self.deliveries[key] = nil }
            guard self.current(model, epoch: epoch, account: account) else { return Delivery.stale }
            try await self.store.submit(model, account, receipt.jws)
            guard self.current(model, epoch: epoch, account: account) else { return Delivery.stale }
            // Only an authoritative applied response permits StoreKit finish.
            await receipt.finish()
            guard self.current(model, epoch: epoch, account: account) else { return Delivery.stale }
            self.pendingTransactions[receipt.key] = nil; self.awaitingApproval.remove(receipt.product)
            self.pendingRefreshes.remove(receipt.key)
            self.revision &+= 1
            return Delivery.applied
        }
        deliveries[key] = InFlight(jws: receipt.jws, task: task)
        return try await result(task)
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
        } else if args.contains("--ui-subscription-status-unavailable") {
            loadIssue = .subscriptions
        } else if args.contains("--ui-stale-subscription") || args.contains("--ui-expired-subscription") {
            storeEnvironment = "Sandbox"
            let expired = args.contains("--ui-expired-subscription")
            subscriptions = [Subscription(id: "fixture", ownership: expired ? .available : .current, environment: "Sandbox",
                state: expired ? .expired : .subscribed, expiresAt: Date.now.addingTimeInterval(expired ? -86400 : 86400))]
            pendingRefreshes = expired ? [] : ["Sandbox:fixture"]
        }
        if args.contains("--ui-purchase-not-started") {
            feedback = Self.purchaseNotStartedMessage
            if args.contains("--ui-purchase-attestation-unavailable") {
                feedback = String(localized: "App verification is unavailable on this device. Your saved content remains available.") + "\n\n" + Self.purchaseNotStartedMessage
            }
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
