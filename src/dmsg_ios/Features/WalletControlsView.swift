import SwiftUI
@preconcurrency import DMsgCore

struct WalletControlsView: View {
    let wallet: String
    @Environment(SessionModel.self) private var model
    @State private var display = DisplayLoad()
    @State private var balanceDisplay = DisplayLoad()
    @State private var details: Details?
    @State private var balances: DisplayValue?
    @State private var chain = "Icp"
    @State private var label = ""
    @State private var member = ""
    @State private var memberLabel = ""
    @State private var role = "Viewer"
    @State private var cosigners: Set<UInt32> = []
    @State private var threshold: UInt8 = 1
    @State private var approvers: UInt8 = 1
    @State private var administrators: UInt8 = 1
    @State private var working = false
    @State private var confirming: WalletChange?
    @State private var submission = WalletProposalSubmission()
    @State private var freezing = false
    struct Details: Decodable {
        struct Address: Decodable, Identifiable { let id: UInt32; let text: String; let label: String; let chain: String; let verified: Bool }
        struct Member: Decodable, Identifiable { let id: String; let label: String; let role: String; let status: String }
        struct Cosigner: Decodable, Identifiable { let id: UInt32; let label: String; let hardware: Bool }
        struct Policy: Decodable { let approverThreshold: UInt8; let adminThreshold: UInt8 }
        let frozen: Bool; let addresses: [Address]; let members: [Member]; let cosigners: [Cosigner]; let policy: Policy
    }
    var body: some View {
        Form {
            DisplayLoadStatus(state: display) { Task { await load(force: true) } }
            DisplayLoadStatus(state: balanceDisplay) { Task { await readBalances() } }
            NoticeBar(text: String(localized: "Changes create proposals and still require the wallet's existing approvals. Existing policy fields and member agent permissions are preserved."))
            if let details {
                Section("Receiving addresses") {
                    ForEach(details.addresses) { address in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(address.label.isEmpty ? address.chain : address.label).font(.headline)
                            FingerprintRow(title: "Address", value: address.text)
                            if address.verified {
                                Button("Copy address") { model.clipboard.copy(address.text) }
                                Button("Replace cosignature policy") { confirming = .setCosign(address: address.id, cosigners: cosigners.sorted(), threshold: threshold) }
                            } else { Text("Address verification failed").foregroundStyle(.red) }
                        }
                    }
                    Button("Refresh balances") { Task { await readBalances() } }
                    if let balances { CanonicalDetails(value: balances) }
                }
                Section("Cosigners for the proposed address policy") {
                    ForEach(details.cosigners) { signer in
                        Toggle(isOn: Binding(get: { cosigners.contains(signer.id) }, set: { if $0 { cosigners.insert(signer.id) } else { cosigners.remove(signer.id) } })) {
                            Text(signer.label.isEmpty ? String(signer.id) : signer.label)
                        }
                        Button("Propose removing cosigner", role: .destructive) { confirming = .removeCosigner(cosigner: signer.id) }
                    }
                    Stepper("Required cosignatures \(threshold)", value: $threshold, in: 1...64)
                    Text("No selected cosigners means no additional cosignature policy. Review the complete proposal before approving.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                Section("Add receiving address") {
                    Picker("Chain", selection: $chain) { Text("ICP").tag("Icp"); Text("Bitcoin").tag("Bitcoin"); Text("EVM").tag("Evm"); Text("Solana").tag("Solana") }
                    TextField("Address label", text: $label)
                    Button("Propose address") { confirming = .addAddress(chain: chain, label: label, cosigners: cosigners.sorted(), threshold: threshold) }
                }
                Section("Members") {
                    ForEach(details.members) { value in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(value.label.isEmpty ? value.id : value.label).font(.headline)
                            Text(walletRoleTitle(value.role) + " · " + value.status).font(.caption).foregroundStyle(.secondary)
                            Button("Edit member") { member = value.id; memberLabel = value.label; role = value.role }
                            Button("Propose removing member", role: .destructive) { confirming = .removeMember(identity: value.id) }
                        }
                    }
                    TextField("Member account or Principal", text: $member).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("Member label", text: $memberLabel)
                    Picker("Role", selection: $role) { ForEach(["Admin","Approver","Proposer","Viewer","Recovery"], id: \.self) { Text(walletRoleTitle($0)).tag($0) } }
                    Button("Propose member change") {
                        confirming = details.members.contains(where: { $0.id == member })
                        ? .updateMember(identity: member, role: role, label: memberLabel)
                        : .addMember(identity: member, role: role, label: memberLabel)
                    }.disabled(member.isEmpty)
                }
                Section("Approval thresholds") {
                    Stepper("Spending approvals \(approvers)", value: $approvers, in: 1...64)
                    Stepper("Administrative approvals \(administrators)", value: $administrators, in: 1...64)
                    Button("Propose thresholds") { confirming = .approvalThresholds(approvers: approvers, administrators: administrators) }
                }
                Section {
                    if details.frozen { Button("Propose unfreezing") { confirming = .unfreeze } }
                    else { Button("Freeze wallet", role: .destructive) { freezing = true } }
                }
            } else { ProgressView() }
        }.disabled(working || submission.attempted)
            .safeAreaInset(edge: .bottom) {
                if submission.attempted && submission.result == nil { Button("Reconcile original proposal") { Task { await submit(nil) } }.buttonStyle(.borderedProminent).padding().disabled(working) }
                if submission.result != nil { Text("Proposal created. Review it in the wallet's proposal list.").font(.footnote).padding() }
            }
            .privateScreen().navigationTitle("Wallet controls").task { await load() }
            .confirmationDialog("Create this wallet proposal?", isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } })) {
                Button("Create proposal") { if let change = confirming { Task { await submit(change) } } }
            }
            .confirmationDialog("Freeze this wallet?", isPresented: $freezing) {
                Button("Freeze wallet", role: .destructive) { Task {
                    guard let engine = model.engine, !working else { return }; working = true; defer { working = false }
                    do { _ = try await engine.freezeWallet(wallet: wallet); await load() } catch { model.report(error) }
                } }
            }
    }
    private func load(force: Bool = false) async {
        await display.load(model, kind: "wallet", id: wallet, force: force) { (value: Details) in
            let editing = details != nil && (approvers != details?.policy.approverThreshold || administrators != details?.policy.adminThreshold)
            details = value
            if !editing { approvers = value.policy.approverThreshold; administrators = value.policy.adminThreshold }
        }
    }
    private func readBalances() async {
        await balanceDisplay.load(model, kind: "balances", id: wallet, force: true) { (next: DisplayValue) in balances = next }
    }
    private func submit(_ change: WalletChange?) async {
        guard let engine = model.engine, !working else { return }; let epoch = model.epoch; working = true; confirming = nil; model.activity(); defer { working = false; submission.interrupted() }
        do {
            guard submission.attempted || change != nil, let attempt = submission.begin() else { return }
            let value: ObjectSnapshot
            switch attempt {
            case .reconcile(let id): value = try await engine.resumeWalletProposal(requestId: id)
            case .create(let id):
                guard let change else { return }
                value = try await engine.walletChange(wallet: wallet, change: change, requestId: id)
            }
            guard epoch == model.epoch else { return }; submission.confirm(try model.decode(value))
        } catch {
            let state = try? await engine.walletProposalAttempt(requestId: submission.requestID)
            if epoch == model.epoch { submission.failed(journal: state) }
            if epoch == model.epoch { model.report(error) }
        }
    }
}

func walletRoleTitle(_ role: String) -> String {
    switch role {
    case "Admin": String(localized: "Administrator")
    case "Approver": String(localized: "Approver")
    case "Proposer": String(localized: "Proposer")
    case "Viewer": String(localized: "Viewer")
    case "Recovery": String(localized: "Recovery")
    default: role
    }
}
