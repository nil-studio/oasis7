#!/usr/bin/env bash
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"
source docker/ci-base/toolchain-contract.sh

# Check the candidate merge tree against its exact base. A missing or
# inconsistent endpoint must not silently skip image verification.
if [[ ${1:-} == --print-verified-pr-base ]]; then
  [[ $# == 3 ]] || exit 1
  tested=$2
  source=$3
  if [[ ! $tested =~ ^[a-f0-9]{40}$ || ! $source =~ ^[a-f0-9]{40}$ || $(git rev-parse --verify HEAD) != "$tested" ]]; then
    printf '%s\n' 'Candidate checkout does not match the expected merge commit.' >&2
    exit 1
  fi
  # %P reads exact commit parents even at the edge of a shallow checkout.
  read -r -a parents <<< "$(git show -s --format=%P "$tested")"
  if [[ ${#parents[@]} != 2 || ${parents[1]} != "$source" ]]; then
    printf '%s\n' 'Candidate merge commit does not bind the expected PR source.' >&2
    exit 1
  fi
  git cat-file -e "${parents[0]}^{commit}"
  git cat-file -e "${parents[1]}^{commit}"
  printf '%s\n' "${parents[0]}"
  exit 0
fi

base=${1:-}
[[ "$base" =~ ^[a-f0-9]{40}$ ]]
tested=$(git rev-parse --verify HEAD)
git merge-base --is-ancestor "$base" "$tested"
paths=$(mktemp)
trap 'rm -f -- "$paths"' EXIT
git diff --name-only --no-renames -z "$base" "$tested" -- > "$paths"

rust_workflow_changed=false
while IFS= read -r -d '' file; do
  case "$file" in
    docker/ci-base/*|scripts/ci-browser-tools/package.json|scripts/ci-browser-tools/package-lock.json|.github/workflows/ci-base-images.yml)
      printf '%s\n' '{"target":["runtime","build","browser","rust-build","rust-wasm-build"]}'
      exit 0
      ;;
    .github/workflows/rust.yml) rust_workflow_changed=true ;;
  esac
done < "$paths"

if [[ "$rust_workflow_changed" == true ]]; then
  path=.github/workflows/rust.yml
  before=$(git show "$base:$path")
  after=$(git show "$tested:$path")
  before_tuple=$(ci_base_toolchain_tuple "$before")
  after_tuple=$(ci_base_toolchain_tuple "$after")
  if [[ "$before_tuple" != "$after_tuple" ]]; then
    printf '%s\n' '{"target":["rust-build","rust-wasm-build"]}'
    exit 0
  fi
fi
printf '%s\n' '{"target":[]}'
