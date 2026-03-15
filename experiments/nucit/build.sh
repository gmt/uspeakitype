#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

(
    cd "$here/rust_worker"
    cargo build
)

mkdir -p "$here/shell/build"
(
    cd "$here/shell/build"
    qmake6 ../nucit.pro
    make -j"$(nproc)"
)
