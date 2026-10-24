#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/repo/src" "$FIXTURE/repo/scripts" "$FIXTURE/repo/.cargo"
cp "$ROOT_DIR/scripts/"{cargo-dev.sh,cargo-dev-lib.sh,find-python-with-module.sh} "$FIXTURE/repo/scripts/"
printf '[package]\nname="target_discovery"\nversion="0.1.0"\nedition="2021"\n' >"$FIXTURE/repo/Cargo.toml"
touch "$FIXTURE/repo/src/lib.rs"
repo_root="$(cd "$FIXTURE/repo" && pwd -P)"
source "$repo_root/scripts/cargo-dev-lib.sh"
unset CARGO_TARGET_DIR
# Caller working directory must not affect the repository's target resolution.
cd "${TMPDIR:-/tmp}"
[[ "$(oasis7_cargo_dev_target_dir "$repo_root")" == "$repo_root/target" ]]
[[ "$(oasis7_cargo_dev_debug_bin_dir "$repo_root")" == "$repo_root/target/debug" ]]
printf '[build]\ntarget-dir="configured target"\n' >"$repo_root/.cargo/config.toml"
[[ "$(oasis7_cargo_dev_target_dir "$repo_root")" == "$repo_root/configured target" ]]
for setting in 'CI=1' 'CI=true' 'OASIS7_FORCE_RAW_CARGO=1'; do
  result="$(env "$setting" CARGO_TARGET_DIR='relative target' bash -c 'source "$1"; oasis7_cargo_dev_target_dir "$2"' _ "$repo_root/scripts/cargo-dev-lib.sh" "$repo_root")"
  [[ "$result" == "$repo_root/relative target" ]]
done
absolute_target="$repo_root/absolute target"
[[ "$(CARGO_TARGET_DIR="$absolute_target" oasis7_cargo_dev_target_dir "$repo_root")" == "$absolute_target" ]]
[[ ! -e "$repo_root/target" && ! -L "$repo_root/target" ]]
mkdir "$FIXTURE/bin"
printf '#!/usr/bin/env bash\nexit 47\n' >"$FIXTURE/bin/cargo"
chmod +x "$FIXTURE/bin/cargo"
if result="$(PATH="$FIXTURE/bin:$PATH" oasis7_cargo_dev_debug_bin_dir "$repo_root")"; then
  echo 'failed target discovery returned a binary directory' >&2; exit 1
else
  [[ "$?" == 47 && -z "$result" ]]
fi
bash "$ROOT_DIR/scripts/cargo-dev-sccache.test.sh"
echo "cargo-dev-lib.test: OK"
