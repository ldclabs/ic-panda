#if DEBUG
import Foundation
@preconcurrency import DMsgCore

/// Fictitious household data for the App Store preview video (`--ui-store-preview`).
/// The takes are driven by `AppPreviewTests` and recorded by
/// `scripts/ios/record-app-preview.sh`; see the preview design in misc/.
extension SessionModel {
    var storePreview: Bool { uiTesting && ProcessInfo.processInfo.arguments.contains("--ui-store-preview") }

    private var previewChinese: Bool { Bundle.main.preferredLocalizations.first?.hasPrefix("zh") == true }

    private func previewText(_ english: String, _ chinese: String) -> String { previewChinese ? chinese : english }

    func loadStorePreview() {
        let text = previewText
        func item(_ id: String, _ kind: String, _ title: String, _ subtitle: String, favorite: Bool = false,
                  notes: String = "", fields: [VaultField] = []) -> VaultItem {
            vaultFixtureItem(id: "preview-" + id, revision: "1", kind: kind, title: title, subtitle: subtitle, notes: notes,
                favorite: favorite, tags: [], synced: true, conflict: false, reserved: false, files: [], fields: fields)
        }
        // The list sorts equal timestamps by id, so the numeric prefixes set the order.
        vault = [
            item("01-travel", "identity", text("Travel documents", "旅行证件"), text("Identity", "身份信息")),
            item("02-bank", "card", text("Bank card", "银行卡"), "•••• 1042"),
            item("03-mail", "login", text("Personal email", "个人邮箱"), "lin@example.com", favorite: true,
                 notes: text("Main email, all bills go here", "常用邮箱，账单都发到这里"), fields: [
                    VaultField(id: "username", kind: "text", label: "", section: "", value: "lin@example.com", concealed: false),
                    VaultField(id: "password", kind: "concealed", label: "", section: "", value: "••••••••", concealed: true),
                    VaultField(id: "url", kind: "url", label: "", section: "", value: "https://mail.example.com", concealed: false),
                 ]),
            item("04-wifi", "note", text("Home Wi-Fi", "家里 Wi-Fi"), ""),
            item("05-health", "card", text("Health insurance", "医保卡"), "•••• 7731"),
            item("06-checkup", "note", text("Checkup notes", "体检记录"), ""),
            item("07-school", "login", text("School account", "孩子学校账号"), "parent@school.example"),
            item("08-car", "note", text("Car insurance", "车辆保险"), ""),
            item("09-address", "note", text("Family addresses", "家庭常用地址"), ""),
            item("10-photos", "login", text("Family photo cloud", "家庭云相册"), "lin.home@example.com"),
        ]
        channels = [
            ChannelItem(id: "preview-family", name: text("Family", "家人"), unread: 0, draft: "", preview: text("Everyone's passport photo is in.", "大家的护照照片都收齐了。")),
            ChannelItem(id: "preview-trip", name: text("Trip planning", "旅行计划"), unread: 0, draft: "", preview: text("The hotel confirmation is in the chat.", "酒店确认单发群里了。")),
            ChannelItem(id: "preview-landlord", name: text("Landlord", "房东"), unread: 0, draft: "", preview: text("The lease looks fine.", "合同我看过了，没问题。")),
        ]
        let receipt = { (id: String) in FileBackupReceipt(uploadId: id + "-backup", verified: true, backedUpAt: 1_793_534_400_000) }
        library = LibraryListing(folder: nil, folders: [], files: [
            LibraryFile(id: "preview-passport", name: text("Passport scan.pdf", "护照扫描件.pdf"), mime: "application/pdf", size: 1_258_291, current: "1", sha256: "", status: "backed_up", versions: 1, backup: receipt("preview-passport")),
            LibraryFile(id: "preview-lease", name: text("Lease.pdf", "租房合同.pdf"), mime: "application/pdf", size: 655_360, current: "1", sha256: "", status: "backed_up", versions: 1, backup: receipt("preview-lease")),
            LibraryFile(id: "preview-checkup", name: text("Health checkup.pdf", "体检报告.pdf"), mime: "application/pdf", size: 2_516_582, current: "1", sha256: "", status: "backed_up", versions: 1, backup: receipt("preview-checkup")),
        ])
        if ProcessInfo.processInfo.arguments.contains("--ui-preview-arrived") {
            let draft = storePreviewRequestDraft()
            let mom = draft.names
            fixtureFileRequests = [FileRequest(requestId: "preview-request", url: "https://dmsg.net/r/preview#key", title: draft.title,
                instructions: draft.instructions, label: mom, expiresAt: UInt64(Date.now.timeIntervalSince1970 * 1000) + 14 * 86_400_000,
                status: "open", files: 3, bytes: 8_178_892, unread: true)]
            fixtureReceivedFiles = [
                (text("Dad.jpg", "爸爸.jpg"), 2_726_297), (text("Mom.jpg", "妈妈.jpg"), 2_516_582), (text("Emma.jpg", "小雨.jpg"), 2_936_013),
            ].enumerated().map { index, file in
                ReceivedFile(uploadId: "preview-upload-\(index)", state: "received", name: file.0, mime: "image/jpeg", size: UInt64(file.1),
                    sender: .init(name: mom, note: ""), bytes: UInt64(file.1) + 4096)
            }
        } else {
            fixtureFileRequests = []
        }
        if ProcessInfo.processInfo.arguments.contains("--ui-preview-files") { selectedTab = .files }
    }

