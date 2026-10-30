import dnssd
import Foundation

/// Bonjour for device transfers (`_dmsg-xfer._tcp`) through DNS Service
/// Discovery: the receiving device advertises its session under a name only
/// a device that knows the session can find, and the sender finds it by that
/// name. Every reply arrives on the main queue.
enum Bonjour {
    static let service = "_dmsg-xfer._tcp"

    enum Failure: Error, Equatable {
        /// Local network access is off for dMsg.
        case denied
        /// Nothing answered in time; the person types the address in.
        case notFound
        case failed(Int32)

        init(_ code: DNSServiceErrorType) {
            self = Int(code) == kDNSServiceErr_PolicyDenied ? .denied : .failed(code)
        }
    }

    /// Advertise session name `name` at `port`, once the system confirms
    /// it. The first use asks the person for local network access.
    @MainActor static func advertise(_ name: String, port: UInt16) async throws -> BonjourAdvert {
        let request = Request(name: name)
        // The TXT record `v=1`.
        let txt: [UInt8] = [3, 0x76, 0x3d, 0x31]
        do {
            _ = try await request.run(timeout: .seconds(60)) { ref in
                DNSServiceRegister(ref, 0, 0, name, service, nil, nil, port.bigEndian,
                                   UInt16(txt.count), txt, registered, request.context)
            }
        } catch { request.stop(); throw error }
        return BonjourAdvert(request)
    }

    /// The host and port advertised as `name`, found within `timeout`.
    @MainActor static func find(_ name: String, timeout: Duration = .seconds(15)) async throws -> (host: String, port: UInt16) {
        let browsing = Request(name: name)
        defer { browsing.stop() }
        let found = try await browsing.run(timeout: timeout) { ref in
            DNSServiceBrowse(ref, 0, 0, service, nil, browsed, browsing.context)
        }
        guard case .found(let interface, let kind, let domain) = found else { throw Failure.notFound }
        browsing.stop()
        let resolving = Request(name: name)
        defer { resolving.stop() }
        let resolved = try await resolving.run(timeout: timeout) { ref in
            DNSServiceResolve(ref, 0, interface, name, kind, domain, resolvedReply, resolving.context)
        }
        guard case .resolved(let host, let port) = resolved else { throw Failure.notFound }
        return (host, port)
    }
}

/// An advertised session; `stop()` withdraws it.
@MainActor final class BonjourAdvert {
    private let request: Request
    fileprivate init(_ request: Request) { self.request = request }
    func stop() { request.stop() }
}

private let noError = DNSServiceErrorType(kDNSServiceErr_NoError)

private enum Answer: Sendable {
    case registered
    case found(UInt32, String, String)
    case resolved(String, UInt16)
}

/// One DNS-SD operation and the first answer it waits for. Its replies run
/// on the main queue, as does everything that touches it.
private final class Request: @unchecked Sendable {
    let name: String
    private var ref: DNSServiceRef?
    private var waiting: CheckedContinuation<Answer, Error>?
    private var answer: Result<Answer, Error>?

    init(name: String) { self.name = name }
    deinit { stop() }

    var context: UnsafeMutableRawPointer { Unmanaged.passUnretained(self).toOpaque() }

    static func of(_ context: UnsafeMutableRawPointer?) -> Request? {
        context.map { Unmanaged<Request>.fromOpaque($0).takeUnretainedValue() }
    }

    func reply(_ code: DNSServiceErrorType, _ value: Answer?) {
        if code != noError { finish(.failure(Bonjour.Failure(code))) }
        else if let value { finish(.success(value)) }
    }

    private func finish(_ result: Result<Answer, Error>) {
        guard answer == nil else { return }
        answer = result
        waiting?.resume(with: result); waiting = nil
    }

    /// Start the operation with `start`, which fills in the reference, and
    /// wait up to `timeout` for its first answer.
    @MainActor func run(timeout: Duration,
                        _ start: (UnsafeMutablePointer<DNSServiceRef?>) -> DNSServiceErrorType) async throws -> Answer {
        let code = start(&ref)
        guard code == noError, let ref else { ref = nil; throw Bonjour.Failure(code) }
        DNSServiceSetDispatchQueue(ref, .main)
        let timer = Task { @MainActor in
            do { try await Task.sleep(for: timeout) } catch { return }
            self.finish(.failure(Bonjour.Failure.notFound))
        }
        defer { timer.cancel() }
        return try await withCheckedThrowingContinuation { continuation in
            if let answer { continuation.resume(with: answer) } else { waiting = continuation }
        }
    }

    /// End the operation; no reply comes after.
    func stop() {
        if let ref { DNSServiceRefDeallocate(ref) }
        ref = nil
        finish(.failure(CancellationError()))
    }
}

private let registered: DNSServiceRegisterReply = { _, _, code, _, _, _, context in
    Request.of(context)?.reply(code, .registered)
}

private let browsed: DNSServiceBrowseReply = { _, flags, interface, code, name, kind, domain, context in
    guard let request = Request.of(context) else { return }
    guard code == noError else { return request.reply(code, nil) }
    guard flags & DNSServiceFlags(kDNSServiceFlagsAdd) != 0, let name, let kind, let domain,
          String(cString: name) == request.name else { return }
    request.reply(code, .found(interface, String(cString: kind), String(cString: domain)))
}

private let resolvedReply: DNSServiceResolveReply = { _, _, _, code, _, host, port, _, _, context in
    guard let request = Request.of(context) else { return }
    guard code == noError, let host else { return request.reply(code, nil) }
    request.reply(code, .resolved(String(cString: host), UInt16(bigEndian: port)))
}
