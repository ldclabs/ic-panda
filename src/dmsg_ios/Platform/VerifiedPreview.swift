import SwiftUI
import QuickLook

/// The core verified this exact file version before returning the URL. Editing
/// is disabled because a temporary preview is not an editable library version.
struct VerifiedPreview: UIViewControllerRepresentable {
    let url: URL
    @Environment(\.dismiss) private var dismiss
    func makeCoordinator() -> Coordinator { Coordinator(url: url, close: { dismiss() }) }
    func makeUIViewController(context: Context) -> UINavigationController {
        let preview = QLPreviewController()
        preview.dataSource = context.coordinator
        preview.delegate = context.coordinator
        preview.navigationItem.rightBarButtonItem = UIBarButtonItem(barButtonSystemItem: .done,
            target: context.coordinator, action: #selector(Coordinator.done))
        return UINavigationController(rootViewController: preview)
    }
    func updateUIViewController(_ controller: UINavigationController, context: Context) {}

    @MainActor final class Coordinator: NSObject, QLPreviewControllerDataSource, QLPreviewControllerDelegate {
        let url: URL
        let close: () -> Void
        init(url: URL, close: @escaping () -> Void) { self.url = url; self.close = close }
        func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }
        func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> any QLPreviewItem { url as NSURL }
        nonisolated func previewController(_ controller: QLPreviewController, editingModeFor previewItem: any QLPreviewItem) -> QLPreviewItemEditingMode { .disabled }
        @objc func done() { close() }
    }
}
