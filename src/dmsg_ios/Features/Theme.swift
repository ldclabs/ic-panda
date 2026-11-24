import SwiftUI

/// Colours come from `Shared/Palette.xcassets` and radii from here; pages use
/// these names rather than literals. See DESIGN.md.
enum Theme {
    /// Tint, links and selected states.
    static let forest = Color("Forest")
    /// Filled actions and own message bubbles: the dark brand green in both themes, under white labels.
    static let forestFill = Color("ForestFill")
    /// Plain pages: the workspace lists and the conversation.
    static let paper = Color("Paper")
    /// Grouped pages: settings, details and forms. System list rows keep their own
    /// colour, since a custom row background loses the pressed highlight; in dark
    /// mode the page is black so those rows still stand apart.
    static let paperGrouped = Color("PaperGrouped")
    /// Cards the app draws itself: bubbles, the composer field, floating notices.
    static let card = Color("Card")
    /// Quiet tinted fill behind icons, quotes and selected capsules.
    static let fill = Color("Fill")
    /// Approval counts keep their white text legible against the red badge.
    static let badgeRed = Color("BadgeRed")

    static let radiusSmall = NativeTheme.radiusSmall
    static let radius = NativeTheme.radius
    static let radiusLarge = NativeTheme.radiusLarge
    static let radiusField = NativeTheme.radiusField
}

/// The app's own motion (DESIGN.md, Motion and feedback). Views take their curves from here;
/// `scripts/ios/test_ui_conventions.py` rejects curve literals elsewhere.
enum Motion {
    /// Something takes its new state with a little give: a checkmark, a star, a badge.
    static let settle = Animation.snappy(duration: 0.3)
    /// A selection glides, a block opens and the rest makes room, a floating notice arrives.
    static let move = Animation.smooth(duration: 0.3)
    /// A fade, a secret developing, a count rolling.
    static let fade = Animation.smooth(duration: 0.2)
    /// A floating notice leaves, quicker than it came.
    static let exit = Animation.easeOut(duration: 0.15)
    /// A countdown ring sweeps evenly through each second.
    static let tick = Animation.linear(duration: 1)
    /// What a jump landed on fades from its highlight.
    static let point = Animation.easeOut(duration: 0.6)
}

extension View {
    /// Animates changes of `value`, or with Reduce Motion uses `reduced` (none by default).
    func motion<V: Equatable>(_ animation: Animation, reduced: Animation? = nil, value: V) -> some View {
        modifier(MotionModifier(animation: animation, reduced: reduced, value: value))
    }

    /// A floating notice rises 12 pt from the edge it sits on as it fades in, and sinks as it
    /// leaves; with Reduce Motion it only fades. Its container animates with `arrivals(of:)`.
    func rising(from edge: VerticalEdge = .bottom) -> some View { modifier(Rising(edge: edge)) }

    /// A badge or floating action pops in and shrinks away; with Reduce Motion it only fades.
    func popping(scale: CGFloat = 0.6) -> some View { modifier(Popping(scale: scale)) }

    /// Animates the floating notice inside as `value` comes (`move`), changes and goes (`exit`).
    /// Anything else, such as the privacy shield covering the screen, stays instant.
    func arrivals<V: Equatable>(of value: V?) -> some View {
        modifier(MotionModifier(animation: value == nil ? Motion.exit : Motion.move, reduced: Motion.fade, value: value))
    }

    /// A count rolls only the digits that changed, up when it grows and down when it shrinks;
    /// with Reduce Motion it fades.
    func countMotion(_ value: some BinaryInteger) -> some View { modifier(CountMotion(value: Double(value))) }

    /// A secret being shown develops from a blur each time `shown` grows while `active`;
    /// hiding (`active == false`) stays instant. With Reduce Motion it appears at once.
    func develop(_ active: Bool, trigger shown: Int) -> some View { modifier(Develop(active: active, shown: shown)) }
}

private struct Develop: ViewModifier {
    let active: Bool
    let shown: Int
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var trigger = 0
    func body(content: Content) -> some View {
        content.keyframeAnimator(initialValue: 1.0, trigger: trigger) { view, clarity in
            let shownClarity = active && !reduceMotion ? clarity : 1.0
            view.blur(radius: (1 - shownClarity) * 6).opacity(0.35 + 0.65 * shownClarity)
        } keyframes: { _ in
            MoveKeyframe(0)
            CubicKeyframe(1, duration: 0.2)
        }
        .onChange(of: shown) { _, _ in
            if active && !reduceMotion { trigger &+= 1 }
        }
    }
}

private struct MotionModifier<V: Equatable>: ViewModifier {
    let animation: Animation
    let reduced: Animation?
    let value: V
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View { content.animation(reduceMotion ? reduced : animation, value: value) }
}

