import SwiftUI

/// Full-row actions are centered; navigation and actions within an item keep
/// their own layout. Native button styles preserve roles and interaction states.
struct SettingsAction: View {
    let title: LocalizedStringKey
    let icon: Image?
    let role: ButtonRole?
    let prominent: Bool
    let working: Bool
    let action: () -> Void

    init(_ title: LocalizedStringKey, systemImage: String? = nil, glyph: AppGlyph? = nil, role: ButtonRole? = nil,
         prominent: Bool = false, working: Bool = false, action: @escaping () -> Void) {
        self.title = title; icon = glyph?.image ?? systemImage.map { Image(systemName: $0) }; self.role = role
        self.prominent = prominent; self.working = working; self.action = action
    }

    var body: some View {
        if prominent && role != .destructive {
            Button(role: role, action: action) { SettingsActionLabel(title, icon: icon, working: working) }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .listRowBackground(Color.clear).listRowSeparator(.hidden)
                .listRowInsets(EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0))
                .disabled(working)
                .accessibilityValue(working ? String(localized: "In progress") : "")
        } else {
            Button(role: role, action: action) {
                SettingsActionLabel(title, icon: icon, working: working)
                    .padding(.horizontal, 20).padding(.vertical, 10)
                    .frame(minHeight: 44).contentShape(Rectangle())
            }.buttonStyle(.borderless)
                .disabled(working)
                .accessibilityValue(working ? String(localized: "In progress") : "")
                .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 0, trailing: 0))
                .alignmentGuide(.listRowSeparatorLeading) { _ in 20 }
        }
    }
}

struct SettingsActionLabel: View {
    let title: LocalizedStringKey
    let icon: Image?
    var working = false

    init(_ title: LocalizedStringKey, systemImage: String? = nil) {
        self.title = title; icon = systemImage.map { Image(systemName: $0) }
    }

    init(_ title: LocalizedStringKey, icon: Image?, working: Bool = false) {
        self.title = title; self.icon = icon; self.working = working
    }

    var body: some View {
        HStack(spacing: 8) {
            if working { ProgressView().controlSize(.small).accessibilityHidden(true) }
            else if let icon { icon.accessibilityHidden(true) }
            Text(title)
        }.frame(maxWidth: .infinity).multilineTextAlignment(.center)
            .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
    }
}

struct IDCopyButton: View {
    let value: String
    let label: LocalizedStringKey
    @Environment(SessionModel.self) private var model
    @State private var copied = CopiedState()

    var body: some View {
        Button { copied.copy(value, model: model) } label: {
            Image(systemName: copied.shown ? "checkmark" : "doc.on.doc")
                .copiedTransition().frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }.buttonStyle(.borderless)
            .accessibilityLabel(label).accessibilityValue(copied.shown ? String(localized: "Copied") : value)
            .copiedState($copied)
    }
}

/// Copies a value and confirms in place: the icon turns into a checkmark and the title into
/// "Copied" for a moment, so the button needs no notice. Copies from menus, which close, and
/// copies of vault values, whose notice gives their lifetime, keep `model.notice` instead.
struct CopyButton: View {
    let title: LocalizedStringKey
    let value: String
    var systemImage: String?
    /// A full-row action laid out like `SettingsAction`.
    var centered = false
    @Environment(SessionModel.self) private var model
    @State private var copied = CopiedState()

    init(_ title: LocalizedStringKey, systemImage: String? = nil, value: String, centered: Bool = false) {
        self.title = title; self.systemImage = systemImage; self.value = value; self.centered = centered
    }

    var body: some View {
        Group {
            if centered {
                Button { copied.copy(value, model: model) } label: {
                    HStack(spacing: 8) { icon; Text(copied.shown ? "Copied" : title) }
                        .frame(maxWidth: .infinity).multilineTextAlignment(.center)
                        .padding(.horizontal, 20).padding(.vertical, 10)
                        .frame(minHeight: 44).contentShape(Rectangle())
                }.buttonStyle(.borderless)
                    .listRowInsets(EdgeInsets(top: 0, leading: 0, bottom: 0, trailing: 0))
                    .alignmentGuide(.listRowSeparatorLeading) { _ in 20 }
            } else {
                Button { copied.copy(value, model: model) } label: {
                    Label { Text(copied.shown ? "Copied" : title) } icon: { icon }
                }
            }
        }.copiedState($copied)
    }

    /// One image whose symbol changes, so the checkmark replaces the copy icon in place.
    @ViewBuilder private var icon: some View {
        if let name = copied.shown ? "checkmark" : systemImage { Image(systemName: name).copiedTransition().accessibilityHidden(true) }
    }
}

/// How long a copy button shows that it copied; a new copy starts the time again.
struct CopiedState: Equatable {
    fileprivate(set) var shown = false
    fileprivate var revision = 0

    @MainActor mutating func copy(_ value: String, model: SessionModel) {
        model.clipboard.copy(value)
        shown = true; revision &+= 1
        UIAccessibility.post(notification: .announcement, argument: String(localized: "Copied"))
    }
}

extension View {
    /// The checkmark replaces the copy icon as a symbol transition; with Reduce Motion it swaps.
    fileprivate func copiedTransition() -> some View { modifier(CopiedTransition()) }

    /// Settles the checkmark in and turns it back after 1.5 seconds.
    fileprivate func copiedState(_ state: Binding<CopiedState>) -> some View {
        motion(Motion.settle, value: state.wrappedValue.shown)
            .task(id: state.wrappedValue.revision) {
                guard state.wrappedValue.shown else { return }
                try? await Task.sleep(for: .seconds(1.5))
                if !Task.isCancelled { state.wrappedValue.shown = false }
            }
    }
}

private struct CopiedTransition: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View { content.contentTransition(reduceMotion ? .identity : .symbolEffect(.replace)) }
}

func utf8ByteCounter(_ text: String, limit: Int64) -> String {
    text.utf8.count.formatted() + " / " + limit.formatted(.byteCount(style: .file, allowedUnits: .bytes))
}

extension View {
    /// Keep the dismissal action above the focused form and the system keyboard.
    func keyboardDone(visible: Bool, identifier: String, dismiss: @escaping () -> Void) -> some View {
        safeAreaInset(edge: .bottom, spacing: 0) {
            if visible {
                HStack {
                    Spacer()
                    Button(action: dismiss) {
                        Text("Done").padding(.horizontal, 16).frame(minHeight: 44).contentShape(Rectangle())
                    }.buttonStyle(.borderless).accessibilityIdentifier(identifier)
                }.background(.bar)
            }
        }
    }
}

/// Explicit multi-selection in a list. The label and checkmark share one hit target.
struct SelectionRowToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack {
                configuration.label
                Spacer(minLength: 8)
                Image(systemName: configuration.isOn ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(configuration.isOn ? Theme.forest : Color.secondary)
                    .accessibilityHidden(true)
            }.frame(minHeight: 44).contentShape(Rectangle())
        }.buttonStyle(.plain)
            .accessibilityRepresentation { Toggle(isOn: configuration.$isOn) { configuration.label }.toggleStyle(.switch) }
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
            }.font(.footnote).foregroundStyle(.secondary)
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
    var failureText: String? = nil
    let retry: () -> Void

    var body: some View {
        if loading { ProgressView("Loading…") }
        if failed {
            NoticeBar(text: failureText ?? String(localized: "Connection unavailable. Your saved content stays on this device."))
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
