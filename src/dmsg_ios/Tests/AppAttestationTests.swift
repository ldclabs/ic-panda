import CryptoKit
import DeviceCheck
import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class AppAttestationTests: XCTestCase {
    private func file() throws -> URL {
        let directory = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: directory) }
        return directory.appending(path: "key.json")
    }

    private func challenge(expires: UInt64? = nil) -> NativeApple.Challenge {
        .init(challengeId: "original-challenge", challenge: Data(repeating: 7, count: 32).base64EncodedString(),
              clientData: "", expiresAt: expires ?? UInt64(Date.now.addingTimeInterval(300).timeIntervalSince1970 * 1000))
    }

    private func service() -> AppAttestation.Service {
        .init(status: { _ in false }, challenge: { _ in self.challenge() }, register: { _, _, _ in },
              generate: { "key" }, attest: { _, _ in Data("attestation".utf8) })
    }

    func testAppleOutageAndRestartReuseTheKeyAndOriginalChallenge() async throws {
        let path = try file()
        var service = service(), keys = 0, challenges = 0, hashes: [Data] = []
        service.generate = { keys += 1; return "key" }
        service.challenge = { _ in challenges += 1; return self.challenge() }
        service.attest = { key, hash in
            XCTAssertEqual(key, "key")
            hashes.append(hash)
            if hashes.count == 1 { throw NSError(domain: DCErrorDomain, code: DCError.Code.serverUnavailable.rawValue) }
            return Data("attestation".utf8)
        }
        do { _ = try await AppAttestation.key(device: "device", path: path, service: service); XCTFail("Apple is unavailable") } catch {}
        // All recovery state comes from disk, as after a process restart.
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "key")
        XCTAssertEqual(keys, 1)
        XCTAssertEqual(challenges, 1)
        XCTAssertEqual(hashes, Array(repeating: Data(SHA256.hash(data: Data(repeating: 7, count: 32))), count: 2))
        let saved = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: path)) as? [String: Any])
        XCTAssertEqual(Set(saved.keys), ["id", "device"], "Registration clears the temporary proof")
    }

    func testLostRegistrationResponseIsReconciledBeforeReportingFailure() async throws {
        let path = try file()
        var service = service(), registered = false, registrations = 0, reads = 0
        service.register = { _, _, _ in
            registrations += 1; registered = true
            throw MobileError.Failure(code: "EXECUTION_UNKNOWN")
        }
        service.status = { _ in reads += 1; return registered }
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "key")
        XCTAssertEqual(registrations, 1)
        XCTAssertEqual(reads, 1)
    }

    func testAnUnresolvedRegistrationKeepsTheExactProofForTheNextAttempt() async throws {
        let path = try file()
        var service = service(), attestations = 0, proofs: [String] = [], challenges: [String] = []
        service.attest = { _, _ in attestations += 1; return Data("one-time-proof".utf8) }
        service.register = { _, challenge, object in
            proofs.append(object); challenges.append(challenge)
            if proofs.count == 1 { throw MobileError.Failure(code: "EXECUTION_UNKNOWN") }
        }
        do { _ = try await AppAttestation.key(device: "device", path: path, service: service); XCTFail("No confirmed registration") } catch {}
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "key")
        XCTAssertEqual(attestations, 1, "Apple attestation is one-time")
        XCTAssertEqual(proofs, Array(repeating: Data("one-time-proof".utf8).base64EncodedString(), count: 2))
        XCTAssertEqual(challenges, ["original-challenge", "original-challenge"])
    }

    func testRegisteredKeyWinsOverAnExpiredSavedChallenge() async throws {
        let path = try file()
        try Data(#"{"id":"old-key","device":"device","challenge":"old","object":"proof","expires":1}"#.utf8).write(to: path)
        var service = service()
        service.status = { key in XCTAssertEqual(key, "old-key"); return true }
        service.generate = { XCTFail("A committed registration must not be rotated"); return "new" }
        service.attest = { _, _ in XCTFail("Already registered"); return Data() }
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "old-key")
        XCTAssertFalse(try String(contentsOf: path, encoding: .utf8).contains("proof"))
    }

    func testExpiredUnregisteredKeyIsReplacedButStatusOutageCannotRotateIt() async throws {
        let path = try file()
        let old = Data(#"{"id":"old-key","device":"device","challenge":"old","object":"proof","expires":1}"#.utf8)
        try old.write(to: path)
        var service = service(), keys = 0
        service.generate = { keys += 1; return "new-key" }
        service.status = { _ in throw URLError(.notConnectedToInternet) }
        do { _ = try await AppAttestation.key(device: "device", path: path, service: service); XCTFail("Cannot infer absence") } catch {}
        XCTAssertEqual(try Data(contentsOf: path), old)
        XCTAssertEqual(keys, 0)
        service.status = { _ in false }
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "new-key")
        XCTAssertEqual(keys, 1)
    }

    func testOldSavedAttestationWithoutNonceStillResumesRegistration() async throws {
        let path = try file()
        let expiry = UInt64(Date.now.addingTimeInterval(300).timeIntervalSince1970 * 1000)
        try Data("{\"id\":\"old-key\",\"device\":\"device\",\"challenge\":\"old\",\"object\":\"proof\",\"expires\":\(expiry)}".utf8).write(to: path)
        var service = service()
        service.attest = { _, _ in XCTFail("An existing one-time proof is sufficient"); return Data() }
        service.register = { key, challenge, object in
            XCTAssertEqual(key, "old-key"); XCTAssertEqual(challenge, "old"); XCTAssertEqual(object, "proof")
        }
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "old-key")
    }

    func testAppleInvalidKeyRetiresTheRecordAndNextAttemptCanEnroll() async throws {
        let path = try file()
        var service = service(), keys = 0
        service.generate = { keys += 1; return "key-\(keys)" }
        service.attest = { _, _ in throw NSError(domain: DCErrorDomain, code: DCError.Code.invalidKey.rawValue) }
        do { _ = try await AppAttestation.key(device: "device", path: path, service: service); XCTFail("Retired key") } catch {}
        XCTAssertFalse(FileManager.default.fileExists(atPath: path.path))
        service.attest = { _, _ in Data("new-proof".utf8) }
        let key = try await AppAttestation.key(device: "device", path: path, service: service)
        XCTAssertEqual(key, "key-2")
    }

    func testCancelledAppleResultIsNotRegistered() async throws {
        let path = try file()
        var service = service()
        let started = expectation(description: "Apple call started")
        var resume: CheckedContinuation<Data, Never>?
        service.attest = { _, _ in await withCheckedContinuation { resume = $0; started.fulfill() } }
        service.register = { _, _, _ in XCTFail("Cancelled enrollment must not submit") }
        let enrollment = Task { try await AppAttestation.key(device: "device", path: path, service: service) }
        await fulfillment(of: [started], timeout: 2)
        enrollment.cancel(); resume?.resume(returning: Data("late-proof".utf8))
        do { _ = try await enrollment.value; XCTFail("Cancelled") } catch { XCTAssertTrue(error is CancellationError) }
    }
}
