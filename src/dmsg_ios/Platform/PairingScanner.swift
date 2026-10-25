import SwiftUI
import VisionKit

/// This scanner only returns text to a review form. It cannot approve a device.
struct PairingScanner: UIViewControllerRepresentable {
    let receive: (String) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(receive: receive) }
    func makeUIViewController(context: Context) -> DataScannerViewController {
        let controller = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .accurate, recognizesMultipleItems: false, isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true, isGuidanceEnabled: true, isHighlightingEnabled: true)
        controller.delegate = context.coordinator
        try? controller.startScanning()
        return controller
    }
    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}
    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) { controller.stopScanning() }

    @MainActor final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let receive: (String) -> Void
        private var received = false
        init(receive: @escaping (String) -> Void) { self.receive = receive }
        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !received else { return }
            for item in addedItems {
                if case .barcode(let code) = item, let text = code.payloadStringValue, text.utf8.count <= 4096 {
                    received = true; dataScanner.stopScanning(); receive(text); break
                }
            }
        }
    }
}
