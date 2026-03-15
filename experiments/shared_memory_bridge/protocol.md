# Shared Memory Wire Protocol

This prototype uses one POSIX shared memory object, mapped read-write by both
processes.

Name:

- default: `/usit-shm-demo`
- configurable via `--shm-name`

## Binary layout

The region is a single fixed-size C layout struct.

```c
struct BridgeLayout {
    char magic[8];                 // "USITSHM\\0"
    uint32_t version;              // 1
    uint32_t reserved0;

    uint64_t snapshot_seq;         // Rust increments after writing a snapshot
    uint64_t command_seq;          // Qt increments after writing a command
    uint64_t last_applied_command_seq;

    float level;
    float peak;
    float gain;

    uint8_t paused;
    uint8_t injection_enabled;
    uint8_t quit_requested;
    uint8_t reserved1;

    uint32_t pending_command;      // enum CommandKind
    float pending_value;           // slider payload

    char committed[128];
    char partial[128];
    char source_label[64];
    char model_label[64];
    char error_label[128];
};
```

Target size: `576` bytes.

Strings are UTF-8, nul-terminated when possible, and truncated on overflow.

## Command path

Qt writes commands by:

1. writing `pending_command`
2. writing `pending_value`
3. incrementing `command_seq`

Rust applies a command only when:

```text
command_seq > last_applied_command_seq
```

Then it updates `last_applied_command_seq`.

## Commands

```text
0 = None
1 = TogglePause
2 = ToggleInjection
3 = SetGain
4 = Quit
```

## Snapshot path

Rust writes snapshot fields, then increments `snapshot_seq`.

Qt polls on a timer and only repaints when `snapshot_seq` changes.

## Deliberate limitations

- no ring buffer
- no atomic field wrappers
- no eventfd/semaphore wakeups
- no resizeable payloads
- no multiple producers

It is intentionally primitive so the feel of “shared-memory-only UI plumbing”
stays obvious.
