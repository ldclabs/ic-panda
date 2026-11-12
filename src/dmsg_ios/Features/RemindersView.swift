import SwiftUI
@preconcurrency import DMsgCore

struct RemindersView: View {
    let entry: ReminderEntry
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var recent = false
    @State private var items: [ReminderItem] = []
    @State private var next: UInt32?
    @State private var loading = false
    @State private var failed = false
    @State private var destination: ReminderDestination?
    @State private var snoozing: ReminderItem?
    @State private var started = false
    @State private var request: UUID?
    @State private var itemScope = ""
    @State private var settingUp = false
    @State private var reading: String?
    @State private var confirmingRead = false
    @State private var channelPreviews: [String: ChannelItem] = [:]
    private var selected: String { model.session?.account ?? "" }
    private var scope: String { "\(model.unlocked):\(model.epoch):\(selected):\(recent):\(started):\(destination == nil)" }
    private var category: ReminderCategory? {
        if selected == entry.account, case .summary(let category) = entry.target { return category }
        return nil
    }
    private var visibleItems: [ReminderItem] { items.filter { category == nil || $0.target.category == category } }
    private var hasUnread: Bool {
        items.contains(where: \.isUnread) || (model.notifications.status?.counts.actionable ?? 0) > 0 || (model.notifications.status?.counts.unread ?? 0) > 0
    }

    var body: some View {
        List {
            Section {
                if (model.notifications.status?.accounts.count ?? 0) > 1 {
                    Label(model.accountIdentities[selected]?.name ?? selected, systemImage: "person.crop.circle")
                        .font(.subheadline).foregroundStyle(.secondary)
                        .accessibilityIdentifier("reminders.current-account")
                }
                Picker(NoticeCopy.text("noticeTitle"), selection: $recent) {
                    Text(NoticeCopy.text("noticeNeedsAction")).tag(false)
                    Text(NoticeCopy.text("noticeRecent")).tag(true)
                }.pickerStyle(.segmented).accessibilityIdentifier("reminders.scope")
            }.listRowBackground(Color.clear).listRowSeparator(.hidden)
            if !model.unlocked, model.canUnlockWorkspace {
                Section {
                    Button { Task { await model.fastUnlock() } } label: {
                        HStack(spacing: 12) {
                            Image(systemName: "lock").accessibilityHidden(true)
                            Text(Biometry.unlockTitle).frame(maxWidth: .infinity, alignment: .leading)
                            if model.busy { ProgressView() }
                        }.frame(minHeight: 44)
                    }.disabled(model.busy).accessibilityIdentifier("reminders.unlock")
                }
            }
            Section {
                if loading && items.isEmpty { ProgressView("Loading…") }
                if failed {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("Connection unavailable. Your saved content stays on this device.").font(.subheadline)
                        Button("Retry") { Task { await load() } }.frame(minHeight: 44)
                    }
                }
                ForEach(visibleItems) { item in
                    Button { destination = ReminderDestination(account: item.account, target: item.target, item: item) } label: {
                        ReminderRow(item: item, channel: preview(for: item))
                    }.buttonStyle(.plain).compactObjectRow().accessibilityIdentifier("reminders.item")
                        .accessibilityValue(item.isUnread ? String(localized: "Unread reminder") : NoticeCopy.text("noticeRead"))
                        .swipeActions { itemActions(item) }
                        .contextMenu { itemActions(item) }
                        .accessibilityActions { itemActions(item) }
                }
                if !loading && !failed && visibleItems.isEmpty {
                    ContentUnavailableView {
                        Label(recent ? String(localized: "No reminders yet") : String(localized: "Nothing needs your attention"), systemImage: "bell")
                    } description: {
                        if !recent { Text("You can find past reminders in Recent.") }
                    }.listRowBackground(Color.clear)
                }
                if next != nil { LoadMoreRow(working: loading, trigger: items.count) { await load(more: true) } }
            }
            NotificationSetupPrompt { settingUp = true }
        }
        .privateScreen().navigationTitle(NoticeCopy.text("noticeTitle"))
        .navigationBarTitleDisplayMode(typeSize.isAccessibilitySize ? .inline : .large)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button("Mark reminders for all accounts as read") { confirmingRead = true }
                        .disabled(reading != nil || !hasUnread).accessibilityIdentifier("reminders.read-all")
                        .accessibilityValue(String(model.notifications.status?.badge ?? 0))
                    NavigationLink(NoticeCopy.text("noticeSettings")) { NotificationSettingsView() }.accessibilityIdentifier("reminders.settings")
                    Button(NoticeCopy.text("noticeClear")) { Task { await clearRecent() } }.disabled(selected.isEmpty)
                } label: { Label("More", systemImage: "ellipsis") }
                    .accessibilityIdentifier("reminders.menu")
            }
        }
        .confirmationDialog("Mark reminders for all accounts as read", isPresented: $confirmingRead, titleVisibility: .visible) {
            Button("Mark as read") { Task { await markRead() } }.accessibilityIdentifier("reminders.confirm-read-all")
            Button("Cancel", role: .cancel) { }
        } message: { Text(NoticeCopy.text("noticeMarkReadHint")) }
        .task {
            guard !started else { return }
            if case .summary(let category) = entry.target { recent = category == .message || category == .service }
            if entry.ended || entry.retryRoute != nil || (entry.target != nil && (category == nil || entry.account != selected)) {
                destination = ReminderDestination(account: entry.account ?? selected, target: entry.target, ended: entry.ended, retryRoute: entry.retryRoute)
            }
            started = true
            await model.notifications.refresh(model)
        }
        .task(id: scope) { if started && destination == nil { await load() } }
        .refreshable { await model.notifications.refresh(model, receive: true); await load() }
        .navigationDestination(item: $destination) { destination in ReminderDestinationView(destination: destination) }
        .navigationDestination(isPresented: $settingUp) { NotificationSettingsView() }
        .sheet(item: $snoozing) { item in NavigationStack { SnoozeReminderView(item: item) } }
        .accessibilityIdentifier("reminders.page")
    }

    private func preview(for item: ReminderItem) -> ChannelItem? {
        guard model.unlocked, selected == item.account, case .channel(let id) = item.target else { return nil }
        return channelPreviews[id] ?? model.channels.first { $0.id == id }
    }

    @ViewBuilder private func itemActions(_ item: ReminderItem) -> some View {
        if item.isUnread {
            Button("Mark as read") { Task { await markRead(item) } }
                .tint(Theme.forest).disabled(reading != nil).accessibilityIdentifier("reminders.read.\(item.id)")
        }
        if item.open && item.actionable {
            Button(NoticeCopy.text("noticeLater")) { snoozing = item }.tint(Theme.forest)
        }
    }

    private func load(more: Bool = false) async {
        guard !selected.isEmpty else { items = []; next = nil; return }
        let current = scope, offset = more ? next ?? 0 : 0
        let token = UUID(); request = token
        let identity = "\(selected):\(recent)"
        if itemScope != identity { items = []; next = nil; itemScope = identity }
        loading = true; failed = false; channelPreviews = [:]
        defer { if request == token { loading = false } }
        if model.uiTesting { items = model.notifications.fixtureItems.filter { $0.account == selected && (recent || $0.open && $0.actionable) }; next = nil; return }
        do {
            guard let engine = model.engine else { return }
            let page: ReminderPage = try model.decode(try await engine.notificationItems(selected: selected, scope: recent ? .recent : .needsAction, offset: offset))
            guard scope == current, request == token, !Task.isCancelled else { return }
            items = more ? items + page.items : page.items; next = page.next
            if model.unlocked {
                let channels = try? await engine.channels()
                guard scope == current, request == token, !Task.isCancelled else { return }
                channelPreviews = Dictionary((channels?.items ?? []).map { ($0.id, $0) }, uniquingKeysWith: { _, new in new })
            }
        } catch { if scope == current && !Task.isCancelled { failed = true } }
    }

    private func clearRecent() async {
        do { try await model.engine?.notificationClearRecent(selected: selected); await load() }
        catch { model.report(error) }
    }

    private func markRead(_ item: ReminderItem? = nil) async {
        guard reading == nil else { return }
        let epoch = model.epoch
        reading = item?.id ?? "all"
        defer { reading = nil }
        do {
            try await model.notifications.markRead(model, item: item)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            await load()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            model.notice = String(localized: "Reminders marked as read")
        } catch { if epoch == model.epoch { model.report(error) } }
    }
}

