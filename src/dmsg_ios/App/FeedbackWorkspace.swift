import Foundation
import Observation
@preconcurrency import DMsgCore

/// Explicit links only; receiving a letter never fetches or previews an image.
func feedbackScreenshotURLs(_ text: String) -> [URL] {
    let pattern = #"https://dmsg\.net/s/[0-9a-f]{64}#[A-Za-z0-9_-]{43}(?![A-Za-z0-9_/-])"#
    guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
    var seen = Set<URL>()
    return regex.matches(in: text, range: NSRange(text.startIndex..., in: text)).compactMap { match in
        guard let range = Range(match.range, in: text), let url = URL(string: String(text[range])), seen.insert(url).inserted else { return nil }
        return url
    }
}

struct FeedbackScreenshot: Codable, Identifiable, Equatable, Sendable {
    var id: String { file }
    let file: String
    let version: String
    let name: String
    let size: UInt64
    let mime: String
    var operation: String = ""
    var url: String?
    var expiresAt: UInt64?
    var libraryFile: LibraryFile { LibraryFile(id: file, name: name, mime: mime, size: size, current: version, sha256: "", status: "local", versions: 1) }
}

struct FeedbackRecord: Codable, Sendable {
    var id: String
    var revision: UInt64
    var recipient: String
    var text: String
    var context: String
    var screenshots: [FeedbackScreenshot]
    var state: String
    var error: String?
    var editable: Bool
}

/// One encrypted, device-local report. Navigation never owns its writes or sends.
@MainActor @Observable
final class FeedbackWorkspace {
    static let recipient = "dbrt466u97gecq800000"
    private(set) var record: FeedbackRecord?
    private(set) var text = ""
    private(set) var screenshots: [FeedbackScreenshot] = []
    private(set) var context = ""
    private(set) var loaded = false
    private(set) var dirty = false
    private(set) var saving = false
    private(set) var busy = false
    private(set) var failure: String?
    private var id = NativeApple.requestID()
    private var currentContext = ""
    private var edits: UInt64 = 0
    private var generation: UInt64 = 0
    private var writing: Task<Void, Never>?
    private var sending: Task<Void, Never>?

    var editable: Bool { record == nil || record?.state == "draft" }
    var valid: Bool { text.utf8.count <= 4500 && (!text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !screenshots.isEmpty) }
    var pendingInput: FeedbackDraftInput? {
        guard dirty, editable else { return nil }
        return FeedbackDraftInput(id: id, revision: record?.revision ?? 0, text: text, context: context, files: screenshots.map(\.file))
    }

    func suspend() {
        generation &+= 1; sending?.cancel(); sending = nil; writing?.cancel(); writing = nil
        busy = false; saving = false
        loaded = false
    }

    func reset() {
        suspend(); record = nil; text = ""; screenshots = []; context = ""; loaded = false; dirty = false; failure = nil
        id = NativeApple.requestID(); edits = 0
    }

    private func accept(_ next: FeedbackRecord?) {
        record = next
        if let next { id = next.id; text = next.text; context = next.context; screenshots = next.screenshots }
    }

    func load(_ model: SessionModel) async {
        guard model.unlocked, !loaded, !busy else { return }
        let stamp = generation, epoch = model.epoch; busy = true
        defer { if stamp == generation { busy = false } }
        do {
            if dirty {
                await flush(model)
                guard stamp == generation, epoch == model.epoch else { return }
                if dirty { loaded = true; return }
            }
            let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
            context = "dMsg iOS \(version) · \(ProcessInfo.processInfo.operatingSystemVersionString)"
            currentContext = context
            if !model.uiTesting {
                guard let engine = model.engine else { return }
                let value: FeedbackRecord? = try model.decode(try await engine.feedback())
                guard epoch == model.epoch, stamp == generation, !Task.isCancelled else { return }
                accept(value)
            }
            loaded = true; failure = nil
        } catch { if epoch == model.epoch, !Task.isCancelled { failure = InboxLoading.message(error) } }
    }

    func update(_ text: String, screenshots: [FeedbackScreenshot]? = nil, model: SessionModel) {
        guard loaded, editable, !busy else { return }
        self.text = text
        if let screenshots { self.screenshots = screenshots }
        edits &+= 1; dirty = true; model.activity()
        startWrite(model)
    }

