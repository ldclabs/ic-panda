import Foundation
import Network

/// Apple AWDL (Network.framework `includePeerToPeer = true`) listener, browser
/// and framed connection bridge for `dmsg-nearby-share/1` inside the macOS daemon.
///
/// Apple provides no supported BSD socket path for AWDL (TN3151), so the daemon
/// hosts `NWListener`/`NWBrowser`/`NWConnection` here and hands 4-byte
/// length-prefixed frames to Rust's `BridgeTransport`.

private let nearbyServiceType = "_dmsg-share._tcp"
private let maxFrameBytes = 1024 * 1024 + 64 * 1024 + 64

private typealias AcceptCallback = @convention(c) (UInt64, UnsafeMutableRawPointer?) -> Void
private typealias FrameCallback = @convention(c) (UInt64, UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?) -> Void
private typealias BrowseCallback = @convention(c) (
    UnsafePointer<CChar>?,
    UnsafePointer<CChar>?,
    UnsafePointer<CChar>?,
    UnsafeMutableRawPointer?
) -> Void

private func p2pParameters() -> NWParameters {
    let params = NWParameters.tcp
    params.includePeerToPeer = true
    return params
}

private final class AwdlState {
    static let shared = AwdlState()
    let lock = NSLock()
    let queue = DispatchQueue(label: "net.dmsg.nearby.awdl", qos: .userInitiated)
    var nextId: UInt64 = 1
    var listeners: [UInt64: NWListener] = [:]
    var connections: [UInt64: AwdlConnection] = [:]

    func allocId() -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        let id = nextId
        nextId &+= 1
        return id
    }
}

private final class AwdlConnection {
    let id: UInt64
    let conn: NWConnection
    let onFrame: FrameCallback
    let ctx: UnsafeMutableRawPointer?
    private let stateLock = NSLock()
    private var closed = false

    init(id: UInt64, conn: NWConnection, onFrame: @escaping FrameCallback, ctx: UnsafeMutableRawPointer?) {
        self.id = id
        self.conn = conn
        self.onFrame = onFrame
        self.ctx = ctx
    }

    func start() {
        conn.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready:
                self.readHeader()
            case .failed, .cancelled:
                self.close()
            default:
                break
            }
        }
        conn.start(queue: AwdlState.shared.queue)
    }

    private func readHeader() {
        conn.receive(minimumIncompleteLength: 4, maximumLength: 4) { [weak self] data, _, isComplete, error in
            guard let self else { return }
            guard error == nil, let data, data.count == 4 else {
                self.close()
                return
            }
            let len = data.withUnsafeBytes { raw -> Int in
                let be = raw.load(as: UInt32.self)
                return Int(UInt32(bigEndian: be))
            }
            guard len > 0, len <= maxFrameBytes else {
                self.close()
                return
            }
            self.readBody(length: len)
            if isComplete {
                // body read will surface EOF if truncated
            }
        }
    }

    private func readBody(length: Int) {
        conn.receive(minimumIncompleteLength: length, maximumLength: length) { [weak self] data, _, _, error in
            guard let self else { return }
            guard error == nil, let data, data.count == length else {
                self.close()
                return
            }
            data.withUnsafeBytes { raw in
                if let ptr = raw.bindMemory(to: UInt8.self).baseAddress {
                    self.onFrame(self.id, ptr, length, self.ctx)
                }
            }
            self.readHeader()
        }
    }

    func send(bytes: Data) -> Int32 {
        stateLock.lock()
        let isClosed = closed
        stateLock.unlock()
        if isClosed || bytes.count > maxFrameBytes { return -1 }
        var framed = Data(capacity: 4 + bytes.count)
        var beLen = UInt32(bytes.count).bigEndian
        withUnsafeBytes(of: &beLen) { framed.append(contentsOf: $0) }
        framed.append(bytes)
        conn.send(content: framed, completion: .contentProcessed { [weak self] error in
            if error != nil { self?.close() }
        })
        return 0
    }

    func close() {
        stateLock.lock()
        if closed {
            stateLock.unlock()
            return
        }
        closed = true
        stateLock.unlock()

        AwdlState.shared.lock.lock()
        AwdlState.shared.connections.removeValue(forKey: id)
        AwdlState.shared.lock.unlock()

        conn.cancel()
        onFrame(id, nil, 0, ctx)
    }
}

