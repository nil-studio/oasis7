#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
python3 prepare.test.py
image=oasis7-ci-ecs-oss-publisher:candidate
# Only this new private-base pull receives a read-only package token. The
# temporary client config is not in the build context or copied into the image.
export DOCKER_CONFIG
DOCKER_CONFIG=$(mktemp -d)
trap 'rm -rf -- "$DOCKER_CONFIG"' EXIT
printf '%s' "$GHCR_TOKEN" | docker login ghcr.io -u "$GITHUB_ACTOR" --password-stdin
unset GHCR_TOKEN
docker build --platform linux/amd64 --target ecs-oss-publisher \
  --label "org.opencontainers.image.revision=$(git rev-parse HEAD)" \
  --label org.opencontainers.image.source=https://github.com/nil-studio/oasis7 \
  --tag "$image" .
docker run --rm --platform linux/amd64 --network none --user 1001:1001 \
  --env HOME=/tmp --env PYTHONDONTWRITEBYTECODE=1 \
  "$image" bash -euo pipefail -c '
    test "$(command -v python3)" = /opt/oasis7-oss/bin/python3
    python3 /opt/oasis7-oss-proof/smoke.py
    /usr/local/bin/node -e '\''require("node:assert/strict").equal(process.versions.node.split(".")[0], "24")'\''
    test ! -w /usr/local/bin/gh
    gh --version | grep -E "^gh version 2[.]105[.]0( |$)"
  '
