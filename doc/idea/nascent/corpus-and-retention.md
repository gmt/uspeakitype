# Corpus And Retention

Status: nascent idea, not a committed plan

## Question

Is there a curated license-compatible FOSS language example corpus we can lean on for:

- regression testing
- baseline retention during fine-tuning
- multilingual evaluation

## Short Answer

Probably not one corpus. More likely a deliberately curated mixture with strict per-dataset license tracking.

## Candidate Spine

- Mozilla Common Voice
  - strongest first choice for broad FOSS speech coverage
  - useful for both evaluation and retention
  - read-speech bias remains a real limitation
- FLEURS
  - good multilingual breadth and cleaner standardized eval shape
  - probably better as eval and calibration than as the dominant training source
- MLS / OpenSLR 94
  - useful for retention and read-speech benchmarking
  - again, not enough spontaneity by itself
- VoxPopuli
  - attractive because it is less "lab clean" than many read corpora
  - stronger candidate for robustness eval than for direct product-style dictation behavior

## Recommended Stance

Treat this as a three-layer stack:

1. Retention spine
   Keep a frozen multilingual set from the sources above to detect regressions after any adaptation.
2. Product eval set
   Build a much smaller `usit`-specific set that reflects actual dictation behavior:
   short utterances, punctuation, hesitations, repairs, code-switching, proper nouns, desktop command language.
3. Optional adaptation pool
   If we fine-tune, use only data with clearly tracked provenance and license, and keep the retention spine untouched.

## Important Caveat

Most public FOSS corpora are read speech. They help resist catastrophic drift, but they do not by themselves teach:

- live revision behavior
- self-correction
- hesitant conversational dictation
- desktop-command phrasing

So they are best used as anti-decoherence ballast, not as the whole recipe.

## Open Questions

- Do we want a repository-local manifest of approved corpora and licenses?
- Do we want to separate "train-approved" from "eval-only" datasets?
- Do we want a tiny hand-curated `usit` eval set checked into the repo as text manifests plus fetch scripts?
