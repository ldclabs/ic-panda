import SwiftUI

/// Full-row actions are centered; navigation and actions within an item keep
/// their own layout. Native button styles preserve roles and interaction states.
struct SettingsAction: View {
    let title: LocalizedStringKey
    let icon: Image?
    let role: ButtonRole?
    let prominent: Bool
    let action: () -> Void

    init(_ title: LocalizedStringKey, systemImage: String? = nil, glyph: AppGlyph? = nil, role: ButtonRole? = nil,
         prominent: Bool = false, action: @escaping () -> Void) {
        self.title = title; icon = glyph?.image ?? systemImage.map { Image(systemName: $0) }; self.role = role
        self.prominent = prominent; self.action = action
    }

    var body: some View {
        if prominent && role != .destructive {
            Button(role: role, action: action) { SettingsActionLabel(title, icon: icon) }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .listRowBackground(Color.clear).listRowSeparator(.hidden)
                .listRowInsets(EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0))
        } else {
            Button(role: role, action: action) {
                SettingsActionLabel(title, icon: icon)
                    .padding(.horizontal, 20).padding(.vertical, 10)
                    .frame(minHeight: 44).contentShape(Rectangle())
            }.buttonStyle(.borderless)
                .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 0, trailing: 0))
                .alignmentGuide(.listRowSeparatorLeading) { _ in 20 }
        }
    }
}

struct SettingsActionLabel: View {
    let title: LocalizedStringKey
    let icon: Image?

    init(_ title: LocalizedStringKey, systemImage: String? = nil) {
        self.title = title; icon = systemImage.map { Image(systemName: $0) }
    }

    init(_ title: LocalizedStringKey, icon: Image?) {
        self.title = title; self.icon = icon
    }

    var body: some View {
        HStack(spacing: 8) {
            if let icon { icon.accessibilityHidden(true) }
            Text(title)
        }.frame(maxWidth: .infinity).multilineTextAlignment(.center)
            .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
    }
}

struct IDCopyButton: View {
    let value: String
    let label: LocalizedStringKey
    @Environment(SessionModel.self) private var model

    var body: some View {
        Button {
            model.clipboard.copy(value)
            model.notice = String(localized: "Copied")
        } label: {
            Image(systemName: "doc.on.doc").frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }.buttonStyle(.borderless)
            .accessibilityLabel(label).accessibilityValue(value)
    }
}

/// Persistent labels remain visible after a field has a value.
struct SettingsField<Content: View>: View {
    let title: LocalizedStringKey
    var counter: String?
    var invalid = false
    @ViewBuilder var content: () -> Content

    init(_ title: LocalizedStringKey, counter: String? = nil, invalid: Bool = false, @ViewBuilder content: @escaping () -> Content) {
        self.title = title; self.counter = counter; self.invalid = invalid
        self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                Spacer()
                if let counter { Text(verbatim: counter).foregroundStyle(invalid ? Color.red : Color.secondary) }
            }.font(.caption).foregroundStyle(.secondary)
            content().accessibilityLabel(title)
        }.padding(.vertical, 2)
    }
}

/// Reaching the end of a list loads the next page. The button stays for
/// VoiceOver and for retrying a page that failed.
struct LoadMoreRow: View {
    var title: LocalizedStringKey = "Load more"
    var working = false
    let trigger: Int
    let load: @MainActor () async -> Void

    var body: some View {
        Group {
            if working { ProgressView() }
            else { Button(title) { Task { await load() } } }
        }.frame(maxWidth: .infinity, minHeight: 44)
            .task(id: trigger) { await load() }
    }
}

struct SettingsLoadStatus: View {
    let loading: Bool
    let failed: Bool
    let retry: () -> Void

    var body: some View {
        if loading { ProgressView("Loading…") }
        if failed {
            NoticeBar(text: String(localized: "Connection unavailable. Your saved content stays on this device."))
            SettingsAction("Retry", action: retry).disabled(loading)
        }
    }
}

extension View {
    func confirmLeavingDraft(_ dirty: Bool) -> some View { modifier(DraftExitConfirmation(dirty: dirty)) }
}

private struct DraftExitConfirmation: ViewModifier {
    let dirty: Bool
    @Environment(\.dismiss) private var dismiss
    @State private var confirming = false

    func body(content: Content) -> some View {
        content.navigationBarBackButtonHidden(dirty)
            .interactiveDismissDisabled(dirty)
            .toolbar {
                if dirty {
                    ToolbarItem(placement: .topBarLeading) {
                        Button { confirming = true } label: { Label("Back", systemImage: "chevron.backward") }
                    }
                }
            }
            .confirmationDialog("Leave this form?", isPresented: $confirming, titleVisibility: .visible) {
                Button("Keep draft and leave") { dismiss() }
                Button("Cancel", role: .cancel) {}
            } message: { Text("Your draft stays until this workspace locks. Nothing is submitted.") }
    }
}

func deviceRoleTitle(_ role: String) -> String {
    switch role.lowercased() {
    case "administrator": String(localized: "Administrator")
    case "member": String(localized: "Member")
    default: role
    }
}

func deviceCapabilityTitle(_ capability: String) -> String {
    switch capability {
    case "ContentSign", "content_sign": String(localized: "Content signing")
    case "VaultUnlock", "vault_unlock": String(localized: "Unlock workspace")
    case "RootManage", "root_manage": String(localized: "Account security")
    case "FormalApprove", "formal_approve": String(localized: "Approvals")
    case "PaymentOffer", "payment_offer": String(localized: "Payments")
    case "WalletApprove", "wallet_approve": String(localized: "Multisig wallets")
    default: capability
    }
}
