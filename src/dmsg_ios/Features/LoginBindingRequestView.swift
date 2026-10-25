import SwiftUI

struct LoginBindingRequestView: View {
    @Environment(SessionModel.self) private var model
    @State private var account = ""
    @State private var token: String?
    var body: some View {
        Form {
            NoticeBar(text: String(localized: "Ask an administrator to bind this login to your existing account. Device authorization is a separate step."))
            TextField("Account ID", text: $account).textInputAutocapitalization(.never).autocorrectionDisabled()
            Button("Create binding request") { Task {
                do {
                    if let result = try await model.engine?.createBindingRequest(account: account) {
                        token = try JSONDecoder().decode(String.self, from: Data(result.json.utf8))
                    }
                } catch { model.report(error) }
            } }.disabled(account.count != 20)
            if let token {
                ShareLink(item: token) { Label("Share binding request", systemImage: "square.and.arrow.up") }
                Button("Copy binding request") { model.clipboard.copy(token) }
                Button("Accept approved binding") { Task {
                    do { _ = try await model.engine?.acceptBinding(account: account); await model.updateSession() }
                    catch { model.report(error) }
                } }
            }
        }.privateScreen().navigationTitle("Join account")
    }
}
