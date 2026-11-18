import SwiftUI
import StoreKit

struct SubscriptionView: View {
    @Environment(SessionModel.self) private var model
    struct Plan: Decodable {
        struct Limits: Decodable { let storageBytes: UInt64; let activeChannels: UInt64; let monthlyExecutionUnits: UInt64 }
        struct Term: Decodable { let endsAtMs: UInt64; let method: String; let storeEnvironment: String? }
        let plan: String; let source: String; let limits: Limits; let current: Term?
        var eligibility: String? = nil
        var leaseUntilMs: UInt64? = nil

        func needsRefresh(at: Date = .now) -> Bool {
            source == "Unverifiable" || eligibility == "Unverifiable"
                || (["Active", "Free", "Expired"].contains(source) && leaseUntilMs.map { Double($0) <= at.timeIntervalSince1970 * 1000 } == true)
        }

        func managesSubscription(environment: String?, at: Date = .now) -> Bool {
            guard !needsRefresh(at: at), source == "Active", let current, current.method == "AppStore",
                  Double(current.endsAtMs) > at.timeIntervalSince1970 * 1000 else { return false }
            return environment == nil || environment == (current.storeEnvironment ?? "Production")
        }

        #if DEBUG
        static var fixture: Self {
            let args = ProcessInfo.processInfo.arguments
            let max = args.contains("--ui-max-subscription"), plus = args.contains("--ui-plus-subscription")
            if args.contains("--ui-stale-subscription") {
                return Plan(plan: "Free", source: "Unverifiable", limits: Limits(storageBytes: 104857600, activeChannels: 5, monthlyExecutionUnits: 5),
                    current: Term(endsAtMs: UInt64(Date.now.addingTimeInterval(86400).timeIntervalSince1970 * 1000), method: "AppStore", storeEnvironment: "Sandbox"))
            }
            if args.contains("--ui-expired-subscription") {
                return Plan(plan: "Free", source: "Expired", limits: Limits(storageBytes: 104857600, activeChannels: 5, monthlyExecutionUnits: 5), current: nil)
            }
            if max || plus || args.contains("--ui-sandbox-subscription") || args.contains("--ui-scheduled-subscription") {
                return Plan(plan: max ? "Max" : plus ? "Plus" : "Pro", source: "Active",
                    limits: Limits(storageBytes: max ? 107374182400 : plus ? 1073741824 : 10737418240, activeChannels: max ? 10000 : plus ? 20 : 100, monthlyExecutionUnits: max ? 10000 : plus ? 20 : 100),
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
        var membershipStale: Bool { cloudError == "MEMBERSHIP_STALE" || executionError == "MEMBERSHIP_STALE" }
        var otherFailure: Bool { [cloudError, executionError].compactMap { $0 }.contains { $0 != "MEMBERSHIP_STALE" } }
    }
    @State private var usage: Usage?
    @State private var usageFailed = false
    @State private var usageLoading = false
    @State private var usageRequest = UUID()
    @State private var display = DisplayLoad()
    @State private var plan: Plan?
    @State private var managing = false
    @State private var loading = false
    @State private var failed = false
    @State private var loadRequest = UUID()
    @State private var catalogue: MembershipCatalogue?
    @State private var catalogueFailed = false
    @State private var catalogueLoading = false
    @State private var catalogueRequest = UUID()

    var body: some View {
        ScrollViewReader { proxy in
            planList.onChange(of: model.purchases.purchaseActivity?.product) { _, product in
                // A tapped offer can be at the bottom of the viewport. Reveal its new status and Apple's confirmation hint together.
                if let product { proxy.scrollTo(product, anchor: .center) }
            }
        }
    }

    private var planList: some View {
        List {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Section("Current plan and limits") {
                if let plan {
                    LabeledContent("Plan", value: plan.needsRefresh() ? String(localized: "Status needs updating") : plan.plan)
                        .accessibilityIdentifier("plan.effective")
                    if !plan.needsRefresh() {
                        LabeledContent("Storage limit", value: fileLimit(plan.limits.storageBytes))
                        LabeledContent("Conversation limit", value: plan.limits.activeChannels.formatted())
                        LabeledContent("Monthly execution limit", value: plan.limits.monthlyExecutionUnits.formatted())
                        if plan.source == "Expired" { Text("Your paid membership has ended. You are now on Free.").foregroundStyle(.secondary).accessibilityIdentifier("plan.expired") }
                    }
                    if let term = plan.current {
                        LabeledContent(plan.needsRefresh() ? "Last recorded term" : "Current term ends") { Text(Date(timeIntervalSince1970: Double(term.endsAtMs) / 1000), style: .date) }
                    }
                    if plan.current?.storeEnvironment == "Sandbox" {
                        Text("Test subscription. No charge. The expiry follows Apple's test environment.").font(.footnote).foregroundStyle(.secondary)
                            .accessibilityIdentifier("plan.sandbox")
                    }
                } else if loading { ProgressView() }
                if failed {
                    Text("Your current entitlement could not be refreshed. Retry when the verification service is available.").foregroundStyle(.secondary)
                    SettingsAction("Retry") { Task { await load() } }
                }
                if membershipNeedsRefresh {
                    NoticeBar(text: String(localized: "Your membership status needs updating. Refresh the membership page and try again."))
                        .accessibilityIdentifier("plan.status-pending")
                    SettingsAction("Retry") { Task { await load(force: true) } }.disabled(loading || model.purchases.working)
                        .accessibilityIdentifier("plan.status-retry")
                }
                if changesInApple { managementAction }
            }
            Section("Usage") {
                if usageLoading { ProgressView() }
                if let cloud = usage?.cloud {
                    LabeledContent("Used", value: fileLimit(cloud.committed))
                    LabeledContent("Reserved for uploads", value: fileLimit(cloud.reserved))
                    if let remaining = cloud.remaining { LabeledContent("Remaining", value: fileLimit(remaining)) }
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
                if usageFailed || usage?.otherFailure == true {
                    NoticeBar(text: String(localized: "Some usage could not be refreshed. Unavailable values are not zero."))
                        .accessibilityIdentifier("plan.usage-error")
                    SettingsAction("Retry") { Task { await loadUsage() } }.disabled(usageLoading)
                }
            }.accessibilityIdentifier("plan.usage")
            Section {
                if let catalogue {
                    NavigationLink {
                        MembershipComparisonView(catalogue: catalogue, currentPlan: currentTier, prices: { planPrices }, retry: { await loadProducts(reconcile: false) })
                    } label: { Label("Compare all plans", systemImage: "list.bullet.rectangle") }
                    .accessibilityIdentifier("plan.compare")
                    Text("Every plan includes end-to-end encryption and device revocation.")
                        .font(.footnote).foregroundStyle(.secondary)
                } else if catalogueFailed {
                    Text("Plan benefits could not be loaded. Try again.").font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("plan.catalogue-error")
                    SettingsAction("Retry") { Task { await loadCatalogue() } }.disabled(catalogueLoading)
                        .accessibilityIdentifier("plan.catalogue-retry")
                } else { ProgressView().accessibilityLabel("Loading…") }
            }
            if model.purchases.showsControls {
                Section("App Store subscriptions") {
                    Text("Choose the storage and device capacity that fits your everyday work. Upgrade to collect files by upload link and make room for more.")
                        .font(.footnote).foregroundStyle(.secondary)
                    if model.purchases.storeEnvironment == "Sandbox" {
                        Text("Test purchases do not charge you. An active paid plan takes priority.").font(.footnote).foregroundStyle(.secondary)
                    }
                    ForEach(model.purchases.subscriptions) { subscription in
                        if let state = subscription.state {
                            LabeledContent("Status", value: Self.subscriptionState(state)).accessibilityIdentifier("plan.apple-status")
                            if let end = subscription.expiresAt {
                                LabeledContent("Last recorded term") { Text(end, style: .date) }
                            }
                        }
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
                    } else if plan?.current == nil, model.purchases.storeEnvironment == "Production", model.purchases.products.contains(where: { $0.type == .consumable }) {
                        Text("Storage purchases require an active App Store subscription.").font(.footnote).foregroundStyle(.secondary)
                    }
                    subscriptionCards
                    if !model.purchases.loading, let issue = model.purchases.loadIssue {
                        NoticeBar(text: issue.message).accessibilityElement(children: .combine).accessibilityIdentifier("plan.store-error")
                            .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
                    }
                    if !model.purchases.loading && model.purchases.loadIssue == nil && !managesSubscription && !model.purchases.hasCurrentSubscription && model.purchases.subscriptionMessage == nil && model.purchases.products.isEmpty && !showsProductFixtures {
                        Text("Purchases are not currently available.").foregroundStyle(.secondary)
                            .accessibilityIdentifier("plan.products-unavailable")
                    }
                    if model.purchases.purchaseActivity == nil && (model.purchases.working || model.purchases.loading) {
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
                    if !model.purchases.loading && (model.purchases.loadIssue != nil || model.purchases.hasUnavailableProducts || (model.purchases.products.isEmpty && !showsProductFixtures) || model.purchases.subscriptionMessage == PurchaseModel.unverifiedMessage || model.purchases.feedback == PurchaseModel.unverifiedMessage) {
                        SettingsAction("Retry") { Task { await model.purchases.retry(model) } }.disabled(model.purchases.working)
                            .accessibilityIdentifier("plan.products-retry")
                    }
                    if model.purchases.approvalPending {
                        NoticeBar(text: String(localized: "Your purchase is awaiting Apple approval.")).accessibilityIdentifier("plan.purchase-approval")
                    } else if model.purchases.deliveryPending {
                        NoticeBar(text: String(localized: "Your purchase is awaiting confirmation. Restore purchases to retry without paying again.")).accessibilityIdentifier("plan.purchase-pending")
                    }
                    SettingsAction("Restore purchases") { Task { await model.purchases.restore(model) } }.disabled(model.purchases.working)
                        .accessibilityIdentifier("plan.restore")
                    if !changesInApple { managementAction }
                    Text("Subscriptions renew automatically unless cancelled in your Apple account settings. Deleting your dMsg account does not cancel an Apple subscription.").font(.footnote).foregroundStyle(.secondary)
                }
                if model.purchases.storeEnvironment == "Production" && model.purchases.subscriptionMessage == nil && model.purchases.products.contains(where: { $0.type == .consumable }) {
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
        }.reminderReadOnDisplay(ready: plan != nil).privateScreen().navigationTitle("Plan and usage").manageSubscriptionsSheet(isPresented: $managing)
            // Resume after the privacy cover lifts and Apple's explicit flow
            // finishes. A cancelled old refresh cannot consume the new request.
            // DMsgApp already replays receipts on these same transitions.
            .task(id: "\(model.epoch):\(model.concealed):\(model.purchases.working)") {
                if !model.concealed, !model.purchases.working { await load(force: true, reconcile: false) }
            }.refreshable { await load(force: true) }
            .onChange(of: model.purchases.revision) { _, _ in Task { await load(force: true, reconcile: false) } }
            .onChange(of: managing) { wasPresented, isPresented in
                if wasPresented && !isPresented { Task { await load(force: true) } }
            }
            .onDisappear { loadRequest = UUID(); usageRequest = UUID(); catalogueRequest = UUID() }
    }
    private var managesSubscription: Bool {
        plan?.managesSubscription(environment: model.purchases.storeEnvironment) == true
    }

    private var membershipNeedsRefresh: Bool {
        plan?.needsRefresh() == true || usage?.membershipStale == true || model.purchases.refreshPending
    }

    static func subscriptionState(_ state: Product.SubscriptionInfo.RenewalState) -> String {
        switch state {
        case .subscribed: String(localized: "Active")
        case .expired: String(localized: "Expired")
        case .revoked: String(localized: "Revoked")
        case .inBillingRetryPeriod: String(localized: "Billing retry")
        case .inGracePeriod: String(localized: "Billing grace period")
        default: String(localized: "Status needs updating")
        }
    }

    private var showsProductFixtures: Bool {
        #if DEBUG
        return model.uiTesting && catalogue != nil && (ProcessInfo.processInfo.arguments.contains("--ui-plan-products") || model.purchases.showsProgressFixture || changesInApple)
        #else
        return false
        #endif
    }

    private var currentTier: String? { plan.flatMap { $0.needsRefresh() ? nil : $0.plan } }
    private var changesInApple: Bool { model.purchases.showsControls && (managesSubscription || model.purchases.hasCurrentSubscription) }

    private var managementAction: some View {
        SettingsAction("Manage subscriptions") { managing = true }
            .disabled(model.purchases.working).accessibilityIdentifier("plan.manage")
    }

    private func product(for plan: MembershipCatalogue.Plan) -> Product? {
        model.purchases.products.first { $0.type == .autoRenewable && model.purchases.productPlans[$0.id] == plan.id }
    }

    private func price(for plan: MembershipCatalogue.Plan) -> String? {
        if let product = product(for: plan) {
            return Self.price(product.displayPrice, period: product.subscription.map { Self.period($0.subscriptionPeriod) })
        }
        #if DEBUG
        if showsProductFixtures {
            let prices = ["Plus": "HK$88.00", "Pro": "HK$388.00", "Max": "HK$1,488.00"]
            if let amount = prices[plan.id] {
                return Self.price(amount, period: DateComponentsFormatter.localizedString(from: DateComponents(year: 1), unitsStyle: .full))
            }
        }
        #endif
        return nil
    }

    static func price(_ amount: String, period: String?) -> String {
        guard let period, !period.isEmpty else { return amount }
        return String.localizedStringWithFormat(String(localized: "%@ / %@"), amount, period)
    }

    private var planPrices: [String: String] {
        Dictionary((catalogue?.ordered ?? []).compactMap { plan in price(for: plan).map { (plan.id, $0) } }, uniquingKeysWith: { first, _ in first })
    }

    @ViewBuilder private var subscriptionCards: some View {
        if let catalogue {
            ForEach(catalogue.ordered.filter { $0.tier != .free }) { offer in
                MembershipPlanCard(plan: offer, current: offer.id == currentTier, price: price(for: offer)) {
                    planAction(offer)
                }.listRowBackground(Color.clear).listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(top: 8, leading: 0, bottom: 8, trailing: 0))
                    .id(product(for: offer)?.id ?? offer.id)
            }
            // An unmapped product never borrows another tier's benefits.
            productRows(model.purchases.products.filter { $0.type == .autoRenewable && model.purchases.productPlans[$0.id] == nil })
        } else {
            productRows(model.purchases.products.filter { $0.type == .autoRenewable })
        }
    }

    @ViewBuilder private func planAction(_ offer: MembershipCatalogue.Plan) -> some View {
        if let tier = offer.tier {
            let title = offer.id == currentTier && changesInApple ? String(localized: "Manage subscriptions")
                : tier.actionTitle(current: currentTier, managed: changesInApple)
            if model.purchases.subscriptionMessage != nil {
                EmptyView()
            } else if changesInApple {
                Button(title) { managing = true }
                    .font(.headline).frame(maxWidth: .infinity, minHeight: 44)
                    .buttonStyle(.bordered).tint(Theme.forest).controlSize(.large)
                    .disabled(model.purchases.working)
                    .accessibilityIdentifier("plan.change.\(offer.id)")
            } else if let product = product(for: offer) {
                PurchaseButton(price: price(for: offer) ?? product.displayPrice, title: title,
                    phase: model.purchases.purchasePhase(for: product.id, model: model),
                    disabled: model.purchases.working || !canPurchase(product), identifier: "plan.purchase.\(product.id)") {
                        Task { await model.purchases.purchase(product, model: model) }
                    }
            } else {
                #if DEBUG
                if showsProductFixtures {
                    PurchaseButton(price: price(for: offer) ?? "", title: title,
                        phase: model.purchases.purchasePhase(for: offer.id, model: model),
                        disabled: model.purchases.working || currentTier == nil || model.purchases.loadIssue != nil || model.purchases.subscriptionMessage != nil,
                        identifier: "plan.purchase.fixture-\(offer.id.lowercased())") {
                            model.purchases.startProgressFixture(model, product: offer.id)
                        }
                } else { Text("Price unavailable").font(.footnote).foregroundStyle(.secondary) }
                #else
                Text(model.purchases.loading ? String(localized: "Loading…") : String(localized: "Price unavailable")).font(.footnote).foregroundStyle(.secondary)
                #endif
            }
        }
    }

    @ViewBuilder private func productRows(_ products: [Product]) -> some View {
        ForEach(products) { product in
            VStack(alignment: .leading, spacing: 8) {
                Text(product.displayName).font(.headline)
                Text(product.description).font(.footnote).foregroundStyle(.secondary)
                if let subscription = product.subscription { Text(Self.period(subscription.subscriptionPeriod)).font(.subheadline) }
                PurchaseButton(price: product.displayPrice, phase: model.purchases.purchasePhase(for: product.id, model: model),
                    disabled: model.purchases.working || !canPurchase(product), identifier: "plan.purchase.\(product.id)") {
                        Task { await model.purchases.purchase(product, model: model) }
                    }
            }.id(product.id)
        }
    }
    private func canPurchase(_ product: Product) -> Bool {
        guard let plan, !plan.needsRefresh(), model.purchases.canPurchase(product) else { return false }
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
    private func load(force: Bool = false, reconcile: Bool = true) async {
        let request = UUID(); loadRequest = request
        loading = true; failed = false
        defer { if loadRequest == request { loading = false } }
        #if DEBUG
        if model.uiTesting {
            model.purchases.installSubscriptionFixture()
            if ProcessInfo.processInfo.arguments.contains("--ui-purchase-not-started") {
                model.error = model.purchases.feedback
            }
            plan = Plan.fixture
            catalogueFailed = ProcessInfo.processInfo.arguments.contains("--ui-catalogue-unavailable")
            catalogue = catalogueFailed ? nil : .fixture
            usage = Usage(cloud: Usage.Cloud(committed: 10485760, reserved: 1048576, storageLimit: plan?.limits.storageBytes, activeChannels: 1, channelLimit: plan?.limits.activeChannels), execution: Usage.Execution(month: 202611, allowed: plan?.limits.monthlyExecutionUnits ?? 3, charged: 1, remaining: (plan?.limits.monthlyExecutionUnits ?? 3) - 1), cloudError: nil, executionError: nil)
            if ProcessInfo.processInfo.arguments.contains("--ui-stale-subscription") {
                usage = Usage(cloud: usage?.cloud, execution: nil, cloudError: nil, executionError: "MEMBERSHIP_STALE")
            }
            return
        }
        #endif
        async let benefits: Void = loadCatalogue()
        if reconcile {
            // Explicit retries deliver receipts before rereading their rights and usage.
            await loadProducts(reconcile: true)
            guard loadRequest == request, !Task.isCancelled else { return }
            await display.load(model, kind: "plan", force: force) { (next: Plan) in if loadRequest == request { plan = next } }
            guard loadRequest == request, !Task.isCancelled else { return }
            await loadUsage()
        } else {
            // Opening the page keeps independent reads independent.
            async let counts: Void = loadUsage()
            async let saved: Void = display.load(model, kind: "plan", force: force) { (next: Plan) in if loadRequest == request { plan = next } }
            async let products: Void = loadProducts(reconcile: false)
            _ = await (saved, products, counts)
        }
        if loadRequest == request, !Task.isCancelled { failed = display.failed }
        await benefits
    }

    private func loadCatalogue() async {
        #if DEBUG
        if model.uiTesting { catalogue = .fixture; catalogueFailed = false; return }
        #endif
        guard let engine = model.engine, model.unlocked else { return }
        let epoch = model.epoch, request = UUID(); catalogueRequest = request
        catalogueLoading = true; catalogueFailed = false
        defer { if catalogueRequest == request { catalogueLoading = false } }
        do {
            let next: MembershipCatalogue = try model.decode(try await engine.planCatalog())
            guard request == catalogueRequest, epoch == model.epoch, !Task.isCancelled else { return }
            catalogue = next
        } catch {
            guard request == catalogueRequest, epoch == model.epoch, !Task.isCancelled else { return }
            catalogue = nil; catalogueFailed = true
        }
    }

    private func loadProducts(reconcile: Bool) async {
        // A delivery can update the plan while another explicit Apple operation
        // is still open. Do not reset that operation's checked product state.
        guard !model.purchases.working else { return }
        if reconcile { await model.purchases.refresh(model) }
        else { await model.purchases.load(model) }
    }

    private func loadUsage() async {
        guard let engine = model.engine, model.unlocked else { return }
        let epoch = model.epoch, request = UUID(); usageRequest = request
        usageLoading = true; usageFailed = false
        defer { if usageRequest == request { usageLoading = false } }
        do {
            let value: Usage = try model.decode(try await engine.accountUsage())
            guard usageRequest == request, epoch == model.epoch, !Task.isCancelled else { return }; usage = value
        } catch { if usageRequest == request, epoch == model.epoch, !Task.isCancelled { usage = nil; usageFailed = true } }
    }

}
