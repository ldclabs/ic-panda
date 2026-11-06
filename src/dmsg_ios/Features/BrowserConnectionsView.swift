import SwiftUI

struct BrowserInvitation: Decodable, Identifiable {
    let inviteId: String; let url: String; let expiresAt: UInt64
    var id: String { inviteId }
}
struct BrowserCandidate: Decodable, Identifiable {
    let pairingId: String; let label: String
    var id: String { pairingId }
}
struct BrowserPairing: Decodable, Identifiable {
    let pairingId: String; let label: String?; let state: String; let expiresAt: UInt64?
    var id: String { pairingId }
}

struct BrowserConnectionsView: View {
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    @State private var invitations: [BrowserInvitation] = []
    @State private var candidates: [String: [BrowserCandidate]] = [:]
    @State private var pairings: [BrowserPairing] = []
    @State private var confirming: CandidateApproval?
    @State private var revoking: BrowserPairing?
    @State private var working = false
    @State private var loading = false
    @State private var failed = false
    struct CandidateApproval { let invite: BrowserInvitation; let candidate: BrowserCandidate }
    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            NoticeBar(text: String(localized: "Connect only a browser you control. A connected browser can submit requests; each signature still requires your approval in dMsg."))
            SettingsAction("Connect this browser") { Task { await create() } }.disabled(working)
            ForEach(invitations) { invite in
                Section("Waiting for browser confirmation") {
                    if let url = URL(string: invite.url) { Link("Open invitation in browser", destination: url) }
                    LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(invite.expiresAt) / 1000), style: .relative) }.font(.caption)
                    SettingsAction("Close invitation", role: .destructive) { Task { await close(invite) } }.disabled(working)
                    ForEach(candidates[invite.id] ?? []) { candidate in
                        Button { confirming = CandidateApproval(invite: invite, candidate: candidate) } label: {
                            VStack(alignment: .leading, spacing: 6) { Text(candidate.label); Text(candidate.id).font(.caption.monospaced()).foregroundStyle(.secondary) }
                        }.disabled(working)
                    }
                }
            }
            Section("Connected browsers") {
                ForEach(pairings) { pairing in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(pairing.label ?? pairing.id).font(.headline)
                        Text(pairing.id).font(.caption.monospaced()).textSelection(.enabled)
                        Text(LocalizedStringKey(pairing.state == "active" ? "Active" : pairing.state == "revoked" ? "Revoked" : pairing.state == "expired" ? "Expired" : pairing.state)).font(.caption).foregroundStyle(.secondary)
                        if let expiry = pairing.expiresAt { LabeledContent("Expires") { Text(Date(timeIntervalSince1970: Double(expiry) / 1000), style: .relative) }.font(.caption) }
                        if pairing.state == "active" { Button("Revoke browser", role: .destructive) { revoking = pairing }.disabled(working) }
                    }.buttonStyle(.borderless)
                }
                if !loading && !failed && pairings.isEmpty { Text("No connected browsers").foregroundStyle(.secondary) }
            }
            if working { ProgressView() }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Connected browsers")
            .task(id: "\(model.epoch):\(scenePhase == .active)") { if scenePhase == .active { await load() } }
            .refreshable { await load() }
            .confirmationDialog("Connect this browser?", isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } })) {
                Button("Connect") { if let choice = confirming { Task { await grant(choice) } } }
            } message: { Text(confirming?.candidate.label ?? "") }
            .confirmationDialog("Revoke this browser?", isPresented: Binding(get: { revoking != nil }, set: { if !$0 { revoking = nil } })) {
                Button("Revoke", role: .destructive) { if let browser = revoking { Task { await revoke(browser) } } }
            } message: { Text(revoking?.label ?? "") }
    }
    private func load() async {
        guard let engine = model.engine, model.unlocked, !loading else { return }; let epoch = model.epoch
        loading = true; failed = false; defer { loading = false }
        do {
            let saved: [BrowserInvitation] = try model.decode(try await engine.browserInvitations())
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }; invitations = saved
            for invite in saved {
                struct Candidates: Decodable { let candidates: [BrowserCandidate] }
                let value: Candidates = try model.decode(try await engine.browserCandidates(invite: invite.id))
                guard epoch == model.epoch && visible, !Task.isCancelled else { return }; candidates[invite.id] = value.candidates
            }
            let browsers: [BrowserPairing] = try model.decode(try await engine.browserPairings())
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }; pairings = browsers
        } catch { if epoch == model.epoch && visible, !Task.isCancelled { failed = true } }
    }

    private func close(_ invite: BrowserInvitation) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; defer { working = false }
        do {
            try await engine.forgetBrowserInvitation(invite: invite.id)
            guard epoch == model.epoch && visible else { return }
            invitations.removeAll { $0.id == invite.id }; candidates.removeValue(forKey: invite.id)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func create() async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do {
            let invitation: BrowserInvitation = try model.decode(try await engine.inviteBrowser())
            guard epoch == model.epoch && visible, let url = URL(string: invitation.url) else { return }
            invitations.append(invitation); openURL(url)
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }
    private func grant(_ choice: CandidateApproval) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do { _ = try await engine.grantBrowser(invite: choice.invite.id, pairing: choice.candidate.id); if epoch == model.epoch && visible { confirming = nil; await load() } }
        catch { if epoch == model.epoch && visible { model.report(error) } }
    }
    private func revoke(_ browser: BrowserPairing) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch
        working = true; model.activity(); defer { working = false }
        do { _ = try await engine.revokeBrowser(pairing: browser.id); if epoch == model.epoch && visible { revoking = nil; await load() } }
        catch { if epoch == model.epoch && visible { model.report(error) } }
    }
}

struct ConnectLandingView: View {
    let link: ConnectLink
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var approval: ApprovalView?
    @State private var loading = false
    @State private var missing = false
    var body: some View {
        Group {
            if let account = link.account, account != model.session?.account {
                Form {
                    Text("This browser request belongs to another account. Switching locks the current workspace and preserves its draft.")
                    LabeledContent("Account ID", value: account)
                }
            } else if !model.unlocked { AccessView() }
            else if link.destination == .pair { BrowserConnectionsView() }
            else if let approval { ApprovalDetailView(request: approval) }
            else {
                VStack(spacing: 20) {
                    if loading { ProgressView() }
                    Text(LocalizedStringKey(missing ? "The request is not available to this device. It may have expired or already been handled." : "Loading the verified request…"))
                    Button("Retry") { Task { await locate() } }.disabled(loading)
                    NavigationLink("Open approvals") { ApprovalsView() }
                }.padding()
            }
        }.navigationTitle("Browser request")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .task(id: "\(model.epoch):\(model.unlocked)") { await locate() }
    }
    private func locate() async {
        guard link.destination == .request, model.unlocked, model.session?.account == link.account, let request = link.request, let engine = model.engine else { return }
        let epoch = model.epoch; loading = true; defer { loading = false }
        do {
            _ = try await coreOperation { try await engine.syncConnect(operation: $0) }
            let id = try await engine.browserRequestApproval(request: request)
            await model.loadApprovals()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            approval = model.approvals.first { $0.id == id }; missing = approval == nil
        } catch { if epoch == model.epoch { missing = true; model.report(error) } }
    }
}
