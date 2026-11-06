import Foundation
@preconcurrency import DMsgCore

/// What a message's row shows besides the message itself, worked out once per update.
struct ConversationRow: Identifiable {
    let message: MessageItem
    let startsDay: Bool
    let firstOfGroup: Bool
    let lastOfGroup: Bool
    var id: String { message.id }
}

enum ConversationTimeline {
    /// Messages from one person within five minutes read as one group.
    static let groupGap: UInt64 = 5 * 60 * 1000

    static func rows(_ messages: [MessageItem], calendar: Calendar = .current) -> [ConversationRow] {
        let days = messages.map { calendar.startOfDay(for: date($0.createdAtMs)) }
        func grouped(_ index: Int) -> Bool {
            let earlier = messages[index - 1], later = messages[index]
            return days[index - 1] == days[index] && earlier.account == later.account
                && later.createdAtMs >= earlier.createdAtMs && later.createdAtMs - earlier.createdAtMs < groupGap
        }
        return messages.indices.map { index in
            ConversationRow(message: messages[index],
                            startsDay: index == 0 || days[index - 1] != days[index],
                            firstOfGroup: index == 0 || !grouped(index),
                            lastOfGroup: index == messages.count - 1 || !grouped(index + 1))
        }
    }

    static func date(_ milliseconds: UInt64) -> Date { Date(timeIntervalSince1970: Double(milliseconds) / 1000) }

    /// "Today", "Yesterday", then the date, in the person's language.
    static func dayTitle(_ date: Date) -> String { dayFormatter.string(from: date) }

    private static let dayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium; formatter.timeStyle = .none
        formatter.doesRelativeDateFormatting = true
        return formatter
    }()
}

/// A list's time that does not tick: the time today, then yesterday, the weekday
/// within a week, and the date after that.
func listDateText(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> String {
    if calendar.isDate(date, inSameDayAs: now) { return date.formatted(date: .omitted, time: .shortened) }
    let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: date), to: calendar.startOfDay(for: now)).day ?? 0
    if days == 1 { return ConversationTimeline.dayTitle(date) }
    if (2..<7).contains(days) { return date.formatted(.dateTime.weekday(.wide)) }
    if calendar.isDate(date, equalTo: now, toGranularity: .year) { return date.formatted(.dateTime.month(.abbreviated).day()) }
    return date.formatted(date: .numeric, time: .omitted)
}
