import SwiftUI
@preconcurrency import DMsgCore

struct AgentsView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var agents: [Agent] = []
    @State private var label = ""
    @State private var working = false
    @State private var loading = false
    @State private var failed = false
    struct Agent: Decodable, Identifiable { let label: String; let agentId: String; let paused: Bool; var id: String { agentId } }
    struct Listing: Decodable { let agents: [Agent] }
    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            if !loading && !failed && agents.isEmpty { ContentUnavailableView("No agents", image: AppGlyph.agents.assetName) }
            NoticeBar(text: String(localized: "This iPhone manages agent identities and permissions. It does not keep agents running while iOS suspends the app."))
            ForEach(agents) { agent in NavigationLink(agent.label) { AgentDetailView(agent: agent) } }
            Section("Create an agent identity") {
                SettingsField("Local label") { TextField("Local label", text: $label).textInputAutocapitalization(.never).autocorrectionDisabled() }
                SettingsAction("Create agent", prominent: true) { Task {
                    guard let engine = model.engine, !working else { return }
                    let epoch = model.epoch; working = true; defer { working = false }
                    do { _ = try await engine.createAgent(label: label.trimmingCharacters(in: .whitespacesAndNewlines)); if epoch == model.epoch && visible { label = ""; await load() } }
                    catch { if epoch == model.epoch && visible { model.report(error) } }
                } }.disabled(working || label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            NavigationLink("Controllers and delegations") { AgentDelegationsView() }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Agents").task { await load() }.refreshable { await load() }
    }
    private func load() async {
        guard !loading else { return }; loading = true; failed = false; defer { loading = false }
        let epoch = model.epoch
        do { if let value = try await model.engine?.agents(), epoch == model.epoch && visible, !Task.isCancelled { let values: Listing = try model.decode(value); agents = values.agents } }
        catch { if epoch == model.epoch && visible, !Task.isCancelled { failed = true } }
    }
}

