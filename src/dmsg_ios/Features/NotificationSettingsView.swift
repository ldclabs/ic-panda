import SwiftUI
import UserNotifications
@preconcurrency import DMsgCore

struct NotificationSettingsView: View {
    var focusRegistration = false
    @Environment(SessionModel.self) private var model
    @State private var status = String(localized: "Checking permission…")
    @State private var saved: NoticePreferences?
    @State private var draft: NoticePreferences?
    @State private var loading = false
    @State private var working = false
    @State private var failed = false
    @State private var request: UUID?
    /// Other local accounts already receiving notifications on this device.
    @State private var others: [String] = []
    @State private var linking = false
    @State private var draftGeneration: UInt64 = 0
    @State private var saveTask: Task<Void, Never>?
    @State private var saveFailed = false
    private var dirty: Bool { draft?.global != saved?.global || draft?.account != saved?.account }
    private var scope: String { "\(model.epoch):\(model.session?.account ?? "")" }
    private var statusMessage: String? {
        if saved?.conflict == true { return String(localized: "The record changed. Refresh and review your edits before saving again.") }
        if saveFailed || failed { return String(localized: "Connection unavailable. Your saved content stays on this device.") }
        if saved?.registrationRequired == true { return String(localized: "Notifications may be delayed. Open dMsg to sync your content.") }
        switch model.push.registration {
        case .failed, .denied, .waiting, .unsupported: return model.push.registration.message
        case .idle, .enabled:
            if saved?.remoteSupported == false { return NoticeCopy.text("noticeRemote") }
            if saved?.pending == true { return NoticeCopy.text("noticePendingHint") }
            if saved?.global.enabled != true || saved?.account?.enabled != true { return nil }
            return model.push.registration.message
        }
    }
    var body: some View {
        Form {
            if focusRegistration, draft != nil { registrationSection }
            Section {
                LabeledContent("System permission", value: status)
                Button("Open notification settings") { if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) } }
                SettingsAction(LocalizedStringKey(NoticeCopy.text("noticeTest"))) { Task { await test() } }.accessibilityIdentifier("notifications.test")
            }
            if draft != nil {
                Section(NoticeCopy.text("noticeSettings")) {
                    Toggle(NoticeCopy.text("noticeEnable"), isOn: Binding(get: { draft?.global.enabled == true }, set: { value in
                        draft?.global.enabled = value
                        if value, saved?.global.revision == 0, draft?.account?.revision == 0 { draft?.account?.enabled = true }
                    })).accessibilityIdentifier("notifications.enabled")
                    Picker(NoticeCopy.text("noticeBadge"), selection: Binding(get: { draft?.global.badge ?? "all" }, set: { draft?.global.badge = $0 })) {
                        Text(NoticeCopy.text("noticeBadgeAll")).tag("all")
                        Text(NoticeCopy.text("noticeBadgeActions")).tag("actions")
                        Text(NoticeCopy.text("noticeBadgeOff")).tag("off")
                    }
                    Menu("Pause notifications") {
                        Button(NoticeCopy.text("noticePause15")) { Task { await pause(NotificationTime.now + 900_000) } }
                        Button(NoticeCopy.text("noticePauseHour")) { Task { await pause(NotificationTime.now + 3_600_000) } }
                        Button(NoticeCopy.text("noticePauseTomorrow")) { Task { await pause(NotificationTime.tomorrow()) } }
                    }
                    if let until = draft?.global.pausedUntil, until > NotificationTime.now {
                        HStack {
                            Text(Date(timeIntervalSince1970: Double(until) / 1000), style: .relative).foregroundStyle(.secondary)
                            Spacer()
                            Button(NoticeCopy.text("noticeResume")) { Task { await pause(nil) } }.buttonStyle(.borderless)
                        }
                    }
                }
                Section(NoticeCopy.text("noticeQuiet")) {
                    Toggle(NoticeCopy.text("noticeQuiet"), isOn: Binding(get: { draft?.global.quietHours != nil }, set: { enabled in
                        draft?.global.quietHours = enabled ? NoticeQuietHours(start: 22 * 60, end: 8 * 60, timezone: TimeZone.current.identifier) : nil
                    })).accessibilityIdentifier("notifications.quiet")
                    if draft?.global.quietHours != nil {
                        DatePicker(NoticeCopy.text("noticeFrom"), selection: time(\.start), displayedComponents: .hourAndMinute)
                        DatePicker(NoticeCopy.text("noticeUntil"), selection: time(\.end), displayedComponents: .hourAndMinute)
                        NavigationLink {
                            TimeZonePicker(selection: Binding(get: { draft?.global.quietHours?.timezone ?? TimeZone.current.identifier }, set: { draft?.global.quietHours?.timezone = $0 }))
                        } label: {
                            LabeledContent(NoticeCopy.text("noticeZone"), value: draft?.global.quietHours?.timezone ?? TimeZone.current.identifier)
                        }
                        Toggle(NoticeCopy.text("noticeSecurityException"), isOn: Binding(get: { draft?.global.securityException == true }, set: { draft?.global.securityException = $0 }))
                    }
                }
                if draft?.account != nil {
                    Section(NoticeCopy.text("noticeAccount")) {
                        Toggle(NoticeCopy.text("noticeAccount"), isOn: Binding(get: { draft?.account?.enabled == true }, set: { draft?.account?.enabled = $0 })).accessibilityIdentifier("notifications.account")
                        ForEach(ReminderCategory.allCases, id: \.self) { category in
                            VStack(alignment: .leading) {
                                Toggle(category.title, isOn: categoryValue(category, sound: false))
                                Toggle(NoticeCopy.text("noticeSound"), isOn: categoryValue(category, sound: true))
                                    .disabled(draft?.account?.categories[category.rawValue]?.enabled != true)
                            }
                        }
                        Toggle(NoticeCopy.text("noticeMerge"), isOn: Binding(get: { draft?.account?.mergeDevices == true }, set: { draft?.account?.mergeDevices = $0 }))
                    }
                }
                if !focusRegistration { registrationSection }
            } else {
                Section {
                    if loading { ProgressView("Loading…") }
                    else {
                        if failed { NoticeBar(text: String(localized: "Connection unavailable. Your saved content stays on this device.")) }
                        SettingsAction("Retry") { Task { await load() } }
                    }
                }
            }
        }
        .disabled(working).privateScreen().navigationTitle(NoticeCopy.text("noticeSettings"))
        .onChange(of: draft?.global) { _, _ in scheduleSave() }
        .onChange(of: draft?.account) { _, _ in scheduleSave() }
        .onChange(of: model.notifications.preferences) { _, value in
            guard !working, let value, value.accountId == model.session?.account else { return }
            let keep = dirty
            saved = value
            if !keep { draft = value }
        }
        .confirmationDialog(NoticeCopy.text("noticeLinkTitle"), isPresented: $linking, titleVisibility: .visible) {
            Button(NoticeCopy.text("noticeLinkConfirm")) { Task { await save(linked: true) } }
            Button("Cancel", role: .cancel) { draft?.account?.enabled = false }
        } message: {
            Text(NoticeCopy.text("noticeLinkBody") + "\n\n" + NoticeCopy.text("noticeLinkAccounts") + "\n" + others.joined(separator: "\n"))
        }
        .task(id: scope) { draftGeneration = model.viewGeneration; await load() }
        .onDisappear {
            // Leaving does not lose a change that was still waiting to be saved.
            saveTask?.cancel()
            if dirty, let draft {
                model.keepFormDraft("notifications", value: draft, generation: draftGeneration)
                Task { await save() }
            }
        }
        .refreshable { await model.notifications.refresh(model, receive: true); await load() }
        .reminderReadOnDisplay(ready: saved != nil && !loading && !failed)
        .accessibilityIdentifier("notifications.settings")
    }

    private var registrationSection: some View {
        Section {
            if working {
                ProgressView("Saving…").accessibilityIdentifier("notifications.saving")
            } else {
                if !model.push.working, let message = statusMessage {
                    Label(message, systemImage: "info.circle")
                        .font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier(saved?.pending == true ? "notifications.pending" : "notifications.status")
                }
                if saved?.conflict == true || saveFailed {
                    SettingsAction("Retry", prominent: true) { Task { await save() } }.accessibilityIdentifier("notifications.save")
                } else if saved?.account?.enabled == true && (model.push.working || saved?.pending == true || saved?.registrationRequired == true || model.push.registration != .enabled) {
                    SettingsAction(model.push.working ? "Waiting for notification registration…" : "Retry registration", prominent: focusRegistration, working: model.push.working) {
                        Task {
                            let epoch = model.epoch
                            await model.push.refresh(model, retryRegistration: true); await load()
                            if epoch == model.epoch, model.push.registration == .enabled, saved?.registrationRequired != true, saved?.pending != true {
                                model.notice = String(localized: "Notification connection restored")
                            }
                        }
                    }.tint(focusRegistration ? Theme.forestFill : Theme.forest).accessibilityIdentifier("notifications.registration")
                }
            }
        } footer: {
            Text(NoticeCopy.text("noticeScopeHint"))
        }
    }

    private func categoryValue(_ category: ReminderCategory, sound: Bool) -> Binding<Bool> {
        Binding(get: { sound ? draft?.account?.categories[category.rawValue]?.sound == true : draft?.account?.categories[category.rawValue]?.enabled == true }, set: { value in
            if sound { draft?.account?.categories[category.rawValue]?.sound = value }
            else { draft?.account?.categories[category.rawValue]?.enabled = value }
        })
    }
    private func time(_ key: WritableKeyPath<NoticeQuietHours, Int>) -> Binding<Date> {
        Binding(get: {
            let minute = draft?.global.quietHours?[keyPath: key] ?? 0
            return Calendar.current.date(bySettingHour: minute / 60, minute: minute % 60, second: 0, of: .now) ?? .now
        }, set: { date in
            let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
            draft?.global.quietHours?[keyPath: key] = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
        })
    }
    private func load() async {
        let current = scope, token = UUID(); request = token; loading = true; failed = false
        defer { if request == token { loading = false } }
        // The system can take seconds to answer, and the settings do not depend on it.
        Task { await checkPermission() }
        do {
            let next: NoticePreferences, counts: ReminderStatus?
            if model.uiTesting, let fixture = model.notifications.preferences { next = fixture; counts = model.notifications.status }
            else {
                guard let engine = model.engine else { return }
                next = try model.decode(try await engine.notificationPreferences(selected: model.session?.account))
                counts = try model.decode(try await engine.notificationStatus())
            }
            guard current == scope, request == token, !Task.isCancelled else { return }
            // A change made while this was loading stays, and saves itself.
            let keep = dirty
            saved = next; if !keep || draft == nil {
                let restored = model.formDraft("notifications", as: NoticePreferences.self)
                if let restored, restored.accountId == next.accountId { draft = restored } else { draft = next }
            }
            others = (counts?.accounts ?? []).filter { $0.enabled && $0.account != next.accountId }.map(\.account)
        } catch { if current == scope { failed = true } }
    }
    /// Changes save shortly after the last one, so a run of toggles is one write.
    private func checkPermission() async {
        let current = scope
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        guard current == scope else { return }
        status = [.authorized, .provisional, .ephemeral].contains(settings.authorizationStatus) ? String(localized: "Allowed") : String(localized: "Not enabled")
    }

    private func scheduleSave() {
        saveTask?.cancel()
        guard dirty else { return }
        saveTask = Task {
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled, dirty else { return }
            // Only the debounce is cancellable by later edits or navigation.
            // An in-flight save must finish; a replacement would see `working`.
            saveTask = nil
            await save()
        }
    }
    private func save(linked: Bool = false) async {
        guard let saved, var value = draft, !working, draftGeneration == model.viewGeneration else { return }
        saveFailed = false
        // Registering another account tells the notification service and the push
        // provider that these accounts share this device; that needs consent.
        if !linked, value.account?.enabled == true, saved.account?.enabled != true, !others.isEmpty { linking = true; return }
        let current = scope; working = true; defer { working = false }
        do {
            if let hours = value.global.quietHours, hours.start == hours.end { throw MobileError.Failure(code: "INVALID_INPUT") }
            if value.global.enabled && !saved.global.enabled { _ = try await model.push.requestPermission() }
            #if DEBUG
            if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-notification-slow-save") {
                // Like an in-flight native/FFI call, this finishes after its caller leaves.
                await Task.detached { try? await Task.sleep(for: .seconds(6)) }.value
            }
            #endif
            guard current == scope, !Task.isCancelled else { return }
            if model.uiTesting {
                value.global.revision += 1; value.account?.revision += 1; value.globalPendingSync = true
                self.saved = value; draft = value
                model.notifications.saveFixture(value); model.settingsDrafts.removeValue(forKey: "notifications"); return
            }
            guard let engine = model.engine else { return }
            if value.global != saved.global || saved.globalConflict == true {
                value.global = try model.decode(try await engine.notificationSetGlobal(expected: saved.global.revision, settingsJson: NoticeEncoding.json(value.global)))
            }
            guard current == scope, !Task.isCancelled else { return }
            if let account = value.account, let id = value.accountId, account != saved.account || saved.accountConflict == true {
                value.account = try model.decode(try await engine.notificationSetAccount(selected: id, expected: saved.account?.revision ?? 0, settingsJson: NoticeEncoding.json(account)))
            }
            guard current == scope, !Task.isCancelled else { return }
            self.saved = value; draft = value; model.settingsDrafts.removeValue(forKey: "notifications")
            // The change is saved; what follows only refreshes around it.
            working = false
            await model.push.refresh(model); await model.notifications.refresh(model, receive: true)
            await load()
        } catch { if current == scope { saveFailed = true; await load(); model.report(error) } }
    }
    private func pause(_ until: UInt64?) async { draft?.global.pausedUntil = until; await save() }
    private func test() async {
        do { if try await model.push.requestPermission() { try await model.notifications.test() } }
        catch { model.report(error) }
        await load()
    }
}

