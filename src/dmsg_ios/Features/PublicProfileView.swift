import SwiftUI

struct PublicProfileView: View {
    @Environment(SessionModel.self) private var model
    @State private var name = ""
    @State private var about = ""
    @State private var updated: Int64?
    @State private var status = ""
    @State private var confirming = false
    struct Publication: Decodable { let published: Bool }
    var body: some View {
        Form {
            NoticeBar(text: String(localized: "This profile is public. Do not include secrets or private contact information."))
            Section("Public profile") { TextField("Name", text: $name); TextField("About", text: $about, axis: .vertical).lineLimit(3...8) }
            if !status.isEmpty { Text(status).font(.footnote).foregroundStyle(.secondary) }
            Button("Publish profile") { confirming = true }.disabled(updated == nil)
        }.privateScreen().navigationTitle("Public profile").task {
            do {
                guard let result = try await model.engine?.profile() else { return }
                let profile: AccountProfile = try model.decode(result)
                name = profile.name ?? ""; about = profile.description ?? ""; updated = profile.updatedAt
            } catch { model.report(error) }
        }.confirmationDialog("Publish these details publicly?", isPresented: $confirming) {
            Button("Publish") { Task { await publish() } }
        }
    }
    private func publish() async {
        guard let updated else { return }
        do {
            guard let result = try await model.engine?.publishProfile(name: name, description: about, expectedUpdatedAt: updated) else { return }
            let publication: Publication = try model.decode(result)
            status = publication.published ? String(localized: "Published") : String(localized: "Saved. Publication is pending.")
        } catch { model.report(error) }
    }
}
