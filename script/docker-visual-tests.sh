#!/bin/bash
# script/docker-visual-tests.sh
#
# Wrapper for running Docker visual tests with staleness detection.
# Automatically rebuilds if Docker image doesn't match current git state.
#
# Usage:
#   ./script/docker-visual-tests.sh                    # Run all visual tests
#   ./script/docker-visual-tests.sh test_opacity_half  # Run specific test
#   USIT_STRICT_SYNC=1 ./script/docker-visual-tests.sh # Fail instead of rebuild
#   USIT_NO_REBUILD=1 ./script/docker-visual-tests.sh  # Warn but don't rebuild

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."

cd "$REPO_ROOT"

# Compute host state
HOST_CARGO_LOCK_HASH=$(sha256sum Cargo.lock | cut -d' ' -f1)
HOST_GIT_COMMIT=$(git rev-parse HEAD)
HOST_GIT_DIFF_HASH=$(git diff HEAD | sha256sum | cut -d' ' -f1)

# Check if image exists and get its state
IMAGE_STATE_OK=true
if docker compose images visual-tests -q 2>/dev/null | grep -q .; then
    # Image exists, check state files
    IMAGE_CARGO_HASH=$(docker compose run --rm --entrypoint cat visual-tests /app/.cargo-lock-hash 2>/dev/null || echo "missing")
    IMAGE_GIT_COMMIT=$(docker compose run --rm --entrypoint cat visual-tests /app/.git-commit 2>/dev/null || echo "missing")
    IMAGE_GIT_DIFF_HASH=$(docker compose run --rm --entrypoint cat visual-tests /app/.git-diff-hash 2>/dev/null || echo "missing")

    if [ "$HOST_CARGO_LOCK_HASH" != "$IMAGE_CARGO_HASH" ]; then
        echo "Stale: Cargo.lock changed"
        echo "  Host:  ${HOST_CARGO_LOCK_HASH:0:16}..."
        echo "  Image: ${IMAGE_CARGO_HASH:0:16}..."
        IMAGE_STATE_OK=false
    fi

    if [ "$HOST_GIT_COMMIT" != "$IMAGE_GIT_COMMIT" ]; then
        echo "Stale: Git commit changed"
        echo "  Host:  ${HOST_GIT_COMMIT:0:8}"
        echo "  Image: ${IMAGE_GIT_COMMIT:0:8}"
        IMAGE_STATE_OK=false
    fi

    if [ "$HOST_GIT_DIFF_HASH" != "$IMAGE_GIT_DIFF_HASH" ]; then
        echo "Stale: Uncommitted changes differ"
        echo "  Host:  ${HOST_GIT_DIFF_HASH:0:16}..."
        echo "  Image: ${IMAGE_GIT_DIFF_HASH:0:16}..."
        IMAGE_STATE_OK=false
    fi
else
    echo "Docker image not found, will build..."
    IMAGE_STATE_OK=false
fi

# Handle staleness
if [ "$IMAGE_STATE_OK" = false ]; then
    if [ "${USIT_STRICT_SYNC:-0}" = "1" ]; then
        echo "ERROR: USIT_STRICT_SYNC=1, aborting due to stale image"
        echo "Run: ./script/docker-visual-tests.sh (without USIT_STRICT_SYNC) to auto-rebuild"
        exit 1
    elif [ "${USIT_NO_REBUILD:-0}" = "1" ]; then
        echo "WARNING: Image is stale (USIT_NO_REBUILD=1, continuing anyway)"
    else
        echo "Rebuilding Docker image..."
        docker compose build \
            --build-arg GIT_COMMIT="$HOST_GIT_COMMIT" \
            --build-arg GIT_DIFF_HASH="$HOST_GIT_DIFF_HASH" \
            visual-tests
    fi
fi

# Build test command
if [ $# -eq 0 ]; then
    TEST_CMD="cargo test --release --test visual_tests -- --nocapture --test-threads=1"
else
    TEST_CMD="cargo test --release --test visual_tests -- $* --nocapture --test-threads=1"
fi

# Run tests
exec docker compose run --rm visual-tests $TEST_CMD
