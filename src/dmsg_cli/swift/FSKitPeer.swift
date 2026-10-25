// Authenticate the filesystem module, not the unknown application reading a file.
// A secret-files extension must run signed by the daemon's own team, with FSKit entitlement.
import Foundation
import Security

/// The team that signed this process; nil when unsigned or ad hoc.
func ownTeam() -> String? {
    var own: SecCode?
    guard SecCodeCopySelf([], &own) == errSecSuccess, let own else { return nil }
    var ownStatic: SecStaticCode?
    guard SecCodeCopyStaticCode(own, [], &ownStatic) == errSecSuccess, let ownStatic else { return nil }
    var info: CFDictionary?
    guard SecCodeCopySigningInformation(ownStatic, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
          let values = info as? [String: Any],
          let team = values[kSecCodeInfoTeamIdentifier as String] as? String,
          team.count == 10, team.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) }) else { return nil }
    return team
}

@_cdecl("dmsg_fskit_peer_allowed")
public func dmsgFSKitPeerAllowed(_ pid: Int32, _ identifier: UnsafePointer<CChar>?) -> Int32 {
    guard pid > 0, let identifier else { return 0 }
    let bundleID = String(cString: identifier)
    guard !bundleID.isEmpty, bundleID.count <= 128,
          bundleID.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 46 || $0 == 45 }) else { return 0 }
    guard let team = ownTeam() else { return 0 }
    var guest: SecCode?
    guard SecCodeCopyGuestWithAttributes(nil, [kSecGuestAttributePid: NSNumber(value: pid)] as CFDictionary, [], &guest) == errSecSuccess,
          let guest else { return 0 }
    // Boolean entitlements match with `exists`; `= true` is not valid requirement syntax.
    let expression = "anchor apple generic and identifier \"\(bundleID)\" and certificate leaf[subject.OU] = \"\(team)\" and entitlement[\"com.apple.developer.fskit.fsmodule\"] exists"
    var requirement: SecRequirement?
    guard SecRequirementCreateWithString(expression as CFString, [], &requirement) == errSecSuccess, let requirement else { return 0 }
    return SecCodeCheckValidity(guest, SecCSFlags(rawValue: kSecCSStrictValidate), requirement) == errSecSuccess ? 1 : 0
}

/// The App Group container `<team>.net.dmsg.secrets` the sandboxed module may
/// connect into. Writes its path to `buffer` and returns the length, or -1
/// without this team's signature and group entitlement.
@_cdecl("dmsg_fskit_ipc_root")
public func dmsgFSKitIPCRoot(_ buffer: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int {
    guard let buffer, let team = ownTeam(),
          let root = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "\(team).net.dmsg.secrets"),
          (try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])) != nil
    else { return -1 }
    let path = Array(root.path.utf8)
    guard path.count <= capacity else { return -1 }
    buffer.update(from: path, count: path.count)
    return path.count
}
