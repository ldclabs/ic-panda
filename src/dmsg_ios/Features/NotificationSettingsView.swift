import SwiftUI
import UserNotifications

struct NotificationSettingsView: View {
    @State private var status = String(localized: "Checking permission…")
    var body: some View {
        List {
            LabeledContent("System permission", value: status)
            NoticeBar(text: String(localized: "Notifications contain no message text, sender, filename or approval payload. Opening dMsg always refreshes the actual state."))
            NoticeBar(text: String(localized: "Push notifications are not enabled in this build. Open dMsg to synchronize."))
        }.privateScreen().navigationTitle("Notifications").task {
            let settings = await UNUserNotificationCenter.current().notificationSettings()
            status = settings.authorizationStatus == .authorized ? String(localized: "Allowed") : String(localized: "Not enabled")
        }
    }
}
