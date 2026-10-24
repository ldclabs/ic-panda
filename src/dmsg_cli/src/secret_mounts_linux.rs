//! Linux FUSE adapter with kernel request cancellation and actual open-handle grants.
use super::*;
use fuse3::{
    FileType, MountOptions, Timestamp,
    raw::{Filesystem, MountHandle, Request, Session, reply::*},
};
use futures_util::{Stream, stream};
use std::{
    ffi::OsStr,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

#[derive(Clone)]
struct Node {
    ino: u64,
    parent: u64,
    name: String,
    entry: Option<mounts::Entry>,
}
struct PendingRead {
    handle: u64,
    cancelled: Arc<AtomicBool>,
    changed: Arc<tokio::sync::Notify>,
}
struct FilesystemAdapter {
    host: Weak<Daemon>,
    id: String,
    owner: u32,
    nodes: Vec<Node>,
    created: Timestamp,
    reads: Mutex<BTreeMap<u64, PendingRead>>,
}
struct Reading<'a> {
    reads: &'a Mutex<BTreeMap<u64, PendingRead>>,
    unique: u64,
}
impl Drop for Reading<'_> {
    fn drop(&mut self) {
        self.reads.lock().expect("FUSE reads").remove(&self.unique);
    }
}
impl FilesystemAdapter {
    fn attr(&self, node: &Node) -> FileAttr {
        FileAttr {
            ino: node.ino,
            size: node.entry.as_ref().map_or(0, |e| e.size),
            blocks: 0,
            atime: self.created,
            mtime: self.created,
            ctime: self.created,
            kind: if node.entry.is_some() {
                FileType::RegularFile
            } else {
                FileType::Directory
            },
            perm: if node.entry.is_some() { 0o400 } else { 0o500 },
            nlink: 1,
            uid: self.owner,
            gid: unsafe { libc::getegid() },
            rdev: 0,
            blksize: 4096,
        }
    }

    fn node(&self, ino: u64) -> fuse3::Result<Node> {
        self.nodes
            .iter()
            .find(|n| n.ino == ino)
            .cloned()
            .ok_or(libc::ENOENT.into())
    }

    fn owner(&self, request: Request) -> fuse3::Result<()> {
        if request.uid == self.owner {
            Ok(())
        } else {
            Err(libc::EACCES.into())
        }
    }
}
impl Filesystem for FilesystemAdapter {
    async fn init(&self, _req: Request) -> fuse3::Result<ReplyInit> {
        Ok(ReplyInit {
            max_write: NonZeroU32::new(1048576).expect("nonzero"),
        })
    }

    async fn destroy(&self, _req: Request) {
        for read in self.reads.lock().expect("FUSE reads").values() {
            read.cancelled.store(true, Ordering::Release);
            read.changed.notify_waiters();
        }
        if let Some(d) = self.host.upgrade() {
            let _ = d.client().lock().await.secret_mount_stop(&self.id);
        }
    }

    async fn lookup(&self, req: Request, parent: u64, name: &OsStr) -> fuse3::Result<ReplyEntry> {
        self.owner(req)?;
        let node = self
            .nodes
            .iter()
            .find(|n| n.parent == parent && OsStr::new(&n.name) == name)
            .ok_or(fuse3::Errno::from(libc::ENOENT))?;
        Ok(ReplyEntry {
            ttl: Duration::ZERO,
            attr: self.attr(node),
            generation: node.entry.as_ref().map_or(0, |e| e.generation),
        })
    }

    async fn getattr(
        &self,
        req: Request,
        ino: u64,
        _fh: Option<u64>,
        _flags: u32,
    ) -> fuse3::Result<ReplyAttr> {
        self.owner(req)?;
        Ok(ReplyAttr {
            ttl: Duration::ZERO,
            attr: self.attr(&self.node(ino)?),
        })
    }

    async fn opendir(&self, req: Request, ino: u64, _flags: u32) -> fuse3::Result<ReplyOpen> {
        self.owner(req)?;
        if self.node(ino)?.entry.is_some() {
            return Err(libc::ENOTDIR.into());
        }
        Ok(ReplyOpen { fh: 0, flags: 0 })
    }

