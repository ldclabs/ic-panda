//! Forward only the core's allowlisted failure categories to the native console.

use dmsg_client::diagnostic::{Diagnostic, TARGET};
use std::sync::Once;

struct Diagnostics;

impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == TARGET && metadata.level() <= log::Level::Warn
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
    }

    fn flush(&self) {}
}

pub(crate) fn init() {
    static INIT: Once = Once::new();
    static DIAGNOSTICS: Diagnostics = Diagnostics;
    INIT.call_once(|| {
        if log::set_logger(&DIAGNOSTICS).is_ok() {
            log::set_max_level(log::LevelFilter::Warn);
        }
    });
}
