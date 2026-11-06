import SwiftUI
import LocalAuthentication

enum Biometry {
    /// "Face ID" or "Touch ID", when this device has one. Apple's names read the same in every language.
    static var name: String? {
        let context = LAContext()
        var error: NSError?
        _ = context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error)
        switch context.biometryType {
        case .faceID: return "Face ID"
        case .touchID: return "Touch ID"
        case .opticID: return "Optic ID"
        default: return nil
        }
    }

    @MainActor static var unlockTitle: LocalizedStringKey {
        name.map { "Unlock with \($0)" } ?? "Unlock with system authentication"
    }

    @MainActor static var quickUnlockTitle: LocalizedStringKey {
        name.map { "Quick unlock with \($0)" } ?? "Quick unlock with system authentication"
    }
}
