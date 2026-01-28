//! Logging - routes all output to journald on Linux, keeping console clean.
//! Falls back to env_logger when journald is unavailable (e.g., Docker).

use log::LevelFilter;

#[cfg(target_os = "linux")]
use systemd_journal_logger::JournalLog;

pub fn init(_is_tui: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Try journald first, fall back to env_logger if unavailable (e.g., Docker)
        let journal_result = JournalLog::new()
            .map(|j| {
                j.with_syslog_identifier("usit".to_string())
                    .with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))])
                    .install()
            });

        match journal_result {
            Ok(Ok(())) => {
                log::set_max_level(LevelFilter::Info);
            }
            _ => {
                // Journald unavailable, use env_logger
                env_logger::Builder::new()
                    .filter_level(LevelFilter::Info)
                    .parse_default_env()
                    .format_timestamp(None)
                    .init();
            }
        }
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
