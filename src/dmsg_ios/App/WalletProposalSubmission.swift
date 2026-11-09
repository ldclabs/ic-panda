import Foundation

/// A proposal's request ID survives uncertain replies. Only a definite absence
/// or rejection permits editing and a new ID; a completed journal still needs
/// its original result read back before the UI reports success.
struct WalletProposalSubmission {
    enum Phase { case editing, submitting, unconfirmed, created(DisplayValue) }
    enum Attempt: Equatable { case create(String), reconcile(String) }

    private(set) var requestID: String
    private(set) var phase: Phase

    init(requestID: String = NativeApple.requestID(), attempted: Bool = false) {
        self.requestID = requestID
        phase = attempted ? .unconfirmed : .editing
    }

    var attempted: Bool { if case .editing = phase { false } else { true } }
    var result: DisplayValue? { if case .created(let value) = phase { value } else { nil } }

    mutating func begin() -> Attempt? {
        let next: Attempt
        switch phase {
        case .editing: next = .create(requestID)
        case .unconfirmed: next = .reconcile(requestID)
        case .submitting, .created: return nil
        }
        phase = .submitting
        return next
    }

    mutating func confirm(_ value: DisplayValue) { phase = .created(value) }

    mutating func failed(journal: String?) {
        if journal == "absent" || journal == "rejected" { self = Self() }
        else { phase = .unconfirmed }
    }

    /// Navigation/background cancellation is not proof the proposal failed.
    mutating func interrupted() {
        if case .submitting = phase { phase = .unconfirmed }
    }
}
