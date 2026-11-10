#if DEBUG
import Foundation
@preconcurrency import DMsgCore

extension SessionModel {
    /// UI-test-only, in-memory presentation fixtures. Never unlocks Rust,
    /// writes a real workspace, or appears in a Release build.
    func loadFixtures() {
        let passkey = ProcessInfo.processInfo.arguments.contains("--ui-passkey")
        let welcome = ProcessInfo.processInfo.arguments.contains("--ui-welcome")
        let access = ProcessInfo.processInfo.arguments.contains("--ui-access")
        let setup = ProcessInfo.processInfo.arguments.contains("--ui-setup")
        let pairing = ProcessInfo.processInfo.arguments.contains("--ui-pairing")
        let storeScreens = ProcessInfo.processInfo.arguments.contains("--ui-store-screenshots")
        let chinese = storeScreens && Bundle.main.preferredLocalizations.first?.hasPrefix("zh") == true
        let loginExpiry: UInt64? = ProcessInfo.processInfo.arguments.contains("--ui-login-expired") ? nil : UInt64(Date.now.timeIntervalSince1970 * 1000) + (ProcessInfo.processInfo.arguments.contains("--ui-login-expiring") ? 30_000 : 86_400_000)
        installFixtures(
            SessionSnapshot(generation: 0, sequence: 0, state: welcome ? "logged_out" : setup ? "no_account" : (access ? (pairing ? "pairing_required" : "root_required") : "unlocked"), account: (setup || welcome) ? nil : "040g2081040g2081040g", deviceId: "fixture-device",
                            pendingSync: storeScreens ? 0 : 2, conflicts: 0, approvalsWaiting: 1, fastUnlock: access, loginExpiresAtMs: loginExpiry, loginRenewable: passkey && !welcome && !ProcessInfo.processInfo.arguments.contains("--ui-passkey-capacity"), passkeyAvailable: passkey, setupPending: false, lastSyncAtMs: nil, syncError: nil),
            items: [
                vaultFixtureItem(id: "fixture-github", revision: "1", kind: "login", title: "GitHub", subtitle: "lin@studio.example", notes: chinese ? "个人开发账号" : "Personal development account", favorite: true, tags: [], synced: true, conflict: false, reserved: false, files: [],
                          fields: [VaultField(id: "username", kind: "text", label: "", section: "", value: "lin@studio.example", concealed: false),
                                   VaultField(id: "password", kind: "concealed", label: "", section: "", value: "••••••••", concealed: true)]),
                vaultFixtureItem(id: "fixture-mail", revision: "1", kind: "login", title: chinese ? "工作邮箱" : "Work email", subtitle: "hello@studio.example", notes: "", favorite: true, tags: [], synced: true, conflict: false, reserved: false, files: [], fields: []),
                vaultFixtureItem(id: "fixture-note", revision: "1", kind: "note", title: chinese ? "项目笔记" : "Project notes", subtitle: "", notes: chinese ? "记录尚未完成的想法" : "A place for unfinished ideas", favorite: false, tags: [], synced: storeScreens, conflict: false, reserved: false, files: [], fields: []),
            ])
        signingIntoAnotherAccount = ProcessInfo.processInfo.arguments.contains("--ui-sign-in-another")
        if ProcessInfo.processInfo.arguments.contains("--ui-passkey-missing") {
            reportLoginFailure(MobileError.Failure(code: "NOT_FOUND"), usingPasskey: passkey, operation: .login)
        } else if ProcessInfo.processInfo.arguments.contains("--ui-passkey-full") {
            reportLoginFailure(MobileError.Failure(code: "SESSION_LIMIT_EXCEEDED"), usingPasskey: passkey, operation: .login, credentialID: Data([3]))
        } else if ProcessInfo.processInfo.arguments.contains("--ui-passkey-quota") {
            reportLoginFailure(MobileError.Failure(code: "QUOTA_EXCEEDED"), usingPasskey: passkey, operation: .login)
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-first-sync") {
            if let session { installFixtures(session, items: []) }; channels = []
            contentSync = .fixture(running: true, entries: 17, total: 240, bytes: 262144)
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-sync-paused") {
            if let session { installFixtures(session, items: []) }; channels = []
            contentSync = .fixture(error: "SYNC_BUSY", retry: UInt64(Date.now.timeIntervalSince1970 * 1000) + (ProcessInfo.processInfo.arguments.contains("--ui-sync-short-pause") ? 10_000 : 90_000))
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-detail-navigation") {
            vault.insert(vaultFixtureItem(id: "fixture-managed", revision: "1", kind: "key", title: "dMsg controller #1",
                subtitle: "", notes: "", favorite: false, tags: ["dmsg-controller"], synced: true, conflict: false,
                reserved: true, files: [], fields: [VaultField(id: "key", kind: "concealed", label: "Key",
                    section: "", value: "••••••••", concealed: true)]), at: 0)
            library = LibraryListing(folder: nil, folders: [], files: [
                LibraryFile(id: "fixture-local", name: "Local image.png", mime: "image/png", size: 2048,
                    current: "1", sha256: "fixture-digest", status: "local", versions: 1),
            ])
        }
        if storeScreens { loadStorePresentation() }
        if ProcessInfo.processInfo.arguments.contains("--ui-reminder-other-account") {
            reminderEntry = ReminderEntry(account: "040g2082040g2082040g", target: .task("conflicts:2"))
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-browser-other-account") {
            ConnectNavigation.shared.pending = ConnectLink(URL(string: "dmsg://connect/request?account=040g2082040g2082040g&request=\(String(repeating: "a", count: 64))")!)
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel") {
            channels = [ChannelItem(id: "fixture-channel", name: "Studio", unread: 0, draft: "", preview: "")]
            if ProcessInfo.processInfo.arguments.contains("--ui-channel-readonly") {
                channels = [ChannelItem(id: "fixture-channel", name: "Studio", unread: 0, draft: "", preview: "", lastAt: 0, muted: false, status: "active", canPublish: false, canInvite: false, pendingSends: 0, readThrough: 0, news: false)]
            }
            selectedTab = .messages
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-inbox") { selectedTab = .messages }
        if ProcessInfo.processInfo.arguments.contains("--ui-syncing") { _ = beginBusyOperation(.synchronizing) }
        if ProcessInfo.processInfo.arguments.contains("--ui-recovery-pending") {
            let account = "040g2081040g2081040g"
            recordRecoveryObservation(RecoveryObservation(account: account, pending: true,
                digest: "fixture-recovery", observedAtMs: UInt64(Date.now.timeIntervalSince1970 * 1000) + 1,
                stale: false), for: account)
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-vault-rich") {
            vault += (0..<75).map { index in
                let kind = vaultKinds()[index % 11]
                return vaultFixtureItem(id: "fixture-rich-\(index)", revision: "1", kind: kind,
                    title: String(format: "Entry %02d", index), subtitle: ["login", "api", "server", "database"].contains(kind) ? "user@example.org" : "", notes: "", favorite: index < 8,
                    tags: [index % 2 == 0 ? "work" : "personal"], synced: true, conflict: false, reserved: false, files: [],
                    fields: [VaultField(id: "username", kind: "text", label: "", section: "Account", value: "user@example.org", concealed: false),
                             VaultField(id: "otp", kind: "otp", label: "", section: "Security", value: "JBSWY3DPEHPK3PXP", concealed: true)])
            }
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-vault-empty") { vault = [] }
        if ProcessInfo.processInfo.arguments.contains("--ui-vault-detail") {
            vault.insert(vaultFixtureItem(id: "fixture-api", revision: "1", kind: "api", title: "API · Production integration", subtitle: "api.example.org", notes: "", favorite: false,
                tags: ["production", "development"], synced: true, conflict: false, reserved: false, files: [], fields: [
                    VaultField(id: "username", kind: "text", label: "", section: "", value: "", concealed: false),
                    VaultField(id: "credential", kind: "concealed", label: "Production API key (read and write)", section: "", value: "**********", concealed: true),
                    VaultField(id: "url", kind: "url", label: "", section: "", value: "https://api.example.org/developer/integrations/production?region=ap-southeast-1", concealed: false),
                    VaultField(id: "expires", kind: "date", label: "", section: "Empty section", value: "", concealed: false),
                    VaultField(id: "region", kind: "text", label: "Region", section: "Saved on api.example.org for the production integration workspace", value: "Asia Pacific · Singapore", concealed: false),
                ]), at: 0)
        }
        fixtureVaultItems = vault
        prepareFileFixtures()
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-saved-attachment") { selectedTab = .messages }
    }

    /// An interrupted setup binds the account before its first root is ready.
    /// The next attempt continues that same account, as the native journal does.
    func initializeAccountFixture() async throws -> SessionSnapshot {
        fixtureInitializationAttempts += 1
        let attempt = fixtureInitializationAttempts, current = epoch
        let slow = ProcessInfo.processInfo.arguments.contains("--ui-setup-slow")
        try await Task.sleep(for: .seconds(slow && attempt == 1 ? 22 : 6))
        guard current == epoch, !Task.isCancelled else { throw CancellationError() }
        let fails = attempt == 1 || (attempt == 2 && ProcessInfo.processInfo.arguments.contains("--ui-setup-failure"))
        let state = SessionSnapshot(generation: UInt64(attempt * 2), sequence: UInt64(attempt),
            state: fails ? "root_required" : "unlocked", account: "040g2081040g2081040g", deviceId: "fixture-device",
            pendingSync: 0, conflicts: 0, approvalsWaiting: 0, fastUnlock: false, loginExpiresAtMs: nil,
            loginRenewable: false, passkeyAvailable: false, setupPending: fails, lastSyncAtMs: nil, syncError: nil)
        installFixtures(state, items: [])
        if fails { throw MobileError.Failure(code: "EXECUTION_UNKNOWN") }
        return state
    }

    /// Model an incoming revision after detail loading, without a network account.
    func scheduleVaultFixtureRefresh(id: String) {
        guard id == "fixture-github", !fixtureVaultRefreshScheduled,
              ProcessInfo.processInfo.arguments.contains("--ui-vault-refresh") else { return }
        fixtureVaultRefreshScheduled = true
        changeVaultFixture(id: id, after: .seconds(8)) { $0.replacing(revision: "remote-2", title: "GitHub updated") }
    }

    /// Model a sync landing while a value read on request is still on screen.
    func scheduleVaultFixtureChangeAfterRead(id: String) {
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("--ui-vault-sync-state") {
            changeVaultFixture(id: id, after: .seconds(5)) { $0.replacing(synced: false) }
        } else if arguments.contains("--ui-vault-new-revision") {
            changeVaultFixture(id: id, after: .seconds(5)) { $0.replacing(revision: "remote-2", title: "GitHub updated") }
        }
    }

    private func changeVaultFixture(id: String, after delay: Duration, _ change: @escaping (VaultItem) -> VaultItem) {
        let epoch = epoch
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: delay)
            guard let self, self.epoch == epoch, self.unlocked,
                  let index = self.fixtureVaultItems.firstIndex(where: { $0.id == id }) else { return }
            self.fixtureVaultItems[index] = change(self.fixtureVaultItems[index])
            await self.loadVault()
        }
    }

