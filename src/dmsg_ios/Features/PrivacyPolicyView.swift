import SwiftUI

struct PrivacyPolicyView: View {
    @State private var paragraphs: [String] = []
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                ForEach(Array(paragraphs.enumerated()), id: \.offset) { _, paragraph in
                    if paragraph.hasPrefix("# ") { Text(String(paragraph.dropFirst(2))).font(.title.bold()) }
                    else if paragraph.hasPrefix("## ") { Text(String(paragraph.dropFirst(3))).font(.title3.bold()) }
                    else { Text(.init(paragraph)).font(.body).textSelection(.enabled) }
                }
                Link("Read the published policy", destination: URL(string: "https://dmsg.net/_/privacy")!)
            }.frame(maxWidth: 680, alignment: .leading).padding(20).frame(maxWidth: .infinity)
        }.navigationTitle("Privacy policy").task {
            let preferred = Bundle.main.preferredLocalizations.first ?? "en"
            let language = preferred == "zh-Hans" ? "zh" : preferred
            if let url = Bundle.main.url(forResource: "privacy-" + language, withExtension: "md"), let text = try? String(contentsOf: url, encoding: .utf8) { paragraphs = text.components(separatedBy: "\n\n").filter { !$0.isEmpty } }
        }
    }
}
