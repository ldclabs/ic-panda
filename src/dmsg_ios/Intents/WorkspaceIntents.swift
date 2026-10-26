import AppIntents
import Foundation
import Observation

enum WorkspaceDestination: String, AppEnum {
    case vault, approvals, imports
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Workspace destination"
    static let caseDisplayRepresentations: [Self: DisplayRepresentation] = [.vault: "Vault", .approvals: "Approvals", .imports: "Shared with dMsg"]
}

@MainActor @Observable
final class IntentNavigation {
    static let shared = IntentNavigation()
    var destination: WorkspaceDestination?
}

struct OpenWorkspaceIntent: AppIntent {
    static let title: LocalizedStringResource = "Open dMsg workspace"
    static let description = IntentDescription("Open dMsg. Unlocking and approvals still require your interaction in the app.")
    static let openAppWhenRun = true
    @Parameter(title: "Destination", default: .vault) var destination: WorkspaceDestination
    @MainActor func perform() async throws -> some IntentResult {
        IntentNavigation.shared.destination = destination
        return .result()
    }
}

struct ImportIntoDMsgIntent: AppIntent {
    static let title: LocalizedStringResource = "Import into dMsg"
    static let description = IntentDescription("Stage selected files, then choose an account and import them in dMsg.")
    static let openAppWhenRun = true
    @Parameter(title: "Files") var files: [IntentFile]
    @MainActor func perform() async throws -> some IntentResult {
        guard !files.isEmpty, files.count <= 10 else { throw CocoaError(.fileReadTooLarge) }
        for file in files {
            guard let url = file.fileURL else { throw CocoaError(.fileReadUnknown) }
            _ = try await Task.detached { try IncomingShares().stage(url) }.value
        }
        IntentNavigation.shared.destination = .imports
        return .result()
    }
}

struct WorkspaceShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: OpenWorkspaceIntent(), phrases: ["Open my workspace in \(.applicationName)"], shortTitle: "Open workspace", systemImageName: "lock.shield")
        AppShortcut(intent: ImportIntoDMsgIntent(), phrases: ["Import files into \(.applicationName)"], shortTitle: "Import files", systemImageName: "square.and.arrow.down")
    }
}
