# Shared extraction for the builder and the pull-request planner.
ci_base_toolchain_tuple() {
  # Prefix each match to preserve a trailing empty duplicate through $(...).
  local workflow=$1 stable nightly wasm_target
  stable=$(sed -n 's/^  RUST_TOOLCHAIN:/@/p' <<< "$workflow") || return
  stable=${stable#@ }
  nightly=$(sed -n 's/^  OASIS7_WASM_TOOLCHAIN:/@/p' <<< "$workflow") || return
  nightly=${nightly#@ }
  wasm_target=$(sed -n 's/^  OASIS7_WASM_TARGET:/@/p' <<< "$workflow") || return
  wasm_target=${wasm_target#@ }
  if ! [[ "$stable" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ &&
          "$nightly" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ &&
          "$wasm_target" == wasm32-unknown-unknown ]]; then
    printf '%s\n' 'error: missing, duplicate, or invalid CI image toolchain values in rust.yml' >&2
    return 1
  fi
  printf '%s %s %s\n' "$stable" "$nightly" "$wasm_target"
}
