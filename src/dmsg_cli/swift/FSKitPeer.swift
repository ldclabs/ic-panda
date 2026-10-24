// Authenticate the filesystem module, not the unknown application reading a file.
// A secret-files extension must run signed by the daemon's own team, with FSKit entitlement.
import Foundation
import Security

@_cdecl("dmsg_fskit_peer_allowed")
public func dmsgFSKitPeerAllowed(_ pid: Int32, _ identifier: UnsafePointer<CChar>?) -> Int32 {
    guard pid > 0, let identifier else { return 0 }
    let bundleID = String(cString: identifier)
    guard !bundleID.isEmpty, bundleID.count <= 128,
          bundleID.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 46 || $0 == 45 }) else { return 0 }
    var own: SecCode?
    guard SecCodeCopySelf([], &own) == errSecSuccess, let own else { return 0 }
    var ownStatic: SecStaticCode?
    guard SecCodeCopyStaticCode(own, [], &ownStatic) == errSecSuccess, let ownStatic else { return 0 }
    var info: CFDictionary?
    guard SecCodeCopySigningInformation(ownStatic, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
          let values = info as? [String: Any],
          let team = values[kSecCodeInfoTeamIdentifier as String] as? String,
          team.count == 10, team.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) }) else { return 0 }
    var guest: SecCode?
    guard SecCodeCopyGuestWithAttributes(nil, [kSecGuestAttributePid: NSNumber(value: pid)] as CFDictionary, [], &guest) == errSecSuccess,
          let guest else { return 0 }
    let expression = "anchor apple generic and identifier \"\(bundleID)\" and certificate leaf[subject.OU] = \"\(team)\" and entitlement[\"com.apple.developer.fskit.fsmodule\"] = true"
    var requirement: SecRequirement?
    guard SecRequirementCreateWithString(expression as CFString, [], &requirement) == errSecSuccess, let requirement else { return 0 }
    return SecCodeCheckValidity(guest, SecCSFlags(rawValue: kSecCSStrictValidate), requirement) == errSecSuccess ? 1 : 0
}
