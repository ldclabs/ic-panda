fn main() {
    println!("cargo:rerun-if-env-changed=DMSG_MOBILE_BUILD_ID");
    let id = std::env::var("DMSG_MOBILE_BUILD_ID").unwrap_or_else(|_| "unversioned".into());
    assert!(id == "unversioned" || (id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())));
    println!("cargo:rustc-env=DMSG_MOBILE_BUILD_ID={id}");
}