struct AgentDetailView: View {
    let agent: AgentsView.Agent
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @Environment(\.dismiss) private var dismiss
    @State private var paused = false
    @State private var capabilities = ""
    @State private var days: UInt64 = 7
    @State private var grants: [Grant] = []
    @State private var request: ApprovalView?
    @State private var removing = false
    @State private var working = false
    @State private var loaded = false
    @State private var failed = false
    @State private var scope: UInt64 = 0
    struct Draft: Codable { let capabilities: String; let days: UInt64 }
    struct Grant: Decodable, Identifiable { let id: String; let grantee: String; let capabilities: [String]; let revokedAt: UInt64? }
    var body: some View {
        Form {
            SettingsLoadStatus(loading: !loaded && !failed, failed: failed) { Task { await load() } }
            FingerprintRow(title: "Agent ID", value: agent.agentId)
            Toggle("Pause this agent", isOn: Binding(get: { paused }, set: { value in Task {
                if await perform({ try await $0.pauseAgent(label: agent.label, paused: value) }) != nil { paused = value }
            } })).disabled(working)
            Section("Device permissions") {
                ForEach(grants.filter { $0.grantee == agent.agentId && $0.revokedAt == nil }) { grant in
                    VStack(alignment: .leading, spacing: 6) {
                        Text(grant.capabilities.joined(separator: "\n")).font(.caption.monospaced())
                        Button("Revoke grant", role: .destructive) { Task {
                            if await perform({ try await $0.revokeAgentPermissions(id: grant.id) }) != nil { await load() }
                        } }
                    }
                }
                SettingsField("Capabilities, one per line") { TextField("Capabilities, one per line", text: $capabilities, axis: .vertical).lineLimit(3...8).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                Stepper("\(days) days", value: $days, in: 1...366)
                SettingsAction("Review permissions", prominent: true) { Task {
                    if let value = await perform({ try await $0.requestAgentPermissions(label: agent.label, capabilities: lines(capabilities), days: days) }) {
                        do { request = try model.decode(value) } catch { model.report(error) }
                    }
                } }.disabled(capabilities.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            SettingsAction("Remove this local agent", role: .destructive) { removing = true }
        }.disabled(working || !loaded).privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle(agent.label).confirmLeavingDraft(!capabilities.isEmpty)
            .task {
                scope = model.viewGeneration
                if !loaded { paused = agent.paused }
                if let draft = model.formDraft("agent:" + agent.id, as: Draft.self) { capabilities = draft.capabilities; days = draft.days }
                await load()
            }
            .onDisappear { if !capabilities.isEmpty { model.keepFormDraft("agent:" + agent.id, value: Draft(capabilities: capabilities, days: days), generation: scope) } }
            .confirmationDialog("Remove this local agent?", isPresented: $removing) {
                Button("Remove", role: .destructive) { Task {
                    if await perform({ try await $0.removeAgent(label: agent.label) }) != nil { dismiss() }
                } }
            } message: { Text("Previously delivered credentials and copies are independent. Revoke delegations separately where required.") }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }
    private func load() async {
        let epoch = model.epoch; failed = false
        do {
            guard let engine = model.engine else { return }
            let values: [Grant] = try model.decode(try await engine.agentGrants())
            let listing: AgentsView.Listing = try model.decode(try await engine.agents())
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }
            grants = values; paused = listing.agents.first { $0.id == agent.id }?.paused ?? agent.paused; loaded = true
        } catch { if epoch == model.epoch && visible, !Task.isCancelled { failed = true; loaded = true } }
    }

    private func perform(_ operation: @MainActor (MobileEngine) async throws -> ObjectSnapshot) async -> ObjectSnapshot? {
        guard let engine = model.engine, !working else { return nil }
        let epoch = model.epoch; working = true; defer { working = false }
        do {
            let result = try await operation(engine)
            guard epoch == model.epoch && visible else { return nil }; return result
        } catch { if epoch == model.epoch && visible { model.report(error) }; return nil }
    }
}

struct AgentAuthorityDraft: Codable, Equatable {
    var scopes = ""
    var audiences = ""
    var valid: Bool {
        let scopes = lines(scopes), audiences = lines(audiences)
        return !scopes.isEmpty && scopes.count <= 8 && Set(scopes).count == scopes.count && scopes.allSatisfy {
            $0.utf8.count <= 64 && !$0.contains("*") && $0.rangeOfCharacter(from: .whitespacesAndNewlines.union(.controlCharacters)) == nil
        } && !audiences.isEmpty && audiences.count <= 4 && Set(audiences).count == audiences.count && audiences.allSatisfy {
            guard $0.utf8.count <= 256, $0.rangeOfCharacter(from: .whitespacesAndNewlines.union(.controlCharacters)) == nil else { return false }
            if $0.hasPrefix("did:agent:") { return $0.count > 10 }
            guard let url = URLComponents(string: $0) else { return false }
            return url.scheme == "https" && url.host != nil && url.path.isEmpty && url.query == nil && url.fragment == nil && url.user == nil && url.password == nil
        }
    }
}

struct AgentDelegationsView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var principal: Principal?
    @State private var credentials: [Credential] = []
    @State private var request: ApprovalView?
    @State private var draft = Draft()
    @State private var working = false
    @State private var loading = false
    @State private var failed = false
    @State private var scope: UInt64 = 0
    struct Draft: Codable, Equatable {
        var name = ""; var controller = AgentAuthorityDraft(); var grant = AgentAuthorityDraft()
        var unrestricted = false; var subject = ""; var days: UInt64 = 7
    }
    struct Principal: Decodable {
        struct Controller: Decodable, Identifiable { let generation: UInt32; let agentId: String; let name: String?; let retiredAt: UInt64?; var id: UInt32 { generation } }
        let principalId: String; let controllers: [Controller]
    }
    struct Credential: Decodable, Identifiable {
        let id: String; let subject: String; let scopes: [String]; let audiences: [String]
        let status: String; let expiresAt: UInt64?
    }
    private var validName: Bool { !draft.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && draft.name.utf8.count <= 64 && draft.name.rangeOfCharacter(from: .controlCharacters) == nil }

    var body: some View {
        Form {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            if let principal {
                FingerprintRow(title: "Principal", value: principal.principalId)
                Section("Controllers") {
                    ForEach(principal.controllers) { controller in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(controller.name ?? String(controller.generation))
                            FingerprintRow(title: "Controller ID", value: controller.agentId)
                            if controller.retiredAt == nil { Button("Review retiring controller", role: .destructive) { Task { await action { try await $0.retireAgentController(generation: controller.generation) } } } }
                            else { Text("Revoked").font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
                DisclosureGroup("New controller") {
                    SettingsField("Controller name") { TextField("Controller name", text: $draft.name) }
                    Toggle("Unrestricted delegation authority", isOn: $draft.unrestricted)
                    if !draft.unrestricted { AuthorityFields(value: $draft.controller) }
                    SettingsAction("Review controller registration") { Task {
                        await action { try await $0.registerAgentController(name: draft.name.trimmingCharacters(in: .whitespacesAndNewlines), scopes: draft.unrestricted ? [] : lines(draft.controller.scopes), audiences: draft.unrestricted ? [] : lines(draft.controller.audiences), unrestricted: draft.unrestricted) }
                    } }.disabled(!validName || !draft.unrestricted && !draft.controller.valid)
                }
                Section("Grant delegation") {
                    SettingsField("Subject Agent ID") { TextField("Subject Agent ID", text: $draft.subject).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                    AuthorityFields(value: $draft.grant)
                    Stepper("\(draft.days) days", value: $draft.days, in: 1...366)
                    SettingsAction("Review delegation grant", prominent: true) { Task { await action {
                        try await $0.grantAgentDelegation(subject: draft.subject.trimmingCharacters(in: .whitespacesAndNewlines), scopes: lines(draft.grant.scopes), audiences: lines(draft.grant.audiences), days: draft.days)
                    } } }.disabled(draft.subject.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !draft.grant.valid)
                }
                Section("Issued credentials") {
                    ForEach(credentials) { credential in
                        VStack(alignment: .leading, spacing: 8) {
                            FingerprintRow(title: "Subject Agent ID", value: credential.subject)
                            Text(credentialStateTitle(credential.status)).font(.subheadline)
                            Text(credential.scopes.joined(separator: ", ")).font(.caption)
                            Text(credential.audiences.joined(separator: "\n")).font(.caption).environment(\.layoutDirection, .leftToRight)
                            if let expiry = credential.expiresAt { LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(expiry) / 1000), style: .date) } }
                            DisclosureGroup("Details") { FingerprintRow(title: "Credential ID", value: credential.id) }
                            if credential.status != "revoked" { Button("Review revocation", role: .destructive) { Task { await action { try await $0.revokeAgentDelegation(id: credential.id, reason: nil) } } } }
                        }
                    }
                    if credentials.isEmpty && !loading && !failed { Text("No credentials").foregroundStyle(.secondary) }
                }
            } else if !loading && !failed {
                SettingsAction("Review enabling a principal", prominent: true) { Task { await action { try await $0.enableAgentPrincipal() } } }
            }
            if working { ProgressView() }
        }.disabled(working).privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Delegations").confirmLeavingDraft(draft != Draft())
            .task(id: model.contentRevision) {
                scope = model.viewGeneration
                if let restored = model.formDraft("agent-delegation", as: Draft.self) {
                    draft = restored
                    model.settingsDrafts.removeValue(forKey: "agent-delegation")
                }
                await load()
            }.refreshable { await load() }
            .onDisappear { if draft != Draft() { model.keepFormDraft("agent-delegation", value: draft, generation: scope) } }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }

    private func load() async {
        guard let engine = model.engine, !loading else { return }
        let epoch = model.epoch; loading = true; failed = false; defer { loading = false }
        do {
            let value = try await engine.agentPrincipal()
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }
            principal = try model.decode(value) as Principal?
            if principal != nil {
                let values: [Credential] = try model.decode(try await engine.agentDelegations())
                guard epoch == model.epoch && visible, !Task.isCancelled else { return }; credentials = values
            }
        } catch { if epoch == model.epoch && visible, !Task.isCancelled { failed = true } }
    }

    private func action(_ operation: @MainActor (DMsgCore.MobileEngine) async throws -> DMsgCore.ObjectSnapshot) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }; model.activity()
        do {
            let result = try await operation(engine)
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }; request = try model.decode(result)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }
}

private struct AuthorityFields: View {
    @Binding var value: AgentAuthorityDraft
    var body: some View {
        SettingsField("Scopes, one per line") { TextField("Scopes, one per line", text: $value.scopes, axis: .vertical).lineLimit(2...6).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
        SettingsField("Audience origins or Agent IDs, one per line") { TextField("Audience origins or Agent IDs, one per line", text: $value.audiences, axis: .vertical).lineLimit(2...6).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
        Text("Up to 8 scopes and 4 audiences. Use HTTPS origins or Agent IDs; wildcards are not allowed.").font(.caption).foregroundStyle(value.valid ? Color.secondary : Color.orange)
    }
}

func credentialStateTitle(_ status: String) -> String {
    switch status {
    case "active": String(localized: "Active")
    case "expired": String(localized: "Expired")
    case "revoked": String(localized: "Revoked")
    case "suspended": String(localized: "Suspended")
    default: status
    }
}

private func lines(_ text: String) -> [String] { text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty } }
