#!/usr/bin/env python3
import argparse
from pathlib import Path

import librosa
import soundfile as sf
import numpy as np


def main():
    parser = argparse.ArgumentParser(description="Convert WAV to 16k mono float32")
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    samples, sr = sf.read(args.input, dtype="float32")
    if samples.ndim > 1:
        samples = np.mean(samples, axis=1)

    if sr != 16000:
        samples = librosa.resample(samples, orig_sr=sr, target_sr=16000)

    sf.write(args.output, samples, 16000, subtype="PCM_16")


if __name__ == "__main__":
    main()
