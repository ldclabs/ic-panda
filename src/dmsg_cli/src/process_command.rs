//! Secret delivery stays in the initiating CLI, never daemon child processes.
use crate::remote::{Failure, Remote};
use dmsg_client::secrets::process::{FileBinding, FileIdentity, RunInput};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

#[derive(clap::Args)]
pub struct RunCommand {
    /// UTF-8 NAME=dmsg-secret://… references, never shell expressions.
    #[arg(long)]
    env_file: Option<PathBuf>,
    /// Inherited sequential pipe: NAME=dmsg-secret://… or dmsg-file://file/version.
    #[arg(long = "file")]
    files: Vec<String>,
    /// Explicit executable and arguments. No implicit shell.
    #[arg(required = true, trailing_var_arg = true)]
    program: Vec<String>,
}

fn invalid(message: &str) -> Failure {
    Failure::new("INVALID_INPUT", message)
}

fn identity(path: &Path, limit: u64, retain: bool) -> Result<(FileIdentity, Vec<u8>), Failure> {
    let path = std::fs::canonicalize(path).map_err(|_| invalid("file path"))?;
    let mut f = std::fs::File::open(&path).map_err(|_| invalid("file open"))?;
    let before = f.metadata().map_err(|_| invalid("file metadata"))?;
    if !before.is_file() || before.len() > limit {
        return Err(invalid("file size or type"));
    }
    let mut bytes = Vec::new();
    let mut hash = Sha256::new();
    let mut consumed = 0u64;
    let mut block = Zeroizing::new([0u8; 65536]);
    loop {
        let count = f.read(&mut block[..]).map_err(|_| invalid("file read"))?;
        if count == 0 {
            break;
        }
        consumed += count as u64;
        if consumed > limit {
            return Err(invalid("file size"));
        }
        hash.update(&block[..count]);
        if retain {
            bytes.extend_from_slice(&block[..count]);
        }
    }
    let after = f.metadata().map_err(|_| invalid("file metadata"))?;
    if consumed != before.len()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(invalid("file changed"));
    }
    #[cfg(unix)]
    let stamp = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", before.dev(), before.ino())
    };
    #[cfg(not(unix))]
    let stamp = String::new();
    Ok((
        FileIdentity {
            path: path.to_str().ok_or_else(|| invalid("UTF-8 path"))?.into(),
            identity: stamp,
            sha256: hex::encode(hash.finalize()),
        },
        bytes,
    ))
}

fn executable(name: &str) -> Result<FileIdentity, Failure> {
    let path = if name.contains('/') {
        PathBuf::from(name)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(name))
            .find(|p| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    p.metadata()
                        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                }
                #[cfg(not(unix))]
                {
                    p.is_file()
                }
            })
            .ok_or_else(|| invalid("executable not found in PATH"))?
    };
    identity(&path, 1024 * 1024 * 1024, false).map(|v| v.0)
}

