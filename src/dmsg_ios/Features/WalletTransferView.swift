import SwiftUI
@preconcurrency import DMsgCore

/// Exact decimal conversion. Never round money through Double or Decimal.
enum AtomicAmount {
    static func parse(_ text: String, decimals: Int, separator: String = Locale.current.decimalSeparator ?? ".") -> String? {
        guard (0...38).contains(decimals) else { return nil }
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: separator, with: ".")
        guard !text.isEmpty, text.count <= 80, !text.hasSuffix(".") else { return nil }
        var normalized = ""
        for character in text {
            if character == "." { normalized.append(character) }
            else if let digit = character.wholeNumberValue, (0...9).contains(digit) { normalized += String(digit) }
            else { return nil }
        }
        let parts = normalized.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count <= 2, !parts[0].isEmpty else { return nil }
        let fraction = parts.count == 2 ? String(parts[1]) : ""
        guard fraction.count <= decimals else { return nil }
        let digits = String((String(parts[0]) + fraction + String(repeating: "0", count: decimals - fraction.count)).drop { $0 == "0" })
        let result = digits.isEmpty ? "0" : digits
        let maximum = "340282366920938463463374607431768211455"
        guard result.count < maximum.count || result.count == maximum.count && result <= maximum else { return nil }
        return result
    }

    static func display(_ atoms: String, decimals: Int) -> String {
        guard (0...38).contains(decimals), !atoms.isEmpty, atoms.allSatisfy({ $0.isASCII && $0.isNumber }) else { return atoms }
        guard decimals > 0 else { return atoms }
        let padded = String(repeating: "0", count: max(0, decimals + 1 - atoms.count)) + atoms
        let split = padded.index(padded.endIndex, offsetBy: -decimals)
        let fraction = String(padded[split...]).replacingOccurrences(of: "0+$", with: "", options: .regularExpression)
        return String(padded[..<split]) + (fraction.isEmpty ? "" : "." + fraction)
    }
}

struct WalletTransferView: View {
    let wallet: String
    @Environment(SessionModel.self) private var model
    @State private var visible = true
    @State private var draft = Draft()
    @State private var assets: [Asset] = []
    @State private var addresses: [Address] = []
    @State private var preview: Preview?
    @State private var feeDecimals: Int?
    @State private var working = false
    @State private var loading = false
    @State private var failed = false
    @State private var requestID = NativeApple.requestID()
    @State private var attempted = false
    @State private var result: DisplayValue?
    @State private var scope: UInt64 = 0
    struct Draft: Codable, Equatable { var from: UInt32 = 0; var asset = ""; var recipient = ""; var amount = ""; var maxFee = "" }
    struct SavedDraft: Codable { let draft: Draft; let request: String; let attempted: Bool }
    struct Asset: Decodable, Identifiable {
        let key: String; let symbol: String?; let decimals: Int?; let error: String?
        var id: String { key }
        var chain: String { key == "icp" || key.hasPrefix("icrc:") ? "Icp" : key == "btc" ? "Bitcoin" : key.hasPrefix("evm:") ? "Evm" : "Solana" }
    }
    struct Address: Decodable, Identifiable { let id: UInt32; let label: String; let text: String; let chain: String; let verified: Bool }
    struct Preview: Decodable {
        struct Requirement: Decodable { let approvals: UInt32; let admin: Bool; let delayMs: UInt64 }
        let estimatedFee: String; let maxFee: String; let feeAsset: String; let feeDecimals: Int
        let assetBalance: String; let nativeBalance: String; let additionalReserve: String
        let blockers: [String]; let readAt: UInt64; let requirement: Requirement
    }
    private var selectedAsset: Asset? { assets.first { $0.key == draft.asset } }
    private var sources: [Address] { addresses.filter { $0.verified && $0.chain == selectedAsset?.chain } }
    private var amount: String? {
        guard let asset = selectedAsset, asset.error == nil, let decimals = asset.decimals else { return nil }
        return AtomicAmount.parse(draft.amount, decimals: decimals)
    }
    private var inputValid: Bool {
        amount != nil && amount != "0" && !draft.recipient.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        sources.contains { $0.id == draft.from } && (draft.maxFee.isEmpty || feeDecimals.flatMap { AtomicAmount.parse(draft.maxFee, decimals: $0) } != nil)
    }