    private func startWrite(_ model: SessionModel) {
        guard writing == nil, dirty else { return }
        let stamp = generation, epoch = model.epoch
        writing = Task {
            saving = true
            var reconciled = false
            defer { if stamp == generation { saving = false; writing = nil } }
            do {
                while dirty, stamp == generation, epoch == model.epoch, !Task.isCancelled, let input = pendingInput {
                    let revision = edits
                    let next: FeedbackRecord
                    if model.uiTesting {
                        next = FeedbackRecord(id: id, revision: input.revision + 1, recipient: Self.recipient, text: input.text, context: input.context, screenshots: self.screenshots, state: "draft", editable: true)
                    } else {
                        guard let engine = model.engine else { return }
                        do { next = try model.decode(try await engine.saveFeedback(draft: input)) }
                        catch {
                            guard case MobileError.Failure(let code) = error, code == "VERSION_CONFLICT", !reconciled else { throw error }
                            let saved: FeedbackRecord? = try model.decode(try await engine.feedback())
                            guard stamp == generation, epoch == model.epoch else { return }
                            guard let saved, saved.id == id, saved.state == "draft" else { throw error }
                            record = saved; reconciled = true; continue
                        }
                    }
                    guard stamp == generation, epoch == model.epoch else { return }
                    record = next; failure = nil
                    if edits == revision { accept(next); dirty = false }
                }
            } catch { if stamp == generation, epoch == model.epoch, !Task.isCancelled { failure = InboxLoading.message(error) } }
        }
    }

    func flush(_ model: SessionModel) async {
        startWrite(model)
        await writing?.value
    }

    func send(_ model: SessionModel) {
        guard !busy, loaded, valid else { return }
        let stamp = generation, epoch = model.epoch
        busy = true; failure = nil
        sending = Task {
            defer { if stamp == generation { busy = false; sending = nil } }
            await flush(model)
            guard stamp == generation, epoch == model.epoch, !Task.isCancelled, !dirty, let original = record else { return }
            do {
                #if DEBUG
                if model.uiTesting {
                    var next = original
                    next.state = "pending"; accept(next)
                    if ProcessInfo.processInfo.arguments.contains("--ui-feedback-failure"), original.error == nil {
                        next.error = "NETWORK_UNAVAILABLE"; accept(next)
                        throw MobileError.Failure(code: "NETWORK_UNAVAILABLE")
                    }
                    next.state = "sent"; next.error = nil; next.editable = false; accept(next)
                    return
                }
                #endif
                guard let engine = model.engine else { return }
                let result = try await coreOperation { try await engine.sendFeedback(id: original.id, revision: original.revision, operation: $0) }
                guard stamp == generation, epoch == model.epoch, !Task.isCancelled else { return }
                accept(try model.decode(result))
            } catch {
                guard stamp == generation, epoch == model.epoch, !Task.isCancelled else { return }
                failure = letterSendFailure(error)
                if !model.uiTesting, let engine = model.engine, let result = try? await engine.feedback(), stamp == generation, epoch == model.epoch {
                    accept(try? model.decode(result))
                    if record?.state == "sent" { failure = nil }
                }
            }
        }
    }

    func edit(_ model: SessionModel) async {
        guard !busy, record?.editable == true, let engine = model.engine else { return }
        let epoch = model.epoch; busy = true; defer { if epoch == model.epoch { busy = false } }
        do {
            let result = try await engine.editFeedback(id: id)
            guard epoch == model.epoch else { return }; accept(try model.decode(result)); failure = nil
        } catch { if epoch == model.epoch { failure = InboxLoading.message(error) } }
    }

    func clear(_ model: SessionModel) async {
        guard !busy else { return }
        let stamp = generation, epoch = model.epoch; busy = true
        defer { if stamp == generation { busy = false } }
        await flush(model)
        guard stamp == generation, epoch == model.epoch else { return }
        do {
            if let record, !model.uiTesting { try await model.engine?.clearFeedback(id: record.id, revision: record.revision) }
            guard stamp == generation, epoch == model.epoch else { return }
            record = nil; id = NativeApple.requestID(); text = ""; context = currentContext; screenshots = []; dirty = false; failure = nil
        } catch { if stamp == generation, epoch == model.epoch { failure = InboxLoading.message(error) } }
    }
}
