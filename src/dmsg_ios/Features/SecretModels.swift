import Foundation

struct SecretReferenceResult: Decodable { let uri: String }

struct SecretAttachmentListing: Decodable {
    struct Entry: Decodable, Identifiable {
        struct Details: Decodable { let file: LibraryFile; let versions: [Version] }
        struct Version: Decodable, Identifiable { let id: String; let size: UInt64; let local: Bool; let current: Bool }
        let id: String
        let available: Bool
        let details: Details?
        let error: String?
        var canShare: Bool {
            guard available, let details else { return false }
            return details.versions.first(where: { $0.id == details.file.current })?.local == true
        }
    }
    let itemId: String
    let revision: String
    let files: [Entry]
    let deviceLocal: Bool
}

struct SecretPackagePreview: Decodable {
    struct Preview: Decodable {
        struct Field: Decodable, Identifiable { let id: String; let type: String; let label: String; let section: String? }
        struct Attachment: Decodable { let name: String; let mime: String; let size: UInt64; let sha256: String }
        let snapshotId: String
        let createdAt: UInt64
        let type: String
        let title: String
        let fields: [Field]
        let notes: Bool
        let attachments: [Attachment]
    }
    struct ChannelSource: Decodable { let channelId: String; let seq: UInt64; let senderAccount: String; let messageDigest: String }
    let packageId: String
    let digest: String
    let preview: Preview
    let alreadyImported: [String]?
    let channelSource: ChannelSource?
}

struct SecretShareJob: Decodable, Identifiable {
    struct Plan: Decodable {
        struct Channel: Decodable { let channelId: String; let name: String; let epoch: UInt64; let controlHead: String; let aclVersion: UInt64 }
        let title: String
        let shareId: String
        let expiresAt: UInt64?
        let maxDownloadSessions: UInt64?
        let plainBytes: UInt64
        let channel: Channel?
        let password: Bool?
    }
    struct Link: Decodable { let url: String; let state: String }
    let id: String
    let state: String
    let plan: Plan
    let requestId: String?
    let error: String?
    let uploadedChunks: UInt64
    let totalChunks: UInt64
    let cancelRequested: Bool
    let link: Link?
}

struct SecretShareListing: Decodable { let shares: [SecretShareJob]; let deviceLocal: Bool }

struct SecretReferenceIndex: Decodable {
    struct Dependency: Decodable, Identifiable {
        let kind: String
        let id: String
        let label: String
        let revision: String
        let state: String
        let action: String
        var listID: String { kind + ":" + id }
    }
    let revision: String?
    let dependencies: [Dependency]
    let digest: String
    let deviceLocal: Bool
}

struct SecretRotation: Decodable, Identifiable {
    struct Reference: Decodable { let accountId: String; let itemId: String; let fieldId: String }
    let id: String
    let reference: Reference
    let previousRevision: String
    let newRevision: String?
    let created: Bool
    let tested: Bool
    let referencesChecked: Bool
    let referencesDigest: String?
    let upstreamRevoked: Bool
    let evidence: String
    let createdAt: UInt64
    let updatedAt: UInt64
    let completedAt: UInt64?
}

struct SecretHealthReport: Decodable {
    struct Finding: Decodable, Identifiable {
        let id: String
        let rule: String
        let severity: String
        let subject: String
        let revision: String
        let field: String?
        let observedAt: UInt64
        let evidence: String
        let action: String
        let dismissed: Bool
    }
    let findings: [Finding]
    let rotations: [SecretRotation]
    let observedAt: UInt64
    let deviceLocal: Bool
}

struct SecretActivityPage: Decodable { let items: [ApprovalView]; let next: String?; let deviceLocal: Bool }

func secretStateTitle(_ state: String) -> String {
    switch state {
    case "awaiting_user": String(localized: "Waiting for approval")
    case "prepared": String(localized: "Prepared on this device")
    case "encrypting": String(localized: "Encrypting")
    case "uploading": String(localized: "Uploading")
    case "publishing": String(localized: "Publication pending")
    case "unknown": String(localized: "Result unconfirmed")
    case "revoke_pending": String(localized: "Revocation pending")
    case "published": String(localized: "Published")
    case "revoked": String(localized: "Revoked")
    case "executing": String(localized: "In progress")
    case "done": String(localized: "Completed")
    case "cancelled": String(localized: "Cancelled")
    case "rejected": String(localized: "Rejected")
    case "expired": String(localized: "Expired")
    case "failed": String(localized: "Failed")
    case "elsewhere": String(localized: "Handled on another device")
    case "lost": String(localized: "Outcome no longer available")
    case "current", "active": String(localized: "Current")
    case "configured": String(localized: "Configured on this device")
    case "granted": String(localized: "Access granted")
    case "disabled": String(localized: "Disabled")
    case "stale": String(localized: "Needs a fresh review")
    case "interrupted": String(localized: "Interrupted")
    default: String(localized: "Unknown state") + ": " + state
    }
}

func secretDependencyKind(_ kind: String) -> String {
    switch kind {
    case "policy", "policy_grant": String(localized: "Device policy")
    case "mount": String(localized: "Desktop file mapping")
    case "ssh": String(localized: "SSH binding")
    case "snapshot_link": String(localized: "Snapshot link")
    case "snapshot_channel": String(localized: "Channel snapshot")
    case "process_observation": String(localized: "Local delivery record")
    default: String(localized: "Local dependency")
    }
}

func secretNextAction(_ action: String) -> String {
    switch action {
    case "review_agent_authorization": String(localized: "Review the agent's authorization on its originating device.")
    case "review_policy_and_grants", "review_device_policy": String(localized: "Review this device's policy and access grants.")
    case "restart_mount_after_source_change", "review_mount_sources_and_restart": String(localized: "Review and restart the desktop file mapping on its originating device.")
    case "approve_current_key": String(localized: "Review the current key and renew the SSH binding on its originating device.")
    case "check_project_refs_and_delivered_copies": String(localized: "Check project references and any copies already delivered.")
    case "repair_reference": String(localized: "Repair the unavailable field reference on its originating device.")
    case "review_device_history": String(localized: "Check this device's activity before deciding whether access is still needed.")
    case "review_share_expiry_and_recipients": String(localized: "Review the snapshot's recipients and expiry.")
    case "review_and_revoke_old_snapshot": String(localized: "Review old snapshots and revoke remaining links if appropriate.")
    case "back_up_selected_file_version": String(localized: "Explicitly back up the selected file version if you want a cloud copy.")
    default: String(localized: "Review this local record before making changes.")
    }
}
