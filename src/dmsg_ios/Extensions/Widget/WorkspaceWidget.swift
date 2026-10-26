import SwiftUI
import WidgetKit

struct WorkspaceEntry: TimelineEntry { let date: Date }
struct WorkspaceProvider: TimelineProvider {
    func placeholder(in context: Context) -> WorkspaceEntry { WorkspaceEntry(date: .now) }
    func getSnapshot(in context: Context, completion: @escaping (WorkspaceEntry) -> Void) { completion(WorkspaceEntry(date: .now)) }
    func getTimeline(in context: Context, completion: @escaping (Timeline<WorkspaceEntry>) -> Void) { completion(Timeline(entries: [WorkspaceEntry(date: .now)], policy: .never)) }
}
@main struct WorkspaceWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "dmsg.workspace", provider: WorkspaceProvider()) { _ in
            VStack(alignment: .leading, spacing: 12) {
                Image(systemName: "lock.shield").font(.largeTitle).foregroundStyle(Color(uiColor: NativeTheme.forest))
                Text("dMsg").font(.headline)
                Text("Open private workspace").font(.caption).foregroundStyle(.secondary)
            }.frame(maxWidth: .infinity, alignment: .leading)
                .containerBackground(Color(uiColor: NativeTheme.paper), for: .widget)
                .widgetURL(URL(string: "dmsg://open/vault"))
        }.configurationDisplayName("dMsg workspace")
            .description("Open dMsg without exposing private content on the Home Screen.")
            .supportedFamilies([.systemSmall])
    }
}
