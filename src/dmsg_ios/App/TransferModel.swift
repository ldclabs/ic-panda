import Foundation
import Observation
@preconcurrency import DMsgCore

@MainActor @Observable
final class TransferModel {
    private(set) var label: String?
    private(set) var completed: UInt64 = 0
    private(set) var total: UInt64 = 0
    private var operation: OperationToken?
    var running: Bool { operation != nil }
    func cancel() { operation?.cancel() }

    func run<T>(_ label: String, work: (OperationToken) async throws -> T) async throws -> T {
        guard operation == nil else { throw MobileError.Failure(code: "PENDING") }
        let token = OperationToken()
        operation = token; self.label = label; completed = 0; total = 0
        let polling = Task {
            while !Task.isCancelled {
                let progress = token.progress(); completed = progress.completed; total = progress.total
                do { try await Task.sleep(for: .milliseconds(200)) } catch { return }
            }
        }
        defer { polling.cancel(); operation = nil; self.label = nil }
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return try await work(token)
        } onCancel: { token.cancel() }
    }
}
