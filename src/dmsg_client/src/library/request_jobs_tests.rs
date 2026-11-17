use super::*;
use crate::cloud::Prepared;
use crate::testing::{TempDir, unlocked, warm};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
};

fn draft(client: &mut Client) -> String {
    client
        .prepare_file_request_intents(
            "Reviewed request",
            "Send the signed copy",
            &["Ada".into()],
            RequestTerms {
                expires_in_ms: Some(14 * DAY),
                max_total_bytes: Some(500 * MIB),
            },
        )
        .unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_batch_is_atomic_and_never_discloses_unpublished_keys() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    warm(&mut client);
    assert!(
        client
            .prepare_file_request_intents(
                "Title",
                "",
                &["Ada".into(), "x".repeat(MAX_LABEL + 1)],
                RequestTerms::default()
            )
            .is_err()
    );
    assert!(client.file_request_intents().unwrap().is_empty());
    let id = draft(&mut client);
    let initial = client.file_request_intents().unwrap();
    assert_eq!(initial[0]["stage"], "prepared");
    assert!(initial[0]["link"].is_null());
    client.prepare_request_publication(&id, 500 * MIB).unwrap();
    let shown = client.file_request_intents().unwrap();
    assert_eq!(shown[0]["stage"], "checking");
    let publication = client.request_intent(&id).unwrap().publication.unwrap();
    let secrets = client
        .open_request_secrets(&id, &publication.management)
        .unwrap()
        .unwrap();
    assert!(!shown[0].to_string().contains(&secrets.hpke_seed));
    assert!(!shown[0].to_string().contains(&secrets.page_key));
    assert_eq!(
        client.discard_file_request_intent(&id),
        Err(Error::ExecutionUnknown)
    );
    let discarded = draft(&mut client);
    client.discard_file_request_intent(&discarded).unwrap();
    assert!(matches!(
        client.prepare_request_publication(&discarded, MIB),
        Err(Error::NotFound)
    ));
}

struct Relay {
    client: Mutex<Client>,
    id: String,
    submitted: Mutex<Vec<(Hash, Vec<u8>)>>,
    visible: AtomicBool,
    corrupt: AtomicBool,
    reads: AtomicUsize,
}

impl Relay {
    fn remote(&self) -> Value {
        let c = self.client.lock().unwrap();
        let p = c.request_intent(&self.id).unwrap().publication.unwrap();
        json!({"request_id":self.id,"expires_at":p.expires_at,
            "max_total_bytes":if self.corrupt.load(SeqCst) { p.max_total_bytes + 1 } else { p.max_total_bytes },
            "management":p.management,"created_at":now_ms(),"revoked_at":null,
            "status":"open","files":0,"bytes":0,"last_stored_at":null})
    }
}

impl Hold for Relay {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        step(&mut self.client.lock().unwrap())
    }

    async fn get(&self, path: &str) -> Result<Value> {
        assert!(path.contains("/links?after="));
        self.reads.fetch_add(1, SeqCst);
        let entries = if self.visible.load(SeqCst) {
            vec![self.remote()]
        } else {
            vec![]
        };
        Ok(json!({"entries":entries,"end":true,"next_cursor":1}))
    }

    async fn send(
        &self,
        mut sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
    ) -> Result<Value> {
        // Exercise the real ready/generation/expiry and command-signing step.
        let mut c = self.client.lock().unwrap();
        sign(&mut c)?;
        let intent = c.request_intent(&self.id)?;
        self.submitted
            .lock()
            .unwrap()
            .push((intent.command, canonical(&intent.publication.unwrap())));
        Err(Error::ExecutionUnknown)
    }
}

fn relay() -> (TempDir, Relay) {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    warm(&mut client);
    let id = draft(&mut client);
    (
        dir,
        Relay {
            client: Mutex::new(client),
            id,
            submitted: Mutex::new(vec![]),
            visible: AtomicBool::new(false),
            corrupt: AtomicBool::new(false),
            reads: AtomicUsize::new(0),
        },
    )
}

#[tokio::test]
async fn uncertain_create_retries_the_exact_command_then_recovers_without_republication() {
    let (_dir, relay) = relay();
    assert_eq!(
        run_inner(&relay, &relay.id).await.err(),
        Some(Error::ExecutionUnknown)
    );
    assert_eq!(
        run_inner(&relay, &relay.id).await.err(),
        Some(Error::ExecutionUnknown)
    );
    {
        let sent = relay.submitted.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0], sent[1]);
    }
    relay.visible.store(true, SeqCst);
    let view = run_inner(&relay, &relay.id).await.unwrap();
    assert_eq!(view["stage"], "complete");
    assert_eq!(view["link"]["request_id"], relay.id);
    assert_eq!(view["link"]["label"], "Ada");
    assert!(view["link"]["url"].as_str().unwrap().contains('#'));
    assert_eq!(run_inner(&relay, &relay.id).await.unwrap(), view);
    assert_eq!(relay.submitted.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn mismatched_remote_terms_are_not_a_success_receipt() {
    let (_dir, relay) = relay();
    assert!(run_inner(&relay, &relay.id).await.is_err());
    relay.visible.store(true, SeqCst);
    relay.corrupt.store(true, SeqCst);
    assert_eq!(
        run_inner(&relay, &relay.id).await.err(),
        Some(Error::IntegrityFailed)
    );
    let c = relay.client.lock().unwrap();
    assert!(!c.request_intent(&relay.id).unwrap().complete);
    assert!(c.files_requests().unwrap().is_empty());
}

#[tokio::test]
async fn a_new_root_refuses_old_key_publication_but_can_reconcile_an_already_created_link() {
    let (_dir, relay) = relay();
    assert!(run_inner(&relay, &relay.id).await.is_err());
    {
        let mut c = relay.client.lock().unwrap();
        c.root.as_mut().unwrap().generation = 2;
        c.security
            .as_mut()
            .unwrap()
            .snapshot
            .content_root_generation = 2;
    }
    assert_eq!(
        run_inner(&relay, &relay.id).await.err(),
        Some(Error::RekeyRequired)
    );
    assert_eq!(relay.submitted.lock().unwrap().len(), 1);
    relay.visible.store(true, SeqCst);
    assert_eq!(
        run_inner(&relay, &relay.id).await.unwrap()["stage"],
        "complete"
    );
    assert_eq!(relay.submitted.lock().unwrap().len(), 1);
}
