import SwiftUI

/// Public, offline product tour. It has no SessionModel/Core reference and
/// cannot turn a sample action into authentication, a purchase or a signature.
struct LocalDemoView: View {
    struct Sample: Identifiable { let id = UUID(); var title: String; var username: String; var password: String }
    @State private var samples = [Sample(title: String(localized: "Example login"), username: "reviewer@example.invalid", password: "Sample-only-42!")]
    @State private var messages = [String(localized: "Welcome to the local demo. These messages stay in this screen.")]
    @State private var draft = ""
    @State private var tab = 0
    @State private var decision: String?
    @State private var blocked = false
    @State private var reportPreview = false
    @State private var confirming = false
    var body: some View {
        VStack(spacing: 0) {
            Text("Local demo · Fictitious data · Resets when closed").font(.caption.weight(.semibold)).padding(12).frame(maxWidth: .infinity).background(Theme.forest.opacity(0.1))
            Picker("Demo section", selection: $tab) { Text("Vault").tag(0); Text("Messages").tag(1); Text("Files").tag(2); Text("Approvals").tag(3) }.pickerStyle(.segmented).padding()
            List {
                if tab == 0 {
                    ForEach($samples) { $sample in
                        NavigationLink { DemoLoginEditor(sample: $sample) } label: {
                            VStack(alignment: .leading, spacing: 4) { Text(sample.title).font(.headline); Text(sample.username).font(.caption).foregroundStyle(.secondary) }
                        }.accessibilityIdentifier("demo.login")
                    }.onDelete { samples.remove(atOffsets: $0) }
                    Button("Add sample login") { samples.append(Sample(title: String(localized: "Example login"), username: "", password: "")) }
                } else if tab == 1 {
                    if !blocked { ForEach(Array(messages.enumerated()), id: \.offset) { _, text in Text(text).textSelection(.enabled) } }
                    TextField("Write a demo message", text: $draft, axis: .vertical)
                    Button("Add message to demo") { messages.append(draft); draft = "" }.disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Toggle("Hide sample sender", isOn: $blocked)
                    Button("Preview reporting disclosure") { reportPreview = true }
                } else if tab == 2 {
                    NavigationLink { ScrollView { Text("This is a fictitious document for the local tour. In your account, selected files are encrypted on the device before backup. Background transfers handle only the encrypted blocks; finishing requires the original unlocked account.").padding() }.navigationTitle("Example document") } label: { Label("Example document", glyph: .file) }
                    Text("This tour does not upload files or register system integrations.").font(.footnote).foregroundStyle(.secondary)
                } else {
                    LabeledContent("Request source", value: "https://example.invalid")
                    Text("Sample action: sign a release announcement").font(.headline)
                    Text("Release 1.0 is ready for review.").textSelection(.enabled)
                    if let decision { Text(decision).foregroundStyle(Theme.forest) }
                    else {
                        Button("Simulate approval") { confirming = true }
                        Button("Simulate rejection", role: .destructive) { decision = String(localized: "Demo rejected · No signature created") }
                    }
                    Text("Live requests show their verified origin and full payload and require the configured system or hardware approval.").font(.footnote).foregroundStyle(.secondary)
                }
                Section {
                    Text("The local tour is available to everyone. To test account creation, synchronization, purchases, notifications and hardware authorization, leave the tour and sign in through Internet Identity.").font(.footnote).foregroundStyle(.secondary)
                    NavigationLink("Privacy policy") { PrivacyPolicyView() }
                    Link("Contact support", destination: URL(string: "https://dmsg.net/_/support")!)
                }
            }
        }.navigationTitle("Local demo").navigationBarTitleDisplayMode(.inline)
            .confirmationDialog("Simulate this approval?", isPresented: $confirming) {
                Button("Simulate approval") { decision = String(localized: "Demo approved · No signature created") }
            } message: { Text("This changes only the local sample.") }
            .sheet(isPresented: $reportPreview) { NavigationStack { List {
                Text("In a real report, the service receives your reason and the account/resource identifiers. Message text is included only if you explicitly choose to disclose it.")
                Text("This preview sends nothing.").foregroundStyle(.secondary)
            }.navigationTitle("Reporting disclosure").toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { reportPreview = false } } } } }
    }
}

private struct DemoLoginEditor: View {
    @Binding var sample: LocalDemoView.Sample
    @State private var reveal = false
    var body: some View {
        Form {
            TextField("Title", text: $sample.title)
            TextField("Username", text: $sample.username).textInputAutocapitalization(.never).autocorrectionDisabled()
            if reveal { TextField("Sample password", text: $sample.password).textInputAutocapitalization(.never).autocorrectionDisabled() }
            else { SecureField("Sample password", text: $sample.password) }
            Toggle("Show sample password", isOn: $reveal)
            Text("Use fictitious data here. Closing the tour discards your changes.").font(.footnote).foregroundStyle(.secondary)
        }.navigationTitle("Sample login")
    }
}
