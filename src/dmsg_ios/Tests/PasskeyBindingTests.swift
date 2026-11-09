import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class PasskeyBindingTests: XCTestCase {
    private func start(_ operation: PasskeyOperation = .bind) -> PasskeyStart {
        PasskeyStart(state: Data([1]), challenge: Data([2]), userId: Data([3]), credentialId: nil, operation: operation, deadlineMs: UInt64.max)
    }
    private var response: NativePasskeyResponse {
        NativePasskeyResponse(credentialId: Data([4]), clientDataJson: Data([5]), authenticatorData: Data([6]), signature: Data([7]))
    }

    func testTransientFailuresRetryTheSavedResponseWithoutAnotherSystemPrompt() async throws {
        for operation in [PasskeyOperation.bind, .bindExisting] {
            let flow = PasskeyBinding()
            var starts = 0, authentications = 0, completions = 0
            let failures = ["NETWORK_UNAVAILABLE", "EXECUTION_UNKNOWN", "HARDWARE_UNAVAILABLE", "LOCAL_STORAGE_UNAVAILABLE"]
            for attempt in 0...failures.count {
                do {
                    let result = try await flow.run {
                        starts += 1; return self.start(operation)
                    } authenticate: { _ in
                        authentications += 1; return self.response
                    } complete: { start, response in
                        completions += 1
                        XCTAssertTrue(flow.working)
                        XCTAssertEqual(start.state, Data([1])); XCTAssertEqual(start.operation, operation)
                        XCTAssertEqual(response.credentialId, Data([4]))
                        if attempt < failures.count { throw MobileError.Failure(code: failures[attempt]) }
                        return ObjectSnapshot(generation: 7, json: "{}")
                    }
                    XCTAssertEqual(result.generation, 7)
                    XCTAssertEqual(attempt, failures.count)
                } catch { XCTAssertLessThan(attempt, failures.count) }
                XCTAssertEqual(flow.hasSavedCredential, attempt < failures.count)
                XCTAssertFalse(flow.working)
            }
            XCTAssertEqual(starts, 1); XCTAssertEqual(authentications, 1); XCTAssertEqual(completions, failures.count + 1)
        }
    }

    func testPreflightFailureDoesNotClaimASavedCredential() async {
        let flow = PasskeyBinding()
        do {
            _ = try await flow.run { throw MobileError.Failure(code: "NETWORK_UNAVAILABLE") } authenticate: { _ in
                XCTFail("No system prompt before preflight succeeds"); return self.response
            } complete: { _, _ in XCTFail("No completion"); return ObjectSnapshot(generation: 0, json: "{}") }
            XCTFail("Expected failure")
        } catch {}
        XCTAssertFalse(flow.working); XCTAssertFalse(flow.hasSavedCredential)
    }

    func testLeavingDuringAuthenticationDiscardsTheLateResponse() async {
        let flow = PasskeyBinding()
        var answer: CheckedContinuation<NativePasskeyResponse, Error>?
        let task = Task {
            try await flow.run { self.start() } authenticate: { _ in
                try await withCheckedThrowingContinuation { answer = $0 }
            } complete: { _, _ in XCTFail("Cancelled response must not be submitted"); return ObjectSnapshot(generation: 0, json: "{}") }
        }
        while answer == nil { await Task.yield() }
        flow.cancel()
        answer?.resume(returning: response)
        do { _ = try await task.value; XCTFail("Expected cancellation") } catch { XCTAssertTrue(error is CancellationError) }
        XCTAssertFalse(flow.working); XCTAssertFalse(flow.hasSavedCredential)
    }

    func testPermanentFailureClearsTheSavedAttemptAndNamesDistinguishNewCredentials() async {
        let flow = PasskeyBinding()
        do {
            _ = try await flow.run { self.start() } authenticate: { _ in self.response } complete: { _, _ in
                throw MobileError.Failure(code: "EXPIRED")
            }
        } catch {}
        XCTAssertFalse(flow.hasSavedCredential)
        XCTAssertEqual(PasskeyLogin.credentialName(Data([1, 2, 3, 4, 5])), "dMsg · 01020304")
        XCTAssertNotEqual(PasskeyLogin.credentialName(Data([1, 2, 3, 4])), PasskeyLogin.credentialName(Data([5, 6, 7, 8])))
    }

    func testLeavingDuringConfirmationDiscardsALateApproval() async {
        let flow = PasskeyBinding()
        var answer: CheckedContinuation<ObjectSnapshot, Error>?
        let task = Task {
            try await flow.run { self.start() } authenticate: { _ in self.response } complete: { _, _ in
                try await withCheckedThrowingContinuation { answer = $0 }
            }
        }
        while answer == nil { await Task.yield() }
        XCTAssertTrue(flow.hasSavedCredential)
        flow.cancel()
        answer?.resume(returning: ObjectSnapshot(generation: 1, json: "{}"))
        do { _ = try await task.value; XCTFail("Late approval must not be displayed") } catch { XCTAssertTrue(error is CancellationError) }
        XCTAssertFalse(flow.working); XCTAssertFalse(flow.hasSavedCredential)
    }
}
