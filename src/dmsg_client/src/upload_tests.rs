use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Relay {
    status: Value,
    result: Value,
    finalized: AtomicUsize,
}

impl Hold for Relay {
    async fn with<T: Send>(&self, _: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        panic!("a committed upload must not read or upload local chunks")
    }

    async fn find(&self, _: &str) -> Result<Option<Value>> {
        Ok(Some(self.status.clone()))
    }

    async fn post<P: Serialize + Sync>(
        &self,
        path: &str,
        action: &str,
        payload: &P,
        _: &Hash,
    ) -> Result<Value> {
        assert!(path.ends_with("/uploads/finalize"));
        assert_eq!(action, "dmsg/upload/finalize/v1");
        assert_eq!(
            serde_json::to_value(payload).unwrap()["upload_id"],
            self.status["plan"]["upload_id"]
        );
        self.finalized.fetch_add(1, Ordering::SeqCst);
        Ok(self.result.clone())
    }
}

fn fixture(kind: &str, published: bool) -> (UploadPlan, Relay) {
    let plan = UploadPlan {
        upload_id: "01".repeat(32),
        object_id: "02".repeat(32),
        version_id: "03".repeat(32),
        root_generation: 0,
        epoch: 0,
        control_head: None,
        kind: kind.into(),
        chunks: vec![],
        manifest_digest: "04".repeat(32),
        manifest_size: 0,
        expires_at: 1,
    };
    let url = format!(
        "https://assets.dmsg.net/avatars/040g2081040g2081040g/{}",
        plan.upload_id
    );
    let relay = Relay {
        status: json!({"status":"committed", "plan":plan, "public_url":published.then_some(&url)}),
        result: json!({"upload_id":plan.upload_id, "object_id":plan.object_id, "version_id":plan.version_id, "digest":plan.manifest_digest, "public_url":url}),
        finalized: AtomicUsize::new(0),
    };
    (plan, relay)
}

#[tokio::test]
async fn committed_avatar_retries_publication_without_uploading_or_reserving_again() {
    let (plan, relay) = fixture("avatar", false);
    let result = upload(
        &relay,
        "/v1/accounts/040g2081040g2081040g",
        &plan,
        &[],
        0,
        Source::Store,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result.as_deref(), relay.result["public_url"].as_str());
    assert_eq!(relay.finalized.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn published_avatar_reuses_the_exact_server_url_and_private_uploads_need_no_publication() {
    for (kind, published) in [("avatar", true), ("file", false)] {
        let (plan, relay) = fixture(kind, published);
        let result = upload(
            &relay,
            "/v1/accounts/040g2081040g2081040g",
            &plan,
            &[],
            0,
            Source::Store,
            |_, _| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(result.as_deref(), relay.status["public_url"].as_str());
        assert_eq!(relay.finalized.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn avatar_finalize_requires_a_matching_object_and_a_public_url() {
    for field in ["public_url", "digest", "upload_id"] {
        let (plan, mut relay) = fixture("avatar", false);
        relay.result.as_object_mut().unwrap().remove(field);
        assert_eq!(
            finalize(&relay, "/v1/accounts/040g2081040g2081040g", &plan).await,
            Err(Error::IntegrityFailed)
        );
    }
}
