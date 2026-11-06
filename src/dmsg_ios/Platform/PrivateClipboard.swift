import UIKit
import UniformTypeIdentifiers

@MainActor
final class PrivateClipboard {
    private var writtenChange: Int?

    func copy(_ text: String, lifetime: TimeInterval = 30) {
        UIPasteboard.general.setItems([[UTType.utf8PlainText.identifier: text]],
            options: [.localOnly: true, .expirationDate: Date().addingTimeInterval(max(0, min(30, lifetime)))])
        writtenChange = UIPasteboard.general.changeCount
        Haptics.tap()
    }

    func clearOwned() {
        if writtenChange == UIPasteboard.general.changeCount { UIPasteboard.general.items = [] }
        writtenChange = nil
    }
}
