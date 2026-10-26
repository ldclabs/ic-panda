//! Owner-only mobile secret workflows. No daemon dispatcher, Agent execution,
//! filesystem paths, process claims or general plaintext resolver are exported.
use crate::{MobileEngine, ObjectSnapshot, OperationToken, error::Result};
use dmsg_client::{
    hold::{Hold, Unlocked},
    secrets::{SecretRef, receiving, shares},
};
use dmsg_types::{Error, ensure, ensure_valid};
use serde_json::json;
use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

/// Whole-package buffers are capped independently of the 100 MiB Files limit.
/// This is a payload ceiling, not a total-memory guarantee: one MiB chunk
/// buffers and the bounded JSON manifest are additional allocations. Password
/// wrapping uses the existing 19 MiB Argon2 workspace in a separate phase.
const MAX_PACKAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(uniffi::Record)]
pub struct SecretAttachmentSelection {
    pub file_id: String,
    pub version_id: String,
}

#[derive(uniffi::Record)]
pub struct SecretSnapshotInput {
    pub item_id: String,
    pub revision: String,
    pub field_ids: Vec<String>,
    pub include_notes: bool,
    pub attachments: Vec<SecretAttachmentSelection>,
    pub expires_in_ms: Option<u64>,
    pub max_download_sessions: Option<u64>,
}

impl From<SecretSnapshotInput> for shares::ShareInput {
    fn from(input: SecretSnapshotInput) -> Self {
        Self {
            item_id: input.item_id,
            revision: input.revision,
            field_ids: input.field_ids,
            include_notes: input.include_notes,
            attachments: input
                .attachments
                .into_iter()
                .map(|a| shares::AttachmentInput {
                    file_id: a.file_id,
                    version_id: a.version_id,
                })
                .collect(),
            expires_in_ms: input.expires_in_ms,
            max_download_sessions: input.max_download_sessions,
        }
    }
}

#[derive(uniffi::Record)]
pub struct SecretPackageSelection {
    pub package_id: String,
    pub digest: String,
    pub fields: Vec<String>,
    pub include_notes: bool,
    pub attachments: Vec<u32>,
    pub folder: String,
}

impl From<SecretPackageSelection> for receiving::Selection {
    fn from(selection: SecretPackageSelection) -> Self {
        Self {
            package_id: selection.package_id,
            digest: selection.digest,
            fields: selection.fields,
            include_notes: selection.include_notes,
            attachments: selection
                .attachments
                .into_iter()
                .map(|index| index as usize)
                .collect(),
            folder: selection.folder,
        }
    }
}

#[uniffi::export]
impl MobileEngine {
    pub async fn secret_limits(&self) -> Result<ObjectSnapshot> {
        self.object(|_| {
            Box::pin(async {
                Ok(json!({"max_package_bytes":MAX_PACKAGE_BYTES,
                "chunk_bytes":dmsg_client::files::CHUNK,
                "manifest_bytes":dmsg_client::secrets::package::MAX_MANIFEST,
                "password_memory_kib":19 * 1024,"device_local":true}))
            })
        })
        .await
    }

