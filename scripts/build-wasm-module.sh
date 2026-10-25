#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
CANONICAL_WASM_TOOLCHAIN="nightly-2025-12-11"
CANONICAL_WASM_TARGET="wasm32-unknown-unknown"
CANONICAL_WASM_BUILD_STD_COMPONENTS="std,panic_abort"
CANONICAL_WASM_BUILD_STD_FEATURES=""
CANONICAL_DOCKER_PLATFORM="linux/amd64"
CANONICAL_CONTAINER_PLATFORM_TOKEN="linux-x86_64"
CONTAINER_WORKSPACE_DIR="/workspace"
DEFAULT_BUILDER_IMAGE="oasis7/wasm-builder:${CANONICAL_WASM_TOOLCHAIN}"
DEFAULT_BUILDER_IMAGE_DIGEST="sha256:f3538a8039074c0b074946ae9d1803f6c74b823aee9152d52584111844ee1ed8"
DEFAULT_BUILDER_DOCKERFILE="$ROOT_DIR/docker/wasm-builder/Dockerfile"
DEFAULT_CANONICALIZER_VERSION="strip-custom-sections-v1"

wasm_env_key() {
  local suffix=$1
  printf 'OASIS7_WASM_%s\n' "$suffix"
}

wasm_env_or_default() {
  local suffix=$1
  local default_value=${2-}
  local key
  key=$(wasm_env_key "$suffix")
  if [[ -n "${!key+x}" ]]; then
    printf '%s\n' "${!key}"
  else
    printf '%s\n' "$default_value"
  fi
}

set_wasm_env() {
  local suffix=$1
  local value=${2-}
  local key
  key=$(wasm_env_key "$suffix")
  export "$key=$value"
}

WASM_TOOLCHAIN="$(wasm_env_or_default TOOLCHAIN "$CANONICAL_WASM_TOOLCHAIN")"
WASM_TARGET="$(wasm_env_or_default TARGET "$CANONICAL_WASM_TARGET")"
WASM_BUILD_STD_ENABLED="$(wasm_env_or_default BUILD_STD "1")"
WASM_BUILD_STD_COMPONENTS="$(wasm_env_or_default BUILD_STD_COMPONENTS "$CANONICAL_WASM_BUILD_STD_COMPONENTS")"
WASM_BUILD_STD_FEATURES="$(wasm_env_or_default BUILD_STD_FEATURES "$CANONICAL_WASM_BUILD_STD_FEATURES")"
WASM_DETERMINISTIC_GUARD="$(wasm_env_or_default DETERMINISTIC_GUARD "1")"
WASM_BUILD_IN_CONTAINER="$(wasm_env_or_default BUILD_IN_CONTAINER "0")"
WASM_SOURCE_WORKSPACE_DIR="$(wasm_env_or_default SOURCE_WORKSPACE_DIR "$CONTAINER_WORKSPACE_DIR")"
WASM_BUILDER_IMAGE="$(wasm_env_or_default BUILDER_IMAGE "$DEFAULT_BUILDER_IMAGE")"
WASM_BUILDER_IMAGE_DIGEST="$(wasm_env_or_default BUILDER_IMAGE_DIGEST "$DEFAULT_BUILDER_IMAGE_DIGEST")"
WASM_BUILDER_DOCKERFILE="$(wasm_env_or_default BUILDER_DOCKERFILE "$DEFAULT_BUILDER_DOCKERFILE")"
WASM_BUILDER_AUTO_BUILD="$(wasm_env_or_default BUILDER_AUTO_BUILD "1")"
WASM_BUILDER_ALLOW_IMAGE_ID_MISMATCH="$(wasm_env_or_default BUILDER_ALLOW_IMAGE_ID_MISMATCH "0")"
WASM_BUILD_SUITE_BIN="$(wasm_env_or_default BUILD_SUITE_BIN "")"
WASM_CANONICALIZER_VERSION="$(wasm_env_or_default CANONICALIZER_VERSION "$DEFAULT_CANONICALIZER_VERSION")"

