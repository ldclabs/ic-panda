import SwiftUI
@preconcurrency import DMsgCore

func vaultKindName(_ kind: String) -> LocalizedStringKey {
    switch kind {
    case "login": "Password"; case "api": "API credential"; case "key": "Key"; case "ssh": "SSH key"
    case "wallet": "Wallet secret"; case "card": "Payment card"; case "identity": "Identity"
    case "server": "Server"; case "database": "Database"; case "license": "Software license"
    default: "Note"
    }
}

func vaultFieldName(_ id: String) -> String {
    switch id {
    case "username": String(localized: "Username")
    case "password": String(localized: "Password")
    case "url": String(localized: "Website")
    default: String(localized: String.LocalizationValue(id.replacingOccurrences(of: "_", with: " ").capitalized))
    }
}


func vaultFieldTitle(_ field: VaultField) -> String {
    field.label.isEmpty ? (field.kind == "otp" ? String(localized: "One-time code") : vaultFieldName(field.id)) : field.label
}

func vaultFieldIsLTR(_ field: VaultField, value: String) -> Bool {
    ["url", "email", "otp", "date", "month", "phone"].contains(field.kind)
        || ["private_key", "public_key", "fingerprint", "number", "cvv", "pin"].contains(field.id)
        || value.contains("@")
}

func vaultReadError(_ error: Error) -> String {
    if case MobileError.Failure(let code) = error, code == "NOT_FOUND" { return String(localized: "This item is no longer available.") }
    return String(localized: "The operation could not be completed. Please try again.")
}

func vaultFieldKindTitle(_ kind: String) -> String {
    switch kind {
    case "concealed": String(localized: "Hidden value")
    case "url": String(localized: "Website")
    case "email": String(localized: "Email")
    case "phone": String(localized: "Phone")
    case "date": String(localized: "Date")
    case "month": String(localized: "Month")
    case "otp": String(localized: "One-time code")
    default: String(localized: "Text")
    }
}
