import SwiftUI

/// Business icons share the desktop's Remix paths. Regenerate the template
/// assets with `python3 scripts/ios/sync-icons.py` after changing this list.
enum AppGlyph: String, CaseIterable {
    case vault
    case channels
    case inbox
    case approvals
    case agents
    case plan
    case folder
    case account
    case profile
    case device
    case recovery
    case security
    case browser
    case audit
    case coins
    case key
    case note
    case api
    case keypair
    case ssh
    case wallet
    case card
    case identity
    case server
    case database
    case license
    case file
    case image
    case link
    case signature
    case language

    var assetName: String { "Glyph-\(rawValue)" }

    var image: Image { Image(decorative: assetName).renderingMode(.template) }

    static func vaultItem(_ kind: String) -> AppGlyph {
        switch kind {
        case "login": .key
        case "api": .api
        case "key": .keypair
        case "ssh": .ssh
        case "wallet": .wallet
        case "card": .card
        case "identity": .identity
        case "server": .server
        case "database": .database
        case "license": .license
        case "note": .note
        default: .file
        }
    }
}

extension Label where Title == Text, Icon == Image {
    init(_ title: LocalizedStringKey, glyph: AppGlyph) {
        self.init { Text(title) } icon: { glyph.image }
    }
}
