//! Fcitx5 bridge backend for text injection via D-Bus
//!
//! Uses the fcitx5-usit-bridge addon to inject text through fcitx5's
//! input method protocol. This allows text injection on compositors
//! (like KDE) where fcitx5 holds the input method protocol.
//!
//! Backend discovery is passive: desktop input-method lifecycle belongs to KDE.

use anyhow::{anyhow, Result};
use zbus::blocking::Connection;

use super::TextInjector;

const FCITX5_BUS_NAME: &str = "org.fcitx.Fcitx5";
const USIT_BRIDGE_PATH: &str = "/usitbridge";
const USIT_BRIDGE_INTERFACE: &str = "org.fcitx.Fcitx5.UsitBridge1";

pub struct Fcitx5BridgeInjector {
    connection: Connection,
}

impl Fcitx5BridgeInjector {
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

    pub fn new() -> Result<Self> {
        let connection =
            Connection::session().map_err(|e| anyhow!("D-Bus session connection failed: {}", e))?;

        // First try: maybe addon is already loaded
        if Self::addon_is_loaded(&connection) {
            log::debug!("fcitx5_bridge: addon already loaded");
            return Ok(Self { connection });
        }

        // Addon not loaded - check if fcitx5 is running
        if !Self::fcitx5_is_running(&connection) {
            return Err(anyhow!("fcitx5 not running"));
        }

        Err(anyhow!(
            "usit bridge addon is not loaded; install it with scripts/install-fcitx-bridge.sh and restart Fcitx through the desktop"
        ))
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
