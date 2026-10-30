import Foundation

/// Keep the directory at `path` (UTF-8, `length` bytes) out of Time Machine
/// and the other backups that honor the flag: 0 when set, 1 for a path that
/// is not UTF-8, -1 when the file system refused.
@_cdecl("dmsg_exclude_from_backup")
public func dmsg_exclude_from_backup(_ path: UnsafePointer<UInt8>, _ length: Int) -> Int32 {
    guard let text = String(bytes: UnsafeBufferPointer(start: path, count: length), encoding: .utf8) else {
        return 1
    }
    var url = URL(fileURLWithPath: text, isDirectory: true)
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    do {
        try url.setResourceValues(values)
        return 0
    } catch {
        return -1
    }
}
