import SwiftUI
import StoreKit

struct SubscriptionView: View {
    @Environment(SessionModel.self) private var model
    struct Plan: Decodable {
        struct Limits: Decodable { let storageBytes: UInt64; let activeChannels: UInt64; let monthlyExecutionUnits: UInt64 }
        struct Term: Decodable { let endsAtMs: UInt64; let method: String; let storeEnvironment: String? }
        let plan: String; let source: String; let limits: Limits; let current: Term?
    }
    struct Usage: Decodable {
        struct Cloud: Decodable {
            let committed: UInt64; let reserved: UInt64; let storageLimit: UInt64?
            let activeChannels: UInt64?; let channelLimit: UInt64?
            var remaining: UInt64? { storageLimit.map { $0 > committed && $0 - committed > reserved ? $0 - committed - reserved : 0 } }
        }
        struct Execution: Decodable { let month: UInt32; let allowed: UInt64; let charged: UInt64; let remaining: UInt64 }
        let cloud: Cloud?; let execution: Execution?; let cloudError: String?; let executionError: String?
    }
    @State private var usage: Usage?
    @State private var usageFailed = false
    @State private var usageLoading = false
    @State private var display = DisplayLoad()
    @State private var plan: Plan?
    @State private var managing = false
    @State private var loading = false
    @State private var failed = false
    var body: some View {
        List {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Section("Current plan and limits") {
                if let plan {
                    LabeledContent("Plan", value: plan.plan)
                    LabeledContent("Storage limit", value: fileLimit(plan.limits.storageBytes))
                    LabeledContent("Conversation limit", value: plan.limits.activeChannels.formatted())
                    LabeledContent("Monthly execution limit", value: plan.limits.monthlyExecutionUnits.formatted())
                    if let term = plan.current { LabeledContent("Current term ends") { Text(Date(timeIntervalSince1970: Double(term.endsAtMs) / 1000), style: .date) } }
                    if plan.current?.storeEnvironment == "Sandbox" {
                        Text("Test subscription. No charge. The expiry follows Apple's test environment.").font(.footnote).foregroundStyle(.secondary)
                            .accessibilityIdentifier("plan.sandbox")
                    }
                    if plan.source == "Unverifiable" { NoticeBar(text: String(localized: "Your current entitlement could not be refreshed. Retry when the verification service is available.")) }
                } else if loading { ProgressView() }
                if failed {
                    Text("Your current entitlement could not be refreshed. Retry when the verification service is available.").foregroundStyle(.secondary)
                    SettingsAction("Retry") { Task { await load() } }
                }
            }
            Section("Usage") {
                if usageLoading { ProgressView() }
                if let cloud = usage?.cloud {
                    LabeledContent("Used", value: fileSize(cloud.committed))
                    LabeledContent("Reserved for uploads", value: fileSize(cloud.reserved))
                    if let remaining = cloud.remaining { LabeledContent("Remaining", value: fileSize(remaining)) }
                    if let limit = cloud.storageLimit, limit > 0 { ProgressView(value: min(Double(cloud.committed) + Double(cloud.reserved), Double(limit)), total: Double(limit)).accessibilityLabel("Cloud storage") }
                    if let used = cloud.activeChannels, let limit = cloud.channelLimit {
                        LabeledContent("Active conversations", value: "\(used.formatted()) / \(limit.formatted())")
                    }
                }
                if let execution = usage?.execution {
                    LabeledContent("Monthly execution units", value: "\(execution.charged.formatted()) / \(execution.allowed.formatted())")
                    LabeledContent("Remaining", value: execution.remaining.formatted())
                    Text(verbatim: "UTC · \(execution.month / 100)-\(String(format: "%02d", execution.month % 100))").font(.caption).foregroundStyle(.secondary)
                }
                if usageFailed || usage?.cloudError != nil || usage?.executionError != nil {
                    NoticeBar(text: String(localized: "Some usage could not be refreshed. Unavailable values are not zero."))
                    SettingsAction("Retry") { Task { await loadUsage() } }.disabled(usageLoading)
                }
            }.accessibilityIdentifier("plan.usage")
            Section("Cloud backup file limit") {
                ForEach(["Free", "Plus", "Pro", "Max"], id: \.self) { tier in
                    LabeledContent {
                        Text(verbatim: tier == "Max" ? "1000 MiB" : "100 MiB")
                    } label: { Text(verbatim: tier) }
                        .accessibilityIdentifier("plan.backup-limit.\(tier)")
                }
            }
            if model.purchases.enabled {
                Section("App Store subscriptions") {
                    if model.purchases.storeEnvironment == "Sandbox" {
                        Text("Test purchases do not charge you. An active paid plan takes priority.").font(.footnote).foregroundStyle(.secondary)
                    }
                    productRows(model.purchases.products.filter { $0.type == .autoRenewable })
                    if let term = plan?.current, term.method != "AppStore", model.purchases.storeEnvironment != "Sandbox" {
                        NoticeBar(text: String(localized: "Your current plan is managed outside the App Store. Wait until it ends before starting an App Store subscription."))
                    } else if plan?.current == nil {
                        Text("Storage purchases require an active App Store subscription.").font(.footnote).foregroundStyle(.secondary)
                    }
                    if model.purchases.products.isEmpty { Text("Purchases are not currently available.").foregroundStyle(.secondary) }
                    if model.purchases.pending { NoticeBar(text: String(localized: "Your purchase is awaiting confirmation. Restore purchases to retry without paying again.")) }
                    SettingsAction("Restore purchases") { Task { await model.purchases.restore(model) } }.disabled(model.purchases.working)
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
                    Text("Plans cannot be changed in this version of the app. Your current plan and usage remain available.")
                        .font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("plan.purchases-disabled")
                }
            }
            Section {
                NavigationLink("Privacy policy") { PrivacyPolicyView() }
                Link("Terms of service", destination: URL(string: "https://dmsg.net/_/terms")!)
            }
        }.privateScreen().navigationTitle("Plan and usage").manageSubscriptionsSheet(isPresented: $managing)
            .task { await load() }.refreshable { await load(force: true) }
            .onChange(of: model.purchases.revision) { _, _ in Task { await load(force: true) } }
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
        guard let plan, plan.source != "Unverifiable" else { return false }
        if model.purchases.storeEnvironment == "Sandbox" { return product.type == .autoRenewable }
        guard plan.current == nil || plan.current?.method == "AppStore" else { return false }
        return product.type == .autoRenewable || (product.type == .consumable && plan.current?.method == "AppStore" && plan.source == "Active" && plan.current?.storeEnvironment != "Sandbox")
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
    private func load(force: Bool = false) async {
        guard !loading else { return }; loading = true; failed = false; defer { loading = false }
        #if DEBUG
        if model.uiTesting {
            plan = Plan(plan: "Free", source: "Free", limits: Plan.Limits(storageBytes: 104857600,
                activeChannels: 2, monthlyExecutionUnits: 3), current: nil)
            if ProcessInfo.processInfo.arguments.contains("--ui-sandbox-subscription") {
                plan = Plan(plan: "Max", source: "Active", limits: Plan.Limits(storageBytes: 107374182400, activeChannels: 200, monthlyExecutionUnits: 200), current: Plan.Term(endsAtMs: UInt64(Date().addingTimeInterval(86400).timeIntervalSince1970 * 1000), method: "AppStore", storeEnvironment: "Sandbox"))
            }
            usage = Usage(cloud: Usage.Cloud(committed: 10485760, reserved: 1048576, storageLimit: plan?.limits.storageBytes, activeChannels: 1, channelLimit: plan?.limits.activeChannels), execution: Usage.Execution(month: 202611, allowed: plan?.limits.monthlyExecutionUnits ?? 3, charged: 1, remaining: (plan?.limits.monthlyExecutionUnits ?? 3) - 1), cloudError: nil, executionError: nil)
            return
        }
        #endif
        async let counts: Void = loadUsage()
        async let saved: Void = display.load(model, kind: "plan", force: force) { (next: Plan) in plan = next }
        async let products: Void = model.purchases.load(model)
        _ = await (saved, products, counts)
        failed = display.failed
    }

    private func loadUsage() async {
        guard let engine = model.engine, !usageLoading else { return }
        let epoch = model.epoch; usageLoading = true; usageFailed = false
        defer { usageLoading = false }
        do {
            let value: Usage = try model.decode(try await engine.accountUsage())
            guard epoch == model.epoch, !Task.isCancelled else { return }; usage = value
        } catch { if epoch == model.epoch, !Task.isCancelled { usageFailed = true } }
    }

}
