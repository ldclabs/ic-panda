import Foundation
import Observation
import UIKit
@preconcurrency import DMsgCore

/// Coordinates foreground face-to-face nearby sharing (`dmsg-nearby-share/1`) on iOS,
/// pairing Rust's `NearbyHost` (LAN TCP + blind relay) with Apple AWDL `Network.framework`
/// peer-to-peer bridges and lifecycle teardown on background or lock.
@MainActor @Observable
final class NearbyShareModel {
    struct Failure: Decodable, Equatable {
        let code: String
        let message: String
    }

    struct FileSummary: Decodable, Equatable, Identifiable {
        let index: UInt32
        let name: String
        let mime: String
        let size: UInt64
        var id: UInt32 { index }
    }

    struct Progress: Decodable, Equatable {
        let totalItems: UInt32
        let completedItems: UInt32
        let totalBytes: UInt64
        let bytes: UInt64
        let currentName: String?
    }

    struct PendingPeer: Decodable, Equatable, Identifiable {
        let peerId: String
        let sas: String
        let qrBound: Bool
        let transport: String
        let kind: String
        let files: [FileSummary]
        let packageSize: UInt64?
        let alias: String?
        var id: String { peerId }
    }

    struct ReceivedFile: Decodable, Equatable, Identifiable {
        let index: UInt32
        let fileId: String
        let versionId: String
        let name: String
        let size: UInt64
        var id: String { fileId }
    }

    struct PeerIdentity: Decodable, Equatable {
        let environment: String
        let account: String
        let device: String
        let handle: String?
        let status: String
    }

    struct ReceiveOutcome: Decodable, Equatable {
        let kind: String
        let files: [ReceivedFile]
        let packagePreview: SecretPackagePreview?
        let identity: PeerIdentity?

        static func == (lhs: ReceiveOutcome, rhs: ReceiveOutcome) -> Bool {
            lhs.kind == rhs.kind && lhs.files == rhs.files && lhs.packagePreview?.digest == rhs.packagePreview?.digest && lhs.identity == rhs.identity
        }
    }

    struct SendOutcome: Decodable, Equatable {
        let kind: String
        let sentItems: UInt32
        let bytes: UInt64
        let packageDigest: String?
        let identity: PeerIdentity?
    }

    struct ReceiveStatus: Decodable, Equatable {
        let id: UInt64
        let state: String
        let alias: String
        let discoverable: Bool
        let advert: String
        let pkHint: String
        let port: UInt16
        let addrs: [String]
        let offer: String
        let expiresAt: UInt64
        let relayReady: Bool
        let transport: String?
        let pendingPeers: [PendingPeer]
        let verificationCodes: [String]?
        let activePeer: String?
        let progress: Progress
        let outcome: ReceiveOutcome?
        let error: Failure?
        var active: Bool { state == "waiting" || state == "running" }
    }

    struct SendStatus: Decodable, Equatable {
        let id: UInt64
        let state: String
        let kind: String
        let transport: String?
        let sas: String?
        let transcript: String?
        let qrBound: Bool
        let peerLabel: String
        let shareJobId: String?
        let progress: Progress
        let outcome: SendOutcome?
        let error: Failure?
        var active: Bool {
            ["connecting", "awaiting_sas", "awaiting_accept", "running"].contains(state)
        }
    }

    struct Status: Decodable, Equatable {
        let receive: ReceiveStatus?
        let send: SendStatus?
    }

    struct ParsedOffer: Decodable {
        let offer: String
        let expiresAt: UInt64
        let addrs: [String]
        let advert: String?
        let hasRelay: Bool
        let pkHint: String
    }

    private(set) var status: Status?
    private(set) var peers: [NearbyDiscoveredPeer] = []
    private(set) var browsing = false
    private(set) var working = false
    private(set) var localNetworkDenied = false
    private(set) var error: String?

    private var listener: NearbyAWDLListener?
    private var browser: NearbyAWDLBrowser?
    private var connector: NearbyAWDLConnector?
    private var watcher: Task<Void, Never>?
    private var epoch = 0
    private var reloadedFilesForReceiveId: UInt64?

