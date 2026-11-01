import SwiftUI
import Observation
@preconcurrency import DMsgCore

/// The view owns values and drafts. Core owns encrypted storage and freshness.
@MainActor @Observable
final class DisplayLoad {
    private(set) var refreshing = false
    private(set) var failed = false
    private(set) var failureText = ""
    private(set) var observed: Date?
    private(set) var stale = false
    private(set) var loaded = false
    private var request = UUID()

    private struct Snapshot<T: Decodable>: Decodable {
        let state: String
        let data: T?
        let observedAtMs: UInt64?
        let stale: Bool
    }

    func load<T: Decodable>(_ model: SessionModel, kind: String, id: String = "", force: Bool = false,
                            apply: @MainActor (T) -> Void) async {
        guard let engine = model.engine, model.unlocked, !model.concealed else { return }
        let token = UUID(), epoch = model.epoch
        request = token; failed = false
        func current() -> Bool { token == request && epoch == model.epoch && !Task.isCancelled }
        func accept(_ object: ObjectSnapshot) throws -> Bool {
            let next: Snapshot<T> = try model.decode(object)
            guard current() else { return false }
            observed = next.observedAtMs.map { Date(timeIntervalSince1970: Double($0) / 1000) }
            stale = next.stale
            loaded = next.state != "missing"
            if let data = next.data { apply(data) }
            return next.stale
        }
        defer { if token == request { refreshing = false } }
        do {
            let due = try accept(try await engine.cachedDisplay(kind: kind, id: id))
            guard current(), force || due else { return }
            refreshing = true
            let fresh = try await coreOperation { try await engine.refreshDisplay(kind: kind, id: id, force: force, operation: $0) }
            _ = try accept(fresh)
        } catch {
            guard current() else { return }
            if case MobileError.Failure(let code) = error, code == "LOCKED" || code == "CANCELLED" { return }
            failed = true
            failureText = String(localized: "Showing saved content. The latest sync did not finish.")
            if case MobileError.Failure(let code) = error, code == "INTEGRITY_FAILED" {
                failureText = String(localized: "A record or device key could not be verified. Your workspace has not been erased.")
            }
            // A verified revocation must still invalidate this account's views.
            if case MobileError.Failure(let code) = error, code == "DEVICE_NOT_APPROVED" { model.report(error) }
        }
    }
}

struct DisplayLoadStatus: View {
    let state: DisplayLoad
    let retry: () -> Void

    var body: some View {
        if state.refreshing { ProgressView("Syncing…").controlSize(.small) }
        if state.stale, let observed = state.observed {
            HStack { Text("Saved on this device"); Text(observed, style: .relative) }
                .font(.caption).foregroundStyle(.secondary)
        }
        if state.failed {
            NoticeBar(text: state.failureText)
            Button("Retry", action: retry).disabled(state.refreshing)
        }
    }
}
