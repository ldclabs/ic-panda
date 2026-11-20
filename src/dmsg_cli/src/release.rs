//! Releases and installs of `dmsg`: the latest stable SemVer release of the
//! repository, a download checked against its published SHA-256 and, on
//! macOS, signed by its team or the pinned recipient of the LDC transition,
//! and the shared location that the install scripts, Homebrew and the desktop app use.
//!
//! GitHub Latest stays on the LDC bridge for old clients. New clients enumerate
//! complete stable releases, excluding canister releases (`canisters-v*`).

use crate::remote::Failure;
use semver::Version;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository's releases; `DMSG_RELEASE_URL` replaces it for tests and mirrors.
const RELEASES: &str = "https://github.com/ldclabs/ic-panda/releases";
/// This binary's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn base() -> String {
    std::env::var("DMSG_RELEASE_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| RELEASES.into())
        .trim_end_matches('/')
        .to_string()
}

/// This platform's release asset.
pub fn asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("dmsg-macos-arm64"),
        ("linux", "x86_64") => Some("dmsg-linux-x86_64"),
        ("linux", "aarch64") => Some("dmsg-linux-arm64"),
        ("windows", "x86_64") => Some("dmsg-windows-x64.exe"),
        _ => None,
    }
}

/// SemVer precedence, including beta sequence numbers. Build metadata does not order versions.
pub fn newer(a: &str, b: &str) -> bool {
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => a.cmp_precedence(&b).is_gt(),
        _ => false,
    }
}

fn parse(version: &str) -> Option<Version> {
    Version::parse(version).ok()
}

fn unavailable(error: impl std::fmt::Display) -> Failure {
    Failure::new("UNAVAILABLE", error.to_string())
}

/// The HTTP client of release downloads.
pub fn http() -> Result<reqwest::Client, Failure> {
    reqwest::Client::builder()
        .user_agent(format!("dmsg/{VERSION}"))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(unavailable)
}

async fn get(http: &reqwest::Client, url: &str, accept: &str) -> Result<Vec<u8>, Failure> {
    let response = http
        .get(url)
        .header(reqwest::header::ACCEPT, accept)
        .send()
        .await
        .map_err(unavailable)?;
    if !response.status().is_success() {
        return Err(Failure::new(
            "NOT_FOUND",
            format!("{url}: HTTP {}", response.status()),
        ));
    }
    Ok(response.bytes().await.map_err(unavailable)?.to_vec())
}

/// The version of the latest dMsg client release.
pub async fn latest(http: &reqwest::Client) -> Result<String, Failure> {
    if base() == RELEASES {
        return latest_from_api(
            http,
            "https://api.github.com/repos/ldclabs/ic-panda/releases",
        )
        .await;
    }
    // Explicit mirrors keep their existing /latest contract. The official
    // GitHub Latest entry stays pinned to the LDC bridge for older clients.
    let url = format!("{}/latest", base());
    let bytes = get(http, &url, "application/json").await?;
    let reply: Value = serde_json::from_slice(&bytes).map_err(unavailable)?;
    let tag = reply["tag_name"].as_str().unwrap_or_default();
    stable_version(&reply).map(str::to_string).ok_or_else(|| {
        Failure::new(
            "NOT_FOUND",
            format!("the latest release ({tag}) is not a dMsg client release"),
        )
    })
}

async fn latest_from_api(http: &reqwest::Client, api: &str) -> Result<String, Failure> {
    let asset =
        asset().ok_or_else(|| Failure::new("UNSUPPORTED", "no release for this platform"))?;
    let mut best: Option<String> = None;
    for page in 1.. {
        let bytes = get(
            http,
            &format!("{api}?per_page=100&page={page}"),
            "application/vnd.github+json",
        )
        .await?;
        let releases: Vec<Value> = serde_json::from_slice(&bytes).map_err(unavailable)?;
        for release in &releases {
            if let Some(version) = downloadable_version(release, asset)
                && best.as_ref().is_none_or(|best| newer(version, best))
            {
                best = Some(version.to_string());
            }
        }
        if releases.len() < 100 {
            break;
        }
    }
    best.ok_or_else(|| {
        Failure::new(
            "NOT_FOUND",
            "no complete stable dMsg release for this platform",
        )
    })
}

