import SwiftUI

/// Benefits come from the certified commerce catalog and current wallet quotas.
/// Deliberately omit cash price_cents: only StoreKit prices belong in this UI.
struct MembershipCatalogue: Decodable {
    struct Plan: Decodable, Identifiable {
        struct Limits: Decodable {
            let storageBytes: UInt64
            let activeChannels: UInt64
            let monthlyExecutionUnits: UInt64
            let activeDevices: UInt64
            let agentIds: UInt64?
            let backupMaxFileBytes: UInt64
            let fileRequestLinks: UInt64
            let teamWallets: UInt64?
        }
        let planId: String
        let limits: Limits
        var id: String { planId }
        var tier: MembershipTier? { MembershipTier(rawValue: planId) }
    }
    let plans: [Plan]
    var ordered: [Plan] { MembershipTier.allCases.compactMap { tier in plans.first { $0.planId == tier.rawValue } } }

    #if DEBUG
    static let fixture = MembershipCatalogue(plans: zip(MembershipTier.allCases, [
        Plan.Limits(storageBytes: 104857600, activeChannels: 5, monthlyExecutionUnits: 5, activeDevices: 2, agentIds: 1, backupMaxFileBytes: 104857600, fileRequestLinks: 0, teamWallets: 0),
        Plan.Limits(storageBytes: 1073741824, activeChannels: 20, monthlyExecutionUnits: 20, activeDevices: 3, agentIds: 3, backupMaxFileBytes: 104857600, fileRequestLinks: 10, teamWallets: 1),
        Plan.Limits(storageBytes: 10737418240, activeChannels: 100, monthlyExecutionUnits: 100, activeDevices: 5, agentIds: 5, backupMaxFileBytes: 104857600, fileRequestLinks: 50, teamWallets: 3),
        Plan.Limits(storageBytes: 107374182400, activeChannels: 10000, monthlyExecutionUnits: 10000, activeDevices: 10, agentIds: 10, backupMaxFileBytes: 1048576000, fileRequestLinks: 200, teamWallets: 10)
    ]).map { Plan(planId: $0.0.rawValue, limits: $0.1) })
    #endif
}

enum MembershipTier: String, CaseIterable {
    case free = "Free", plus = "Plus", pro = "Pro", max = "Max"
    var rank: Int { Self.allCases.firstIndex(of: self)! }
    var audience: String {
        switch self {
        case .free: String(localized: "For your everyday essentials.")
        case .plus: String(localized: "For everyday file exchange.")
        case .pro: String(localized: "For more files and devices.")
        case .max: String(localized: "For large backups and frequent collaboration.")
        }
    }
    func actionTitle(current: String?, managed: Bool) -> String {
        if managed {
            if let current = Self(rawValue: current ?? ""), rank > current.rank {
                return String.localizedStringWithFormat(String(localized: "Upgrade to %@"), rawValue)
            }
            return String(localized: "Change plan")
        }
        return String.localizedStringWithFormat(String(localized: "Subscribe to %@"), rawValue)
    }
}

enum MembershipBenefit: String, CaseIterable, Identifiable {
    case storage, devices, agentIds, backup, requests, channels, executions, wallets
    var id: String { rawValue }
    var title: String {
        switch self {
        case .storage: String(localized: "Cloud storage")
        case .devices: String(localized: "Active devices")
        case .agentIds: String(localized: "Hosted Agent IDs")
        case .backup: String(localized: "Cloud backup file limit")
        case .requests: String(localized: "Active file request links")
        case .channels: String(localized: "Conversation limit")
        case .executions: String(localized: "Monthly execution limit")
        case .wallets: String(localized: "Multisig wallet slots")
        }
    }
    func value(_ plan: MembershipCatalogue.Plan) -> String {
        switch self {
        case .storage: fileLimit(plan.limits.storageBytes)
        case .devices: plan.limits.activeDevices.formatted()
        case .agentIds: plan.limits.agentIds?.formatted() ?? String(localized: "Unavailable")
        case .backup: fileLimit(plan.limits.backupMaxFileBytes)
        case .requests: plan.limits.fileRequestLinks == 0 ? String(localized: "Not included") : plan.limits.fileRequestLinks.formatted()
        case .channels: plan.limits.activeChannels.formatted()
        case .executions: plan.limits.monthlyExecutionUnits.formatted()
        case .wallets: plan.limits.teamWallets?.formatted() ?? String(localized: "Unavailable")
        }
    }
}

