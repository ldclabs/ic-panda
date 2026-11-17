use super::*;
use crate::testing::{TempDir, unlocked, warm};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering::SeqCst},
};

fn imported() -> (TempDir, Client, String, String) {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    warm(&mut client);
    let path = dir.path("reviewed.png");
    image::RgbImage::new(2, 2).save(&path).unwrap();
    let item = client
        .files_import(&path, "root", true, &mut |_, _| {})
        .unwrap();
    (dir, client, item.file.id, item.version)
}

#[test]
fn preparing_freezes_the_review_without_publishing_or_exposing_the_password() {
    let (_dir, mut client, file, version) = imported();
    let view = client
        .prepare_file_link_intent(
            &file,
            &version,
            LinkTerms {
                password: Some("private password not in views"),
                max_downloads: 5,
                ..Default::default()
            },
        )
        .unwrap();
    let id = view["id"].as_str().unwrap();
    assert_eq!(view["stage"], "prepared");
    assert_eq!(view["can_discard"], true);
    assert!(view["link"].is_null());
    assert!(!view.to_string().contains("private password not in views"));
    assert!(client.file_link_attempt(id).unwrap().is_none());
    client.files_rename(&file, "renamed-later.png").unwrap();
    let intent = client.file_link_intent(id).unwrap();
    assert_eq!(intent.version, version);
    assert_eq!(intent.name, "reviewed.png");
    assert_eq!(
        client.file_link_intents().unwrap()[0]["name"],
        "reviewed.png"
    );
    client.discard_file_link_intent(id).unwrap();
    assert!(client.file_link_intents().unwrap().is_empty());
}

#[tokio::test]
async fn a_discarded_local_draft_cannot_gain_a_publication_journal() {
    let (_dir, mut client, file, version) = imported();
    let view = client
        .prepare_file_link_intent(&file, &version, LinkTerms::default())
        .unwrap();
    let id = view["id"].as_str().unwrap();
    let intent = client.file_link_intent(id).unwrap();
    client.discard_file_link_intent(id).unwrap();
    let key = format!("{PUBLICATION}{id}");
    assert_eq!(
        publish_link(
            &client.hold(),
            intent.manifest.unwrap(),
            None,
            LinkTerms::default(),
            &mut |_, _, _| {},
            Some(&key)
        )
        .await
        .err(),
        Some(Error::NotFound)
    );
    assert!(client.file_link_attempt(id).unwrap().is_none());
    assert!(
        client
            .workspace
            .store
            .sealed_keys("lib-share-job:")
            .unwrap()
            .is_empty()
    );
}

struct Relay {
    client: Mutex<Client>,
    attempt: LinkAttempt,
    creates: AtomicUsize,
}

impl Hold for Relay {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut self.client.lock().unwrap())
    }

    async fn find(&self, _: &str) -> Result<Option<Value>> {
        Ok(Some(json!({"status":"committed","plan":self.attempt.plan})))
    }

    async fn get(&self, path: &str) -> Result<Value> {
        assert!(path.contains("/shares"));
        let link = &self.attempt.link;
        Ok(
            json!({"entries":[{"share_id":link.share_id,"expires_at":link.expires_at,
            "max_downloads":link.max_downloads,"password":link.password,"show_profile":link.show_profile,
            "created_at":link.created_at,"revoked_at":null,"downloads":0}],"end":true,"next_cursor":1}),
        )
    }

    async fn post<P: Serialize + Sync>(
        &self,
        path: &str,
        _: &str,
        payload: &P,
        _: &Hash,
    ) -> Result<Value> {
        let plan = &self.attempt.plan;
        if path.ends_with("/uploads/finalize") {
            return Ok(
                json!({"upload_id":plan.upload_id,"object_id":plan.object_id,"version_id":plan.version_id,"digest":plan.manifest_digest}),
            );
        }
        assert!(path.ends_with("/shares"));
        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            self.attempt.link.create_command(None)
        );
        self.creates.fetch_add(1, SeqCst);
        Err(Error::ExecutionUnknown)
    }
}

#[tokio::test]
async fn a_lost_publication_reply_reuses_the_original_edition_upload_link_and_terms() {
    let (_dir, mut client, file, version) = imported();
    let view = client
        .prepare_file_link_intent(
            &file,
            &version,
            LinkTerms {
                max_downloads: 5,
                show_profile: true,
                ..Default::default()
            },
        )
        .unwrap();
    let id = view["id"].as_str().unwrap();
    assert!(
        run_inner(&client.hold(), id, &mut |_, _, _| {})
            .await
            .is_err()
    );
    let attempt = client.file_link_attempt(id).unwrap().unwrap();
    // Once an attempt exists, it owns the new edition; retries no longer need
    // the original file key or the password used to prepare its key wrap.
    let stored = client.file_link_intent(id).unwrap();
    assert!(stored.password.is_none() && stored.manifest.is_none());
    assert_eq!(
        client.discard_file_link_intent(id),
        Err(Error::ExecutionUnknown)
    );
    let relay = Relay {
        client: Mutex::new(client),
        attempt,
        creates: AtomicUsize::new(0),
    };
    assert_eq!(
        run_inner(&relay, id, &mut |_, _, _| {}).await.err(),
        Some(Error::ExecutionUnknown)
    );
    let recovered = run_inner(&relay, id, &mut |_, _, _| {}).await.unwrap();
    assert_eq!(recovered["stage"], "complete");
    assert_eq!(recovered["link"]["share_id"], relay.attempt.link.share_id);
    assert_eq!(recovered["link"]["max_downloads"], 5);
    assert_eq!(recovered["link"]["show_profile"], true);
    assert_eq!(relay.creates.load(SeqCst), 1);
    let again = run_inner(&relay, id, &mut |_, _, _| {}).await.unwrap();
    assert_eq!(again["link"], recovered["link"]);
    assert_eq!(relay.creates.load(SeqCst), 1);
}
