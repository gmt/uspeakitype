#!/bin/bash
set -e
VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
TAG="v$VERSION"
EXISTING=$(git rev-parse --verify --quiet "${TAG}^{commit}" || true)
HEAD=$(git rev-parse HEAD)
if [ -z "$EXISTING" ]; then
    git tag -a "$TAG" -m "Release $VERSION"
    echo "Tagged $TAG"
    exit 0
elif [ "$EXISTING" = "$HEAD" ]; then
    exit 0  # Already tagged here, silent
elif git merge-base --is-ancestor "$EXISTING" HEAD; then
    exit 0  # Tag in our history, silent
else
    echo "Warning: Tag $TAG exists but is not ancestor of HEAD"
    echo "  Tagged: ${EXISTING:0:8}"
    echo "  HEAD:   ${HEAD:0:8}"
    exit 1
fi
