#!/bin/bash
# scripts/docker-visual-tests.sh
#
# Wrapper for running Docker visual tests with Cargo.lock sync checking.
#
# Usage:
#   ./scripts/docker-visual-tests.sh                    # Run all visual tests
#   ./scripts/docker-visual-tests.sh test_opacity_half  # Run specific test
#   USIT_STRICT_SYNC=1 ./scripts/docker-visual-tests.sh # Fail if stale

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."

cd "$REPO_ROOT"

# Compute host Cargo.lock hash
HOST_HASH=$(sha256sum Cargo.lock | cut -d' ' -f1)

# Build test command
if [ $# -eq 0 ]; then
    TEST_CMD="cargo test --release --test visual_tests -- --ignored --nocapture --test-threads=1"
else
    TEST_CMD="cargo test --release --test visual_tests -- $* --ignored --nocapture --test-threads=1"
fi

# Run with hash passed to container
exec docker compose run --rm \
    -e HOST_CARGO_LOCK_HASH="$HOST_HASH" \
    -e USIT_STRICT_SYNC="${USIT_STRICT_SYNC:-0}" \
    visual-tests $TEST_CMD
