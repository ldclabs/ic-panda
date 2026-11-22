import SwiftUI

struct LoginBindingRequestView: View {
    @Environment(SessionModel.self) private var model
    @State private var account = ""
    @State private var issued: (account: String, token: String)?
    @State private var working = false

    var body: some View {
        Form {
            NoticeBar(text: String(localized: "Ask an administrator to bind this login to your existing account. Device authorization is a separate step."))
            TextField("Account ID", text: $account).textInputAutocapitalization(.never).autocorrectionDisabled()
            Button("Create binding request") { Task { await create() } }.disabled(!validMessageAccount(account))
            if let issued, issued.account == account {
                if let principal = model.session?.principal { FingerprintRow(title: "Login identity", value: principal) }
                if let qr = QRCode.image(issued.token) { Image(uiImage: qr).interpolation(.none).resizable().scaledToFit().frame(maxWidth: 240).accessibilityLabel("Share binding request") }
                ShareLink(item: issued.token) { Label("Share binding request", systemImage: "square.and.arrow.up") }
                Button("Copy binding request") { model.clipboard.copy(issued.token) }
                Button("Accept approved binding") { Task { await accept(issued.account) } }
            }
            if working { ProgressView("Loading…") }
        }.disabled(working).privateScreen().navigationTitle("Join account")
            .onChange(of: account) { _, _ in issued = nil }
            .onChange(of: model.viewGeneration) { _, _ in issued = nil }
    }

    private func create() async {
        guard !working, validMessageAccount(account) else { return }
        let target = account, stamp = model.epoch
        working = true; defer { working = false }
        do {
            let token = try await bindingToken(for: target)
            guard stamp == model.epoch, target == account, !Task.isCancelled else { return }
            issued = (target, token)
        } catch { if stamp == model.epoch { model.report(error) } }
    }

    private func bindingToken(for target: String) async throws -> String {
        #if DEBUG
        if model.uiTesting { return "fixture-binding-" + target }
        #endif
        guard let engine = model.engine else { throw CancellationError() }
        let result = try await engine.createBindingRequest(account: target)
        return try JSONDecoder().decode(String.self, from: Data(result.json.utf8))
    }

    private func accept(_ target: String) async {
        guard !working, issued?.account == target, account == target, let engine = model.engine else { return }
        let stamp = model.epoch
        working = true; defer { working = false }
        do {
            _ = try await engine.acceptBinding(account: target)
            guard stamp == model.epoch else { return }
            if await model.updateSession() { model.showingJoinAccount = false }
        } catch { if stamp == model.epoch { model.report(error) } }
    }
}