RUSTFLAGS_EFFECTIVE=""
SELECTED_BUILDER_IMAGE_ID=""

is_truthy() {
  local value="$1"
  case "$value" in
    1|[Tt][Rr][Uu][Ee]|[Yy][Ee][Ss]|[Oo][Nn]) return 0 ;;
    *) return 1 ;;
  esac
}

starts_with_path_prefix() {
  local candidate="$1"
  local prefix="$2"
  [[ "$candidate" == "$prefix" || "$candidate" == "$prefix/"* ]]
}

normalize_path() {
  local raw_path="$1"
  local base_dir="$2"
  local joined_path
  if [[ "$raw_path" == /* ]]; then
    joined_path="$raw_path"
  else
    joined_path="$base_dir/$raw_path"
  fi

  local old_ifs="$IFS"
  IFS='/'
  local -a pieces normalized=()
  read -r -a pieces <<< "$joined_path"
  IFS="$old_ifs"

  local piece
  for piece in "${pieces[@]}"; do
    case "$piece" in
      ""|".")
        continue
        ;;
      "..")
        if (( ${#normalized[@]} == 0 )); then
          echo "error: path escapes filesystem root: $raw_path" >&2
          exit 2
        fi
        unset 'normalized[${#normalized[@]}-1]'
        ;;
      *)
        normalized+=("$piece")
        ;;
    esac
  done

  local result="/"
  local index
  for ((index = 0; index < ${#normalized[@]}; index += 1)); do
    result+="${normalized[$index]}"
    if (( index + 1 < ${#normalized[@]} )); then
      result+="/"
    fi
  done
  printf '%s\n' "$result"
}

containerize_path() {
  local workspace_root="$1"
  local host_path="$2"
  if [[ "$host_path" == "$workspace_root" ]]; then
    printf '%s\n' "$CONTAINER_WORKSPACE_DIR"
    return 0
  fi
  local suffix="${host_path#"$workspace_root"/}"
  printf '%s/%s\n' "$CONTAINER_WORKSPACE_DIR" "$suffix"
}

require_unset_env() {
  local key="$1"
  if [[ -n "${!key:-}" ]]; then
    echo "error: deterministic wasm build forbids inherited $key" >&2
    exit 1
  fi
}

require_env_value_or_unset() {
  local key="$1"
  local expected="$2"
  local actual="${!key:-}"
  if [[ -z "$actual" ]]; then
    return 0
  fi
  if [[ "$actual" != "$expected" ]]; then
    echo "error: deterministic wasm build requires $key=$expected when set (got $actual)" >&2
    exit 1
  fi
}

enforce_deterministic_build_inputs() {
  if [[ "$WASM_TOOLCHAIN" != "$CANONICAL_WASM_TOOLCHAIN" ]]; then
    echo "error: deterministic wasm build requires OASIS7_WASM_TOOLCHAIN=$CANONICAL_WASM_TOOLCHAIN (got $WASM_TOOLCHAIN)" >&2
    exit 1
  fi
  if [[ "$WASM_TARGET" != "$CANONICAL_WASM_TARGET" ]]; then
    echo "error: deterministic wasm build requires OASIS7_WASM_TARGET=$CANONICAL_WASM_TARGET (got $WASM_TARGET)" >&2
    exit 1
  fi
  if ! is_truthy "$WASM_BUILD_STD_ENABLED"; then
    echo "error: deterministic wasm build requires OASIS7_WASM_BUILD_STD=1" >&2
    exit 1
  fi
  if [[ "$WASM_BUILD_STD_COMPONENTS" != "$CANONICAL_WASM_BUILD_STD_COMPONENTS" ]]; then
    echo "error: deterministic wasm build requires OASIS7_WASM_BUILD_STD_COMPONENTS=$CANONICAL_WASM_BUILD_STD_COMPONENTS (got $WASM_BUILD_STD_COMPONENTS)" >&2
    exit 1
  fi
  if [[ "$WASM_BUILD_STD_FEATURES" != "$CANONICAL_WASM_BUILD_STD_FEATURES" ]]; then
    echo "error: deterministic wasm build requires OASIS7_WASM_BUILD_STD_FEATURES to be empty (got $WASM_BUILD_STD_FEATURES)" >&2
    exit 1
  fi

  require_unset_env "RUSTFLAGS"
  require_unset_env "CARGO_ENCODED_RUSTFLAGS"
  require_unset_env "CARGO_BUILD_RUSTFLAGS"
  require_unset_env "CARGO_TARGET_DIR"
  require_unset_env "RUSTC_BOOTSTRAP"
  require_env_value_or_unset "RUSTUP_TOOLCHAIN" "$CANONICAL_WASM_TOOLCHAIN"
}

prepare_container_build_std() {
  # Select the canonical toolchain before rustup resolves the workspace override.
  export RUSTUP_TOOLCHAIN="$WASM_TOOLCHAIN"
  if ! command -v rustup >/dev/null 2>&1; then
    echo "error: containerized wasm build requires rustup in PATH" >&2
    exit 1
  fi
  if ! rustup toolchain list | awk '{print $1}' | grep -Eq "^${WASM_TOOLCHAIN}($|-)"; then
    echo "error: container builder image is missing toolchain $WASM_TOOLCHAIN" >&2
    exit 1
  fi
  if ! rustup target list --toolchain "$WASM_TOOLCHAIN" --installed | grep -Fxq "$WASM_TARGET"; then
    echo "error: container builder image is missing target $WASM_TARGET for toolchain $WASM_TOOLCHAIN" >&2
    exit 1
  fi

  set_wasm_env "BUILD_STD" "1"
  set_wasm_env "BUILD_STD_COMPONENTS" "$WASM_BUILD_STD_COMPONENTS"
  set_wasm_env "BUILD_STD_FEATURES" "$WASM_BUILD_STD_FEATURES"
}

prepare_nightly_build_std_impl() {
  if ! command -v rustup >/dev/null 2>&1; then
    echo "error: OASIS7_WASM_BUILD_STD=1 requires rustup in PATH" >&2
    exit 1
  fi

  if ! rustup toolchain list | awk '{print $1}' | grep -Eq "^${WASM_TOOLCHAIN}($|-)"; then
    rustup toolchain install "$WASM_TOOLCHAIN" --profile minimal --component rust-src >/dev/null
  fi

  if ! rustup target list --toolchain "$WASM_TOOLCHAIN" --installed | grep -Fxq "$WASM_TARGET"; then
    rustup target add "$WASM_TARGET" --toolchain "$WASM_TOOLCHAIN" >/dev/null
  fi

  export RUSTUP_TOOLCHAIN="$WASM_TOOLCHAIN"
  set_wasm_env "BUILD_STD" "1"
  set_wasm_env "BUILD_STD_COMPONENTS" "$WASM_BUILD_STD_COMPONENTS"
  set_wasm_env "BUILD_STD_FEATURES" "$WASM_BUILD_STD_FEATURES"
}

prepare_nightly_build_std() {
  local rustup_home_dir="${RUSTUP_HOME:-}"
  if [[ -z "$rustup_home_dir" && -n "${HOME:-}" ]]; then
    rustup_home_dir="$HOME/.rustup"
  fi
  if [[ -z "$rustup_home_dir" ]]; then
    rustup_home_dir="/tmp"
  fi
  mkdir -p "$rustup_home_dir"
  local lock_file="$rustup_home_dir/.oasis7-wasm-rustup.lock"

  if command -v flock >/dev/null 2>&1; then
    exec 9>"$lock_file"
    flock 9
    local status=0
    prepare_nightly_build_std_impl || status=$?
    flock -u 9 || true
    exec 9>&-
    return "$status"
  fi

  local lock_dir="${lock_file}.d"
  local waited_s=0
  while ! mkdir "$lock_dir" 2>/dev/null; do
    sleep 1
    waited_s=$((waited_s + 1))
    if (( waited_s >= 180 )); then
      echo "error: timed out waiting for rustup lock: $lock_file" >&2
      return 1
    fi
  done

  local status=0
  prepare_nightly_build_std_impl || status=$?
  rmdir "$lock_dir" >/dev/null 2>&1 || true
  return "$status"
}

append_rustflag_once() {
  local flag="$1"
  if [[ " $RUSTFLAGS_EFFECTIVE " == *" $flag "* ]]; then
    return 0
  fi
  if [[ -n "$RUSTFLAGS_EFFECTIVE" ]]; then
    RUSTFLAGS_EFFECTIVE="$RUSTFLAGS_EFFECTIVE $flag"
  else
    RUSTFLAGS_EFFECTIVE="$flag"
  fi
}

run_build_suite() {
  if [[ -n "$WASM_BUILD_SUITE_BIN" ]]; then
    env -u RUSTC_WRAPPER "$WASM_BUILD_SUITE_BIN" build "$@"
    return 0
  fi

  env -u RUSTC_WRAPPER cargo run \
    --quiet \
    --manifest-path "$ROOT_DIR/tools/wasm_build_suite/Cargo.toml" \
    -- \
    build "$@"
}

run_native_build() {
  if is_truthy "$WASM_DETERMINISTIC_GUARD"; then
    enforce_deterministic_build_inputs
  fi

  if is_truthy "$WASM_BUILD_STD_ENABLED"; then
    if is_truthy "$WASM_BUILD_IN_CONTAINER"; then
      prepare_container_build_std
    else
      prepare_nightly_build_std
    fi
  else
    set_wasm_env "BUILD_STD" "0"
  fi

  if ! is_truthy "$WASM_DETERMINISTIC_GUARD"; then
    RUSTFLAGS_EFFECTIVE="${RUSTFLAGS:-}"
  fi

  if [[ -n "${HOME:-}" ]]; then
    append_rustflag_once "--remap-path-prefix=$HOME/.cargo=/cargo"
    append_rustflag_once "--remap-path-prefix=$HOME/.rustup=/rustup"
  fi
  append_rustflag_once "--remap-path-prefix=$WASM_SOURCE_WORKSPACE_DIR=$CONTAINER_WORKSPACE_DIR"

  RUSTUP_HOME_DIR="${RUSTUP_HOME:-}"
  if [[ -z "$RUSTUP_HOME_DIR" && -n "${HOME:-}" ]]; then
    RUSTUP_HOME_DIR="$HOME/.rustup"
  fi
  if [[ -n "$RUSTUP_HOME_DIR" && -d "$RUSTUP_HOME_DIR/toolchains" ]]; then
    local toolchain_dir
    for toolchain_dir in "$RUSTUP_HOME_DIR"/toolchains/*; do
      [[ -d "$toolchain_dir" ]] || continue
      append_rustflag_once "--remap-path-prefix=$toolchain_dir=/rustup/toolchain"
    done
  fi

  RUSTC_SYSROOT="$(rustc --print sysroot 2>/dev/null || true)"
  if [[ -n "$RUSTC_SYSROOT" ]]; then
    append_rustflag_once "--remap-path-prefix=$RUSTC_SYSROOT=/rustup/toolchain"
  fi
  export RUSTFLAGS="$RUSTFLAGS_EFFECTIVE"

  export CARGO_INCREMENTAL=0
  export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-0}"
  export TZ=UTC
  export LANG=C
  export LC_ALL=C

  run_build_suite "$@"
}

require_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "error: docker is required; canonical wasm builds no longer support host-native fallback" >&2
    exit 1
  fi
  if ! docker info >/dev/null 2>&1; then
    echo "error: docker daemon is not available; canonical wasm builds require docker" >&2
    exit 1
  fi
}

build_local_builder_image() {
  if [[ ! -f "$WASM_BUILDER_DOCKERFILE" ]]; then
    echo "error: wasm builder Dockerfile not found: $WASM_BUILDER_DOCKERFILE" >&2
    exit 1
  fi
  if ! command -v docker >/dev/null 2>&1; then
    echo "error: docker is required to build the canonical wasm builder image" >&2
    exit 1
  fi

  if docker buildx version >/dev/null 2>&1; then
    docker buildx build \
      --load \
      --platform "$CANONICAL_DOCKER_PLATFORM" \
      --build-arg "WASM_TOOLCHAIN=$CANONICAL_WASM_TOOLCHAIN" \
      --build-arg "WASM_TARGET=$CANONICAL_WASM_TARGET" \
      --tag "$DEFAULT_BUILDER_IMAGE" \
      --file "$WASM_BUILDER_DOCKERFILE" \
      "$ROOT_DIR"
    return 0
  fi

  docker build \
    --platform "$CANONICAL_DOCKER_PLATFORM" \
    --build-arg "WASM_TOOLCHAIN=$CANONICAL_WASM_TOOLCHAIN" \
    --build-arg "WASM_TARGET=$CANONICAL_WASM_TARGET" \
    --tag "$DEFAULT_BUILDER_IMAGE" \
    --file "$WASM_BUILDER_DOCKERFILE" \
    "$ROOT_DIR"
}

ensure_builder_image() {
  local actual_image_id=""
  if actual_image_id="$(docker image inspect "$WASM_BUILDER_IMAGE" --format '{{.Id}}' 2>/dev/null)"; then
    if [[ -z "$WASM_BUILDER_IMAGE_DIGEST" || "$actual_image_id" == "$WASM_BUILDER_IMAGE_DIGEST" ]]; then
      SELECTED_BUILDER_IMAGE_ID="$actual_image_id"
      return 0
    fi

    echo "warning: builder image digest mismatch for $WASM_BUILDER_IMAGE" >&2
    echo "warning: expected $WASM_BUILDER_IMAGE_DIGEST but found $actual_image_id" >&2

    if [[ "$WASM_BUILDER_IMAGE" != "$DEFAULT_BUILDER_IMAGE" ]]; then
      echo "error: configured builder image does not match OASIS7_WASM_BUILDER_IMAGE_DIGEST" >&2
      exit 1
    fi

    if is_truthy "$WASM_BUILDER_ALLOW_IMAGE_ID_MISMATCH"; then
      echo "warning: reusing canonical builder image despite Docker image-id variance" >&2
      SELECTED_BUILDER_IMAGE_ID="$actual_image_id"
      return 0
    fi

    if ! is_truthy "$WASM_BUILDER_AUTO_BUILD"; then
      echo "error: canonical wasm builder image has stale digest: $WASM_BUILDER_IMAGE" >&2
      echo "hint: set OASIS7_WASM_BUILDER_AUTO_BUILD=1 or rebuild the image manually" >&2
      exit 1
    fi

    # Building retags the result without deleting an image another invocation
    # has already selected. Never create a missing-tag window before publication.
  else
    if [[ "$WASM_BUILDER_IMAGE" != "$DEFAULT_BUILDER_IMAGE" ]]; then
      echo "error: configured builder image is missing: $WASM_BUILDER_IMAGE" >&2
      echo "hint: pre-build or pull that image before invoking this script" >&2
      exit 1
    fi

    if ! is_truthy "$WASM_BUILDER_AUTO_BUILD"; then
      echo "error: canonical wasm builder image is missing: $WASM_BUILDER_IMAGE" >&2
      echo "hint: set OASIS7_WASM_BUILDER_AUTO_BUILD=1 or build the image manually" >&2
      exit 1
    fi
  fi

  build_local_builder_image

  actual_image_id="$(docker image inspect "$WASM_BUILDER_IMAGE" --format '{{.Id}}')"
  SELECTED_BUILDER_IMAGE_ID="$actual_image_id"
  if [[ -n "$WASM_BUILDER_IMAGE_DIGEST" ]]; then
    if [[ "$actual_image_id" != "$WASM_BUILDER_IMAGE_DIGEST" ]]; then
      echo "warning: rebuilt canonical wasm builder image id differs from configured recipe digest" >&2
      echo "warning: configured recipe digest $WASM_BUILDER_IMAGE_DIGEST; rebuilt image id $actual_image_id" >&2
    fi
  fi
}

builder_image_digest() {
  # Receipt provenance retains the recipe digest; execution uses the separately
  # captured immutable local ID. Do not re-resolve a mutable tag here.
  local actual_image_id="$1"
  if [[ -n "$WASM_BUILDER_IMAGE_DIGEST" && "$actual_image_id" != "$WASM_BUILDER_IMAGE_DIGEST" ]]; then
    echo "warning: builder image id differs from configured recipe digest for $WASM_BUILDER_IMAGE" >&2
    echo "warning: configured recipe digest $WASM_BUILDER_IMAGE_DIGEST; image id $actual_image_id" >&2
    printf '%s\n' "$WASM_BUILDER_IMAGE_DIGEST"
    return 0
  fi
  printf '%s\n' "$actual_image_id"
}

parse_host_paths() {
  local cwd="$1"
  shift

  HOST_MANIFEST_PATH=""
  HOST_OUT_DIR=""

  local expect_manifest=0
  local expect_out_dir=0
  local arg
  for arg in "$@"; do
    if (( expect_manifest == 1 )); then
      HOST_MANIFEST_PATH="$(normalize_path "$arg" "$cwd")"
      expect_manifest=0
      continue
    fi
    if (( expect_out_dir == 1 )); then
      HOST_OUT_DIR="$(normalize_path "$arg" "$cwd")"
      expect_out_dir=0
      continue
    fi

    case "$arg" in
      --manifest-path)
        expect_manifest=1
        ;;
      --out-dir)
        expect_out_dir=1
        ;;
    esac
  done

  if (( expect_manifest == 1 )); then
    echo "error: --manifest-path requires a value" >&2
    exit 2
  fi
  if (( expect_out_dir == 1 )); then
    echo "error: --out-dir requires a value" >&2
    exit 2
  fi
  if [[ -z "$HOST_MANIFEST_PATH" ]]; then
    echo "error: --manifest-path is required for canonical docker build" >&2
    exit 2
  fi
}

translate_args_for_container() {
  local container_manifest_path="$1"
  local container_out_dir="$2"
  shift 2

  local -a translated=()
  local expect_manifest=0
  local expect_out_dir=0
  local saw_out_dir=0
  local arg
  for arg in "$@"; do
    if (( expect_manifest == 1 )); then
      translated+=("$container_manifest_path")
      expect_manifest=0
      continue
    fi
    if (( expect_out_dir == 1 )); then
      translated+=("$container_out_dir")
      expect_out_dir=0
      saw_out_dir=1
      continue
    fi

    case "$arg" in
      --manifest-path)
        translated+=("$arg")
        expect_manifest=1
        ;;
      --out-dir)
        translated+=("$arg")
        expect_out_dir=1
        ;;
      *)
        translated+=("$arg")
        ;;
    esac
  done

  if (( expect_manifest == 1 )); then
    echo "error: --manifest-path requires a value" >&2
    exit 2
  fi
  if (( expect_out_dir == 1 )); then
    echo "error: --out-dir requires a value" >&2
    exit 2
  fi
  if (( saw_out_dir == 0 )); then
    translated+=(--out-dir "$container_out_dir")
  fi

  TRANSLATED_ARGS=("${translated[@]}")
}

run_docker_wrapper() {
  local host_cwd
  host_cwd="$(pwd -P)"
  parse_host_paths "$host_cwd" "$@"

  local host_workspace_root="$host_cwd"
  if starts_with_path_prefix "$HOST_MANIFEST_PATH" "$ROOT_DIR"; then
    host_workspace_root="$ROOT_DIR"
  fi
  if [[ -z "$HOST_OUT_DIR" ]]; then
    HOST_OUT_DIR="$host_workspace_root/.tmp/wasm-build-suite"
  fi

  if ! starts_with_path_prefix "$HOST_MANIFEST_PATH" "$host_workspace_root"; then
    echo "error: manifest path must stay within the canonical workspace root" >&2
    echo "hint: run this script from the source workspace root or use a manifest under $ROOT_DIR" >&2
    exit 2
  fi
  if ! starts_with_path_prefix "$HOST_OUT_DIR" "$host_workspace_root"; then
    echo "error: out-dir must stay within the canonical workspace root" >&2
    echo "hint: choose an out-dir under $host_workspace_root" >&2
    exit 2
  fi

  local container_manifest_path
  local container_out_dir
  container_manifest_path="$(containerize_path "$host_workspace_root" "$HOST_MANIFEST_PATH")"
  container_out_dir="$(containerize_path "$host_workspace_root" "$HOST_OUT_DIR")"
  translate_args_for_container "$container_manifest_path" "$container_out_dir" "$@"

  mkdir -p "$HOST_OUT_DIR"
  require_docker
  ensure_builder_image
  if [[ ! "$SELECTED_BUILDER_IMAGE_ID" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    echo "error: docker returned an invalid selected builder image id" >&2
    exit 1
  fi
  local builder_image_digest_value
  builder_image_digest_value="$(builder_image_digest "$SELECTED_BUILDER_IMAGE_ID")"

  docker run \
    --rm \
    --platform "$CANONICAL_DOCKER_PLATFORM" \
    --user "$(id -u):$(id -g)" \
    --workdir "$CONTAINER_WORKSPACE_DIR" \
    --mount "type=bind,src=$host_workspace_root,dst=$CONTAINER_WORKSPACE_DIR" \
    --env OASIS7_WASM_BUILD_IN_CONTAINER=1 \
    --env "OASIS7_WASM_SOURCE_WORKSPACE_DIR=$CONTAINER_WORKSPACE_DIR" \
    --env "OASIS7_WASM_TOOLCHAIN=$WASM_TOOLCHAIN" \
    --env "OASIS7_WASM_TARGET=$WASM_TARGET" \
    --env "OASIS7_WASM_BUILD_STD=$WASM_BUILD_STD_ENABLED" \
    --env "OASIS7_WASM_BUILD_STD_COMPONENTS=$WASM_BUILD_STD_COMPONENTS" \
    --env "OASIS7_WASM_BUILD_STD_FEATURES=$WASM_BUILD_STD_FEATURES" \
    --env "OASIS7_WASM_DETERMINISTIC_GUARD=$WASM_DETERMINISTIC_GUARD" \
    --env "OASIS7_WASM_CANONICAL_CONTAINER_PLATFORM=$CANONICAL_CONTAINER_PLATFORM_TOKEN" \
    --env "OASIS7_WASM_CANONICALIZER_VERSION=$WASM_CANONICALIZER_VERSION" \
    --env "OASIS7_WASM_BUILDER_IMAGE_REF=$WASM_BUILDER_IMAGE" \
    --env "OASIS7_WASM_BUILDER_IMAGE_DIGEST=$builder_image_digest_value" \
    --env HOME=/tmp/oasis7-home \
    --env CARGO_HOME=/tmp/oasis7-cargo-home \
    --env RUSTUP_HOME=/rustup \
    "$SELECTED_BUILDER_IMAGE_ID" \
    "${TRANSLATED_ARGS[@]}"
}

if is_truthy "$WASM_BUILD_IN_CONTAINER"; then
  run_native_build "$@"
else
  run_docker_wrapper "$@"
fi
