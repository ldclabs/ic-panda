import SwiftUI
@preconcurrency import DMsgCore

struct AgentsView: View {
    @Environment(SessionModel.self) private var model
    @State private var agents: [Agent] = []
    @State private var label = ""
    @State private var working = false
    struct Agent: Decodable, Identifiable { let label: String; let agentId: String; let paused: Bool; var id: String { agentId } }
    struct Listing: Decodable { let agents: [Agent] }
    var body: some View {
        List {
            NoticeBar(text: String(localized: "This iPhone manages agent identities and permissions. It does not keep agents running while iOS suspends the app."))
            ForEach(agents) { agent in NavigationLink(agent.label) { AgentDetailView(agent: agent) } }
            Section("Create an agent identity") {
                TextField("Local label", text: $label).textInputAutocapitalization(.never).autocorrectionDisabled()
                Button("Create agent") { Task {
                    guard !working else { return }; working = true; defer { working = false }
                    do { _ = try await model.engine?.createAgent(label: label); label = ""; await load() }
                    catch { model.report(error) }
                } }.disabled(working || label.isEmpty)
            }
            NavigationLink("Controllers and delegations") { AgentDelegationsView() }
        }.privateScreen().navigationTitle("Agents").task { await load() }.refreshable { await load() }
    }
    private func load() async {
        do { if let value = try await model.engine?.agents() { let values: Listing = try model.decode(value); agents = values.agents } }
        catch { model.report(error) }
    }
}

struct AgentDetailView: View {
    let agent: AgentsView.Agent
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var paused = false
    @State private var capabilities = ""
    @State private var days: UInt64 = 7
    @State private var grants: [Grant] = []
    @State private var request: ApprovalView?
    @State private var removing = false
    @State private var working = false
    struct Grant: Decodable, Identifiable { let id: String; let grantee: String; let capabilities: [String]; let revokedAt: UInt64? }
    var body: some View {
        Form {
            FingerprintRow(title: "Agent ID", value: agent.agentId)
            Toggle("Pause this agent", isOn: Binding(get: { paused }, set: { value in Task {
                guard !working else { return }; working = true; defer { working = false }
                do { _ = try await model.engine?.pauseAgent(label: agent.label, paused: value); paused = value } catch { model.report(error) }
            } })).disabled(working)
            Section("Device permissions") {
                ForEach(grants.filter { $0.grantee == agent.agentId && $0.revokedAt == nil }) { grant in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(grant.capabilities.joined(separator: "\n")).font(.caption.monospaced())
                        Button("Revoke grant", role: .destructive) { Task {
                            do { _ = try await model.engine?.revokeAgentPermissions(id: grant.id); await load() } catch { model.report(error) }
                        } }
                    }
                }
                TextField("Capabilities, one per line", text: $capabilities, axis: .vertical).lineLimit(3...8).textInputAutocapitalization(.never).autocorrectionDisabled()
                Stepper("\(days) days", value: $days, in: 1...366)
                Button("Review permissions") { Task {
                    do { if let value = try await model.engine?.requestAgentPermissions(label: agent.label, capabilities: lines(capabilities), days: days) { request = try model.decode(value) } }
                    catch { model.report(error) }
                } }.disabled(capabilities.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            Button("Remove this local agent", role: .destructive) { removing = true }
        }.privateScreen().navigationTitle(agent.label).onAppear { paused = agent.paused }.task { await load() }
            .confirmationDialog("Remove this local agent?", isPresented: $removing) {
                Button("Remove", role: .destructive) { Task {
                    do { _ = try await model.engine?.removeAgent(label: agent.label); dismiss() } catch { model.report(error) }
                } }
            } message: { Text("Previously delivered credentials and copies are independent. Revoke delegations separately where required.") }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }
    private func load() async { do { if let value = try await model.engine?.agentGrants() { grants = try model.decode(value) } } catch { model.report(error) } }
}

struct AgentDelegationsView: View {
    @Environment(SessionModel.self) private var model
    @State private var principal: Principal?
    @State private var credentials: DisplayValue?
    @State private var request: ApprovalView?
    @State private var name = ""
    @State private var scopes = ""
    @State private var audiences = ""
    @State private var unrestricted = false
    @State private var subject = ""
    @State private var revoking = ""
    @State private var days: UInt64 = 7
    @State private var working = false
    struct Principal: Decodable {
        struct Controller: Decodable, Identifiable { let generation: UInt32; let agentId: String; let name: String?; let retiredAt: UInt64?; var id: UInt32 { generation } }
        let principalId: String; let controllers: [Controller]
    }
    var body: some View {
        Form {
            if let principal {
                FingerprintRow(title: "Principal", value: principal.principalId)
                Section("Controllers") {
                    ForEach(principal.controllers) { controller in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(controller.name ?? String(controller.generation))
                            FingerprintRow(title: "Controller ID", value: controller.agentId)
                            if controller.retiredAt == nil { Button("Review retiring controller", role: .destructive) { Task { await action { try await $0.retireAgentController(generation: controller.generation) } } } }
                        }
                    }
                }
                Section("New controller") {
                    TextField("Controller name", text: $name)
                    Toggle("Unrestricted delegation authority", isOn: $unrestricted)
                    TextField("Scopes, one per line", text: $scopes, axis: .vertical).lineLimit(2...6).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("Audience origins or Agent IDs, one per line", text: $audiences, axis: .vertical).lineLimit(2...6).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Review controller registration") { Task { await action { try await $0.registerAgentController(name: name, scopes: lines(scopes), audiences: lines(audiences), unrestricted: unrestricted) } } }.disabled(name.isEmpty)
                }
                Section("Grant delegation") {
                    TextField("Subject Agent ID", text: $subject).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Text("The explicit scopes and audiences above also apply to this credential. Unrestricted controller authority never makes a credential unrestricted.").font(.footnote).foregroundStyle(.secondary)
                    Stepper("\(days) days", value: $days, in: 1...366)
                    Button("Review delegation grant") { Task { await action { try await $0.grantAgentDelegation(subject: subject, scopes: lines(scopes), audiences: lines(audiences), days: days) } } }.disabled(subject.isEmpty || scopes.isEmpty || audiences.isEmpty)
                }
                if let credentials { DisclosureGroup("Issued credentials") { CanonicalDetails(value: credentials) } }
                Section("Revoke delegation") {
                    TextField("Credential ID", text: $revoking).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Review revocation", role: .destructive) { Task { await action { try await $0.revokeAgentDelegation(id: revoking, reason: nil) } } }.disabled(revoking.isEmpty)
                }
            } else {
                Button("Review enabling a principal") { Task { await action { try await $0.enableAgentPrincipal() } } }
            }
        }.disabled(working).privateScreen().navigationTitle("Delegations").task { await load() }.refreshable { await load() }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }
    private func load() async {
        do {
            if let value = try await model.engine?.agentPrincipal() { principal = try model.decode(value) }
            if principal != nil, let value = try await model.engine?.agentDelegations() { credentials = try model.decode(value) }
        } catch { model.report(error) }
    }
    private func action(_ work: @MainActor (DMsgCore.MobileEngine) async throws -> DMsgCore.ObjectSnapshot) async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }; model.activity()
        do { request = try model.decode(try await work(engine)) } catch { model.report(error) }
    }
}

private func lines(_ text: String) -> [String] { text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty } }
