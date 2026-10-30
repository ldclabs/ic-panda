import SwiftUI
@preconcurrency import DMsgCore

/// What this iPhone holds for a team wallet: its native Taproot key root and
/// this wallet's key, native addresses whose key another device holds and
/// which move here, keys this device does not hold that a key here
/// replaces, and bindings of devices the account no longer lists.
struct WalletDeviceKeysView: View {
    let wallet: String
    private struct Details: Decodable {
        struct Me: Decodable { let id: String; let role: String; let status: String }
        struct Control: Decodable { let cosigner: UInt32 }
        struct Address: Decodable, Identifiable { let id: UInt32; let text: String; let label: String; let control: String; let controlDetails: Control?; let archived: Bool? }
        struct Cosigner: Decodable, Identifiable { let id: UInt32; let member: String; let curve: String; let publicKey: String; let label: String; let local: Bool? }
        let me: Me?
        let addresses: [Address]
        let cosigners: [Cosigner]
    }
    private struct Native: Decodable {
        struct Protector: Decodable, Identifiable { let index: UInt32; let biometryOnly: Bool; let createdAt: UInt64; var id: UInt32 { index } }
        struct Key: Decodable { let publicKey: String }
        let created: Bool
        let protection: String?
        let protectors: [Protector]
        let hardware: String?
        let key: Key?
    }
    private struct LocalKey: Decodable, Identifiable { let label: String; let publicKey: String; var id: String { label } }
    private struct Bindings: Decodable { let unbind: [String] }

    @Environment(SessionModel.self) private var model
    @State private var details: Details?
    @State private var native: Native?
    @State private var keys: [LocalKey] = []
    @State private var unbind: [String] = []
    @State private var biometryOnly = false
    @State private var replacing: UInt32?
    @State private var replacement = ""
    @State private var confirming = false
    @State private var replaceID = NativeApple.requestID()
    @State private var replaceAttempted = false
    @State private var replaced = false
    @State private var working = false
    @State private var request: ApprovalView?

    private var mayPropose: Bool {
        guard let me = details?.me else { return false }
        return me.status == "Active" && !["Viewer", "Recovery"].contains(me.role)
    }

    /// Native addresses of this member whose L1 key another device holds.
    private var movable: [Details.Address] {
        guard let details, mayPropose else { return [] }
        return details.addresses.filter { address in
            guard address.control == "taproot_2of2", address.archived != true, let control = address.controlDetails,
                  let key = details.cosigners.first(where: { $0.id == control.cosigner }) else { return false }
            return key.local != true && key.member == details.me?.id
        }
    }

    /// Keys of this member this device does not hold that governance may
    /// replace: not native, not committed by a Taproot address.
    private var replaceable: [Details.Cosigner] {
        guard let details, mayPropose else { return [] }
        return details.cosigners.filter { key in
            key.member == details.me?.id && key.local != true && key.curve != "Bip340"
                && !details.addresses.contains { $0.controlDetails?.cosigner == key.id }
        }
    }

    /// Keys on this device that the wallet does not have yet.
    private var spare: [LocalKey] { keys.filter { key in !(details?.cosigners.contains { $0.publicKey == key.publicKey } ?? false) } }

