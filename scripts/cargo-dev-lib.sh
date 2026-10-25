#!/usr/bin/env bash

oasis7_cargo_dev_repo_root() {
  local repo_root
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
  printf '%s\n' "$repo_root"
}

oasis7_cargo_dev_use_wrapper() {
  if [[ "${OASIS7_FORCE_RAW_CARGO:-0}" == "1" ]]; then
    return 1
  fi
  if [[ "${CI:-}" == "1" || "${CI:-}" == "true" ]]; then
    return 1
  fi
  return 0
}

oasis7_cargo_dev_target_dir() {
  local repo_root="${1:-$(oasis7_cargo_dev_repo_root)}"
  # Ask Cargo so .cargo/config.toml, CARGO_HOME config and environment use
  # the same precedence as the build. Metadata does not compile or fetch.
  local python_bin metadata
  python_bin="$("$repo_root/scripts/find-python-with-module.sh" json)" || return
  metadata="$(cd "$repo_root" && OASIS7_CARGO_DEV_REPO_ROOT="$repo_root" oasis7_cargo_dev metadata --offline --no-deps --format-version 1)" || return
  "$python_bin" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<<"$metadata"
}

oasis7_cargo_dev_debug_bin_dir() {
  local repo_root="${1:-$(oasis7_cargo_dev_repo_root)}"
  local target_dir
  target_dir="$(oasis7_cargo_dev_target_dir "$repo_root")" || return
  printf '%s/debug\n' "$target_dir"
}

oasis7_cargo_dev() {
  local repo_root="${OASIS7_CARGO_DEV_REPO_ROOT:-$(oasis7_cargo_dev_repo_root)}"
  if oasis7_cargo_dev_use_wrapper; then
    (cd "$repo_root" && ./scripts/cargo-dev.sh "$@")
  else
    env -u RUSTC_WRAPPER cargo "$@"
  fi
}
