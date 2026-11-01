//! Forward only allowlisted failure categories and message timings to the native console.

use dmsg_client::diagnostic::{Diagnostic, MessageTiming, TARGET, TIMING_TARGET};
use std::sync::Once;

struct Diagnostics;

impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        (metadata.target() == TARGET && metadata.level() <= log::Level::Warn)
            || (metadata.target() == TIMING_TARGET && metadata.level() <= log::Level::Info)
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata())
            && let Some(diagnostic) = Diagnostic::decode(&record.args().to_string())
        {
            eprintln!(
                "dmsg: {} {} {}",
                diagnostic.source,
                diagnostic.reason,
                diagnostic.http_status.unwrap_or(0)
            );
        }
        if record.target() == TIMING_TARGET
            && let Some((phase, millis)) = MessageTiming::decode(&record.args().to_string())
        {
            eprintln!("dmsg: message_timing {phase} {millis}");
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
