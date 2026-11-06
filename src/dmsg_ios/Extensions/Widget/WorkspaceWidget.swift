import SwiftUI
import WidgetKit

struct WorkspaceEntry: TimelineEntry { let date: Date }
struct WorkspaceProvider: TimelineProvider {
    func placeholder(in context: Context) -> WorkspaceEntry { WorkspaceEntry(date: .now) }
    func getSnapshot(in context: Context, completion: @escaping (WorkspaceEntry) -> Void) { completion(WorkspaceEntry(date: .now)) }
    func getTimeline(in context: Context, completion: @escaping (Timeline<WorkspaceEntry>) -> Void) { completion(Timeline(entries: [WorkspaceEntry(date: .now)], policy: .never)) }
}
/// Shows no private content, only ways in.
private struct WorkspaceWidgetView: View {
    @Environment(\.widgetFamily) private var family
    private var forest: Color { Color(uiColor: NativeTheme.forest) }

    var body: some View {
        Group {
            if family == .systemMedium {
                HStack(spacing: 12) {
                    shortcut("Vault", symbol: "lock.shield", destination: "vault")
                    shortcut("Messages", symbol: "bubble.left.and.bubble.right", destination: "messages")
                    shortcut("Files", symbol: "folder", destination: "files")
                }
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    Image(systemName: "lock.shield").font(.largeTitle).foregroundStyle(forest)
                    Text("dMsg").font(.headline)
                    Text("Open private workspace").font(.caption).foregroundStyle(.secondary)
                }.frame(maxWidth: .infinity, alignment: .leading)
                    .widgetURL(URL(string: "dmsg://open/vault"))
            }
        }.containerBackground(Color(uiColor: NativeTheme.paper), for: .widget)
    }

    private func shortcut(_ title: LocalizedStringKey, symbol: String, destination: String) -> some View {
        Link(destination: URL(string: "dmsg://open/" + destination)!) {
            VStack(spacing: 8) {
                Image(systemName: symbol).font(.title2).foregroundStyle(forest)
                Text(title).font(.caption).lineLimit(1).minimumScaleFactor(0.8)
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(forest.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))
        }
    }
}

@main struct WorkspaceWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "dmsg.workspace", provider: WorkspaceProvider()) { _ in
            WorkspaceWidgetView()
        }.configurationDisplayName("dMsg workspace")
            .description("Open dMsg without exposing private content on the Home Screen.")
            .supportedFamilies([.systemSmall, .systemMedium])
    }
}
