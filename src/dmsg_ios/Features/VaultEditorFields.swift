import SwiftUI
import VisionKit
import AVFoundation
@preconcurrency import DMsgCore

struct VaultEditorField: View {
    @Binding var field: VaultDraft.Field
    let existing: Bool
    let validationRequested: Bool
    @Binding var addedField: String?
    var focus: FocusState<VaultEditorFocus?>.Binding
    @Environment(\.dynamicTypeSize) private var textSize
    @State private var showingDraft = false
    private enum Tool: String, Identifiable { case scanner, generator, date; var id: Self { self } }
    @State private var tool: Tool?
    @State private var sectionExpanded = false
    @State private var requestingCamera = false
    @State private var cameraDenied = false
    @State private var nameVisited = false
    @State private var valueVisited = false
    private var label: String { field.title }
    private var namePrompt: String { field.id.hasPrefix("field_") ? String(localized: "Field name") : (field.kind == "otp" ? vaultFieldKindTitle(field.kind) : vaultFieldName(field.id)) }
    private var keeping: Bool { existing && field.concealed && !field.changed }
    private var placeholder: LocalizedStringKey { keeping ? "Leave empty to keep existing value" : "Value" }
    /// Values the person chooses freely; a card's numbers and key material come from elsewhere.
    private var generates: Bool { field.kind != "otp" && !["number", "cvv", "pin", "private_key", "public_key", "phrase"].contains(field.id) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                let header = textSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4)) : AnyLayout(HStackLayout(alignment: .firstTextBaseline))
                header {
                    Text("Field name")
                    if !textSize.isAccessibilitySize { Spacer() }
                    Text(vaultFieldKindTitle(field.kind))
                }.font(.caption).foregroundStyle(.secondary)
                TextField("Field name", text: $field.label, prompt: Text(namePrompt))
                    .font(.body.weight(.medium)).focused(focus, equals: .name(field.id))
                    .submitLabel(.next).onSubmit { focus.wrappedValue = .value(field.id) }
                    .accessibilityIdentifier("vault.edit.name.\(field.id)")
                if (nameVisited || validationRequested), let error = vaultFieldNameError(field) {
                    Text(error).font(.caption).foregroundStyle(.red).accessibilityIdentifier("vault.edit.name-error.\(field.id)")
                }
            }
            VStack(alignment: .leading, spacing: 6) {
                Text("Value").font(.caption).foregroundStyle(.secondary)
                if field.concealed && !showingDraft {
                    SecureField(placeholder, text: valueBinding).focused(focus, equals: .value(field.id))
                        .accessibilityLabel(label).accessibilityIdentifier("vault.edit.field.\(field.id)")
                } else if ["private_key", "public_key", "phrase"].contains(field.id) || field.value.contains("\n") {
                    TextEditor(text: valueBinding).font(.body.monospaced()).frame(minHeight: 110)
                        .focused(focus, equals: .value(field.id)).accessibilityLabel(label).accessibilityIdentifier("vault.edit.field.\(field.id)")
                } else {
                    TextField(placeholder, text: valueBinding, axis: .vertical).keyboardType(keyboard)
                        .focused(focus, equals: .value(field.id))
                        .accessibilityLabel(label).accessibilityIdentifier("vault.edit.field.\(field.id)")
                }
                if (valueVisited || validationRequested), let error = vaultFieldValueError(field, existing: existing) {
                    Text(error).font(.caption).foregroundStyle(.red).accessibilityIdentifier("vault.edit.field-error.\(field.id)")
                }
            }
            if field.concealed {
                let layout = textSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8)) : AnyLayout(HStackLayout(spacing: 12))
                layout {
                    if !field.value.isEmpty { Button(showingDraft ? "Hide" : "Show draft") { showingDraft.toggle() }.frame(minHeight: 44) }
                    if generates { Button("Generate password") { tool = .generator }.frame(minHeight: 44) }
                    if field.kind == "otp" {
                        Button("Scan QR code", systemImage: "qrcode.viewfinder") { requestingCamera = true }
                            .frame(minHeight: 44)
                            .disabled(!DataScannerViewController.isSupported || requestingCamera)
                    }
                }.font(.footnote).buttonStyle(.borderless)
                if existing && field.changed {
                    Text("An empty replacement clears the saved value.").font(.caption).foregroundStyle(.secondary)
                    Button("Keep existing value") { field.value = ""; field.changed = false }.font(.footnote)
                }
            }
            if ["date", "month"].contains(field.kind) {
                Text(verbatim: field.kind == "date" ? "YYYY-MM-DD" : "YYYY-MM").font(.caption).foregroundStyle(.secondary)
                Button(field.kind == "date" ? "Date" : "Month", systemImage: "calendar") { tool = .date }
                    .accessibilityIdentifier("vault.edit.calendar.\(field.id)")
                if !field.value.isEmpty { Button("Clear") { field.value = ""; field.changed = true } }
            }
            if field.kind == "otp" { Text("Enter a setup key or otpauth URL.").font(.caption).foregroundStyle(.secondary) }
            if cameraDenied {
                Text("Camera access is unavailable. Enter a setup key or allow camera access in Settings.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            DisclosureGroup(isExpanded: $sectionExpanded) {
                TextField("Section", text: $field.section).focused(focus, equals: .section(field.id))
                    .accessibilityIdentifier("vault.edit.section.\(field.id)")
                    .submitLabel(.done).onSubmit { focus.wrappedValue = nil }
            } label: {
                HStack {
                    Text("Section")
                    Spacer()
                    Text(field.section.isEmpty ? String(localized: "No section") : field.section).foregroundStyle(.secondary)
                }
            }.font(.subheadline)
        }.textInputAutocapitalization(.never).autocorrectionDisabled()
            .padding(.vertical, 6)
            .onAppear {
                if addedField == field.id { focus.wrappedValue = .name(field.id); addedField = nil }
            }
            .onChange(of: focus.wrappedValue) { previous, _ in
                if previous == .name(field.id) { nameVisited = true }
                if previous == .value(field.id) { valueVisited = true }
            }
            .task(id: requestingCamera) {
                guard requestingCamera else { return }
                defer { requestingCamera = false }
                let allowed = await AVCaptureDevice.requestAccess(for: .video)
                guard !Task.isCancelled else { return }
                cameraDenied = !allowed || !DataScannerViewController.isAvailable
                if !cameraDenied { tool = .scanner }
            }
            .sheet(item: $tool) { tool in
                NavigationStack {
                    switch tool {
                    case .scanner:
                        QRCodeScanner { value in
                            field.value = value.trimmingCharacters(in: .whitespacesAndNewlines); field.changed = true; self.tool = nil
                        }.navigationTitle("One-time code")
                            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { self.tool = nil } } }
                    case .generator:
                        VaultPasswordGenerator { value in field.value = value; field.changed = true; self.tool = nil }
                    case .date:
                        VaultDatePicker(kind: field.kind, value: field.value) { value in field.value = value; field.changed = true; self.tool = nil }
                    }
                }
            }
    }

    private var valueBinding: Binding<String> {
        Binding(get: { field.value }, set: { field.value = $0; field.changed = true })
    }

    private var keyboard: UIKeyboardType {
        switch field.kind {
        case "url": .URL
        case "email": .emailAddress
        case "phone": .phonePad
        case "date", "month": .numbersAndPunctuation
        default: .default
        }
    }

}

