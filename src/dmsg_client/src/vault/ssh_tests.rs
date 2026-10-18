use super::*;

#[test]
fn generates_openssh_keys() {
    let key = generate("me@host");
    assert!(key.private_key.starts_with(BEGIN));
    assert!(key.private_key.ends_with(&format!("{END}\n")));
    assert!(
        key.private_key
            .lines()
            .all(|l| l.len() <= 70 || l.starts_with("-----"))
    );
    assert!(
        key.public_key
            .starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA")
    );
    assert!(key.public_key.ends_with(" me@host"));
    assert!(key.fingerprint.starts_with("SHA256:"));
    assert_eq!(key.fingerprint.len(), 7 + 43);
    assert_eq!(
        inspect(&key.private_key),
        Some((key.public_key.clone(), key.fingerprint.clone()))
    );
    assert_ne!(generate("").public_key, key.public_key);
    // OpenSSH reads it, where it is installed.
    let dir = crate::testing::TempDir::new();
    let path = dir.0.join("id_ed25519");
    std::fs::write(&path, &key.private_key).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    if let Ok(out) = std::process::Command::new("ssh-keygen")
        .arg("-y")
        .arg("-f")
        .arg(&path)
        .output()
    {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8(out.stdout).unwrap().trim(),
            key.public_key
        );
    }
    assert_eq!(generate("").public_key.split(' ').count(), 2);
}

#[test]
fn reads_the_public_key_of_other_keys() {
    // `ssh-keygen -t ecdsa -C ec -N ""`, a test-only key.
    let pasted = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAaAAAABNlY2RzYS
1zaGEyLW5pc3RwMjU2AAAACG5pc3RwMjU2AAAAQQTo3Bc/Jw9F2DdllV6VP3ChvlPCPCti
XQG+O46Z7Jb7VsFsOsn+ncpmnhRhzAFQhLGIMNx4ESVGBO28ea1GQ7QvAAAAoO7hOsTu4T
rEAAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBOjcFz8nD0XYN2WV
XpU/cKG+U8I8K2JdAb47jpnslvtWwWw6yf6dymaeFGHMAVCEsYgw3HgRJUYE7bx5rUZDtC
8AAAAhALBxOSnHttYTGEkD8ZokL0xnSlpHnFR5gTQnftoGkDWWAAAAAmVjAQIDBAU=
-----END OPENSSH PRIVATE KEY-----
";
    let (public, fingerprint) = inspect(pasted).unwrap();
    assert_eq!(
        public,
        "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBOjcFz8nD0XYN2WVXpU/cKG+U8I8K2JdAb47jpnslvtWwWw6yf6dymaeFGHMAVCEsYgw3HgRJUYE7bx5rUZDtC8= ec"
    );
    assert_eq!(
        fingerprint,
        "SHA256:QpI6P0mLylt1tWd6ufh873H7lEKe0uIelRzlN/TN/6Y"
    );
    for wrong in [
        "",
        "-----BEGIN OPENSSH PRIVATE KEY-----\n!!\n-----END OPENSSH PRIVATE KEY-----",
    ] {
        assert_eq!(inspect(wrong), None);
    }
}
