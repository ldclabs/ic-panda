//! Replacing a device on the phone: the new iPhone's request, the old
//! device's review and approval, each side's steps, and a copied workspace
//! whose files import or which approves this iPhone in its place.

use super::*;
use crate::transfer::hash;
use dmsg_client::{moved, succession};

fn step(text: &str) -> Result<succession::Step> {
    Ok(
        serde_json::from_value(serde_json::Value::String(text.into()))
            .map_err(|_| dmsg_types::invalid("step"))?,
    )
}

#[uniffi::export]
impl MobileEngine {
    /// Ask to replace administrator device `predecessor` (hex). This
    /// device's approval and cosigner keys are made and prove themselves
    /// here, which asks for the person; the view carries the token the old
    /// device reviews.
    pub async fn succession_request(&self, predecessor: String) -> Result<ObjectSnapshot> {
        let predecessor = hash(&predecessor, "predecessor")?;
        self.held_object(move |h| {
            Box::pin(async move { succession::request(h, predecessor).await })
        })
        .await
    }

    /// What a new device's request asks of this device: its enrollment,
    /// keys and the wallets the handover covers.
    pub async fn succession_review(&self, token: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| Box::pin(async move { succession::review(h, &token).await }))
            .await
    }

    /// Ask to approve the request `token` once the person named the device
    /// ID the new device shows; starts this device's handover.
    pub async fn succession_approve(
        &self,
        token: String,
        device: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { succession::accept(h, "gui", &token, &device).await })
        })
        .await
    }

    /// This device's handovers, oldest first, with the steps it runs now.
    pub async fn successions(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { succession::list(h).await }))
            .await
    }

    /// Run `step` of handover `id` here.
    pub async fn succession_run(&self, id: String, step: String) -> Result<ObjectSnapshot> {
        let (id, step) = (hash(&id, "id")?, self::step(&step)?);
        self.held_object(move |h| {
            Box::pin(async move { succession::run(h, "gui", id, step).await })
        })
        .await
    }

    /// Leave `step` of handover `id` behind, recording what stays.
    pub async fn succession_skip(
        &self,
        id: String,
        step: String,
        skipped: Vec<String>,
    ) -> Result<ObjectSnapshot> {
        let (id, step) = (hash(&id, "id")?, self::step(&step)?);
        self.held_object(move |h| {
            Box::pin(async move { succession::skip(h, id, step, skipped).await })
        })
        .await
    }

    /// Stop handover `id`; settled steps stay.
    pub async fn succession_cancel(&self, id: String) -> Result<ObjectSnapshot> {
        let id = hash(&id, "id")?;
        self.held_object(move |h| Box::pin(async move { succession::cancel(h, id).await }))
            .await
    }

    /// Import the file library of copy `id` set aside here, for the content
    /// step of `succession` when given.
    pub async fn moved_import(
        &self,
        id: String,
        succession: Option<String>,
    ) -> Result<ObjectSnapshot> {
        let succession = succession.map(|s| hash(&s, "succession")).transpose()?;
        self.held_object(move |h| Box::pin(async move { moved::import(h, &id, succession).await }))
            .await
    }

    /// Ask copy `id`, of an administrator device, to approve this device in
    /// its place before it joins.
    pub async fn moved_approve_successor(&self, id: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { moved::approve_successor(h, "gui", &id).await })
        })
        .await
    }
}