func vaultTags(_ text: String) -> [String] {
    text.split(whereSeparator: { $0 == "," || $0 == "，" || $0 == "،" }).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
}

/// Tags as removable chips; typing a comma or pressing return adds one.
struct VaultTagEditor: View {
    @Binding var tags: String
    @Binding var entry: String
    private var list: [String] { vaultTags(tags) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Tags").font(.caption).foregroundStyle(.secondary)
            if !list.isEmpty {
                FlowLayout(spacing: 6) {
                    ForEach(Array(list.enumerated()), id: \.offset) { index, tag in
                        Button { remove(at: index) } label: {
                            HStack(spacing: 4) {
                                Text(tag).font(.subheadline).lineLimit(1)
                                Image(systemName: "xmark").font(.caption2).accessibilityHidden(true)
                            }.padding(.horizontal, 10).frame(minHeight: 32)
                                .foregroundStyle(Theme.forest).background(Theme.forest.opacity(0.1), in: Capsule())
                        }.buttonStyle(.borderless).accessibilityLabel(Text("Remove") + Text(verbatim: " · " + tag))
                    }
                }
            }
            TextField("Tags, separated by commas", text: $entry).textInputAutocapitalization(.never).autocorrectionDisabled()
                .submitLabel(.done).onSubmit(commit).accessibilityIdentifier("vault.edit.tags")
                .onChange(of: entry) { _, value in if value.contains(where: { ",，،".contains($0) }) { commit() } }
        }
    }

    private func commit() {
        tags = (list + vaultTags(entry)).joined(separator: ", ")
        entry = ""
    }

    private func remove(at index: Int) {
        var next = list
        next.remove(at: index)
        tags = next.joined(separator: ", ")
    }
}

