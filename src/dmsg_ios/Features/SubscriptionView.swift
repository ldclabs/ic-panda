import SwiftUI
import StoreKit

struct SubscriptionView: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.scenePhase) private var scenePhase
    struct Plan: Decodable {
        struct Limits: Decodable { let storageBytes: UInt64; let activeChannels: UInt64; let monthlyExecutionUnits: UInt64 }
        struct Term: Decodable { let endsAtMs: UInt64; let method: String; let storeEnvironment: String? }
        let plan: String; let source: String; let limits: Limits; let current: Term?

        func managesSubscription(environment: String?, at: Date = .now) -> Bool {
            guard source == "Active", let current, current.method == "AppStore",
                  Double(current.endsAtMs) > at.timeIntervalSince1970 * 1000 else { return false }
            return environment == nil || environment == (current.storeEnvironment ?? "Production")
        }

        #if DEBUG
        static var fixture: Self {
            let args = ProcessInfo.processInfo.arguments
            let max = args.contains("--ui-max-subscription")
            if max || args.contains("--ui-sandbox-subscription") || args.contains("--ui-scheduled-subscription") {
                return Plan(plan: max ? "Max" : "Pro", source: "Active",
                    limits: Limits(storageBytes: max ? 107374182400 : 10737418240, activeChannels: max ? 200 : 100, monthlyExecutionUnits: max ? 200 : 100),
                    current: Term(endsAtMs: UInt64(Date().addingTimeInterval(86400).timeIntervalSince1970 * 1000), method: "AppStore", storeEnvironment: "Sandbox"))
            }
            return Plan(plan: "Free", source: "Free", limits: Limits(storageBytes: 104857600, activeChannels: 5, monthlyExecutionUnits: 5), current: nil)
        }
        #endif
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
    @State private var queuedRefresh = false
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
            if model.purchases.showsControls {
                Section("App Store subscriptions") {
                    if model.purchases.storeEnvironment == "Sandbox" {
                        Text("Test purchases do not charge you. An active paid plan takes priority.").font(.footnote).foregroundStyle(.secondary)
                    }
                    if let message = model.purchases.subscriptionMessage {
                        NoticeBar(text: message).accessibilityElement(children: .combine).accessibilityIdentifier("plan.subscription-account")
                            .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
                        if model.purchases.subscriptions.contains(where: { $0.ownership == .otherAccount }) {
                            NavigationLink("Switch account") { AccountsView() }.accessibilityIdentifier("plan.switch-account")
                        }
                    }
                    if managesSubscription, let plan {
                        LabeledContent("Plan", value: plan.plan).accessibilityIdentifier("plan.current-subscription")
                        Text("Change your plan or cancel renewal in Manage subscriptions.").font(.footnote).foregroundStyle(.secondary)
                    } else if model.purchases.hasCurrentSubscription {
                        Text("Change your plan or cancel renewal in Manage subscriptions.").font(.footnote).foregroundStyle(.secondary)
                    } else if model.purchases.subscriptionMessage == nil {
                        productRows(model.purchases.products.filter { $0.type == .autoRenewable && model.purchases.canPurchase($0) })
                    }
                    ForEach(model.purchases.subscriptions) { subscription in
                        if let product = subscription.nextProduct, subscription.ownership == .current {
                            LabeledContent("Plan at next renewal", value: model.purchases.products.first(where: { $0.id == product })?.displayName ?? product)
                                .accessibilityIdentifier("plan.next-renewal")
                            if let date = subscription.renewalDate {
                                Text(date, style: .date).font(.footnote).foregroundStyle(.secondary)
                            }
                        }
                    }
                    if let term = plan?.current, term.method != "AppStore", model.purchases.storeEnvironment != "Sandbox" {
                        NoticeBar(text: String(localized: "Your current plan is managed outside the App Store. Wait until it ends before starting an App Store subscription."))
                    } else if plan?.current == nil, model.purchases.products.contains(where: { $0.type == .consumable }) {
                        Text("Storage purchases require an active App Store subscription.").font(.footnote).foregroundStyle(.secondary)
                    }
                    if !managesSubscription && !model.purchases.hasCurrentSubscription && model.purchases.subscriptionMessage == nil && model.purchases.products.isEmpty { Text("Purchases are not currently available.").foregroundStyle(.secondary) }
                    if model.purchases.working {
                        HStack(spacing: 8) {
                            ProgressView().accessibilityHidden(true)
                            Text("Loading…")
                        }.frame(maxWidth: .infinity, minHeight: 44).foregroundStyle(.secondary)
                            .accessibilityElement(children: .combine).accessibilityIdentifier("plan.purchase-progress")
                            .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
                    }
                    if let feedback = model.purchases.feedback, feedback != model.purchases.subscriptionMessage {
                        NoticeBar(text: feedback).accessibilityElement(children: .combine).accessibilityIdentifier("plan.purchase-result")
                    }
                    if model.purchases.subscriptionMessage == PurchaseModel.unverifiedMessage || model.purchases.feedback == PurchaseModel.unverifiedMessage {
                        SettingsAction("Retry") { Task { await model.purchases.load(model) } }.disabled(model.purchases.working)
                    }
                    if model.purchases.pending { NoticeBar(text: String(localized: "Your purchase is awaiting confirmation. Restore purchases to retry without paying again.")) }
                    SettingsAction("Restore purchases") { Task { await model.purchases.restore(model) } }.disabled(model.purchases.working)
                        .accessibilityIdentifier("plan.restore")
                    SettingsAction("Manage subscriptions") { managing = true }.disabled(model.purchases.working).accessibilityIdentifier("plan.manage")
                    Text("Subscriptions renew automatically unless cancelled in your Apple account settings. Deleting your dMsg account does not cancel an Apple subscription.").font(.footnote).foregroundStyle(.secondary)
                }
                if model.purchases.subscriptionMessage == nil && model.purchases.products.contains(where: { $0.type == .consumable }) {
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
            // Apple's purchase and restore sheets make the scene inactive. Those
            // flows reload when they finish; a concurrent failed reload would
            // clear the account token their delivery still needs.
            .task(id: scenePhase) { if scenePhase == .active, !model.purchases.working { await load(force: true) } }.refreshable { await load(force: true) }
            .onChange(of: model.purchases.revision) { _, _ in Task { await load(force: true) } }
            .onChange(of: managing) { wasPresented, isPresented in
                if wasPresented && !isPresented { Task { await load(force: true) } }
            }
    }
    private var managesSubscription: Bool {
        plan?.managesSubscription(environment: model.purchases.storeEnvironment) == true
    }

    private func productRows(_ products: [Product]) -> some View {
                ForEach(products) { product in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(product.displayName).font(.headline)
                        Text(product.description).font(.footnote).foregroundStyle(.secondary)
                        if let subscription = product.subscription { Text(Self.period(subscription.subscriptionPeriod)).font(.subheadline) }
                        Button(product.displayPrice) { Task { await model.purchases.purchase(product, model: model) } }.disabled(model.purchases.working || !canPurchase(product)).accessibilityIdentifier("plan.purchase.\(product.id)")
                    }
                }
    }
    private func canPurchase(_ product: Product) -> Bool {
        guard let plan, plan.source != "Unverifiable", model.purchases.canPurchase(product) else { return false }
        if product.type == .autoRenewable && managesSubscription { return false }
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
        if loading { queuedRefresh = queuedRefresh || force; return }
        loading = true; failed = false; defer { loading = false }
        #if DEBUG
        if model.uiTesting {
            model.purchases.installSubscriptionFixture()
            plan = Plan.fixture
            usage = Usage(cloud: Usage.Cloud(committed: 10485760, reserved: 1048576, storageLimit: plan?.limits.storageBytes, activeChannels: 1, channelLimit: plan?.limits.activeChannels), execution: Usage.Execution(month: 202611, allowed: plan?.limits.monthlyExecutionUnits ?? 3, charged: 1, remaining: (plan?.limits.monthlyExecutionUnits ?? 3) - 1), cloudError: nil, executionError: nil)
            return
        }
        #endif
        var refresh = force
        repeat {
            queuedRefresh = false
            let refreshThisLoad = refresh
            async let counts: Void = loadUsage()
            async let saved: Void = display.load(model, kind: "plan", force: refreshThisLoad) { (next: Plan) in plan = next }
            async let products: Void = model.purchases.load(model)
            _ = await (saved, products, counts)
            failed = display.failed
            refresh = queuedRefresh
        } while refresh && !Task.isCancelled
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
