import XCTest
@testable import dMsg
@preconcurrency import DMsgCore

final class ConversationTimelineTests: XCTestCase {
    private let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()
    /// 2026-11-06 12:00:00 UTC.
    private let noon: UInt64 = 1_793_966_400_000
    private let minute: UInt64 = 60_000

    private func message(_ id: String, _ account: String, _ at: UInt64) -> MessageItem {
        MessageItem(id: id, sequence: 1, account: account, text: id, createdAtMs: at,
            attachmentName: nil, attachmentMime: nil, attachmentSize: nil, unreadable: false,
            digest: id, replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: false)
    }

    func testMessagesFromOnePersonCloseInTimeFormOneGroup() {
        let rows = ConversationTimeline.rows([
            message("a", "lin", noon),
            message("b", "lin", noon + minute),
            message("c", "lin", noon + 7 * minute),
            message("d", "sam", noon + 8 * minute),
        ], calendar: calendar)
        XCTAssertEqual(rows.map(\.firstOfGroup), [true, false, true, true])
        XCTAssertEqual(rows.map(\.lastOfGroup), [false, true, true, true])
    }

    func testANewDayStartsAGroupEvenWithinTheGap() {
        let midnight = noon + 12 * 60 * minute
        let rows = ConversationTimeline.rows([
            message("a", "lin", midnight - minute),
            message("b", "lin", midnight + minute),
        ], calendar: calendar)
        XCTAssertEqual(rows.map(\.startsDay), [true, true])
        XCTAssertEqual(rows.map(\.firstOfGroup), [true, true])
    }

    func testAMessageOlderThanThePreviousOneDoesNotJoinItsGroup() {
        let rows = ConversationTimeline.rows([
            message("a", "lin", noon),
            message("b", "lin", noon - minute),
        ], calendar: calendar)
        XCTAssertEqual(rows.map(\.firstOfGroup), [true, true])
    }

    func testListTimesDoNotRepeatTheDateWithinAWeek() {
        let now = ConversationTimeline.date(noon)
        let today = listDateText(ConversationTimeline.date(noon - 60 * minute), now: now, calendar: calendar)
        XCTAssertEqual(today, ConversationTimeline.date(noon - 60 * minute).formatted(date: .omitted, time: .shortened))
        let threeDays = ConversationTimeline.date(noon - 3 * 24 * 60 * minute)
        XCTAssertEqual(listDateText(threeDays, now: now, calendar: calendar), threeDays.formatted(.dateTime.weekday(.wide)))
    }
}