    nonisolated static func groupedSas(_ sas: String) -> String {
        guard sas.count == 6 else { return sas }
        let idx = sas.index(sas.startIndex, offsetBy: 3)
        // Keep the groups in protocol order when embedded in right-to-left text.
        return "\(sas[..<idx])\u{00a0}\(sas[idx...])"
    }

    func startReceive(_ model: SessionModel, discoverable: Bool) async {
        guard let engine = model.engine else { return }
        working = true
        error = nil
        localNetworkDenied = false
        listener?.stop()
        listener = nil
        defer { working = false }
        let alias = UIDevice.current.name
        do {
            let snap = try await engine.nearbyReceiveStart(alias: alias, discoverable: discoverable)
            let decoded: Status = try model.decode(snap)
            status = decoded
            if discoverable, let recv = decoded.receive, recv.active {
                listener = NearbyAWDLListener(
                    advert: recv.advert,
                    alias: recv.alias,
                    pkHint: recv.pkHint,
                    engine: engine
                ) { [weak self] ok in
                    if !ok {
                        Task { @MainActor in self?.localNetworkDenied = true }
                    }
                }
            }
            watch(model)
        } catch {
            self.error = error.localizedDescription
            model.report(error)
        }
    }

    func accept(_ model: SessionModel, peerId: String, folder: String, selected: [UInt32], shareIdentity: Bool) {
        guard let engine = model.engine else { return }
        error = nil
        do {
            let snap = try engine.nearbyReceiveAccept(peerId: peerId, folder: folder, selected: selected, shareIdentity: shareIdentity)
            status = try model.decode(snap)
            watch(model)
        } catch {
            self.error = error.localizedDescription
            model.report(error)
        }
    }

    func reject(_ model: SessionModel, peerId: String) {
        guard let engine = model.engine else { return }
        error = nil
        do {
            let snap = try engine.nearbyReceiveReject(peerId: peerId)
            status = try model.decode(snap)
        } catch {
            self.error = error.localizedDescription
            model.report(error)
        }
    }

    func cancelReceive(_ model: SessionModel) async {
        listener?.stop()
        listener = nil
        if let engine = model.engine, let snap = try? await engine.nearbyReceiveCancel() {
            status = try? model.decode(snap)
        }
        updateIdleTimer()
    }

    func startBrowse() {
        stopBrowse()
        localNetworkDenied = false
        browsing = true
        let ownAdvert = status?.receive?.advert
        browser = NearbyAWDLBrowser(
            onPeers: { [weak self] discovered in
                let filtered = discovered.filter { $0.advert != ownAdvert }
                Task { @MainActor in
                    self?.peers = filtered
                }
            },
            onDenied: { [weak self] in
                Task { @MainActor in
                    self?.localNetworkDenied = true
                }
            }
        )
    }

    func stopBrowse() {
        browser?.stop()
        browser = nil
        browsing = false
    }

    func sendFiles(
        _ model: SessionModel,
        offer: String?,
        peer: NearbyDiscoveredPeer?,
        files: [NearbyFileItem],
        shareIdentity: Bool
    ) async {
        guard let engine = model.engine else { return }
        working = true
        error = nil
        connector?.stop()
        connector = nil
        defer { working = false }
        do {
            var bridgeId: UInt64? = nil
            if let peer {
                if let conn = NearbyAWDLConnector(advert: peer.advert, engine: engine) {
                    connector = conn
                    bridgeId = conn.bridgeId
                }
            } else if let offer,
                      let parsedJson = try? nearbyParseOffer(text: offer, expectedEnv: nil),
                      let parsed = try? makeDecoder().decode(ParsedOffer.self, from: Data(parsedJson.utf8)),
                      let adv = parsed.advert,
                      let conn = NearbyAWDLConnector(advert: adv, engine: engine) {
                connector = conn
                bridgeId = conn.bridgeId
            }
            let snap = try await engine.nearbySendFiles(
                offer: offer,
                host: nil,
                port: nil,
                peerAlias: peer?.alias,
                resolvedHost: nil,
                resolvedPort: nil,
                awdlBridgeId: bridgeId,
                files: files,
                alias: UIDevice.current.name,
                shareIdentity: shareIdentity
            )
            status = try model.decode(snap)
            watch(model)
        } catch {
            connector?.stop()
            connector = nil
            self.error = error.localizedDescription
            model.report(error)
        }
    }

