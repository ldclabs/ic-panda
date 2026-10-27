import SwiftUI

/// Phones use a real stack: replacing a collapsed split-view detail during
/// its push transition can cancel the destination task and strand selection.
/// Both layouts share one selection, including programmatic favorite links.
struct WorkspaceNavigation<Sidebar: View, Detail: View>: View {
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Binding var selection: String?
    @ViewBuilder var sidebar: (Binding<String?>?) -> Sidebar
    @ViewBuilder var detail: (String?) -> Detail

    var body: some View {
        if sizeClass == .compact {
            NavigationStack(path: Binding(
                get: { selection.map { [$0] } ?? [] },
                set: { selection = $0.last }
            )) {
                sidebar(nil)
                    .navigationDestination(for: String.self) { detail($0) }
            }
        } else {
            NavigationSplitView {
                sidebar($selection)
            } detail: {
                detail(selection)
            }
            .navigationSplitViewStyle(.balanced)
        }
    }
}