fn downloadable_version<'a>(release: &'a Value, asset: &str) -> Option<&'a str> {
    let version = stable_version(release)?;
    if release["draft"] != false || release["prerelease"] != false {
        return None;
    }
    let assets = release["assets"].as_array()?;
    [asset.to_string(), format!("{asset}.sha256")]
        .iter()
        .all(|name| {
            assets.iter().any(|item| {
                item["name"].as_str() == Some(name.as_str()) && item["state"] == "uploaded"
            })
        })
        .then_some(version)
}

fn stable_version(reply: &Value) -> Option<&str> {
    let tag = reply["tag_name"].as_str()?;
    let version = parse(tag)?;
    // The web /releases/latest endpoint already excludes drafts and prereleases
    // and omits these REST API fields. Honor explicit flags from mirrors.
    (version.pre.is_empty()
        && version.build.is_empty()
        && reply.get("draft").is_none_or(|flag| flag == false)
        && reply.get("prerelease").is_none_or(|flag| flag == false))
    .then_some(tag)
}

/// This platform's binary of release `version`, checked against the
/// release's SHA-256.
pub async fn download(http: &reqwest::Client, version: &str) -> Result<Vec<u8>, Failure> {
    let asset = asset()
        .ok_or_else(|| Failure::new("UNSUPPORTED", "dMsg releases no binary for this platform"))?;
    let url = format!("{}/download/{version}/{asset}", base());
    let listed = get(http, &format!("{url}.sha256"), "text/plain").await?;
    let expected = String::from_utf8_lossy(&listed)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = get(http, &url, "application/octet-stream").await?;
    if hex::encode(Sha256::digest(&bytes)) != expected {
        return Err(Failure::new(
            "INTEGRITY_FAILED",
            format!("{asset} does not match its published SHA-256"),
        ));
    }
    Ok(bytes)
}

/// Homebrew owns `path`; only `brew upgrade` replaces it.
pub fn homebrew(path: &Path) -> bool {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .components()
        .any(|part| part.as_os_str() == "Cellar")
}

/// The version an installed `dmsg` reports.
pub fn version_of(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.split_whitespace().last()?;
    parse(version).map(|_| version.to_string())
}

fn executable() -> &'static str {
    if cfg!(windows) { "dmsg.exe" } else { "dmsg" }
}

/// Where the install scripts put `dmsg`: `DMSG_INSTALL_DIR`, else
/// `~/.local/bin`, or `%LOCALAPPDATA%\dMsg\bin` on Windows (outside the
/// desktop app's install directory, whose installer stops every process
/// under it).
pub fn install_dir() -> Result<PathBuf, Failure> {
    if let Some(dir) = std::env::var_os("DMSG_INSTALL_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("dMsg").join("bin"))
    } else {
        std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".local").join("bin"))
    };
    base.ok_or_else(|| Failure::new("UNAVAILABLE", "no home directory"))
}

/// The installed `dmsg` to keep using: the shared location, else a
/// Homebrew one, else the shared location for a new install.
fn locate() -> Result<PathBuf, Failure> {
    let shared = install_dir()?.join(executable());
    if std::env::var_os("DMSG_INSTALL_DIR").is_some() || cfg!(windows) {
        return Ok(shared);
    }
    let brewed = [
        "/opt/homebrew/bin/dmsg",
        "/usr/local/bin/dmsg",
        "/home/linuxbrew/.linuxbrew/bin/dmsg",
    ];
    Ok(std::iter::once(shared.clone())
        .chain(brewed.into_iter().map(PathBuf::from))
        .find(|path| path.is_file())
        .unwrap_or(shared))
}

/// The team that signed a macOS binary; `None` when unsigned or ad hoc.
#[cfg(target_os = "macos")]
fn team(path: &Path) -> Option<String> {
    let output = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=2"])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier="))
        .filter(|team| valid_team(team))
        .map(str::to_string)
}

