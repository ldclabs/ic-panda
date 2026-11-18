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
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-agent-grants") {
            agents = [Agent(label: "Studio assistant", agentId: "did:agent:studio.test", paused: false)]
            return
        }
        #endif
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
    @State private var pattern = ""
    @State private var target = ""
    @State private var folders: [FolderItem] = []
    @State private var choosingFolder = false
    @State private var request: ApprovalView?
    @State private var removing = false
    @State private var revoking: Grant?
    @State private var working = false
    @State private var loaded = false
    @State private var failed = false
    @State private var scope: UInt64 = 0
    struct Draft: Codable { let capabilities: String; let days: UInt64; let pattern: String; let target: String }
    struct Grant: Decodable, Identifiable { let id: String; let grantee: String; let capabilities: [String]; let revokedAt: UInt64?; let expiresAt: UInt64? }
    private var hasDraft: Bool { !capabilities.isEmpty || !pattern.isEmpty || days != 7 }

    var body: some View {
        Form {
            SettingsLoadStatus(loading: !loaded && !failed, failed: failed) { Task { await load() } }
            FingerprintRow(title: "Agent ID", value: agent.agentId)
            Toggle("Pause this agent", isOn: Binding(get: { paused }, set: { value in Task {
                if await perform({ try await $0.pauseAgent(label: agent.label, paused: value) }) != nil { paused = value }
            } })).disabled(working)
            Section("Device permissions") {
                Text("Content you allow an external AI client to read is also handled under that client’s data policies. Grant access only to what the task needs.")
                    .font(.footnote).foregroundStyle(.secondary)
                ForEach(grants.filter { $0.grantee == agent.agentId }) { grant in
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(grant.capabilities, id: \.self) { item in Text(permissionDescription(item)) }
                        if grant.revokedAt != nil { Text("Revoked").foregroundStyle(.secondary) }
                        else {
                            if let expiry = grant.expiresAt {
                                TimelineView(.periodic(from: .now, by: 60)) { time in
                                    LabeledContent(fileDate(expiry) <= time.date ? "Expired" : "Expires") { Text(fileDate(expiry), format: .dateTime) }
                                }.font(.footnote)
                            }
                            Button("Revoke grant", role: .destructive) { revoking = grant }
                        }
                    }.buttonStyle(.borderless)
                }
                Picker("Permissions", selection: Binding(get: { pattern }, set: { pattern = $0; target = "" })) {
                    Text("Choose a permission").tag("")
                    ForEach(AgentPermissionName.options, id: \.self) { Text(AgentPermissionName.title($0)).tag($0) }
                }.accessibilityIdentifier("agent.permission.picker")
                if let key = AgentPermissionName.target(pattern) {
                    SettingsField(key) {
                        TextField(key, text: $target).textInputAutocapitalization(.never).autocorrectionDisabled()
                            .environment(\.layoutDirection, .leftToRight).accessibilityIdentifier("agent.permission.target")
                    }
                    if pattern.contains("<channel>") && !model.channels.isEmpty {
                        Menu("Conversations") { ForEach(model.channels, id: \.id) { c in Button(c.name) { target = c.id } } }
                    } else if pattern.contains("<tag>") && !model.vaultCounts.tags.isEmpty {
                        Menu("Tags") { ForEach(model.vaultCounts.tags, id: \.self) { tag in Button(tag) { target = tag } } }
                    } else if pattern.contains("<folder>") {
                        SettingsAction("Choose folder", systemImage: "folder") { choosingFolder = true }
                            .accessibilityIdentifier("agent.permission.folder")
                    }
                }
                Button("Add permission") {
                    guard let candidate else { return }
                    capabilities = (lines(capabilities) + [candidate]).joined(separator: "\n"); target = ""; pattern = ""
                }.disabled(candidate == nil).accessibilityIdentifier("agent.permission.add")
                ForEach(lines(capabilities), id: \.self) { value in
                    HStack {
                        Text(permissionDescription(value)).frame(maxWidth: .infinity, alignment: .leading)
                        Button("Remove", systemImage: "minus.circle", role: .destructive) {
                            capabilities = lines(capabilities).filter { $0 != value }.joined(separator: "\n")
                        }.labelStyle(.iconOnly)
                    }
                }
                Stepper("\(days) days", value: $days, in: 1...366)
                SettingsAction("Review permissions", prominent: true) { Task { await reviewPermissions() } }.disabled(lines(capabilities).isEmpty || lines(capabilities).count > 16 || !lines(capabilities).allSatisfy { validAgentCapability(capability: $0) })
                    .accessibilityIdentifier("agent.permission.review")
            }
            SettingsAction("Remove this local agent", role: .destructive) { removing = true }
        }.disabled(working || !loaded).privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle(agent.label).confirmLeavingDraft(hasDraft)
            .task {
                scope = model.viewGeneration
                if !loaded { paused = agent.paused }
                if let draft = model.formDraft("agent:" + agent.id, as: Draft.self) {
                    capabilities = draft.capabilities; days = draft.days; pattern = draft.pattern; target = draft.target
                    model.settingsDrafts.removeValue(forKey: "agent:" + agent.id)
                }
                await load()
            }
            .onDisappear {
                if hasDraft {
                    model.keepFormDraft("agent:" + agent.id, value: Draft(capabilities: capabilities, days: days, pattern: pattern, target: target), generation: scope)
                }
            }
            .sheet(isPresented: $choosingFolder) {
                NavigationStack { FileFolderPicker { id, _ in target = id } }
            }
            .confirmationDialog("Revoke grant", isPresented: Binding(get: { revoking != nil }, set: { if !$0 { revoking = nil } }), presenting: revoking) { grant in
                Button("Revoke grant", role: .destructive) { Task {
                    if await perform({ try await $0.revokeAgentPermissions(id: grant.id) }) != nil { await load() }
                } }
            } message: { grant in
                Text(([agent.label] + grant.capabilities.map { permissionDescription($0) }).joined(separator: "\n"))
            }
            .confirmationDialog("Remove this local agent?", isPresented: $removing) {
                Button("Remove", role: .destructive) { Task {
                    if await perform({ try await $0.removeAgent(label: agent.label) }) != nil { dismiss() }
                } }
            } message: { Text("Previously delivered credentials and copies are independent. Revoke delegations separately where required.") }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }
    private func reviewPermissions() async {
        #if DEBUG
        if model.uiTesting {
            let now = NotificationTime.now
            model.fixtureGrantReview = .object(["grantee": .string(agent.id), "capabilities": .array(lines(capabilities).map(DisplayValue.string)), "days": .number(Decimal(days))])
            request = ApprovalView(id: "test-grant", digest: "test-grant", type: "grant", state: "awaiting_user", source: agent.label,
                                   risk: "high", expiresAt: now + 60_000, mobileSupported: true, updatedAt: now)
            return
        }
        #endif
        if let value = await perform({ try await $0.requestAgentPermissions(label: agent.label, capabilities: lines(capabilities), days: days) }) {
            do { request = try model.decode(value) } catch { model.report(error) }
        }
    }

    private func permissionDescription(_ value: String) -> String {
        let label = AgentPermissionName.describe(value)
        let parts = value.split(separator: ":", maxSplits: 1)
        guard parts.count == 2 else { return label }
        let id = String(parts[1])
        let name: String?
        switch parts[0] {
        case "channel.read", "channel.publish": name = model.channels.first { $0.id == id }?.name
        case "files.read", "files.write": name = id == "root" ? String(localized: "Files") : folders.first { $0.id == id }.map(folderName)
        default: name = nil
        }
        return name.map { label + " (" + $0 + ")" } ?? label
    }

    private func folderName(_ folder: FolderItem) -> String {
        ((folder.path ?? []) + [folder.name]).joined(separator: " / ")
    }

    private var candidate: String? {
        guard AgentPermissionName.options.contains(pattern) else { return nil }
        let target = target.trimmingCharacters(in: .whitespacesAndNewlines)
        let code = pattern.split(separator: ":", maxSplits: 1)[0]
        let value = AgentPermissionName.target(pattern) == nil ? String(code) : String(code) + ":" + target
        let existing = lines(capabilities)
        return validAgentCapability(capability: value) && existing.count < 16 && !existing.contains(value) ? value : nil
    }

    private func load() async {
        let epoch = model.epoch; failed = false
        #if DEBUG
        if model.uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-agent-grants") {
            grants = [Grant(id: "old", grantee: agent.id, capabilities: ["channel.read:studio"], revokedAt: nil, expiresAt: 1)]
            folders = model.fixtureFileFolders
            loaded = true; return
        }
        #endif
        do {
            guard let engine = model.engine else { return }
            let values: [Grant] = try model.decode(try await engine.agentGrants())
            let listing: AgentsView.Listing = try model.decode(try await engine.agents())
            let folders = try? await engine.fileFolders()
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }
            if let folders { self.folders = (try? model.decode(folders)) ?? self.folders }
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
                        }.buttonStyle(.borderless)
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
                        }.buttonStyle(.borderless)
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

