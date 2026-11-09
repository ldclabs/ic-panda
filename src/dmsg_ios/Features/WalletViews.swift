import SwiftUI
@preconcurrency import DMsgCore

struct WalletsView: View {
    @Environment(SessionModel.self) private var model
    @State private var display = DisplayLoad()
    @State private var wallets: [Wallet] = []
    @State private var pending: [Pending] = []
    @State private var walletID = ""
    @State private var request: ApprovalView?
    @State private var working = false
    struct Wallet: Decodable, Identifiable { let wallet: String; let name: String; var id: String { wallet } }
    struct Pending: Decodable, Identifiable { let id: String; let wallet: String; let intent: DisplayValue }
    var body: some View {
        List {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            Section {
                Text("Manage personal or shared assets across chains. Set approval thresholds and device co-signing requirements.")
                    .font(.subheadline).foregroundStyle(.secondary)
            }
            Section("Multisig wallets") {
                ForEach(wallets) { wallet in NavigationLink(wallet.name) { WalletDetailView(wallet: wallet.wallet, name: wallet.name) } }
                if display.loaded && wallets.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Your assets. Your approval rules.").font(.headline)
                        Text("Set up device co-signing for your own assets, or invite others to manage funds together.")
                            .font(.subheadline).foregroundStyle(.secondary)
                    }
                }
            }
            Section("Join an existing wallet") {
                SettingsField("Wallet canister ID") { TextField("Wallet canister ID", text: $walletID).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                Button("Review joining wallet") { Task { await join() } }.disabled(walletID.isEmpty || working)
            }
            Section {
                NavigationLink("Hardware cosigners") { WalletCosignersView() }
            }
            if !pending.isEmpty {
                Section("Unconfirmed proposals") {
                    ForEach(pending) { entry in
                        VStack(alignment: .leading, spacing: 8) {
                            FingerprintRow(title: "Wallet", value: entry.wallet)
                            CanonicalDetails(value: entry.intent)
                            Button("Reconcile original proposal") { Task {
                                do { _ = try await model.engine?.resumeWalletProposal(requestId: entry.id); await load() }
                                catch { model.report(error) }
                            } }.disabled(working)
                        }
                    }
                }
            }
        }.privateScreen().navigationTitle("Multisig wallets").task { await load() }.refreshable { await load(force: true) }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }
    private func load(force: Bool = false) async {
        guard let engine = model.engine else { return }
        do { pending = try model.decode(try await engine.pendingWalletProposals()) } catch { model.report(error) }
        await display.load(model, kind: "wallets", force: force) { (next: [Wallet]) in wallets = next }
    }
    private func join() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
        do { request = try model.decode(try await engine.walletJoin(wallet: walletID.trimmingCharacters(in: .whitespacesAndNewlines), replaces: [], exclusive: false)) }
        catch { model.report(error) }
    }
}

struct WalletDetailView: View {
    let wallet: String; let name: String
    @Environment(SessionModel.self) private var model
    @State private var display = DisplayLoad()
    @State private var history = DisplayLoad()
    @State private var allProposals: [Proposal] = []
    @State private var details: DisplayValue?
    @State private var proposals: [Proposal] = []
    @State private var next: UInt64?
    @State private var reading = false
    struct Proposal: Decodable, Identifiable { let id: UInt64; let status: String; let intent: DisplayValue }
    struct Page: Decodable { let proposals: [Proposal]; let next: UInt64? }
    var body: some View {
        List {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            DisplayLoadStatus(state: history) { Task { await load(force: true) } }
            NavigationLink("Propose transfer") { WalletTransferView(wallet: wallet) }
            NavigationLink("Wallet controls") { WalletControlsView(wallet: wallet) }
            NavigationLink("This iPhone's keys") { WalletDeviceKeysView(wallet: wallet) }
            Section("Proposals") {
                ForEach(proposals) { proposal in NavigationLink { WalletProposalView(wallet: wallet, proposal: proposal.id) } label: {
                    VStack(alignment: .leading, spacing: 4) { Text("Proposal \(proposal.id)"); Text(proposal.status).font(.caption).foregroundStyle(.secondary) }
                } }
                if next != nil { LoadMoreRow(title: "Load earlier proposals", working: reading, trigger: proposals.count) { await load(more: true) } }
            }
            if let details { DisclosureGroup("Wallet details and policy") { CanonicalDetails(value: details) } }
        }.privateScreen().navigationTitle(name).task { await load() }.refreshable { await load(force: true) }
    }
    private func load(more: Bool = false, force: Bool = false) async {
        guard !reading else { return }; reading = true; defer { reading = false }
        if more {
            proposals = Array(allProposals.prefix(proposals.count + 30))
            next = proposals.count < allProposals.count ? UInt64(proposals.count) : nil
            return
        }
        async let detail: Void = display.load(model, kind: "wallet", id: wallet, force: force) { (next: DisplayValue) in details = next }
        async let entries: Void = history.load(model, kind: "history", id: wallet, force: force) { (page: Page) in
            allProposals = page.proposals
            proposals = Array(allProposals.prefix(max(30, proposals.count)))
            next = proposals.count < allProposals.count ? UInt64(proposals.count) : nil
        }
        _ = await (detail, entries)
    }
}

