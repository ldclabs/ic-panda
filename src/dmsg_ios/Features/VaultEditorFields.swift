import SwiftUI
import VisionKit
@preconcurrency import DMsgCore

struct VaultEditorField: View {
    @Binding var field: VaultDraft.Field
    let existing: Bool
    @State private var showingDraft = false
    private enum Tool: String, Identifiable { case scanner, generator, date; var id: Self { self } }
    @State private var tool: Tool?
    @State private var settings = false
    private var label: String { field.label.isEmpty ? (field.kind == "otp" ? String(localized: "One-time code") : vaultFieldName(field.id)) : field.label }
    private var keeping: Bool { existing && field.concealed && !field.changed }
    private var placeholder: LocalizedStringKey { keeping ? "Leave empty to keep existing value" : "Value" }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(label).font(.caption).foregroundStyle(.secondary)
            if field.concealed && !showingDraft {
                SecureField(placeholder, text: valueBinding).accessibilityLabel(label).accessibilityIdentifier("vault.edit.field.\(field.id)")
            } else if ["private_key", "public_key", "phrase"].contains(field.id) || field.value.contains("\n") {
                TextEditor(text: valueBinding).font(.body.monospaced()).frame(minHeight: 110).accessibilityLabel(label)
            } else {
                TextField(placeholder, text: valueBinding, axis: .vertical).keyboardType(keyboard)
                    .accessibilityLabel(label).accessibilityIdentifier("vault.edit.field.\(field.id)")
            }
            if field.concealed {
                HStack {
                    if !field.value.isEmpty { Button(showingDraft ? "Hide" : "Show draft") { showingDraft.toggle() } }
                    if field.id == "password" { Button("Generate password") { tool = .generator } }
                    if field.kind == "otp" {
                        Button("Scan QR code", systemImage: "qrcode.viewfinder") { tool = .scanner }
                            .disabled(!DataScannerViewController.isSupported || !DataScannerViewController.isAvailable)
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
            if let error = vaultInputError(field, existing: existing) { Text(error).font(.caption).foregroundStyle(.red).accessibilityIdentifier("vault.edit.field-error.\(field.id)") }
            DisclosureGroup("Field settings", isExpanded: $settings) {
                LabeledContent("Field name") { TextField("Field name", text: $field.label).multilineTextAlignment(.trailing) }
                LabeledContent("Section") { TextField("Section", text: $field.section).multilineTextAlignment(.trailing) }
            }
        }.textInputAutocapitalization(.never).autocorrectionDisabled()
            .onAppear { if field.id.hasPrefix("field_") && field.label.isEmpty { settings = true } }
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
    if field.id.hasPrefix("field_") && field.label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return String(localized: "Field name is required.") }
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
