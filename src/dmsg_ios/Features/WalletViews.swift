import SwiftUI
@preconcurrency import DMsgCore

struct WalletsView: View {
    @Environment(SessionModel.self) private var model
    @State private var wallets: [Wallet] = []
    @State private var pending: [Pending] = []
    @State private var walletID = ""
    @State private var request: ApprovalView?
    @State private var working = false
    struct Wallet: Decodable, Identifiable { let wallet: String; let name: String; var id: String { wallet } }
    struct Pending: Decodable, Identifiable { let id: String; let wallet: String; let intent: DisplayValue }
    var body: some View {
        List {
            Section("Team wallets") {
                ForEach(wallets) { wallet in NavigationLink(wallet.name) { WalletDetailView(wallet: wallet.wallet, name: wallet.name) } }
                if wallets.isEmpty { Text("No team wallets").foregroundStyle(.secondary) }
            }
            Section("Join an existing wallet") {
                TextField("Wallet canister ID", text: $walletID).textInputAutocapitalization(.never).autocorrectionDisabled()
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
        }.privateScreen().navigationTitle("Wallets").task { await load() }.refreshable { await load() }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
    }
    private func load() async {
        guard let engine = model.engine else { return }
        do { pending = try model.decode(try await engine.pendingWalletProposals()); wallets = try model.decode(try await engine.wallets()) }
        catch { model.report(error) }
    }
    private func join() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
        do { request = try model.decode(try await engine.walletJoin(wallet: walletID.trimmingCharacters(in: .whitespacesAndNewlines))) }
        catch { model.report(error) }
    }
}

struct WalletDetailView: View {
    let wallet: String; let name: String
    @Environment(SessionModel.self) private var model
    @State private var details: DisplayValue?
    @State private var proposals: [Proposal] = []
    @State private var next: UInt64?
    @State private var reading = false
    struct Proposal: Decodable, Identifiable { let id: UInt64; let status: String; let intent: DisplayValue }
    struct Page: Decodable { let proposals: [Proposal]; let next: UInt64? }
    var body: some View {
        List {
            NavigationLink("Propose transfer") { WalletTransferView(wallet: wallet) }
            NavigationLink("Wallet controls") { WalletControlsView(wallet: wallet) }
            Section("Proposals") {
                ForEach(proposals) { proposal in NavigationLink { WalletProposalView(wallet: wallet, proposal: proposal.id) } label: {
                    VStack(alignment: .leading, spacing: 4) { Text("Proposal \(proposal.id)"); Text(proposal.status).font(.caption).foregroundStyle(.secondary) }
                } }
                if next != nil { Button("Load earlier proposals") { Task { await load(more: true) } }.disabled(reading) }
            }
            if let details { DisclosureGroup("Wallet details and policy") { CanonicalDetails(value: details) } }
        }.privateScreen().navigationTitle(name).task { await load() }.refreshable { await load() }
    }
    private func load(more: Bool = false) async {
        guard let engine = model.engine, !reading else { return }; reading = true; defer { reading = false }
        do {
            if !more { details = try model.decode(try await engine.walletDetails(wallet: wallet)) }
            let page: Page = try model.decode(try await engine.walletProposals(wallet: wallet, before: more ? next : nil))
            proposals = more ? proposals + page.proposals : page.proposals; next = page.next
        } catch { model.report(error) }
    }
}

