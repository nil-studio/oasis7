#!/usr/bin/env bash
# Private orchestration for build-builtin-wasm-modules.sh. Identity is always
# computed by the original Rust producer tools and checked again inside image.
candidate_batch_tool() {
  (cd "$_OASIS7_CB_ROOT" && env -u RUSTC_WRAPPER cargo run --quiet --locked -p wasm_build_suite -- "$@")
}

candidate_batch_path() {
  python3 - "$_OASIS7_CB_ROOT" "$1" "$2" <<'PY'
import pathlib, stat, sys
root = pathlib.Path(sys.argv[1])
path = pathlib.Path(sys.argv[2])
if '..' in path.parts or any(c in str(path) for c in ',\n\r'):
    raise SystemExit('error: candidate batch unsafe path')
path = path.absolute()
if path != root and root not in path.parents:
    raise SystemExit('error: candidate batch path must belong to producer workspace')
for ancestor in [*reversed(path.parents), path]:
    try:
        mode = ancestor.lstat().st_mode
    except FileNotFoundError:
        if sys.argv[3] != 'output':
            raise SystemExit('error: candidate batch input missing: ' + str(ancestor))
        continue
    if stat.S_ISLNK(mode) or (ancestor != path and not stat.S_ISDIR(mode)):
        raise SystemExit('error: candidate batch path ancestor must be a real directory')
    if ancestor == path and not (stat.S_ISREG(mode) if sys.argv[3] == 'manifest' else stat.S_ISDIR(mode)):
        raise SystemExit('error: candidate batch invalid path kind')
print(path)
PY
}

candidate_batch_close() {
  local status=0
  if [[ -n "${_OASIS7_CB_PIN:-}" ]]; then
    docker rm "$_OASIS7_CB_PIN" >/dev/null || status=$?
    _OASIS7_CB_PIN=""
  fi
  if [[ -n "${_OASIS7_CB_DIR:-}" ]]; then
    rm -rf -- "$_OASIS7_CB_DIR" || status=$?
    _OASIS7_CB_DIR=""
  fi
  return "$status"
}

candidate_batch_recipe_check() {
  candidate_batch_tool producer-recipe "$_OASIS7_CB_ROOT" "$_OASIS7_CB_DIR/current-recipe.json"
  cmp "$_OASIS7_CB_DIR/recipe.json" "$_OASIS7_CB_DIR/current-recipe.json"
}

candidate_batch_open() {
  _OASIS7_CB_ROOT="$1"
  _OASIS7_CB_DIR=""
  _OASIS7_CB_PIN=""
  _OASIS7_CB_IMAGE=""
  local key
  for key in OASIS7_WASM_BUILDER_IMAGE OASIS7_WASM_BUILDER_IMAGE_REF OASIS7_WASM_BUILDER_IMAGE_DIGEST OASIS7_WASM_BUILDER_DOCKERFILE OASIS7_WASM_BUILDER_ALLOW_IMAGE_ID_MISMATCH OASIS7_WASM_CANDIDATE_BUILDER_EVIDENCE OASIS7_WASM_BUILD_SUITE_BIN OASIS7_WASM_BUILD_IN_CONTAINER; do
    if [[ -n "${!key+x}" ]]; then
      echo "error: candidate batch forbids inherited $key" >&2
      return 2
    fi
  done
  _OASIS7_CB_ROOT="$(candidate_batch_path "$_OASIS7_CB_ROOT" root)"
  _OASIS7_CB_OUT="$(candidate_batch_path "$2" output)"
  command -v docker >/dev/null
  docker info >/dev/null
  mkdir -p "$_OASIS7_CB_OUT"
  _OASIS7_CB_DIR="$(mktemp -d "$_OASIS7_CB_OUT/.candidate-batch.XXXXXX")"
  trap 'candidate_batch_exit=$?; candidate_batch_close || candidate_batch_exit=$?; exit "$candidate_batch_exit"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  candidate_batch_tool producer-recipe "$_OASIS7_CB_ROOT" "$_OASIS7_CB_DIR/recipe.json"
  local recipe_hash image_ref
  recipe_hash="$(python3 - "$_OASIS7_CB_DIR/recipe.json" <<'PY'
import json,re,sys
value=json.load(open(sys.argv[1]))['producer_recipe_hash']
if not re.fullmatch('[0-9a-f]{64}',value):
    raise SystemExit('error: candidate batch invalid recipe digest')
print(value)
PY
)"
  image_ref="oasis7/wasm-builder:candidate-$recipe_hash"
  if ! docker image inspect "$image_ref" > "$_OASIS7_CB_DIR/inspect.json"; then
    local -a build=(docker build)
    if docker buildx version >/dev/null 2>&1; then
      build=(docker buildx build --load)
    fi
    "${build[@]}" --platform linux/amd64 \
      --build-arg WASM_TOOLCHAIN=nightly-2025-12-11 \
      --build-arg WASM_TARGET=wasm32-unknown-unknown \
      --label io.oasis7.candidate.purpose=candidate-build \
      --label "io.oasis7.candidate.producer-recipe=$recipe_hash" \
      --iidfile "$_OASIS7_CB_DIR/build.iid" --tag "$image_ref" \
      --file "$_OASIS7_CB_ROOT/docker/wasm-builder/Dockerfile" "$_OASIS7_CB_ROOT"
    local built
    built="$(cat "$_OASIS7_CB_DIR/build.iid")"
    [[ "$built" =~ ^sha256:[0-9a-f]{64}$ ]]
    docker image inspect "$built" > "$_OASIS7_CB_DIR/inspect.json"
  fi
  # Rust checks exact recipe/platform labels and records the actual immutable
  # image ID. This also applies when a prior genuine local build is reused.
  candidate_batch_tool candidate-builder-evidence "$_OASIS7_CB_DIR/recipe.json" "$_OASIS7_CB_DIR/inspect.json" "$_OASIS7_CB_DIR/evidence.json"
  # A correct label does not select the actual producer program. Reused images
  # must retain the canonical exec entrypoint and the complete Dockerfile Env;
  # this also rejects loader/shell/tool overrides such as BASH_ENV or LD_PRELOAD.
  python3 - "$_OASIS7_CB_DIR/inspect.json" <<'PY_CONFIG'
