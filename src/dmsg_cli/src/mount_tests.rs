use super::*;
use dmsg_client::library::catalog::Origin;
use dmsg_client::testing::{unlocked, TempDir};
use futures_util::StreamExt;

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
        .filesystem(Box::new(LibraryFs::new(client.clone())))
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
        .handle(builder.body(Body::from(body)).unwrap())
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
        .files_read(id, None, u64::MAX)
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
    let archive = match client.files_lookup("archive").unwrap() {
        Some(Node::Folder(folder)) => folder,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        intercept(&mut client, "DELETE", "archive", &HeaderMap::new(), PREFIX),
        Some(StatusCode::NO_CONTENT)
    );
    assert_eq!(
        intercept(&mut client, "DELETE", "", &HeaderMap::new(), PREFIX),
        Some(StatusCode::FORBIDDEN)
    );
    assert_eq!(
        intercept(
            &mut client,
            "DELETE",
            "docs/nothing",
            &HeaderMap::new(),
            PREFIX
        ),
        None
    );
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
    assert!(client
        .files_show(&report.id)
        .unwrap()
        .file
        .trashed_at
        .is_none());
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

    let mut headers = HeaderMap::new();
    headers.insert(
        "destination",
        format!("http://127.0.0.1:1{PREFIX}/notes.txt")
            .parse()
            .unwrap(),
    );
    let mut client = share.client.lock().await;
    // Without overwrite, or onto nothing, the handler takes it.
    let mut no_overwrite = headers.clone();
    no_overwrite.insert("overwrite", "F".parse().unwrap());
    assert_eq!(
        intercept(
            &mut client,
            "MOVE",
            "notes.txt.sb-1a2b",
            &no_overwrite,
            PREFIX
        ),
        None
    );
    let mut elsewhere = HeaderMap::new();
    elsewhere.insert(
        "destination",
        format!("{PREFIX}/other.txt").parse().unwrap(),
    );
    assert_eq!(
        intercept(&mut client, "MOVE", "notes.txt.sb-1a2b", &elsewhere, PREFIX),
        None
    );
    // Onto the original: a new version of it, and the temporary file is gone.
    assert_eq!(
        intercept(&mut client, "MOVE", "notes.txt.sb-1a2b", &headers, PREFIX),
        Some(StatusCode::NO_CONTENT)
    );
    let details = client.files_show(&notes.id).unwrap();
    assert_eq!(details.versions.len(), 2);
    assert_eq!(details.versions[1].origin, Origin::Mounted);
    assert_eq!(details.file.size, "version two".len() as u64);
    assert_eq!(
        &client.files_read(&notes.id, None, u64::MAX).unwrap().1[..],
        b"version two"
    );
    assert!(client.files_lookup("notes.txt.sb-1a2b").unwrap().is_none());
    assert!(matches!(client.files_show(&temp.id), Err(Error::NotFound)));
    assert!(client.files_list(ROOT, true).unwrap().files.is_empty());
    // The temporary file's chunks are gone with it; two versions remain.
    assert_eq!(client.files_usage().unwrap().blobs, 2);
}

#[test]
fn credentials_destinations_and_noise() {
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

    headers.insert(
        "destination",
        "http://127.0.0.1:9/dav/t/dMsg/docs/a%20b.txt"
            .parse()
            .unwrap(),
    );
    assert_eq!(
        destination(&headers, PREFIX).as_deref(),
        Some("/docs/a b.txt")
    );
    headers.insert("destination", "/dav/t/dMsg".parse().unwrap());
    assert_eq!(destination(&headers, PREFIX).as_deref(), Some("/"));
    headers.insert("destination", "/dav/other/dMsg/x".parse().unwrap());
    assert_eq!(destination(&headers, PREFIX), None);

    assert!(
        noise(".DS_Store") && noise("._report.pdf") && noise("Thumbs.db") && noise("desktop.ini")
    );
    assert!(!noise("report.pdf") && !noise(".gitignore") && !noise("_underscore"));
    assert_eq!(split("/docs/a/b.txt"), ("docs/a".into(), "b.txt".into()));
    assert_eq!(split("/b.txt"), (String::new(), "b.txt".into()));
    assert_eq!(split("/"), (String::new(), String::new()));
}
