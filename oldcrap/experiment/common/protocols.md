# Wire Protocol Notes

These spikes are allowed to cheat on features, but they should stay honest
about how state crosses boundaries.

## Common control vocabulary

Even the toy prototypes should speak in roughly the same semantic terms:

- `pause` / `resume`
- `toggle_visualizer_mode`
- `set_gain`
- `quit`
- `source_next`
- `model_next`

Not every spike needs to implement every command, but unsupported commands
should be documented explicitly instead of silently vanishing.

## Stdio JSON baseline

The current graphical branch uses newline-delimited JSON over stdio:

- Rust -> shell: snapshots
- shell -> Rust: user intents

Strengths:

- simple to inspect
- easy to proxy or log
- tolerant of crashes

Weaknesses:

- text serialization overhead
- polling cadence bias
- awkward for high-rate audio/meters if we keep everything as JSON

## Shared memory baseline

For shared memory spikes, prefer a very small explicit ABI:

- fixed-size header with `version`, `sequence`, `flags`, and lengths
- one meter/levels region
- one UTF-8 text region
- optional command mailbox in the opposite direction

Strengths:

- cheap high-rate meter/audio-ish updates
- fewer copies than JSON text streams

Weaknesses:

- synchronization complexity
- stale-reader/stale-writer edge cases
- ABI/version headaches show up immediately

## Evaluation bias

These spikes are not trying to prove raw performance first. They are trying to
surface:

- integration friction
- build-system pain
- debugging ergonomics
- how much architecture distortion each option causes
