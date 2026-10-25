// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "DMsgCore", platforms: [.iOS(.v18)],
    products: [.library(name: "DMsgCore", targets: ["DMsgCore"])],
    targets: [
        .binaryTarget(name: "DMsgCoreFFI", path: "Artifacts/DMsgCoreFFI.xcframework"),
        .target(name: "DMsgCore", dependencies: ["DMsgCoreFFI"], path: "Generated",
                exclude: ["DMsgCoreFFI.h", "DMsgCoreFFI.modulemap"],
                linkerSettings: [.linkedFramework("Security"), .linkedFramework("SystemConfiguration"),
                                 .linkedLibrary("resolv"), .linkedLibrary("iconv")]),
    ], swiftLanguageModes: [.v5]
)
