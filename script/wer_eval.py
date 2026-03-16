#!/usr/bin/env python3
import argparse
from pathlib import Path
import re


def normalize(text):
    text = text.lower()
    text = re.sub(r"[^a-z0-9\s]", "", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text


def wer(ref, hyp):
    r = ref.split()
    h = hyp.split()
    d = [[0] * (len(h) + 1) for _ in range(len(r) + 1)]
    for i in range(len(r) + 1):
        d[i][0] = i
    for j in range(len(h) + 1):
        d[0][j] = j
    for i in range(1, len(r) + 1):
        for j in range(1, len(h) + 1):
            if r[i - 1] == h[j - 1]:
                d[i][j] = d[i - 1][j - 1]
            else:
                substitute = d[i - 1][j - 1] + 1
                insert = d[i][j - 1] + 1
                delete = d[i - 1][j] + 1
                d[i][j] = min(substitute, insert, delete)
    return d[len(r)][len(h)] / max(1, len(r))


def main():
    parser = argparse.ArgumentParser(description="Simple WER evaluator")
    parser.add_argument("reference", type=Path)
    parser.add_argument("hypothesis", type=Path)
    args = parser.parse_args()

    ref = normalize(args.reference.read_text())
    hyp = normalize(args.hypothesis.read_text())
    score = wer(ref, hyp)
    print(f"WER: {score:.3f}")


if __name__ == "__main__":
    main()
