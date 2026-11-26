import Foundation
import Network
import dnssd
@preconcurrency import DMsgCore

/// Nearby peer discovered over LAN Bonjour and/or Apple AWDL (`_dmsg-share._tcp`).
struct NearbyDiscoveredPeer: Identifiable, Hashable, Sendable {
    let advert: String
    let alias: String
    let pkHint: String
    let transport: String
    var id: String { advert }
}

private let nearbyServiceType = "_dmsg-share._tcp"
private let nearbyMaxFrameBytes = 1_114_176

private func makePeerToPeerParameters() -> NWParameters {
    let params = NWParameters.tcp
    params.includePeerToPeer = true
    return params
}

/// Pumps 4-byte big-endian length-prefixed frames between an `NWConnection` and a
/// Rust `BridgeTransport` slot (`nearbyBridgeOpen` / `Push` / `Pull` / `Close`).
private final class FramedAWDLBridge: @unchecked Sendable {
    private let connection: NWConnection
    private let engine: MobileEngine
    private let bridgeId: UInt64
    private let queue: DispatchQueue
    private var writerTask: Task<Void, Never>?
    private var closed = false

    init(connection: NWConnection, engine: MobileEngine, bridgeId: UInt64, queue: DispatchQueue) {
        self.connection = connection
        self.engine = engine
        self.bridgeId = bridgeId
        self.queue = queue
    }

    func start() {
        connection.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready:
                self.beginPump()
            case .failed, .cancelled:
                self.stop()
            default:
                break
            }
        }
        connection.start(queue: queue)
    }

    private func beginPump() {
        guard !closed else { return }
        readNextHeader()
        let engine = self.engine
        let bridgeId = self.bridgeId
        writerTask = Task.detached { [weak self] in
            while !Task.isCancelled {
                do {
                    guard let frame = try await engine.nearbyBridgePull(bridgeId: bridgeId) else {
                        self?.stop()
                        return
                    }
                    guard let self else { return }
                    try await self.sendFramed(frame)
                } catch {
                    self?.stop()
                    return
                }
            }
        }
    }

    private func sendFramed(_ frame: Data) async throws {
        var packet = Data(capacity: 4 + frame.count)
        var length = UInt32(frame.count).bigEndian
        withUnsafeBytes(of: &length) { packet.append(contentsOf: $0) }
        packet.append(frame)
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            connection.send(content: packet, completion: .contentProcessed { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            })
        }
    }

    private func readNextHeader() {
        connection.receive(minimumIncompleteLength: 4, maximumLength: 4) { [weak self] content, _, isComplete, error in
            guard let self, !self.closed else { return }
            if error != nil {
                self.stop()
                return
            }
            guard let header = content, header.count == 4 else {
                if isComplete { self.stop() }
                return
            }
            let length = header.withUnsafeBytes { raw in
                Int(raw.loadUnaligned(as: UInt32.self).bigEndian)
            }
            guard length > 0, length <= nearbyMaxFrameBytes else {
                self.stop()
                return
            }
            self.readBody(length: length)
        }
    }

    private func readBody(length: Int) {
        connection.receive(minimumIncompleteLength: length, maximumLength: length) { [weak self] content, _, isComplete, error in
            guard let self, !self.closed else { return }
            if error != nil {
                self.stop()
                return
            }
            guard let body = content, body.count == length else {
                if isComplete { self.stop() }
                return
            }
            let engine = self.engine
            let bridgeId = self.bridgeId
            Task.detached { [weak self] in
                do {
                    try await engine.nearbyBridgePush(bridgeId: bridgeId, frame: body)
                    self?.queue.async {
                        guard let self, !self.closed else { return }
                        self.readNextHeader()
                    }
                } catch {
                    self?.stop()
                }
            }
        }
    }

    func stop() {
        queue.async { [weak self] in
            guard let self, !self.closed else { return }
            self.closed = true
            self.writerTask?.cancel()
            self.writerTask = nil
            self.connection.stateUpdateHandler = nil
            self.connection.cancel()
            self.engine.nearbyBridgeClose(bridgeId: self.bridgeId)
        }
    }
}

/// Foreground AWDL + Bonjour listener advertising `_dmsg-share._tcp` with `includePeerToPeer = true`.
final class NearbyAWDLListener: @unchecked Sendable {
    private let queue = DispatchQueue(label: "net.dmsg.nearby.awdl.listener")
    private let engine: MobileEngine
    private var listener: NWListener?
    private var bridges: [FramedAWDLBridge] = []

