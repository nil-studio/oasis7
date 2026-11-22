#!/usr/bin/env bash
set -euo pipefail

# Compatibility entrypoint for legacy hooks. Ordinary local commits are a silent
# no-op so internal steps can be checkpointed cheaply on one task branch; run
# cargo-dev.sh / check-local.py before batch-pushing, and rely on CI for PR gates.
exit 0