/// The shared core supplies the vocabulary. An unfamiliar grant stays visible verbatim.
enum AgentPermissionName {
    static let options = agentCapabilityVocabulary().filter { $0 != "profile.read" }

    static func target(_ pattern: String) -> LocalizedStringKey? {
        switch pattern.split(separator: ":", maxSplits: 1).last {
        case "<channel>": "Channel ID"
        case "<folder>": "Folder"
        case "<tag>": "Tags"
        case "<wallet>": "Wallet canister ID"
        case "<policy>": "Policy ID"
        case "<origin>": "Website"
        default: nil
        }
    }

    static func describe(_ value: String) -> String {
        guard validAgentCapability(capability: value) else { return value }
        let parts = value.split(separator: ":", maxSplits: 1)
        guard let code = parts.first else { return value }
        let label = title(String(code))
        guard label != code else { return value }
        guard parts.count == 2 else { return label }
        let scope = parts[1] == "*" ? String(localized: "All") + " (*)" : String(parts[1])
        return label + " · " + scope
    }

    static func title(_ pattern: String) -> String {
        let code = String(pattern.prefix { $0 != ":" })
        guard let separator = code.lastIndex(of: ".") else { return code }
        let group: String
        switch code[..<separator] {
        case "account": group = String(localized: "Account information")
        case "vault", "secret": group = String(localized: "Vault")
        case "files": group = String(localized: "Files")
        case "channel": group = String(localized: "Conversations")
        case "inbox": group = String(localized: "Inbox")
        case "profile": group = String(localized: "Public profile")
        case "attest" where code == "attest.request": return String(localized: "Request a signature")
        case "action" where code == "action.request": return String(localized: "Request an action")
        case "agent.delegation": group = String(localized: "Delegations")
        case "events" where code == "events.endpoint": return String(localized: "Manage event endpoints")
        case "wallet": group = String(localized: "Wallet")
        case "agent.profile": group = "Agent Profile"
        case "discourse": group = "Agent Discourse"
        case "mail": group = "Agent Mail"
        case "knowledge": group = "Agent Knowledge"
        default: return code
        }
        let verb: String
        switch code[code.index(after: separator)...] {
        case "read": verb = String(localized: "Read")
        case "write": verb = String(localized: "Write")
        case "list": verb = String(localized: "List items")
        case "publish": verb = String(localized: "Publish")
        case "contact": verb = String(localized: "Write a letter")
        case "use": verb = String(localized: "Use a secret through a device policy")
        case "propose": verb = String(localized: "Create proposal")
        case "execute": verb = String(localized: "Execute approved proposal")
        default: return code
        }
        return group + " · " + verb
    }
}
