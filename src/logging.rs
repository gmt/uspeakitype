//! Logging module with journald integration
//!
//! Routes diagnostic messages to systemd-journal when available,
//! falls back to stderr with TUI-aware filtering.

use log::LevelFilter;

#[cfg(target_os = "linux")]
use systemd_journal_logger::{connected_to_journal, JournalLog};

pub fn init(is_tui: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    if connected_to_journal() {
        JournalLog::new()?
            .with_syslog_identifier("usit".to_string())
            .with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))])
            .install()?;
        log::set_max_level(LevelFilter::Info);
        return Ok(());
    }

    // Fallback: stderr logging
    // In TUI mode, only show errors (don't pollute display)
    // In non-TUI mode, show info+
    let level = if is_tui {
        LevelFilter::Error
    } else {
        LevelFilter::Info
    };
    env_logger::Builder::new()
        .filter_level(level)
        .format_timestamp(None)
        .init();

    Ok(())
}
