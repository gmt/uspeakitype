# NeMo TDT / Parakeet ONNX workflow (fine-tune → export → `usit`)

`usit` treats **NeMo** as the training/fine-tuning environment (Python/PyTorch) and **ONNX** as the deployment artifact that the Rust app runs via ONNX Runtime.

This doc describes the model folder layout that `usit` expects for NeMo Conformer transducer models (e.g. Parakeet-TDT), and how to produce those artifacts.

## Folder layout expected by `usit`

`usit` loads models from the “models root” directory (default: `~/.cache/usit/models`).

For `--model parakeet-tdt-0.6b-v3`, `usit` expects:

```text
~/.cache/usit/models/
  parakeet-tdt-0.6b-v3/
    encoder-model.onnx                (or encoder.onnx / encoder_model.onnx)
    encoder-model.onnx.data           (optional external weights; must sit next to the .onnx)
    decoder_joint-model.onnx          (or decoder_joint.onnx / decoder_joint_model.onnx)
    vocab.txt                         (token → id mapping; must include <blk>)
    config.json                       (optional but recommended; features_size/subsampling_factor)
    nemo128.onnx                      (required for features_size=128; or nemo80.onnx)
```

Notes:

- If the encoder uses **external data** (`*.onnx.data`), ONNX Runtime requires it to be in the same directory as the `.onnx` file.
- `config.json` is optional, but helps avoid ambiguity. `usit` uses:
  - `features_size` (80 or 128)
  - `subsampling_factor` (commonly 8)
  - `max_tokens_per_step` (optional; default 10)

### Option A: use an already-exported ONNX model

If you already have a known-good ONNX export (for example from a Hugging Face repo like `istupakov/parakeet-tdt-0.6b-v3-onnx`), copy the files into the folder layout above and run:

```bash
usit --model parakeet-tdt-0.6b-v3
```

### Option B: fine-tune in NeMo, then export ONNX

#### 1) Fine-tune in NeMo (Python)

- Fine-tune using NeMo training scripts/configs as usual.
- Treat the resulting `.nemo` / checkpoint as the **source of truth**.

#### 2) Export ONNX (encoder + decoder/joint)

NeMo models that implement `Exportable` can be exported to ONNX via `.export(...)`. RNN-T/TDT models often export in **multiple subnets** (encoder / decoder / joint).

NeMo’s export docs (use these as the canonical reference):

- `https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/core/export.html`

Important constraints for `usit` today:

- The runtime expects an encoder ONNX and a **combined** `decoder_joint` ONNX (as used by the `onnx-asr` reference implementation).
- If your export produces separate decoder + joint models, you’ll need to adapt the export step (or adapt `usit` to run both; not implemented yet).

After export, rename/copy into the `usit` folder layout:

- `encoder-model.onnx`
- `decoder_joint-model.onnx`

#### 3) Generate `vocab.txt` (and ensure `<blk>`)

`vocab.txt` is a plain text mapping of `token id` per line:

```text
hello 123
 world 456
<blk> 8192
```

Requirements:

- IDs must match the model tokenizer’s IDs.
- The blank token must exist and be exactly `<blk>`.
- SentencePiece-style `▁` tokens should be preserved in the raw vocab; `usit` converts `▁` to a space at load time.

#### 4) Provide a NeMo preprocessor ONNX (`nemo128.onnx` / `nemo80.onnx`)

`usit` uses an ONNX “preprocessor graph” to convert 16kHz mono PCM samples into log-mel features.

For Parakeet-TDT exports, you typically want:

- `nemo128.onnx` for `features_size=128`

This preprocessor is commonly shipped alongside ONNX exports (as in `onnx-asr`-style packaging).  If your export does not include it, you need to generate it (for example via an `onnxscript` implementation of NeMo’s log-mel frontend).

### Troubleshooting

- **Model loads but produces empty text**: verify `vocab.txt` contains `<blk>` and that the blank id matches the model’s training/export.
- **ONNX Runtime can’t find weights**: if you have `encoder-model.onnx.data`, ensure it’s in the same directory as `encoder-model.onnx`.
- **Preprocessor missing / wrong feature size**: ensure `config.json` sets `features_size` and that the corresponding `nemo{features_size}.onnx` exists.
