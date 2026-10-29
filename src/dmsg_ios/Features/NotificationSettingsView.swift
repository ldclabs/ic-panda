import SwiftUI
import UserNotifications
@preconcurrency import DMsgCore

struct NotificationSettingsView: View {
    @Environment(SessionModel.self) private var model
    @State private var status = String(localized: "Checking permission…")
    @State private var saved: NoticePreferences?
    @State private var draft: NoticePreferences?
    @State private var loading = false
    @State private var working = false
    @State private var failed = false
    @State private var request: UUID?
    private var dirty: Bool { draft?.global != saved?.global || draft?.account != saved?.account }
    private var scope: String { "\(model.epoch):\(model.session?.account ?? "")" }
    var body: some View {
        Form {
            Section {
                LabeledContent("System permission", value: status)
                Button("Open notification settings") { if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) } }
                Button(NoticeCopy.text("noticeTest")) { Task { await test() } }.accessibilityIdentifier("notifications.test")
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
                    Menu(NoticeCopy.text("noticePauseHour")) {
                        Button(NoticeCopy.text("noticePause15")) { Task { await pause(NotificationTime.now + 900_000) } }
                        Button(NoticeCopy.text("noticePauseHour")) { Task { await pause(NotificationTime.now + 3_600_000) } }
                        Button(NoticeCopy.text("noticePauseTomorrow")) { Task { await pause(NotificationTime.tomorrow()) } }
                        Button(NoticeCopy.text("noticeResume")) { Task { await pause(nil) } }
                    }
                    if let until = draft?.global.pausedUntil, until > NotificationTime.now {
                        Text(Date(timeIntervalSince1970: Double(until) / 1000), style: .relative).foregroundStyle(.secondary)
                    }
                }
                Section(NoticeCopy.text("noticeQuiet")) {
                    Toggle(NoticeCopy.text("noticeQuiet"), isOn: Binding(get: { draft?.global.quietHours != nil }, set: { enabled in
                        draft?.global.quietHours = enabled ? NoticeQuietHours(start: 22 * 60, end: 8 * 60, timezone: TimeZone.current.identifier) : nil
                    })).accessibilityIdentifier("notifications.quiet")
                    if draft?.global.quietHours != nil {
                        DatePicker(NoticeCopy.text("noticeFrom"), selection: time(\.start), displayedComponents: .hourAndMinute)
                        DatePicker(NoticeCopy.text("noticeUntil"), selection: time(\.end), displayedComponents: .hourAndMinute)
                        Picker(NoticeCopy.text("noticeZone"), selection: Binding(get: { draft?.global.quietHours?.timezone ?? TimeZone.current.identifier }, set: { draft?.global.quietHours?.timezone = $0 })) {
                            ForEach(Array(Set([TimeZone.current.identifier] + TimeZone.knownTimeZoneIdentifiers)).sorted(), id: \.self) { Text($0).tag($0) }
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
                Section {
                    NoticeBar(text: NoticeCopy.text("noticeScopeHint"))
                    if saved?.remoteSupported == false { NoticeBar(text: NoticeCopy.text("noticeRemote")) }
                    if saved?.pending == true { NoticeBar(text: NoticeCopy.text("noticePending")); Text(NoticeCopy.text("noticePendingHint")).font(.footnote).foregroundStyle(.secondary) }
                    if let registration = model.push.status { NoticeBar(text: registration) }
                    Button(saved?.conflict == true ? String(localized: "Retry") : String(localized: "Save settings")) { Task { await save() } }
                        .disabled(!dirty && saved?.conflict != true).accessibilityIdentifier("notifications.save")
                    Button("Retry registration") { Task { await model.push.refresh(model); await load(preserve: true) } }
                }
            } else if !loading { Button("Retry") { Task { await load() } } }
            if loading || working || model.push.working { ProgressView() }
            if failed { NoticeBar(text: String(localized: "Connection unavailable. Your saved content stays on this device.")) }
        }
        .disabled(working).privateScreen().navigationTitle(NoticeCopy.text("noticeSettings"))
        .task(id: scope) { await load() }
        .refreshable { await model.notifications.refresh(model, receive: true); await load(preserve: dirty) }
        .accessibilityIdentifier("notifications.settings")
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
    private func load(preserve: Bool = false) async {
        let current = scope, token = UUID(); request = token; loading = true; failed = false
        defer { if request == token { loading = false } }
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        guard current == scope, request == token, !Task.isCancelled else { return }
        status = [.authorized, .provisional, .ephemeral].contains(settings.authorizationStatus) ? String(localized: "Allowed") : String(localized: "Not enabled")
        do {
            let next: NoticePreferences
            if model.uiTesting, let fixture = model.notifications.preferences { next = fixture }
            else { guard let engine = model.engine else { return }; next = try model.decode(try await engine.notificationPreferences(selected: model.session?.account)) }
            guard current == scope, request == token, !Task.isCancelled else { return }
            saved = next; if !preserve || draft == nil { draft = next }
        } catch { if current == scope { failed = true } }
    }
    private func save() async {
        guard let saved, var value = draft, !working else { return }
        let current = scope; working = true; defer { working = false }
        do {
            if let hours = value.global.quietHours, hours.start == hours.end { throw MobileError.Failure(code: "INVALID_INPUT") }
            if value.global.enabled && !saved.global.enabled { _ = try await model.push.requestPermission() }
            guard current == scope, !Task.isCancelled else { return }
            if model.uiTesting {
                value.global.revision += 1; value.account?.revision += 1; value.globalPendingSync = true
                model.notifications.saveFixture(value); self.saved = value; draft = value; return
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
            self.saved = value; draft = value
            await model.push.refresh(model); await model.notifications.refresh(model, receive: true)
            await load()
        } catch { if current == scope { await load(preserve: true); model.report(error) } }
    }
    private func pause(_ until: UInt64?) async { draft?.global.pausedUntil = until; await save() }
    private func test() async {
        do { if try await model.push.requestPermission() { try await model.notifications.test() } }
        catch { model.report(error) }
        await load(preserve: dirty)
    }
}
