use super::filesystem::{LibraryFs, noise, split};
use super::*;
use dav_server::{
    davpath::DavPath,
    fs::{DavFileSystem, FsError, OpenOptions},
};
use dmsg_client::library::catalog::Origin;
use dmsg_client::testing::{TempDir, unlocked};
use dmsg_client::{
    Client,
    files::CHUNK,
    library::{catalog::ROOT, mount::Node},
};
use futures_util::StreamExt;
use tokio::sync::Mutex;

const PREFIX: &str = "/dav/t/dMsg";

struct Share {
    handler: DavHandler,
    client: Arc<Mutex<Client>>,
    _dir: TempDir,
}

fn share() -> Share {
    let dir = TempDir::new();
    let client = Arc::new(Mutex::new(unlocked(&dir.0, false, None)));
    let handler = DavHandler::builder()
        .strip_prefix(PREFIX)
        .filesystem(Box::new(LibraryFs::new(
            client.clone(),
            dmsg_client::testing::account().id.to_string(),
        )))
        .locksystem(FakeLs::new())
        .build_handler();
    Share {
        handler,
        client,
        _dir: dir,
    }
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i as u64 * 31 + seed as u64) % 251) as u8)
        .collect()
}

async fn request(
    share: &Share,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = axum::http::Request::builder()
        .method(method)
        .uri(format!("{PREFIX}{path}"));
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = share
        .handler
        .handle_with(
            DavConfig::new().filesystem(Box::new(
                LibraryFs::new(
                    share.client.clone(),
                    dmsg_client::testing::account().id.to_string(),
                )
                .for_method(method),
            )),
            builder.body(Body::from(body)).unwrap(),
        )
        .await;
    let (parts, mut body) = response.into_parts();
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    (parts.status, parts.headers, bytes)
}

async fn lookup(share: &Share, path: &str) -> Option<Node> {
    share.client.lock().await.files_lookup(path).unwrap()
}

async fn file(share: &Share, path: &str) -> dmsg_client::library::catalog::FileView {
    match lookup(share, path).await {
        Some(Node::File(file)) => file,
        other => panic!("{path}: {other:?}"),
    }
}

async fn bytes_of(share: &Share, id: &str) -> Vec<u8> {
    share
        .client
        .lock()
        .await
        .files_read_range(id, None, 0, u64::MAX)
        .unwrap()
        .1
        .to_vec()
}

