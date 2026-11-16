#!/usr/bin/env bash
set -euo pipefail

repo_root="${OASIS7_STANDALONE_TOOL_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$repo_root"
repo_root="$(pwd -P)"

rust_baseline_selector="${OASIS7_CI_RUN_RUST_BASELINE:-true}"
case "$rust_baseline_selector" in
  true|1) validate_lockfile_metadata=true ;;
  false|0) validate_lockfile_metadata=false ;;
  *)
    echo "error: OASIS7_CI_RUN_RUST_BASELINE must be true|false (or 1|0), got: $rust_baseline_selector" >&2
    exit 1
    ;;
esac

root_metadata_args=()
if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then root_metadata_args=(--locked); fi
root_workspace_member_manifest_paths="$(
  env -u RUSTC_WRAPPER cargo metadata \
    "${root_metadata_args[@]}" \
    --manifest-path "$repo_root/Cargo.toml" \
    --no-deps \
    --format-version 1 \
    | python3 -c '
import json
import sys

metadata = json.load(sys.stdin)
member_ids = set(metadata["workspace_members"])
for package in metadata["packages"]:
    if package["id"] in member_ids:
        print(package["manifest_path"])
'
)"
root_workspace_member_manifest_set=$'\n'"$root_workspace_member_manifest_paths"$'\n'

is_root_workspace_member_manifest() {
  local manifest="$1"
  local manifest_path
  manifest_path="$(cd "$(dirname "$manifest")" && pwd -P)/$(basename "$manifest")"
  [[ "$root_workspace_member_manifest_set" == *$'\n'"$manifest_path"$'\n'* ]]
}

tracked_standalone_file_set=$'\n'
manifests=()
lockfiles=()
while IFS= read -r tracked_file; do
  tracked_standalone_file_set+="$tracked_file"$'\n'
  case "$tracked_file" in
    */Cargo.toml)
      if ! is_root_workspace_member_manifest "$tracked_file"; then
        manifests+=("$tracked_file")
      fi
      ;;
    */Cargo.lock) lockfiles+=("$tracked_file") ;;
  esac
done < <(git ls-files \
  "tools/**/Cargo.toml" \
  "tools/**/Cargo.lock" \
  "crates/oasis7_builtin_wasm_modules/*/Cargo.toml" \
  "crates/oasis7_builtin_wasm_modules/*/Cargo.lock" | sort)

is_tracked_standalone_file() {
  local needle="$1"
  [[ "$tracked_standalone_file_set" == *$'\n'"$needle"$'\n'* ]]
}

if [[ "${#manifests[@]}" -eq 0 ]]; then
  echo "error: no tracked standalone Cargo manifests outside the root workspace found" >&2
  exit 1
fi

checked=0
for manifest in "${manifests[@]}"; do
  # A standalone workspace has one lockfile shared by its members.
  # Ask Cargo for identity without resolving dependencies or downloading crates.
  workspace_manifest="$(env -u RUSTC_WRAPPER cargo locate-project \
    --manifest-path "$manifest" --workspace --message-format plain 2>/dev/null || printf '%s\n' "$repo_root/$manifest")"
  workspace_dir="$(cd "$(dirname "$workspace_manifest")" && pwd -P)"
  if [[ "$workspace_dir" == "$repo_root" ]]; then
    # Unlisted nested packages are not allowed to borrow the root lockfile.
    lockfile="$(dirname "$manifest")/Cargo.lock"
  else
    lockfile="${workspace_dir#"$repo_root"/}/Cargo.lock"
  fi
  if [[ ! -f "$lockfile" ]]; then
    echo "error: standalone tool lockfile missing: $lockfile" >&2
    exit 1
  fi
  if ! is_tracked_standalone_file "$lockfile"; then
    echo "error: standalone lockfile is not tracked: $lockfile" >&2
    exit 1
  fi

  if [[ "$validate_lockfile_metadata" == true ]]; then
    echo "checking standalone lockfile: $manifest"
    env -u RUSTC_WRAPPER cargo metadata \
      --manifest-path "$manifest" \
      --locked \
      --format-version 1 >/dev/null
  else
    echo "checking standalone lockfile structure: $manifest"
  fi
  checked=$((checked + 1))
done

for lockfile in "${lockfiles[@]}"; do
  manifest="$(dirname "$lockfile")/Cargo.toml"
  if ! is_tracked_standalone_file "$manifest"; then
    echo "error: standalone manifest missing for lockfile: $lockfile" >&2
    exit 1
  fi
done

echo "ok: standalone lockfiles are locked and manifest-consistent ($checked manifests)"
if [[ "$validate_lockfile_metadata" == false ]]; then
  echo "ok: standalone lockfiles structurally checked; standalone Cargo metadata validation skipped because Rust baseline is disabled"
fi
