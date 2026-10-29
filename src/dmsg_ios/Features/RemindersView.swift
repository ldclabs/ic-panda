import SwiftUI
@preconcurrency import DMsgCore

struct RemindersView: View {
    let entry: ReminderEntry
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var selected = ""
    @State private var recent = false
    @State private var items: [ReminderItem] = []
    @State private var next: UInt32?
    @State private var loading = false
    @State private var failed = false
    @State private var destination: ReminderDestination?
    @State private var snoozing: ReminderItem?
    @State private var started = false
    @State private var openedEntry = false
    @State private var request: UUID?
    @State private var itemScope = ""
    @State private var settingUp = false
    private var accounts: [ReminderStatus.Account] { model.notifications.status?.accounts ?? [] }
    private var scope: String { "\(model.epoch):\(selected):\(recent):\(started)" }
    private var category: ReminderCategory? {
        if selected == entry.account, case .summary(let category) = entry.target { return category }
        return nil
    }
    var body: some View {
        List {
            NotificationSetupPrompt { settingUp = true }
            if entry.ended { NoticeBar(text: NoticeCopy.text("noticeEnded")) }
            if let route = entry.retryRoute {
                NoticeBar(text: String(localized: "Connection unavailable. Your saved content stays on this device."))
                Button("Retry") { Task { await model.openPushRoute(route) } }
            }
            if !accounts.isEmpty {
                Picker("Account", selection: $selected) {
                    ForEach(accounts) { account in Text(account.account).font(.caption.monospaced()).tag(account.account) }
                }.accessibilityIdentifier("reminders.account")
            }
            Picker(NoticeCopy.text("noticeTitle"), selection: $recent) {
                Text(NoticeCopy.text("noticeNeedsAction")).tag(false)
                Text(NoticeCopy.text("noticeRecent")).tag(true)
            }.pickerStyle(.segmented).accessibilityIdentifier("reminders.scope")
            if !selected.isEmpty && selected != model.session?.account {
                NoticeBar(text: NoticeCopy.text("noticeSwitch"))
                Button(NoticeCopy.text("noticeSwitch")) { Task { await switchAccount() } }.accessibilityIdentifier("reminders.switch")
            } else if !model.unlocked && !selected.isEmpty {
                Button("Unlock with system authentication") { Task { await model.fastUnlock() } }.disabled(model.busy)
            }
            if loading && items.isEmpty { ProgressView() }
            if failed { Button("Retry") { Task { await load() } } }
            ForEach(items.filter { category == nil || $0.target.category == category }) { item in
                VStack(alignment: .leading, spacing: 8) {
                    HStack(alignment: .top) {
                        Image(systemName: item.target.category == .security ? "shield" : item.actionable ? "checkmark.circle" : "bell").foregroundStyle(Theme.forest)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(item.target.category.title).font(.headline)
                            Text(Date(timeIntervalSince1970: Double(item.updatedAt) / 1000), style: .relative).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if let deadline = item.deadline { Text(Date(timeIntervalSince1970: Double(deadline) / 1000), style: .relative).font(.caption).foregroundStyle(.secondary) }
                    }
                    HStack {
                        Button("Open") { destination = ReminderDestination(account: item.account, target: item.target) }
                        if item.open && item.actionable {
                            Button(NoticeCopy.text("noticeLater")) { snoozing = item }.disabled(loading)
                        }
                    }.buttonStyle(.bordered)
                }.accessibilityIdentifier("reminders.item")
            }
            if !loading && !failed && items.isEmpty { ContentUnavailableView(NoticeCopy.text("noticeEmpty"), systemImage: "clock") }
            if next != nil { Button("Load more") { Task { await load(more: true) } }.disabled(loading) }
            NoticeBar(text: NoticeCopy.text("noticeReadOnlyHint"))
        }
        .privateScreen().navigationTitle(NoticeCopy.text("noticeTitle"))
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    NavigationLink(NoticeCopy.text("noticeSettings")) { NotificationSettingsView() }.accessibilityIdentifier("reminders.settings")
                    Button(NoticeCopy.text("noticeClear")) { Task { await clearRecent() } }.disabled(selected.isEmpty)
                } label: { Label(NoticeCopy.text("noticeSettings"), systemImage: "ellipsis") }
                    .accessibilityIdentifier("reminders.menu")
            }
        }
        .task {
            await model.notifications.refresh(model)
            selected = (entry.ended || entry.retryRoute != nil) && entry.account == nil ? "" : entry.account ?? model.session?.account ?? accounts.first?.account ?? ""
            if case .summary(let category) = entry.target { recent = category == .message || category == .service }
            started = true
        }
        .task(id: scope) {
            if started {
                await load()
                if !openedEntry, !Task.isCancelled, let target = entry.target, let account = entry.account,
                   selected == account, account == model.session?.account, model.unlocked {
                    openedEntry = true
                    if case .summary = target { return }
                    destination = ReminderDestination(account: account, target: target)
                }
            }
        }
        .refreshable { await model.notifications.refresh(model, receive: true); await load() }
        .navigationDestination(item: $destination) { destination in ReminderDestinationView(destination: destination) }
        .navigationDestination(isPresented: $settingUp) { NotificationSettingsView() }
        .sheet(item: $snoozing) { item in NavigationStack { SnoozeReminderView(item: item) } }
        .accessibilityIdentifier("reminders.page")
    }
    private func load(more: Bool = false) async {
        guard !selected.isEmpty else { items = []; return }
        let current = scope, offset = more ? next ?? 0 : 0
        let token = UUID(); request = token
        let identity = "\(selected):\(recent)"
        if itemScope != identity { items = []; next = nil; itemScope = identity }
        loading = true; failed = false
        defer { if scope == current, request == token { loading = false } }
        if model.uiTesting { items = model.notifications.fixtureItems.filter { $0.account == selected }; next = nil; return }
        do {
            guard let engine = model.engine else { return }
            let page: ReminderPage = try model.decode(try await engine.notificationItems(selected: selected, scope: recent ? .recent : .needsAction, offset: offset))
            guard scope == current, request == token, !Task.isCancelled else { return }
            items = more ? items + page.items : page.items; next = page.next
        } catch { if scope == current && !Task.isCancelled { failed = true } }
    }
    private func switchAccount() async {
        guard let engine = model.engine, accounts.contains(where: { $0.account == selected }) else { return }
        do { _ = try await engine.switchAccount(account: selected); await model.updateSession() }
        catch { model.report(error) }
    }
    private func clearRecent() async {
        do { try await model.engine?.notificationClearRecent(selected: selected); await load() }
        catch { model.report(error) }
    }
}

