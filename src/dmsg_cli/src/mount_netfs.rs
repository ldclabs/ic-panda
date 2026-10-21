//! The macOS mount uses NetFS in a short-lived child of the daemon. The
//! request travels over stdin, so credentials never enter argv or a file.
//! Keeping the synchronous API in a child lets the daemon enforce a timeout.

use crate::remote::Failure;
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, TCFType, kCFAllocatorDefault};
use core_foundation::boolean::kCFBooleanTrue;
use core_foundation::dictionary::{
    CFDictionaryCreateMutable, CFDictionarySetValue, CFMutableDictionaryRef,
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLCreateWithString, CFURLRef};
use dmsg_types::{Error, Result};
use serde_json::{Value, json};
use std::ffi::{CStr, c_int, c_void};
use std::io::Read;
use std::ptr;

/// `EEXIST`: a volume of the name is mounted already.
const EEXIST: c_int = 17;

/// Read the kernel's mount table without touching the WebDAV filesystem.
/// A leftover directory is not a mount, and an unresponsive disk must not
/// block the daemon's status checks.
pub(crate) fn is_mounted(point: &str) -> Result<bool> {
    let error = || Error::Unavailable(format!("mount: {}", std::io::Error::last_os_error()));
    // SAFETY: a null buffer requests only the number of mounted filesystems.
    let mut count = unsafe { libc::getfsstat(ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count < 0 {
        return Err(error());
    }
    loop {
        // Leave room to detect a mount added between the two calls.
        // SAFETY: statfs contains only integers and fixed-size byte arrays.
        let mut mounts = vec![unsafe { std::mem::zeroed::<libc::statfs>() }; count as usize + 1];
        let size = c_int::try_from(std::mem::size_of_val(mounts.as_slice()))
            .map_err(|_| Error::Unavailable("mount: mount table is too large".into()))?;
        // SAFETY: the buffer is writable for `size` bytes and owned by this call.
        count = unsafe { libc::getfsstat(mounts.as_mut_ptr(), size, libc::MNT_NOWAIT) };
        if count < 0 {
            return Err(error());
        }
        if count as usize == mounts.len() {
            continue;
        }
        return Ok(mounts[..count as usize].iter().any(|mount| {
            // SAFETY: getfsstat returns a NUL-terminated mount-point name.
            unsafe { CStr::from_ptr(mount.f_mntonname.as_ptr()) }.to_bytes() == point.as_bytes()
        }));
    }
}

#[link(name = "NetFS", kind = "framework")]
unsafe extern "C" {
    fn NetFSMountURLSync(
        url: CFURLRef,
        mountpath: CFURLRef,
        user: CFStringRef,
        passwd: CFStringRef,
        open_options: CFMutableDictionaryRef,
        mount_options: CFMutableDictionaryRef,
        mountpoints: *mut CFArrayRef,
    ) -> c_int;
}

/// Mount `url` as `user` with `password`; returns where it mounted.
pub(crate) fn mount(url: &str, user: &str, password: &str) -> Result<String> {
    let string = CFString::new(url);
    let cf_url = unsafe {
        CFURLCreateWithString(
            kCFAllocatorDefault,
            string.as_concrete_TypeRef(),
            ptr::null(),
        )
    };
    if cf_url.is_null() {
        return Err(Error::Unavailable(
            "mount: the share URL is not valid".into(),
        ));
    }
    let cf_url = unsafe { CFURL::wrap_under_create_rule(cf_url) };
    let user = CFString::new(user);
    let password = CFString::new(password);
    // The NetFS option keys in <NetFS/NetFS.h>: a loopback mount is allowed,
    // and no authentication dialog may appear.
    let allow_loopback = CFString::new("AllowLoopback");
    let ui_option = CFString::new("UIOption");
    let no_ui = CFString::new("NoUI");
    let options = unsafe {
        CFDictionaryCreateMutable(
            kCFAllocatorDefault,
            0,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    };
    if options.is_null() {
        return Err(Error::Unavailable(
            "mount: no memory for the mount options".into(),
        ));
    }
    let mut mounts: CFArrayRef = ptr::null();
    let status = unsafe {
        CFDictionarySetValue(
            options,
            allow_loopback.as_concrete_TypeRef().cast::<c_void>(),
            kCFBooleanTrue.cast::<c_void>(),
        );
        CFDictionarySetValue(
            options,
            ui_option.as_concrete_TypeRef().cast::<c_void>(),
            no_ui.as_concrete_TypeRef().cast::<c_void>(),
        );
        NetFSMountURLSync(
            cf_url.as_concrete_TypeRef(),
            ptr::null(),
            user.as_concrete_TypeRef(),
            password.as_concrete_TypeRef(),
            options,
            ptr::null_mut(),
            &mut mounts,
        )
    };
    unsafe { CFRelease(options.cast()) };
    if status != 0 {
        if !mounts.is_null() {
            unsafe { CFRelease(mounts.cast()) };
        }
        return Err(match status {
            EEXIST => Error::Unavailable("mount: the disk name is mounted already".into()),
            status => Error::Unavailable(format!("mount: NetFS status {status}")),
        });
    }
    let point = if mounts.is_null() {
        None
    } else {
        let mounts = unsafe { CFArray::<CFString>::wrap_under_create_rule(mounts) };
        mounts.get(0).map(|point| point.to_string())
    };
    point.ok_or_else(|| Error::Unavailable("mount: NetFS returned no mount point".into()))
}

/// Run NetFS in a child whose lifetime is owned by the daemon.
pub(crate) async fn helper_mount(url: &str, password: &str) -> Result<String> {
    let exe =
        std::env::current_exe().map_err(|error| Error::Unavailable(format!("mount: {error}")))?;
    let request = json!({"url": url, "password": password}).to_string();
    let output = crate::mount::run(
        tokio::process::Command::new(exe).args(["--json", "mount-netfs"]),
        Some(&request),
    )
    .await?;
    serde_json::from_str::<Value>(&output)
        .ok()
        .and_then(|value| {
            value["point"]
                .as_str()
                .filter(|point| !point.is_empty())
                .map(str::to_string)
        })
        .ok_or_else(|| Error::Unavailable("mount: NetFS returned no mount point".into()))
}

/// The internal `dmsg mount-netfs` command: one mount request on stdin,
/// the mount point as JSON on stdout.
pub(crate) fn from_stdin() -> std::result::Result<(), Failure> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| Failure::new("UNAVAILABLE", format!("mount: {e}")))?;
    let request: Value = serde_json::from_str(&text)
        .map_err(|e| Failure::new("INVALID_INPUT", format!("mount: {e}")))?;
    let url = request["url"]
        .as_str()
        .ok_or_else(|| Failure::new("INVALID_INPUT", "mount: url"))?;
    let password = request["password"]
        .as_str()
        .ok_or_else(|| Failure::new("INVALID_INPUT", "mount: password"))?;
    let point = mount(url, crate::mount::USER, password)
        .map_err(|e| Failure::new("UNAVAILABLE", e.to_string()))?;
    println!("{}", json!({"point": point}));
    Ok(())
}