import json, sys
images = json.load(open(sys.argv[1]))
config = images[0]['Config']
if config.get('Entrypoint') != ['/opt/oasis7/scripts/build-wasm-module.sh']:
    raise SystemExit('error: candidate batch noncanonical image entrypoint')
expected = {
    'PATH': '/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
    'DEBIAN_FRONTEND': 'noninteractive', 'CARGO_HOME': '/cargo', 'RUSTUP_HOME': '/rustup',
    'OASIS7_WASM_TOOLCHAIN': 'nightly-2025-12-11',
    'OASIS7_WASM_TARGET': 'wasm32-unknown-unknown',
    'OASIS7_WASM_BUILD_STD': '1',
    'OASIS7_WASM_BUILD_STD_COMPONENTS': 'std,panic_abort',
    'OASIS7_WASM_BUILD_STD_FEATURES': '',
    'OASIS7_WASM_BUILD_IN_CONTAINER': '1',
    'OASIS7_WASM_BUILD_SUITE_BIN': '/cargo/bin/wasm_build_suite',
    'TZ': 'UTC', 'LANG': 'C', 'LC_ALL': 'C', 'SOURCE_DATE_EPOCH': '0',
}
values = config.get('Env')
if not isinstance(values, list):
    raise SystemExit('error: candidate batch missing image environment')
actual = {}
for value in values:
    if not isinstance(value, str) or '=' not in value:
        raise SystemExit('error: candidate batch malformed image environment')
    key, value = value.split('=', 1)
    if key in actual:
        raise SystemExit('error: candidate batch duplicate image environment')
    actual[key] = value
if actual != expected:
    raise SystemExit('error: candidate batch noncanonical image environment')
PY_CONFIG
  _OASIS7_CB_IMAGE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["builder_execution"]["image_id"])' "$_OASIS7_CB_DIR/evidence.json")"
  [[ "$_OASIS7_CB_IMAGE" =~ ^sha256:[0-9a-f]{64}$ ]]
  if [[ -n "${built:-}" && "$_OASIS7_CB_IMAGE" != "$built" ]]; then
    echo 'error: candidate batch inspected image differs from actual build' >&2
    return 2
  fi
  # A private stopped container pins precisely this image against image GC.
  # It never runs. Cleanup removes this container only, never shared images.
  local pin
  pin="$(docker create --platform linux/amd64 --entrypoint /bin/true "$_OASIS7_CB_IMAGE")"
  [[ "$pin" =~ ^[0-9a-f]{64}$ ]]
  _OASIS7_CB_PIN="$pin"
  candidate_batch_recipe_check
}

candidate_batch_build() (
  local module_id="$1" manifest profile="$3"
  manifest="$(candidate_batch_path "$2" manifest)"
  candidate_batch_path "$_OASIS7_CB_OUT" output >/dev/null
  candidate_batch_recipe_check
  local module_dir
  module_dir="$(mktemp -d "$_OASIS7_CB_DIR/module.XXXXXX")"
  trap 'rm -rf -- "$module_dir"' EXIT
  mkdir "$module_dir/cargo-home" "$module_dir/compiler-tmp"
  local manifest_relative="${manifest#"$_OASIS7_CB_ROOT"/}"
  local output_relative="${_OASIS7_CB_OUT#"$_OASIS7_CB_ROOT"}"
  local evidence_relative="${_OASIS7_CB_DIR#"$_OASIS7_CB_ROOT"}/evidence.json"
  # Use the unchanged canonical image ENTRYPOINT. The original build suite
  # verifies /opt/oasis7 producer content against this evidence before compiling
  # any guest and computes/checks each actual guest source closure itself.
  docker run --rm --platform linux/amd64 --user "$(id -u):$(id -g)" --workdir /workspace \
    --mount "type=bind,src=$_OASIS7_CB_ROOT,dst=/workspace" \
    --mount "type=bind,src=$module_dir/cargo-home,dst=/tmp/oasis7-cargo-home" \
    --mount "type=bind,src=$module_dir/compiler-tmp,dst=/tmp/oasis7-compiler-tmp" \
    --env OASIS7_WASM_BUILD_IN_CONTAINER=1 \
    --env OASIS7_WASM_SOURCE_WORKSPACE_DIR=/workspace \
    --env "OASIS7_WASM_CANDIDATE_BUILDER_EVIDENCE=/workspace$evidence_relative" \
    --env CARGO_BUILD_JOBS=2 \
    --env OASIS7_WASM_CANONICAL_CONTAINER_PLATFORM=linux-x86_64 \
    --env OASIS7_WASM_CANONICALIZER_VERSION=strip-custom-sections-v1 \
    --env HOME=/tmp/oasis7-home --env CARGO_HOME=/tmp/oasis7-cargo-home \
    --env TMPDIR=/tmp/oasis7-compiler-tmp --env RUSTUP_HOME=/rustup \
    "$_OASIS7_CB_IMAGE" --module-id "$module_id" --manifest-path "/workspace/$manifest_relative" \
    --out-dir "/workspace$output_relative" --profile "$profile"
  candidate_batch_recipe_check
)