    func inboxFixture(refresh: Bool) async throws -> InboxSnapshot {
        let arguments = ProcessInfo.processInfo.arguments
        if refresh {
            fixtureInboxRefreshes += 1
            if arguments.contains("--ui-inbox-failure"), fixtureInboxRefreshes == 1 {
                throw MobileError.Failure(code: "RECOVERY_INCOMPLETE")
            }
            if arguments.contains("--ui-inbox-slow") { try await Task.sleep(for: .seconds(30)) }
            if !arguments.contains("--ui-inbox-empty"), fixtureInboxLetters.isEmpty {
                fixtureInboxLetters = [InboxLetter(orderId: "fixture-letter", sender: "040g2082040g2082040g",
                    state: "visible", text: "A verified letter · 已验证来信", createdAt: 1_793_534_400_000,
                    read: false, archived: false, abortReason: nil)]
            }
            fixtureInboxComplete = true
        }
        return InboxSnapshot(letters: fixtureInboxLetters, complete: fixtureInboxComplete)
    }

    func markInboxFixture(order: String, read: Bool, archived: Bool) {
        guard let index = fixtureInboxLetters.firstIndex(where: { $0.id == order }) else { return }
        fixtureInboxLetters[index].read = read; fixtureInboxLetters[index].archived = archived
    }