    init(advert: String, alias: String, pkHint: String, engine: MobileEngine, onState: @escaping @Sendable (Bool) -> Void) {
        self.engine = engine
        let params = makePeerToPeerParameters()
        guard let listener = try? NWListener(using: params) else {
            onState(false)
            return
        }
        let txt = NWTXTRecord(["v": "1", "n": alias, "p": pkHint])
        listener.service = NWListener.Service(name: advert, type: nearbyServiceType, domain: nil, txtRecord: txt)
        listener.stateUpdateHandler = { state in
            switch state {
            case .ready:
                onState(true)
            case .failed, .waiting:
                onState(false)
            default:
                break
            }
        }
        listener.newConnectionHandler = { [weak self] connection in
            guard let self else {
                connection.cancel()
                return
            }
            guard let bridgeId = try? self.engine.nearbyBridgeOpen(incoming: true) else {
                connection.cancel()
                return
            }
            let bridge = FramedAWDLBridge(connection: connection, engine: self.engine, bridgeId: bridgeId, queue: self.queue)
            self.bridges.append(bridge)
            bridge.start()
        }
        self.listener = listener
        listener.start(queue: queue)
    }

    func stop() {
        queue.async { [weak self] in
            guard let self else { return }
            self.listener?.stateUpdateHandler = nil
            self.listener?.newConnectionHandler = nil
            self.listener?.cancel()
            self.listener = nil
            for bridge in self.bridges {
                bridge.stop()
            }
            self.bridges.removeAll()
        }
    }
}

/// Continuous AWDL + Bonjour browser for `_dmsg-share._tcp` peers (`includePeerToPeer = true`).
final class NearbyAWDLBrowser: @unchecked Sendable {
    private let queue = DispatchQueue(label: "net.dmsg.nearby.awdl.browser")
    private var browser: NWBrowser?

    init(onPeers: @escaping @Sendable ([NearbyDiscoveredPeer]) -> Void, onDenied: @escaping @Sendable () -> Void) {
        let params = NWParameters()
        params.includePeerToPeer = true
        let descriptor = NWBrowser.Descriptor.bonjourWithTXTRecord(type: nearbyServiceType, domain: nil)
        let browser = NWBrowser(for: descriptor, using: params)
        browser.stateUpdateHandler = { state in
            if case .waiting(let error) = state, case .dns(let code) = error, Int(code) == kDNSServiceErr_PolicyDenied {
                onDenied()
            }
        }
        browser.browseResultsChangedHandler = { results, _ in
            var peers: [NearbyDiscoveredPeer] = []
            for result in results {
                guard case .service(let name, _, _, _) = result.endpoint else { continue }
                var version = ""
                var alias = "dMsg Device"
                var pkHint = ""
                if case .bonjour(let record) = result.metadata {
                    version = record["v"] ?? ""
                    if let rawAlias = record["n"], !rawAlias.isEmpty {
                        alias = nearbySanitizeAlias(raw: rawAlias)
                    }
                    pkHint = record["p"] ?? ""
                }
                guard version.isEmpty || version == "1" else { continue }
                peers.append(NearbyDiscoveredPeer(advert: name, alias: alias, pkHint: pkHint, transport: "awdl"))
            }
            onPeers(peers)
        }
        self.browser = browser
        browser.start(queue: queue)
    }

    func stop() {
        queue.async { [weak self] in
            self?.browser?.stateUpdateHandler = nil
            self?.browser?.browseResultsChangedHandler = nil
            self?.browser?.cancel()
            self?.browser = nil
        }
    }
}

/// Outgoing Apple AWDL + Bonjour `NWConnection` bound to a Rust `BridgeTransport`.
final class NearbyAWDLConnector: @unchecked Sendable {
    private let queue = DispatchQueue(label: "net.dmsg.nearby.awdl.client")
    private let bridge: FramedAWDLBridge
    let bridgeId: UInt64

    init?(advert: String, engine: MobileEngine) {
        guard let bridgeId = try? engine.nearbyBridgeOpen(incoming: false) else {
            return nil
        }
        self.bridgeId = bridgeId
        let endpoint = NWEndpoint.service(name: advert, type: nearbyServiceType, domain: "local.", interface: nil)
        let connection = NWConnection(to: endpoint, using: makePeerToPeerParameters())
        self.bridge = FramedAWDLBridge(connection: connection, engine: engine, bridgeId: bridgeId, queue: queue)
        self.bridge.start()
    }

    func stop() {
        bridge.stop()
    }
}
