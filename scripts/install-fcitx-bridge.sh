#!/usr/bin/env bash
# Install the dictation bridge for the current user without restarting Fcitx.
set -euo pipefail
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
build_dir="$repo_dir/target/fcitx5-bridge"
cmake -S "$repo_dir/fcitx5-usit-bridge" -B "$build_dir"
cmake --build "$build_dir" --parallel 4
addon_dir="$HOME/.local/lib/fcitx5"
config_dir="${XDG_DATA_HOME:-$HOME/.local/share}/fcitx5/addon"
install -d "$addon_dir" "$config_dir"
install -m 755 "$build_dir/src/libusitbridge.so" "$addon_dir/libusitbridge.so"
# An absolute Library path works with lib and lib64 distributions alike.
awk -v library="$addon_dir/libusitbridge" '
    /^Library=/ { print "Library=" library; next }
    { print }
' "$build_dir/src/usitbridge.conf" > "$config_dir/usitbridge.conf"
printf 'Installed usit bridge. Restart Fcitx through the desktop to load it.\n'
