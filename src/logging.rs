//! Logging - routes all output to journald on Linux, keeping console clean.

use log::LevelFilter;

#[cfg(target_os = "linux")]
use systemd_journal_logger::JournalLog;

pub fn init(_is_tui: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        JournalLog::new()?
            .with_syslog_identifier("usit".to_string())
            .with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))])
            .install()?;
        log::set_max_level(LevelFilter::Info);
    }

    #[cfg(not(target_os = "linux"))]
    {
        env_logger::Builder::new()
            .filter_level(LevelFilter::Error)
            .parse_default_env()
            .format_timestamp(None)
            .init();
    }

    Ok(())
}
