# Uncertainty Surface

Status: nascent idea, not a committed plan

## Idea

Flow uncertainty metadata alongside transcript text as we stream, cache it internally, and maybe surface it visually.

## Naming Note

"Perplexity" is probably the wrong umbrella term here.

For ASR systems like ours, the more portable concept is uncertainty or confidence, not classical language-model perplexity.

## More Realistic Signals

Depending on backend, useful token or span signals might include:

- log-probability of selected token
- entropy over next-token distribution
- top-1 vs top-2 margin
- blank-vs-emit confidence for transducer steps
- calibrated confidence score after normalization

## Why It Could Be Valuable

- improve internal debugging of revision behavior
- make it easier to inspect unstable spans
- support smarter commit heuristics later
- support training-data filtering if we ever learn from user corrections

## Why It Is Tricky

- Moonshine and Parakeet do not naturally expose identical uncertainty semantics
- raw scores are not directly comparable across model families
- partial text revisions mean we need span-aligned caches, not just final-string confidence
- surfacing low confidence too aggressively may create distracting UI flicker

## Suggested Internal Shape

Cache uncertainty at the token or subword span level and derive UI-friendly aggregates later.

That suggests a future internal object more like:

- committed text
- partial text
- span metadata
  - start/end offsets
  - backend token ids
  - uncertainty stats
  - revision generation index

## Possible UI Treatments

Start subtle if we ever expose it:

- dimmer partial text for low-confidence spans
- underline or tint for unstable spans
- optional debug overlay rather than default end-user UI

The safest default is probably:

- cache internally first
- expose only in debug mode
- revisit user-facing color coding after calibration work

## Recommendation

This is a strong instrumentation idea and a weaker immediate UX idea.

I would prioritize:

1. internal uncertainty plumbing
2. logging/debug visualization
3. calibration experiments
4. only then any default user-facing confidence coloring
