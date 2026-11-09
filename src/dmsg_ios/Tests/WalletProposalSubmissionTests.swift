import XCTest
@testable import dMsg

final class WalletProposalSubmissionTests: XCTestCase {
    func testUncertainAndCompletedJournalsAlwaysReconcileTheOriginalRequest() {
        for journal: String? in [nil, "pending", "completed", "unrecognized"] {
            var submission = WalletProposalSubmission(requestID: "original")
            XCTAssertEqual(submission.begin(), .create("original"))
            XCTAssertNil(submission.begin(), "A second tap must not submit a concurrent proposal")
            submission.failed(journal: journal)
            XCTAssertTrue(submission.attempted)
            XCTAssertNil(submission.result, "A completed journal is not yet the displayed proposal result")
            XCTAssertEqual(submission.begin(), .reconcile("original"))
            submission.confirm(.string("proposal"))
            submission.interrupted()
            XCTAssertEqual(submission.result?.text, "proposal")
            XCTAssertNil(submission.begin(), "A confirmed proposal cannot be submitted again")
        }
    }

    func testOnlyDefiniteAbsenceOrRejectionAllowsANewProposal() {
        for journal in ["absent", "rejected"] {
            var submission = WalletProposalSubmission(requestID: "original")
            _ = submission.begin()
            submission.failed(journal: journal)
            XCTAssertFalse(submission.attempted)
            XCTAssertNotEqual(submission.requestID, "original")
            XCTAssertEqual(submission.begin(), .create(submission.requestID))
        }
    }

    func testInterruptedAndRestoredDraftsKeepTheirOriginalRequest() throws {
        var submission = WalletProposalSubmission(requestID: "original")
        _ = submission.begin()
        submission.interrupted()
        let saved = WalletTransferView.SavedDraft(draft: .init(), request: submission.requestID, attempted: submission.attempted)
        let restored = try JSONDecoder().decode(WalletTransferView.SavedDraft.self, from: JSONEncoder().encode(saved))
        var reopened = WalletProposalSubmission(requestID: restored.request, attempted: restored.attempted)
        XCTAssertEqual(reopened.begin(), .reconcile("original"))
    }
}