    var body: some View {
        Form {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            Section("Transfer proposal") {
                Picker("Asset", selection: $draft.asset) {
                    Text("Select an asset").tag("")
                    ForEach(assets) { asset in Text(verbatim: "\(asset.symbol ?? asset.key) · \(asset.key)").tag(asset.key) }
                }.accessibilityIdentifier("wallet.asset")
                if let asset = selectedAsset, asset.decimals == nil || asset.error != nil { NoticeBar(text: String(localized: "Asset details are unavailable. Retry before entering an amount.")) }
                Picker("Source address", selection: $draft.from) {
                    ForEach(sources) { address in Text(verbatim: address.label.isEmpty ? address.text : address.label).tag(address.id) }
                }
                if assets.isEmpty || selectedAsset != nil && sources.isEmpty { NavigationLink("Wallet controls") { WalletControlsView(wallet: wallet) } }
                if let source = sources.first(where: { $0.id == draft.from }) { FingerprintRow(title: "Address", value: source.text) }
                SettingsField("Recipient address") { TextField("Recipient address", text: $draft.recipient, axis: .vertical).textInputAutocapitalization(.never).autocorrectionDisabled().environment(\.layoutDirection, .leftToRight) }
                SettingsField("Amount") { TextField("Amount", text: $draft.amount).keyboardType(.decimalPad).environment(\.layoutDirection, .leftToRight) }
                if !draft.amount.isEmpty && amount == nil { Text("Enter an exact amount within the asset's precision.").font(.footnote).foregroundStyle(.red) }
                if let amount { LabeledContent("Amount in base units", value: amount).font(.caption) }
                if feeDecimals != nil {
                    SettingsField("Maximum fee") { TextField("Suggested fee", text: $draft.maxFee).keyboardType(.decimalPad).environment(\.layoutDirection, .leftToRight) }
                }
            }.disabled(working || attempted)
            Text("Review the asset, destination, balance and fee before creating a proposal. No funds move until the wallet's approvals and execution are complete.").font(.footnote).foregroundStyle(.secondary)
            if !attempted { SettingsAction("Preview transfer") { Task { await inspect() } }.disabled(working || !inputValid) }
            if let preview {
                Section("Preview") {
                    LabeledContent("Balance", value: AtomicAmount.display(preview.assetBalance, decimals: selectedAsset?.decimals ?? 0) + " " + (selectedAsset?.symbol ?? ""))
                    LabeledContent("Estimated fee", value: AtomicAmount.display(preview.estimatedFee, decimals: preview.feeDecimals) + " " + preview.feeAsset)
                    LabeledContent("Maximum fee", value: AtomicAmount.display(preview.maxFee, decimals: preview.feeDecimals) + " " + preview.feeAsset)
                    if preview.additionalReserve != "0" { LabeledContent("Additional reserve / rent", value: AtomicAmount.display(preview.additionalReserve, decimals: preview.feeDecimals) + " " + preview.feeAsset) }
                    LabeledContent("Required approvals", value: preview.requirement.approvals.formatted())
                    LabeledContent("Role", value: walletRoleTitle(preview.requirement.admin ? "Admin" : "Approver"))
                    if preview.requirement.delayMs > 0 { LabeledContent("Available after") { Text(Date(timeIntervalSince1970: Double(preview.readAt + preview.requirement.delayMs) / 1000), style: .relative) } }
                    ForEach(preview.blockers, id: \.self) { blocker in Text(blockerTitle(blocker)).foregroundStyle(.red) }
                }
            }
            SettingsAction(LocalizedStringKey(attempted ? "Reconcile original proposal" : "Create transfer proposal"), prominent: true) { Task { await submit() } }
                .disabled(working || result != nil || (!attempted && (preview == nil || preview?.blockers.isEmpty != true)))
            if working { ProgressView() }
            if let result { Label("Proposal created", systemImage: "checkmark.circle"); DisclosureGroup("Details") { CanonicalDetails(value: result) } }
        }.privateScreen().onAppear { visible = true }.onDisappear { visible = false }.navigationTitle("Propose transfer").confirmLeavingDraft(!attempted && draft != Draft())
            .task {
                scope = model.viewGeneration
                if let restored = model.formDraft("transfer:" + wallet, as: SavedDraft.self) { draft = restored.draft; requestID = restored.request; attempted = restored.attempted }
                await load()
            }
            .onChange(of: draft) { _, _ in preview = nil }
            .onChange(of: draft.asset) { _, _ in
                draft.maxFee = ""; feeDecimals = nil
                if let source = sources.first { draft.from = source.id }
            }
            .onDisappear {
                if result == nil { model.keepFormDraft("transfer:" + wallet, value: SavedDraft(draft: draft, request: requestID, attempted: attempted), generation: scope) }
            }
    }

