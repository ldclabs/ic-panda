import SwiftUI

enum Theme {
    static let forest = Color(uiColor: NativeTheme.forest)
    static let paper = Color(uiColor: NativeTheme.paper)
    static let surface = Color(uiColor: .secondarySystemGroupedBackground)
}

struct ObjectIcon: View {
    private let image: Image

    init(name: String) { image = Image(systemName: name) }

    init(glyph: AppGlyph) { image = glyph.image }

    var body: some View {
        image.resizable().scaledToFit().frame(width: 18, height: 18).foregroundStyle(Theme.forest)
            .frame(width: 36, height: 36).background(Theme.forest.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
            .accessibilityHidden(true)
    }
}

struct NoticeBar: View {
    let text: String
    var body: some View { Label(text, systemImage: "info.circle").font(.footnote).foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading).padding(12).background(Theme.forest.opacity(0.06), in: RoundedRectangle(cornerRadius: 12)) }
}

extension View {
    func privateScreen() -> some View { modifier(PrivateScreen()) }

    /// Two-line object rows are 56 pt at the default text size and grow with
    /// their content. Keep the full row tappable, including its outer insets.
    func compactObjectRow() -> some View {
        frame(minHeight: 44)
            .listRowInsets(EdgeInsets(top: 6, leading: 20, bottom: 6, trailing: 20))
            .hoverEffect(.highlight)
    }
}

extension View {
    /// A plain list's rows sit on the page's own colour. On a wide screen a selectable
    /// list keeps the system's rows, so that its selection still shows.
    func plainListRows(selectable: Bool = false) -> some View {
        listRowBackground(selectable ? nil : Color.clear)
    }
}

extension View {
    /// An alert cannot appear from under a sheet, so the active screen presents it.
    func workspaceAlert(presenting: Bool) -> some View { modifier(WorkspaceAlert(presenting: presenting)) }
}

private struct WorkspaceAlert: ViewModifier {
    let presenting: Bool
    @Environment(SessionModel.self) private var model

    func body(content: Content) -> some View {
        content.alert("dMsg", isPresented: Binding(get: { presenting && model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK", role: .cancel) { model.error = nil }
        } message: { Text(model.error ?? "") }
    }
}

private struct PrivateScreen: ViewModifier {
    @Environment(SessionModel.self) private var model
    @State private var noticeHost = UUID()
    @State private var bottomInset: CGFloat = 0
    func body(content: Content) -> some View {
        content.scrollDismissesKeyboard(.interactively).scrollContentBackground(.hidden).background(Theme.paper).privacySensitive()
            .listSectionSpacing(.compact)
            .overlay(alignment: .bottom) { if model.noticeHosts.last == noticeHost { TransientNotice() } }
            .workspaceAlert(presenting: model.noticeHosts.last == noticeHost)
            .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { value in
                bottomInset = value
                if model.noticeHosts.last == noticeHost { model.screenBottomInset = value }
            }
            .onChange(of: model.noticeHosts.last == noticeHost) { _, active in if active { model.screenBottomInset = bottomInset } }
            .onAppear {
                model.noticeHosts.removeAll { $0 == noticeHost }; model.noticeHosts.append(noticeHost)
                model.activity()
            }
            .onDisappear { model.noticeHosts.removeAll { $0 == noticeHost } }
    }
}

/// Any touch in the window counts as activity for the idle lock. The recognizer
/// sees touches without claiming them, so controls behave as before.
struct TouchActivityProbe: UIViewRepresentable {
    let touched: @MainActor () -> Void

    func makeUIView(context: Context) -> ProbeView { ProbeView(touched: touched) }

    func updateUIView(_ view: ProbeView, context: Context) { view.recognizer.touched = touched }

    final class ProbeView: UIView {
        let recognizer: TouchRecognizer

        init(touched: @escaping @MainActor () -> Void) {
            recognizer = TouchRecognizer(touched: touched)
            super.init(frame: .zero)
            isUserInteractionEnabled = false
        }

        @available(*, unavailable) required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            recognizer.view?.removeGestureRecognizer(recognizer)
            window?.addGestureRecognizer(recognizer)
        }
    }

    final class TouchRecognizer: UIGestureRecognizer, UIGestureRecognizerDelegate {
        var touched: @MainActor () -> Void

        init(touched: @escaping @MainActor () -> Void) {
            self.touched = touched
            super.init(target: nil, action: nil)
            cancelsTouchesInView = false; delaysTouchesEnded = false; delegate = self
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
            touched(); state = .failed
        }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
    }
}
