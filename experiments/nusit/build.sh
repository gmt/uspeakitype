#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

(
    cd "$here/rust_helper"
    cargo build
)

mkdir -p "$here/shell/build"
(
    cd "$here/shell/build"
    qmake6 ../nusit.pro
    make -j"$(nproc)"
)
