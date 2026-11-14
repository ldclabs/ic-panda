use super::*;

/// Authored feedback waiting for a local save, also flushed by an explicit lock.
#[derive(Clone, uniffi::Record)]
pub struct FeedbackDraftInput {
    pub id: String,
    pub revision: u64,
    pub text: String,
    pub context: String,
    pub files: Vec<String>,
}

impl FeedbackDraftInput {
    pub(crate) fn save(
        &self,
        c: &mut Client,
    ) -> dmsg_types::Result<dmsg_client::feedback::Feedback> {
        c.save_feedback(
            &self.id,
            self.revision,
            &self.text,
            &self.context,
            &self.files,
        )
    }
}

#[uniffi::export]
impl MobileEngine {
    pub async fn feedback(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.feedback() })).await
    }

    pub async fn save_feedback(&self, draft: FeedbackDraftInput) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("feedback");
        self.held_object(move |h| {
            Box::pin(async move {
                let _turn = turn.lock().await;
                h.with(|c| draft.save(c)).await
            })
        })
        .await
    }

    pub async fn send_feedback(
        &self,
        id: String,
        revision: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("feedback");
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::feedback::send(h, &id, revision).await
                })
            }))
            .await
    }

    pub async fn edit_feedback(&self, id: String) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("feedback");
        self.held_object(move |h| {
            Box::pin(async move {
                let _turn = turn.lock().await;
                h.with(|c| c.edit_feedback(&id)).await
            })
        })
        .await
    }

    pub async fn clear_feedback(&self, id: String, revision: u64) -> Result<()> {
        let turn = self.read_turns.lane("feedback");
        self.held(move |h| {
            Box::pin(async move {
                let _turn = turn.lock().await;
                h.with(|c| c.clear_feedback(&id, revision)).await
            })
        })
        .await
    }
}
