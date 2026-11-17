//! Forward only allowlisted failure categories and message timings to the native console.

use dmsg_client::diagnostic::{Diagnostic, MessageTiming, TARGET, TIMING_TARGET};
use std::sync::Once;

struct Diagnostics;

fn emit(line: String) {
    #[cfg(target_os = "android")]
    {
        #[link(name = "log")]
        unsafe extern "C" {
            fn __android_log_write(
                priority: i32,
                tag: *const std::ffi::c_char,
                text: *const std::ffi::c_char,
            ) -> i32;
        }
        // Only already decoded, fixed diagnostic labels reach this function.
        if let Ok(line) = std::ffi::CString::new(line) {
            unsafe {
                __android_log_write(4, c"dMsg".as_ptr(), line.as_ptr());
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("{line}");
}

impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        (metadata.target() == TARGET && metadata.level() <= log::Level::Warn)
            || (metadata.target() == TIMING_TARGET && metadata.level() <= log::Level::Info)
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata())
            && let Some(diagnostic) = Diagnostic::decode(&record.args().to_string())
        {
            emit(format!(
                "dmsg: {} {} {}",
                diagnostic.source,
                diagnostic.reason,
                diagnostic.http_status.unwrap_or(0)
            ));
        }
        if record.target() == TIMING_TARGET
            && let Some((phase, millis)) = MessageTiming::decode(&record.args().to_string())
        {
            emit(format!("dmsg: message_timing {phase} {millis}"));
        }
    }

    fn flush(&self) {}
}

pub(crate) fn init() {
    static INIT: Once = Once::new();
    static DIAGNOSTICS: Diagnostics = Diagnostics;
    INIT.call_once(|| {
        if log::set_logger(&DIAGNOSTICS).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    });
}
