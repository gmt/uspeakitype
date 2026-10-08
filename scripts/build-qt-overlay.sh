#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cmake -S "$root/qt_quick_overlay" -B "$root/target/qt-quick" -DCMAKE_BUILD_TYPE=Release
cmake --build "$root/target/qt-quick" --parallel "${CMAKE_BUILD_PARALLEL_LEVEL:-4}"
