// A registration host for local synthetic integration tests. No dMsg home or URL handler.
import AppKit
@main struct DevelopmentHost {
    static func main() {
        let app = NSApplication.shared; app.setActivationPolicy(.regular)
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 580, height: 190), styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "dMsg FSKit — development host"
        let label = NSTextField(wrappingLabelWithString: "This host registers the native dMsg secret-files extension. It does not open your dMsg account or run a daemon.\n\nUse only the isolated fixture daemon for development tests. Every file read still requires that daemon's owner approval. macOS grants one file/version to the mounting user for a short window, including other programs of that user.")
        label.frame = NSRect(x: 24, y: 20, width: 532, height: 150); window.contentView?.addSubview(label)
        window.makeKeyAndOrderFront(nil); app.run()
    }
}
