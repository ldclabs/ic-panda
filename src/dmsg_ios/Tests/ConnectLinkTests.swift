import XCTest
@testable import dMsg

final class ConnectLinkTests: XCTestCase {
    func testOnlyNavigationHintsAreAccepted() throws {
        let account = "040g2081040g2081040g", request = String(repeating: "a", count: 64)
        let link = try XCTUnwrap(ConnectLink(URL(string: "https://connect.dmsg.net/app/request?account=\(account)&request=\(request)")!))
        XCTAssertEqual(link.destination, .request); XCTAssertEqual(link.account, account); XCTAssertEqual(link.request, request)
        XCTAssertNotNil(ConnectLink(URL(string: "dmsg://connect/pair")!))
        for url in ["https://evil.test/app/pair", "https://connect.dmsg.net.evil.test/app/pair", "https://user@connect.dmsg.net/app/pair", "https://connect.dmsg.net/app/pair?account=\(account)&account=\(account)", "dmsg://connect/request?account=\(account)", "dmsg://connect/pair?callback=https://evil.test", "dmsg://connect/pair#secret"] {
            XCTAssertNil(ConnectLink(URL(string: url)!), url)
        }
    }
}
