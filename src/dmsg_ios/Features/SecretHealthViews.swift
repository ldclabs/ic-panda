import SwiftUI

struct SecretHealthView: View {
    @Environment(SessionModel.self) private var model
    @State private var report: SecretHealthReport?
    @State private var includeDismissed = false
    @State private var dismissing: SecretHealthReport.Finding?
    @State private var working = false
    @State private var loading = true
    var body: some View {
        List {
            NoticeBar(text: String(localized: "These are local observations, not a breach scan or proof that a provider revoked a credential. Other devices can have different records."))
            if let report {
                Section("Local findings") {
                    Toggle("Show dismissed findings", isOn: $includeDismissed)
                    ForEach(report.findings.filter { includeDismissed || !$0.dismissed }) { finding in
                        VStack(alignment: .leading, spacing: 8) {
                            Label(secretFindingTitle(finding.rule), systemImage: finding.severity == "high" ? "exclamationmark.shield" : "info.circle")
                                .font(.headline).foregroundStyle(finding.severity == "high" ? Color.red : Color.primary)
                            Text(finding.evidence == "last_known_cloud_state" ? "Last known cloud state" : "Observed on this device")
                                .font(.caption).foregroundStyle(.secondary)
                            Text(Date(timeIntervalSince1970: Double(finding.observedAt) / 1000), style: .date).font(.caption).foregroundStyle(.secondary)
                            if let field = finding.field {
                                NavigationLink("Review field and dependencies") { SecretFieldReferenceView(itemID: finding.subject, fieldID: field) }
                                NavigationLink("Open secret") { VaultDetailView(id: finding.subject) }
                            } else {
                                Text(secretNextAction(finding.action)).font(.subheadline)
                                Text(finding.subject).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(2)
                            }
                            if !finding.dismissed { Button("Dismiss this observation") { dismissing = finding }.disabled(working) }
                        }.padding(.vertical, 5)
                    }
                    if report.findings.filter({ includeDismissed || !$0.dismissed }).isEmpty {
                        Text("No findings from this device's checks.").foregroundStyle(.secondary)
                    }
                }
                Section("Rotation checklists") {
                    ForEach(report.rotations.sorted { $0.updatedAt > $1.updatedAt }) { rotation in
                        NavigationLink { SecretRotationView(initial: rotation) } label: {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(vaultFieldName(rotation.reference.fieldId)).font(.headline)
                                Text(rotation.completedAt == nil ? "In progress" : "Checklist completed").font(.caption).foregroundStyle(.secondary)
                                Text(rotation.reference.itemId).font(.caption.monospaced()).lineLimit(1).foregroundStyle(.secondary)
                            }
                        }
                    }
                    if report.rotations.isEmpty { Text("Start a checklist from a field's reference page.").foregroundStyle(.secondary) }
                }
            } else if loading { ProgressView() }
            else { Button("Retry") { Task { await load() } } }
        }.privateScreen().navigationTitle("Local security review").navigationBarTitleDisplayMode(.inline).task { await load() }.refreshable { await load() }
            .confirmationDialog("Dismiss this observation?", isPresented: Binding(get: { dismissing != nil }, set: { if !$0 { dismissing = nil } })) {
                Button("Dismiss this observation") { if let finding = dismissing { Task { await dismiss(finding) } } }
            } message: { Text("This only hides this exact finding and revision on this device. It does not repair a credential or dismiss future changes.") }
    }
    private func load() async {
        loading = true; defer { loading = false }
        let epoch = model.epoch
        do {
            guard let result = try await model.engine?.secretHealth(), epoch == model.epoch, !Task.isCancelled else { return }
            report = try model.decode(result)
        } catch { if epoch == model.epoch { model.report(error) } }
    }
    private func dismiss(_ finding: SecretHealthReport.Finding) async {
        dismissing = nil
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do { _ = try await engine.secretHealthDismiss(id: finding.id); if epoch == model.epoch { await load() } }
        catch { if epoch == model.epoch { model.report(error) } }
    }
}