struct MembershipPlanCard<Action: View>: View {
    let plan: MembershipCatalogue.Plan
    let current: Bool
    let price: String?
    @ViewBuilder let action: () -> Action

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .firstTextBaseline) {
                Text(verbatim: plan.planId).font(.title2.weight(.semibold))
                    .accessibilityIdentifier("plan.name.\(plan.planId)")
                Spacer(minLength: 8)
                if current {
                    Text("Current plan").font(.caption.weight(.semibold)).foregroundStyle(Theme.forest)
                        .padding(.horizontal, 10).padding(.vertical, 5)
                        .background(Theme.forest.opacity(0.1), in: Capsule())
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            if let tier = plan.tier { Text(tier.audience).font(.subheadline).foregroundStyle(.secondary) }
            VStack(spacing: 8) {
                benefit(.storage)
                benefit(.devices)
                benefit(plan.tier == .max ? .backup : .requests)
            }.font(.callout)
            if let price { Text(verbatim: price).font(.title3.weight(.semibold)).foregroundStyle(Theme.forest).fixedSize(horizontal: false, vertical: true) }
            action()
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: 18))
            .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(current ? Theme.forest.opacity(0.5) : Color.secondary.opacity(0.18), lineWidth: 1))
    }

    private func benefit(_ item: MembershipBenefit) -> some View {
        LabeledContent { Text(verbatim: item.value(plan)).foregroundStyle(.primary).multilineTextAlignment(.trailing) }
        label: { Text(item.title).foregroundStyle(.secondary) }
    }
}

struct MembershipComparisonView: View {
    let catalogue: MembershipCatalogue
    let currentPlan: String?
    let prices: () -> [String: String]
    let retry: () async -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.dynamicTypeSize) private var textSize

    var body: some View {
        List {
            Section {
                if let currentPlan { LabeledContent("Current plan", value: currentPlan) }
                if !model.purchases.showsControls {
                    Text("Plans cannot be changed in this version of the app. Your current plan and usage remain available.").font(.footnote)
                }
                Text("Every plan includes end-to-end encryption and device revocation.").font(.subheadline).foregroundStyle(.secondary)
            }
            if model.purchases.showsControls {
                Section("App Store subscriptions") {
                    let available = prices()
                    ForEach(catalogue.ordered.filter { $0.tier == .free || available[$0.id] != nil }) { plan in
                        let value = available[plan.id] ?? String(localized: "No charge")
                        if textSize.isAccessibilitySize {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(verbatim: plan.planId).font(.headline)
                                Text(verbatim: value).fixedSize(horizontal: false, vertical: true)
                            }.accessibilityElement(children: .combine)
                        } else {
                            LabeledContent(plan.planId, value: value)
                        }
                    }
                    if catalogue.ordered.contains(where: { $0.tier != .free && available[$0.id] == nil }) {
                        if model.purchases.loading { ProgressView("Loading…") }
                        else {
                            Text("Price unavailable").foregroundStyle(.secondary)
                            SettingsAction("Retry") { Task { await retry() } }.disabled(model.purchases.working)
                                .accessibilityIdentifier("plan.compare.retry")
                        }
                    }
                }
            }
            ForEach(MembershipBenefit.allCases) { benefit in
                Section {
                    comparisonValues(benefit)
                } header: { Text(benefit.title) }
            }
            Section("About these benefits") {
                Text("Limits are for the base plan. Storage packs are additional. Monthly execution units are prorated and do not reset when changing plans.")
                Text("Plus and above can ask anyone for files by link. People need no account, see only what they sent, and only you can open the files.")
                Text("The backup file limit applies to cloud backups. File sharing and channel attachments have their own limits.")
                Text("Channel limits apply to channels you create or sponsor. Ordinary device signatures do not use the monthly allowance.")
                Text("Wallet activation is charged separately; check availability in the app.")
                Text("Subscriptions renew automatically unless cancelled in your Apple account settings. Deleting your dMsg account does not cancel an Apple subscription.")
            }.font(.footnote).foregroundStyle(.secondary)
        }.navigationTitle("Compare all plans").navigationBarTitleDisplayMode(.inline).privateScreen()
            .accessibilityIdentifier("plan.comparison")
    }

    @ViewBuilder private func comparisonValues(_ benefit: MembershipBenefit) -> some View {
        if textSize.isAccessibilitySize {
            ForEach(catalogue.ordered) { plan in
                LabeledContent(plan.planId, value: benefit.value(plan))
                    .accessibilityIdentifier("plan.compare.\(benefit.id).\(plan.id)")
                    .foregroundStyle(plan.id == currentPlan ? Theme.forest : .primary)
            }
        } else {
            HStack(alignment: .top, spacing: 6) {
                ForEach(catalogue.ordered) { plan in
                    VStack(spacing: 8) {
                        Text(verbatim: plan.planId).font(.caption.weight(.medium))
                        Text(verbatim: benefit.value(plan)).font(.subheadline.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                    }.multilineTextAlignment(.center).frame(maxWidth: .infinity, alignment: .top)
                        .foregroundStyle(plan.id == currentPlan ? Theme.forest : .primary)
                        .accessibilityElement(children: .combine)
                }
            }.padding(.vertical, 6)
        }
    }
}