#[tokio::test]
async fn listings_reads_and_ranges_follow_the_library() {
    let share = share();
    let data = pattern(CHUNK + 777, 3);
    let (docs, report) = {
        let mut client = share.client.lock().await;
        let docs = client.files_mkdir(ROOT, "docs").unwrap();
        let report = client
            .files_write(&docs.id, "report.bin", &data)
            .unwrap()
            .file;
        (docs, report)
    };

    let (status, _, body) = request(&share, "PROPFIND", "/", &[("Depth", "1")], vec![]).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    let body = String::from_utf8(body).unwrap();
    assert!(
        body.contains("/docs/") && !body.contains("report.bin"),
        "{body}"
    );

    let (status, _, body) = request(&share, "PROPFIND", "/docs/", &[("Depth", "1")], vec![]).await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    let body = String::from_utf8(body).unwrap();
    assert!(body.contains("report.bin"), "{body}");
    assert!(
        body.contains(&format!(
            "<D:getcontentlength>{}</D:getcontentlength>",
            data.len()
        )),
        "{body}"
    );
    assert!(body.contains(&report.sha256[..32]), "{body}");

    let (status, headers, body) = request(&share, "GET", "/docs/report.bin", &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, data);
    assert_eq!(
        headers.get("etag").unwrap().to_str().unwrap(),
        format!("\"{}\"", &report.sha256[..32])
    );

    // A range across the chunk boundary.
    let (status, headers, body) = request(
        &share,
        "GET",
        "/docs/report.bin",
        &[("Range", "bytes=1048570-1048585")],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, &data[1048570..=1048585]);
    assert_eq!(
        headers.get("content-range").unwrap().to_str().unwrap(),
        format!("bytes 1048570-1048585/{}", data.len())
    );
    let (status, _, body) = request(
        &share,
        "GET",
        "/docs/report.bin",
        &[("Range", "bytes=-100")],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, &data[data.len() - 100..]);

    let (status, headers, body) = request(&share, "HEAD", "/docs/report.bin", &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());
    assert_eq!(
        headers.get("content-length").unwrap().to_str().unwrap(),
        data.len().to_string()
    );

    for path in [
        "/docs/missing.bin",
        "/nowhere/",
        "/docs/._report.bin",
        "/.DS_Store",
    ] {
        let (status, _, _) = request(&share, "PROPFIND", path, &[("Depth", "0")], vec![]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, _, _) = request(&share, "GET", "/docs/", &[], vec![]).await;
    assert_ne!(status, StatusCode::OK);
    let _ = docs;
}

#[tokio::test]
async fn writes_become_versions_and_noise_is_dropped() {
    let share = share();
    let docs = share.client.lock().await.files_mkdir(ROOT, "docs").unwrap();

    let (status, _, _) = request(&share, "PUT", "/docs/new.txt", &[], b"hello".to_vec()).await;
    assert_eq!(status, StatusCode::CREATED);
    let new = file(&share, "docs/new.txt").await;
    assert_eq!(
        (new.size, new.folder.as_str(), new.mime.as_str()),
        (5, docs.id.as_str(), "text/plain")
    );
    let details = share.client.lock().await.files_show(&new.id).unwrap();
    assert_eq!(details.versions.len(), 1);
    assert_eq!(details.versions[0].origin, Origin::Mounted);

    // The same name again: a version of the same file.
    let big = pattern(CHUNK * 2 + 5, 9);
    let (status, _, _) = request(&share, "PUT", "/docs/new.txt", &[], big.clone()).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let again = file(&share, "docs/new.txt").await;
    assert_eq!(
        (again.id.as_str(), again.size, again.versions),
        (new.id.as_str(), big.len() as u64, 2)
    );
    assert_eq!(bytes_of(&share, &again.id).await, big);
    let (status, _, body) = request(&share, "GET", "/docs/new.txt", &[], vec![]).await;
    assert_eq!((status, body), (StatusCode::OK, big));

    // Nothing waits in the upload directory once imported.
    let dir = share.client.lock().await.files_mount_dir().unwrap();
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

    // An empty file is a file.
    let (status, _, _) = request(&share, "PUT", "/docs/empty", &[], vec![]).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(file(&share, "docs/empty").await.size, 0);

    // The file managers' noise is taken and dropped.
    for path in [
        "/docs/.DS_Store",
        "/docs/._new.txt",
        "/Thumbs.db",
        "/desktop.ini",
    ] {
        let (status, _, _) = request(&share, "PUT", path, &[], b"noise".to_vec()).await;
        assert!(status.is_success(), "{path}: {status}");
        assert!(lookup(&share, path).await.is_none(), "{path}");
    }
    let (status, _, _) = request(&share, "MKCOL", "/docs/.Trashes/", &[], vec![]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // No parent, no file.
    let (status, _, _) = request(&share, "PUT", "/missing/x.txt", &[], b"x".to_vec()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    // A name the library refuses.
    let (status, _, _) = request(&share, "PUT", "/docs/bad%01name", &[], b"x".to_vec()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(share.client.lock().await.files_usage().unwrap().files, 2);
}

#[tokio::test]
async fn folders_moves_copies_and_deletes_map_to_the_library() {
    let share = share();
    let data = pattern(CHUNK + 1, 5);
    let report = {
        let mut client = share.client.lock().await;
        let docs = client.files_mkdir(ROOT, "docs").unwrap();
        client
            .files_write(&docs.id, "report.bin", &data)
            .unwrap()
            .file
    };

    let (status, _, _) = request(&share, "MKCOL", "/docs/new/", &[], vec![]).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(matches!(
        lookup(&share, "docs/new").await,
        Some(Node::Folder(_))
    ));
    let (status, _, _) = request(&share, "MKCOL", "/docs/new/", &[], vec![]).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    let (status, _, _) = request(&share, "MKCOL", "/missing/new/", &[], vec![]).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A move keeps the file, with its ID.
    let destination = format!("http://127.0.0.1:1{PREFIX}/docs/new/renamed.bin");
    let (status, _, _) = request(
        &share,
        "MOVE",
        "/docs/report.bin",
        &[("Destination", &destination)],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(lookup(&share, "docs/report.bin").await.is_none());
    let moved = file(&share, "docs/new/renamed.bin").await;
    assert_eq!(moved.id, report.id);
    let new_folder = match lookup(&share, "docs/new").await {
        Some(Node::Folder(folder)) => folder,
        other => panic!("{other:?}"),
    };
    assert_eq!(moved.folder, new_folder.id);
    // A folder moves too.
    let (status, _, _) = request(
        &share,
        "MOVE",
        "/docs/new/",
        &[(
            "Destination",
            &format!("http://127.0.0.1:1{PREFIX}/archive/"),
        )],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(file(&share, "archive/renamed.bin").await.id, report.id);
    assert!(lookup(&share, "docs/new").await.is_none());
    // Onto an existing name without overwrite: refused.
    let (status, _, _) = request(
        &share,
        "MOVE",
        "/archive/",
        &[
            ("Destination", &format!("http://127.0.0.1:1{PREFIX}/docs/")),
            ("Overwrite", "F"),
        ],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED);

    // A copy is a new file with the same bytes under its own key.
    let (status, _, _) = request(
        &share,
        "COPY",
        "/archive/renamed.bin",
        &[(
            "Destination",
            &format!("http://127.0.0.1:1{PREFIX}/copy.bin"),
        )],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let copy = file(&share, "copy.bin").await;
    assert_ne!(copy.id, report.id);
    assert_eq!(copy.sha256, report.sha256);
    assert_eq!(bytes_of(&share, &copy.id).await, data);
    assert_eq!(share.client.lock().await.files_usage().unwrap().blobs, 4);

    // A deleted file is in the trash; a deleted folder is one trash entry.
    let (status, _, _) = request(&share, "DELETE", "/copy.bin", &[], vec![]).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(lookup(&share, "copy.bin").await.is_none());
    let archive = {
        let mut client = share.client.lock().await;
        let trash = client.files_list(ROOT, true).unwrap();
        assert_eq!(
            trash
                .files
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            [copy.id.as_str()]
        );
        match client.files_lookup("archive").unwrap() {
            Some(Node::Folder(folder)) => folder,
            other => panic!("{other:?}"),
        }
    };
    let (status, _, _) = request(&share, "DELETE", "/archive/", &[], vec![]).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = request(&share, "DELETE", "/", &[], vec![]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = request(&share, "DELETE", "/docs/nothing", &[], vec![]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let mut client = share.client.lock().await;
    let trash = client.files_list(ROOT, true).unwrap();
    assert_eq!(
        trash
            .folders
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        [archive.id.as_str()]
    );
    // The file inside went with its folder, not on its own.
    assert_eq!(trash.files.len(), 1);
    assert!(
        client
            .files_show(&report.id)
            .unwrap()
            .file
            .trashed_at
            .is_none()
    );
    client
        .files_restore(std::slice::from_ref(&archive.id))
        .unwrap();
    assert_eq!(
        client
            .files_lookup("archive/renamed.bin")
            .unwrap()
            .map(|n| matches!(n, Node::File(_))),
        Some(true)
    );
}

#[tokio::test]
async fn a_safe_save_becomes_a_version_of_the_original() {
    let share = share();
    let notes = share
        .client
        .lock()
        .await
        .files_write(ROOT, "notes.txt", b"version one")
        .unwrap()
        .file;
    let (status, _, _) = request(
        &share,
        "PUT",
        "/notes.txt.sb-1a2b",
        &[],
        b"version two".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let temp = file(&share, "notes.txt.sb-1a2b").await;

    let destination = format!("http://127.0.0.1:1{PREFIX}/notes.txt");
    for extra in [
        ("Overwrite", "F"),
        ("If-Match", "\"stale\""),
        ("If", "([\"stale\"])"),
    ] {
        let (status, _, _) = request(
            &share,
            "MOVE",
            "/notes.txt.sb-1a2b",
            &[("Destination", &destination), extra],
            vec![],
        )
        .await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED);
        assert!(lookup(&share, "notes.txt.sb-1a2b").await.is_some());
    }
    let (status, _, _) = request(
        &share,
        "MOVE",
        "/notes.txt.sb-1a2b",
        &[("Destination", &destination)],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let mut client = share.client.lock().await;
    let details = client.files_show(&notes.id).unwrap();
    assert_eq!(details.versions.len(), 2);
    assert_eq!(details.versions[1].origin, Origin::Mounted);
    assert_eq!(details.file.size, "version two".len() as u64);
    assert_eq!(
        &client
            .files_read_range(&notes.id, None, 0, u64::MAX)
            .unwrap()
            .1[..],
        b"version two"
    );
    assert!(client.files_lookup("notes.txt.sb-1a2b").unwrap().is_none());
    assert!(matches!(client.files_show(&temp.id), Err(Error::NotFound)));
    assert!(client.files_list(ROOT, true).unwrap().files.is_empty());
    // The temporary file's chunks are gone with it; two versions remain.
    assert_eq!(client.files_usage().unwrap().blobs, 2);
}

#[test]
fn credentials_and_noise() {
    let mut headers = HeaderMap::new();
    assert!(!authorized(&headers, "pw"));
    let basic = |user: &str, pass: &str| {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
        )
    };
    headers.insert(header::AUTHORIZATION, basic("dmsg", "pw").parse().unwrap());
    assert!(authorized(&headers, "pw"));
    assert!(!authorized(&headers, "pw2"));
    headers.insert(header::AUTHORIZATION, basic("other", "pw").parse().unwrap());
    assert!(!authorized(&headers, "pw"));

    assert!(
        noise(".DS_Store") && noise("._report.pdf") && noise("Thumbs.db") && noise("desktop.ini")
    );
    assert!(!noise("report.pdf") && !noise(".gitignore") && !noise("_underscore"));
    assert_eq!(split("/docs/a/b.txt"), ("docs/a".into(), "b.txt".into()));
    assert_eq!(split("/b.txt"), (String::new(), "b.txt".into()));
    assert_eq!(split("/"), (String::new(), String::new()));
}

#[tokio::test]
async fn put_returns_the_committed_etag_and_rejects_incomplete_versions() {
    let share = share();
    let (status, headers, _) = request(&share, "PUT", "/note.txt", &[], b"original".to_vec()).await;
    assert_eq!(status, StatusCode::CREATED);
    let etag = headers[header::ETAG].to_str().unwrap();
    let (_, head, _) = request(&share, "HEAD", "/note.txt", &[], vec![]).await;
    assert_eq!(head[header::ETAG], etag);
    let (status, _, _) = request(
        &share,
        "PUT",
        "/note.txt",
        &[("If-Match", etag)],
        b"complete".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let original = file(&share, "note.txt").await;
    for headers in [
        vec![("X-Expected-Entity-Length", "10")],
        vec![("Content-Length", "10")],
        vec![("Content-Length", "18446744073709551615")],
    ] {
        let (status, _, _) = request(&share, "PUT", "/note.txt", &headers, b"short".to_vec()).await;
        assert!(!status.is_success());
        assert_eq!(file(&share, "note.txt").await.current, original.current);
        assert_eq!(bytes_of(&share, &original.id).await, b"complete");
    }
}

#[tokio::test]
async fn moves_use_the_final_name_and_deletes_check_preconditions() {
    let share = share();
    let (source, folder) = {
        let mut client = share.client.lock().await;
        let source = client.files_write(ROOT, "old.txt", b"source").unwrap().file;
        let folder = client.files_mkdir(ROOT, "docs").unwrap();
        client
            .files_write(&folder.id, "old.txt", b"unrelated")
            .unwrap();
        (source, folder)
    };
    let (status, _, _) = request(
        &share,
        "MOVE",
        "/old.txt",
        &[("Destination", &format!("{PREFIX}/docs/new.txt"))],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(file(&share, "docs/new.txt").await.id, source.id);
    let old = file(&share, "docs/old.txt").await;
    assert_eq!(bytes_of(&share, &old.id).await, b"unrelated");
    let (status, _, _) = request(
        &share,
        "DELETE",
        "/docs/",
        &[("If-Match", "\"stale\"")],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED);
    assert!(lookup(&share, "docs/new.txt").await.is_some());
    assert_eq!(
        share
            .client
            .lock()
            .await
            .files_list(&folder.id, false)
            .unwrap()
            .files
            .len(),
        2
    );
}

#[tokio::test]
async fn an_open_reader_stops_at_lock_even_with_a_cached_chunk() {
    let share = share();
    share
        .client
        .lock()
        .await
        .files_write(ROOT, "note.txt", b"private")
        .unwrap();
    let fs = LibraryFs::new(
        share.client.clone(),
        dmsg_client::testing::account().id.to_string(),
    );
    let path = DavPath::new("/note.txt").unwrap();
    let mut reader = fs
        .open(
            &path,
            OpenOptions {
                read: true,
                ..OpenOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(reader.read_bytes(1).await.unwrap().as_ref(), b"p");
    share.client.lock().await.lock();
    assert_eq!(reader.read_bytes(1).await.unwrap_err(), FsError::Forbidden);
}

async fn daemon_share(
    name: &str,
) -> (
    Arc<Daemon>,
    TempDir,
    tokio::task::JoinHandle<std::io::Result<()>>,
    String,
) {
    let dir = TempDir::new();
    let home = crate::home::Home {
        dir: dir.0.join("home"),
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let daemon = Arc::new(Daemon::new(
        "owner-token".into(),
        port,
        home.settings().unwrap(),
        unlocked(&dir.0.join("workspace"), false, None),
        dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
        crate::mcp_events::McpEvents::new(&[]).unwrap(),
    ));
    daemon.files_mount.store(true, Ordering::SeqCst);
    let account = dmsg_client::testing::account().id.to_string();
    let (prefix, url) = share_location(port, "t", name);
    *daemon.mount.lock().unwrap() = Some(Mount {
        token: "t".into(),
        name: name.into(),
        password: Some("test-password".into()),
        account: account.clone(),
        url: url.clone(),
        point: None,
        since: now_ms(),
        ready: true,
        handler: DavHandler::builder()
            .strip_prefix(&prefix)
            .filesystem(Box::new(LibraryFs::new(daemon.client_handle(), account)))
            .locksystem(FakeLs::new())
            .build_handler(),
    });
    let server =
        tokio::spawn(axum::serve(listener, crate::daemon::app(daemon.clone())).into_future());
    (daemon, dir, server, url)
}

#[tokio::test]
async fn the_daemon_routes_both_forms_of_the_mount_root() {
    let (daemon, _dir, server, url) = daemon_share("dMsg").await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let rpc = async |method: &str| -> Value {
        http.post(format!("http://127.0.0.1:{}/app/v1", daemon.port()))
            .bearer_auth("owner-token")
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": null}))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["result"]
            .clone()
    };
    let deadline = rpc("daemon/status").await["lock_at"].clone();
    for suffix in ["", "/"] {
        let response = http
            .request(
                reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
                format!("{url}{suffix}"),
            )
            .basic_auth(USER, Some("test-password"))
            .header("Depth", "0")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::MULTI_STATUS,
            "suffix={suffix:?}"
        );
        assert!(response.text().await.unwrap().contains("/dav/t/dMsg/"));
    }
    // Metadata polling is not activity. A real disk write is, and the
    // desktop's event cursor advances so its file view can refresh.
    assert_eq!(rpc("daemon/status").await["lock_at"], deadline);
    let before = rpc("events/poll").await["seq"].as_u64().unwrap();
    tokio::time::sleep(Duration::from_millis(2)).await;
    let response = http
        .put(format!("{url}/note.txt"))
        .basic_auth(USER, Some("test-password"))
        .body("from disk")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(rpc("daemon/status").await["lock_at"].as_u64().unwrap() > deadline.as_u64().unwrap());
    assert!(rpc("events/poll").await["seq"].as_u64().unwrap() > before);
    assert_eq!(
        http.get(format!("{url}/")).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    {
        let mut state = daemon.mount.lock().unwrap();
        let mount = state.as_mut().unwrap();
        mount.ready = false;
        assert_eq!(mount.status(true)["mounted"], false);
        assert_eq!(mount.status(true)["mounting"], true);
    }
    server.abort();
}

#[tokio::test]
async fn localized_volume_names_route_without_becoming_file_paths() {
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    for name in [
        "dMsg Files",
        "dMsg 文件",
        "Файлы dMsg",
        "ملفات dMsg",
        "Fichiers dMsg",
        "Archivos dMsg",
    ] {
        let (daemon, _dir, server, url) = daemon_share(name).await;
        assert_eq!(daemon.mount_status()["name"], name);
        for suffix in ["", "/"] {
            let response = http
                .request(
                    reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
                    format!("{url}{suffix}"),
                )
                .basic_auth(USER, Some("test-password"))
                .header("Depth", "0")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::MULTI_STATUS, "{name}");
        }
        let response = http
            .put(format!("{url}/note.txt"))
            .basic_auth(USER, Some("test-password"))
            .body("content")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "{name}");
        let file = daemon
            .client()
            .lock()
            .await
            .files_lookup("note.txt")
            .unwrap();
        assert!(matches!(file, Some(Node::File(_))));
        let wrong = format!("http://127.0.0.1:{}/dav/t/wrong-volume/", daemon.port());
        assert_eq!(
            http.get(wrong)
                .basic_auth(USER, Some("test-password"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        server.abort();
    }
}

/// Run explicitly after `cargo build -p dmsg_cli`; requires macOS permission
/// to mount a system volume. Only a temporary, offline test account is used.
#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "mounts a temporary macOS WebDAV volume; requires a built dmsg binary"]
async fn native_mount_reads_writes_safe_saves_and_unmounts() {
    let (daemon, dir, server, url) = daemon_share("dMsg 文件").await;
    daemon
        .client()
        .lock()
        .await
        .files_write(ROOT, "note.txt", b"original")
        .unwrap();
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("dmsg");
    let request = json!({"url": url, "password": "test-password"}).to_string();
    let output = run(
        tokio::process::Command::new(binary).args(["--json", "mount-netfs"]),
        Some(&request),
    )
    .await
    .unwrap();
    let point = serde_json::from_str::<Value>(&output).unwrap()["point"]
        .as_str()
        .unwrap()
        .to_string();
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::process::Command::new("/usr/sbin/diskutil")
                .args(["unmount", "force", &self.0])
                .output();
        }
    }
    let cleanup = Cleanup(point.clone());
    assert!(
        std::path::Path::new(&point)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("dMsg 文件"),
        "{point}"
    );
    daemon.mount.lock().unwrap().as_mut().unwrap().point = Some(point.clone());
    // An OS failure must not be reported as success or lose the mount.
    daemon.mount.lock().unwrap().as_mut().unwrap().point = Some(
        dir.path("not-a-mounted-volume")
            .to_string_lossy()
            .into_owned(),
    );
    let failed = daemon.unmount_library().await;
    daemon.mount.lock().unwrap().as_mut().unwrap().point = Some(point.clone());
    assert!(failed.is_err());
    assert_eq!(daemon.mount_status()["mounted"], true);
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let root = std::path::Path::new(&point);
        assert_eq!(std::fs::read(root.join("note.txt")).unwrap(), b"original");
        let mut temp = std::fs::File::create(root.join("note.tmp")).unwrap();
        temp.write_all(b"saved through WebDAV").unwrap();
        // macOS WebDAV does not support F_FULLFSYNC (`File::sync_all`).
        // Closing the native file completes its WebDAV PUT.
        temp.flush().unwrap();
        drop(temp);
        std::fs::rename(root.join("note.tmp"), root.join("note.txt")).unwrap();
        assert_eq!(
            std::fs::read(root.join("note.txt")).unwrap(),
            b"saved through WebDAV"
        );
        std::fs::create_dir(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/测试 file.txt"), b"nested").unwrap();
        assert_eq!(
            std::fs::read(root.join("docs/测试 file.txt")).unwrap(),
            b"nested"
        );
    })
    .await
    .unwrap();
    {
        let mut client = daemon.client().lock().await;
        let Some(Node::File(file)) = client.files_lookup("note.txt").unwrap() else {
            panic!("saved file")
        };
        assert_eq!(
            client
                .files_read_range(&file.id, None, 0, u64::MAX)
                .unwrap()
                .1
                .as_slice(),
            b"saved through WebDAV"
        );
        assert!(file.versions >= 2);
        client.lock();
    }
    daemon.check_mount().await;
    assert_eq!(daemon.mount_status()["mounted"], false);
    drop(cleanup);
    server.abort();
}
