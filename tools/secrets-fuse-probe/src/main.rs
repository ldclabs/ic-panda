//! Synthetic Linux gate, independent of Vault or any daemon credential.
use fuser::{FileAttr,FileType,Filesystem,MountOption,ReplyAttr,ReplyData,ReplyDirectory,ReplyEmpty,ReplyEntry,ReplyOpen,Request};
use std::{collections::BTreeMap,ffi::OsStr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant,UNIX_EPOCH}};
struct Handle {active:AtomicBool,deadline:Mutex<Option<Instant>>}
struct Probe {uid:u32,next:u64,handles:BTreeMap<u64,Arc<Handle>>,bytes:Arc<Vec<u8>>}
impl Probe {
    fn attr(&self,ino:u64)->FileAttr {FileAttr{ino,size:if ino==1{0}else{self.bytes.len() as u64},blocks:0,atime:UNIX_EPOCH,mtime:UNIX_EPOCH,ctime:UNIX_EPOCH,crtime:UNIX_EPOCH,kind:if ino==1{FileType::Directory}else{FileType::RegularFile},perm:if ino==1{0o500}else{0o400},nlink:1,uid:self.uid,gid:unsafe{libc::getegid()},rdev:0,blksize:4096,flags:0}}
}
impl Filesystem for Probe {
    fn lookup(&mut self,req:&Request<'_>,parent:u64,name:&OsStr,reply:ReplyEntry) {
        if req.uid()!=self.uid {reply.error(libc::EACCES)} else if parent==1 && name=="fixture.env" {reply.entry(&Duration::ZERO,&self.attr(2),0)}else{reply.error(libc::ENOENT)}
    }

    fn getattr(&mut self,_req:&Request<'_>,ino:u64,_fh:Option<u64>,reply:ReplyAttr) {if (1..=2).contains(&ino){reply.attr(&Duration::ZERO,&self.attr(ino))}else{reply.error(libc::ENOENT)}}
    fn readdir(&mut self,req:&Request<'_>,ino:u64,_fh:u64,offset:i64,mut reply:ReplyDirectory) {
        if req.uid()!=self.uid || ino!=1 {return reply.error(libc::EACCES)}
        for (i,(ino,kind,name)) in [(1,FileType::Directory,"."),(1,FileType::Directory,".."),(2,FileType::RegularFile,"fixture.env")].into_iter().enumerate().skip(offset.max(0) as usize) {if reply.add(ino,(i+1) as i64,kind,name){break}}
        reply.ok()
    }

    fn open(&mut self,req:&Request<'_>,ino:u64,flags:i32,reply:ReplyOpen) {
        if req.uid()!=self.uid || ino!=2 || flags&libc::O_ACCMODE!=libc::O_RDONLY {return reply.error(libc::EACCES)}
        if self.handles.len()>=64{return reply.error(libc::EMFILE)}
        self.next+=1;self.handles.insert(self.next,Arc::new(Handle{active:AtomicBool::new(true),deadline:Mutex::new(None)}));
        reply.opened(self.next,fuser::consts::FOPEN_DIRECT_IO);
    }

    fn read(&mut self,req:&Request<'_>,ino:u64,fh:u64,offset:i64,size:u32,_flags:i32,_owner:Option<u64>,reply:ReplyData) {
        let Some(handle)=self.handles.get(&fh).cloned() else{return reply.error(libc::EBADF)};
        if req.uid()!=self.uid || ino!=2 || offset<0 || size>1048576{return reply.error(libc::EACCES)}
        let bytes=self.bytes.clone();
        std::thread::spawn(move || {
            let waiting=handle.deadline.lock().unwrap().is_none();
            if waiting {std::thread::sleep(Duration::from_secs(5));}
            if !handle.active.load(Ordering::SeqCst){return reply.error(libc::EACCES)}
            let mut deadline=handle.deadline.lock().unwrap();let end=*deadline.get_or_insert_with(||Instant::now()+Duration::from_secs(3));
            if Instant::now()>=end{return reply.error(libc::EACCES)}
            let start=(offset as usize).min(bytes.len());let end=(start+size as usize).min(bytes.len());reply.data(&bytes[start..end]);
        });
    }

    fn release(&mut self,_req:&Request<'_>,_ino:u64,fh:u64,_flags:i32,_lock_owner:Option<u64>,_flush:bool,reply:ReplyEmpty) {
        if let Some(handle)=self.handles.remove(&fh){handle.active.store(false,Ordering::SeqCst)}reply.ok()
    }
}

fn main(){
    let point=std::env::args().nth(1).expect("mount point");
    let bytes=format!("DMSG_PROBE='synthetic-only'\n{}","#fixture\n".repeat(150000)).into_bytes();
    fuser::mount2(Probe{uid:unsafe{libc::geteuid()},next:0,handles:BTreeMap::new(),bytes:Arc::new(bytes)},point,&[MountOption::RO,MountOption::DefaultPermissions,MountOption::AllowOther,MountOption::NoDev,MountOption::NoSuid,MountOption::NoExec,MountOption::FSName("dmsg-secret-probe".into())]).unwrap();
}
