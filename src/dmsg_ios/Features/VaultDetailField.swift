import SwiftUI
@preconcurrency import DMsgCore

/// The value is the primary copy target; secondary actions belong to its menu.
struct VaultDetailField: View {
    let field: VaultField
    let value: String?
    let remaining: Int?
    let busy: Bool
    let readOnly: Bool
    let live: Bool
    let copied: Bool
    let reveal: () -> Void
    let copy: () -> Void
    let enlarge: () -> Void
    let reference: (() -> Void)?
    @Environment(\.dynamicTypeSize) private var size
    @Environment(\.layoutDirection) private var direction
    @Environment(\.openURL) private var openURL

    private var text: String { field.concealed && value == nil ? "••••••••" : value ?? field.value }
    private var layout: AnyLayout {
        size.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4)) : AnyLayout(HStackLayout(spacing: 8))
    }

    var body: some View {
        layout {
            if readOnly { content }
            else {
                Button(action: copy) { content }
                    .buttonStyle(.plain).disabled(busy)
                    .accessibilityLabel(Text("Copy") + Text(verbatim: " · " + vaultFieldTitle(field)))
                    .accessibilityValue(spokenValue)
                    .accessibilityIdentifier("vault.copy.\(field.id)")
                    .contextMenu { actions }
            }
            if !readOnly {
                HStack(spacing: 4) {
                    if let remaining, value != nil { VaultCodeCountdown(remaining: remaining) }
                    Menu("Actions", systemImage: copied ? "checkmark.circle.fill" : "ellipsis.circle.fill") { actions }
                        .labelStyle(.iconOnly).font(.system(size: 24)).symbolRenderingMode(.palette)
                        .foregroundStyle(copied ? Theme.forest : Color.secondary, Color.secondary.opacity(0.12))
                        .frame(width: 44, height: 44).contentShape(Rectangle())
                        .buttonStyle(.borderless).menuOrder(.fixed).disabled(busy)
                        .opacity(busy ? 0 : 1)
                        .overlay { if busy { ProgressView().accessibilityHidden(true) } }
                        .accessibilityLabel(Text("Actions") + Text(verbatim: " · " + vaultFieldTitle(field)))
                        .accessibilityValue(copied ? Text("Copied") : Text(verbatim: ""))
                        .accessibilityIdentifier("vault.field.more.\(field.id)")
                }
            }
        }.listRowInsets(EdgeInsets(top: 12, leading: 16, bottom: 12, trailing: 8))
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(vaultFieldTitle(field)).font(.footnote).foregroundStyle(.secondary).accessibilityHidden(true)
            Text(verbatim: text).font(field.concealed ? .body.monospaced() : .body)
                .foregroundStyle(field.kind == "url" && !field.concealed ? Theme.forest : Color.primary)
                .fixedSize(horizontal: false, vertical: true)
                .environment(\.layoutDirection, vaultFieldIsLTR(field, value: text) ? .leftToRight : direction)
                .accessibilityLabel(vaultFieldTitle(field)).accessibilityValue(spokenValue)
                .accessibilityIdentifier("vault.value.\(field.id)")
        }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).contentShape(Rectangle())
    }

    @ViewBuilder private var actions: some View {
        Button("Copy", systemImage: "doc.on.doc", action: copy).accessibilityIdentifier("vault.field.copy.\(field.id)")
        if field.concealed && !live {
            Button(value == nil ? "Reveal" : "Hide", systemImage: value == nil ? "eye" : "eye.slash", action: reveal)
                .accessibilityIdentifier("vault.reveal.\(field.id)")
        }
        if let url = vaultWebsiteURL(field) {
            Button("Open website", systemImage: "arrow.up.right.square") { openURL(url) }
                .accessibilityIdentifier("vault.open.\(field.id)")
        }
        Button("Show in Large Type", systemImage: "textformat.size", action: enlarge)
            .accessibilityIdentifier("vault.enlarge.\(field.id)")
        if let reference {
            Divider()
            Button("Field reference", systemImage: "link", action: reference)
                .accessibilityIdentifier("vault.reference.\(field.id)")
        }
    }

    private var spokenValue: String {
        if field.concealed && value == nil { return String(localized: "Hidden value") }
        if field.kind == "otp" { return text.map(String.init).joined(separator: " ") }
        return text
    }
}

struct VaultCodeCountdown: View {
    let remaining: Int

    var body: some View {
        ZStack {
            Circle().stroke(Theme.forest.opacity(0.15), lineWidth: 3)
            Circle().trim(from: 0, to: CGFloat(min(remaining, 30)) / 30)
                .stroke(Theme.forest, style: StrokeStyle(lineWidth: 3, lineCap: .round)).rotationEffect(.degrees(-90))
            Text(remaining.formatted()).font(.caption2.monospacedDigit())
        }.frame(width: 30, height: 30).accessibilityElement(children: .ignore)
            .accessibilityLabel("Remaining").accessibilityValue(remaining.formatted())
    }
}
