//! Fcitx5 bridge backend for text injection via D-Bus
//!
//! Uses the fcitx5-usit-bridge addon to inject text through fcitx5's
//! input method protocol. This allows text injection on compositors
//! (like KDE) where fcitx5 holds the input method protocol.
//!
//! During development, usit can register the addon by writing a user-local
//! `usitbridge.conf` whose `Library=` points at an absolute build artifact
//! path. Packaged installs should place the addon in fcitx5's normal system
//! addon directories instead.

use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use zbus::blocking::Connection;

use super::TextInjector;

const FCITX5_BUS_NAME: &str = "org.fcitx.Fcitx5";
const USIT_BRIDGE_PATH: &str = "/rocks/gmt/usit/FcitxBridge1";
const USIT_BRIDGE_INTERFACE: &str = "rocks.gmt.UsitFcitxBridge1";
const USIT_BRIDGE_LIBRARY_FILE: &str = "libusitbridge.so";

pub struct Fcitx5BridgeInjector {
    connection: Connection,
}

impl Fcitx5BridgeInjector {
    fn connect_passive() -> Result<Self> {
        let local_install = Self::ensure_local_addon_config()?;
        let connection =
            Connection::session().map_err(|e| anyhow!("D-Bus session connection failed: {}", e))?;

        if Self::addon_is_loaded(&connection) {
            log::debug!("fcitx5_bridge: addon already loaded");
            return Ok(Self { connection });
        }

        if !Self::fcitx5_is_running(&connection) {
            return Err(anyhow!("fcitx5 not running"));
        }

        if local_install.is_none() {
            return Err(anyhow!(
                "addon not registered; build fcitx5-usit-bridge or install the packaged addon"
            ));
        }

        Err(anyhow!(
            "addon registered but not loaded; reload fcitx5 or bounce the Plasma virtual keyboard"
        ))
    }

    fn addon_conf_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))?;
        Ok(home.join(".local/share/fcitx5/addon/usitbridge.conf"))
    }

    fn candidate_addon_library_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(path) = std::env::var("USIT_FCITX5_BRIDGE_LIB") {
            paths.push(PathBuf::from(path));
        }

        if let Some(home) = dirs::home_dir() {
            paths.push(
                home.join(".local/lib/fcitx5")
                    .join(USIT_BRIDGE_LIBRARY_FILE),
            );
        }

        paths.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fcitx5-usit-bridge/build/src")
                .join(USIT_BRIDGE_LIBRARY_FILE),
        );

        paths
    }

    fn addon_library_path() -> Option<PathBuf> {
        Self::candidate_addon_library_paths()
            .into_iter()
            .find(|path| path.exists())
    }

    fn addon_library_conf_value(path: &std::path::Path) -> PathBuf {
        if path.extension().and_then(|ext| ext.to_str()) == Some("so") {
            path.with_extension("")
        } else {
            path.to_path_buf()
        }
    }

    fn addon_conf_contents(path: &std::path::Path) -> String {
        let library = Self::addon_library_conf_value(path);
        format!(
            "[Addon]\n\
             Name=Usit Bridge\n\
             Name[en]=Usit Bridge\n\
             Category=Module\n\
             Library={}\n\
             Type=SharedLibrary\n\
             OnDemand=False\n\
             Configurable=False\n\
             \n\
             [Addon/Dependencies]\n\
             0=dbus\n",
            library.display()
        )
    }

    /// Ensure the user-local addon metadata points at the best available bridge library.
    fn ensure_local_addon_config() -> Result<Option<(PathBuf, PathBuf)>> {
        let Some(lib_path) = Self::addon_library_path() else {
            return Ok(None);
        };

        let conf_path = Self::addon_conf_path()?;
        let conf = Self::addon_conf_contents(&lib_path);

        if let Some(parent) = conf_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow!("failed to create {}: {}", parent.display(), e))?;
        }

        let current = fs::read_to_string(&conf_path).ok();
        if current.as_deref() != Some(conf.as_str()) {
            fs::write(&conf_path, conf)
                .map_err(|e| anyhow!("failed to write {}: {}", conf_path.display(), e))?;
            log::info!(
                "fcitx5_bridge: registered addon metadata at {}",
                conf_path.display()
            );
        }

        Ok(Some((lib_path, conf_path)))
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

    /// Ask fcitx5 to reload its addon set after local metadata changes.
    fn reload_fcitx5() -> Result<()> {
        log::info!("fcitx5_bridge: reloading fcitx5");
        Command::new("fcitx5")
            .arg("-r")
            .spawn()
            .map_err(|e| anyhow!("failed to run fcitx5 -r: {}", e))?;

        thread::sleep(Duration::from_secs(2));
        Ok(())
    }

    pub fn new() -> Result<Self> {
        if let Ok(injector) = Self::connect_passive() {
            return Ok(injector);
        }

        // If a local config is available, ask fcitx5 to reload its addon set.
        if Self::ensure_local_addon_config()?.is_none() {
            return Err(anyhow!(
                "addon not registered; build fcitx5-usit-bridge or install the packaged addon"
            ));
        }

        Self::reload_fcitx5()?;

        let connection =
            Connection::session().map_err(|e| anyhow!("D-Bus reconnect failed: {}", e))?;

        if Self::addon_is_loaded(&connection) {
            log::info!("fcitx5_bridge: addon loaded after reload");
            Ok(Self { connection })
        } else {
            Err(anyhow!("addon still not loaded after fcitx5 reload"))
        }
    }

    pub fn new_passive() -> Result<Self> {
        Self::connect_passive()
    }
}

impl TextInjector for Fcitx5BridgeInjector {
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

#[cfg(test)]
mod tests {
    use super::Fcitx5BridgeInjector;
    use std::path::Path;

    #[test]
    fn addon_conf_uses_absolute_library_path_without_suffix() {
        let conf = Fcitx5BridgeInjector::addon_conf_contents(Path::new(
            "/tmp/usit-build/libusitbridge.so",
        ));
        assert!(conf.contains("Library=/tmp/usit-build/libusitbridge\n"));
        assert!(!conf.contains("Library=/tmp/usit-build/libusitbridge.so\n"));
    }
}