    private func transfer(fee: String) -> WalletTransfer {
        WalletTransfer(wallet: wallet, from: draft.from, asset: draft.asset, recipient: draft.recipient.trimmingCharacters(in: .whitespacesAndNewlines), amount: amount ?? "", maxFee: fee, memo: nil, requestId: requestID)
    }

    private func load() async {
        guard let engine = model.engine, !loading else { return }; let epoch = model.epoch
        loading = true; failed = false; defer { loading = false }
        struct Assets: Decodable { let assets: [Asset] }
        struct Details: Decodable { let addresses: [Address] }
        do {
            async let assetRead = engine.walletAssets(wallet: wallet)
            async let detailsRead = engine.walletDetails(wallet: wallet)
            let (assetValue, detailValue) = try await (assetRead, detailsRead)
            guard epoch == model.epoch && visible, !Task.isCancelled else { return }
            let incomingAssets: Assets = try model.decode(assetValue)
            let incomingDetails: Details = try model.decode(detailValue)
            assets = incomingAssets.assets; addresses = incomingDetails.addresses
        } catch { if epoch == model.epoch && visible, !Task.isCancelled { failed = true } }
    }

    private func inspect() async {
        guard let engine = model.engine, !working, inputValid else { return }
        let epoch = model.epoch, submitted = draft; working = true; defer { working = false }
        do {
            let fee = draft.maxFee.isEmpty ? "" : AtomicAmount.parse(draft.maxFee, decimals: feeDecimals ?? 0) ?? ""
            let value: Preview = try model.decode(try await engine.previewWalletTransfer(transfer: transfer(fee: fee)))
            guard epoch == model.epoch && visible, submitted == draft, !Task.isCancelled else { return }
            feeDecimals = value.feeDecimals; preview = value
        } catch { if epoch == model.epoch && visible { model.report(error) } }
    }

    private func submit() async {
        guard let engine = model.engine, !working else { return }
        if !attempted, preview == nil || preview?.blockers.isEmpty != true || Date.now.timeIntervalSince1970 * 1000 - Double(preview?.readAt ?? 0) > 60_000 {
            preview = nil; model.notice = String(localized: "Refresh the preview before continuing."); return
        }
        let epoch = model.epoch; working = true; defer { working = false }; model.activity()
        do {
            let value: ObjectSnapshot
            if attempted { value = try await engine.resumeWalletProposal(requestId: requestID) }
            else {
                let input = transfer(fee: preview?.maxFee ?? "")
                attempted = true; value = try await engine.walletTransfer(transfer: input)
            }
            guard epoch == model.epoch && visible else { return }
            result = try model.decode(value); model.settingsDrafts.removeValue(forKey: "transfer:" + wallet)
        } catch {
            guard epoch == model.epoch && visible else { return }
            if let state = try? await engine.walletProposalAttempt(requestId: requestID) {
                guard epoch == model.epoch && visible else { return }
                attempted = state == "pending" || state == "completed"
                if !attempted { requestID = NativeApple.requestID(); preview = nil }
            }
            model.report(error)
        }
    }

    private func blockerTitle(_ code: String) -> String {
        switch code {
        case "FEE_TOO_LOW": String(localized: "The maximum fee is below the estimate.")
        case "INSUFFICIENT_NATIVE_BALANCE", "INSUFFICIENT_ASSET_BALANCE": String(localized: "Insufficient balance")
        default: code
        }
    }
}