    func conversationFixture(before: UInt64?, query: String) -> MessagePage {
        let items = fixtureChannelMessages.filter {
            $0.sequence < (before ?? .max) && (query.isEmpty || $0.text.localizedCaseInsensitiveContains(query) || $0.attachmentName?.localizedCaseInsensitiveContains(query) == true)
        }
        let limit = ProcessInfo.processInfo.arguments.contains("--ui-channel-history") ? 3 : 100
        let page = Array(items.suffix(limit)), more = items.count > limit
        return MessagePage(generation: 0, items: page, hasMore: more, nextBefore: more ? page.first?.sequence : nil)
    }

    func refreshConversationFixture() async throws {
        fixtureChannelRefreshes += 1
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-hint-during-refresh") {
            let first = fixtureChannelRefreshes == 1
            fixtureChannelMessages = (5...(first ? 5 : 6)).map { sequence in
                MessageItem(id: "fixture-\(sequence)", sequence: UInt64(sequence), account: "fixture-peer",
                    text: "测试消息\(sequence)", createdAtMs: 1_793_513_600_000 + UInt64(sequence),
                    attachmentName: nil, attachmentMime: nil, attachmentSize: nil, unreadable: false,
                    digest: "fixture-digest-\(sequence)", replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: false)
            }
            if first {
                try await Task.sleep(for: .milliseconds(300))
                receiveChannelHintFixture("fixture-channel")
                try await Task.sleep(for: .milliseconds(700))
            }
            return
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-history"), fixtureChannelRefreshes > 1 {
            // Scrolling to the beginning must not trigger a network refresh.
            throw MobileError.Failure(code: "UNAVAILABLE")
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-failure"), fixtureChannelRefreshes == 1 {
            throw MobileError.Failure(code: "UNAVAILABLE")
        }
        try await Task.sleep(for: .milliseconds(200))
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-empty") { return }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-send") { return }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-history") {
            fixtureChannelMessages = (1...5).map { sequence in
                MessageItem(id: "fixture-\(sequence)", sequence: UInt64(sequence), account: "fixture-peer",
                    text: "History message \(sequence)", createdAtMs: 1_793_513_600_000,
                    attachmentName: nil, attachmentMime: nil, attachmentSize: nil, unreadable: false,
                    digest: "fixture-digest-\(sequence)", replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: false)
            }
            return
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-hidden-reply") {
            fixtureChannelMessages = [
                MessageItem(id: "fixture-hidden", sequence: 1, account: "fixture-peer", text: "Hidden original text",
                    createdAtMs: 1_793_513_600_000, attachmentName: nil, attachmentMime: nil, attachmentSize: nil,
                    unreadable: false, digest: "fixture-hidden", replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: true),
                MessageItem(id: "fixture-reply", sequence: 2, account: "fixture-peer", text: "Reply to a hidden message",
                    createdAtMs: 1_793_513_600_001, attachmentName: nil, attachmentMime: nil, attachmentSize: nil,
                    unreadable: false, digest: "fixture-reply", replySequence: 1, edited: false, retracted: false, attachmentDeleted: false, hidden: false),
            ]
            return
        }
        fixtureChannelMessages = [MessageItem(id: "fixture-message", sequence: 1, account: "fixture-peer",
            text: "Hello channel · 频道消息已显示", createdAtMs: 1_793_513_600_000,
            attachmentName: ProcessInfo.processInfo.arguments.contains("--ui-channel-attachment") ? "plan.pdf" : nil, attachmentMime: "application/pdf", attachmentSize: 2048, unreadable: false,
            digest: "fixture-digest", replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: false)]
        // A verified first page is local, but a later relay request is still pending.
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-slow") { try await Task.sleep(for: .seconds(30)) }
    }

    func sendConversationFixture(_ item: OutgoingMessage) async throws -> MessageItem {
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-send-held"), fixtureChannelMessages.isEmpty {
            // Keep the first send pending until the view disappears. This
            // proves the next draft is usable before confirmation, and that
            // navigation cannot own or cancel the delivery task.
            while activeChannel != nil { try await Task.sleep(for: .milliseconds(100)) }
        }
        try await Task.sleep(for: .seconds(8))
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-send-failure") {
            throw MobileError.Failure(code: "EXECUTION_UNKNOWN")
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-channel-send-retry"), !item.attempted {
            throw MobileError.Failure(code: "NETWORK_UNAVAILABLE")
        }
        let sequence = (fixtureChannelMessages.last?.sequence ?? 0) + 1
        let message = MessageItem(id: item.id, sequence: sequence, account: session?.account ?? "fixture-self",
            text: item.text, createdAtMs: 1_793_513_600_000, attachmentName: item.attachmentName,
            attachmentMime: nil, attachmentSize: nil, unreadable: false, digest: "fixture-\(item.id)",
            replySequence: nil, edited: false, retracted: false, attachmentDeleted: false, hidden: false)
        fixtureChannelMessages.append(message)
        return message
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
            vaultFixtureItem(id: "fixture-" + id, revision: "1", kind: kind, title: title, subtitle: subtitle,
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
            LibraryFile(id: "fixture-brief", name: text("Project brief.pdf", "项目简介.pdf"), mime: "application/pdf", size: 245_760, current: "1", sha256: "", status: "backed_up", versions: 1, backup: FileBackupReceipt(uploadId: "fixture-brief-backup", verified: true, backedUpAt: 1_793_534_400_000)),
            LibraryFile(id: "fixture-notes", name: text("Research notes.md", "研究笔记.md"), mime: "text/markdown", size: 8192, current: "1", sha256: "", status: "local", versions: 1),
            LibraryFile(id: "fixture-image", name: text("Design reference.png", "设计参考.png"), mime: "image/png", size: 1_572_864, current: "1", sha256: "", status: "backed_up", versions: 1, backup: FileBackupReceipt(uploadId: "fixture-image-backup", verified: true, backedUpAt: 1_793_534_400_000)),
        ])
    }
}
func vaultFixtureItem(id: String, revision: String, kind: String, title: String, subtitle: String, notes: String,
    favorite: Bool, tags: [String], synced: Bool, conflict: Bool, reserved: Bool, files: [String], fields: [VaultField]) -> VaultItem {
    VaultItem(id: id, revision: revision, kind: kind, title: title, subtitle: subtitle, notes: notes,
        favorite: favorite, tags: tags, synced: synced, conflict: conflict, reserved: reserved, files: files,
        fields: fields, deleted: false, createdAt: 1_793_534_400_000, updatedAt: 1_793_534_400_000)
}

