//! Instance detection via /proc filesystem, and the single graphical session.
//!
//! Detects running usit instances by scanning /proc for processes with
//! argv[0] ending in `/usit` or equal to `usit`, then parsing tags
//! from command-line arguments. An untagged Qt session also claims a runtime
//! lock so a second desktop launch shows it instead of starting another engine.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// Information about a running usit instance
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceInfo {
    /// Process ID of the usit instance
    pub pid: u32,
    /// Tag from `--tag` argument, if present
    /// - `None` = no `--tag` argument
    /// - `Some("")` = `--tag` with no value or empty value
    /// - `Some("value")` = `--tag value` or `--tag=value`
    pub tag: Option<String>,
}

/// Find all running usit instances, optionally filtered by tag
///
/// # Arguments
/// * `tag_filter` - Optional tag to filter by (exact match only)
///   - `None` → return all instances
///   - `Some("foo")` → return only instances with `tag == Some("foo")`
///   - `Some("")` → return only instances with `tag == Some("")`
///   - Filter does NOT match `tag == None` (untagged instances)
///
/// # Returns
/// Vector of `InstanceInfo` structs, excluding the current process.
/// Returns empty vec if `/proc` is unavailable or unreadable.
///
/// # Examples
/// ```
/// use usit::instance::find_instances;
/// // Find all usit instances
/// let all = find_instances(None);
///
/// // Find instances with specific tag
/// let tagged = find_instances(Some("my session"));
/// ```
pub fn find_instances(tag_filter: Option<&str>) -> Vec<InstanceInfo> {
    let proc_dir = match fs::read_dir("/proc") {
        Ok(dir) => dir,
        Err(_) => {
            warn_proc_unavailable();
            return Vec::new();
        }
    };

    let self_pid = std::process::id();
    let mut instances = Vec::new();

    for entry in proc_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        let file_name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };

        // Only process numeric directory names (PIDs)
        let pid: u32 = match file_name.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Skip self
        if pid == self_pid {
            continue;
        }

        // Read /proc/{pid}/cmdline
        let cmdline_path = path.join("cmdline");
        let cmdline_bytes = match fs::read(&cmdline_path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        // Parse argv from NUL-separated bytes
        let argv: Vec<&str> = cmdline_bytes
            .split(|&b| b == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();

        if argv.is_empty() {
            continue;
        }

        // Check if argv[0] is usit (ends with /usit or equals usit)
        let argv0 = argv[0];
        let is_usit = argv0.ends_with("/usit") || argv0 == "usit";

        if !is_usit {
            continue;
        }

        // Parse tag from argv
        let tag = parse_tag_from_argv(&argv);

        // Apply tag filter if provided
        if let Some(filter) = tag_filter {
            if tag != Some(filter.to_string()) {
                continue;
            }
        }

        instances.push(InstanceInfo { pid, tag });
    }

    instances
}

/// Find any usit instance with the given tag (excluding self)
///
/// # Arguments
/// * `tag` - The tag to search for (exact match)
///
/// # Returns
/// PID of a usit instance with this tag, or `None` if not found.
///
/// # Examples
/// ```
/// use usit::instance::find_duplicate_tag;
/// if let Some(pid) = find_duplicate_tag("my session") {
///     eprintln!("usit already running with tag 'my session' (PID {})", pid);
/// }
/// ```
pub fn find_duplicate_tag(tag: &str) -> Option<u32> {
    find_instances(Some(tag)).first().map(|info| info.pid)
}

/// Parse `--tag` argument from argv
///
/// Supports both `--tag value` and `--tag=value` formats.
/// Last occurrence wins if multiple `--tag` arguments present.
///
/// # Arguments
/// * `argv` - Command-line arguments (typically from /proc/{pid}/cmdline)
///
/// # Returns
/// - `None` if no `--tag` argument found
/// - `Some("")` if `--tag` with no value or empty value
/// - `Some("value")` if `--tag value` or `--tag=value`
///
/// # Examples
/// ```
/// use usit::instance::parse_tag_from_argv;
/// assert_eq!(parse_tag_from_argv(&["usit", "--demo"]), None);
/// assert_eq!(parse_tag_from_argv(&["usit", "--tag", "foo"]), Some("foo".to_string()));
/// assert_eq!(parse_tag_from_argv(&["usit", "--tag=foo"]), Some("foo".to_string()));
/// assert_eq!(parse_tag_from_argv(&["usit", "--tag"]), Some("".to_string()));
/// ```
pub fn parse_tag_from_argv(argv: &[&str]) -> Option<String> {
    let mut tag: Option<String> = None;

    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i];

        if arg == "--tag" {
            // Format: --tag value
            if i + 1 < argv.len() {
                tag = Some(argv[i + 1].to_string());
                i += 2;
            } else {
                // Trailing --tag with no value
                tag = Some("".to_string());
                i += 1;
            }
        } else if arg.starts_with("--tag=") {
            // Format: --tag=value
            let value = arg.strip_prefix("--tag=").unwrap_or("");
            tag = Some(value.to_string());
            i += 1;
        } else {
            i += 1;
        }
    }

    tag
}