#[cfg(any(target_os = "macos", test))]
fn valid_team(team: &str) -> bool {
    team.len() == 10
        && team
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[cfg(any(target_os = "macos", test))]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacSigningPolicy {
    daemon_identifier: String,
}

#[cfg(any(target_os = "macos", test))]
fn mac_signing_policy() -> MacSigningPolicy {
    serde_json::from_str(include_str!("../../../scripts/dmsg/macos-signing.json"))
        .expect("reviewed macOS signing policy")
}

#[cfg(any(target_os = "macos", test))]
fn mac_requirement(identifier: &str, signer: &str) -> String {
    assert!(
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    );
    assert!(valid_team(signer));
    format!(
        "identifier \"{identifier}\" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"{signer}\""
    )
}

#[cfg(target_os = "macos")]
fn matches_requirement(path: &Path, requirement: &str) -> bool {
    Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R", &format!("={requirement}")])
        .arg(path)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Signed clients accept a Developer ID binary from their own signing team.
#[cfg(target_os = "macos")]
fn check_signature(candidate: &Path) -> Result<(), Failure> {
    let own = std::env::current_exe().map_err(unavailable)?;
    let Some(expected) = team(&own) else {
        return Ok(());
    };
    let policy = mac_signing_policy();
    if matches_requirement(
        candidate,
        &mac_requirement(&policy.daemon_identifier, &expected),
    ) {
        return Ok(());
    }
    Err(Failure::new(
        "INTEGRITY_FAILED",
        format!(
            "the binary must be Developer ID signed as {} by {}",
            policy.daemon_identifier, expected
        ),
    ))
}

#[cfg(not(target_os = "macos"))]
fn check_signature(_candidate: &Path) -> Result<(), Failure> {
    Ok(())
}

/// Put `bytes` at `target` through a staged file next to it, so a reader
/// sees the old binary or the new one; a running daemon keeps the old one
/// until it restarts.
pub fn place(bytes: &[u8], target: &Path) -> Result<(), Failure> {
    let dir = target
        .parent()
        .ok_or_else(|| Failure::new("INVALID_INPUT", "the target has no directory"))?;
    std::fs::create_dir_all(dir).map_err(unavailable)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable().into());
    let staged = dir.join(format!(
        ".{name}.{}",
        hex::encode(&dmsg_client::crypto::random_id()[..6])
    ));
    let result = std::fs::write(&staged, bytes)
        .map_err(unavailable)
        .and_then(|_| commit(&staged, target));
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

#[cfg(unix)]
fn commit(staged: &Path, target: &Path) -> Result<(), Failure> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(staged, std::fs::Permissions::from_mode(0o755))
        .map_err(unavailable)?;
    check_signature(staged)?;
    std::fs::rename(staged, target)
        .map_err(|e| unavailable(format!("replace {}: {e}", target.display())))
}

/// A running executable cannot be replaced but can be renamed: the old one
/// moves aside, and the next placement deletes it once nothing runs it.
#[cfg(windows)]
fn commit(staged: &Path, target: &Path) -> Result<(), Failure> {
    check_signature(staged)?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let prefix = format!(".{name}.old-");
    if let Some(dir) = target.parent() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let aside = target.with_file_name(format!(
        "{prefix}{}",
        hex::encode(&dmsg_client::crypto::random_id()[..6])
    ));
    let moved = target.exists();
    if moved {
        std::fs::rename(target, &aside).map_err(unavailable)?;
    }
    std::fs::rename(staged, target).map_err(|e| {
        if moved {
            let _ = std::fs::rename(&aside, target);
        }
        unavailable(format!("replace {}: {e}", target.display()))
    })
}

fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|p| p == dir))
}

/// `dmsg install`: put this binary at the shared location unless the one
/// there is the same release or newer, or Homebrew owns it. The desktop app
/// runs its bundled copy with this on every start and then uses the
/// reported path, so an installed binary stays only when it is signed as
/// [`check_signature`] requires.
pub fn install(dir: Option<PathBuf>) -> Result<Value, Failure> {
    let source = std::env::current_exe().map_err(unavailable)?;
    let (target, shared) = match dir {
        Some(dir) => (dir.join(executable()), dir.join(executable())),
        None => (locate()?, install_dir()?.join(executable())),
    };
    install_at(&source, target, shared, |path| {
        check_signature(path).is_ok()
    })
}

