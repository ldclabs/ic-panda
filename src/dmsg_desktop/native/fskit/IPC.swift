// One bounded, cancellable request. No thread waits for a person's approval.
import Foundation
import Darwin

final class ContentRequest {
    private static let queue = DispatchQueue(label: "net.dmsg.secrets.ipc")
    private var file: FileHandle?
    private var header = Data()
    private var body = Data()
    private var expected: Int?
    private var finished = false
    private var timer: DispatchWorkItem?
    private let completion: (Result<[String: Any], Error>) -> Void

    init(path: String, request: [String: Any], completion: @escaping (Result<[String: Any], Error>) -> Void) {
        self.completion = completion
        Self.queue.async { self.start(path, request) }
    }

    func cancel() { Self.queue.async { self.finish(.failure(POSIXError(.ECANCELED))) } }

    private func start(_ path: String, _ request: [String: Any]) {
        guard !finished else { return }
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return finish(.failure(POSIXError(.EIO))) }
        var address = sockaddr_un(); address.sun_family = sa_family_t(AF_UNIX)
        let name = Array(path.utf8) + [0]
        guard name.count <= MemoryLayout.size(ofValue: address.sun_path) else { Darwin.close(fd); return finish(.failure(POSIXError(.ENAMETOOLONG))) }
        withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: name) }
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        var timeout = timeval(tv_sec: 2, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
        let connected = withUnsafePointer(to: &address) { pointer in pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) } }
        guard connected == 0 else {
            diagnostics.error("content service connect failed: errno \(errno, privacy: .public)")
            Darwin.close(fd); return finish(.failure(POSIXError(.ECONNREFUSED)))
        }
        var user: uid_t = 0; var group: gid_t = 0
        guard getpeereid(fd, &user, &group) == 0, user == getuid() else {
            diagnostics.error("content service peer is another user")
            Darwin.close(fd); return finish(.failure(POSIXError(.EACCES)))
        }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true); file = handle
        do {
            let payload = try JSONSerialization.data(withJSONObject: request)
            guard payload.count <= 65536 else { throw POSIXError(.E2BIG) }
            var size = UInt32(payload.count).bigEndian
            var wire = withUnsafeBytes(of: &size) { Data($0) }; wire.append(payload)
            try handle.write(contentsOf: wire)
            handle.readabilityHandler = { [weak self] handle in Self.queue.sync { self?.receive(handle) } }
            let timeout = DispatchWorkItem { [weak self] in self?.finish(.failure(POSIXError(.ETIMEDOUT))) }
            timer = timeout; Self.queue.asyncAfter(deadline: .now() + 35, execute: timeout)
        } catch { finish(.failure(POSIXError(.EIO))) }
    }

    private func receive(_ handle: FileHandle) {
        guard !finished else { return }
        do {
            let limit = expected.map { min(65536, $0 - body.count) } ?? (4 - header.count)
            guard limit > 0, let bytes = try handle.read(upToCount: limit), !bytes.isEmpty else { throw POSIXError(.EPIPE) }
            if expected == nil {
                header.append(bytes)
                if header.count == 4 {
                    let length = header.reduce(0) { ($0 << 8) | Int($1) }
                    guard (1...2097152).contains(length) else { throw POSIXError(.E2BIG) }
                    expected = length
                }
            } else { body.append(bytes) }
            if body.count == expected {
                guard let reply = try JSONSerialization.jsonObject(with: body) as? [String: Any] else { throw POSIXError(.EIO) }
                guard reply["ok"] as? Bool == true else { throw POSIXError(.EACCES) }
                finish(.success(reply["result"] as? [String: Any] ?? [:]))
            }
        } catch { finish(.failure(POSIXError(.EACCES))) }
    }

    private func finish(_ result: Result<[String: Any], Error>) {
        guard !finished else { return }; finished = true
        timer?.cancel(); timer = nil
        file?.readabilityHandler = nil
        if let fd = file?.fileDescriptor { Darwin.shutdown(fd, SHUT_RDWR) }
        try? file?.close(); file = nil
        body.resetBytes(in: 0..<body.count); body.removeAll()
        completion(result)
    }
}

func unsigned(_ value: Any?, maximum: UInt64 = 9_007_199_254_740_991) -> UInt64? {
    guard let n = value as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID(), n.doubleValue.isFinite,
          n.doubleValue >= 0, n.doubleValue.rounded(.down) == n.doubleValue,
          n.doubleValue <= Double(maximum) else { return nil }
    return n.uint64Value
}

// CLOCK_MONOTONIC_RAW includes suspend time on macOS; it is shared with the daemon.
func continuousMilliseconds() throws -> UInt64 {
    var time = timespec()
    guard clock_gettime(CLOCK_MONOTONIC_RAW, &time) == 0, time.tv_sec >= 0,
          (0..<1_000_000_000).contains(time.tv_nsec) else { throw POSIXError(.EIO) }
    return UInt64(time.tv_sec) * 1000 + UInt64(time.tv_nsec) / 1_000_000
}