extension SessionModel {
    func saveVaultFixture(target: VaultEditorTarget, edit: VaultEdit) throws {
        fixtureVaultSaveAttempts += 1
        if ProcessInfo.processInfo.arguments.contains("--ui-vault-save-failure") && fixtureVaultSaveAttempts == 1 { throw MobileError.Failure(code: "UNAVAILABLE") }
        let previous = fixtureVaultItems.first { $0.id == target.id }
        let now = UInt64(Date.now.timeIntervalSince1970 * 1000)
        let id = previous?.id ?? "fixture-created"
        let fields = edit.fields.map { field in
            VaultField(id: field.id, kind: field.kind, label: field.label, section: field.section,
                value: field.value ?? previous?.fields.first(where: { $0.id == field.id })?.value ?? "",
                concealed: ["concealed", "otp"].contains(field.kind))
        }
        // Mirror the core's safe list metadata; free-form fields stay in details.
        var subtitle = ""
        if ["login", "api", "server", "database"].contains(edit.kind) {
            let username = fields.first { $0.id == "username" && !$0.concealed }?.value.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !username.isEmpty { subtitle = String(username.split(separator: "\n").first ?? "") }
            else if let field = fields.first(where: { $0.id == "url" && !$0.concealed }), let url = URLComponents(string: field.value),
                    ["https", "http"].contains(url.scheme?.lowercased() ?? ""), let host = url.host {
                subtitle = host + (url.port.map { ":\($0)" } ?? "")
            }
        }
        let item = VaultItem(id: id, revision: UUID().uuidString, kind: edit.kind, title: edit.title,
            subtitle: String(subtitle.prefix(160)), notes: edit.notes, favorite: edit.favorite,
            tags: edit.tags, synced: false, conflict: false, reserved: false, files: edit.files ?? previous?.files ?? [],
            fields: fields, deleted: false, createdAt: previous?.createdAt ?? now, updatedAt: now)
        if let previous { fixtureVaultHistory[id, default: []].append(previous) }
        fixtureVaultItems.removeAll { $0.id == id }; fixtureVaultItems.append(item)
    }