struct WalletProposalView: View {
    let wallet: String; let proposal: UInt64
    @Environment(SessionModel.self) private var model
    @State private var value: DisplayValue?
    @State private var request: ApprovalView?
    @State private var working = false
    @State private var execute = false
    @State private var work: Task<Void, Never>?
    var body: some View {
        List {
            if let value { CanonicalDetails(value: value) } else { ProgressView() }
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
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) { if let request { ApprovalDetailView(request: request) } }
            .onDisappear { work?.cancel() }
    }
    private func load() async {
        do { if let snapshot = try await model.engine?.walletProposal(wallet: wallet, proposal: proposal) { value = try model.decode(snapshot) } }
        catch { model.report(error) }
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

struct WalletTransferView: View {
    let wallet: String
    @Environment(SessionModel.self) private var model
    @State private var from: UInt32 = 0
    @State private var asset = "icp"
    @State private var recipient = ""
    @State private var amount = ""
    @State private var maxFee = ""
    @State private var working = false
    @State private var requestID = NativeApple.requestID()
    @State private var attempted = false
    @State private var result: DisplayValue?
    var body: some View {
        Form {
            Section("Transfer proposal") {
                Stepper("Source address \(from)", value: $from, in: 0...1000)
                TextField("Asset identifier", text: $asset).textInputAutocapitalization(.never).autocorrectionDisabled()
                Text("Use icp, icrc:ledger, btc, evm:chain, evm:chain:token, sol or sol:mint.").font(.caption).foregroundStyle(.secondary)
                TextField("Recipient address", text: $recipient, axis: .vertical).textInputAutocapitalization(.never).autocorrectionDisabled()
                TextField("Amount in base units", text: $amount).keyboardType(.numberPad)
                TextField("Maximum fee in base units", text: $maxFee).keyboardType(.numberPad)
            }.disabled(working || attempted)
            NoticeBar(text: String(localized: "This creates a proposal. It does not transfer funds until the wallet's approval and execution requirements are met. Amounts are exact base-unit integers."))
            Button(LocalizedStringKey(attempted ? "Reconcile original proposal" : "Create transfer proposal")) { Task { await submit() } }.disabled(working || result != nil || amount.isEmpty || maxFee.isEmpty || recipient.isEmpty)
            if let result { CanonicalDetails(value: result) }
        }.privateScreen().navigationTitle("Propose transfer")
    }
    private func submit() async {
        guard let engine = model.engine, !working else { return }; working = true; defer { working = false }; model.activity()
        do {
            let value: ObjectSnapshot
            if attempted { value = try await engine.resumeWalletProposal(requestId: requestID) }
            else {
                attempted = true
                value = try await engine.walletTransfer(transfer: WalletTransfer(wallet: wallet, from: from, asset: asset, recipient: recipient, amount: amount, maxFee: maxFee, memo: nil, requestId: requestID))
            }
            result = try model.decode(value)
        } catch {
            if let state = try? await engine.walletProposalAttempt(requestId: requestID) {
                attempted = state == "pending" || state == "completed"
                if !attempted { requestID = NativeApple.requestID() }
            }
            model.report(error)
        }
    }
}

struct WalletCosignersView: View {
    @Environment(SessionModel.self) private var model
    @State private var keys: DisplayValue?
    @State private var created: DisplayValue?
    @State private var label = ""
    @State private var wallet = ""
    @State private var registrationID = NativeApple.requestID()
    @State private var registrationAttempted = false
    @State private var working = false
    var body: some View {
        Form {
            if let keys { Section("Hardware keys on this device") { CanonicalDetails(value: keys) } }
            TextField("Key label", text: $label).textInputAutocapitalization(.never).disabled(registrationAttempted)
            Button("Create hardware cosigner") { Task {
                guard !working else { return }; working = true; defer { working = false }
                do { if let value = try await model.engine?.newWalletCosigner(label: label) { created = try model.decode(value) }; await load() }
                catch { model.report(error) }
            } }.disabled(working || label.isEmpty)
            TextField("Wallet canister ID", text: $wallet).textInputAutocapitalization(.never).autocorrectionDisabled().disabled(registrationAttempted)
            Button("Propose registering this key") { Task {
                guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
                do {
                    registrationAttempted = true
                    created = try model.decode(try await engine.registerWalletCosigner(wallet: wallet, label: label, requestId: registrationID))
                } catch {
                    if let state = try? await engine.walletProposalAttempt(requestId: registrationID) {
                        registrationAttempted = state == "pending" || state == "completed"
                        if !registrationAttempted { registrationID = NativeApple.requestID() }
                    }
                    model.report(error)
                }
            } }.disabled(wallet.isEmpty || label.isEmpty || working)
            if let created { Section("Register this public key through a wallet proposal") { CanonicalDetails(value: created) } }
        }.privateScreen().navigationTitle("Hardware cosigners").task { await load() }
    }
    private func load() async { do { if let value = try await model.engine?.walletCosigners() { keys = try model.decode(value) } } catch { model.report(error) } }
}
