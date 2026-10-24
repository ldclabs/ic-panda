//! Real local OS clients against synthetic in-memory fixture credentials only.
use crate::{daemon::Daemon, home::Home, mcp_events::McpEvents};
use dmsg_client::{
    secrets::SecretRef,
    testing::{TempDir, unlocked},
    vault::{Changes, FieldChange},
};
#[cfg(target_os = "linux")]
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};

fn fixture() -> (TempDir, Arc<Daemon>) {
    let mut dir = TempDir::new();
    let short = std::path::Path::new("/tmp").join(format!(
        "dmsg-os-{}",
        &hex::encode(dmsg_client::crypto::random_id())[..16]
    ));
    std::fs::rename(&dir.0, &short).unwrap();
    dir.0 = short;
    let c = unlocked(&dir.0.join("workspace"), false, None);
    let home = Home {
        dir: dir.0.join("home"),
    };
    std::fs::create_dir(&home.dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&home.dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let d = Arc::new(Daemon::new(
        "fixture-owner".into(),
        0,
        home.settings().unwrap(),
        c,
        dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
        McpEvents::new(&[]).unwrap(),
    ));
    d.secret_mounts.bind(Arc::downgrade(&d));
    (dir, d)
}

async fn enable_key(d: &Daemon) -> String {
    let key = dmsg_client::vault::ssh::generate("synthetic-local-integration");
    d.under_lock(async |c| {
        let entry = c.vault_create(
            "ssh",
            &Changes {
                title: Some("Synthetic OS fixture".into()),
                set: Some(vec![FieldChange {
                    id: Some("private_key".into()),
                    value: Some(key.private_key.clone()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )?;
        let reference = SecretRef {
            schema_version: 1,
            account_id: dmsg_client::testing::account().id.to_string(),
            item_id: entry.id,
            field_id: "private_key".into(),
        };
        let r = c.secret_ssh_binding_request(
            "gui",
            reference,
            vec!["git_sign".into(), "ssh_auth".into()],
        )?;
        c.approval_respond(
            r["id"].as_str().unwrap(),
            r["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
    })
    .await
    .unwrap();
    key.public_key
}

struct Approver(tokio::task::JoinHandle<()>);
impl Drop for Approver {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn approve_fixture(d: Arc<Daemon>, kind: &'static str) -> Approver {
    Approver(tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let waiting = d.client().lock().await.approvals(false).unwrap();
            for r in waiting
                .into_iter()
                .filter(|r| r["type"] == kind && r["state"] == "awaiting_user")
            {
                d.under_lock(async |c| {
                    c.approval_respond(
                        r["id"].as_str().unwrap(),
                        r["digest"].as_str().unwrap(),
                        true,
                        "gui",
                        false,
                    )
                    .await
                })
                .await
                .unwrap();
                d.emit("approval/changed", serde_json::Value::Null);
            }
        }
    }))
}

async fn command(
    dir: &Path,
    program: &str,
    args: &[&str],
    socket: Option<&Path>,
) -> std::process::Output {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .kill_on_drop(true)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    if let Some(socket) = socket {
        command.env("SSH_AUTH_SOCK", socket);
    }
    let output = tokio::time::timeout(Duration::from_secs(40), command.output())
        .await
        .expect("OS client timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires installed git and ssh-keygen; creates only synthetic temporary keys"]
async fn real_git_ssh_signature_verifies_with_the_public_key() {
    let (dir, d) = fixture();
    let public = enable_key(&d).await;
    let socket = crate::ssh_agent::start(d.clone(), &dir.0.join("home")).unwrap();
    let path = d.ssh_path.lock().unwrap().clone().unwrap();
    let _approve = approve_fixture(d.clone(), "secret_ssh");
    let repo = dir.0.join("git");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("key.pub"), &public).unwrap();
    std::fs::write(
        repo.join("allowed_signers"),
        format!("fixture@example.invalid {public}\n"),
    )
    .unwrap();
    command(&repo, "git", &["init", "-q"], None).await;
    for args in [
        ["config", "user.name", "Synthetic Fixture"],
        ["config", "user.email", "fixture@example.invalid"],
        ["config", "gpg.format", "ssh"],
        ["config", "user.signingkey", "key.pub"],
        ["config", "gpg.ssh.allowedSignersFile", "allowed_signers"],
    ] {
        command(&repo, "git", &args, None).await;
    }
    command(
        &repo,
        "git",
        &[
            "commit",
            "--allow-empty",
            "-S",
            "-m",
            "Synthetic approval integration",
        ],
        Some(&path),
    )
    .await;
    command(&repo, "git", &["verify-commit", "HEAD"], None).await;
    let records = d.client().lock().await.approvals(true).unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|r| r["type"] == "secret_ssh" && r["state"] == "done")
            .count(),
        1
    );
    drop(socket);
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires an isolated root container with openssh-server; loopback only"]
async fn real_openssh_session_bind_authenticates_against_local_sshd() {
    assert_eq!(
        unsafe { libc::geteuid() },
        0,
        "run only in an isolated test container"
    );
    let (dir, d) = fixture();
    let public = enable_key(&d).await;
    let _socket = crate::ssh_agent::start(d.clone(), &dir.0.join("home")).unwrap();
    let path = d.ssh_path.lock().unwrap().clone().unwrap();
    let _approve = approve_fixture(d, "secret_ssh");
    std::fs::write(dir.0.join("key.pub"), &public).unwrap();
    std::fs::write(dir.0.join("authorized_keys"), &public).unwrap();
    command(
        &dir.0,
        "ssh-keygen",
        &["-q", "-t", "ed25519", "-N", "", "-f", "host"],
        None,
    )
    .await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = format!(
        "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nPermitRootLogin yes\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nAuthenticationMethods publickey\nAllowUsers root\n",
        dir.0.join("host").display(),
        dir.0.join("sshd.pid").display(),
        dir.0.join("authorized_keys").display()
    );
    std::fs::write(dir.0.join("sshd_config"), config).unwrap();
    let mut server = tokio::process::Command::new("/usr/sbin/sshd")
        .args(["-D", "-e", "-f"])
        .arg(dir.0.join("sshd_config"))
        .kill_on_drop(true)
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let host = std::fs::read_to_string(dir.0.join("host.pub")).unwrap();
    std::fs::write(
        dir.0.join("known_hosts"),
        format!("[127.0.0.1]:{port} {host}"),
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let out = command(
        &dir.0,
        "ssh",
        &[
            "-F",
            "/dev/null",
            "-p",
            &port.to_string(),
            "-i",
            "key.pub",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "UserKnownHostsFile=known_hosts",
            "root@127.0.0.1",
            "printf SYNTHETIC_SSH_OK",
        ],
        Some(&path),
    )
    .await;
    assert_eq!(&out.stdout, b"SYNTHETIC_SSH_OK");
    server.kill().await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires /dev/fuse and SYS_ADMIN in an isolated Linux container"]
async fn real_fuse_waits_for_approval_and_revokes_an_open_handle() {
    use std::io::{Read, Seek, SeekFrom};
    let (dir, d) = fixture();
    let project = dir.0.join("project");
    std::fs::create_dir(&project).unwrap();
    let reference = {
        let mut c = d.client().lock().await;
        let item = c
            .vault_create(
                "api",
                &Changes {
                    title: Some("mount integration".into()),
                    set: Some(vec![FieldChange {
                        id: Some("credential".into()),
                        value: Some("SYNTHETIC_FUSE_VALUE".into()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            )
            .unwrap();
        c.secret_ref(item.id, "credential".into()).unwrap()["uri"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let environment_reference = SecretRef::parse(&reference).unwrap();
    let prepared=d.call_secret_mounts("secrets/mounts/prepare",&json!({"input":{"project_label":"fixture","project_directory":project,"mount_point":project.join(".dmsg-secrets"),"entries":[{"relative_path":"secret.txt","source":{"type":"field_utf8","reference":reference}},{"relative_path":".env","source":{"type":"refs_dotenv","variables":[{"name":"TOKEN","reference":environment_reference}]}}],"aliases":[{"name":"secret.txt","entry":"secret.txt"},{"name":".env","entry":".env"}]}})).await.unwrap();
    let r = d
        .call_secret_mounts(
            "secrets/mounts/save_request",
            &json!({"input":prepared["input"],"source":"gui"}),
        )
        .await
        .unwrap();
    d.under_lock(async |c| {
        c.approval_respond(
            r["id"].as_str().unwrap(),
            r["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
    })
    .await
    .unwrap();
    let id = d.client().lock().await.secret_mount_configs().unwrap()[0]
        .id
        .clone();
    d.call_secret_mounts("secrets/mounts/start", &json!({"id":id}))
        .await
        .unwrap();
    let read_path = project.join("secret.txt");
    let mut reading = tokio::task::spawn_blocking(move || {
        let mut f = std::fs::File::open(read_path).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        (f, bytes)
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut reading)
            .await
            .is_err(),
        "bytes released without approval"
    );
    let approver = approve_fixture(d.clone(), "secret_mount_read");
    let (mut handle, bytes) = tokio::time::timeout(Duration::from_secs(5), reading)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&bytes, b"SYNTHETIC_FUSE_VALUE");
    command(&project,"python3",&["-c","from pathlib import Path; assert Path('secret.txt').read_bytes()==b'SYNTHETIC_FUSE_VALUE'"],None).await;
    command(&project,"node",&["-e","if(require('fs').readFileSync('secret.txt','utf8')!=='SYNTHETIC_FUSE_VALUE')process.exit(2)"],None).await;
    let env_path = project.join(".env");
    assert_eq!(
        tokio::task::spawn_blocking(move || std::fs::read(env_path))
            .await
            .unwrap()
            .unwrap(),
        b"TOKEN='SYNTHETIC_FUSE_VALUE'\n"
    );
    command(&project,"python3",&["-c",r#"from pathlib import Path; assert Path('.env').read_bytes()==b"TOKEN='SYNTHETIC_FUSE_VALUE'\n""#],None).await;
    command(&project,"node",&["-e",r#"if(require('fs').readFileSync('.env','utf8')!=="TOKEN='SYNTHETIC_FUSE_VALUE'\n")process.exit(2)"#],None).await;
    let denied = tokio::process::Command::new("runuser")
        .args(["-u", "nobody", "--", "cat"])
        .arg(project.join("secret.txt"))
        .output()
        .await
        .unwrap();
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    drop(approver);
    let mut cancelled = tokio::process::Command::new("python3")
        .args([
            "-c",
            "from pathlib import Path; Path('secret.txt').read_bytes()",
        ])
        .current_dir(&project)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pending = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let requests = d.client().lock().await.approvals(false).unwrap();
            if let Some(request) = requests
                .into_iter()
                .find(|r| r["type"] == "secret_mount_read" && r["state"] == "awaiting_user")
            {
                break request;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    cancelled.kill().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = {
                let mut c = d.client().lock().await;
                let _ = c.approvals(false).unwrap();
                c.approval(pending["id"].as_str().unwrap()).unwrap()
            };
            if state["state"] != "awaiting_user" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("kernel cancellation must invalidate the original pending approval");
    let _ = d
        .under_lock(async |c| {
            c.approval_respond(
                pending["id"].as_str().unwrap(),
                pending["digest"].as_str().unwrap(),
                true,
                "gui",
                false,
            )
            .await
        })
        .await;
    let state = d
        .call_secret_mounts("secrets/mounts/status", &json!({"id":id}))
        .await
        .unwrap();
    assert!(
        !state["core"]["grants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["request_id"] == pending["id"])
    );
    d.call_secret_mounts("secrets/mounts/revoke_reads", &json!({"id":id}))
        .await
        .unwrap();
    tokio::task::spawn_blocking(move || {
        handle.seek(SeekFrom::Start(0)).unwrap();
        assert!(handle.read(&mut [0u8; 1]).is_err());
    })
    .await
    .unwrap();
    d.call_secret_mounts("secrets/mounts/stop", &json!({"id":id}))
        .await
        .unwrap();
    assert!(std::fs::read(project.join("secret.txt")).is_err());
}
