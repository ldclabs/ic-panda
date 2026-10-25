import UIKit
import UniformTypeIdentifiers

@MainActor
final class PrivateClipboard {
    private var writtenChange: Int?

    func copy(_ text: String) {
        UIPasteboard.general.setItems([[UTType.utf8PlainText.identifier: text]],
            options: [.localOnly: true, .expirationDate: Date().addingTimeInterval(30)])
        writtenChange = UIPasteboard.general.changeCount
    }

    func clearOwned() {
        if writtenChange == UIPasteboard.general.changeCount { UIPasteboard.general.items = [] }
        writtenChange = nil
    }
}
