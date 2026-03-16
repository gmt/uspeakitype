# crazyideas findings

These notes are the point of the branch: not which sketch is prettiest, but
which kinds of ugliness feel survivable.

## quick verdict

- The current direction still looks broadly right: keep the core in Rust and
  let the graphical shell drift toward a more desktop-native layer.
- A thin C++ shell or integration layer now feels much more plausible than a
  whole-project rewrite to C++.
- Shared memory did not feel like a better default bridge than the current
  line-oriented stdio protocol.
- CXX-Qt is viable enough to stop dismissing, but not yet persuasive enough to
  beat either the current subprocess bridge or a thin manual C++ shim.
- Raw C++ interop was less scary than expected for a tiny shell, but exactly as
  annoying as expected for anything larger than a tiny shell.
- A bare-Qt OpenGL canvas feels much more plausible than I expected as a
  compromise position: native hierarchy, no text wire, and no Qt Quick
  commitment.

## per spike

### nusit

The good:

- A C++/Qt executable owning the window felt conceptually clean.
- Keeping Rust as a helper over a boring JSON stream made debugging and failure
  boundaries easy to understand.
- This was the easiest sketch to imagine growing into a “desktop-native shell
  around a Rust engine.”

The pain:

- We immediately accepted a process boundary and all the lifecycle work that
  comes with it.
- The wire protocol is easy, but still a protocol we must version and keep
  honest.
- It does not reduce the number of moving parts; it just redistributes them
  more legibly.

Verdict:

- Strong contender.
- If we want to lean further into KDE/Qt without rewriting the engine, this is
  one of the best-feeling directions.

### nucit

The good:

- It makes the “Qt/C++ owns the realtime-ish shell loop” story very tangible.
- Local meters and local shell responsiveness feel natural when the shell owns
  them directly.

The pain:

- The architectural split starts drifting fast: once C++ owns the fake audio
  loop, it is easy to imagine it trying to own more and more of the real audio
  story.
- That drift feels dangerous, because audio/model/integration logic stop having
  a single obvious home.

Verdict:

- Useful warning shot.
- I would only follow this road if we deliberately decide that desktop-native
  integration and local UI timing are more important than keeping audio in the
  Rust core.

### shared memory bridge

The good:

- Snapshot publication is cheap and mechanically satisfying.
- The fixed-layout ABI makes data ownership brutally explicit.

The pain:

- The command path immediately got uglier than stdio JSON.
- Mailbox discipline, sequence counters, and stale-reader/writer questions show
  up right away.
- Variable-length text feels cramped and unnatural in the shared-memory shape.

Verdict:

- Probably not the right default.
- This feels more like a special-purpose optimization than the bridge we should
  reach for first.

### nuxxit

The good:

- The in-process story is attractive: no helper process, no JSON, no explicit
  IPC.
- CXX-Qt is real enough that a toy app stands up and runs.
- For a tiny pure-Rust Qt/QML app, the result is pleasantly compact.

The pain:

- The codegen and generated-type/property rules are real ceremony.
- Small mistakes around property plumbing and mutability are easy to make and
  not especially charming to debug.
- It feels like adopting a framework worldview, not merely borrowing a widget
  library.

Verdict:

- More plausible than I expected, less carefree than I hoped.
- Worth keeping in mind, especially if we later decide a Qt Quick/QML shell is
  strategically important.
- Not yet a clear winner over the current subprocess model.

### hellnuxit

The good:

- The manual boundary is ugly in an honest way.
- For a tiny shell, raw interop felt surprisingly tractable.
- It keeps the architecture explicit: Rust core, C++ shell, hand-written seam.

The pain:

- The build loop becomes custom immediately.
- The ABI stays cute only while the state surface is tiny.
- Every additional feature threatens to turn “thin shim” into “shadow
  application.”

Verdict:

- Better than expected as a narrow tactic.
- Bad as a sweeping rewrite strategy.
- Very plausible if we intentionally keep the C++ side small and disciplined.

### nuxglit

The good:

- `QOpenGLWidget` drops neatly into a normal Qt Widgets hierarchy, which makes
  the whole thing feel much less exotic than a Quick scenegraph experiment.
- `QOpenGLPaintDevice` gives us a real "paint into the current GL context"
  seam without making the visualizer a subprocess or a text-decoded protocol
  performance test.
- The Rust/C++ boundary stays pleasingly boring: scalars, a status string, and
  a bin buffer. No QML object-model contortions, no wire protocol.

The pain:

- This is still a C++ island, and the build knows it immediately.
- The current spike still copies bins across the boundary each frame, so it is
  not yet the "absolutely zero-copy forever" dream.
- `QOpenGLPaintDevice` feels like a widgets/OpenGL trick, not a general
  desktop-graphics abstraction. It is promising, but also quite specific.

Verdict:

- Stronger than expected.
- If the future shell stays bare Qt rather than Quick, this is one of the most
  credible directions we have touched so far.
- It does not prove we should abandon the current direction, but it does prove
  that "native Qt hierarchy plus hot Rust data path" is a real thing, not a
  fantasy.

## what might actually beat the current direction?

Two things seem capable of beating the current direction, but only in narrow
forms:

- `nusit` style shell-first architecture
  - if we decide the real product wants a KDE/Qt-native outer shell and can
    tolerate IPC as a first-class fact of life
- `hellnuxit` style thin manual shim
  - if we want one process and very explicit ownership, and we are disciplined
    enough to keep the C++ side small
- `nuxglit` style native GL canvas in a thin C++ shell layer
  - if we want the visualizer to stay rich and local without accepting either a
    text wire or a full Qt Quick worldview

What did not feel like an improvement:

- shared memory as the default bridge
- moving audio into C++ by default
- rewriting the whole application into Qt/C++ just to “keep it simple”

## current recommendation

Keep the present Rust-core direction, but be less dogmatic about the shell.

That suggests:

1. Keep the engine, state, downloads, and ASR stack in Rust.
2. Be open to a narrow C++ integration/shell layer if KDE/Qt-native behavior
   becomes the bottleneck.
3. Keep stdio JSON or another simple stream protocol unless measurement proves
   it is the problem.
4. Treat CXX-Qt as a credible future option, not as the default next step.
