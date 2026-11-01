import SwiftUI

struct DisplayCacheView: View {
    @Environment(SessionModel.self) private var model
    @State private var bytes: UInt64?
    @State private var working = false
    @State private var failed = false
    private struct Status: Decodable {
        struct Domain: Decodable { let sealedBytes: UInt64 }
        let domains: [Domain]
    }

    var body: some View {
        List {
            Text("Encrypted display data, attachment copies and cloud file lists can be downloaded again. Clearing keeps your vault, messages, library files and unsynced work.")
                .font(.footnote).foregroundStyle(.secondary).accessibilityIdentifier("cache.explanation")
            if let bytes { LabeledContent("On this device", value: ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)) }
            if working { ProgressView() }
            Button("Clear cached data") { Task { await clear() } }.accessibilityIdentifier("cache.clear")
                .disabled(working || bytes == nil || bytes == 0 || failed)
            if failed { Button("Retry") { Task { await load() } } }
        }.privateScreen().navigationTitle("Cached data").task { await load() }
    }

    private func load() async {
        guard let engine = model.engine else { return }
        working = true; defer { working = false }
        do {
            let status: Status = try model.decode(try await engine.displayCacheStatus())
            bytes = status.domains.reduce(0) { $0 + $1.sealedBytes }; failed = false
        } catch { failed = true; model.report(error) }
    }

    private func clear() async {
        guard let engine = model.engine, !working else { return }
        working = true
        do { try await engine.clearDisplayCache(); await load() }
        catch { failed = true; model.report(error) }
        working = false
    }
}