struct WalletProposalView: View {
    let wallet: String; let proposal: UInt64
    @Environment(SessionModel.self) private var model
    @State private var value: DisplayValue?
    @State private var observedVersion: UInt64?
    @State private var request: ApprovalView?
    @State private var working = false
    @State private var execute = false
    @State private var work: Task<Void, Never>?
    /// The plan cosigned (0 for an ICP proposal), its collected signatures
    /// and a packet pasted from another cosigner.
    @State private var plan: UInt32 = 0
    @State private var cosign: Cosign?
    @State private var packet = ""
    @State private var executeCollected = false
    struct Plans: Decodable { let plans: UInt32 }
    struct Cosign: Decodable { let collected: UInt64; let required: UInt64; let ready: Bool; let packet: String; let canCosign: Bool?; let stale: [UInt32]? }
    var body: some View {
        List {
            if let value { CanonicalDetails(value: value).reminderVisible(.wallet(wallet, String(proposal)), watermark: observedVersion, ready: observedVersion != nil) } else { ProgressView() }
            if let cosign {
                Section {
                    LabeledContent("Signatures collected", value: "\(cosign.collected) / \(cosign.required)")
                    if let stale = cosign.stale, !stale.isEmpty {
                        Label("Signatures from cosigners \(stale.map { "#\($0)" }.joined(separator: ", ")) no longer match their current keys and were set aside; they need to sign again.", systemImage: "exclamationmark.triangle")
                            .font(.caption).foregroundStyle(.orange)
                    }
                    Button("Cosign on this device") { work = Task { await cosignHere() } }.disabled(cosign.canCosign != true)
                    if cosign.canCosign == false { Text("This device has no cosigner key for this proposal.").font(.caption).foregroundStyle(.secondary) }
                    if cosign.collected > 0 { ShareLink(item: cosign.packet) { Label("Share cosign packet", systemImage: "square.and.arrow.up") } }
                    TextField("Cosign packet from another device", text: $packet, axis: .vertical).lineLimit(2...6).font(.caption.monospaced()).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button("Import cosign packet") { work = Task { await importPacket() } }.disabled(packet.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    Button("Execute with collected signatures") { executeCollected = true }.disabled(!cosign.ready)
                } header: { Text("Cosignatures") } footer: {
                    Text("Share this device's signatures with the other cosigners, or import theirs. Importing only collects signatures; executing is a separate step.")
                }.disabled(working)
            }
            Section {
                Button("Review approval") { work = Task { await decide(true) } }
                Button("Review rejection", role: .destructive) { work = Task { await decide(false) } }
                Button("Execute approved proposal") { execute = true }
                Button("Broadcast original signed transaction") { work = Task {
                    guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
                    do { _ = try await coreOperation { try await engine.broadcastWallet(wallet: wallet, proposal: proposal, operation: $0) }; await load() }
                    catch { model.report(error) }
                } }
                Button("Cancel proposal", role: .destructive) { work = Task {
                    guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
                    do { _ = try await engine.cancelWalletProposal(wallet: wallet, proposal: proposal); await load() }
                    catch { model.report(error) }
                } }
                Button("Reconcile original execution") { work = Task { await run(reconcile: true) } }
            }.disabled(working || value == nil)
        }.privateScreen().navigationTitle("Proposal \(proposal)").task { await load() }.refreshable { await load() }
            .confirmationDialog("Execute this approved wallet proposal?", isPresented: $execute) {
                Button("Execute") { work = Task { await run(reconcile: false) } }
            } message: { Text("The wallet checks its current policy and approvals. A required hardware signature is requested for this exact plan.") }
            .confirmationDialog("Execute with collected signatures", isPresented: $executeCollected, titleVisibility: .visible) {
                Button("Execute") { work = Task { await executeCollectedPlan() } }
            }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
            .onDisappear { work?.cancel() }
    }
    private func load() async {
        let epoch = model.epoch
        observedVersion = nil
        do {
            guard let engine = model.engine, let account = model.session?.account else { return }
            let before: ReminderItem? = try model.decode(try await engine.notificationReference(selected: account, targetJson: NoticeEncoding.json(ReminderTarget.wallet(wallet, String(proposal)))))
            let snapshot = try await engine.walletProposal(wallet: wallet, proposal: proposal)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            value = try model.decode(snapshot); observedVersion = before.flatMap { UInt64($0.version) }
            // A proposal without cosigning, or a plan no longer live, has no status.
            let plans: Plans = try model.decode(snapshot)
            let status = try? await engine.walletCosignStatus(wallet: wallet, proposal: proposal, plan: plans.plans)
            guard epoch == model.epoch, !Task.isCancelled else { return }
            plan = plans.plans; cosign = try status.map { try model.decode($0) }
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func cosignHere() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
        do { request = try model.decode(try await engine.walletCosignRequest(wallet: wallet, proposal: proposal, plan: plan)) }
        catch { model.report(error) }
    }
    private func importPacket() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
        do { _ = try await engine.walletCosignImport(wallet: wallet, proposal: proposal, plan: plan, packet: packet); packet = ""; await load() }
        catch { model.report(error) }
    }
    private func executeCollectedPlan() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }; model.activity()
        do { _ = try await coreOperation { try await engine.walletCosignExecute(wallet: wallet, proposal: proposal, plan: plan, operation: $0) }; await load() }
        catch { model.report(error); await load() }
    }
    private func decide(_ approve: Bool) async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
        do { request = try model.decode(try await engine.walletDecision(wallet: wallet, proposal: proposal, approve: approve)) }
        catch { model.report(error) }
    }
    private func run(reconcile: Bool) async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }; model.activity()
        do {
            let result = try await coreOperation { token in
                if reconcile { return try await engine.reconcileWallet(wallet: wallet, proposal: proposal, operation: token) }
                return try await engine.executeWallet(wallet: wallet, proposal: proposal, operation: token)
            }
            if let approval: ApprovalView = try? model.decode(result) { request = approval }
            await load()
        } catch { model.report(error); await load() }
    }
}

