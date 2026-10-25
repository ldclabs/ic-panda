import Foundation
@preconcurrency import DMsgCore

/// UniFFI's future alone does not propagate Swift task cancellation. This
/// private adapter ties one explicit Rust cancellation token to the UI task.
@MainActor
func coreOperation<T>(_ work: (OperationToken) async throws -> T) async throws -> T {
    let token = OperationToken()
    return try await withTaskCancellationHandler {
        try Task.checkCancellation()
        return try await work(token)
    } onCancel: { token.cancel() }
}
