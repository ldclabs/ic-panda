import XCTest
import CryptoKit

final class HardwareVectorTests: XCTestCase {
    struct Vector: Decodable {
        let name: String
        let cbor_hex: String
        let sha256_hex: String
        let p256_public_hex: String
        let p256_signature_over_sha256_hex: String
    }

    private func bytes(_ hex: String) throws -> Data {
        guard hex.count.isMultiple(of: 2) else { throw CocoaError(.coderInvalidValue) }
        var result = Data()
        var index = hex.startIndex
        while index < hex.endIndex {
            let end = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<end], radix: 16) else { throw CocoaError(.coderInvalidValue) }
            result.append(byte); index = end
        }
        return result
    }

    func testCryptoKitMatchesRustHardwareCommitmentVectors() throws {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "hardware-vectors", withExtension: "json"))
        let vectors = try JSONDecoder().decode([Vector].self, from: Data(contentsOf: url))
        XCTAssertEqual(vectors.count, 3)
        for vector in vectors {
            let encoded = try bytes(vector.cbor_hex), commitment = try bytes(vector.sha256_hex)
            XCTAssertEqual(Data(SHA256.hash(data: encoded)), commitment, vector.name)
            let key = try P256.Signing.PublicKey(compressedRepresentation: bytes(vector.p256_public_hex))
            let signature = try P256.Signing.ECDSASignature(rawRepresentation: bytes(vector.p256_signature_over_sha256_hex))
            XCTAssertTrue(key.isValidSignature(signature, for: commitment), vector.name)
            // The protocol signs Data(H), not the prehash H itself.
            XCTAssertFalse(key.isValidSignature(signature, for: SHA256.hash(data: encoded)), vector.name)
            var changed = commitment; changed[0] ^= 1
            XCTAssertFalse(key.isValidSignature(signature, for: changed), vector.name)
        }
    }
}
