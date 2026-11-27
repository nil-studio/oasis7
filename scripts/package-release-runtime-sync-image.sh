#!/usr/bin/env bash
set -euo pipefail

image='ghcr.io/nil-studio/oasis7/ci-rust-ordinary-builder@sha256:714b3f428097659c56639b37083269f04a67f1dcbde56cdcef501a7b18d3c7a8'
workspace=/home/runner/work/oasis7/oasis7
socket=/var/run/docker.sock

# The package plan already verifies the protected tag and its exact full-CI receipt.
# Only these hosted package entrypoints may forward the existing daemon into the image.
eligible_context() {
  [[ "${GITHUB_REPOSITORY:-}" == nil-studio/oasis7 &&
     "${GITHUB_JOB:-}" == release_gate_runtime_sync &&
     "${RUNNER_ENVIRONMENT:-}" == github-hosted &&
     "${RUNNER_OS:-}" == Linux && "${RUNNER_ARCH:-}" == X64 ]] || return 1
  case "${GITHUB_EVENT_NAME:-}:${GITHUB_WORKFLOW_REF:-}" in
    workflow_dispatch:nil-studio/oasis7/.github/workflows/release-packages.yml@refs/heads/main|\
    workflow_dispatch:nil-studio/oasis7/.github/workflows/mainnet-packages.yml@refs/heads/main)
      [[ "${GITHUB_REF:-}" == refs/heads/main &&
         "${GITHUB_WORKFLOW_SHA:-}" =~ ^[0-9a-f]{40}$ &&
         "${GITHUB_SHA:-}" == "$GITHUB_WORKFLOW_SHA" ]]
      ;;
    push:nil-studio/oasis7/.github/workflows/release-packages.yml@refs/tags/*)
      [[ "${GITHUB_REF:-}" == refs/tags/* && "$GITHUB_WORKFLOW_REF" == *@"$GITHUB_REF" &&
         "${GITHUB_SHA:-}" == "${GITHUB_WORKFLOW_SHA:-}" && "${GITHUB_SHA:-}" == "$EXPECTED_SHA" ]]
      ;;
    *) return 1 ;;
  esac
}

compatible_source() {
  [[ "$EXPECTED_SHA" =~ ^[0-9a-f]{40}$ && "$(git rev-parse HEAD)" == "$EXPECTED_SHA" ]] || return 1
  python3 -I -c '
import pathlib
import sys
import tomllib

path = pathlib.Path("rust-toolchain.toml")
if path.is_symlink() or not path.is_file() or pathlib.Path("rust-toolchain").exists():
    sys.exit(1)
with path.open("rb") as handle:
    toolchain = tomllib.load(handle).get("toolchain", {})
sys.exit(0 if toolchain == {
    "channel": "1.96.0",
    "components": ["rustfmt", "clippy"],
    "targets": ["wasm32-unknown-unknown"],
} else 1)
'
}

if [[ "${1:-}" == --select ]]; then
  selected=false
  if eligible_context && compatible_source; then
    selected=true
  fi
  printf 'enabled=%s\n' "$selected" >>"$GITHUB_OUTPUT"
  exit
fi
[[ $# == 0 ]]
eligible_context
compatible_source
[[ "$(id -u):$(id -g)" == 1001:1001 && "$(pwd -P)" == "$workspace" &&
   "${GITHUB_WORKSPACE:-}" == "$workspace" && -S "$socket" ]]
[[ -z "${DOCKER_HOST:-}${DOCKER_CONTEXT:-}${DOCKER_TLS_VERIFY:-}${DOCKER_CERT_PATH:-}" ]]
linker=/opt/oasis7-mold/3.0.0-v1/bin/oasis7-mold-gcc
[[ "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER:-}" == "$linker" && -x "$linker" ]]
# Credentials remain in the runner-owned configuration used for the single pull.
scratch=$(mktemp -d "$RUNNER_TEMP/oasis7-runtime-sync.XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
export DOCKER_CONFIG="$scratch/docker"
mkdir -m 700 "$DOCKER_CONFIG" "$scratch/home"
daemon=(docker --host unix://"$socket")
socket_group=$(stat -c %g "$socket")
[[ "$socket_group" =~ ^[1-9][0-9]*$ && "$(stat -c %u "$socket")" == 0 ]]
[[ " $(id -G) " == *" $socket_group "* ]]
server=$("${daemon[@]}" info --format '{{.ID}}')
[[ -n "$server" ]]
printf '%s' "$GHCR_TOKEN" | "${daemon[@]}" login ghcr.io --username "$GITHUB_ACTOR" --password-stdin
unset GHCR_TOKEN
time "${daemon[@]}" pull "$image"
# Pass only the original build settings. The inner canonical builder still uses
# the original daemon and the physical bind source selected by build-wasm-module.
"${daemon[@]}" run --rm --init --pull never --platform linux/amd64 --interactive \
  --user 1001:1001 --group-add "$socket_group" \
  --mount "type=bind,source=$workspace,target=$workspace" \
  --mount "type=bind,source=$socket,target=$socket" \
  --mount "type=bind,source=${linker%/bin/*},target=${linker%/bin/*},readonly" \
  --mount "type=bind,source=$scratch/home,target=/oasis7-runtime-sync" \
  --workdir "$workspace" --env HOME=/oasis7-runtime-sync \
  --env CARGO_HOME=/oasis7-runtime-sync/cargo --env CARGO_TARGET_DIR=/oasis7-runtime-sync/target \
  --env RUSTUP_AUTO_INSTALL=0 --env CI=true --env EXPECTED_SHA \
  --env "EXPECTED_WORKSPACE_ID=$(stat -c '%d:%i' "$workspace")" --env "EXPECTED_DOCKER_SERVER=$server" \
  --env CARGO_TERM_COLOR --env CARGO_INCREMENTAL --env CARGO_NET_RETRY \
  --env CARGO_REGISTRIES_CRATES_IO_PROTOCOL --env CARGO_PROFILE_DEV_DEBUG --env CARGO_PROFILE_TEST_DEBUG \
  --env CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER --env RUST_TOOLCHAIN \
  --env OASIS7_WASM_TOOLCHAIN --env OASIS7_WASM_TARGET \
  --env OASIS7_WASM_BUILD_STD --env OASIS7_WASM_BUILD_STD_COMPONENTS \
  "$image" bash --noprofile --norc -e -o pipefail -c '
    test "$(id -u):$(id -g)" = 1001:1001
    test "$(stat -c "%d:%i" "$PWD")" = "$EXPECTED_WORKSPACE_ID"
    test "$(git rev-parse HEAD)" = "$EXPECTED_SHA"
    test -z "${OASIS7_WASM_BUILD_IN_CONTAINER+x}"
    test "$(docker info --format "{{.ID}}")" = "$EXPECTED_DOCKER_SERVER"
    docker buildx version
    test "$(rustc --version | cut -d " " -f 2)" = "$RUST_TOOLCHAIN"
    rustup target list --toolchain "$RUST_TOOLCHAIN" --installed | grep -Fx wasm32-unknown-unknown
    rustup target list --toolchain "$OASIS7_WASM_TOOLCHAIN" --installed | grep -Fx "$OASIS7_WASM_TARGET"
    rustup component list --toolchain "$OASIS7_WASM_TOOLCHAIN" --installed | grep -Fx rust-src
    pkg-config --exists wayland-client xkbcommon x11 xi xcursor xrandr xinerama alsa libudev
    jq --version
    printf "int main(void) { return 0; }\n" |
      "$CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER" -x c - -o "$HOME/mold-smoke"
    readelf -p .comment "$HOME/mold-smoke" | grep -F "mold 3.0.0"
    "$HOME/mold-smoke"
    exec bash --noprofile --norc -e -o pipefail -s
  '
