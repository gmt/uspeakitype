#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bridge_dir="$repo_dir/fcitx5-usit-bridge"
build_dir="$bridge_dir/build"
lib_path="$build_dir/src/libusitbridge.so"
conf_dir="${XDG_DATA_HOME:-$HOME/.local/share}/fcitx5/addon"
conf_path="$conf_dir/usitbridge.conf"
library_field="${lib_path%.so}"

cmake -S "$bridge_dir" -B "$build_dir"
cmake --build "$build_dir"

mkdir -p "$conf_dir"
cat >"$conf_path" <<EOF
[Addon]
Name=Usit Bridge
Name[en]=Usit Bridge
Category=Module
Library=$library_field
Type=SharedLibrary
OnDemand=False
Configurable=False

[Addon/Dependencies]
0=dbus
EOF

echo "Wrote $conf_path"

if pgrep -x fcitx5 >/dev/null 2>&1; then
    fcitx5 -r >/dev/null 2>&1 &
    sleep 2
    echo "Triggered fcitx5 reload"
else
    echo "fcitx5 is not running; start or reselect the stock Fcitx 5 Wayland launcher."
fi
