#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

import librosa
import numpy as np
import soundfile as sf
import webrtcvad


def frame_signal(signal, frame_size, hop_size):
    for start in range(0, max(1, len(signal) - frame_size + 1), hop_size):
        yield signal[start:start + frame_size]


def rms(samples):
    if len(samples) == 0:
        return 0.0
    return float(np.sqrt(np.mean(np.square(samples))))


def dbfs(value):
    return float(20 * np.log10(max(value, 1e-12)))


def compute_stats(samples, sample_rate):
    peak = float(np.max(np.abs(samples))) if len(samples) else 0.0
    overall_rms = rms(samples)
    dc_offset = float(np.mean(samples)) if len(samples) else 0.0
    clip_fraction = float(np.mean(np.abs(samples) >= 0.999)) if len(samples) else 0.0

    window_size = int(sample_rate * 0.03)
    hop_size = int(sample_rate * 0.01)
    rms_windows = [rms(frame) for frame in frame_signal(samples, window_size, hop_size)]
    rms_windows = rms_windows if rms_windows else [0.0]

    noise_floor = float(np.percentile(rms_windows, 10))
    rms_p95 = float(np.percentile(rms_windows, 95))

    return {
        "peak": peak,
        "peak_dbfs": dbfs(peak),
        "rms": overall_rms,
        "rms_dbfs": dbfs(overall_rms),
        "dc_offset": dc_offset,
        "clip_fraction": clip_fraction,
        "noise_floor_rms": noise_floor,
        "noise_floor_dbfs": dbfs(noise_floor),
        "rms_p95": rms_p95,
        "rms_p95_dbfs": dbfs(rms_p95),
    }


def compute_vad(samples, sample_rate):
    vad = webrtcvad.Vad(2)
    frame_ms = 20
    frame_size = int(sample_rate * frame_ms / 1000)
    hop = frame_size
    if sample_rate != 16000:
        samples = librosa.resample(samples, orig_sr=sample_rate, target_sr=16000)
        sample_rate = 16000
        frame_size = int(sample_rate * frame_ms / 1000)
        hop = frame_size

    pcm16 = (np.clip(samples, -1.0, 1.0) * 32767.0).astype(np.int16)
    speech_frames = 0
    total_frames = 0

    for frame in frame_signal(pcm16, frame_size, hop):
        if len(frame) != frame_size:
            continue
        total_frames += 1
        if vad.is_speech(frame.tobytes(), sample_rate):
            speech_frames += 1

    ratio = speech_frames / total_frames if total_frames else 0.0
    return {
        "speech_frame_ratio": ratio,
        "speech_frames": speech_frames,
        "total_frames": total_frames,
    }


def analyze_file(path):
    samples, sr = sf.read(path, dtype="float32")
    if samples.ndim > 1:
        samples = np.mean(samples, axis=1)
    stats = compute_stats(samples, sr)
    vad_stats = compute_vad(samples, sr)
    duration = len(samples) / sr if sr else 0.0
    return {
        "path": str(path),
        "sample_rate": sr,
        "duration_seconds": duration,
        "stats": stats,
        "vad": vad_stats,
    }


def main():
    parser = argparse.ArgumentParser(description="Analyze WAV files for AGC/ASR testing")
    parser.add_argument("paths", nargs="+", type=Path, help="WAV files to analyze")
    parser.add_argument("--json", action="store_true", help="Output JSON")
    args = parser.parse_args()

    results = [analyze_file(path) for path in args.paths]
    if args.json:
        print(json.dumps(results, indent=2))
    else:
        for result in results:
            print(f"{result['path']} ({result['duration_seconds']:.2f}s @ {result['sample_rate']}Hz)")
            stats = result["stats"]
            print(f"  peak: {stats['peak']:.3f} ({stats['peak_dbfs']:.1f} dBFS)")
            print(f"  rms: {stats['rms']:.3f} ({stats['rms_dbfs']:.1f} dBFS)")
            print(f"  noise floor: {stats['noise_floor_rms']:.3f} ({stats['noise_floor_dbfs']:.1f} dBFS)")
            print(f"  rms p95: {stats['rms_p95']:.3f} ({stats['rms_p95_dbfs']:.1f} dBFS)")
            print(f"  dc offset: {stats['dc_offset']:.5f}")
            print(f"  clip fraction: {stats['clip_fraction']:.4f}")
            vad_stats = result["vad"]
            print(f"  speech ratio: {vad_stats['speech_frame_ratio']:.2f} ({vad_stats['speech_frames']}/{vad_stats['total_frames']})")


if __name__ == "__main__":
    main()