    var body: some View {
        Form {
            Section {
                if let native { nativeRows(native) } else { ProgressView() }
            } header: { Text("Native Taproot key") } footer: {
                Text("L1 spends from native Taproot addresses need this device's BIP-340 key. One root on this device holds every wallet's key; using it asks for you.")
            }
            if !movable.isEmpty {
                Section("Native addresses") {
                    ForEach(movable) { address in
                        NavigationLink { WalletNativeMoveView(wallet: wallet, from: address.id) } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: address.label.isEmpty ? "#\(address.id)" : address.label)
                                Text(verbatim: address.text).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                                Text("Move to this device").font(.caption).foregroundStyle(Theme.forest)
                            }
                        }
                    }
                }
            }
            if !replaceable.isEmpty {
                Section {
                    ForEach(replaceable) { key in
                        Button { replacing = key.id; if replacement.isEmpty { replacement = spare.first?.label ?? "" } } label: {
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: key.label.isEmpty ? "#\(key.id)" : key.label)
                                    Text(verbatim: "#\(key.id) · \(key.curve)").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                if replacing == key.id { Image(systemName: "checkmark").accessibilityHidden(true) }
                            }
                        }.disabled(replaceAttempted)
                    }
                    if spare.isEmpty { NavigationLink("Hardware cosigners") { WalletCosignersView() } }
                    else if replacing != nil && !replaceAttempted {
                        Picker("Replacement key", selection: $replacement) { ForEach(spare) { Text(verbatim: $0.label).tag($0.label) } }
                        Button("Replace key") { confirming = true }.disabled(working || replacement.isEmpty)
                    }
                    if replaced { Text("Proposal created. Review it in the wallet's proposal list.").font(.footnote) }
                    else if replaceAttempted { Button("Reconcile original proposal") { Task { await resume() } }.disabled(working) }
                } header: { Text("Replace key") } footer: {
                    Text("For a key this device does not hold, such as one on a lost device. After the team approves, the new key takes effect 24 hours later.")
                }
            }
            if !unbind.isEmpty {
                Section("Wallet bindings") {
                    Text("\(unbind.count) devices revoked or no longer in your account are still bound to this wallet.")
                    Button("Unbind") { Task { await unbindDevices() } }.disabled(working)
                }
            }
        }.privateScreen().navigationTitle("This iPhone's keys").navigationBarTitleDisplayMode(.inline)
            .task(id: model.session?.generation) { await load() }
            .refreshable { await load() }
            .confirmationDialog("Create this wallet proposal?", isPresented: $confirming) {
                Button("Create proposal") { Task { await replace() } }
            }
            .navigationDestination(isPresented: Binding(get: { request != nil }, set: { if !$0 { request = nil } })) {
                if let request { ApprovalDetailView(request: request) }
            }
    }

    @ViewBuilder private func nativeRows(_ native: Native) -> some View {
        if native.hardware == "biometrics" { Toggle("Biometrics only", isOn: $biometryOnly).disabled(working) }
        if !native.created {
            Button("Create key root") { Task { await nativeRun { try await $0.walletNativeCreate(biometryOnly: biometryOnly) } } }
                .disabled(working).accessibilityIdentifier("wallet.native.create")
        } else {
            LabeledContent("Protection") { Text(LocalizedStringKey(native.protection == "hardware" ? "Secure Enclave" : "Software")) }
            ForEach(native.protectors) { protector in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Protector \(Int(protector.index) + 1)")
                        if protector.biometryOnly { Text("Biometrics only").font(.caption).foregroundStyle(.secondary) }
                    }
                    Spacer()
                    Button("Remove", role: .destructive) { Task { await nativeRun { try await $0.walletNativeProtectorRemove(index: protector.index) } } }
                        .buttonStyle(.borderless).disabled(working || native.protectors.count < 2)
                }
            }
            Button("Add protector") { Task { await nativeRun { try await $0.walletNativeProtectorAdd(biometryOnly: biometryOnly) } } }.disabled(working)
            FingerprintRow(title: "This wallet's key", value: native.key?.publicKey ?? String(localized: "Not made yet; it is derived when this device registers its key."))
        }
    }

    private func load() async {
        guard let engine = model.engine else { return }
        do {
            details = try model.decode(try await engine.walletDetails(wallet: wallet))
            native = try model.decode(try await engine.walletNativeStatus(wallet: wallet))
            keys = (try? model.decode(try await engine.walletCosigners())) ?? []
            let bindings: Bindings? = try? model.decode(try await engine.walletBindings(wallet: wallet))
            unbind = bindings?.unbind ?? []
        } catch { model.report(error) }
    }

    private func nativeRun(_ work: @MainActor (MobileEngine) async throws -> ObjectSnapshot) async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do { _ = try await work(engine) } catch { model.report(error) }
        await load()
    }

    private func replace() async {
        guard let engine = model.engine, let cosigner = replacing, !working else { return }
        working = true; defer { working = false }
        do {
            replaceAttempted = true
            _ = try await engine.replaceWalletCosigner(wallet: wallet, cosigner: cosigner, label: replacement, requestId: replaceID)
            replaced = true
        } catch {
            if let state = try? await engine.walletProposalAttempt(requestId: replaceID) {
                replaceAttempted = state == "pending" || state == "completed"
                if !replaceAttempted { replaceID = NativeApple.requestID() }
            }
            model.report(error)
        }
    }

    private func resume() async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do { _ = try await engine.resumeWalletProposal(requestId: replaceID); replaced = true }
        catch { model.report(error) }
    }

    private func unbindDevices() async {
        guard let engine = model.engine, !working else { return }
        working = true; defer { working = false }
        do { request = try model.decode(try await engine.walletUnbind(wallet: wallet, devices: unbind)) }
        catch { model.report(error) }
        await load()
    }
}

