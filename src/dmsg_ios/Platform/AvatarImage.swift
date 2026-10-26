import ImageIO
import UIKit

/// Decode only the pixels the header can display, off the main actor.
enum AvatarImage {
    static func decode(_ bytes: Data?) async -> UIImage? {
        guard let bytes else { return nil }
        let image = await Task.detached(priority: .utility) { () -> CGImage? in
            guard let source = CGImageSourceCreateWithData(bytes as CFData, [kCGImageSourceShouldCache: false] as CFDictionary) else { return nil }
            return CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 180,
                kCGImageSourceShouldCacheImmediately: true,
            ] as CFDictionary)
        }.value
        return image.map(UIImage.init(cgImage:))
    }
}

extension AvatarImage {
    static func prepare(_ url: URL) async throws -> Data {
        try await Task.detached(priority: .utility) {
            guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
                  let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                    kCGImageSourceCreateThumbnailFromImageAlways: true,
                    kCGImageSourceCreateThumbnailWithTransform: true,
                    kCGImageSourceThumbnailMaxPixelSize: 512,
                    kCGImageSourceShouldCacheImmediately: true,
                  ] as CFDictionary) else { throw CocoaError(.fileReadCorruptFile) }
            let data = NSMutableData()
            guard let output = CGImageDestinationCreateWithData(data, "public.jpeg" as CFString, 1, nil) else { throw CocoaError(.coderInvalidValue) }
            CGImageDestinationAddImage(output, image, [kCGImageDestinationLossyCompressionQuality: 0.85] as CFDictionary)
            guard CGImageDestinationFinalize(output), data.length <= 512 * 1024 else { throw CocoaError(.fileReadTooLarge) }
            return data as Data
        }.value
    }
}