private struct TimeZonePicker: View {
    @Binding var selection: String
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    private var zones: [String] {
        let all = Array(Set([TimeZone.current.identifier] + TimeZone.knownTimeZoneIdentifiers)).sorted()
        return query.isEmpty ? all : all.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        List(zones, id: \.self) { zone in
            Button { selection = zone; dismiss() } label: {
                HStack {
                    Text(verbatim: zone).foregroundStyle(.primary)
                    Spacer()
                    if zone == selection { Image(systemName: "checkmark").foregroundStyle(Theme.forest).accessibilityLabel("Selected") }
                }
            }
        }.privateScreen().navigationTitle(NoticeCopy.text("noticeZone")).navigationBarTitleDisplayMode(.inline)
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always))
    }
}

/// New installations start with notifications off. Once the account is set up,
/// this points to the settings; it never asks the system for permission.
struct NotificationSetupPrompt: View {
    @Environment(SessionModel.self) private var model
    let open: () -> Void
    var body: some View {
        if let account = model.session?.account, model.unlocked, let preferences = model.notifications.preferences,
           preferences.accountId == account, !preferences.securityAlertsOn, !model.notificationSetupDismissed.contains(account) {
            VStack(alignment: .leading, spacing: 8) {
                Label(NoticeCopy.text("noticeSetupTitle"), systemImage: "shield").font(.subheadline.weight(.semibold))
                Text(NoticeCopy.text("noticeSetupBody")).font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack {
                    Button(NoticeCopy.text("noticeSetupOpen"), action: open).buttonStyle(.bordered).accessibilityIdentifier("notifications.setup.open")
                    Button(NoticeCopy.text("noticeSetupLater")) { model.setNotificationSetupDismissed(true, account: account) }
                        .buttonStyle(.borderless).accessibilityIdentifier("notifications.setup.later")
                }
            }.padding(.vertical, 4).accessibilityElement(children: .contain).accessibilityIdentifier("notifications.setup")
        }
    }
}
