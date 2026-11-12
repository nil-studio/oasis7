#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/repo/scripts" "$fixture/tools" "$fixture/external"
cp "$SCRIPT_DIR/ensure-wasm-bindgen-cli.sh" "$SCRIPT_DIR/wasm-bindgen-cli-common.sh" "$fixture/repo/scripts/"
cat > "$fixture/repo/Cargo.lock" <<'LOCK'
[[package]]
name = "wasm-bindgen"
version = "0.2.999"
LOCK
export XDG_CACHE_HOME="$fixture/cache"
export PATH="$fixture/tools:$PATH"
export CARGO_TEST_LOG="$fixture/cargo.log"
cache_bin="$XDG_CACHE_HOME/oasis7/wasm-bindgen-cli/0.2.999/bin/wasm-bindgen"
helper="$fixture/repo/scripts/ensure-wasm-bindgen-cli.sh"

write_cli() {
  mkdir -p "$(dirname "$1")"
  printf '#!/usr/bin/env bash\nprintf "wasm-bindgen %s\\n"\n' "$2" > "$1"
  chmod +x "$1"
}
cat > "$fixture/tools/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$CARGO_TEST_LOG"
[[ "${FAKE_INSTALL_FAIL:-0}" == 0 ]] || exit 42
force=0
root=""
version=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) force=1; shift ;;
    --root) root="$2"; shift 2 ;;
    --version) version="$2"; shift 2 ;;
    *) shift ;;
  esac
done
# Model Cargo's install record: without force it may skip a registered version.
if [[ "${FAKE_REGISTERED:-0}" == 1 && "$force" == 0 ]]; then
  exit 0
fi
mkdir -p "$root/bin"
printf '#!/usr/bin/env bash\nprintf "wasm-bindgen %s\\n"\n' "${FAKE_INSTALLED_VERSION:-$version}" > "$root/bin/wasm-bindgen"
chmod +x "$root/bin/wasm-bindgen"
CARGO
chmod +x "$fixture/tools/cargo"
reset_case() {
  rm -rf "$XDG_CACHE_HOME" "$fixture/tools/wasm-bindgen"
  : > "$CARGO_TEST_LOG"
  unset WASM_BINDGEN_BIN FAKE_INSTALL_FAIL FAKE_REGISTERED FAKE_INSTALLED_VERSION
}
assert_bin() {
  local result
  result="$(bash "$helper" "$@" --print-bin)"
  [[ "$result" == "$cache_bin" ]] || { echo "unexpected CLI: $result" >&2; exit 1; }
  [[ "$("$result" --version)" == 'wasm-bindgen 0.2.999' ]]
}
assert_installed_once() {
  [[ "$(wc -l < "$CARGO_TEST_LOG" | tr -d ' ')" == 1 ]]
  [[ "$(cat "$CARGO_TEST_LOG")" == "install --force --locked --root ${cache_bin%/bin/wasm-bindgen} --version 0.2.999 wasm-bindgen-cli" ]]
}
assert_failure() {
  if bash "$helper" --ensure-cache --print-bin > "$fixture/stdout" 2> "$fixture/stderr"; then
    echo 'expected preparation failure' >&2
    exit 1
  fi
  [[ ! -s "$fixture/stdout" ]]
}

reset_case
assert_bin --ensure-cache
assert_installed_once

reset_case
write_cli "$cache_bin" 0.2.999
assert_bin --ensure-cache
[[ ! -s "$CARGO_TEST_LOG" ]]

reset_case
write_cli "$cache_bin" 0.2.998
export FAKE_REGISTERED=1
assert_bin --ensure-cache
assert_installed_once

reset_case
write_cli "$cache_bin" 0.2.999
chmod -x "$cache_bin"
assert_bin --ensure-cache
assert_installed_once

reset_case
write_cli "$cache_bin" 0.2.999
printf 'exit 1\n' >> "$cache_bin"
assert_bin --ensure-cache
assert_installed_once

reset_case
write_cli "$fixture/external/wasm-bindgen" 0.2.999
write_cli "$fixture/tools/wasm-bindgen" 0.2.999
export WASM_BINDGEN_BIN="$fixture/external/wasm-bindgen"
assert_bin --ensure-cache
assert_installed_once

reset_case
write_cli "$fixture/external/wasm-bindgen" 0.2.999
export WASM_BINDGEN_BIN="$fixture/external/wasm-bindgen"
[[ "$(bash "$helper" --print-bin)" == "$WASM_BINDGEN_BIN" ]]
[[ ! -s "$CARGO_TEST_LOG" ]]

reset_case
write_cli "$fixture/tools/wasm-bindgen" 0.2.999
[[ "$(bash "$helper" --print-bin)" == "$fixture/tools/wasm-bindgen" ]]
[[ ! -s "$CARGO_TEST_LOG" ]]

reset_case
# Default mode still installs when no external CLI is available.
assert_bin
[[ "$(cat "$CARGO_TEST_LOG")" == "install --locked --root ${cache_bin%/bin/wasm-bindgen} --version 0.2.999 wasm-bindgen-cli" ]]

reset_case
export FAKE_INSTALL_FAIL=1
assert_failure
assert_installed_once

reset_case
export FAKE_INSTALLED_VERSION=0.2.998
assert_failure
assert_installed_once
[[ "$(cat "$fixture/stderr")" == *'failed to provision'* ]]

reset_case
write_cli "$cache_bin" 0.2.999
[[ -z "$(bash "$helper" --ensure-cache)" ]]
[[ ! -s "$CARGO_TEST_LOG" ]]
printf 'ensure-wasm-bindgen-cli tests passed\n'
