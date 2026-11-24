import UIKit
import UniformTypeIdentifiers

@MainActor
final class PrivateClipboard {
    private var writtenChange: Int?

    func copy(_ text: String, lifetime: TimeInterval = 30) {
        write([UTType.utf8PlainText.identifier: text], lifetime: lifetime)
    }

    /// Image bytes under their own type, on the same local-only, expiring terms as text.
    func copy(_ data: Data, type: UTType, lifetime: TimeInterval = 30) {
        write([type.identifier: data], lifetime: lifetime)
    }

    private func write(_ item: [String: Any], lifetime: TimeInterval) {
        UIPasteboard.general.setItems([item],
            options: [.localOnly: true, .expirationDate: Date().addingTimeInterval(max(0, min(30, lifetime)))])
        writtenChange = UIPasteboard.general.changeCount
        Haptics.tap()
    }

    func clearOwned() {
        if writtenChange == UIPasteboard.general.changeCount { UIPasteboard.general.items = [] }
        writtenChange = nil
    }
}