func secretFindingTitle(_ rule: String) -> String {
    switch rule {
    case "reused": String(localized: "Credential reused")
    case "weak": String(localized: "Weak credential")
    case "expired": String(localized: "Credential expired")
    case "invalid_policy_reference": String(localized: "Policy points to an unavailable field")
    case "policy_review_due": String(localized: "Device policy needs review")
    case "not_observed_on_this_device": String(localized: "No recent use observed on this device")
    case "invalid_ssh_reference": String(localized: "SSH binding needs review")
    case "invalid_mount_reference": String(localized: "Desktop file mapping needs review")
    case "active_share": String(localized: "A snapshot link is still active")
    case "snapshot_precedes_current_credential": String(localized: "A snapshot contains an older credential")
    case "attachment_backup_unconfirmed": String(localized: "Attachment backup is unconfirmed")
    default: String(localized: "Review local finding")
    }
}

struct SecretRotationView: View {
    let initial: SecretRotation
    @Environment(SessionModel.self) private var model
    @State private var rotation: SecretRotation?
    @State private var dependencies: SecretReferenceIndex?
    @State private var confirmStep: String?
    @State private var working = false
    private var current: SecretRotation { rotation ?? initial }
    var body: some View {
        List {
            NoticeBar(text: String(localized: "You perform provider changes yourself. dMsg records your statements and checks local revisions; it cannot verify upstream creation, testing or revocation."))
            Section("Credential") {
                LabeledContent("Field", value: vaultFieldName(current.reference.fieldId))
                NavigationLink("Open secret") { VaultDetailView(id: current.reference.itemId) }
                FingerprintRow(title: "Original revision", value: current.previousRevision)
                if let revision = current.newRevision { FingerprintRow(title: "New revision", value: revision) }
            }
            Section("Confirm your completed steps") {
                step("Create a new credential at its provider and save it in the vault", key: "created", completed: current.created, enabled: true)
                step("Test the new credential at its provider", key: "tested", completed: current.tested, enabled: current.created)
                step("Review this device's current dependencies", key: "references_checked", completed: current.referencesChecked, enabled: current.tested && dependencies != nil)
                step("Revoke the old credential at its provider", key: "upstream_revoked", completed: current.upstreamRevoked, enabled: current.referencesChecked)
            }
            Section("Current local dependencies") {
                if let dependencies {
                    SecretDependencyRows(index: dependencies)
                    DisclosureGroup("Inventory fingerprint") { FingerprintRow(title: "SHA-256", value: dependencies.digest) }
                } else { ProgressView() }
                Button("Refresh dependencies") { Task { await load() } }.disabled(working)
            }
            if current.completedAt != nil { Text("Checklist completed from your statements. Upstream status remains independently verifiable only with the provider.").font(.footnote).foregroundStyle(.secondary) }
        }.privateScreen().navigationTitle("Credential rotation").navigationBarTitleDisplayMode(.inline).task { await load() }.refreshable { await load() }
            .confirmationDialog("Record this step as completed?", isPresented: Binding(get: { confirmStep != nil }, set: { if !$0 { confirmStep = nil } })) {
                Button("I completed this step") { if let step = confirmStep { Task { await record(step) } } }
            } message: { Text("Confirm only work you have actually completed. This records your statement; it does not execute the provider action.") }
    }

    private func step(_ title: LocalizedStringKey, key: String, completed: Bool, enabled: Bool) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: completed ? "checkmark.circle.fill" : "circle").foregroundStyle(completed ? Theme.forest : Color.secondary)
            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                if !completed && current.completedAt == nil {
                    Button("Confirm completed step") { confirmStep = key }.font(.subheadline).disabled(!enabled || working)
                }
            }
        }.padding(.vertical, 5)
    }

    private func load() async {
        struct Rotations: Decodable { let rotations: [SecretRotation] }
        guard let engine = model.engine else { return }; let epoch = model.epoch
        do {
            let result = try await engine.secretRotations()
            guard epoch == model.epoch else { return }
            let listing: Rotations = try model.decode(result); rotation = listing.rotations.first { $0.id == initial.id }
            let index = try await engine.secretRotationDependencies(id: initial.id)
            guard epoch == model.epoch else { return }; dependencies = try model.decode(index)
        } catch { if epoch == model.epoch { model.report(error) } }
    }

    private func record(_ step: String) async {
        confirmStep = nil
        guard let engine = model.engine, !working else { return }
        let epoch = model.epoch; working = true; model.activity(); defer { working = false }
        do {
            let result = try await engine.secretRotationUpdate(id: current.id, step: step,
                dependenciesDigest: step == "references_checked" ? dependencies?.digest : nil)
            guard epoch == model.epoch else { return }; rotation = try model.decode(result); await load()
        } catch { if epoch == model.epoch { model.report(error) }; await load() }
    }
}
