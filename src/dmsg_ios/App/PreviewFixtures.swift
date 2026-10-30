#if DEBUG
import Foundation
@preconcurrency import DMsgCore

extension SessionModel {
    /// UI-test-only, in-memory presentation fixtures. Never unlocks Rust,
    /// writes a real workspace, or appears in a Release build.
    func loadFixtures() {
        let access = ProcessInfo.processInfo.arguments.contains("--ui-access")
        let pairing = ProcessInfo.processInfo.arguments.contains("--ui-pairing")
        let storeScreens = ProcessInfo.processInfo.arguments.contains("--ui-store-screenshots")
        let chinese = storeScreens && Bundle.main.preferredLocalizations.first?.hasPrefix("zh") == true
        installFixtures(
            SessionSnapshot(generation: 0, sequence: 0, state: access ? (pairing ? "pairing_required" : "root_required") : "unlocked", account: "040g2081040g2081040g", deviceId: "fixture-device",
                            pendingSync: storeScreens ? 0 : 2, conflicts: 0, approvalsWaiting: 1, fastUnlock: access, setupPending: false, lastSyncAtMs: nil, syncError: nil),
            items: [
                VaultItem(id: "fixture-github", revision: "1", kind: "login", title: "GitHub", subtitle: "lin@studio.example", notes: chinese ? "个人开发账号" : "Personal development account", favorite: true, tags: [], synced: true, conflict: false, reserved: false, files: [],
                          fields: [VaultField(id: "username", kind: "text", label: "Username", section: "", value: "lin@studio.example", concealed: false),
                                   VaultField(id: "password", kind: "concealed", label: "Password", section: "", value: "••••••••", concealed: true)]),
                VaultItem(id: "fixture-mail", revision: "1", kind: "login", title: chinese ? "工作邮箱" : "Work email", subtitle: "hello@studio.example", notes: "", favorite: true, tags: [], synced: true, conflict: false, reserved: false, files: [], fields: []),
                VaultItem(id: "fixture-note", revision: "1", kind: "note", title: chinese ? "项目笔记" : "Project notes", subtitle: chinese ? "记录尚未完成的想法" : "A place for unfinished ideas", notes: "", favorite: false, tags: [], synced: storeScreens, conflict: false, reserved: false, files: [], fields: []),
            ])
        if ProcessInfo.processInfo.arguments.contains("--ui-detail-navigation") {
            vault.insert(VaultItem(id: "fixture-managed", revision: "1", kind: "key", title: "dMsg controller #1",
                subtitle: "", notes: "", favorite: false, tags: ["dmsg-controller"], synced: true, conflict: false,
                reserved: true, files: [], fields: [VaultField(id: "key", kind: "concealed", label: "Key",
                    section: "", value: "••••••••", concealed: true)]), at: 0)
            library = LibraryListing(folder: nil, folders: [], files: [
                LibraryFile(id: "fixture-local", name: "Local image.png", mime: "image/png", size: 2048,
                    current: "1", sha256: "fixture-digest", status: "local", versions: 1),
            ])
        }
        if storeScreens { loadStorePresentation() }
        if ProcessInfo.processInfo.arguments.contains("--ui-syncing") { _ = beginBusyOperation(synchronizing: true) }
        if ProcessInfo.processInfo.arguments.contains("--ui-recovery-pending") {
            let account = "040g2081040g2081040g"
            recordRecoveryObservation(RecoveryObservation(account: account, pending: true,
                digest: "fixture-recovery", observedAtMs: UInt64(Date.now.timeIntervalSince1970 * 1000) + 1,
                stale: false), for: account)
        }
    }

    /// Fictitious catalog data for native store screenshots. The normal UI-test
    /// fixtures stay small; no real account or service is used for these images.
    private func loadStorePresentation() {
        let chinese = Bundle.main.preferredLocalizations.first?.hasPrefix("zh") == true
        func text(_ english: String, _ chineseText: String) -> String { chinese ? chineseText : english }
        let items: [(String, String, String, String)] = [
            ("api", "api", text("Project API", "项目 API"), "api.studio.example"),
            ("ssh", "ssh", text("Development server", "开发服务器"), "deploy@studio.example"),
            ("card", "card", text("Travel card", "旅行卡"), "•••• 1042"),
            ("wifi", "note", text("Studio Wi-Fi", "工作室 Wi-Fi"), text("Shared with the team", "与团队共享")),
            ("license", "license", text("Design tools", "设计工具"), text("Annual license", "年度许可证")),
            ("database", "database", text("Project database", "项目数据库"), "research.studio.example"),
            ("identity", "identity", text("Travel documents", "旅行证件"), text("Personal reference", "个人资料")),
        ]
        vault += items.map { id, kind, title, subtitle in
            VaultItem(id: "fixture-" + id, revision: "1", kind: kind, title: title, subtitle: subtitle,
                      notes: "", favorite: false, tags: [], synced: true, conflict: false, reserved: false, files: [], fields: [])
        }
        channels = [
            ChannelItem(id: "fixture-channel", name: text("Studio", "工作室"), unread: 2, draft: "", preview: text("The new designs are ready to review.", "新设计已准备好，可以开始评审。")),
            ChannelItem(id: "fixture-research", name: text("Research", "研究小组"), unread: 0, draft: "", preview: text("Notes from today's discussion", "今天讨论的笔记")),
            ChannelItem(id: "fixture-release", name: text("Release planning", "发布计划"), unread: 0, draft: "", preview: text("Let's check the final checklist.", "一起核对最后的检查清单。")),
            ChannelItem(id: "fixture-family", name: text("Family", "家人"), unread: 0, draft: "", preview: text("I've shared the travel document.", "旅行资料已经分享了。")),
            ChannelItem(id: "fixture-design", name: text("Design review", "设计评审"), unread: 1, draft: "", preview: text("Thanks for the feedback!", "谢谢反馈！")),
            ChannelItem(id: "fixture-personal", name: text("Personal notes", "个人笔记"), unread: 0, draft: "", preview: text("Ideas for next week", "下周的想法")),
        ]
        library = LibraryListing(folder: nil, folders: [], files: [
            LibraryFile(id: "fixture-brief", name: text("Project brief.pdf", "项目简介.pdf"), mime: "application/pdf", size: 245_760, current: "1", sha256: "", status: "backed_up", versions: 1),
            LibraryFile(id: "fixture-notes", name: text("Research notes.md", "研究笔记.md"), mime: "text/markdown", size: 8192, current: "1", sha256: "", status: "local", versions: 1),
            LibraryFile(id: "fixture-image", name: text("Design reference.png", "设计参考.png"), mime: "image/png", size: 1_572_864, current: "1", sha256: "", status: "backed_up", versions: 1),
        ])
    }
}
#endif
