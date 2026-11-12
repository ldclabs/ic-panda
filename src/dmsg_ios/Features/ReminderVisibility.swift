import SwiftUI
import Observation

@MainActor @Observable
private final class ReminderViewport {
    @ObservationIgnored weak var view: UIView?
    var attached = false
    var visible: Bool {
        guard let view, let window = view.window, window.isKeyWindow, !window.isHidden,
              view.bounds.width > 0, view.bounds.height > 0,
              view.convert(view.bounds, to: window).intersects(window.bounds) else { return false }
        var ancestor: UIView? = view
        while let current = ancestor {
            if current.isHidden || current.alpha == 0 { return false }
            if current.clipsToBounds && !view.convert(view.bounds, to: current).intersects(current.bounds) { return false }
            ancestor = current.superview
        }
        var top = window.rootViewController
        while let presented = top?.presentedViewController, !presented.isBeingDismissed { top = presented }
        return top.map { view.isDescendant(of: $0.view) } ?? false
    }
}
@MainActor private struct ReminderViewportProbe: UIViewRepresentable {
    let state: ReminderViewport
    final class Probe: UIView {
        var changed: ((Probe) -> Void)?
        override func didMoveToWindow() { super.didMoveToWindow(); changed?(self) }
    }
    func makeUIView(context: Context) -> Probe {
        let view = Probe(); view.isUserInteractionEnabled = false; state.view = view
        view.changed = { [weak state] view in state?.attached = view.window != nil }
        return view
    }
    func updateUIView(_ view: Probe, context: Context) { state.view = view }
}

private struct OpenedReminderKey: EnvironmentKey {
    static let defaultValue: ReminderItem? = nil
}

extension EnvironmentValues {
    var openedReminder: ReminderItem? {
        get { self[OpenedReminderKey.self] }
        set { self[OpenedReminderKey.self] = newValue }
    }
}

/// Report only a loaded, on-screen business detail in the active scene. Listing
/// reminders or receiving a push never invokes this modifier.
@MainActor private struct ReminderVisibility: ViewModifier {
    let target: ReminderTarget?
    var acknowledgeOnly = false
    let watermark: UInt64?
    let ready: Bool
    let onVisible: @MainActor () async -> Void
    @Environment(SessionModel.self) private var model
    @Environment(\.openedReminder) private var openedReminder
    @Environment(\.scenePhase) private var scenePhase
    @State private var owner = UUID()
    @State private var onScreen = false
    @State private var viewport = ReminderViewport()
    private var scope: String { "\(String(describing: target)):\(openedReminder?.id ?? ""):\(openedReminder?.version ?? ""):\(model.epoch):\(scenePhase == .active):\(onScreen):\(ready):\(watermark ?? 0):\(model.concealed):\(viewport.attached)" }
    func body(content: Content) -> some View {
        content.background(ReminderViewportProbe(state: viewport))
            .onGeometryChange(for: Bool.self) { proxy in
                return proxy.size.height > 0 && proxy.size.width > 0
            } action: { onScreen = $0 }
            .onScrollVisibilityChange { onScreen = $0 }
            .task(id: scope) {
                guard ready, onScreen, scenePhase == .active, model.unlocked, !model.concealed else { model.notifications.hide(owner); return }
                let currentViewport = viewport
                guard currentViewport.visible else { model.notifications.hide(owner); return }
                let epoch = model.epoch
                await onVisible()
                guard epoch == model.epoch, !Task.isCancelled, model.unlocked, !model.concealed, scenePhase == .active, currentViewport.visible else { return }
                if let item = openedReminder, item.account == model.session?.account,
                   (acknowledgeOnly || item.target == target), item.isUnread,
                   watermark.map({ (UInt64(item.version) ?? .max) <= $0 }) ?? true,
                   currentViewport.visible {
                    // A failed acknowledgement never blocks readable content. Preserve
                    // the version the user opened, even when a newer reminder arrives.
                    try? await model.notifications.markRead(model, item: item)
                }
                guard epoch == model.epoch, !Task.isCancelled, let target, !acknowledgeOnly else { return }
                await model.notifications.observed(model, owner: owner, target: target, watermark: watermark, active: { currentViewport.visible })
            }
            .onDisappear { model.notifications.hide(owner) }
    }
}
extension View {
    func reminderReadOnDisplay(ready: Bool = true) -> some View {
        modifier(ReminderVisibility(target: nil, acknowledgeOnly: true, watermark: nil, ready: ready, onVisible: {}))
    }

    func reminderVisible(_ target: ReminderTarget, watermark: UInt64? = nil, ready: Bool = true, onVisible: @escaping @MainActor () async -> Void = {}) -> some View {
        modifier(ReminderVisibility(target: target, watermark: watermark, ready: ready, onVisible: onVisible))
    }
}