private struct Rising: ViewModifier {
    let edge: VerticalEdge
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View {
        content.transition(reduceMotion ? .opacity : .opacity.combined(with: .offset(y: edge == .bottom ? 12 : -12)))
    }
}

private struct Popping: ViewModifier {
    let scale: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View {
        content.transition(reduceMotion ? .opacity : .scale(scale: scale).combined(with: .opacity))
    }
}

private struct CountMotion: ViewModifier {
    let value: Double
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View {
        content.contentTransition(reduceMotion ? .opacity : .numericText(value: value)).animation(Motion.fade, value: value)
    }
}

extension View {
    /// A bar the content scrolls beneath. iOS 26 fades the content under it as it does
    /// under the toolbars; earlier systems put the bar on `fallback`.
    @ViewBuilder func edgeBar<Bar: View, Fill: ShapeStyle>(_ edge: VerticalEdge, fallback: Fill, @ViewBuilder _ bar: () -> Bar) -> some View {
        if #available(iOS 26, *) { safeAreaBar(edge: edge, spacing: 0) { bar() } }
        else { safeAreaInset(edge: edge, spacing: 0) { bar().background(fallback) } }
    }

    /// Liquid Glass on iOS 26 and later; earlier systems fill the shape with `fallback`.
    @ViewBuilder func glassSurface<S: Shape, Fill: ShapeStyle>(in shape: S, interactive: Bool = false, fallback: Fill) -> some View {
        if #available(iOS 26, *) { glassEffect(interactive ? .regular.interactive() : .regular, in: shape) }
        else { background(fallback, in: shape) }
    }
}

/// The tinted square behind an object's icon. It scales with the text size.
struct ObjectIcon: View {
    private let image: Image
    @ScaledMetric private var size: CGFloat

    init(name: String) { image = Image(systemName: name); _size = ScaledMetric(wrappedValue: 36, relativeTo: .body) }

    init(glyph: AppGlyph, size: CGFloat = 36) { image = glyph.image; _size = ScaledMetric(wrappedValue: size, relativeTo: .body) }

    var body: some View {
        image.font(.system(size: size / 2)).foregroundStyle(Theme.forest)
            .frame(width: size, height: size).background(Theme.fill, in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// A problem to act on or wait for: a failure, a pending step or a risk.
/// Plain explanations go in a section footer instead.
struct NoticeBar: View {
    let text: String
    var body: some View { Label(text, systemImage: "info.circle").font(.footnote).foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading).padding(12).background(Theme.fill, in: RoundedRectangle(cornerRadius: Theme.radius)) }
}

enum ScreenSurface {
    /// Workspace lists and the conversation sit on paper.
    case plain
    /// Settings, details and forms put their rows on the deeper grouped paper.
    case grouped
}

extension View {
    /// The second line of an object row and its time: 15 pt secondary text, as in the system apps.
    func rowSubtitle() -> some View { font(.subheadline).foregroundStyle(.secondary) }
}

extension View {
    func privateScreen(_ surface: ScreenSurface = .grouped) -> some View { modifier(PrivateScreen(surface: surface)) }

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
    let surface: ScreenSurface
    @Environment(SessionModel.self) private var model
    @State private var noticeHost = UUID()
    @State private var bottomInset: CGFloat = 0
    func body(content: Content) -> some View {
        content.scrollDismissesKeyboard(.interactively).scrollContentBackground(.hidden).background(surface == .plain ? Theme.paper : Theme.paperGrouped).privacySensitive()
            .listSectionSpacing(.compact)
            .overlay(alignment: .bottom) { if model.noticeHosts.last == noticeHost { TransientNotice() } }
            .workspaceAlert(presenting: model.noticeHosts.last == noticeHost)
            .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { value in
                bottomInset = value
                if model.noticeHosts.last == noticeHost { model.screenBottomInset = value }
            }
            .onChange(of: model.noticeHosts.last == noticeHost) { _, active in if active { model.screenBottomInset = bottomInset } }
            .onAppear {
                // Split views can deliver onAppear again without disappearing.
                // Re-registering the same host invalidates the root and repeats that cycle.
                if !model.noticeHosts.contains(noticeHost) { model.noticeHosts.append(noticeHost) }
                model.activity()
            }
            .onDisappear {
                if model.noticeHosts.contains(noticeHost) { model.noticeHosts.removeAll { $0 == noticeHost } }
            }
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

/// A red count badge for approvals waiting on the user.
struct CountBadge: View {
    let count: UInt64

    var body: some View {
        Text(count.formatted()).font(.footnote.weight(.semibold)).monospacedDigit().foregroundStyle(.white).countMotion(count)
            .padding(.horizontal, 7).frame(minWidth: 22, minHeight: 22).background(Theme.badgeRed, in: Capsule())
            .popping()
    }
}