/// Lays its views out in rows, wrapping where they no longer fit.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(width: proposal.width ?? .infinity, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for (subview, origin) in zip(subviews, arrange(width: bounds.width, subviews: subviews).origins) {
            subview.place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), proposal: .unspecified)
        }
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> (origins: [CGPoint], size: CGSize) {
        var origins: [CGPoint] = [], x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0, widest: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0 && x + size.width > width { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            origins.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height); widest = max(widest, x - spacing)
        }
        return (origins, CGSize(width: widest, height: y + rowHeight))
    }
}

private struct VaultDatePicker: View {
    let kind: String
    let value: String
    let use: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var selected = Date.now
    var body: some View {
        Form {
            DatePicker(kind == "date" ? "Date" : "Month", selection: $selected, displayedComponents: .date).datePickerStyle(.graphical)
        }.environment(\.timeZone, TimeZone(secondsFromGMT: 0)!)
            .environment(\.calendar, Calendar(identifier: .gregorian))
            .privateScreen().navigationTitle(kind == "date" ? "Date" : "Month")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { use(vaultDateFormatter(kind).string(from: selected)) } }
            }
            .onAppear {
                var calendar = Calendar(identifier: .gregorian)
                let now = Date.now
                let day = calendar.dateComponents([.year, .month, .day], from: now)
                calendar.timeZone = TimeZone(secondsFromGMT: 0)!
                selected = vaultDateFormatter(kind).date(from: value) ?? calendar.date(from: day) ?? now
            }
    }
}

func vaultDateFormatter(_ kind: String) -> DateFormatter {
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.timeZone = TimeZone(secondsFromGMT: 0)
    formatter.dateFormat = kind == "month" ? "yyyy-MM" : "yyyy-MM-dd"; formatter.isLenient = false
    return formatter
}

func vaultInputError(_ field: VaultDraft.Field, existing: Bool) -> String? {
    vaultFieldNameError(field) ?? vaultFieldValueError(field, existing: existing)
}

private func vaultFieldNameError(_ field: VaultDraft.Field) -> String? {
    field.id.hasPrefix("field_") && field.label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? String(localized: "Field name is required.") : nil
}

private func vaultFieldValueError(_ field: VaultDraft.Field, existing: Bool) -> String? {
    if existing && !field.changed || field.value.isEmpty { return nil }
    let value = field.value.trimmingCharacters(in: .whitespacesAndNewlines)
    switch field.kind {
    case "otp": if !validateVaultOtp(value: value) { return String(localized: "Invalid setup key or otpauth URL.") }
    case "date", "month":
        let formatter = vaultDateFormatter(field.kind)
        if let date = formatter.date(from: value), formatter.string(from: date) == value {} else { return String(localized: "Enter a valid date.") }
    case "email": if !value.contains("@") || value.hasPrefix("@") || value.hasSuffix("@") { return String(localized: "Enter a valid email address.") }
    case "url":
        let candidate = value.contains("://") ? value : "https://" + value
        if let url = URLComponents(string: candidate), let host = url.host, !host.isEmpty,
           ["http", "https"].contains(url.scheme?.lowercased() ?? "") {} else { return String(localized: "Enter a valid website address.") }
    default: break
    }
    return nil
}

/// Swift's system generator uses the platform's cryptographically secure source.
func makeVaultPassword(length: Int, symbols: Bool) -> String {
    var random = SystemRandomNumberGenerator()
    let groups = [Array("ABCDEFGHJKLMNPQRSTUVWXYZ"), Array("abcdefghijkmnopqrstuvwxyz"), Array("23456789")]
        + (symbols ? [Array("!@#$%&*-_+=?")] : [])
    let alphabet = groups.flatMap { $0 }
    var result = groups.map { $0.randomElement(using: &random)! }
    while result.count < max(12, min(length, 64)) { result.append(alphabet.randomElement(using: &random)!) }
    result.shuffle(using: &random)
    return String(result)
}

private struct VaultPasswordGenerator: View {
    let use: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var length = 20
    @State private var symbols = true
    @State private var value = ""
    var body: some View {
        Form {
            Text(value).font(.body.monospaced()).environment(\.layoutDirection, .leftToRight)
                .accessibilityIdentifier("vault.generator.value")
            Stepper(value: $length, in: 12...64) { LabeledContent("Length", value: length.formatted()) }
            Toggle("Symbols", isOn: $symbols)
            Button("Generate password") { generate() }
            Button("Use password") { use(value) }.disabled(value.isEmpty)
        }.privateScreen().navigationTitle("Generate password").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .onAppear { generate() }.onChange(of: length) { _, _ in generate() }.onChange(of: symbols) { _, _ in generate() }
            .onDisappear { value = "" }
    }
    private func generate() { value = makeVaultPassword(length: length, symbols: symbols) }
}
