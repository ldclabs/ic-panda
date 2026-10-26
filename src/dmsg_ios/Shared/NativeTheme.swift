import UIKit

enum NativeTheme {
    static let forest = UIColor { $0.userInterfaceStyle == .dark
        ? UIColor(red: 0.44, green: 0.78, blue: 0.64, alpha: 1)
        : UIColor(red: 0.078, green: 0.36, blue: 0.27, alpha: 1) }
    static let paper = UIColor { $0.userInterfaceStyle == .dark
        ? UIColor(red: 0.055, green: 0.082, blue: 0.071, alpha: 1)
        : UIColor(red: 0.97, green: 0.973, blue: 0.957, alpha: 1) }
}
