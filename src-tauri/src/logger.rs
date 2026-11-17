//! Minimal stderr logger. Message content, phone numbers and keys are never
//! logged by Notchat; dependencies are capped at `warn` to keep it that way.

use log::{Level, LevelFilter, Log, Metadata, Record};

struct Stderr;

impl Log for Stderr {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let ours = metadata.target().starts_with("notchat");
        metadata.level() <= if ours { Level::Info } else { Level::Warn }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    if log::set_logger(&Stderr).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}
