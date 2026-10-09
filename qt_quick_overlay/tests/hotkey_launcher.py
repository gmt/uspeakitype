#!/usr/bin/env python3
"""Verify hold-to-talk borrows usit's own KDE launcher key and hands it back.

A private session bus and standalone kglobalacceld run against temporary XDG
directories, so the current desktop's shortcuts are never touched. Key events
cannot reach that daemon; launcher grabs are observed through component state.
"""
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile
import time

BINARY = Path(__file__).resolve().parents[2] / "target/qt-quick/usit-qt-overlay"
DAEMON = Path("/usr/libexec/kglobalacceld")
F9 = "16777272"  # Qt::Key_F9 as kglobalacceld reports it


def wait_for(predicate, what, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.05)
    raise AssertionError(f"Timed out waiting for {what}")


class Session:
    def __init__(self, root):
        self.root = Path(root)
        for name in ("config", "data/applications", "cache", "state", "dirs"):
            (self.root / name).mkdir(parents=True)
        runtime = self.root / "runtime"
        runtime.mkdir(mode=0o700)
        apps = self.root / "data/applications"
        # Identified by its Exec rather than by the usit.desktop name.
        (apps / "dictation.desktop").write_text(
            "[Desktop Entry]\nType=Application\nName=Dictation\n"
            'Exec="/opt/speech/bin/usit" --qt\n')
        (apps / "other.desktop").write_text(
            "[Desktop Entry]\nType=Application\nName=Other\nExec=other-app\n")
        self.shortcuts = self.root / "config/kglobalshortcutsrc"
        self.shortcuts.write_text(
            "[services][dictation.desktop]\n_launch=F9\n\n"
            "[services][other.desktop]\n_launch=F10\n")
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("DBUS_", "XDG_", "WAYLAND_", "DISPLAY", "KDE_"))}
        self.env.update(
            XDG_CONFIG_HOME=str(self.root / "config"), XDG_CONFIG_DIRS=str(self.root / "dirs"),
            XDG_DATA_HOME=str(self.root / "data"), XDG_DATA_DIRS=str(self.root / "dirs"),
            XDG_CACHE_HOME=str(self.root / "cache"), XDG_STATE_HOME=str(self.root / "state"),
            XDG_RUNTIME_DIR=str(runtime), XDG_SESSION_TYPE="tty",
            QT_QPA_PLATFORM="offscreen")
        subprocess.run(["kbuildsycoca6", "--noincremental"], env=self.env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.bus = subprocess.Popen(
            ["dbus-daemon", "--session", "--nofork", "--print-address=1",
             f"--address=unix:path={runtime / 'bus'}"],
            env=self.env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        assert select.select([self.bus.stdout], [], [], 5)[0], "Private bus did not start"
        self.env["DBUS_SESSION_BUS_ADDRESS"] = self.bus.stdout.readline().decode().strip()
        self.log = open(self.root / "kglobalacceld.log", "wb")
        self.daemon = subprocess.Popen([str(DAEMON)], env=self.env,
                                       stdout=self.log, stderr=subprocess.STDOUT)
        wait_for(lambda: self.has_owner("org.kde.kglobalaccel"), "private kglobalacceld")

    def gdbus(self, dest, path, method, *args):
        result = subprocess.run(
            ["gdbus", "call", "--address", self.env["DBUS_SESSION_BUS_ADDRESS"],
             "--dest", dest, "--object-path", path, "--method", method, *args],
            capture_output=True, text=True, timeout=5)
        assert result.returncode == 0, result.stderr
        return result.stdout.strip()

    def has_owner(self, name):
        reply = self.gdbus("org.freedesktop.DBus", "/org/freedesktop/DBus",
                           "org.freedesktop.DBus.NameHasOwner", name)
        return reply == "(true,)"

    def grabbed(self, component):
        path = self.gdbus("org.kde.kglobalaccel", "/kglobalaccel",
                          "org.kde.KGlobalAccel.getComponent", component)
        path = path.split("'")[1]
        reply = self.gdbus("org.kde.kglobalaccel", path, "org.kde.kglobalaccel.Component.isActive")
        return reply == "(true,)"

    def hold_keys(self):
        return self.gdbus("org.kde.kglobalaccel", "/kglobalaccel",
                          "org.kde.KGlobalAccel.shortcutKeys",
                          "['usit', 'hold_to_dictate', '', '']")

    def close(self):
        for process in (self.daemon, self.bus):
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        self.bus.stdout.close()
        self.log.close()


class Helper:
    def __init__(self, session):
        self.errors = open(session.root / "helper.log", "ab")
        self.process = subprocess.Popen([str(BINARY), "--hotkey-only"], env=session.env,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.errors)

    def configure(self, enabled, key):
        snapshot = {"push_to_talk": enabled, "ptt_shortcut": key}
        self.process.stdin.write(json.dumps(snapshot).encode() + b"\n")
        self.process.stdin.flush()
        assert select.select([self.process.stdout], [], [], 5)[0], f"No status for {snapshot}"
        status = json.loads(self.process.stdout.readline())
        assert status["type"] == "hotkey_status", status
        return status

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=5) == 0, "Helper failed to exit on stdin EOF"
        self.process.stdout.close()
        self.errors.close()

    def kill(self):
        self.process.kill()
        self.process.wait()
        self.process.stdin.close()
        self.process.stdout.close()
        self.errors.close()


def main():
    missing = [tool for tool in ("dbus-daemon", "gdbus", "kbuildsycoca6") if not shutil.which(tool)]
    if missing or not DAEMON.exists():
        print(f"SKIP: launcher hand-off needs {', '.join(missing) or DAEMON}")
        return
    with tempfile.TemporaryDirectory(prefix="usit-kglobalaccel-") as root:
        session = Session(root)
        try:
            run(session)
        except BaseException:
            for log in ("kglobalacceld.log", "helper.log"):
                path = Path(root) / log
                if path.exists():
                    print(f"--- {log}\n{path.read_text(errors='replace')}")
            raise
        finally:
            session.close()
    print("Hotkey launcher: borrow, disable/rebind/exit hand-back, foreign refusal "
          "and orphan recovery passed")


def run(session):
    assert session.grabbed("dictation.desktop")
    helper = Helper(session)
    status = helper.configure(True, "F9")
    assert status["available"] and status["error"] is None, status
    assert not session.grabbed("dictation.desktop"), "Launcher kept F9 while hold-to-talk was armed"
    assert F9 in session.hold_keys()

    status = helper.configure(False, "F9")
    assert not status["available"] and status["error"] is None, status
    assert session.grabbed("dictation.desktop"), "Disabling did not return F9 to the launcher"
    assert F9 not in session.hold_keys()

    assert helper.configure(True, "F9")["available"]
    assert not session.grabbed("dictation.desktop")
    status = helper.configure(True, "F10")
    assert not status["available"] and "Other" in status["error"], status
    assert session.grabbed("dictation.desktop"), "Rebinding did not return F9 to the launcher"
    assert session.grabbed("other.desktop"), "Another application's launcher was taken"

    assert helper.configure(True, "F9")["available"]
    assert not session.grabbed("dictation.desktop")
    helper.close()
    assert session.grabbed("dictation.desktop"), "Exit did not return F9 to the launcher"
    assert F9 not in session.hold_keys()

    # A helper killed mid-loan cannot hand back; the next session repairs it.
    helper = Helper(session)
    assert helper.configure(True, "F9")["available"]
    helper.kill()
    wait_for(lambda: not session.has_owner("org.uspeakitype.PushToTalk"), "loan owner to vanish")
    assert not session.grabbed("dictation.desktop")
    helper = Helper(session)
    assert not helper.configure(False, "F9")["available"]
    assert session.grabbed("dictation.desktop"), "Orphaned launcher was not recovered"
    assert F9 not in session.hold_keys(), "Orphaned hold binding was not removed"
    helper.close()

    # Parking never rewrites the user's launcher bindings.
    time.sleep(0.8)  # kglobalacceld batches config writes for 500 ms
    saved = session.shortcuts.read_text()
    assert "[services][dictation.desktop]\n_launch=F9" in saved, saved
    assert "[services][other.desktop]\n_launch=F10" in saved, saved


if __name__ == "__main__":
    main()
