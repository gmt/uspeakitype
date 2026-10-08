#!/usr/bin/env python3
"""Verify held-key lifecycles without registering any desktop shortcuts."""
import json
import os
from pathlib import Path
import select
import subprocess

BINARY = Path(__file__).resolve().parents[2] / "target/qt-quick/usit-qt-overlay"
ENV = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software")


def launch(flag, environment=ENV):
    return subprocess.Popen([str(BINARY), flag], env=environment,
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE)


def finish(process):
    try:
        process.wait(timeout=5)
        output = [json.loads(line) for line in process.stdout.read().decode().splitlines()]
        errors = process.stderr.read().decode()
        assert process.returncode == 0, errors
        assert "qrc:" not in errors, "The hotkey-only helper must not load QML"
        return output
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        for stream in (process.stdin, process.stdout, process.stderr):
            stream.close()


process = launch("--hotkey-self-test")
messages = finish(process)
assert [m["type"] for m in messages] == [
    "hotkey_status", "ptt_press", "ptt_release", "ptt_press", "ptt_release",
    "hotkey_status", "hotkey_status", "ptt_press", "ptt_release", "hotkey_status",
], messages
statuses = [m for m in messages if m["type"] == "hotkey_status"]
assert [s["available"] for s in statuses] == [True, False, True, False], statuses
assert statuses[-1]["error"], statuses

# Exercise the actual stdin EOF shutdown while a simulated key remains held.
process = launch("--hotkey-disconnect-test")
assert select.select([process.stdout], [], [], 3)[0], "No test registration status"
status = json.loads(process.stdout.readline())
assert status["type"] == "hotkey_status" and status["available"], status
press = json.loads(process.stdout.readline())
assert press["type"] == "ptt_press", press
process.stdin.close()
messages = finish(process)
assert messages == [{"type": "ptt_release"}], messages

# Production code must fail closed with useful status when its session bus is
# unavailable. This deliberately cannot reach the real KDE shortcut service.
isolated = dict(ENV, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent-usit-test-bus")
process = launch("--hotkey-only", isolated)
process.stdin.write(json.dumps({"push_to_talk": True, "ptt_shortcut": "F9"}).encode() + b"\n")
process.stdin.flush()
assert select.select([process.stdout], [], [], 3)[0], "No missing-service status"
status = json.loads(process.stdout.readline())
assert status["type"] == "hotkey_status" and not status["available"] and status["error"], status
process.stdin.close()
assert finish(process) == []
print("Hotkey helper: press/release, repeat suppression, disable/service-loss release, EOF release and unavailable-service status passed")
