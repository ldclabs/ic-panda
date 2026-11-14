import ImageIO
import SwiftUI
@preconcurrency import DMsgCore

/// Small previews of images in the library, held in memory until the workspace
/// locks. One file is opened at a time, and only for as long as it takes to read.
@MainActor
final class ThumbnailStore {
    private static let maxBytes: UInt64 = 20 * 1024 * 1024
    private let cache = NSCache<NSString, UIImage>()
    private var tail: Task<Void, Never>?

    static func supports(_ file: LibraryFile) -> Bool {
        file.canOpen && file.mime.hasPrefix("image/") && file.size <= maxBytes
    }

    func cached(_ file: LibraryFile) -> UIImage? { cache.object(forKey: Self.key(file)) }

    func image(for file: LibraryFile, model: SessionModel) async -> UIImage? {
        if let hit = cached(file) { return hit }
        guard Self.supports(file) else { return nil }
        let previous = tail
        let work = Task<UIImage?, Never> { [weak self] in
            await previous?.value
            guard let self, !Task.isCancelled else { return nil }
            return await self.read(file, model: model)
        }
        tail = Task { _ = await work.value }
        return await withTaskCancellationHandler { await work.value } onCancel: { work.cancel() }
    }

    func reset() { cache.removeAllObjects() }

    private func read(_ file: LibraryFile, model: SessionModel) async -> UIImage? {
        if let hit = cached(file) { return hit }
        #if DEBUG
        if model.uiTesting {
            let image = UIGraphicsImageRenderer(size: CGSize(width: 240, height: 160)).image { context in
                NativeTheme.forest.setFill(); context.fill(context.format.bounds)
            }
            cache.setObject(image, forKey: Self.key(file))
            return image
        }
        #endif
        struct Opened: Decodable { let path: String; let nonce: String }
        guard let engine = model.engine else { return nil }
        let stamp = model.epoch
        guard let result = try? await coreOperation({ try await engine.openFile(id: file.id, version: file.current, operation: $0) }),
              let opened = try? JSONDecoder().decode(Opened.self, from: Data(result.json.utf8)) else { return nil }
        let image = await Self.downsample(URL(fileURLWithPath: opened.path))
        try? await engine.closeFile(nonce: opened.nonce)
        guard stamp == model.epoch, let image else { return nil }
        cache.setObject(image, forKey: Self.key(file))
        return image
    }

    private nonisolated static func downsample(_ url: URL) async -> UIImage? {
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true,
                                        kCGImageSourceCreateThumbnailWithTransform: true,
                                        kCGImageSourceShouldCacheImmediately: true,
                                        kCGImageSourceThumbnailMaxPixelSize: 640]
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        return UIImage(cgImage: image)
    }

    private static func key(_ file: LibraryFile) -> NSString { "\(file.id):\(file.current)" as NSString }
}

/// The image's preview, or nothing while there is none to show.
struct LibraryThumbnail: View {
    let file: LibraryFile
    var contentMode: ContentMode = .fill
    @Environment(SessionModel.self) private var model
    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image { Image(uiImage: image).resizable().aspectRatio(contentMode: contentMode).accessibilityHidden(true) }
            else { Color.clear }
        }.task(id: "\(file.id):\(file.current):\(model.viewGeneration)") {
            image = model.thumbnails.cached(file)
            if image == nil { image = await model.thumbnails.image(for: file, model: model) }
        }
    }
}
