//! Housekeeping: trashed files and folders are deleted for good after
//! `files_trash_days`, and chunk files nothing references any more go. The
//! reference set is every local version's chunks, every version's
//! thumbnail, the chunks of unfinished imports and the chunks of the
//! library's cloud jobs (backups and shared editions in flight); it is
//! derived by scanning, never maintained as counts. A version evicted to
//! the cloud references nothing but its thumbnail, so its chunks go.
//! Temporary chunk files older than a day go too.

use super::catalog::{Version, VERSION};
use super::Client;
use dmsg_types::{Result, DAY, MINUTE};
use serde::Serialize;
use std::collections::BTreeSet;
use std::time::Duration;

/// Maintenance runs at most this often.
pub const MAINTENANCE_MS: u64 = 10 * MINUTE;

/// What a maintenance pass did.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Maintenance {
    /// Files deleted for good.
    pub purged_files: u64,
    /// Folders deleted for good.
    pub purged_folders: u64,
    /// Chunk files deleted.
    pub collected: u64,
    /// Temporary chunk files deleted.
    pub tmp_removed: u64,
}

impl Client {
    /// Every chunk digest something still needs.
    fn referenced_blobs(&self) -> Result<BTreeSet<String>> {
        let mut keep = BTreeSet::new();
        for key in self.workspace.store.sealed_keys(VERSION)? {
            if let Some(version) = self.workspace.private::<Version>(self.keys()?, &key)? {
                if version.local {
                    keep.extend(version.chunks.into_iter().map(|c| c.digest));
                }
                keep.extend(version.thumbnail);
            }
        }
        for job in self.import_jobs()? {
            keep.extend(job.chunks.into_iter().map(|c| c.digest));
        }
        keep.extend(self.library_job_blobs()?);
        Ok(keep)
    }

    /// Delete the chunk files nothing references and the day-old temporary
    /// files; returns how many of each. Backups in flight keep their chunks
    /// through the reference set; a fetch holds the client while it adds
    /// chunks, so nothing collects beside it.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub(crate) fn collect_blobs(&mut self) -> Result<(u64, u64)> {
        self.keys()?;
        let keep = self.referenced_blobs()?;
        let blobs = self.blobs()?;
        let mut collected = 0;
        for (digest, _) in blobs.entries()? {
            if !keep.contains(&digest) {
                blobs.delete(&digest)?;
                collected += 1;
            }
        }
        let tmp_removed = blobs.sweep_tmp(Duration::from_millis(DAY))?;
        Ok((collected, tmp_removed))
    }

    /// The periodic pass: delete trashed files and folders past
    /// `files_trash_days`, then collect chunks. `None` when the last pass
    /// was less than [`MAINTENANCE_MS`] ago.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_maintain(&mut self, now: u64) -> Result<Option<Maintenance>> {
        self.keys()?;
        if self
            .library_maintained_at
            .is_some_and(|at| now < at + MAINTENANCE_MS)
        {
            return Ok(None);
        }
        self.library_maintained_at = Some(now);
        let keep_ms = self.config.files_trash_days * DAY;
        let expired: Vec<String> = {
            let catalog = self.catalog()?;
            catalog
                .folders
                .iter()
                .filter(|(_, f)| f.trashed_at.is_some_and(|t| t + keep_ms <= now))
                .map(|(id, _)| id.clone())
                .chain(
                    catalog
                        .files
                        .iter()
                        .filter(|(_, f)| f.trashed_at.is_some_and(|t| t + keep_ms <= now))
                        .map(|(id, _)| id.clone()),
                )
                .collect()
        };
        let report = self.files_purge(Some(&expired))?;
        Ok(Some(report))
    }
}
