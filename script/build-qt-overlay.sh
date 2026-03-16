#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src_dir="${repo_root}/qt_widgets_overlay"
build_dir="${src_dir}/build"

mkdir -p "${build_dir}"
cd "${build_dir}"

qmake_bin="${QMAKE:-}"
if [[ -z "${qmake_bin}" ]]; then
  if command -v qmake6 >/dev/null 2>&1; then
    qmake_bin="$(command -v qmake6)"
  else
    qmake_bin="$(command -v qmake)"
  fi
fi

"${qmake_bin}" ../usit_qt_overlay.pro
make -j"$(nproc)"
