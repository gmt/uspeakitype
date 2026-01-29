//! Fcitx5 bridge backend for text injection via D-Bus
//!
//! Uses the fcitx5-usit-bridge addon to inject text through fcitx5's
//! input method protocol. This allows text injection on compositors
//! (like KDE) where fcitx5 holds the input method protocol.
//!
//! If the addon is installed in ~/.local but not loaded, this backend
//! will automatically restart fcitx5 with the correct FCITX_ADDON_DIRS.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use zbus::blocking::Connection;

use super::TextInjector;

const FCITX5_BUS_NAME: &str = "org.fcitx.Fcitx5";
const USIT_BRIDGE_PATH: &str = "/usitbridge";
const USIT_BRIDGE_INTERFACE: &str = "org.fcitx.Fcitx5.UsitBridge1";

pub struct Fcitx5BridgeInjector {
    connection: Connection,
}

impl Fcitx5BridgeInjector {
    /// Check if our addon files are installed in ~/.local
    fn addon_files_exist() -> Option<(PathBuf, PathBuf)> {
        let home = dirs::home_dir()?;
        let lib_path = home.join(".local/lib/fcitx5/libusitbridge.so");
        let conf_path = home.join(".local/share/fcitx5/addon/usitbridge.conf");

        if lib_path.exists() && conf_path.exists() {
            Some((lib_path, conf_path))
        } else {
            None
        }
    }

    /// Check if fcitx5 D-Bus service is available
    fn fcitx5_is_running(connection: &Connection) -> bool {
        connection
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &FCITX5_BUS_NAME,
            )
            .ok()
            .and_then(|reply| reply.body().deserialize::<bool>().ok())
            .unwrap_or(false)
    }

    /// Check if our addon is responding
    fn addon_is_loaded(connection: &Connection) -> bool {
        connection
            .call_method(
                Some(FCITX5_BUS_NAME),
                USIT_BRIDGE_PATH,
                Some(USIT_BRIDGE_INTERFACE),
                "IsActive",
                &(),
            )
            .is_ok()
    }

    /// Restart fcitx5 with FCITX_ADDON_DIRS pointing to ~/.local
    fn restart_fcitx5_with_addon() -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))?;
        let local_addon_dir = home.join(".local/lib/fcitx5");

        // Build addon dirs: user local first, then system
        let addon_dirs = format!("{}:/usr/lib/fcitx5", local_addon_dir.display());

        log::info!("Restarting fcitx5 with FCITX_ADDON_DIRS={}", addon_dirs);

        // Kill existing fcitx5
        let _ = Command::new("pkill").args(["-x", "fcitx5"]).status();

        // Wait for it to die
        thread::sleep(Duration::from_millis(500));

        // Start fcitx5 with our addon path
        Command::new("fcitx5")
            .env("FCITX_ADDON_DIRS", &addon_dirs)
            .arg("-d") // daemonize
            .spawn()
            .map_err(|e| anyhow!("failed to start fcitx5: {}", e))?;

        // Wait for it to start and register on D-Bus
        thread::sleep(Duration::from_secs(2));

        Ok(())
    }

    pub fn new() -> Result<Self> {
        let connection = Connection::session()
            .map_err(|e| anyhow!("D-Bus session connection failed: {}", e))?;

        // First try: maybe addon is already loaded
        if Self::addon_is_loaded(&connection) {
            log::debug!("fcitx5_bridge: addon already loaded");
            return Ok(Self { connection });
        }

        // Addon not loaded - check if fcitx5 is running
        if !Self::fcitx5_is_running(&connection) {
            return Err(anyhow!("fcitx5 not running"));
        }

        // Fcitx5 running but addon not loaded - can we fix it?
        if Self::addon_files_exist().is_none() {
            return Err(anyhow!(
                "addon not loaded; install to ~/.local/lib/fcitx5/ and ~/.local/share/fcitx5/addon/"
            ));
        }

        // Addon files exist! Restart fcitx5 with the right paths
        log::info!("fcitx5_bridge: addon files found, restarting fcitx5...");
        Self::restart_fcitx5_with_addon()?;

        // Reconnect after restart
        let connection = Connection::session()
            .map_err(|e| anyhow!("D-Bus reconnect failed: {}", e))?;

        // Verify addon is now loaded
        if Self::addon_is_loaded(&connection) {
            log::info!("fcitx5_bridge: addon loaded after restart");
            Ok(Self { connection })
        } else {
            Err(anyhow!("addon still not loaded after fcitx5 restart"))
        }
    }
}

impl TextInjector for Fcitx5BridgeInjector {
    fn name(&self) -> &'static str {
        "fcitx5_bridge"
    }

    fn inject(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        // Add trailing space like other backends
        let text_with_space = format!("{} ", text);

        self.connection
            .call_method(
                Some(FCITX5_BUS_NAME),
                USIT_BRIDGE_PATH,
                Some(USIT_BRIDGE_INTERFACE),
                "CommitString",
                &text_with_space,
            )
            .map_err(|e| anyhow!("CommitString failed: {}", e))?;

        Ok(())
    }
}
