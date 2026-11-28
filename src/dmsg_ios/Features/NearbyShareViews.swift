import SwiftUI
import VisionKit
import AVFoundation
@preconcurrency import DMsgCore

/// Foreground receive sheet for face-to-face encrypted Files and Vault sharing.
struct NearbyReceiveSheet: View {
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var discoverable = false
    @State private var shareIdentity: [String: Bool] = [:]
    @State private var folder = "root"
    @State private var folderName = String(localized: "Files")
    @State private var choosingFolder = false
    @State private var selectedFiles: [String: Set<UInt32>] = [:]
    @State private var importingPackage: SecretPackagePreview?

    private var nearby: NearbyShareModel { model.nearbyShare }
    private var receive: NearbyShareModel.ReceiveStatus? { nearby.status?.receive }

    var body: some View {
        Form {
            if let err = nearby.error {
                NoticeBar(text: err)
            }
            if nearby.localNetworkDenied {
                NoticeBar(text: String(localized: "Local network access is off for dMsg. Enable it in Settings or scan the QR code to use encrypted relay."))
            }
            if let receive {
                switch receive.state {
                case "waiting":
                    waitingSections(receive)
                case "running":
                    runningSection(receive)
                case "done":
                    doneSection(receive)
                case "failed":
                    Section {
                        NoticeBar(text: receive.error?.message ?? String(localized: "Nearby transfer failed."))
                        SettingsAction("Start again", prominent: true) { Task { await nearby.startReceive(model, discoverable: discoverable) } }
                    }
                default:
                    Section {
                        SettingsAction("Start receiving", prominent: true) { Task { await nearby.startReceive(model, discoverable: discoverable) } }
                    }
                }
            } else {
                Section {
                    ProgressView("Preparing nearby receive…")
                }
            }
        }
        .privateScreen()
        .interactiveDismissDisabled(receive?.active == true || nearby.working)
        .navigationTitle("Receive nearby")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close") {
                    Task {
                        if receive?.active == true {
                            await nearby.cancelReceive(model)
                        }
                        dismiss()
                    }
                }
                .disabled(nearby.working)
            }
        }
        .task {
            if receive?.active != true {
                await nearby.startReceive(model, discoverable: discoverable)
            }
        }
        .onChange(of: discoverable) { _, next in
            Task { await nearby.startReceive(model, discoverable: next) }
        }
        .onChange(of: receive?.outcome?.packagePreview?.digest) { _, _ in
            if let preview = receive?.outcome?.packagePreview {
                importingPackage = preview
            }
        }
        .sheet(isPresented: $choosingFolder) {
            NavigationStack { SecretFolderPicker { id, name in folder = id; folderName = name } }
        }
        .sheet(item: $importingPackage) { preview in
            NavigationStack { SecretPackageImportView(stagedPreview: preview) }
        }
    }

    @ViewBuilder private func waitingSections(_ receive: NearbyShareModel.ReceiveStatus) -> some View {
        Section {
            if let image = QRCode.image(receive.offer) {
                Image(uiImage: image)
                    .interpolation(.none)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: 220)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
                    .accessibilityLabel("Nearby receive QR code")
            }
            CopyButton("Copy connection code", value: receive.offer, centered: true)
            LabeledContent("Connection") {
                Text(receive.relayReady ? "Local + Relay ready" : "Local network only")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Toggle("Allow nearby discovery", isOn: $discoverable)
        } footer: {
            Text("QR-only mode does not broadcast on the local network. Enable discovery to appear in nearby device lists and accept direct peer-to-peer transfers.")
        }

        let codes = receive.verificationCodes ?? []
        if !codes.isEmpty {
            Section("Verification code") {
                ForEach(Array(codes.enumerated()), id: \.offset) { _, sas in
                    Text(NearbyShareModel.groupedSas(sas)).font(.title2.monospacedDigit().bold())
                }
            }
        }
        if !receive.pendingPeers.isEmpty {
            ForEach(receive.pendingPeers) { peer in
                pendingPeerSection(peer)
            }
        } else if codes.isEmpty {
            Section {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Waiting for sender…")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder private func pendingPeerSection(_ peer: NearbyShareModel.PendingPeer) -> some View {
        Section {
            VStack(alignment: .leading, spacing: 6) {
                Text(peer.alias ?? String(localized: "Nearby device"))
                    .font(.headline)
                Text(NearbyShareModel.groupedSas(peer.sas))
                    .font(.title2.monospacedDigit().bold())
                    .accessibilityLabel("Verification code")
                    .accessibilityValue(peer.sas)
                Text(peer.qrBound
                     ? String(localized: "QR verified · Compare this 6-digit code with the sender before accepting.")
                     : String(localized: "Unverified discovery · Compare this 6-digit code face-to-face before accepting."))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            if peer.kind == "files" {
                ForEach(peer.files) { file in
                    let isOn = Binding(
                        get: { selectedFiles[peer.peerId]?.contains(file.index) ?? true },
                        set: { checked in
                            var set = selectedFiles[peer.peerId] ?? Set(peer.files.map(\.index))
                            if checked { set.insert(file.index) } else { set.remove(file.index) }
                            selectedFiles[peer.peerId] = set
                        }
                    )
                    Toggle(isOn: isOn) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(file.name).lineLimit(2)
                            Text(fileSize(file.size)).font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                }
                Button { choosingFolder = true } label: {
                    LabeledContent("Destination folder", value: folderName)
                }
            } else {
                LabeledContent("Vault snapshot") {
                    Text(fileSize(peer.packageSize ?? 0)).font(.footnote).foregroundStyle(.secondary)
                }
            }
            Toggle("Share my dMsg identity", isOn: Binding(
                get: { shareIdentity[peer.peerId] ?? false },
                set: { shareIdentity[peer.peerId] = $0 }
            ))
            HStack(spacing: 12) {
                Button(role: .destructive) {
                    nearby.reject(model, peerId: peer.peerId)
                } label: {
                    Text("Reject").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                Button {
                    let picks: [UInt32]
                    if peer.kind == "files" {
                        let chosen = selectedFiles[peer.peerId] ?? Set(peer.files.map(\.index))
                        picks = peer.files.map(\.index).filter { chosen.contains($0) }
                    } else {
                        picks = []
                    }
                    nearby.accept(model, peerId: peer.peerId, folder: folder, selected: picks, shareIdentity: shareIdentity[peer.peerId] ?? false)
                } label: {
                    Text("Accept").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(peer.kind == "files" && (selectedFiles[peer.peerId]?.isEmpty ?? false))
            }
            .controlSize(.large)
            .listRowInsets(EdgeInsets(top: 10, leading: 16, bottom: 10, trailing: 16))
        } header: {
            Text("Incoming transfer")
        }
    }

    @ViewBuilder private func runningSection(_ receive: NearbyShareModel.ReceiveStatus) -> some View {
        Section("Receiving") {
            ProgressView(
                value: Double(receive.progress.bytes),
                total: Double(max(1, receive.progress.totalBytes))
            )
            if let name = receive.progress.currentName {
                Text(name).lineLimit(2)
            }
            Text("\(fileSize(receive.progress.bytes)) / \(fileSize(receive.progress.totalBytes))")
                .font(.footnote.monospacedDigit())
                .foregroundStyle(.secondary)
            SettingsAction("Cancel", role: .destructive) {
                Task { await nearby.cancelReceive(model) }
            }
        }
    }

    @ViewBuilder private func doneSection(_ receive: NearbyShareModel.ReceiveStatus) -> some View {
        Section("Transfer complete") {
            if let outcome = receive.outcome {
                if outcome.kind == "secret_package", let preview = outcome.packagePreview {
                    Text("Vault snapshot verified on this device. Review and choose which fields to import.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    SettingsAction("Review and import snapshot", prominent: true) {
                        importingPackage = preview
                    }
                } else {
                    ForEach(outcome.files) { file in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(file.name).lineLimit(2)
                            Text(fileSize(file.size)).font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                }
            }
            SettingsAction("Receive more") {
                Task { await nearby.startReceive(model, discoverable: discoverable) }
            }
        }
    }
}

/// Foreground send sheet for sharing one or more local Files library items nearby.
struct NearbySendFilesSheet: View {
    let files: [LibraryFile]
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var offerText = ""
    @State private var shareIdentity = false
    @State private var scanning = false
    @State private var cameraDenied = false

    private var nearby: NearbyShareModel { model.nearbyShare }
    @State private var previousSend: UInt64?
    private var send: NearbyShareModel.SendStatus? {
        guard let send = nearby.status?.send, send.id != previousSend else { return nil }
        return send
    }

    var body: some View {
        Form {
            if let err = nearby.error {
                NoticeBar(text: err)
            }
            if let send, send.active || send.state == "done" || send.state == "failed" {
                NearbySendProgressSection(send: send)
                if send.state == "failed" { SettingsAction("Start again", prominent: true) { previousSend = send.id } }
            } else {
                Section("Files to send") {
                    ForEach(files, id: \.id) { file in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(file.name).lineLimit(2)
                            Text(fileSize(file.size)).font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                }
                NearbyTargetPickerSection(
                    offerText: $offerText,
                    shareIdentity: $shareIdentity,
                    scanning: $scanning,
                    cameraDenied: $cameraDenied,
                    onSendOffer: { text in
                        let items = files.map { NearbyFileItem(fileId: $0.id, versionId: $0.current) }
                        Task { await nearby.sendFiles(model, offer: text, peer: nil, files: items, shareIdentity: shareIdentity) }
                    },
                    onSendPeer: { peer in
                        let items = files.map { NearbyFileItem(fileId: $0.id, versionId: $0.current) }
                        Task { await nearby.sendFiles(model, offer: nil, peer: peer, files: items, shareIdentity: shareIdentity) }
                    }
                )
            }
        }
        .privateScreen()
        .interactiveDismissDisabled(send?.active == true || nearby.working)
        .navigationTitle("Share nearby")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close") {
                    if send?.active == true { nearby.cancelSend(model) }
                    nearby.stopBrowse()
                    dismiss()
                }
                .disabled(nearby.working)
            }
        }
        .onAppear {
            if previousSend == nil { previousSend = send.flatMap { $0.active ? nil : $0.id } ?? 0 }
            nearby.startBrowse()
        }
        .onDisappear { nearby.stopBrowse() }
        .sheet(isPresented: $scanning) {
            QRCodeScanner { text in
                offerText = text
                scanning = false
                let items = files.map { NearbyFileItem(fileId: $0.id, versionId: $0.current) }
                Task { await nearby.sendFiles(model, offer: text, peer: nil, files: items, shareIdentity: shareIdentity) }
            }
        }
    }
}

/// Foreground send sheet for sharing a Vault snapshot (`dmsg-secret-package/1`) nearby.
struct NearbySendVaultSheet: View {
    let item: VaultItem
    @Environment(SessionModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var selectedFields: Set<String> = []
    @State private var includeNotes = false
    @State private var selectedAttachments: Set<String> = []
    @State private var attachmentsTouched = false
    @State private var initializedSelection = false
    @State private var attachmentDetails: [String: LibraryDetails] = [:]
    @State private var offerText = ""
    @State private var shareIdentity = false
    @State private var scanning = false
    @State private var cameraDenied = false

    private var nearby: NearbyShareModel { model.nearbyShare }
    @State private var previousSend: UInt64?
    private var send: NearbyShareModel.SendStatus? {
        guard let send = nearby.status?.send, send.id != previousSend else { return nil }
        return send
    }
    private var populatedFields: [VaultField] { item.fields.filter { !$0.value.isEmpty } }
    private var hasSelection: Bool { !selectedFields.isEmpty || includeNotes || !selectedAttachments.isEmpty }

    var body: some View {
        Form {
            if let err = nearby.error {
                NoticeBar(text: err)
            }
            if let send, send.active || send.state == "done" || send.state == "failed" {
                NearbySendProgressSection(send: send)
                if send.state == "failed" { SettingsAction("Start again", prominent: true) { previousSend = send.id } }
            } else {
                Section("Select fields to share") {
                    ForEach(populatedFields, id: \.id) { field in
                        Toggle(field.label.isEmpty ? vaultFieldName(field.id) : field.label, isOn: Binding(
                            get: { selectedFields.contains(field.id) },
                            set: { if $0 { selectedFields.insert(field.id) } else { selectedFields.remove(field.id) } }
                        )).accessibilityIdentifier("nearby.field.\(field.id)")
                    }
                    if !item.notes.isEmpty {
                        Toggle("Notes", isOn: $includeNotes)
                    }
                }
                if !item.files.isEmpty {
                    Section("Select attachments to share") {
                        ForEach(item.files, id: \.self) { fileId in
                            if let details = attachmentDetails[fileId] {
                                Toggle(isOn: Binding(
                                    get: { selectedAttachments.contains(fileId) },
                                    set: { attachmentsTouched = true; if $0 { selectedAttachments.insert(fileId) } else { selectedAttachments.remove(fileId) } }
                                )) {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(details.file.name).lineLimit(2)
                                        Text(fileSize(details.file.size)).font(.footnote).foregroundStyle(.secondary)
                                    }
                                }
                            }
                        }
                    }
                }
                if hasSelection {
                    NearbyTargetPickerSection(
                        offerText: $offerText,
                        shareIdentity: $shareIdentity,
                        scanning: $scanning,
                        cameraDenied: $cameraDenied,
                        onSendOffer: { text in
                            Task { await nearby.sendVault(model, offer: text, peer: nil, input: makeInput(), shareIdentity: shareIdentity) }
                        },
                        onSendPeer: { peer in
                            Task { await nearby.sendVault(model, offer: nil, peer: peer, input: makeInput(), shareIdentity: shareIdentity) }
                        }
                    )
                } else {
                    Section {
                        Text("Choose at least one field, note or attachment before selecting a nearby receiver.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        .privateScreen()
        .interactiveDismissDisabled(send?.active == true || nearby.working)
        .navigationTitle("Share nearby")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Close") {
                    if send?.active == true { nearby.cancelSend(model) }
                    nearby.stopBrowse()
                    dismiss()
                }
                .disabled(nearby.working)
            }
        }
        .task {
            if previousSend == nil { previousSend = send.flatMap { $0.active ? nil : $0.id } ?? 0 }
            if !initializedSelection {
                selectedFields = Set(populatedFields.map(\.id))
                includeNotes = !item.notes.isEmpty
                initializedSelection = true
            }
            nearby.startBrowse()
            for fileId in item.files {
                if let details = try? await model.fileDetails(fileId) {
                    attachmentDetails[fileId] = details
                    if !attachmentsTouched && details.file.trashedAt == nil && details.versions.first(where: { $0.id == details.file.current })?.local == true {
                        selectedAttachments.insert(fileId)
                    }
                }
            }
        }
        .onDisappear { nearby.stopBrowse() }
        .sheet(isPresented: $scanning) {
            QRCodeScanner { text in
                offerText = text
                scanning = false
                if hasSelection {
                    Task { await nearby.sendVault(model, offer: text, peer: nil, input: makeInput(), shareIdentity: shareIdentity) }
                }
            }
        }
    }

    private func makeInput() -> SecretSnapshotInput {
        let attachments = selectedAttachments.compactMap { id -> SecretAttachmentSelection? in
            guard let version = attachmentDetails[id]?.file.current else { return nil }
            return SecretAttachmentSelection(fileId: id, versionId: version)
        }
        return SecretSnapshotInput(
            itemId: item.id,
            revision: item.revision,
            fieldIds: Array(selectedFields),
            includeNotes: includeNotes,
            attachments: attachments,
            expiresInMs: nil,
            maxDownloadSessions: nil
        )
    }
}

private struct NearbyTargetPickerSection: View {
    @Environment(SessionModel.self) private var model
    @Binding var offerText: String
    @Binding var shareIdentity: Bool
    @Binding var scanning: Bool
    @Binding var cameraDenied: Bool
    let onSendOffer: (String) -> Void
    let onSendPeer: (NearbyDiscoveredPeer) -> Void

    private var nearby: NearbyShareModel { model.nearbyShare }

    var body: some View {
        Section("Receiver QR or connection code") {
            if DataScannerViewController.isSupported {
                SettingsAction("Scan receiver QR code", systemImage: "qrcode.viewfinder", prominent: true) {
                    Task {
                        let allowed = await AVCaptureDevice.requestAccess(for: .video)
                        cameraDenied = !allowed || !DataScannerViewController.isAvailable
                        scanning = !cameraDenied
                    }
                }
            }
            if cameraDenied {
                NoticeBar(text: String(localized: "Camera access is unavailable. Paste the connection code or allow camera access in Settings."))
            }
            TextField("Paste dmsg-nearby/1: code", text: $offerText, axis: .vertical)
                .lineLimit(2...4)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Toggle("Share my dMsg identity", isOn: $shareIdentity)
            SettingsAction("Send via code") {
                onSendOffer(offerText.trimmingCharacters(in: .whitespacesAndNewlines))
            }
            .disabled(offerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || nearby.working)
        }
        Section {
            if nearby.peers.isEmpty {
                HStack(spacing: 10) {
                    if nearby.browsing { ProgressView() }
                    Text("Looking for nearby dMsg devices…")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            } else {
                ForEach(nearby.peers) { peer in
                    Button {
                        onSendPeer(peer)
                    } label: {
                        LabeledContent(peer.alias) {
                            Text(peer.pkHint).font(.footnote.monospaced()).foregroundStyle(.secondary)
                        }
                    }
                    .disabled(nearby.working)
                }
            }
        } header: {
            Text("Nearby devices")
        } footer: {
            Text("Only devices that turned on nearby discovery appear here. You can always scan the receiver's QR code instead.")
        }
    }
}

private struct NearbySendProgressSection: View {
    @Environment(SessionModel.self) private var model
    let send: NearbyShareModel.SendStatus

    var body: some View {
        Section("Transfer status") {
            if let sas = send.sas {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Verification code").font(.footnote).foregroundStyle(.secondary)
                    Text(NearbyShareModel.groupedSas(sas))
                        .font(.title2.monospacedDigit().bold())
                }
            }
            if let transport = send.transport {
                LabeledContent("Transport", value: transport.uppercased())
            }
            switch send.state {
            case "connecting":
                HStack(spacing: 10) { ProgressView(); Text("Connecting…") }
            case "awaiting_sas":
                NoticeBar(text: String(localized: "Confirm that the receiver shows this exact 6-digit code before sending any details."))
                SettingsAction("Codes match — continue", prominent: true) {
                    model.nearbyShare.confirmSas(model)
                }
            case "awaiting_accept":
                HStack(spacing: 10) { ProgressView(); Text("Waiting for receiver to accept…") }
            case "running":
                ProgressView(
                    value: Double(send.progress.bytes),
                    total: Double(max(1, send.progress.totalBytes))
                )
                Text("\(fileSize(send.progress.bytes)) / \(fileSize(send.progress.totalBytes))")
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(.secondary)
            case "done":
                Label("Sent successfully", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(Theme.forest)
            case "failed":
                NoticeBar(text: send.error?.message ?? String(localized: "Nearby transfer failed."))
            default:
                Text(send.state).foregroundStyle(.secondary)
            }
            if send.active {
                SettingsAction("Cancel", role: .destructive) {
                    model.nearbyShare.cancelSend(model)
                }
            }
        }
    }
}

extension SecretPackagePreview: Identifiable {
    var id: String { packageId + ":" + digest }
}