private struct ReminderRow: View {
    let item: ReminderItem
    let channel: ChannelItem?
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            if !typeSize.isAccessibilitySize { ObjectIcon(name: item.target.symbol) }
            VStack(alignment: .leading, spacing: 5) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(channel?.name ?? item.title).font(.headline).foregroundStyle(.primary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if item.isUnread { Circle().fill(Theme.forest).frame(width: 7, height: 7).accessibilityHidden(true) }
                }
                Text(channel.flatMap { $0.preview.isEmpty ? nil : $0.preview } ?? item.body)
                    .font(.subheadline).foregroundStyle(.secondary).lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) { metadata }
                    VStack(alignment: .leading, spacing: 4) { metadata }
                }.font(.caption).foregroundStyle(.secondary)
                if let deadline = item.deadline, item.open {
                    Text("Due \(ConversationTimeline.date(deadline).formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption).foregroundStyle(.primary)
                }
            }
            if !typeSize.isAccessibilitySize {
                Image(systemName: "chevron.forward").font(.caption).foregroundStyle(.tertiary).padding(.top, 12).accessibilityHidden(true)
            }
        }.padding(.vertical, 6).contentShape(Rectangle()).accessibilityElement(children: .combine)
    }

    @ViewBuilder private var metadata: some View {
        Text(verbatim: listDateText(ConversationTimeline.date(item.updatedAt)))
        if item.open && item.actionable { Text(NoticeCopy.text("noticeNeedsAction")) }
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