/// Install `source` at `target`. Nothing runs an installed binary that
/// fails `verified`: a Homebrew one gives way to the shared location, and
/// any other one is replaced, even one that resolves under a `Cellar`.
fn install_at(
    source: &Path,
    target: PathBuf,
    shared: PathBuf,
    verified: impl Fn(&Path) -> bool,
) -> Result<Value, Failure> {
    let (target, brewed) = match homebrew(&target) {
        true if verified(&target) => (target, true),
        true => (shared, false),
        false => (target, false),
    };
    let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    let copy = || std::fs::read(source).map_err(unavailable);
    let (action, version) = if !target.exists() {
        place(&copy()?, &target)?;
        ("installed", Some(VERSION.to_string()))
    } else if brewed {
        ("homebrew", version_of(&target))
    } else if same(source, &target) {
        ("current", Some(VERSION.to_string()))
    } else if !verified(&target) {
        place(&copy()?, &target)?;
        ("replaced", Some(VERSION.to_string()))
    } else {
        match version_of(&target) {
            Some(version) if version == VERSION => ("current", Some(version)),
            Some(version) if newer(&version, VERSION) => ("kept_newer", Some(version)),
            _ => {
                place(&copy()?, &target)?;
                ("upgraded", Some(VERSION.to_string()))
            }
        }
    };
    Ok(json!({
        "action": action,
        "path": target,
        "version": version,
        "on_path": target.parent().is_some_and(on_path),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn developer_id_updates_require_the_same_team_and_daemon_identifier() {
        let policy = mac_signing_policy();
        for invalid in ["", "not set", "short", "S9Z2WAMAV8\" or true", "s9z2wamav8"] {
            assert!(!valid_team(invalid));
        }
        let requirement = mac_requirement(&policy.daemon_identifier, "63G8VNQPXB");
        assert!(requirement.contains("certificate leaf[subject.OU] = \"63G8VNQPXB\""));
        assert!(!requirement.contains(" or "));
        assert!(requirement.contains("identifier \"net.dmsg.dmsg\""));
        assert!(requirement.contains("anchor apple generic"));
        assert!(requirement.contains("certificate leaf[field.1.2.840.113635.100.6.1.13] exists"));
    }

    fn complete_release(version: &str) -> Value {
        let asset = asset().unwrap();
        json!({"tag_name":version,"draft":false,"prerelease":false,"assets":[
            {"name":asset,"state":"uploaded"}, {"name":format!("{asset}.sha256"),"state":"uploaded"}
        ]})
    }

    #[test]
    fn enumeration_requires_complete_stable_assets_for_this_platform() {
        let asset = asset().unwrap();
        assert_eq!(
            downloadable_version(&complete_release("1.6.0"), asset),
            Some("1.6.0")
        );
        for version in ["1.6.0-beta.1", "canisters-v3.0.0", "v1.6.0"] {
            assert!(downloadable_version(&complete_release(version), asset).is_none());
        }
        for (field, value) in [
            ("draft", json!(true)),
            ("prerelease", json!(true)),
            ("assets", json!([])),
        ] {
            let mut release = complete_release("1.6.0");
            release[field] = value;
            assert!(downloadable_version(&release, asset).is_none());
        }
        let mut uploading = complete_release("1.6.0");
        uploading["assets"][1]["state"] = json!("new");
        assert!(downloadable_version(&uploading, asset).is_none());
    }

    #[tokio::test]
    async fn discovery_passes_the_pinned_bridge_and_paginates_without_selecting_beta() {
        use axum::{Json, Router, http::Uri, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/releases",
            get(|uri: Uri| async move {
                let values = match uri.query().unwrap() {
                    "per_page=100&page=1" => vec![complete_release("1.5.8"); 100],
                    "per_page=100&page=2" => vec![
                        complete_release("1.6.1-beta.1"),
                        complete_release("1.6.0"),
                        complete_release("1.5.9"),
                    ],
                    query => panic!("unexpected release page: {query}"),
                };
                Json(values)
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let result = latest_from_api(&http().unwrap(), &format!("http://{address}/releases")).await;
        server.abort();
        assert_eq!(result.unwrap(), "1.6.0");
    }

    #[test]
    fn the_default_update_requires_a_stable_client_release() {
        let release = |tag, draft, prerelease| json!({"tag_name": tag, "draft": draft, "prerelease": prerelease});
        // GitHub's web /releases/latest endpoint omits the REST API's flags.
        assert_eq!(stable_version(&json!({"tag_name": "1.3.0"})), Some("1.3.0"));
        assert_eq!(
            stable_version(&release("1.3.0", false, false)),
            Some("1.3.0")
        );
        for tag in [
            "dmsg-v1.3.0",
            "v3.0.0",
            "canisters-v3.1.6",
            "1.3.0-beta.1",
            "1.3.0+build.1",
        ] {
            assert_eq!(stable_version(&release(tag, false, false)), None);
            assert_eq!(stable_version(&json!({"tag_name": tag})), None);
        }
        assert_eq!(stable_version(&release("1.3.0", true, false)), None);
        assert_eq!(stable_version(&release("1.3.0", false, true)), None);
    }

    #[test]
    fn versions_order_numerically_and_releases_follow_prereleases() {
        assert!(newer("1.3.0-beta.10", "1.3.0-beta.2"));
        assert!(newer("1.3.0-beta.2", "1.3.0-beta.1"));
        assert!(!newer("1.3.0-beta.1", "1.3.0-beta.2"));
        assert!(!newer("1.3.0+build.2", "1.3.0+build.1"));
        assert!(!newer("1.3.0-beta.01", "1.2.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "1.0.0-beta.2"));
        assert!(!newer("1.0.0-beta.2", "1.0.0"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
        assert!(!newer("next", "0.1.0"));
    }

    #[test]
    fn placing_replaces_through_a_staged_file() {
        let dir = std::env::temp_dir().join(format!(
            "dmsg-place-{}",
            hex::encode(dmsg_client::crypto::random_id())
        ));
        let target = dir.join("bin").join(executable());
        place(b"one", &target).unwrap();
        place(b"two", &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A `dmsg` that reports `version` and leaves `ran` behind once run.
    #[cfg(unix)]
    fn fake(path: &Path, version: &str, ran: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let script = format!(
            "#!/bin/sh\ntouch '{}'\necho 'dmsg {version}'\n",
            ran.display()
        );
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn install_keeps_only_a_verified_binary() {
        let dir = std::env::temp_dir().join(format!(
            "dmsg-install-{}",
            hex::encode(dmsg_client::crypto::random_id())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("bundled");
        std::fs::write(&source, b"bundled").unwrap();
        let shared = dir.join("bin").join(executable());
        let ran = dir.join("ran");

        // A newer release that verifies stays.
        fake(&shared, "9.9.9", &ran);
        let kept = install_at(&source, shared.clone(), shared.clone(), |_| true).unwrap();
        assert_eq!(
            (&kept["action"], &kept["version"]),
            (&json!("kept_newer"), &json!("9.9.9"))
        );
        // One that fails is replaced before anything runs it.
        std::fs::remove_file(&ran).unwrap();
        let replaced = install_at(&source, shared.clone(), shared.clone(), |_| false).unwrap();
        assert_eq!(replaced["action"], "replaced");
        assert_eq!(std::fs::read(&shared).unwrap(), b"bundled");
        assert!(!ran.exists());

        // A Homebrew binary that fails gives way to the shared location.
        std::fs::remove_file(&shared).unwrap();
        let brewed = dir.join("Cellar/dmsg/9.9.9/bin").join(executable());
        fake(&brewed, "9.9.9", &ran);
        let installed =
            install_at(&source, brewed.clone(), shared.clone(), |p| !homebrew(p)).unwrap();
        assert_eq!(
            (&installed["action"], &installed["path"]),
            (&json!("installed"), &json!(shared))
        );
        assert!(!ran.exists());
        let kept = install_at(&source, brewed.clone(), shared.clone(), |_| true).unwrap();
        assert_eq!(kept["action"], "homebrew");

        // A shared binary that fails is replaced even when it resolves under
        // a `Cellar`.
        std::fs::remove_file(&ran).unwrap();
        std::fs::remove_file(&shared).unwrap();
        std::os::unix::fs::symlink(&brewed, &shared).unwrap();
        let replaced =
            install_at(&source, shared.clone(), shared.clone(), |p| !homebrew(p)).unwrap();
        assert_eq!(replaced["action"], "replaced");
        assert_eq!(std::fs::read(&shared).unwrap(), b"bundled");
        assert!(!ran.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
