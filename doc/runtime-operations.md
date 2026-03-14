# Runtime Operations

This note covers a few operational behaviors that matter in practice but do not belong in the user-facing feature overview.

## Logging

On Linux, `usit` tries journald first.

If journald initialization succeeds:

- logs go to the journal with `SYSLOG_IDENTIFIER=usit`
- the package version is attached as an extra field

If journald is unavailable, such as inside some Docker environments, `usit` falls back to `env_logger`.

This keeps the normal terminal surfaces clean while still preserving diagnostics.

## Instance Tags

`usit` supports tagging instances with `--tag`.

Supported forms:

- `--tag value`
- `--tag=value`

Tags may contain spaces. If multiple `--tag` arguments appear, the last one wins.

## Duplicate Detection

Instance discovery is `/proc`-based and intentionally simple:

- scan numeric `/proc/<pid>` directories
- read `cmdline`
- treat `argv[0] == "usit"` or paths ending in `/usit` as matches
- parse tags from argv
- ignore the current process

When `--tag` is set, startup checks for another instance with the same exact tag.

- default behavior: warn and continue
- with `--no-duplicate-tag`: print an error and exit with status 1

## Listing Instances

`--list-instances` has two output modes.

Machine-readable mode prints:

```text
PID<TAB>TAG
```

Notes:

- untagged and empty-tag instances both print an empty tag field
- control characters in tags are escaped

Human-readable mode prints a simple table:

- `None` becomes `(untagged)`
- empty string becomes `(empty)`

If no instances are running, human mode prints `No usit instances running` and machine mode prints nothing.

## Why `/proc` Matters

The `/proc` approach avoids lockfiles or a separate daemon, which keeps the feature lightweight. The tradeoff is that instance discovery is Linux-specific; if `/proc` is unavailable, `usit` warns once and disables discovery behavior gracefully.
