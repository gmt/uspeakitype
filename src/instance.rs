//! Instance detection via /proc filesystem
//!
//! Detects running Barbara instances by scanning /proc for processes with
//! argv[0] ending in `/barbara` or equal to `barbara`, then parsing tags
//! from command-line arguments.
//!
//! Key insight: VAD is for COMMIT DETECTION, not batching.

use std::fs;
use std::sync::Once;

/// Information about a running Barbara instance
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceInfo {
    /// Process ID of the Barbara instance
    pub pid: u32,
    /// Tag from `--tag` argument, if present
    /// - `None` = no `--tag` argument
    /// - `Some("")` = `--tag` with no value or empty value
    /// - `Some("value")` = `--tag value` or `--tag=value`
    pub tag: Option<String>,
}

/// Find all running Barbara instances, optionally filtered by tag
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
/// ```ignore
/// // Find all Barbara instances
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

        // Check if argv[0] is barbara (ends with /barbara or equals barbara)
        let argv0 = argv[0];
        let is_barbara = argv0.ends_with("/barbara") || argv0 == "barbara";

        if !is_barbara {
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

/// Find any Barbara instance with the given tag (excluding self)
///
/// # Arguments
/// * `tag` - The tag to search for (exact match)
///
/// # Returns
/// PID of a Barbara instance with this tag, or `None` if not found.
///
/// # Examples
/// ```ignore
/// if let Some(pid) = find_duplicate_tag("my session") {
///     eprintln!("Barbara already running with tag 'my session' (PID {})", pid);
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
/// ```ignore
/// assert_eq!(parse_tag_from_argv(&["barbara", "--demo"]), None);
/// assert_eq!(parse_tag_from_argv(&["barbara", "--tag", "foo"]), Some("foo".to_string()));
/// assert_eq!(parse_tag_from_argv(&["barbara", "--tag=foo"]), Some("foo".to_string()));
/// assert_eq!(parse_tag_from_argv(&["barbara", "--tag"]), Some("".to_string()));
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

/// Warn once if /proc is unavailable
///
/// Uses `std::sync::Once` to ensure warning is printed only once per process.
fn warn_proc_unavailable() {
    static PROC_WARN_ONCE: Once = Once::new();
    PROC_WARN_ONCE.call_once(|| {
        eprintln!("[barbara] Warning: /proc unavailable, instance detection disabled");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tag_from_argv_no_tag() {
        assert_eq!(parse_tag_from_argv(&["barbara", "--demo"]), None);
    }

    #[test]
    fn test_parse_tag_from_argv_with_value() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag", "foo", "--demo"]),
            Some("foo".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_equals_format() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag=foo", "--demo"]),
            Some("foo".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_with_spaces() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag", "my session"]),
            Some("my session".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_empty_value() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag", ""]),
            Some("".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_trailing_tag() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag"]),
            Some("".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_last_wins() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag", "a", "--tag", "b"]),
            Some("b".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_with_equals() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag=with=equals"]),
            Some("with=equals".to_string())
        );
    }

    #[test]
    fn test_parse_tag_from_argv_unicode() {
        assert_eq!(
            parse_tag_from_argv(&["barbara", "--tag", "unicode: 日本語"]),
            Some("unicode: 日本語".to_string())
        );
    }
}
