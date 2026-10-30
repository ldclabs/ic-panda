import Foundation
import Observation
import UIKit
@preconcurrency import DMsgCore

/// A device transfer of the file library over the local network. The core
/// runs the connection; this model advertises the session with Bonjour or
/// finds it, keeps the screen on while the transfer runs, and starts it
/// again with the same session when the app returns to the foreground.
@MainActor @Observable
final class DeviceTransferModel {
    private struct ExportRequest: Decodable { let approval: ApprovalView? }
    struct Status: Decodable, Equatable {
        struct Failure: Decodable, Equatable {
            let code: String
            let message: String
        }
        struct Report: Decodable, Equatable {
            let peer: String?
            let entries: Int
            let records: Int
            let kept: Int
            let skipped: Int
            let blobs: Int
            let bytes: UInt64
            let totalBytes: UInt64
            let lastError: String?
        }
        let role: String
        let session: String
        let advert: String
        let to: String
        let succession: String?
        let port: UInt16?
        let addresses: [String]
        let state: String
        let error: Failure?
        let report: Report
        var running: Bool { state == "waiting" || state == "running" }
    }

    /// What runs, to start again in the foreground.
    private enum Plan {
        case receive(session: String?, succession: String?)
        case send(session: String, to: String?, address: (host: String, port: UInt16)?, succession: String?)
    }

    private(set) var status: Status?
    private(set) var approval: ApprovalView?
    private(set) var starting = false
    /// Local network access is off for dMsg; the person turns it on in Settings.
    private(set) var localNetworkDenied = false
    /// Bonjour could not advertise or find the session: the address is typed in.
    private(set) var bonjourFailed = false
    private var plan: Plan?
    private var advert: BonjourAdvert?
    private var watching: Task<Void, Never>?
    private var pendingPlan: Plan?
    private var epoch = 0

    var running: Bool { starting || status?.running == true }

    /// Receive the file library from another device of the account, for
    /// `succession` when given.
    func receive(_ model: SessionModel, succession: String?) async {
        await start(model, .receive(session: nil, succession: succession))
    }

    /// Send the file library to receiving session `session`, found with
    /// Bonjour or at `address`, to device `to` or the other device of
    /// `succession`.
    func send(_ model: SessionModel, session: String, to: String?, address: (host: String, port: UInt16)?, succession: String?) async {
        await start(model, .send(session: session, to: to, address: address, succession: succession))
    }

    /// Start a stopped transfer again with its session. A sender finds the
    /// receiver again, whose port changes when it resumes.
    func resume(_ model: SessionModel) async {
        guard let status, !running else { return }
        if status.role == "receive" { await start(model, .receive(session: status.session, succession: status.succession)) }
        else { await start(model, .send(session: status.session, to: status.to, address: status.addresses.first.flatMap(Self.parse), succession: status.succession)) }
    }

    func cancel(_ model: SessionModel) {
        epoch += 1; starting = false; plan = nil; approval = nil; pendingPlan = nil
        if let engine = model.engine { status = try? model.decode(engine.transferCancel()) }
        settle()
    }

    /// The device locks: nothing starts again by itself.
    func reset() {
        epoch += 1; starting = false; plan = nil; status = nil; approval = nil; pendingPlan = nil
        localNetworkDenied = false; bonjourFailed = false; settle()
    }

    /// The app leaves the foreground, and the core stops the transfer.
    func background() { epoch += 1; starting = false; settle() }

    /// Back in the foreground: start an interrupted transfer again.
    func foreground(_ model: SessionModel) async {
        guard let plan, let engine = model.engine else { return }
        status = try? model.decode(engine.transferStatus())
        guard status?.running != true else { return watch(model) }
        // Sending again needs another reviewed export; receive can reopen.
        if case .receive = plan { await start(model, plan) }
    }

