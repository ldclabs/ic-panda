//! Foreground-only owner operations over the same archive implementation as the desktop.
use super::*;
use dmsg_client::vault::backup;
use zeroize::Zeroizing;

pub(super) fn discard_previous_outputs(staging: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(staging.join("vault-backups")) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir())
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| dmsg_client::upload::hex_id(name).is_ok())
        {
            let _ = std::fs::remove_file(entry.path().join("dmsg-vault.dmsgvault"));
            let _ = std::fs::remove_dir(entry.path());
        }
    }
}

#[uniffi::export]
impl MobileEngine {
    pub async fn local_import_preview(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async move {
                let request = h.with(|c| c.prepare_local_import()).await?;
                let access = tokio::task::spawn_blocking(move || request.authenticate(None))
                    .await
                    .map_err(|_| Error::Unavailable("source authentication".into()))??;
                h.with(|c| backup::direct::preview(c, access)).await
            })
        })
        .await
    }

    pub async fn local_import_commit(
        &self,
        id: String,
        items: Vec<u32>,
        files: Vec<u32>,
        folder: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                backup::direct::commit(
                    h,
                    &id,
                    items.into_iter().map(|v| v as usize).collect(),
                    files.into_iter().map(|v| v as usize).collect(),
                    &folder,
                )
                .await
            })
        })
        .await
    }

    pub async fn local_import_status(&self, id: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| Box::pin(h.with(move |c| backup::direct::status(c, &id))))
            .await
    }

    pub async fn local_import_cancel(&self, id: String) -> Result<()> {
        self.held(move |h| {
            Box::pin(h.with(move |c| {
                backup::direct::cancel(c, &id);
                Ok(())
            }))
        })
        .await
    }

    pub async fn vault_backup_begin(&self, importing: bool) -> Result<String> {
        self.held(move |h| Box::pin(h.with(move |c| backup::begin(c, importing))))
            .await
    }

    pub fn vault_backup_password(&self) -> String {
        backup::generate_password()
    }

    pub async fn vault_backup_inventory(&self, id: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| Box::pin(async move { backup::inventory(h, &id).await }))
            .await
    }

    pub async fn vault_backup_request(
        &self,
        id: String,
        items: Vec<String>,
        attachments: bool,
        password: String,
        confirmation: String,
    ) -> Result<ObjectSnapshot> {
        let password = Zeroizing::new(password);
        let confirmation = Zeroizing::new(confirmation);
        self.held_object(move |h| {
            Box::pin(h.with(move |c| {
                backup::request(c, &id, &items, attachments, &password, &confirmation)
            }))
        })
        .await
    }

    pub async fn vault_backup_export(
        &self,
        id: String,
        approval: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| Box::pin(async move { backup::export(h, &id, &approval).await }))
            .await
    }

    /// Give the OS picker a private, fully verified ciphertext copy.
    pub async fn vault_backup_output(&self, id: String) -> Result<String> {
        dmsg_client::upload::hex_id(&id)?;
        let root = self.staging.join("vault-backups");
        let directory = root.join(&id);
        std::fs::create_dir_all(&directory)
            .map_err(|_| Error::Unavailable("backup staging".into()))?;
        let file = directory.join("dmsg-vault.dmsgvault");
        self.held(move |h| {
            Box::pin(async move {
                backup::save(h, &id, &file).await?;
                Ok(file.to_string_lossy().into_owned())
            })
        })
        .await
    }

    pub async fn vault_backup_inspect(
        &self,
        id: String,
        name: String,
        password: String,
    ) -> Result<ObjectSnapshot> {
        let (path, owner) = self.staged_file(&name)?;
        let password = Zeroizing::new(password);
        self.held_object(move |h| {
            Box::pin(async move {
                h.with(|c| {
                    dmsg_types::ensure(
                        c.session_state().account.as_deref() == Some(owner.as_str()),
                        dmsg_types::Error::Forbidden,
                    )
                })
                .await?;
                backup::inspect(h, &id, &path, &password).await
            })
        })
        .await
    }

    pub async fn vault_backup_commit(
        &self,
        id: String,
        digest: String,
        items: Vec<u32>,
        folder: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(h.with(move |c| {
                backup::commit(
                    c,
                    &id,
                    &digest,
                    items.into_iter().map(|i| i as usize).collect(),
                    &folder,
                )
            }))
        })
        .await
    }

    pub async fn vault_backup_status(&self, id: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| Box::pin(h.with(move |c| backup::progress(c, &id))))
            .await
    }

    pub async fn vault_backup_cancel(&self, id: String) -> Result<()> {
        dmsg_client::upload::hex_id(&id)?;
        let directory = self.staging.join("vault-backups").join(&id);
        let result = self
            .held(move |h| {
                Box::pin(h.with(move |c| {
                    backup::cancel(c, &id);
                    Ok(())
                }))
            })
            .await;
        let _ = std::fs::remove_file(directory.join("dmsg-vault.dmsgvault"));
        let _ = std::fs::remove_dir(directory);
        result
    }
}
