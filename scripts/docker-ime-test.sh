#!/bin/bash
set -euo pipefail

# Share the canonical compositor setup and cleanup with visual tests.
exec /docker-test.sh cargo test --release --test input_method_tests -- --nocapture --test-threads=1
