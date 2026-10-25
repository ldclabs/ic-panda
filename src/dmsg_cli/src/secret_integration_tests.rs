//! Real local OS clients against synthetic in-memory fixture credentials only.
use crate::{daemon::Daemon, home::Home, mcp_events::McpEvents};
use dmsg_client::{
    secrets::SecretRef,
    testing::{TempDir, unlocked},
    vault::{Changes, FieldChange},
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
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
    let binary: Vec<u8> = (0..1_048_593).map(|i| (i % 256) as u8).collect();
    let files = [
        ("secret.bin", binary.clone()),
        ("service.json", br#"{"token":"SYNTHETIC_JSON"}"#.to_vec()),
        (
            "service.pem",
            b"-----BEGIN PRIVATE KEY-----\nSYNTHETIC_PEM\n-----END PRIVATE KEY-----\n".to_vec(),
        ),
    ];
    let mut file_entries = Vec::new();
    for (name, bytes) in &files {
        let source = dir.0.join(name);
        std::fs::write(&source, bytes).unwrap();
        let imported = d
            .client()
            .lock()
            .await
            .files_import(&source, "root", false, &mut |_, _| {})
            .unwrap();
        file_entries.push(json!({"relative_path":name,"source":{"type":"library_file","file_id":imported.file.id,"version_id":imported.version}}));
        std::fs::remove_file(&source).unwrap();
    }
    let environment_reference = SecretRef::parse(&reference).unwrap();
    let prepared=d.call_secret_mounts("secrets/mounts/prepare",&json!({"input":{"project_label":"fixture","project_directory":project,"mount_point":project.join(".dmsg-secrets"),"entries":[{"relative_path":"secret.txt","source":{"type":"field_utf8","reference":reference}},{"relative_path":".env","source":{"type":"refs_dotenv","variables":[{"name":"TOKEN","reference":environment_reference}]}}],"aliases":[{"name":"secret.txt","entry":"secret.txt"},{"name":".env","entry":".env"}]}})).await.unwrap();
    let mut prepared = prepared;
    prepared["input"]["entries"]
        .as_array_mut()
        .unwrap()
        .extend(file_entries);
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
    for (name, expected) in files {
        let path = project.join(".dmsg-secrets").join(name);
        let actual = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(actual, expected);
    }
    command(&project,"node",&["-e",r#"if(JSON.parse(require('fs').readFileSync('.dmsg-secrets/service.json','utf8')).token!=='SYNTHETIC_JSON')process.exit(2)"#],None).await;
    // Real worker and framework env loaders, when the isolated fixture installs them.
    command(&project,"node",&["-e",r#"const {Worker}=require('node:worker_threads'); const w=new Worker(String.raw`const fs=require('fs');if(fs.readFileSync('.env','utf8')!=="TOKEN='SYNTHETIC_FUSE_VALUE'\n")process.exit(2)`,{eval:true});w.on('exit',n=>{if(n)process.exit(n)})"#],None).await;
    if let Ok(loaders) = std::env::var("DMSG_ENV_LOADERS") {
        command(&project,"node",&["--input-type=module","-e",r#"const {loadEnv}=await import(process.argv[1]+'/node_modules/vite/dist/node/index.js');if(loadEnv('development',process.cwd(),'').TOKEN!=='SYNTHETIC_FUSE_VALUE')process.exit(2)"#,&loaders],None).await;
        command(&project,"node",&["-e",r#"const {loadEnvConfig}=require(process.argv[1]+'/node_modules/@next/env');if(loadEnvConfig(process.cwd(),true,console,true).combinedEnv.TOKEN!=='SYNTHETIC_FUSE_VALUE')process.exit(2)"#,&loaders],None).await;
    }
    if unsafe { libc::geteuid() } == 0 {
        let denied = tokio::process::Command::new("runuser")
            .args(["-u", "nobody", "--", "cat"])
            .arg(project.join("secret.txt"))
            .output()
            .await
            .unwrap();
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
    }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_socket_keeps_unknown_extensions_and_closes_a_broken_wait() {
    use dmsg_client::secrets::ssh::{put, take};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixStream,
    };
    async fn write(stream: &mut UnixStream, frame: &[u8]) {
        stream.write_u32(frame.len() as u32).await.unwrap();
        stream.write_all(frame).await.unwrap();
    }
    async fn call(stream: &mut UnixStream, frame: &[u8]) -> Vec<u8> {
        write(stream, frame).await;
        let mut reply = vec![0; stream.read_u32().await.unwrap() as usize];
        stream.read_exact(&mut reply).await.unwrap();
        reply
    }
    let (dir, d) = fixture();
    enable_key(&d).await;
    let _socket = crate::ssh_agent::start(d.clone(), &dir.0.join("home")).unwrap();
    let path = d.ssh_path.lock().unwrap().clone().unwrap();
    let mut stream = UnixStream::connect(&path).await.unwrap();
    let mut query = vec![27];
    put(&mut query, b"query");
    assert_eq!(call(&mut stream, &query).await, [5]);
    // Still framed: identities follow on the same connection.
    let identities = call(&mut stream, &[11]).await;
    assert_eq!(identities[..5], [12, 0, 0, 0, 1]);
    let mut rest = &identities[5..];
    let public = take(&mut rest).unwrap().to_vec();
    let mut data = b"SSHSIG".to_vec();
    for field in [&b"git"[..], b"", b"sha256", &[7; 32]] {
        put(&mut data, field);
    }
    let mut sign = vec![13];
    put(&mut sign, &public);
    put(&mut sign, &data);
    sign.extend_from_slice(&[0; 4]);
    write(&mut stream, &sign).await;
    let waiting = async {
        loop {
            let pending = d.client().lock().await.approvals(false).unwrap();
            if pending.iter().any(|r| r["type"] == "secret_ssh") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap();
    // Input while waiting ends the connection: no reply on misaligned framing.
    stream.write_all(&[0]).await.unwrap();
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    assert!(rest.is_empty(), "{rest:?}");
    let records = d.client().lock().await.approvals(true).unwrap();
    assert!(
        records
            .iter()
            .any(|r| r["type"] == "secret_ssh" && r["state"] == "failed")
    );
}

/// The read approval awaiting a person for one mapped path.
#[cfg(target_os = "macos")]
async fn awaiting_read(d: &Daemon, path: &str) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let found = d
                .client()
                .lock()
                .await
                .approvals(false)
                .unwrap()
                .into_iter()
                .find(|r| {
                    r["type"] == "secret_mount_read"
                        && r["state"] == "awaiting_user"
                        && r["detail"]["entry"]["relative_path"] == path
                });
            if let Some(request) = found {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a content read waits for approval")
}

#[cfg(target_os = "macos")]
async fn approve(d: &Daemon, request: &serde_json::Value) -> dmsg_types::Result<serde_json::Value> {
    let result = d
        .under_lock(async |c| {
            c.approval_respond(
                request["id"].as_str().unwrap(),
                request["digest"].as_str().unwrap(),
                true,
                "gui",
                false,
            )
            .await
        })
        .await;
    d.emit("approval/changed", serde_json::Value::Null);
    result
}

/// Whether the system lists a volume mounted at this directory.
#[cfg(target_os = "macos")]
fn mounted(path: &Path) -> bool {
    let path = std::fs::canonicalize(path.parent().unwrap())
        .unwrap()
        .join(path.file_name().unwrap());
    let listed = std::process::Command::new("/sbin/mount").output().unwrap();
    String::from_utf8_lossy(&listed.stdout).contains(&format!(" on {} (", path.display()))
}

#[cfg(target_os = "macos")]
async fn decline(d: &Daemon, request: &serde_json::Value) {
    d.under_lock(async |c| {
        c.approval_respond(
            request["id"].as_str().unwrap(),
            request["digest"].as_str().unwrap(),
            false,
            "gui",
            false,
        )
        .await
    })
    .await
    .unwrap();
    d.emit("approval/changed", serde_json::Value::Null);
}

/// Another program of the same user reading a whole file.
#[cfg(target_os = "macos")]
fn reader(project: &Path, path: &str) -> tokio::process::Child {
    tokio::process::Command::new("python3")
        .args([
            "-c",
            "import sys; sys.stdout.buffer.write(open(sys.argv[1], 'rb').read())",
            path,
        ])
        .current_dir(project)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[cfg(target_os = "macos")]
async fn read_output(child: tokio::process::Child) -> Option<Vec<u8>> {
    let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
        .await
        .expect("reader finished")
        .unwrap();
    output.status.success().then_some(output.stdout)
}

/// Approval leaves the request; no grant may come from it any more.
#[cfg(target_os = "macos")]
async fn abandoned(d: &Daemon, id: &str, request: &serde_json::Value) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = d
                .client()
                .lock()
                .await
                .approval(request["id"].as_str().unwrap())
                .unwrap();
            if state["state"] != "awaiting_user" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a cancelled read invalidates its pending approval");
    assert!(approve(d, request).await.is_err());
    let status = d
        .call_secret_mounts("secrets/mounts/status", &json!({"id":id}))
        .await
        .unwrap();
    assert!(
        !status["core"]["grants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["request_id"] == request["id"])
    );
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the enabled dMsg FSKit module and this test binary signed by its team with the App Group entitlement"]
async fn real_fskit_window_reads_expire_revoke_and_stop() {
    use std::io::{Read, Seek, SeekFrom};
    const WINDOW: u64 = 4000;
    const VALUE: &[u8] = b"SYNTHETIC_FSKIT_VALUE";
    struct Unmount(std::path::PathBuf);
    impl Drop for Unmount {
        fn drop(&mut self) {
            let _ = std::process::Command::new("/sbin/umount")
                .arg("-f")
                .arg(&self.0)
                .output();
        }
    }
    let (dir, d) = fixture();
    let project = dir.0.join("project");
    std::fs::create_dir(&project).unwrap();
    let volume = project.join(".dmsg-secrets");
    let _unmount = Unmount(volume.clone());
    let (item, reference) = {
        let mut c = d.client().lock().await;
        let item = c
            .vault_create(
                "api",
                &Changes {
                    title: Some("FSKit integration".into()),
                    set: Some(vec![FieldChange {
                        id: Some("credential".into()),
                        value: Some(String::from_utf8(VALUE.to_vec()).unwrap()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            )
            .unwrap();
        let uri = c.secret_ref(item.id.clone(), "credential".into()).unwrap()["uri"]
            .as_str()
            .unwrap()
            .to_owned();
        (item.id, uri)
    };
    let binary: Vec<u8> = (0..1_048_593).map(|i| (i % 251) as u8).collect();
    let files = [
        ("secret.bin", binary.clone()),
        ("service.json", br#"{"token":"SYNTHETIC_JSON"}"#.to_vec()),
        (
            "service.pem",
            b"-----BEGIN PRIVATE KEY-----\nSYNTHETIC_PEM\n-----END PRIVATE KEY-----\n".to_vec(),
        ),
    ];
    let mut file_entries = Vec::new();
    for (name, bytes) in &files {
        let source = dir.0.join(name);
        std::fs::write(&source, bytes).unwrap();
        let imported = d
            .client()
            .lock()
            .await
            .files_import(&source, "root", false, &mut |_, _| {})
            .unwrap();
        file_entries.push(json!({"relative_path":name,"source":{"type":"library_file","file_id":imported.file.id,"version_id":imported.version}}));
        std::fs::remove_file(&source).unwrap();
    }
    let environment = SecretRef::parse(&reference).unwrap();
    let mut prepared = d
        .call_secret_mounts(
            "secrets/mounts/prepare",
            &json!({"window_ms":WINDOW,"input":{"project_label":"fixture","project_directory":project,"mount_point":volume,"entries":[{"relative_path":"secret.txt","source":{"type":"field_utf8","reference":reference}},{"relative_path":".env","source":{"type":"refs_dotenv","variables":[{"name":"TOKEN","reference":environment}]}}],"aliases":[{"name":"secret.txt","entry":"secret.txt"},{"name":".env","entry":".env"}]}}),
        )
        .await
        .unwrap();
    assert_eq!(prepared["preview"]["same_user_other_programs"], true);
    prepared["input"]["entries"]
        .as_array_mut()
        .unwrap()
        .extend(file_entries);
    let r = d
        .call_secret_mounts(
            "secrets/mounts/save_request",
            &json!({"input":prepared["input"],"source":"gui","window_ms":WINDOW}),
        )
        .await
        .unwrap();
    approve(&d, &r).await.unwrap();
    let id = d.client().lock().await.secret_mount_configs().unwrap()[0]
        .id
        .clone();
    let started = d
        .call_secret_mounts("secrets/mounts/start", &json!({"id":id}))
        .await;
    if let Ok(expected) = std::env::var("DMSG_FSKIT_UNAVAILABLE") {
        // A disabled module or an unsigned daemon: a named cause, never a plaintext fallback.
        let error = started.unwrap_err();
        assert!(
            matches!(&error, dmsg_types::Error::Unavailable(m) if m.starts_with(&format!("{expected}:"))),
            "{error:?}"
        );
        let status = d
            .call_secret_mounts("secrets/mounts/status", &json!({"id":id}))
            .await
            .unwrap();
        assert_eq!(status["state"], "stopped");
        assert_eq!(status["error"], expected.as_str());
        assert!(std::fs::read(volume.join("secret.txt")).is_err());
        assert!(std::fs::read(project.join("secret.txt")).is_err());
        assert!(!mounted(&volume));
        return;
    }
    started.unwrap();
    assert!(mounted(&volume));
    let blocking =
        async |work: Box<dyn FnOnce() + Send>| tokio::task::spawn_blocking(work).await.unwrap();

    // Metadata needs no approval and reports true sizes; writes are refused.
    let (v, b) = (volume.clone(), binary.len() as u64);
    blocking(Box::new(move || {
        assert_eq!(std::fs::metadata(v.join("secret.bin")).unwrap().len(), b);
        assert_eq!(
            std::fs::metadata(v.join("secret.txt")).unwrap().len(),
            VALUE.len() as u64
        );
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(v.join("secret.txt"))
                .is_err()
        );
        assert!(std::fs::write(v.join("new.txt"), b"x").is_err());
    }))
    .await;

    // The first content read waits for a person, then the same call continues.
    let path = project.join("secret.txt");
    let first = tokio::task::spawn_blocking(move || {
        let mut f = std::fs::File::open(path).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        (f, bytes)
    });
    let pending = awaiting_read(&d, "secret.txt").await;
    assert_eq!(pending["detail"]["scope"]["type"], "macos_file_window");
    assert_eq!(pending["detail"]["scope"]["window_ms"], WINDOW);
    assert!(!first.is_finished(), "bytes released without approval");
    approve(&d, &pending).await.unwrap();
    let approved = std::time::Instant::now();
    let (mut held, bytes) = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, VALUE);

    // Inside the window other programs of the same user read without a new approval.
    assert_eq!(
        read_output(reader(&project, "secret.txt")).await.unwrap(),
        VALUE
    );
    let node = tokio::process::Command::new("node")
        .args([
            "-e",
            "process.stdout.write(require('fs').readFileSync('secret.txt'))",
        ])
        .current_dir(&project)
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    assert_eq!(read_output(node).await.unwrap(), VALUE);
    if tokio::process::Command::new("sudo")
        .args(["-n", "true"])
        .status()
        .await
        .is_ok_and(|s| s.success())
    {
        let denied = tokio::process::Command::new("sudo")
            .args(["-n", "-u", "nobody", "/bin/cat"])
            .arg(volume.join("secret.txt"))
            .output()
            .await
            .unwrap();
        assert!(!denied.status.success() && denied.stdout.is_empty());
        eprintln!("cross-UID read denied inside the window");
    } else {
        eprintln!("cross-UID check skipped: sudo needs a password");
    }

    // A different file is not covered by that window.
    let env = reader(&project, ".env");
    let pending = awaiting_read(&d, ".env").await;
    approve(&d, &pending).await.unwrap();
    assert_eq!(
        read_output(env).await.unwrap(),
        [&b"TOKEN='"[..], VALUE, &b"'\n"[..]].concat()
    );

    // Files versions are exact bytes, including a ranged read past the first MiB.
    for (name, expected) in &files {
        let path = format!(".dmsg-secrets/{name}");
        let child = reader(&project, &path);
        let pending = awaiting_read(&d, name).await;
        approve(&d, &pending).await.unwrap();
        assert_eq!(&read_output(child).await.unwrap(), expected, "{name}");
    }
    let (v, expected) = (volume.clone(), binary.clone());
    blocking(Box::new(move || {
        let mut f = std::fs::File::open(v.join("secret.bin")).unwrap();
        f.seek(SeekFrom::Start(1_048_000)).unwrap();
        let mut range = Vec::new();
        f.read_to_end(&mut range).unwrap();
        assert_eq!(range, expected[1_048_000..]);
    }))
    .await;

    // Reads never renew the window: at its fixed end even this open handle needs
    // a new approval, and a declined one fails the read.
    let reading = tokio::task::spawn_blocking(move || {
        let mut byte = [0u8; 1];
        loop {
            if held.seek(SeekFrom::Start(0)).is_err() || held.read(&mut byte).is_err() {
                return held;
            }
            assert!(
                approved.elapsed() < Duration::from_secs(20),
                "window renewed"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let pending = awaiting_read(&d, "secret.txt").await;
    let ended = approved.elapsed();
    assert!(
        ended >= Duration::from_millis(WINDOW - 300)
            && ended < Duration::from_millis(WINDOW + 1500),
        "{ended:?}"
    );
    eprintln!("window ended {ended:?} after approval");
    decline(&d, &pending).await;
    let held = tokio::time::timeout(Duration::from_secs(5), reading)
        .await
        .unwrap()
        .unwrap();
    // New opens of the path reach a new approval too; a cancelled one cannot be approved late.
    let mut late = reader(&project, "secret.txt");
    let pending = awaiting_read(&d, "secret.txt").await;
    late.kill().await.unwrap();
    abandoned(&d, &id, &pending).await;

    // Revocation is confirmed by the module; afterwards every read needs a new approval.
    let child = reader(&project, "secret.txt");
    let pending = awaiting_read(&d, "secret.txt").await;
    approve(&d, &pending).await.unwrap();
    assert_eq!(read_output(child).await.unwrap(), VALUE);
    d.call_secret_mounts("secrets/mounts/revoke_reads", &json!({"id":id}))
        .await
        .unwrap();
    let mut held = held;
    let reading = tokio::task::spawn_blocking(move || {
        held.seek(SeekFrom::Start(0)).is_ok() && held.read(&mut [0u8; 1]).is_ok()
    });
    let pending = awaiting_read(&d, "secret.txt").await;
    decline(&d, &pending).await;
    assert!(!reading.await.unwrap());
    let mut again = reader(&project, "secret.txt");
    let pending = awaiting_read(&d, "secret.txt").await;
    again.kill().await.unwrap();
    abandoned(&d, &id, &pending).await;

    // A changed source refuses reads at once; the periodic check stops the volume.
    let path = project.join("secret.txt");
    let opening = tokio::task::spawn_blocking(move || {
        let mut f = std::fs::File::open(path).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        (f, bytes)
    });
    let pending = awaiting_read(&d, "secret.txt").await;
    approve(&d, &pending).await.unwrap();
    let (mut held, _) = opening.await.unwrap();
    d.client()
        .lock()
        .await
        .vault_edit(
            &item,
            None,
            &Changes {
                set: Some(vec![FieldChange {
                    id: Some("credential".into()),
                    value: Some("SYNTHETIC_FSKIT_CHANGED".into()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
    let held = tokio::task::spawn_blocking(move || {
        assert!(held.seek(SeekFrom::Start(0)).is_err() || held.read(&mut [0u8; 1]).is_err());
        held
    })
    .await
    .unwrap();
    drop(held);
    d.check_secret_mounts().await;
    let status = d
        .call_secret_mounts("secrets/mounts/status", &json!({"id":id}))
        .await
        .unwrap();
    assert_eq!(status["state"], "stopped", "{status}");
    assert_eq!(status["error"], "SOURCE_CHANGED_RESTART_REQUIRED");
    assert!(!mounted(&volume));

    // A restart serves the current source; locking stops and unmounts the volume.
    d.call_secret_mounts("secrets/mounts/start", &json!({"id":id}))
        .await
        .unwrap();
    let child = reader(&project, "secret.txt");
    let pending = awaiting_read(&d, "secret.txt").await;
    approve(&d, &pending).await.unwrap();
    assert_eq!(
        read_output(child).await.unwrap(),
        b"SYNTHETIC_FSKIT_CHANGED"
    );
    d.client().lock().await.lock();
    d.stop_secret_mounts().await;
    assert!(!mounted(&volume));
    let alias = project.join("secret.txt");
    assert!(
        tokio::task::spawn_blocking(move || std::fs::read(alias))
            .await
            .unwrap()
            .is_err()
    );
}