/// Result of claiming the untagged graphical session.
pub enum SessionClaim {
    /// This process is the session; later launches are counted as activations.
    Primary(Session),
    /// The running session was asked to show itself.
    Forwarded,
}

/// Held for the life of the untagged graphical session.
///
/// The kernel releases the advisory lock when its holder exits, so a crash never
/// leaves usit unlaunchable. A socket beside the lock carries activation requests.
pub struct Session {
    socket: PathBuf,
    activations: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    _lock: File,
}

impl Session {
    /// Count of activation requests from later launches.
    pub fn activations(&self) -> Arc<AtomicU64> {
        self.activations.clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        // Wake the blocking accept so the listener observes the stop flag. If the
        // socket was removed underneath us, leave the listener to process exit.
        if UnixStream::connect(&self.socket).is_ok() {
            if let Some(listener) = self.listener.take() {
                let _ = listener.join();
            }
        }
        let _ = fs::remove_file(&self.socket);
    }
}

/// Become the graphical session in `directory`, or ask the running one to show itself.
pub fn claim_session(directory: &Path) -> Result<SessionClaim> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("session.lock"))
        .context("opening usit session lock")?;
    let socket = directory.join("session.sock");
    // A holder binds its socket just after locking and removes it just before
    // unlocking, and a forked child briefly shares its lock. Retry both ways.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => {
                if forward_activation(&socket).is_ok() {
                    return Ok(SessionClaim::Forwarded);
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "usit is already running but did not respond to this launch"
                );
                thread::sleep(Duration::from_millis(50));
            }
            Err(TryLockError::Error(error)) => {
                return Err(error).context("locking usit session");
            }
        }
    }
    // Holding the lock makes any existing socket a leftover from a dead holder.
    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).context("binding usit session socket")?;
    let activations = Arc::new(AtomicU64::new(0));
    let stopping = Arc::new(AtomicBool::new(false));
    let counter = activations.clone();
    let stop = stopping.clone();
    let listener = thread::spawn(move || {
        for stream in listener.incoming() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let mut request = String::new();
            if BufReader::new(&stream).read_line(&mut request).is_ok()
                && request.trim() == "activate"
            {
                counter.fetch_add(1, Ordering::Relaxed);
                let _ = stream.write_all(b"ok\n");
            }
        }
    });
    Ok(SessionClaim::Primary(Session {
        socket,
        activations,
        stopping,
        listener: Some(listener),
        _lock: lock,
    }))
}

fn forward_activation(socket: &Path) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(b"activate\n")?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    if reply.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::ErrorKind::ConnectionAborted.into())
    }
}

/// Warn once if /proc is unavailable
///
/// Uses `std::sync::Once` to ensure warning is printed only once per process.
fn warn_proc_unavailable() {
    static PROC_WARN_ONCE: Once = Once::new();
    PROC_WARN_ONCE.call_once(|| {
        log::warn!("/proc unavailable, instance detection disabled");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_launches_activate_the_session_until_it_ends() {
        let directory = tempfile::tempdir().unwrap();
        let SessionClaim::Primary(session) = claim_session(directory.path()).unwrap() else {
            panic!("first launch must own the session");
        };
        for expected in 1..=2 {
            assert!(matches!(
                claim_session(directory.path()).unwrap(),
                SessionClaim::Forwarded
            ));
            assert_eq!(session.activations().load(Ordering::Relaxed), expected);
        }
        drop(session);
        assert!(!directory.path().join("session.sock").exists());
        assert!(matches!(
            claim_session(directory.path()).unwrap(),
            SessionClaim::Primary(_)
        ));
    }

    #[test]
    fn shutdown_does_not_wait_on_a_removed_socket() {
        let directory = tempfile::tempdir().unwrap();
        let SessionClaim::Primary(session) = claim_session(directory.path()).unwrap() else {
            panic!("first launch must own the session");
        };
        fs::remove_file(directory.path().join("session.sock")).unwrap();
        let began = Instant::now();
        drop(session);
        assert!(began.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn leftover_socket_from_dead_session_is_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let stale = UnixListener::bind(directory.path().join("session.sock")).unwrap();
        drop(stale);
        let SessionClaim::Primary(session) = claim_session(directory.path()).unwrap() else {
            panic!("an unlocked session must be claimable");
        };
        assert!(matches!(
            claim_session(directory.path()).unwrap(),
            SessionClaim::Forwarded
        ));
        assert_eq!(session.activations().load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_parse_tag_from_argv_no_tag() {
        assert_eq!(parse_tag_from_argv(&["usit", "--demo"]), None);
    }

    #[test]
    fn test_parse_tag_from_argv_with_value() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag", "foo", "--demo"]),
            Some("foo".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_equals_format() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag=foo", "--demo"]),
            Some("foo".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_with_spaces() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag", "my session"]),
            Some("my session".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_empty_value() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag", ""]),
            Some("".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_trailing_tag() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag"]),
            Some("".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_last_wins() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag", "a", "--tag", "b"]),
            Some("b".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_with_equals() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag=with=equals"]),
            Some("with=equals".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_unicode() {
        assert_eq!(
            parse_tag_from_argv(&["usit", "--tag", "unicode: 日本語"]),
            Some("unicode: 日本語".to_string())
        );
    }
}