    /// The composer opens already filled in, so the take shows the form rather than typing.
    func storePreviewRequestDraft() -> (title: String, instructions: String, names: String) {
        (previewText("Family passport photos", "全家护照照片"), previewText("A clear photo of the info page", "拍清楚个人信息页"), previewText("Mom", "妈妈"))
    }

    /// Opens the preview workspace after the button's own progress state. The
    /// simulator's Face ID sheet cannot be driven reliably, and the take must not
    /// imitate system UI, so none is shown.
    func unlockStorePreview() async {
        guard let session, session.state != "unlocked", !busy else { return }
        let operation = beginBusyOperation(.unlocking)
        try? await Task.sleep(for: .milliseconds(900))
        _ = finishBusyOperation(operation)
        let channels = channels
        installFixtures(SessionSnapshot(generation: session.generation, sequence: session.sequence + 1, state: "unlocked",
            account: session.account, deviceId: session.deviceId, pendingSync: 0, conflicts: 0, approvalsWaiting: 0,
            fastUnlock: false, loginExpiresAtMs: session.loginExpiresAtMs, loginRenewable: session.loginRenewable,
            passkeyAvailable: session.passkeyAvailable, setupPending: false, lastSyncAtMs: nil, syncError: nil), items: fixtureVaultItems)
        self.channels = channels
        await loadVault()
    }

    /// This iPhone administers the account; the iPad and Mac are ordinary members.
    func storePreviewDevices() -> [DeviceInfo]? {
        guard storePreview else { return nil }
        let devices = [
            DeviceInfo(deviceId: "6f3a91c20b7e44d1a5c8e2f07b19d36e", role: "administrator", capabilities: ["ContentSign", "VaultUnlock", "RootManage"],
                       addedAt: 1_790_000_000_000, addedBy: nil, revokedAt: nil, current: true),
            DeviceInfo(deviceId: "a27d5e10c9f307b8d41e6a2c95b03f7e", role: "member", capabilities: ["ContentSign", "VaultUnlock"],
                       addedAt: 1_791_000_000_000, addedBy: "6f3a91c20b7e44d1a5c8e2f07b19d36e", revokedAt: nil, current: false),
            DeviceInfo(deviceId: "e81c2f469ad0b3357c1e04d8a6f92b5c", role: "member", capabilities: ["ContentSign", "VaultUnlock"],
                       addedAt: 1_792_000_000_000, addedBy: "6f3a91c20b7e44d1a5c8e2f07b19d36e", revokedAt: nil, current: false),
        ]
        setDeviceLabel(devices[1].deviceId, label: previewText("My iPad", "我的 iPad"))
        setDeviceLabel(devices[2].deviceId, label: previewText("My MacBook", "我的 MacBook"))
        return devices
    }
}
#endif
