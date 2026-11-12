import SwiftUI
@preconcurrency import DMsgCore

struct ReminderDestination: Identifiable, Hashable {
    let account: String
    let target: ReminderTarget?
    var item: ReminderItem? = nil
    var ended = false
    var retryRoute: String? = nil
    var id: String { account + String(describing: target) }
}

struct ReminderDestinationView: View {
    let destination: ReminderDestination
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var ready = false
    @State private var unavailable: ReminderUnavailable?
    @State private var failed = false
    @State private var choosingAccount = false
    @State private var accessing = false
    @State private var channel: ChannelItem?
    @State private var letter: InboxLetter?
    @State private var approval: ApprovalView?
    @State private var item: ReminderItem?
    @State private var loadedEpoch: UInt64?
    @State private var accessed = false
    private var matchingAccount: Bool { destination.account == model.session?.account }
    private var scope: String { "\(model.epoch):\(model.unlocked):\(model.session?.account ?? "")" }

    var body: some View {
        Group {
            if destination.ended {
                state(title: String(localized: "Reminder unavailable"), detail: String(localized: "This reminder can no longer open its original item."), symbol: "bell.slash") { }
            } else if let route = destination.retryRoute {
                state(title: String(localized: "Unable to open reminder"), detail: String(localized: "Connection unavailable. Your saved content stays on this device."), symbol: "wifi.slash") {
                    SettingsAction("Retry", prominent: true, working: accessing) {
                        Task { accessing = true; await model.openPushRoute(route); accessing = false }
                    }.tint(Theme.forestFill)
                }
            } else if !matchingAccount {
                state(title: String(localized: "This reminder belongs to another account"), detail: String(localized: "Choose its account to continue. Your current account will stay selected until you choose another."), symbol: "person.crop.circle") {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(model.accountIdentities[destination.account]?.name ?? destination.account).font(.headline)
                        if model.accountIdentities[destination.account]?.name != nil {
                            Text(destination.account).font(.caption.monospaced()).foregroundStyle(.secondary)
                        }
                    }.accessibilityIdentifier("reminders.originalAccount")
                    Button("Switch account") { choosingAccount = true }.buttonStyle(.borderedProminent).tint(Theme.forestFill)
                        .controlSize(.large).accessibilityIdentifier("reminders.choose-account")
                }
            } else if !model.unlocked {
                state(title: String(localized: "Unlock to view reminder"), detail: String(localized: "Your reminder is protected. Unlock to continue where you left off."), symbol: "lock") {
                    if model.canUnlockWorkspace {
                        SettingsAction(Biometry.unlockTitle, prominent: true, working: model.busy || accessing) {
                            Task { await unlock() }
                        }.tint(Theme.forestFill).accessibilityIdentifier("reminders.unlock")
                    } else {
                        Button("Unlock workspace") { accessed = true }.buttonStyle(.borderedProminent).tint(Theme.forestFill).controlSize(.large)
                    }
                }
            } else if let unavailable, loadedEpoch == model.epoch {
                state(title: unavailable.title, detail: unavailable.detail, symbol: "clock") { }
            } else if failed && !ready {
                state(title: String(localized: "Unable to open reminder"), detail: String(localized: "Connection unavailable. Your saved content stays on this device."), symbol: "wifi.slash") {
                    Button("Retry") { Task { await load() } }.buttonStyle(.borderedProminent).tint(Theme.forestFill)
                        .controlSize(.large).accessibilityIdentifier("reminders.retry")
                }
            } else if ready && loadedEpoch == model.epoch {
                content.environment(\.openedReminder, item)
            } else {
                state(title: String(localized: "Opening reminder…"), detail: "", symbol: "bell") { ProgressView() }
            }
        }
        .privateScreen()
        .task(id: scope) { await load() }
        .sheet(isPresented: $accessed) {
            NavigationStack {
                AccessView().toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { model.signingIntoAnotherAccount = false; accessed = false } } }
            }
        }
        .onChange(of: model.unlocked) { _, unlocked in if unlocked { accessed = false } }
        .sheet(isPresented: $choosingAccount, onDismiss: { if model.signingIntoAnotherAccount { accessed = true } }) {
            NavigationStack {
                AccountsView(onSelected: { choosingAccount = false })
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { choosingAccount = false } } }
            }
        }
        .accessibilityIdentifier("reminders.destination")
    }

    private func state<Actions: View>(title: String, detail: String, symbol: String, @ViewBuilder actions: () -> Actions) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                ObjectIcon(name: symbol)
                Text(title).font(.title2.bold()).accessibilityAddTraits(.isHeader)
                if !detail.isEmpty { Text(detail).font(.body).foregroundStyle(.secondary) }
                actions()
                Button("Back to reminders") { dismiss() }.frame(minHeight: 44)
                    .accessibilityIdentifier("reminders.return")
            }.frame(maxWidth: 480, alignment: .leading).padding(24).frame(maxWidth: .infinity, alignment: .center)
        }.navigationTitle("Details").navigationBarTitleDisplayMode(.inline)
    }

    @ViewBuilder private var content: some View {
        switch destination.target {
        case .channel:
            if let channel {
                ConversationView(channel: channel).safeAreaInset(edge: .top) {
                    if failed { NoticeBar(text: String(localized: "Showing saved content. The latest sync did not finish.")) }
                }
            }
        case .inbox:
            if let letter { InboxDetail(letter: letter, changed: {}) }
            else { MessagesView().task { model.route.messagesMode = 1 } }
        case .approval: if let approval { ApprovalDetailView(request: approval) }
        case .recovery: RecoveryView()
        case .wallet(let wallet, let proposal):
            if let id = UInt64(proposal) { WalletProposalView(wallet: wallet, proposal: id) }
        case .plan: SubscriptionView()
        case .task(let id):
            if id.hasPrefix("conflicts:") { ConflictsView() }
            else { SyncSettingsView().reminderReadOnDisplay() }
        case .service(let id):
            if id.hasPrefix("network:") { SyncSettingsView().reminderReadOnDisplay() }
            else { NotificationSettingsView(focusRegistration: true) }
        case .summary: Color.clear.task { dismiss() }
        case .agentMail, nil:
            state(title: String(localized: "Reminder unavailable"), detail: String(localized: "This reminder can no longer open its original item."), symbol: "bell.slash") { }
        }
    }

    private func unlock() async {
        accessing = true
        defer { accessing = false }
        #if DEBUG
        if model.uiTesting { model.unlockReminderFixture(); return }
        #endif
        await model.fastUnlock()
    }

    private func load() async {
        ready = false; failed = false; unavailable = nil
        channel = nil; letter = nil; approval = nil; item = nil; loadedEpoch = nil
        guard !destination.ended, destination.retryRoute == nil, matchingAccount, model.unlocked else { return }
        let epoch = model.epoch
        loadedEpoch = epoch
        item = destination.item
        #if DEBUG
        if model.uiTesting {
            if ProcessInfo.processInfo.arguments.contains("--ui-reminder-failed") { failed = true; return }
            if ProcessInfo.processInfo.arguments.contains("--ui-reminder-expired") { unavailable = .expired; return }
            item = item ?? model.notifications.fixtureItems.first { $0.account == destination.account && $0.target == destination.target }
            ready = true
            return
        }
        #endif
        guard let engine = model.engine else { failed = true; return }
        // Reading a reference must not prevent access to the business content.
        if item == nil, let target = destination.target {
            let reference = try? await engine.notificationReference(selected: destination.account, targetJson: NoticeEncoding.json(target))
            guard epoch == model.epoch, !Task.isCancelled else { return }
            if let reference { item = try? model.decode(reference) as ReminderItem? }
        }
        guard epoch == model.epoch, !Task.isCancelled else { return }
        do {
            switch destination.target {
            case .channel(let id):
                if let cached = try? await engine.channels(), let saved = cached.items.first(where: { $0.id == id }), epoch == model.epoch, !Task.isCancelled {
                    channel = saved; ready = true
                }
                _ = try await coreOperation { try await engine.refreshChannel(channel: id, operation: $0) }
                let value = try await engine.channels().items.first { $0.id == id }
                guard epoch == model.epoch, !Task.isCancelled else { return }
                channel = value; unavailable = value == nil ? .missing : nil
            case .inbox(let id):
                if let value = try? await engine.inbox(refresh: false), let cached = try? model.decode(value) as [InboxLetter],
                   let saved = cached.first(where: { $0.orderId == id }), epoch == model.epoch, !Task.isCancelled { letter = saved; ready = true }
                let rows: [InboxLetter] = try model.decode(try await engine.inbox(refresh: true))
                guard epoch == model.epoch, !Task.isCancelled else { return }
                letter = rows.first { $0.orderId == id }; unavailable = id != nil && letter == nil ? .missing : nil
            case .approval(let id):
                let value: ApprovalView = try model.decode(try await engine.approval(id: id))
                guard epoch == model.epoch, !Task.isCancelled else { return }
                approval = value
            case .wallet(let wallet, let proposal):
                if let id = UInt64(proposal) { _ = try await engine.walletProposal(wallet: wallet, proposal: id) }
                else { unavailable = .missing }
            case .recovery(let digest):
                await model.refreshRecoveryWarning()
                guard epoch == model.epoch, !Task.isCancelled else { return }
                if model.recoveryWarning?.pending == true, model.recoveryWarning?.digest != digest { unavailable = .missing }
            default: break
            }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            ready = true
        } catch {
            guard epoch == model.epoch, !Task.isCancelled else { return }
            if let core = error as? MobileError, case .Failure(let code) = core, let reason = ReminderUnavailable(code: code) {
                unavailable = reason
            } else { failed = true }
        }
    }
}

enum ReminderUnavailable {
    case expired, forbidden, missing

    init?(code: String) {
        switch code { case "EXPIRED": self = .expired; case "FORBIDDEN": self = .forbidden; case "NOT_FOUND": self = .missing; default: return nil }
    }

    var title: String {
        switch self {
        case .expired: String(localized: "This request has expired")
        case .forbidden: String(localized: "Access unavailable")
        case .missing: String(localized: "Reminder unavailable")
        }
    }

    var detail: String {
        switch self {
        case .expired: String(localized: "This request no longer needs a response.")
        case .forbidden: String(localized: "This account no longer has access to the item.")
        case .missing: String(localized: "This reminder can no longer open its original item.")
        }
    }
}
