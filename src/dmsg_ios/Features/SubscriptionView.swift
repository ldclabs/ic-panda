import SwiftUI
import StoreKit

struct SubscriptionView: View {
    @Environment(SessionModel.self) private var model
    struct Plan: Decodable {
        struct Limits: Decodable { let storageBytes: UInt64; let activeChannels: UInt64; let monthlyExecutionUnits: UInt64 }
        struct Term: Decodable { let endsAtMs: UInt64; let method: String }
        let plan: String; let source: String; let limits: Limits; let current: Term?
    }
    @State private var plan: Plan?
    @State private var managing = false
    @State private var loading = false
    @State private var failed = false
    var body: some View {
        List {
            Section("Current plan and limits") {
                if let plan {
                    LabeledContent("Plan", value: plan.plan)
                    LabeledContent("Cloud storage", value: ByteCountFormatter.string(fromByteCount: Int64(clamping: plan.limits.storageBytes), countStyle: .file))
                    LabeledContent("Active conversations", value: plan.limits.activeChannels.formatted())
                    LabeledContent("Monthly execution units", value: plan.limits.monthlyExecutionUnits.formatted())
                    if let term = plan.current { LabeledContent("Current term ends") { Text(Date(timeIntervalSince1970: Double(term.endsAtMs) / 1000), style: .date) } }
                    if plan.source == "Unverifiable" { NoticeBar(text: String(localized: "Your current entitlement could not be refreshed. Retry when the verification service is available.")) }
                } else if loading { ProgressView() }
                if failed {
                    Text("Your current entitlement could not be refreshed. Retry when the verification service is available.").foregroundStyle(.secondary)
                    Button("Retry") { Task { await load() } }
                }
            }
            if model.purchases.enabled {
                Section("App Store subscriptions") {
                    productRows(model.purchases.products.filter { $0.type == .autoRenewable })
                    if let term = plan?.current, term.method != "AppStore" {
                        NoticeBar(text: String(localized: "Your current plan is managed outside the App Store. Wait until it ends before starting an App Store subscription."))
                    } else if plan?.current == nil {
                        Text("Storage purchases require an active App Store subscription.").font(.footnote).foregroundStyle(.secondary)
                    }
                    if model.purchases.products.isEmpty { Text("Purchases are not currently available.").foregroundStyle(.secondary) }
                    if model.purchases.pending { NoticeBar(text: String(localized: "Your purchase is awaiting confirmation. Restore purchases to retry without paying again.")) }
                    Button("Restore purchases") { Task { await model.purchases.restore(model) } }.disabled(model.purchases.working)
                        .accessibilityIdentifier("plan.restore")
                    Button("Manage subscriptions") { managing = true }.accessibilityIdentifier("plan.manage")
                    Text("Subscriptions renew automatically unless cancelled in your Apple account settings. Deleting your dMsg account does not cancel an Apple subscription.").font(.footnote).foregroundStyle(.secondary)
                }
                if model.purchases.products.contains(where: { $0.type == .consumable }) {
                    Section("Storage packs") {
                        Text("Storage is a one-time purchase for the current subscription term and does not renew automatically.").font(.footnote).foregroundStyle(.secondary)
                        productRows(model.purchases.products.filter { $0.type == .consumable })
                    }
                }
            } else {
                Section {
                    Text("In-app purchases are disabled in this build. Your current plan and usage remain available.")
                        .font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("plan.purchases-disabled")
                }
            }
            Section {
                NavigationLink("Privacy policy") { PrivacyPolicyView() }
                Link("Terms of service", destination: URL(string: "https://dmsg.net/_/terms")!)
            }
        }.privateScreen().navigationTitle("Plan and usage").manageSubscriptionsSheet(isPresented: $managing)
            .task { await load() }.refreshable { await load() }
    }
    private func productRows(_ products: [Product]) -> some View {
                ForEach(products) { product in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(product.displayName).font(.headline)
                        Text(product.description).font(.footnote).foregroundStyle(.secondary)
                        if let subscription = product.subscription { Text(Self.period(subscription.subscriptionPeriod)).font(.subheadline) }
                        Button(product.displayPrice) { Task { await model.purchases.purchase(product, model: model) } }.disabled(model.purchases.working || !canPurchase(product))
                    }
                }
    }
    private func canPurchase(_ product: Product) -> Bool {
        guard let plan, plan.source != "Unverifiable", plan.current == nil || plan.current?.method == "AppStore" else { return false }
        return product.type == .autoRenewable || (product.type == .consumable && plan.current?.method == "AppStore" && plan.source == "Active")
    }
    private static func period(_ period: Product.SubscriptionPeriod) -> String {
        let formatter = DateComponentsFormatter(); formatter.unitsStyle = .full
        var components = DateComponents()
        switch period.unit {
        case .day: formatter.allowedUnits = [.day]; components.day = period.value
        case .week: formatter.allowedUnits = [.weekOfMonth]; components.weekOfMonth = period.value
        case .month: formatter.allowedUnits = [.month]; components.month = period.value
        case .year: formatter.allowedUnits = [.year]; components.year = period.value
        @unknown default: return ""
        }
        return formatter.string(from: components) ?? ""
    }
    private func load() async {
        guard !loading else { return }; loading = true; failed = false; defer { loading = false }
        #if DEBUG
        if model.uiTesting {
            plan = Plan(plan: "Free", source: "Free", limits: Plan.Limits(storageBytes: 104857600,
                activeChannels: 2, monthlyExecutionUnits: 3), current: nil)
            return
        }
        #endif
        let epoch = model.epoch
        await model.purchases.load(model)
        do { if let result = try await model.engine?.accountPlan() { if epoch == model.epoch, !Task.isCancelled { plan = try model.decode(result) } } }
        catch { if epoch == model.epoch { failed = true; model.report(error) } }
    }
}