    pub async fn secret_reference(
        &self,
        item_id: String,
        field_id: String,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_ref(item_id, field_id) }))
            .await
    }

    /// Attachment IDs synchronize with the item; the Files bytes and backups do not.
    pub async fn secret_item_attachments(&self, item_id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                let mut entry = c.vault_show(&item_id)?;
                let ids = std::mem::take(&mut entry.item.files);
                for field in &mut entry.item.fields {
                    field.value.zeroize();
                }
                entry.item.notes.zeroize();
                let mut files = Vec::with_capacity(ids.len());
                for id in ids {
                    match c.files_show(&id) {
                        Ok(details) => {
                            files.push(json!({"id":id,"available":true,"details":details}))
                        }
                        Err(Error::NotFound) => {
                            files.push(json!({"id":id,"available":false,"error":"NOT_FOUND"}))
                        }
                        Err(error) => return Err(error),
                    }
                }
                Ok(
                    json!({"item_id":item_id,"revision":entry.revision,"files":files,
                "device_local":true,"attachments_sync_separately":true}),
                )
            })
        })
        .await
    }

    pub async fn secret_package_preview(
        &self,
        link: String,
        password: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(
            link.len() <= 8192 && password.as_ref().is_none_or(|p| p.len() <= 4096),
            "share input",
        )?;
        let link = Zeroizing::new(link);
        let password = password.map(Zeroizing::new);
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    receiving::preview_limited(
                        &c.cooperative_hold(),
                        &link,
                        password.as_deref().map(|p| p.as_str()),
                        MAX_PACKAGE_BYTES,
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn secret_channel_package_preview(
        &self,
        channel: String,
        sequence: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    receiving::channel_preview_limited(
                        &c.cooperative_hold(),
                        &channel,
                        sequence,
                        MAX_PACKAGE_BYTES,
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn secret_package_import_request(
        &self,
        selection: SecretPackageSelection,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                c.secret_package_request_limited("gui", selection.into(), MAX_PACKAGE_BYTES)
            })
        })
        .await
    }

    pub async fn secret_share_prepare(
        &self,
        input: SecretSnapshotInput,
        password: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(
            password.as_ref().is_none_or(|p| p.len() <= 4096),
            "share password",
        )?;
        let password = password.map(Zeroizing::new);
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    let hold = c.cooperative_hold();
                    let bound = Unlocked::new(&hold).await?;
                    let prepared = bound
                        .with(|c| {
                            c.secret_share_prepare(input.into())?
                                .limit_size(MAX_PACKAGE_BYTES)
                        })
                        .await?;
                    let prepared = crate::runtime()
                        .spawn_blocking(move || {
                            prepared.wrap_password(password.as_deref().map(|p| p.as_str()))
                        })
                        .await
                        .map_err(|_| Error::Unavailable("password preparation".into()))??;
                    bound
                        .with(|c| c.secret_share_save_preparation(prepared))
                        .await
                })
            }))
            .await
    }

    pub async fn secret_channel_share_prepare(
        &self,
        channel: String,
        input: SecretSnapshotInput,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure(
            input.expires_in_ms.is_none() && input.max_download_sessions.is_none(),
            Error::InvalidInput("channel delivery has no link limits".into()),
        )?;
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    shares::prepare_channel_limited(
                        &c.cooperative_hold(),
                        &channel,
                        input.into(),
                        MAX_PACKAGE_BYTES,
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn secret_share_request(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_share_request("gui", &id) }))
            .await
    }

    pub async fn secret_shares(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.secret_shares() })).await
    }

    /// The UI must set include_link only for an explicit owner reveal/copy/share.
    pub async fn secret_share(&self, id: String, include_link: bool) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_share_get(&id, include_link) }))
            .await
    }

    pub async fn secret_share_reconcile(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move { shares::reconcile(&c.cooperative_hold(), &id).await })
            }))
            .await
    }

    pub async fn secret_share_revoke(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move { shares::revoke(&c.cooperative_hold(), &id).await })
            }))
            .await
    }

    pub async fn secret_health(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.secret_health() })).await
    }

    pub async fn secret_health_dismiss(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_health_dismiss(&id) }))
            .await
    }

    pub async fn secret_references(&self, reference: String) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.secret_references(&SecretRef::parse(&reference)?) })
        })
        .await
    }

    pub async fn secret_rotations(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async { Ok(json!({"rotations":c.secret_rotations()?,"device_local":true})) })
        })
        .await
    }

    pub async fn secret_rotation_create(&self, reference: String) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.secret_rotation_create(SecretRef::parse(&reference)?) })
        })
        .await
    }

    pub async fn secret_rotation_dependencies(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_rotation_dependencies(&id) }))
            .await
    }

    pub async fn secret_rotation_update(
        &self,
        id: String,
        step: String,
        dependencies_digest: Option<String>,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(
                async move { c.secret_rotation_update(&id, &step, dependencies_digest.as_deref()) },
            )
        })
        .await
    }

    pub async fn secret_activity(
        &self,
        before: Option<String>,
        limit: u32,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move { c.secret_activity(before.as_deref(), limit as usize) })
        })
        .await
    }

    pub async fn secret_activity_cancel(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_activity_cancel(&id) }))
            .await
    }

    pub async fn secret_activity_reviewed(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.secret_activity_reviewed(&id) }))
            .await
    }
}