/// Moving a native Taproot address to this device. Its L1 commits to the
/// old device's key, so this device registers its own key, gets a Taproot
/// address with the old delay and recovery, adds it to the address book
/// when the tier needs that, and sweeps the old address there without
/// change: through L1 while the old device cosigns, or through L2 once
/// outputs are deep enough. Each step is a proposal the team approves.
struct WalletNativeMoveView: View {
    let wallet: String
    let from: UInt32
    private struct Plan: Decodable {
        struct From: Decodable { let id: UInt32; let text: String; let label: String; let chainKeyAfter: Int }
        struct To: Decodable { let id: UInt32; let text: String; let label: String }
        struct Pending: Decodable { let proposal: UInt64; let status: String }
        struct Utxo: Decodable, Identifiable { let txid: String; let vout: UInt32; let value: String; let spendableAt: UInt64?; var id: String { "\(txid):\(vout)" } }
        struct Sweep: Decodable { let tip: UInt64; let feeRate: UInt64; let utxos: [Utxo]; let inputs: Int; let total: String; let fee: String?; let amount: String?; let dust: Bool; let empty: Bool }
        let from: From
        let cosigner: UInt32?
        let to: To?
        let next: String
        let pending: Pending?
        let cooldownUntil: UInt64?
        let archived: Bool
        let sweep: Sweep?
    }

    @Environment(SessionModel.self) private var model
    @State private var l2 = false
    @State private var plan: Plan?
    @State private var working = false

    private var proposes: Bool {
        guard let plan else { return false }
        return ["register", "address", "book", "sweep", "archive"].contains(plan.next) && !(plan.next == "sweep" && plan.sweep?.dust == true)
    }

    var body: some View {
        Form {
            Text("A native address's L1 commits to one device's key, so this device gets an address of its own and the coins move to it. Each step is a proposal the team approves.")
                .font(.footnote).foregroundStyle(.secondary)
            Section {
                Picker("Sweep path", selection: $l2) {
                    Text("L1 · old device cosigns").tag(false)
                    Text("L2 · after the delay").tag(true)
                }.pickerStyle(.segmented).disabled(working)
                if !l2 { Text("The old device must be online to cosign the sweep.").font(.footnote).foregroundStyle(.secondary) }
                else if let blocks = plan?.from.chainKeyAfter {
                    Text("Only outputs at least \(blocks) blocks deep move now; each other output shows the block from which it can.").font(.footnote).foregroundStyle(.secondary)
                }
            }
            if let plan { planSections(plan) } else { ProgressView() }
            Button(LocalizedStringKey(plan?.next == "archive" ? "Archive old address" : "Propose next step")) { Task { await next() } }
                .disabled(!proposes || working).accessibilityIdentifier("wallet.nc.next")
        }.privateScreen().navigationTitle("Move a native address").navigationBarTitleDisplayMode(.inline)
            // Another path's plan never shows for this one.
            .task(id: l2) { plan = nil; await load() }
            .refreshable { await load() }
    }

