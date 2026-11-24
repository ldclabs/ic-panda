use crate::*;
use dmsg_client::library::{
    backup, cloud,
    link::{self, LinkTerms},
    request::{self, RequestDraft, RequestTerms},
};

#[derive(Clone, uniffi::Record)]
pub struct FileMetadata {
    pub tags: Vec<String>,
    pub note: String,
    pub starred: bool,
}

#[uniffi::export]
impl MobileEngine {
    pub async fn file_folders(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.files_tree() })).await
    }

    pub async fn file_library_usage(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.files_usage() })).await
    }

    /// Local-only bounded content/thumbnail preview; never restores a cloud copy.
    pub async fn file_preview(&self, id: String, version: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_preview(&id, Some(&version)) }))
            .await
    }

    /// Full verified in-memory preview for fullscreen viewing (images up to 20 MiB,
    /// parsed safe Markdown up to 2 MiB, plain text up to 1 MiB) without writing
    /// plaintext temporary files.
    pub async fn preview_library_file(
        &self,
        id: String,
        version: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<PreviewPayload> {
        let generation = self.host.generation();
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let preview =
                        dmsg_client::library::reading::preview_full(h, &id, version.as_deref())
                            .await?;
                    Ok(PreviewPayload::from_full(generation, preview))
                })
            }))
            .await
    }

    /// Compare the original fields, so two edits within one clock millisecond
    /// cannot overwrite each other through a timestamp collision.
    pub async fn update_file_metadata(
        &self,
        id: String,
        before: FileMetadata,
        after: FileMetadata,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                let current = c.files_show(&id)?.file;
                ensure(
                    current.tags == before.tags
                        && current.note == before.note
                        && current.starred == before.starred,
                    Error::VersionConflict,
                )?;
                c.files_update(&id, Some(after.tags), Some(after.note), Some(after.starred))
            })
        })
        .await
    }

    pub async fn use_file_version(
        &self,
        id: String,
        version: String,
        expected_current: String,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                ensure(
                    c.files_show(&id)?.file.current == expected_current,
                    Error::VersionConflict,
                )?;
                c.files_version_use(&id, &version)
            })
        })
        .await
    }

    /// A viewed historical version cannot become a deletion of a newer current one.
    pub async fn delete_file_version(
        &self,
        id: String,
        version: String,
        expected_current: String,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                ensure(
                    version != expected_current
                        && c.files_show(&id)?.file.current == expected_current,
                    Error::VersionConflict,
                )?;
                c.files_version_delete(&id, &version)
            })
        })
        .await
    }

    /// Purge only the explicitly reviewed trashed target, including its contents if a folder.
    pub async fn purge_file(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_purge(Some(&[id])) }))
            .await
    }

    /// Freeze one library version and link terms locally before publication.
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_file_link(
        &self,
        id: String,
        version: String,
        expires_in_ms: u64,
        max_downloads: u64,
        password: Option<String>,
        show_profile: bool,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                h.with(|c| {
                    c.prepare_file_link_intent(
                        &id,
                        &version,
                        LinkTerms {
                            expires_in_ms: Some(expires_in_ms),
                            max_downloads,
                            password: password.as_deref(),
                            show_profile,
                        },
                    )
                })
                .await
            })
        })
        .await
    }

    /// Freeze the selected cloud version without importing it into the library.
    pub async fn prepare_backup_link(
        &self,
        upload: String,
        expires_in_ms: u64,
        max_downloads: u64,
        password: Option<String>,
        show_profile: bool,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                h.with(|c| {
                    c.prepare_backup_link_intent(
                        &upload,
                        LinkTerms {
                            expires_in_ms: Some(expires_in_ms),
                            max_downloads,
                            password: password.as_deref(),
                            show_profile,
                        },
                    )
                })
                .await
            })
        })
        .await
    }

    pub async fn file_link_intents(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { h.with(|c| c.file_link_intents()).await }))
            .await
    }

    /// Resume original encrypted edition/upload/publication, never another link.
    pub async fn run_file_link_intent(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        dmsg_client::upload::hex_id(&id)?;
        let turn = self.read_turns.lane(&format!("file-link:{id}"));
        let progress = operation.clone();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    link::jobs::run(h, &id, &mut |_, done, total| progress.report(done, total))
                        .await
                })
            }))
            .await
    }

    /// Only drafts with no remote attempt can be discarded.
    pub async fn discard_file_link_intent(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        dmsg_client::upload::hex_id(&id)?;
        let turn = self.read_turns.lane(&format!("file-link:{id}"));
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    h.with(|c| c.discard_file_link_intent(&id)).await
                })
            }))
            .await
    }

    /// The current account's backup limit and remaining cloud capacity.
    pub async fn file_backup_capacity(
        &self,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(|h| Box::pin(async { cloud::cloud_usage(h).await })))
            .await
    }

    /// Keep the catalog entry and verified cloud copy, removing local chunks only.
    pub async fn evict_file(
        &self,
        id: String,
        version: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    // Reconcile deleted backups before allowing the local copy to go.
                    cloud::cloud_refresh(h).await?;
                    h.with(|c| c.files_evict(&id, Some(&version))).await
                })
            }))
            .await
    }

    /// Share a library version by link; one freed from this device comes down
    /// from its backup only long enough to be re-encrypted.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_file_link(
        &self,
        id: String,
        version: String,
        expires_in_ms: u64,
        max_downloads: u64,
        password: Option<String>,
        show_profile: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let terms = LinkTerms {
                        expires_in_ms: Some(expires_in_ms),
                        max_downloads,
                        password: password.as_deref(),
                        show_profile,
                    };
                    link::share_link(h, &id, Some(&version), terms, &mut |_, _, _| {}).await
                })
            }))
            .await
    }

    /// Share a cloud backup by link; the library does not change.
    pub async fn create_backup_link(
        &self,
        upload: String,
        expires_in_ms: u64,
        max_downloads: u64,
        password: Option<String>,
        show_profile: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let terms = LinkTerms {
                        expires_in_ms: Some(expires_in_ms),
                        max_downloads,
                        password: password.as_deref(),
                        show_profile,
                    };
                    link::share_backup_link(h, &upload, terms, &mut |_, _, _| {}).await
                })
            }))
            .await
    }

    /// The account's share links as saved here; `refresh` reads the relay's
    /// counts and the links other devices made first.
    pub async fn file_links(
        &self,
        refresh: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    if refresh {
                        link::share_links_refresh(h).await
                    } else {
                        h.with(|c| c.files_share_links()).await
                    }
                })
            }))
            .await
    }

    pub async fn revoke_file_link(
        &self,
        share_id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { link::share_revoke(h, &share_id).await })
            }))
            .await
    }

    /// Freeze a reviewed batch locally before publishing any request link.
    pub async fn prepare_file_requests(
        &self,
        title: String,
        instructions: String,
        labels: Vec<String>,
        expires_in_ms: u64,
        max_total_bytes: Option<u64>,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                h.with(|c| {
                    c.prepare_file_request_intents(
                        &title,
                        &instructions,
                        &labels,
                        RequestTerms {
                            expires_in_ms: Some(expires_in_ms),
                            max_total_bytes,
                        },
                    )
                })
                .await
            })
        })
        .await
    }

    pub async fn file_request_intents(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { h.with(|c| c.file_request_intents()).await }))
            .await
    }

    pub async fn run_file_request_intent(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        dmsg_client::upload::hex_id(&id)?;
        let turn = self.read_turns.lane(&format!("file-request:{id}"));
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    request::jobs::run(h, &id).await
                })
            }))
            .await
    }

    pub async fn discard_file_request_intent(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        dmsg_client::upload::hex_id(&id)?;
        let turn = self.read_turns.lane(&format!("file-request:{id}"));
        operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    h.with(|c| c.discard_file_request_intent(&id)).await
                })
            }))
            .await
    }

    /// Make a link that receives files from people without an account.
    pub async fn create_file_request(
        &self,
        title: String,
        instructions: String,
        label: String,
        expires_in_ms: Option<u64>,
        max_total_bytes: Option<u64>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let draft = RequestDraft {
                        title: &title,
                        instructions: &instructions,
                        label: &label,
                    };
                    let terms = RequestTerms {
                        expires_in_ms,
                        max_total_bytes,
                    };
                    request::request_create(h, draft, terms).await
                })
            }))
            .await
    }

    /// The account's request links as saved here; `refresh` reads the relay first.
    pub async fn file_requests(
        &self,
        refresh: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    if refresh {
                        request::requests_refresh(h).await
                    } else {
                        h.with(|c| c.files_requests()).await
                    }
                })
            }))
            .await
    }

    /// Close a request link; files already received stay.
    pub async fn revoke_file_request(
        &self,
        request_id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { request::request_revoke(h, &request_id).await })
            }))
            .await
    }

    /// A request link's files; their contents stay in the cloud until saved.
    pub async fn file_request_files(
        &self,
        request_id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { request::request_files(h, &request_id).await })
            }))
            .await
    }

    /// Download a received file and save it to the library; without `folder`
    /// it goes under `root`, the app's localized name for received files.
    pub async fn save_request_file(
        &self,
        upload: String,
        folder: Option<String>,
        root: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        dmsg_client::upload::hex_id(&upload)?;
        let turn = self.read_turns.lane(&format!("received-file:{upload}"));
        let progress = operation.clone();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    request::request_save(
                        h,
                        &upload,
                        folder.as_deref(),
                        &root,
                        &mut |_, done, total| progress.report(done, total),
                    )
                    .await
                })
            }))
            .await
    }

    /// Delete a received file's cloud copy; a saved copy stays.
    pub async fn delete_request_file(
        &self,
        upload: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        dmsg_client::upload::hex_id(&upload)?;
        let turn = self.read_turns.lane(&format!("received-file:{upload}"));
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    request::request_delete_file(h, &upload).await?;
                    Ok(serde_json::json!({"deleted": true}))
                })
            }))
            .await
    }

    pub async fn retry_backup(&self, id: String, version: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_backup_retry(Some(&id), Some(&version)) }))
            .await?;
        Ok(())
    }

    pub async fn retry_download(&self, upload: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_download_retry(Some(&upload)) }))
            .await?;
        Ok(())
    }

    pub async fn cancel_backup(
        &self,
        id: String,
        version: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        operation
            .run(self.held(move |h| {
                Box::pin(async move { backup::backup_cancel(h, &id, &version).await })
            }))
            .await
    }
}
