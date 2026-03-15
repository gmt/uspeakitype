#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[shared-memory-bridge] building rust helper"
cargo build --manifest-path "$ROOT/rust_helper/Cargo.toml"

echo "[shared-memory-bridge] building qt shell"
mkdir -p "$ROOT/qt_shell/build"
cd "$ROOT/qt_shell/build"
if command -v qmake6 >/dev/null 2>&1; then
    qmake6 ../usit-shm-shell.pro
else
    qmake ../usit-shm-shell.pro
fi
make -j"$(nproc)"
