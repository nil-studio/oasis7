#!/usr/bin/env bash
# Build only the canonical Provider on the trusted hosted daemon; transport the sealed bundle.
set -euo pipefail
source_sha=${1:?expected trusted source}
image=ghcr.io/nil-studio/oasis7/ci-rust-full-regression@sha256:32f8b8cc2b07729362df6f1bceba09ff4ac1659e4932e41d88a06b3bc5d8d427
root=/home/runner/work/oasis7/oasis7
temp=/home/runner/work/_temp
socket=/var/run/docker.sock
test "$GITHUB_ACTIONS" = true
test "$GITHUB_REPOSITORY" = nil-studio/oasis7
test "$GITHUB_REF" = refs/heads/main
test "$GITHUB_WORKFLOW_REF" = nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main
test "$GITHUB_JOB" = full-regression-provider
test "$GITHUB_SHA" = "$GITHUB_WORKFLOW_SHA"
test "$source_sha" = "$GITHUB_SHA"
test "$RUNNER_ENVIRONMENT/$RUNNER_OS/$RUNNER_ARCH" = github-hosted/Linux/X64
test "$GITHUB_WORKSPACE" = "$root"
test "$RUNNER_TEMP" = "$temp"
test "$(id -u):$(id -g)" = 1001:1001
test -S "$socket"
test -z "${DOCKER_HOST:-}${DOCKER_CONTEXT:-}${DOCKER_TLS_VERIFY:-}${DOCKER_CERT_PATH:-}"
case "$GITHUB_EVENT_NAME" in
  schedule) ;;
  workflow_dispatch)
    python3 - "$GITHUB_EVENT_PATH" <<'PY_DISPATCH'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
assert path.is_absolute() and not path.is_symlink() and path.stat().st_size <= 1024 * 1024
inputs = json.loads(path.read_text())['inputs']
assert inputs.get('run_mode') == 'full' and not inputs.get('source_ref')
PY_DISPATCH
    ;;
  *) exit 2 ;;
esac
cd "$root"
test "$(git rev-parse HEAD)" = "$source_sha"
test -z "$(git status --porcelain)"
scratch=$(mktemp -d "$temp/provider.XXXXXX")
name="oasis7-full-provider-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT"
trap 'docker rm --force "$name" >/dev/null 2>&1 || true; rm -rf -- "$scratch"' EXIT
mkdir -p target
mkdir target/full-provider-transport
gid=$(stat -c %g "$socket")
docker run --name "$name" --rm --init --pull=never --platform linux/amd64 \
  --user 1001:1001 --group-add "$gid" --workdir "$root" \
  --mount "type=bind,source=$root,target=$root" \
  --mount "type=bind,source=$scratch,target=$scratch" \
  --mount "type=bind,source=$socket,target=$socket" \
  --env "HOME=$scratch" --env "TMPDIR=$scratch" --env "CARGO_HOME=$scratch/cargo" \
  --env "SOURCE_SHA=$source_sha" --env "ORIGIN_RUN=$GITHUB_RUN_ID" --env "ORIGIN_ATTEMPT=$GITHUB_RUN_ATTEMPT" \
  --env "WORKSPACE_ID=$(stat -c '%d:%i' "$root")" \
  --env CARGO_INCREMENTAL --env CARGO_PROFILE_DEV_DEBUG --env CARGO_PROFILE_TEST_DEBUG \
  --env CARGO_TERM_COLOR --env CARGO_NET_RETRY --env CARGO_REGISTRIES_CRATES_IO_PROTOCOL \
  "$image" bash -euo pipefail -c '
    test "$(stat -c "%d:%i" "$PWD")" = "$WORKSPACE_ID"
    test "$(git rev-parse HEAD)" = "$SOURCE_SHA"
    test "$(id -u):$(id -g)" = 1001:1001
    test -z "$(git status --porcelain)"
    python3 .github/actions/setup-linux-mold/setup.py --install-dir "$TMPDIR/mold"
    export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$TMPDIR/mold/bin/oasis7-mold-gcc"
    bundle=$(mktemp -d "$PWD/target/full-provider-build.XXXXXX")
    trap '\''rm -rf -- "$bundle"'\'' EXIT
    python3 scripts/ci-test-artifacts.py prepare-provider --repo-root "$PWD" --bundle "$bundle" --output "$bundle/environment.sh"
    python3 scripts/ci-trusted-full-provider.py pack --root "$PWD" --bundle "$bundle" \
      --source "$SOURCE_SHA" --run-id "$ORIGIN_RUN" --attempt "$ORIGIN_ATTEMPT" \
      --archive target/full-provider-transport/provider.tar
  '
digest=$(sha256sum target/full-provider-transport/provider.tar | cut -d ' ' -f1)
printf 'digest=%s\nattempt=%s\nsource=%s\n' "$digest" "$GITHUB_RUN_ATTEMPT" "$source_sha" >>"$GITHUB_OUTPUT"
