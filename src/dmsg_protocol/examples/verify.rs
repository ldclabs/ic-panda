//! Offline mathematical verification; trust and authorization require separate evidence.
use dmsg_types::SignedArtifact;
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: verify artifact.cbor")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(262_145)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 262_144 {
        return Err("artifact too large".into());
    }
    let artifact: SignedArtifact = cbor2::from_slice(&bytes)?;
    let statement = dmsg_protocol::verify_artifact(&artifact)
        .map_err(|e| format!("verification failed: {e:?}"))?;
    println!("Mathematical signature valid. Identity, authorization and TSA trust have not been evaluated.");
    println!("{statement:?}");
    Ok(())
}