@_cdecl("dmsg_awdl_listen_start")
public func dmsg_awdl_listen_start(
    _ instance: UnsafePointer<CChar>,
    _ alias: UnsafePointer<CChar>,
    _ pkHint: UnsafePointer<CChar>,
    _ onAccept: UnsafeRawPointer,
    _ onFrame: UnsafeRawPointer,
    _ ctx: UnsafeMutableRawPointer?
) -> UInt64 {
    let name = String(cString: instance)
    let aliasStr = String(cString: alias)
    let hintStr = String(cString: pkHint)
    let acceptCb = unsafeBitCast(onAccept, to: AcceptCallback.self)
    let frameCb = unsafeBitCast(onFrame, to: FrameCallback.self)

    guard let listener = try? NWListener(using: p2pParameters()) else {
        return 0
    }
    var txt = NWTXTRecord()
    txt["v"] = "1"
    txt["n"] = aliasStr
    txt["p"] = hintStr
    listener.service = NWListener.Service(name: name, type: nearbyServiceType, domain: nil, txtRecord: txt)

    let listenerId = AwdlState.shared.allocId()
    listener.newConnectionHandler = { conn in
        let connId = AwdlState.shared.allocId()
        let wrapped = AwdlConnection(id: connId, conn: conn, onFrame: frameCb, ctx: ctx)
        AwdlState.shared.lock.lock()
        AwdlState.shared.connections[connId] = wrapped
        AwdlState.shared.lock.unlock()
        acceptCb(connId, ctx)
        wrapped.start()
    }
    listener.stateUpdateHandler = { state in
        if case .failed = state {
            AwdlState.shared.lock.lock()
            AwdlState.shared.listeners.removeValue(forKey: listenerId)
            AwdlState.shared.lock.unlock()
            listener.cancel()
        }
    }

    AwdlState.shared.lock.lock()
    AwdlState.shared.listeners[listenerId] = listener
    AwdlState.shared.lock.unlock()
    listener.start(queue: AwdlState.shared.queue)
    return listenerId
}

@_cdecl("dmsg_awdl_listen_stop")
public func dmsg_awdl_listen_stop(_ listenerId: UInt64) {
    AwdlState.shared.lock.lock()
    let listener = AwdlState.shared.listeners.removeValue(forKey: listenerId)
    AwdlState.shared.lock.unlock()
    listener?.cancel()
}

@_cdecl("dmsg_awdl_connect")
public func dmsg_awdl_connect(
    _ instance: UnsafePointer<CChar>,
    _ onFrame: UnsafeRawPointer,
    _ ctx: UnsafeMutableRawPointer?
) -> UInt64 {
    let name = String(cString: instance)
    let frameCb = unsafeBitCast(onFrame, to: FrameCallback.self)
    let endpoint = NWEndpoint.service(name: name, type: nearbyServiceType, domain: "local.", interface: nil)
    let conn = NWConnection(to: endpoint, using: p2pParameters())
    let connId = AwdlState.shared.allocId()
    let wrapped = AwdlConnection(id: connId, conn: conn, onFrame: frameCb, ctx: ctx)
    AwdlState.shared.lock.lock()
    AwdlState.shared.connections[connId] = wrapped
    AwdlState.shared.lock.unlock()
    wrapped.start()
    return connId
}

@_cdecl("dmsg_awdl_conn_send")
public func dmsg_awdl_conn_send(_ connId: UInt64, _ data: UnsafePointer<UInt8>, _ length: Int) -> Int32 {
    AwdlState.shared.lock.lock()
    let wrapped = AwdlState.shared.connections[connId]
    AwdlState.shared.lock.unlock()
    guard let wrapped, length > 0 else { return -1 }
    return wrapped.send(bytes: Data(bytes: data, count: length))
}

@_cdecl("dmsg_awdl_conn_close")
public func dmsg_awdl_conn_close(_ connId: UInt64) {
    AwdlState.shared.lock.lock()
    let wrapped = AwdlState.shared.connections[connId]
    AwdlState.shared.lock.unlock()
    wrapped?.close()
}

@_cdecl("dmsg_awdl_browse")
public func dmsg_awdl_browse(
    _ timeoutMs: UInt32,
    _ onPeer: UnsafeRawPointer,
    _ ctx: UnsafeMutableRawPointer?
) -> Int32 {
    let cb = unsafeBitCast(onPeer, to: BrowseCallback.self)
    let descriptor = NWBrowser.Descriptor.bonjourWithTXTRecord(type: nearbyServiceType, domain: nil)
    let browser = NWBrowser(for: descriptor, using: p2pParameters())
    let sem = DispatchSemaphore(value: 0)
    let seenLock = NSLock()
    var seen: [String: (String, String)] = [:]

    browser.browseResultsChangedHandler = { results, _ in
        seenLock.lock()
        for result in results {
            if case .service(let name, _, _, _) = result.endpoint {
                var alias = "dMsg Device"
                var hint = ""
                if case .bonjour(let txt) = result.metadata {
                    if txt["v"] != "1" { continue }
                    if let n = txt["n"], !n.isEmpty { alias = n }
                    if let p = txt["p"] { hint = p }
                }
                seen[name] = (alias, hint)
            }
        }
        seenLock.unlock()
    }
    browser.stateUpdateHandler = { state in
        if case .failed = state { sem.signal() }
    }
    browser.start(queue: AwdlState.shared.queue)
    let clamped = min(max(Int(timeoutMs), 200), 3000)
    _ = sem.wait(timeout: .now() + .milliseconds(clamped))
    browser.cancel()

    seenLock.lock()
    let snapshot = seen
    seenLock.unlock()
    for (name, (alias, hint)) in snapshot {
        name.withCString { nPtr in
            alias.withCString { aPtr in
                hint.withCString { hPtr in
                    cb(nPtr, aPtr, hPtr, ctx)
                }
            }
        }
    }
    return Int32(snapshot.count)
}