    @ViewBuilder private func planSections(_ plan: Plan) -> some View {
        Section {
            FingerprintRow(title: "Old address", value: plan.from.label.isEmpty ? plan.from.text : "\(plan.from.label) · \(plan.from.text)")
            if let to = plan.to { FingerprintRow(title: "New address", value: to.label.isEmpty ? to.text : "\(to.label) · \(to.text)") }
            check("Register this device's key", done: plan.cosigner != nil)
            check("Add the new address", done: plan.to != nil)
            check("Sweep the old address", done: plan.archived || plan.sweep?.empty == true)
            check("Archive the old address", done: plan.archived)
            LabeledContent("Next") { Text(Self.step(plan.next)) }
            if let pending = plan.pending { LabeledContent { Text(verbatim: pending.status) } label: { Text("Proposal \(pending.proposal)") } }
            if plan.next == "cooldown", let until = plan.cooldownUntil {
                LabeledContent("Wait for the address book cooldown") { Text(Date(timeIntervalSince1970: TimeInterval(until) / 1000), style: .relative) }
            }
            if plan.next == "root" {
                NoticeBar(text: String(localized: "Create this device's native key root first, in this wallet's settings."))
                NavigationLink("This iPhone's keys") { WalletDeviceKeysView(wallet: wallet) }
            }
            if plan.next == "done" { Label("Moved", systemImage: "checkmark.circle").foregroundStyle(.green) }
        }
        if let sweep = plan.sweep {
            Section {
                ForEach(sweep.utxos) { utxo in
                    HStack {
                        Text(verbatim: "\(utxo.txid.prefix(16))…:\(utxo.vout)").font(.caption.monospaced()).lineLimit(1)
                        Spacer()
                        VStack(alignment: .trailing, spacing: 2) {
                            Text(verbatim: Self.btc(utxo.value)).font(.caption.monospacedDigit())
                            if !l2 || utxo.spendableAt.map({ $0 <= sweep.tip + 1 }) == true { Text("Spendable now").font(.caption2).foregroundStyle(.green) }
                            else if let height = utxo.spendableAt { Text("From block \(height)").font(.caption2).foregroundStyle(.orange) }
                            else { Text("Unconfirmed").font(.caption2).foregroundStyle(.secondary) }
                        }
                    }
                }
                if sweep.empty { Text("The old address holds nothing.").foregroundStyle(.secondary) }
                else {
                    LabeledContent("Outputs swept", value: "\(sweep.inputs) · \(Self.btc(sweep.total))")
                    if let fee = sweep.fee { LabeledContent("Fee", value: Self.btc(fee)) }
                    if let amount = sweep.amount { LabeledContent("Arrives", value: Self.btc(amount)) }
                }
                if sweep.dust { Text("Nothing above the dust limit can move through this path yet.").font(.footnote).foregroundStyle(.orange) }
            } header: { Text("Chain tip: block \(sweep.tip)") } footer: { Text(verbatim: "\(sweep.feeRate) sat/vB") }
        }
    }

    private func check(_ title: LocalizedStringKey, done: Bool) -> some View {
        Label { Text(title) } icon: {
            Image(systemName: done ? "checkmark.circle.fill" : "circle.dotted").foregroundStyle(done ? Color.green : Color.secondary)
        }
    }

    private static func step(_ next: String) -> String {
        switch next {
        case "root": String(localized: "Create this device's key root")
        case "register": String(localized: "Register this device's key")
        case "address": String(localized: "Add the new address")
        case "book": String(localized: "Add it to the address book")
        case "cooldown": String(localized: "Wait for the address book cooldown")
        case "wait": String(localized: "Wait for the proposal")
        case "sweep": String(localized: "Sweep the old address")
        case "archive": String(localized: "Archive the old address")
        case "done": String(localized: "Moved")
        default: next
        }
    }

    /// Satoshis as BTC, exact.
    private static func btc(_ sats: String) -> String {
        NSDecimalNumber(decimal: (Decimal(string: sats) ?? 0) / 100_000_000).stringValue + " BTC"
    }

    private func load() async {
        guard let engine = model.engine else { return }
        do { plan = try model.decode(try await engine.walletNcMigratePlan(wallet: wallet, from: from, l2: l2)) }
        catch { model.report(error) }
    }

    private func next() async {
        guard let engine = model.engine, proposes, !working else { return }
        working = true; defer { working = false }
        do { _ = try await engine.walletNcMigrateStart(wallet: wallet, from: from, l2: l2) }
        catch { model.report(error) }
        await load()
    }
}