#[cfg(unix)]
pub async fn run(remote: &Remote, command: RunCommand) -> Result<(), Failure> {
    let executable = executable(&command.program[0])?;
    let (refs_file, refs) = match command.env_file {
        Some(path) => {
            let (i, b) = identity(&path, 65536, true)?;
            (
                Some(i),
                String::from_utf8(b).map_err(|_| invalid("UTF-8 references"))?,
            )
        }
        None => (None, String::new()),
    };
    dmsg_client::secrets::process::parse_refs(refs.as_bytes())
        .map_err(|_| invalid("reference document"))?;
    let files = command
        .files
        .iter()
        .map(|s| {
            s.split_once('=')
                .map(|(name, source)| FileBinding {
                    name: name.into(),
                    source: source.into(),
                })
                .ok_or_else(|| invalid("--file NAME=source"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let input = RunInput {
        executable,
        argv: command.program[1..].to_vec(),
        cwd: std::fs::canonicalize(".")
            .map_err(|_| invalid("cwd"))?
            .to_str()
            .ok_or_else(|| invalid("UTF-8 cwd"))?
            .into(),
        refs_file,
        refs,
        files,
    };
    let mut pending = remote
        .call(
            "secrets/run/request",
            serde_json::to_value(&input).map_err(|_| invalid("run plan"))?,
        )
        .await?;
    let claim = Zeroizing::new(
        pending["claim"]
            .take()
            .as_str()
            .ok_or_else(|| invalid("claim response"))?
            .to_owned(),
    );
    let id = pending["request_id"]
        .as_str()
        .ok_or_else(|| invalid("request response"))?
        .to_owned();
    eprintln!("dMsg: waiting for approval ({id})");
    loop {
        let status = remote.call("approval/show", json!({"id":id})).await?;
        match status["state"].as_str() {
            Some("executing") => break,
            Some("awaiting_user") => {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await
            }
            _ => return Err(Failure::new("FORBIDDEN", "run was not approved")),
        }
    }
    // Refuse changed disk input before even claiming, and executable again after.
    verify(&input)?;
    let mut delivered = remote
        .call(
            "secrets/run/claim",
            json!({"request_id":id,"claim":&*claim}),
        )
        .await?;
    let result = (|| {
        verify(&input)?;
        let mut process = std::process::Command::new(&input.executable.path);
        process.args(&input.argv).current_dir(&input.cwd);
        for entry in delivered["env"]
            .as_array_mut()
            .ok_or_else(|| invalid("delivery"))?
        {
            let name = entry["name"]
                .as_str()
                .ok_or_else(|| invalid("delivery"))?
                .to_owned();
            let value = Zeroizing::new(
                entry["value"]
                    .take()
                    .as_str()
                    .ok_or_else(|| invalid("delivery"))?
                    .to_owned(),
            );
            process.env(name, &*value);
        }
        let mut pipes = Vec::new();
        for entry in delivered["files"]
            .as_array_mut()
            .ok_or_else(|| invalid("delivery"))?
        {
            use base64::Engine;
            use std::os::fd::AsRawFd;
            let encoded = Zeroizing::new(
                entry["base64url"]
                    .take()
                    .as_str()
                    .ok_or_else(|| invalid("delivery"))?
                    .to_owned(),
            );
            let bytes = Zeroizing::new(
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(encoded.as_bytes())
                    .map_err(|_| invalid("delivery"))?,
            );
            let (read, write) = pipe()?;
            process.env(
                entry["name"].as_str().ok_or_else(|| invalid("delivery"))?,
                format!("/dev/fd/{}", read.as_raw_fd()),
            );
            pipes.push((read, write, bytes));
        }
        spawn(process, pipes)
    })();
    let (started, code) = match &result {
        Ok(code) => (true, Some(*code)),
        Err(_) => (false, None),
    };
    let _ = remote
        .call(
            "secrets/run/report",
            json!({"request_id":id,"claim":&*claim,"started":started,"exit_code":code}),
        )
        .await;
    let code = result?;
    if code == 0 {
        Ok(())
    } else {
        Err(Failure::new("PROCESS_EXIT", code.to_string()))
    }
}

#[cfg(not(unix))]
pub async fn run(_remote: &Remote, _command: RunCommand) -> Result<(), Failure> {
    Err(Failure::new(
        "UNSUPPORTED_PROTOCOL",
        "run requires macOS or Linux",
    ))
}

fn verify(input: &RunInput) -> Result<(), Failure> {
    if identity(Path::new(&input.executable.path), 1024 * 1024 * 1024, false)?.0 != input.executable
    {
        return Err(invalid("executable changed after approval"));
    }
    if let Some(refs) = &input.refs_file
        && identity(Path::new(&refs.path), 65536, false)?.0 != *refs
    {
        return Err(invalid("references changed after approval"));
    }
    Ok(())
}

#[cfg(unix)]
fn pipe() -> Result<(std::fs::File, std::fs::File), Failure> {
    use std::os::fd::FromRawFd;
    let mut fds = [-1; 2];
    // Both ends are close-on-exec until the child's pre_exec selectively opens readers.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return Err(Failure::new("UNAVAILABLE", "pipe"));
        }
        let pair = (
            std::fs::File::from_raw_fd(fds[0]),
            std::fs::File::from_raw_fd(fds[1]),
        );
        if libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC) < 0
            || libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC) < 0
        {
            return Err(Failure::new("UNAVAILABLE", "pipe flags"));
        }
        Ok(pair)
    }
}

