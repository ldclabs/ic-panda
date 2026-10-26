import SwiftUI
import UserNotifications

struct NotificationSettingsView: View {
    @Environment(SessionModel.self) private var model
    @State private var status = String(localized: "Checking permission…")
    var body: some View {
        List {
            LabeledContent("System permission", value: status)
            Toggle("Notify this account on this device", isOn: Binding(get: { model.push.enabled(model.session?.account) }, set: { value in Task { await model.push.setEnabled(value, model: model) } })).disabled(model.push.working)
            if let status = model.push.status { NoticeBar(text: status) }
            if model.push.working { ProgressView() }
            Button("Retry registration") { Task { await model.push.refresh(model) } }.disabled(model.push.working)
            Button("Open notification settings") { if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) } }
            NoticeBar(text: String(localized: "Notifications contain no message text, sender, filename or approval payload. Opening dMsg always refreshes the actual state."))
        }.privateScreen().navigationTitle("Notifications").task {
            if let files = model.files { try? model.push.load(files) }
            let settings = await UNUserNotificationCenter.current().notificationSettings()
            status = settings.authorizationStatus == .authorized ? String(localized: "Allowed") : String(localized: "Not enabled")
        }
    }
}
