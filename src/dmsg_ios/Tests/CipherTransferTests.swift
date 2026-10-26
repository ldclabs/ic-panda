import XCTest
@testable import dMsg

final class CipherTransferTests: XCTestCase {
    private func fixture() throws -> (CipherTransferStore, CipherTransfer, URL, Data) {
        let root = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
        let store = try CipherTransferStore(root: root)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bytes = Data("immutable encrypted sample".utf8), id = String(repeating: "a", count: 64)
        let transfer = CipherTransfer(id: id, account: "040g2081040g2081040g", device: String(repeating: "b", count: 64), method: "GET", uploadId: String(repeating: "c", count: 64), expiresAt: UInt64(Date.now.timeIntervalSince1970 * 1000) + 60_000, parts: [.init(slot: "0", size: UInt64(bytes.count), digest: CipherTransferStore.digest(bytes), url: URL(string: "https://core.example.test/mobile-transfer/v1/test/\(id)/0?token=\(id)")!)])
        let input = root.appending(path: "download"); try bytes.write(to: input)
        return (store,transfer,input,bytes)
    }
    func testRestartRetainsOnlyBoundedCiphertextAndTaskProgress() throws {
        let (store,transfer,input,bytes) = try fixture()
        try store.start(transfer); try store.receive(transfer.id, attempt: 0, slot: "0", temporary: input)
        let reopened = try CipherTransferStore(root: store.root), state = try XCTUnwrap(reopened.snapshot())
        XCTAssertTrue(state.ready); XCTAssertEqual(state.transfer.account, transfer.account)
        XCTAssertEqual(try Data(contentsOf: reopened.path(transfer,"0")), bytes)
        XCTAssertThrowsError(try reopened.start(transfer))
        XCTAssertThrowsError(try reopened.path(transfer,"../../workspace"))
        try reopened.discard(transfer); XCTAssertNil(try reopened.snapshot())
    }
    func testTamperingAndOldAttemptCannotCompleteARetriedTask() throws {
        let (store,transfer,input,bytes) = try fixture(); try store.start(transfer)
        try Data(repeating: 0, count: bytes.count).write(to: input)
        XCTAssertThrowsError(try store.receive(transfer.id, attempt: 0, slot: "0", temporary: input))
        XCTAssertFalse(try XCTUnwrap(store.snapshot()).ready)
        try bytes.write(to: input); try store.update(transfer.id) { $0.attempt = 1 }
        XCTAssertThrowsError(try store.receive(transfer.id, attempt: 0, slot: "0", temporary: input))
        try store.receive(transfer.id, attempt: 1, slot: "0", temporary: input)
        XCTAssertTrue(try XCTUnwrap(store.snapshot()).ready)
    }
}