#[cfg(unix)]
fn spawn(
    mut process: std::process::Command,
    pipes: Vec<(std::fs::File, std::fs::File, Zeroizing<Vec<u8>>)>,
) -> Result<i32, Failure> {
    use std::io::Write;
    use std::os::{
        fd::AsRawFd,
        unix::process::{CommandExt, ExitStatusExt},
    };
    let fds: Vec<_> = pipes.iter().map(|(r, _, _)| r.as_raw_fd()).collect();
    // Parent alone ignores Ctrl-C; its child remains in the same foreground group.
    let old = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
    struct Restore(libc::sighandler_t);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                libc::signal(libc::SIGINT, self.0);
            }
        }
    }
    let _restore = Restore(old);
    unsafe {
        process.pre_exec(move || {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            for fd in &fds {
                if libc::fcntl(*fd, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = process
        .spawn()
        .map_err(|_| Failure::new("UNAVAILABLE", "program could not start"))?;
    drop(process); // Command owns its environment copy until dropped.
    // Detached pipe writers exit on EOF/EPIPE; never delay a child which chooses not to read.
    for (read, mut write, bytes) in pipes {
        drop(read);
        std::thread::spawn(move || {
            let _ = write.write_all(&bytes);
        });
    }
    let status = child
        .wait()
        .map_err(|_| Failure::new("UNAVAILABLE", "program wait failed"))?;
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    #[test]
    fn executable_and_reference_file_changes_are_checked_before_spawn() {
        let dir = dmsg_client::testing::TempDir::new();
        let program = dir.0.join("program");
        let refs = dir.0.join(".env.refs");
        std::fs::write(&program, b"program-v1").unwrap();
        std::fs::write(&refs, b"# references only\n").unwrap();
        let input = RunInput {
            executable: identity(&program, 65536, false).unwrap().0,
            refs_file: Some(identity(&refs, 65536, false).unwrap().0),
            argv: vec![],
            cwd: dir.0.display().to_string(),
            refs: String::new(),
            files: vec![],
        };
        verify(&input).unwrap();
        std::fs::write(&program, b"program-v2").unwrap();
        assert!(verify(&input).is_err());
        std::fs::write(&program, b"program-v1").unwrap();
        std::fs::write(&refs, b"# changed\n").unwrap();
        assert!(verify(&input).is_err());
    }

    #[test]
    fn inherited_pipe_is_sequential_and_exit_code_is_preserved() {
        let (r, w) = pipe().unwrap();
        let mut command = std::process::Command::new("python3");
        command
            .env("P1_PIPE", format!("/dev/fd/{}", r.as_raw_fd()))
            .env("P1_VALUE", "synthetic-only");
        command.args(["-c", "import os,sys; f=open(os.environ['P1_PIPE'],'rb'); assert f.read()==b'pipe-only'; assert f.read()==b''; assert os.environ['P1_VALUE']=='synthetic-only'; sys.exit(19)"]);
        assert_eq!(
            spawn(command, vec![(r, w, Zeroizing::new(b"pipe-only".to_vec()))]).unwrap(),
            19
        );
    }
    // Run signal handling in a separate test process: signal dispositions are process-global.
    #[test]
    #[ignore = "subprocess helper for terminal_group_interrupt_reaches_child_and_descendant"]
    fn signal_child_helper() {
        let root = std::env::var("DMSG_SIGNAL_FIXTURE").expect("isolated helper only");
        let mut child = std::process::Command::new("python3");
        child.args(["-c", r#"
import os,signal,subprocess,sys,time
from pathlib import Path
root=Path(sys.argv[1])
def interrupted(sig,frame):
    root.joinpath('parent-interrupted').write_text('yes')
    sys.exit(130)
signal.signal(signal.SIGINT,interrupted)
p=subprocess.Popen([sys.executable,'-c',"import signal,sys,time; from pathlib import Path; root=Path(sys.argv[1]); signal.signal(signal.SIGINT,lambda s,f:(root.joinpath('descendant-interrupted').write_text('yes'),sys.exit(130))); root.joinpath('ready').write_text('yes'); time.sleep(30)",str(root)])
p.wait()
"#, &root]);
        let code = spawn(child, vec![]).unwrap();
        assert_eq!(code, 130);
    }

    #[test]
    fn terminal_group_interrupt_reaches_child_and_descendant() {
        use std::os::unix::process::CommandExt;
        let dir = dmsg_client::testing::TempDir::new();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process_command::tests::signal_child_helper",
                "--ignored",
                "--nocapture",
            ])
            .env("DMSG_SIGNAL_FIXTURE", &dir.0)
            .process_group(0);
        let mut child = command.spawn().unwrap();
        struct Cleanup(u32);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    libc::kill(-(self.0 as i32), libc::SIGKILL);
                }
            }
        }
        let _cleanup = Cleanup(child.id());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !dir.0.join("ready").exists() {
            assert!(std::time::Instant::now() < until, "reader not ready");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(unsafe { libc::kill(-(child.id() as i32), libc::SIGINT) }, 0);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(std::time::Instant::now() < until, "wrapper did not finish");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for file in ["parent-interrupted", "descendant-interrupted"] {
            assert!(dir.0.join(file).exists(), "{file}");
        }
    }

    #[test]
    fn unread_pipe_does_not_hold_up_child_exit_or_spawn_failure() {
        let (r, w) = pipe().unwrap();
        let mut command = std::process::Command::new("python3");
        command.args(["-c", "raise SystemExit(23)"]);
        let before = std::time::Instant::now();
        assert_eq!(
            spawn(command, vec![(r, w, Zeroizing::new(vec![1; 1024 * 1024]))]).unwrap(),
            23
        );
        assert!(before.elapsed() < std::time::Duration::from_secs(5));
        assert!(
            spawn(
                std::process::Command::new("/nonexistent/dmsg-synthetic"),
                vec![]
            )
            .is_err()
        );
    }
}
