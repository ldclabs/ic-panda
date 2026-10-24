//! On macOS, build `swift/Enclave.swift` into a static library: the
//! daemon's Secure Enclave keys go through CryptoKit, which only Swift
//! reaches (`src/enclave.rs`). Other targets build nothing.

use std::path::PathBuf;
use std::process::Command;

fn output(command: &mut Command) -> String {
    let out = command.output().expect("run xcrun");
    assert!(
        out.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 output")
}

fn main() {
    println!("cargo:rerun-if-changed=swift/Enclave.swift");
    println!("cargo:rerun-if-changed=swift/FSKitPeer.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").expect("target arch") {
        arch if arch == "aarch64" => "arm64".to_string(),
        arch => arch,
    };
    let target = format!("{arch}-apple-macosx11.0");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let sdk = output(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]));
    let sdk = sdk.trim();
    output(
        Command::new("xcrun")
            .args(["--sdk", "macosx", "swiftc", "-parse-as-library", "-O"])
            .args(["-swift-version", "5", "-module-name", "DmsgEnclave"])
            .args(["-emit-library", "-static", "-target", &target, "-sdk", sdk])
            .arg("swift/Enclave.swift")
            .arg("swift/FSKitPeer.swift")
            .arg("-o")
            .arg(out.join("libdmsg_enclave.a")),
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=dmsg_enclave");
    // The Swift runtime and the compatibility libraries the object autolinks.
    let info: serde_json::Value = serde_json::from_str(&output(Command::new("xcrun").args([
        "--sdk",
        "macosx",
        "swiftc",
        "-print-target-info",
        "-target",
        &target,
    ])))
    .expect("swiftc target info");
    for path in info["paths"]["runtimeLibraryPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str())
    {
        println!("cargo:rustc-link-search=native={path}");
    }
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    for framework in ["CryptoKit", "LocalAuthentication", "Security", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
