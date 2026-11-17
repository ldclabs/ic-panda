//! Public Android proof commitment fixture; no real token or installation.
use dmsg_protocol::android::{AndroidAssertion, request_hash};

fn main() {
    let assertion = AndroidAssertion {
        account: "111111111111111111111111".into(),
        device: "22".repeat(32),
        installation: "33".repeat(32),
        purpose: "push".into(),
        digest: "44".repeat(32),
        nonce: "55".repeat(32),
        environment: "local".into(),
        application: "net.dmsg.android.qa".into(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "protocol": "dmsg-android/1", "assertion": assertion,
            "request_hash": request_hash(&assertion)
        }))
        .unwrap()
    );
}
