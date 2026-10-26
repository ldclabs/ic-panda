import AuthenticationServices
import UIKit

@MainActor
final class WebLogin: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?
    private var generation: UInt64 = 0

    func authenticate(url: URL) async throws -> URL {
        cancel()
        let attempt = generation
        return try await withCheckedThrowingContinuation { continuation in
            let completion = Completion(continuation)
            let session = ASWebAuthenticationSession(url: url,
                callback: .https(host: "dmsg.net", path: "/_/ios/auth/callback")) { [weak self] callback, error in
                Task { @MainActor in if self?.generation == attempt { self?.session = nil } }
                if let callback { completion.finish(.success(callback)) }
                else if (error as? ASWebAuthenticationSessionError)?.code == .canceledLogin { completion.finish(.failure(CancellationError())) }
                else { completion.finish(.failure(error ?? CancellationError())) }
            }
            session.presentationContextProvider = self
            // II caches subnet keys in IndexedDB. Isolate each attempt from
            // persisted browser data; a failed cache must not poison retries.
            session.prefersEphemeralWebBrowserSession = true
            self.session = session
            if !session.start() { self.session = nil; completion.finish(.failure(CancellationError())) }
        }
    }

    func cancel() { generation &+= 1; let previous = session; session = nil; previous?.cancel() }

    private final class Completion: @unchecked Sendable {
        private let lock = NSLock()
        private var continuation: CheckedContinuation<URL, Error>?
        init(_ continuation: CheckedContinuation<URL, Error>) { self.continuation = continuation }
        func finish(_ result: Result<URL, Error>) {
            lock.lock(); let pending = continuation; continuation = nil; lock.unlock()
            pending?.resume(with: result)
        }
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows).first(where: \.isKeyWindow) ?? ASPresentationAnchor()
    }
}