    func sendVault(
        _ model: SessionModel,
        offer: String?,
        peer: NearbyDiscoveredPeer?,
        input: SecretSnapshotInput,
        shareIdentity: Bool
    ) async {
        guard let engine = model.engine else { return }
        working = true
        error = nil
        connector?.stop()
        connector = nil
        defer { working = false }
        do {
            var bridgeId: UInt64? = nil
            if let peer {
                if let conn = NearbyAWDLConnector(advert: peer.advert, engine: engine) {
                    connector = conn
                    bridgeId = conn.bridgeId
                }
            } else if let offer,
                      let parsedJson = try? nearbyParseOffer(text: offer, expectedEnv: nil),
                      let parsed = try? makeDecoder().decode(ParsedOffer.self, from: Data(parsedJson.utf8)),
                      let adv = parsed.advert,
                      let conn = NearbyAWDLConnector(advert: adv, engine: engine) {
                connector = conn
                bridgeId = conn.bridgeId
            }
            let snap = try await engine.nearbySendVault(
                offer: offer,
                host: nil,
                port: nil,
                peerAlias: peer?.alias,
                resolvedHost: nil,
                resolvedPort: nil,
                awdlBridgeId: bridgeId,
                input: input,
                alias: UIDevice.current.name,
                shareIdentity: shareIdentity
            )
            status = try model.decode(snap)
            watch(model)
        } catch {
            connector?.stop()
            connector = nil
            self.error = error.localizedDescription
            model.report(error)
        }
    }

    func confirmSas(_ model: SessionModel) {
        guard let engine = model.engine else { return }
        do {
            status = try model.decode(try engine.nearbySendConfirmSas())
        } catch {
            self.error = error.localizedDescription
            model.report(error)
        }
    }

    func cancelSend(_ model: SessionModel) {
        connector?.stop()
        connector = nil
        if let engine = model.engine {
            status = try? model.decode(engine.nearbySendCancel())
        }
        updateIdleTimer()
    }

    func background() {
        epoch += 1
        watcher?.cancel()
        watcher = nil
        listener?.stop()
        listener = nil
        stopBrowse()
        connector?.stop()
        connector = nil
        status = nil
        peers = []
        UIApplication.shared.isIdleTimerDisabled = false
    }

    func reset() {
        background()
        error = nil
        localNetworkDenied = false
        reloadedFilesForReceiveId = nil
    }

    private func watch(_ model: SessionModel) {
        epoch += 1
        let current = epoch
        watcher?.cancel()
        updateIdleTimer()
        watcher = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(350))
                guard let self, current == self.epoch, let engine = model.engine else { return }
                if let next: Status = try? model.decode(engine.nearbyStatus()) {
                    self.status = next
                    if let recv = next.receive, !recv.active, recv.progress.completedItems > 0,
                       recv.outcome?.kind != "secret_package",
                       self.reloadedFilesForReceiveId != recv.id {
                        self.reloadedFilesForReceiveId = recv.id
                        await model.loadFiles()
                    }
                    if next.receive?.active != true {
                        self.listener?.stop()
                        self.listener = nil
                    }
                    if next.send?.active != true {
                        self.connector?.stop()
                        self.connector = nil
                    }
                    self.updateIdleTimer()
                    if next.receive?.active != true && next.send?.active != true {
                        return
                    }
                }
            }
        }
    }

    private func updateIdleTimer() {
        let active = status?.receive?.active == true || status?.send?.active == true
        UIApplication.shared.isIdleTimerDisabled = active
    }

    private func makeDecoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }
}
