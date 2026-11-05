import SwiftUI

/// The system camera, for photos and videos. It returns them only to this app:
/// nothing here writes to the Photos library, so no photo permission is needed.
struct CameraCapture: UIViewControllerRepresentable {
    /// `nil` means the person cancelled.
    let finish: (Result<PhotoImport, Error>?) -> Void

    static var isAvailable: Bool { UIImagePickerController.isSourceTypeAvailable(.camera) }

    func makeCoordinator() -> Coordinator { Coordinator(finish: finish) }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let controller = UIImagePickerController()
        controller.sourceType = .camera
        controller.mediaTypes = UIImagePickerController.availableMediaTypes(for: .camera) ?? []
        controller.videoQuality = .typeHigh
        // At up to 130 MB a minute this stays within the library's 1000 MiB file limit.
        controller.videoMaximumDuration = 7 * 60
        controller.delegate = context.coordinator
        return controller
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    @MainActor final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let finish: (Result<PhotoImport, Error>?) -> Void

        init(finish: @escaping (Result<PhotoImport, Error>?) -> Void) { self.finish = finish }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            // A recording's temporary file belongs to the picker, so take it before it closes.
            finish(Result {
                if let movie = info[.mediaURL] as? URL { return try PhotoImport.capturedMovie(movie, at: .now) }
                guard let photo = info[.originalImage] as? UIImage else { throw CocoaError(.fileReadUnknown) }
                return try PhotoImport.capturedPhoto(photo, at: .now)
            })
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { finish(nil) }
    }
}