    func fixtureVaultPage(offset: Int, limit: Int) -> (items: [VaultItem], counts: VaultCounts, more: Bool) {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        let inventory = fixtureVaultItems.filter { item in
            !item.deleted && (query.isEmpty || ([item.title, item.notes] + item.tags + item.fields.filter { !$0.concealed }.map(\.value)).contains { $0.localizedCaseInsensitiveContains(query) })
        }
        let tags = Array(Set(inventory.flatMap(\.tags))).sorted()
        let tagged = inventory.filter { vaultTag.isEmpty || $0.tags.contains(vaultTag) }
        var counts = VaultCounts(all: tagged.count, favorites: tagged.filter(\.favorite).count, tags: tags)
        for item in tagged { counts.kinds[item.kind, default: 0] += 1 }
        var filtered = tagged.filter { vaultFilter == "all" || (vaultFilter == "favorites" ? $0.favorite : $0.kind == vaultFilter) }
        filtered.sort { a, b in
            if vaultSort == "title" { return a.title.localizedCaseInsensitiveCompare(b.title) == .orderedAscending }
            return a.updatedAt == b.updatedAt ? a.id < b.id : a.updatedAt > b.updatedAt
        }
        counts.total = filtered.count
        return (Array(filtered.dropFirst(offset).prefix(limit)), counts, offset + limit < filtered.count)
    }
}
#endif

#if DEBUG
extension ContentSyncStatus {
    static func fixture(running: Bool = false, initialized: Bool = false, entries: UInt64 = 0,
                        total: UInt64 = 0, bytes: UInt64 = 0, error: String? = nil,
                        retry: UInt64? = nil, success: UInt64? = nil) -> ContentSyncStatus {
        ContentSyncStatus(running: running, initialized: initialized, stage: "download", entries: entries,
            total: total, downloadedBytes: bytes, elapsedMs: 2500, lastAttemptAtMs: nil,
            lastSuccessAtMs: success, retryAtMs: retry, retryRequired: error != nil,
            failures: error == nil ? 0 : 4, through: 0, error: error)
    }
}
#endif