    /// Returning from the existing approval screen starts only that reviewed send.
    func approved(_ model: SessionModel) async {
        guard let approval, let next = pendingPlan, let engine = model.engine else { return }
        self.approval = nil; pendingPlan = nil
        let current = epoch
        do {
            let fresh: ApprovalView = try model.decode(try await engine.approval(id: approval.id))
            guard current == epoch, fresh.state == "done" else { return }
            await start(model, next, approval: approval.id)
        } catch { if current == epoch { model.report(error) } }
    }

    /// `host:port`, with an IPv6 host in brackets.
    static func parse(_ address: String) -> (host: String, port: UInt16)? {
        let text = address.trimmingCharacters(in: .whitespaces)
        guard let colon = text.lastIndex(of: ":"), let port = UInt16(text[text.index(after: colon)...]), port > 0 else { return nil }
        var host = String(text[..<colon])
        if host.hasPrefix("["), host.hasSuffix("]") { host = String(host.dropFirst().dropLast()) }
        return host.isEmpty ? nil : (host, port)
    }

    private func start(_ model: SessionModel, _ next: Plan, approval: String? = nil) async {
        guard let engine = model.engine, !running else { return }
        let current = epoch
        starting = true; localNetworkDenied = false; bonjourFailed = false
        defer { if current == epoch { starting = false } }
        do {
            let snapshot: ObjectSnapshot
            switch next {
            case .receive(let session, let succession):
                snapshot = try await engine.transferReceiveStart(session: session, succession: succession)
            case .send(let session, let to, let address, let succession):
                let target: (host: String, port: UInt16)
                if let address { target = address }
                else { target = try await Bonjour.find(try transferAdvert(session: session)) }
                guard current == epoch, !Task.isCancelled else { return }
                snapshot = try await engine.transferSendStart(session: session, to: to, host: target.host,
                                                              port: target.port, succession: succession, approval: approval)
                guard current == epoch, !Task.isCancelled else { return }
                let request: ExportRequest = try model.decode(snapshot)
                if let request = request.approval {
                    pendingPlan = .send(session: session, to: to, address: target, succession: succession)
                    self.approval = request
                    return
                }
            }
            guard current == epoch, !Task.isCancelled else { return }
            let started: Status = try model.decode(snapshot)
            status = started
            switch next {
            // A receiving session resumes under its own ID.
            case .receive(_, let succession): plan = .receive(session: started.session, succession: succession)
            case .send: plan = next
            }
            if started.role == "receive", let port = started.port { await advertise(started.advert, port: port, epoch: current) }
            guard current == epoch, !Task.isCancelled else { return }
            UIApplication.shared.isIdleTimerDisabled = true
            watch(model)
        } catch {
            guard current == epoch else { return }
            plan = nil
            switch error as? Bonjour.Failure {
            case .denied?: localNetworkDenied = true
            case .some: bonjourFailed = true
            case nil: model.report(error)
            }
        }
    }

    private func advertise(_ name: String, port: UInt16, epoch: Int) async {
        do {
            let made = try await Bonjour.advertise(name, port: port)
            guard self.epoch == epoch, !Task.isCancelled else { made.stop(); return }
            advert = made
        }
        catch Bonjour.Failure.denied { if self.epoch == epoch { localNetworkDenied = true } }
        catch { if self.epoch == epoch { bonjourFailed = true } }
    }

    private func watch(_ model: SessionModel) {
        watching?.cancel()
        watching = Task { [weak self, weak model] in
            var moved: UInt64?
            while !Task.isCancelled, let self, let model, let engine = model.engine {
                guard let next: Status = try? model.decode(engine.transferStatus()) else { return }
                self.status = next
                // Moving bytes keep the app from locking for idleness.
                if let moved, moved != next.report.bytes { model.transferMoved() }
                moved = next.report.bytes
                if !next.running {
                    // One the app stopped by leaving the foreground starts again.
                    if next.error?.code != "LOCKED" { self.plan = nil }
                    self.settle()
                    return
                }
                do { try await Task.sleep(for: .milliseconds(500)) } catch { return }
            }
        }
    }

    private func settle() {
        watching?.cancel(); watching = nil
        advert?.stop(); advert = nil
        UIApplication.shared.isIdleTimerDisabled = false
    }
}
