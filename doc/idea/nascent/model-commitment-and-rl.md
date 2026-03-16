# Model Commitment And RL

Status: nascent idea, not a committed plan

## Problem

The current UX is permissive about model choice, but RL from user corrections wants strong attribution.

If the user corrects text, we need to know which model family and version produced it.

Right now that is too fuzzy for clean online learning.

## Principle

There should be one explicit designated driver model for any session whose outputs may generate reward data.

`auto` can still exist, but it should be treated as operational convenience, not clean RL data by default.

## Why This Matters

Without strong attribution we risk:

- mixing reward signals across incompatible model families
- training the wrong model on the wrong correction
- masking regressions because the serving model changed mid-session
- poisoning data if fallback behavior is mistaken for intentional model choice

## Suggested UX Direction

For future RL-capable modes:

- show the active model clearly
- require an explicit model selection or profile selection before collecting learning data
- pin that choice for the session unless the user deliberately switches
- record exact model provenance alongside each emitted token span

Possible modes:

- Display mode
  - current flexible behavior is fine
- Learning mode
  - one pinned model, one provenance trail
- Auto benchmark mode
  - can compare models, but should not emit undifferentiated reward data

## Data Model Sketch

Every correction-bearing event should eventually carry:

- model family
- model id
- model version or commit
- decoding settings
- timestamp
- whether the output came from normal inference, fallback, or swap-after-download behavior

## Recommendation

Do not start RL on user corrections until the product has an explicit "learning-safe" model commitment story.

This is more important than the specific RL algorithm.
