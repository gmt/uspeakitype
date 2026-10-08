#!/usr/bin/env python3
"""Exercise the compiled Qt frontend with isolated software rendering and pipes."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/qt-quick/usit-qt-overlay"
ENV = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software")
SNAPSHOT = {
    "committed": "snapshot committed", "partial": "snapshot partial",
    "push_to_talk": False, "ptt_shortcut": "F9", "ptt_held": False, "ptt_error": None,
    "paused": True, "injection_enabled": False, "transcription_available": True,
    "auto_gain_enabled": True, "auto_save": False, "gain": 1.0,
    "viz_mode": "waterfall", "color_scheme": "flame", "opacity": 1.0,
    "helper_summary": "Paused · Moonshine Base", "helper_mode": "paused",
    "source_label": "Default", "model_label": "Moonshine Base", "error": None,
    "models": [{"id": 0, "label": "Moonshine Base"},
               {"id": 1, "label": "Moonshine Tiny"},
               {"id": 2, "label": "Parakeet"}],
    "selected_model": 1,
    "download_progress": None, "spectrum": [0.1, 0.8, 0.5, 0.2],
}

with tempfile.TemporaryDirectory(prefix="usit-qt-test-") as directory:
    screenshot = Path(directory) / "overlay.png"
    process = subprocess.Popen(
        [str(BINARY), "--self-test", "--screenshot", str(screenshot)],
        env=ENV, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    try:
        # Invalid input must not kill the window. Fragmented frames must be held
        # until newline, and multiple frames in a read must all be processed.
        process.stdin.write(b"not json\n")
        line = json.dumps(SNAPSHOT).encode() + b"\n"
        process.stdin.write(line[:31])
        process.stdin.flush()
        time.sleep(0.04)
        process.stdin.write(line[31:] + line)
        process.stdin.flush()
        process.wait(timeout=8)
        output = process.stdout.read().decode()
        errors = process.stderr.read().decode()
        assert process.returncode == 0, errors
        commands = [json.loads(line) for line in output.splitlines()]
        assert [cmd["type"] for cmd in commands] == [
            "toggle_pause", "toggle_pause",  # real mouse and keyboard events
            "select_model",  # one activation after browsing directly to last entry
            "toggle_pause", "toggle_injection", "toggle_viz", "toggle_agc",
            "toggle_auto_save", "cycle_device", "cycle_model", "set_gain",
            "set_opacity", "cycle_color", "save", "toggle_ptt", "set_ptt_shortcut",
            "toggle_pause", "toggle_injection", "toggle_ptt",  # tray uses the same commands
        ], commands
        assert commands[2]["value"] == 2, commands
        assert commands[10]["value"] == 1.25, commands
        assert commands[11]["value"] == 0.8, commands
        assert commands[15]["value"] == "Ctrl+Alt+Space", commands
        assert screenshot.stat().st_size > 5000, "Rendered image is unexpectedly empty"
        unexpected = [line for line in errors.splitlines()
                      if "qrc:" in line or "ReferenceError" in line or "TypeError" in line]
        assert not unexpected, errors
        # Preserve the rendered result only when explicitly requested.
        if os.environ.get("USIT_QT_TEST_SCREENSHOT"):
            Path(os.environ["USIT_QT_TEST_SCREENSHOT"]).write_bytes(screenshot.read_bytes())
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdin.close()
        process.stdout.close()
        process.stderr.close()

# Engine exit/disconnection must never leave a detached graphical helper alive.
result = subprocess.run([str(BINARY)], input=b"", env=ENV, capture_output=True, timeout=5)
assert result.returncode == 0, result.stderr.decode()
print("Qt frontend: live bindings, mouse/keyboard commands, software rendering, invalid/fragmented input and EOF passed")

# Closing without a tray must exit; an available tray must keep the window alive
# while hidden. The optional live test creates only this helper, with no engine,
# microphone or text injector, and removes its tray item on Quit.
checks = [("--close-test", ENV, ["quit"])]
if os.environ.get("USIT_QT_LIVE_TRAY_TEST") == "1":
    checks.append(("--tray-self-test", dict(os.environ),
                   ["toggle_pause", "toggle_injection", "toggle_ptt", "quit"]))
for flag, environment, expected in checks:
    process = subprocess.Popen([str(BINARY), flag], env=environment,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    try:
        process.stdin.write(json.dumps(SNAPSHOT).encode() + b"\n")
        process.stdin.flush()
        process.wait(timeout=8)
        output = process.stdout.read().decode()
        errors = process.stderr.read().decode()
        assert process.returncode == 0, errors
        assert [json.loads(line)["type"] for line in output.splitlines()] == expected, output
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdin.close()
        process.stdout.close()
        process.stderr.close()
print("Tray actions and close lifecycle passed" +
      (" (including live desktop tray)" if len(checks) == 2 else " (offscreen)"))
