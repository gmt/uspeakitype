#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHM_NAME="/usit-shm-$$"
AUTO_QUIT_MS=""

while (($#)); do
    case "$1" in
        --shm-name)
            SHM_NAME="$2"
            shift 2
            ;;
        --auto-quit-ms)
            AUTO_QUIT_MS="$2"
            shift 2
            ;;
        *)
            echo "unknown arg: $1" >&2
            exit 1
            ;;
    esac
done

"$ROOT/build.sh"

HELPER="$ROOT/rust_helper/target/debug/usit-shm-helper"
SHELL="$ROOT/qt_shell/build/usit-shm-shell"

"$HELPER" --shm-name "$SHM_NAME" &
HELPER_PID=$!

cleanup() {
    kill "$HELPER_PID" 2>/dev/null || true
    wait "$HELPER_PID" 2>/dev/null || true
}
trap cleanup EXIT

SHELL_ARGS=(--shm-name "$SHM_NAME")
if [[ -n "$AUTO_QUIT_MS" ]]; then
    SHELL_ARGS+=(--auto-quit-ms "$AUTO_QUIT_MS")
fi

"$SHELL" "${SHELL_ARGS[@]}"
