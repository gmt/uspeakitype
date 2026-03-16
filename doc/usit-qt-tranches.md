# usit-qt Tranches

This is the deliberate reintroduction order for the `usit-qt` rebuild.

The point is not to blast through the whole list quickly. The point is to add
one tranche at a time, run it, and stop so we can catch startup delay or
architectural regret as close as possible to the moment it enters.

## Order

1. ANSI
   Bring back the terminal surface first so the rebuilt app keeps a
   low-complexity control and debug path while the graphical shell evolves.
2. Config
   Reintroduce unified config loading and precedence only after the minimal app
   shape is stable enough to deserve persisted state.
3. CPL
   Reintroduce the control-panel layer once ANSI and config agree on the basic
   state model.
4. Model Download + Cache
   Bring back downloading and cache integrity with permission to rework the
   pipeline rather than porting the old one mechanically.
5. Model Loading
   Reintroduce actual transcription backends in slices:
   - 5a. Moonshine
   - 5b. Parakeet

## Rule

After each tranche:

- run it
- time it
- notice what feels worse
- stop and review before pulling the next subsystem in

If a nasty startup delay appears, that tranche is guilty until proven innocent.
