import SwiftUI

struct AccountsView: View {
    @Environment(SessionModel.self) private var model
    @State private var accounts: [LocalAccount] = []
    @State private var loading = false
    @State private var failed = false
    @State private var working = false
    struct LocalAccount: Decodable, Identifiable {
        let account: String; let current: Bool
        var fastUnlock: Bool? = nil; var moved: Bool? = nil; var loginUnlockedAt: UInt64? = nil
        var id: String { account }
    }
    struct PublicName: Decodable { let name: String? }
    var body: some View {
        List {
            SettingsLoadStatus(loading: loading, failed: failed) { Task { await load() } }
            ForEach(accounts) { account in
                let row = LocalAccountRow(account: account, identity: model.accountIdentities[account.id])
                if account.current {
                    row.accessibilityElement(children: .combine).accessibilityAddTraits(.isSelected)
                        .accessibilityIdentifier("accounts.\(account.id)")
                } else {
                    Button { Task { await choose(account) } } label: { row }
                        .buttonStyle(.plain).disabled(working).accessibilityIdentifier("accounts.\(account.id)")
                }
            }
            if !loading && !failed && accounts.isEmpty { ContentUnavailableView("No accounts", systemImage: "person.crop.circle") }
            SettingsAction("Sign in to another account") { Task {
                guard !working else { return }; working = true; defer { working = false }
                await model.lock(); await model.login()
            } }.disabled(working)
            if working { ProgressView() }
        }.privateScreen().navigationTitle("Accounts").task { await load() }.refreshable { await load() }
    }

    private func load() async {
        guard !loading else { return }; loading = true; failed = false; defer { loading = false }
        let epoch = model.epoch
        #if DEBUG
        if model.uiTesting {
            accounts = [LocalAccount(account: "040g2081040g2081040g", current: true),
                        LocalAccount(account: "040g2082040g2082040g", current: false),
                        LocalAccount(account: "040g2083040g2083040g", current: false)]
            let chinese = Bundle.main.preferredLocalizations.first?.hasPrefix("zh") == true
            model.rememberAccountNames([accounts[0].id: "Studio", accounts[1].id: chinese ? "个人账户" : "Personal", accounts[2].id: nil], epoch: epoch)
            return
        }
        #endif
        do {
            guard let engine = model.engine else { return }
            let value = try await engine.accounts()
            guard epoch == model.epoch, !Task.isCancelled else { return }
            accounts = try model.decode(value)
            let ids = accounts.map(\.id)
            for refresh in [false, true] {
                for start in stride(from: 0, to: ids.count, by: 100) {
                    let batch = Array(ids[start..<min(start + 100, ids.count)])
                    do {
                        let snapshot = try await coreOperation { try await engine.channelNames(accounts: batch, refresh: refresh, operation: $0) }
                        guard epoch == model.epoch, !Task.isCancelled else { return }
                        let names: [String: PublicName] = try model.decode(snapshot)
                        model.rememberAccountNames(names.mapValues(\.name), epoch: epoch)
                    } catch {
                        guard epoch == model.epoch, !Task.isCancelled else { return }
                        if refresh { failed = true }
                    }
                }
            }
        } catch { if epoch == model.epoch, !Task.isCancelled { failed = true } }
    }

    private func choose(_ account: LocalAccount) async {
        guard !working, !account.current else { return }; working = true; defer { working = false }
        model.invalidateSystemExports()
        await model.lock()
        do { _ = try await model.engine?.switchAccount(account: account.account); await model.updateSession() }
        catch { model.report(error) }
    }
}

private struct LocalAccountRow: View {
    let account: AccountsView.LocalAccount
    let identity: LocalAccountIdentity?

    var body: some View {
        HStack(spacing: 12) {
            if let bytes = identity?.avatar, let image = UIImage(data: bytes) {
                Image(uiImage: image).resizable().scaledToFill().frame(width: 36, height: 36)
                    .clipShape(RoundedRectangle(cornerRadius: 9))
            } else { ObjectIcon(glyph: .account) }
            VStack(alignment: .leading, spacing: 4) {
                if let name = identity?.name {
                    Text(name).font(.headline).foregroundStyle(.primary).fixedSize(horizontal: false, vertical: true)
                }
                Text(account.account).font(identity?.name == nil ? .body.monospaced() : .caption.monospaced())
                    .foregroundStyle(identity?.name == nil ? .primary : .secondary).environment(\.layoutDirection, .leftToRight)
                if account.moved == true { Text("Workspace copied from another iPhone").font(.caption) }
                else if !account.current { Text("Unlock workspace").font(.caption).foregroundStyle(.secondary) }
            }
            Spacer()
            if account.current { Image(systemName: "checkmark").foregroundStyle(Theme.forest).accessibilityLabel("Current account") }
        }.contentShape(Rectangle())
    }
}