    async fn readdir<'a>(
        &'a self,
        req: Request,
        ino: u64,
        _fh: u64,
        offset: i64,
    ) -> fuse3::Result<ReplyDirectory<impl Stream<Item = fuse3::Result<DirectoryEntry>> + Send + 'a>>
    {
        self.owner(req)?;
        let node = self.node(ino)?;
        if node.entry.is_some() || offset < 0 {
            return Err(libc::EINVAL.into());
        }
        let mut rows = vec![
            (ino, FileType::Directory, ".".into()),
            (node.parent, FileType::Directory, "..".into()),
        ];
        rows.extend(
            self.nodes
                .iter()
                .filter(|n| n.parent == ino && n.ino != 1)
                .map(|n| {
                    (
                        n.ino,
                        if n.entry.is_some() {
                            FileType::RegularFile
                        } else {
                            FileType::Directory
                        },
                        n.name.clone(),
                    )
                }),
        );
        let entries = rows
            .into_iter()
            .enumerate()
            .skip(offset as usize)
            .map(|(i, (inode, kind, name))| {
                Ok(DirectoryEntry {
                    inode,
                    kind,
                    name: name.into(),
                    offset: (i + 1) as i64,
                })
            })
            .collect::<Vec<_>>();
        Ok(ReplyDirectory {
            entries: stream::iter(entries),
        })
    }

    async fn open(&self, req: Request, ino: u64, flags: u32) -> fuse3::Result<ReplyOpen> {
        self.owner(req)?;
        if flags as i32 & libc::O_ACCMODE != libc::O_RDONLY {
            return Err(libc::EROFS.into());
        }
        let entry = self
            .node(ino)?
            .entry
            .ok_or(fuse3::Errno::from(libc::EISDIR))?;
        let d = self.host.upgrade().ok_or(fuse3::Errno::from(libc::EIO))?;
        let fh = d
            .client()
            .lock()
            .await
            .secret_mount_open(&self.id, &entry.id, req.uid, Some(req.pid))
            .map_err(|_| fuse3::Errno::from(libc::EACCES))?;
        Ok(ReplyOpen {
            fh,
            flags: 1, /* FOPEN_DIRECT_IO, Linux uapi fuse.h */
        })
    }

    async fn read(
        &self,
        req: Request,
        ino: u64,
        fh: u64,
        offset: u64,
        size: u32,
    ) -> fuse3::Result<ReplyData> {
        self.owner(req)?;
        if size > 1048576 {
            return Err(libc::E2BIG.into());
        }
        let entry = self
            .node(ino)?
            .entry
            .ok_or(fuse3::Errno::from(libc::EISDIR))?;
        let d = self.host.upgrade().ok_or(fuse3::Errno::from(libc::EIO))?;
        let ticket = d
            .client()
            .lock()
            .await
            .secret_mount_intent(&self.id, &entry.id, entry.generation, req.uid, Some(fh))
            .map_err(|_| fuse3::Errno::from(libc::EACCES))?;
        let cancelled = ticket.cancellation();
        let changed = Arc::new(tokio::sync::Notify::new());
        {
            let mut reads = self.reads.lock().expect("FUSE reads");
            if reads.len() >= 64 {
                return Err(libc::EAGAIN.into());
            }
            reads.insert(
                req.unique,
                PendingRead {
                    handle: fh,
                    cancelled: cancelled.clone(),
                    changed: changed.clone(),
                },
            );
        }
        let _reading = Reading {
            reads: &self.reads,
            unique: req.unique,
        };
        let interrupted = async {
            loop {
                let next = changed.notified();
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
                next.await;
            }
        };
        let outcome = tokio::select! {
            biased;
            _=interrupted=>Err(libc::EINTR.into()),
            result=read_ticket(&d,&self.id,&entry.id,Some(fh),&ticket,(offset,size))=>match result{
                Ok((bytes,_)) if !cancelled.load(Ordering::Acquire)=>Ok(ReplyData{data:bytes::Bytes::from_owner(bytes)}),
                _=>Err(libc::EACCES.into()),
            }
        };
        d.emit("approval/changed", Value::Null);
        outcome
    }

    async fn interrupt(&self, req: Request, unique: u64) -> fuse3::Result<()> {
        self.owner(req)?;
        let reads = self.reads.lock().expect("FUSE reads");
        let read = reads.get(&unique).ok_or(fuse3::Errno::from(libc::EAGAIN))?;
        // This is the same atomic flag the core checks at approval and before plaintext release.
        read.cancelled.store(true, Ordering::Release);
        read.changed.notify_waiters();
        Ok(())
    }

    async fn release(
        &self,
        _req: Request,
        _ino: u64,
        fh: u64,
        _flags: u32,
        _owner: u64,
        _flush: bool,
    ) -> fuse3::Result<()> {
        for read in self
            .reads
            .lock()
            .expect("FUSE reads")
            .values()
            .filter(|r| r.handle == fh)
        {
            read.cancelled.store(true, Ordering::Release);
            read.changed.notify_waiters();
        }
        if let Some(d) = self.host.upgrade() {
            let _ = d.client().lock().await.secret_mount_close(&self.id, fh);
        }
        Ok(())
    }
}

pub(super) async fn start(
    host: Weak<Daemon>,
    id: String,
    config: Config,
    entries: Vec<mounts::Entry>,
) -> Result<MountHandle> {
    let mut nodes = vec![Node {
        ino: 1,
        parent: 1,
        name: String::new(),
        entry: None,
    }];
    let mut dirs = BTreeMap::from([(String::new(), 1u64)]);
    let mut next = 2;
    for entry in entries {
        let parts: Vec<_> = entry.relative_path.split('/').collect();
        let mut parent = 1;
        let mut path = String::new();
        for part in &parts[..parts.len() - 1] {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            parent = if let Some(ino) = dirs.get(&path) {
                *ino
            } else {
                let ino = next;
                next += 1;
                nodes.push(Node {
                    ino,
                    parent,
                    name: (*part).into(),
                    entry: None,
                });
                dirs.insert(path.clone(), ino);
                ino
            };
        }
        nodes.push(Node {
            ino: next,
            parent,
            name: parts.last().expect("path").to_string(),
            entry: Some(entry),
        });
        next += 1;
    }
    let filesystem = FilesystemAdapter {
        host,
        id,
        owner: uid(),
        nodes,
        created: SystemTime::now().into(),
        reads: Mutex::new(BTreeMap::new()),
    };
    let mut options = MountOptions::default();
    options
        .read_only(true)
        .default_permissions(true)
        .fs_name("dmsg-secrets")
        .custom_options("nodev,nosuid,noexec");
    let session = Session::new(options);
    // The helper parses generic nodev/nosuid/noexec mount flags; passing them as
    // FUSE kernel data in a direct mount would be invalid.
    session
        .mount_with_unprivileged(filesystem, &config.input.mount_point)
        .await
        .map_err(|error| Error::Unavailable(format!("FUSE_UNAVAILABLE: {}", error.kind())))
}
