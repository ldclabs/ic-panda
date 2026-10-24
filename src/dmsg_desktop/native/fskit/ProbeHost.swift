import AppKit
@main struct ProbeHost {
static func main() {
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 560, height: 180), styleMask: [.titled, .closable], backing: .buffered, defer: false)
window.title = "dMsg FSKit — synthetic probe"
let label = NSTextField(wrappingLabelWithString: "This development probe contains fixed synthetic bytes only.\n\nEnable ‘dmsgprobe’ in System Settings → General → Login Items & Extensions → File System Extensions to test delayed reads and uncached expiry. No Vault access or macFUSE runtime is included.")
label.frame = NSRect(x: 24, y: 24, width: 512, height: 130)
window.contentView?.addSubview(label)
window.makeKeyAndOrderFront(nil)
app.run()

}
}