struct ReminderDestination: Identifiable, Hashable { let account: String; let target: ReminderTarget; var id: String { account + String(describing: target) } }

private struct ReminderDestinationView: View {
    let destination: ReminderDestination
    @Environment(SessionModel.self) private var model
    @State private var ready = false
    @State private var ended = false
    @State private var failed = false
    @State private var channel: ChannelItem?
    @State private var letter: InboxLetter?
    @State private var approval: ApprovalView?
    var body: some View {
        Group {
            if destination.account != model.session?.account || !model.unlocked { NoticeBar(text: NoticeCopy.text("noticeSwitch")) }
            else if ended { ContentUnavailableView(NoticeCopy.text("noticeEnded"), systemImage: "clock") }
            else if failed { VStack { NoticeBar(text: String(localized: "Connection unavailable. Your saved content stays on this device.")); Button("Retry") { Task { await load() } } }.padding() }
            else if !ready { ProgressView() }
            else {
                switch destination.target {
                case .channel: if let channel { ConversationView(channel: channel) }
                case .inbox: if let letter { InboxDetail(letter: letter, changed: {}) }
                case .approval: if let approval { ApprovalDetailView(request: approval) }
                case .recovery: AccountSecurityView()
                case .wallet(let wallet, let proposal): if let id = UInt64(proposal) { WalletProposalView(wallet: wallet, proposal: id) }
                case .plan: SubscriptionView()
                case .task(let id): if id.hasPrefix("conflicts:") { ConflictsView() } else { AccountView() }
                case .service: NotificationSettingsView()
                case .summary, .agentMail: ContentUnavailableView(NoticeCopy.text("noticeEnded"), systemImage: "clock")
                }
            }
        }.privateScreen().task(id: model.epoch) { await load() }
    }
    private func load() async {
        ready = false; ended = false; failed = false
        guard model.session?.account == destination.account, model.unlocked, let engine = model.engine else { ended = model.uiTesting; return }
        let epoch = model.epoch
        do {
            // A push can arrive before local catch-up, and a resolved reminder
            // can still point to readable messages or an approval's result.
            // The original business read determines availability, not the cache.
            var nextChannel: ChannelItem?, nextLetter: InboxLetter?, nextApproval: ApprovalView?
            var nextEnded = false
            switch destination.target {
            case .channel(let id):
                _ = try await coreOperation { try await engine.refreshChannel(channel: id, operation: $0) }
                nextChannel = try await engine.channels().items.first { $0.id == id }; nextEnded = nextChannel == nil
            case .inbox(let id):
                let rows: [InboxLetter] = try model.decode(try await engine.inbox(refresh: true)); nextLetter = rows.first { $0.orderId == id }; nextEnded = nextLetter == nil
            case .approval(let id): nextApproval = try model.decode(try await engine.approval(id: id))
            case .wallet(let wallet, let proposal): if let id = UInt64(proposal) { _ = try await engine.walletProposal(wallet: wallet, proposal: id) } else { nextEnded = true }
            case .recovery(let digest): await model.refreshRecoveryWarning(); nextEnded = model.recoveryWarning?.digest != digest
            default: break
            }
            guard epoch == model.epoch, !Task.isCancelled else { return }
            channel = nextChannel; letter = nextLetter; approval = nextApproval; ended = nextEnded; ready = true
        } catch {
            if epoch == model.epoch, !Task.isCancelled {
                if let core = error as? MobileError, case .Failure(let code) = core, ["NOT_FOUND", "FORBIDDEN", "EXPIRED"].contains(code) { ended = true }
                else { failed = true }
            }
        }
    }
}

private struct SnoozeReminderView: View {
    let item: ReminderItem
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var working = false
    private var choices: [(String, UInt64)] {
        let now = NotificationTime.now
        return [("noticePause15", now + 900_000), ("noticePauseHour", now + 3_600_000), ("noticePauseTomorrow", NotificationTime.tomorrow())]
    }
    var body: some View {
        List {
            ForEach(choices, id: \.0) { choice($0.0, until: $0.1) }
            if let deadline = item.deadline, choices.contains(where: { $0.1 >= deadline }) { NoticeBar(text: NoticeCopy.text("noticeLate")) }
        }.navigationTitle(NoticeCopy.text("noticeLater"))
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
    }
    private func choice(_ key: String, until: UInt64) -> some View {
        Button(NoticeCopy.text(key)) { Task {
            working = true; defer { working = false }
            do {
                try await model.engine?.notificationSnooze(selected: item.account, id: item.id, expectedVersion: item.version, until: until)
                await model.notifications.refresh(model, receive: true); dismiss()
            } catch { model.report(error) }
        } }.disabled(working || item.deadline.map { until >= $0 } == true)
    }
}
