import Observation
import StoreKit
@preconcurrency import DMsgCore

@MainActor @Observable
final class PurchaseModel {
    // Explicitly disabled in the current build, including transaction replay.
    // Missing configuration fails closed; server-side verification is unchanged.
    let enabled = (Bundle.main.object(forInfoDictionaryKey: "DMsgInAppPurchasesEnabled") as? String) == "YES"
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
    private var account: String?
    private var token: UUID?

    func load(_ model: SessionModel, reportErrors: Bool = true) async {
        guard enabled else { return }
        guard let engine = model.engine, model.unlocked else { return }; let epoch = model.epoch
        #if DEBUG
        guard model.environment == "staging" else { products = []; token = nil; return }
        #endif
        if account != model.session?.account { products = []; token = nil; pending = false; account = model.session?.account }
        do {
            let catalogue: Catalogue = try model.decode(try await engine.storeProducts())
            guard epoch == model.epoch, !Task.isCancelled else { return }
            guard let config = catalogue.configuration else { products = []; return }
            guard config.bundleId == "net.dmsg.ios", config.appAppleId == 6827050680,
                  let token = UUID(uuidString: catalogue.accountToken) else { throw CocoaError(.coderInvalidValue) }
            let found = try await Product.products(for: config.products.map(\.productId))
            guard epoch == model.epoch, !Task.isCancelled else { return }
            self.token = token; products = found.sorted { $0.price < $1.price }
        } catch { if epoch == model.epoch, reportErrors { model.report(error) } }
    }

    func purchase(_ product: Product, model: SessionModel) async {
        guard enabled, !working, let token, account == model.session?.account else { return }
        working = true; defer { working = false }
        let epoch = model.epoch, expectedAccount = account
        do {
            struct Check: Decodable { let eligible: Bool; let reason: String?; let kind: String }
            guard let engine = model.engine else { return }
            let check: Check = try model.decode(try await engine.purchasePreflight(product: product.id))
            guard epoch == model.epoch, expectedAccount == model.session?.account else { return }
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
            switch try await product.purchase(options: [.appAccountToken(token)]) {
            case .success(let verification): try await deliver(verification, model: model)
            case .pending: pending = true
            case .userCancelled: break
            @unknown default: break
            }
        } catch { if epoch == model.epoch { pending = true; model.report(error) } }
    }

    func restore(_ model: SessionModel) async {
        guard enabled, !working else { return }; working = true; defer { working = false }
        do {
            try await AppStore.sync()
            await load(model)
            for await value in Transaction.currentEntitlements { try await deliver(value, model: model) }
            for await value in Transaction.unfinished { try await deliver(value, model: model) }
        } catch { pending = true; model.report(error) }
    }

    func listen(_ model: SessionModel) async {
        guard enabled else { return }
        await load(model, reportErrors: false)
        for await value in Transaction.unfinished {
            if Task.isCancelled { return }
            let epoch = model.epoch
            do { try await deliver(value, model: model) } catch { if epoch == model.epoch { pending = true } }
        }
        for await value in Transaction.updates {
            if Task.isCancelled { return }
            let epoch = model.epoch
            do { try await deliver(value, model: model) } catch { if epoch == model.epoch { pending = true } }
        }
    }

    private func deliver(_ value: VerificationResult<Transaction>, model: SessionModel) async throws {
        guard enabled else { return }
        guard case .verified(let transaction) = value else { throw MobileError.Failure(code: "INTEGRITY_FAILED") }
        guard let token, transaction.appAccountToken == token, let account, account == model.session?.account,
              model.unlocked, let engine = model.engine, let files = model.files else { return }
        let epoch = model.epoch
        try await model.apple.submitPurchase(engine: engine, files: files, account: account, jws: value.jwsRepresentation)
        guard epoch == model.epoch else { return }
        // Apple payment is finished only after the authoritative service applied it.
        await transaction.finish(); pending = false; await model.updateSession()
    }
}
