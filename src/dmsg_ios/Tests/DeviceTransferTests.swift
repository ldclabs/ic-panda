import XCTest
@preconcurrency import DMsgCore
@testable import dMsg

@MainActor
final class DeviceTransferTests: XCTestCase {
    func testTypedAddressesTakeAPortAndBracketedIPv6() {
        let v4 = DeviceTransferModel.parse(" 192.168.1.20:52411 ")
        XCTAssertEqual(v4?.host, "192.168.1.20"); XCTAssertEqual(v4?.port, 52411)
        let v6 = DeviceTransferModel.parse("[fe80::1%en0]:7000")
        XCTAssertEqual(v6?.host, "fe80::1%en0"); XCTAssertEqual(v6?.port, 7000)
        let named = DeviceTransferModel.parse("Mac.local.:80")
        XCTAssertEqual(named?.host, "Mac.local."); XCTAssertEqual(named?.port, 80)
        for invalid in ["", "host", "host:", ":80", "host:0", "host:70000", "host:port"] {
            XCTAssertNil(DeviceTransferModel.parse(invalid), invalid)
        }
    }

    func testAdvertNamesComeFromTheSessionOnly() throws {
        let session = String(repeating: "ab", count: 16)
        let name = try transferAdvert(session: session)
        XCTAssertEqual(name.count, 16)
        XCTAssertEqual(try transferAdvert(session: session), name)
        XCTAssertNotEqual(try transferAdvert(session: String(repeating: "cd", count: 16)), name)
        XCTAssertThrowsError(try transferAdvert(session: "abcd"))
    }

    func testAnAdvertisedSessionIsFoundUntilWithdrawn() async throws {
        let name = try transferAdvert(session: UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased())
        let advert = try await Bonjour.advertise(name, port: 52411)
        let found = try await Bonjour.find(name, timeout: .seconds(10))
        XCTAssertEqual(found.port, 52411)
        XCTAssertTrue(found.host.hasSuffix(".local."), found.host)
        advert.stop()
        let other = try transferAdvert(session: UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased())
        do {
            _ = try await Bonjour.find(other, timeout: .milliseconds(500))
            XCTFail("found a session nobody advertised")
        } catch let failure as Bonjour.Failure {
            XCTAssertEqual(failure, .notFound)
        }
    }
}