struct WalletCosignersView: View {
    @Environment(SessionModel.self) private var model
    @State private var keys: DisplayValue?
    @State private var created: DisplayValue?
    @State private var label = ""
    @State private var wallet = ""
    @State private var registration = WalletProposalSubmission()
    @State private var working = false
    var body: some View {
        Form {
            if let keys { Section("Hardware keys on this device") { CanonicalDetails(value: keys) } }
            TextField("Key label", text: $label).textInputAutocapitalization(.never).disabled(registration.attempted)
            Button("Create hardware cosigner") { Task {
                guard !working else { return }; working = true; defer { working = false }
                do { if let value = try await model.engine?.newWalletCosigner(label: label) { created = try model.decode(value) }; await load() }
                catch { model.report(error) }
            } }.disabled(working || label.isEmpty || registration.attempted)
            TextField("Wallet canister ID", text: $wallet).textInputAutocapitalization(.never).autocorrectionDisabled().disabled(registration.attempted)
            Button(LocalizedStringKey(registration.attempted ? "Reconcile original proposal" : "Propose registering this key")) { Task {
                guard let engine = model.engine, !working else { return }; working = true; defer { working = false; registration.interrupted() }
                let epoch = model.epoch
                do {
                    guard let attempt = registration.begin() else { return }
                    let value: ObjectSnapshot
                    switch attempt {
                    case .create(let id): value = try await engine.registerWalletCosigner(wallet: wallet, label: label, requestId: id)
                    case .reconcile(let id): value = try await engine.resumeWalletProposal(requestId: id)
                    }
                    guard epoch == model.epoch else { return }
                    registration.confirm(try model.decode(value))
                } catch {
                    guard epoch == model.epoch else { return }
                    let state = try? await engine.walletProposalAttempt(requestId: registration.requestID)
                    guard epoch == model.epoch else { return }
                    registration.failed(journal: state)
                    model.report(error)
                }
            } }.disabled(wallet.isEmpty || label.isEmpty || working || registration.result != nil)
            if let value = registration.result ?? created { Section("Register this public key through a wallet proposal") { CanonicalDetails(value: value) } }
        }.privateScreen().navigationTitle("Hardware cosigners").task { await load() }
    }
    private func load() async { do { if let value = try await model.engine?.walletCosigners() { keys = try model.decode(value) } } catch { model.report(error) } }
}
