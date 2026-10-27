#!/usr/bin/env bash
set -euo pipefail
role="${1:?usage: build-role.sh identity|owner|executor|recovery-fence}"
case "$role" in identity|owner|executor|recovery-fence) ;; *) echo "invalid custody role" >&2; exit 2 ;; esac
cd "$(dirname "$0")/.."
if [[ "$(worker-build --version)" != "0.7.5" ]]; then
  echo 'Install the pinned builder: cargo install worker-build --version 0.7.5 --locked' >&2
  exit 2
fi
# worker-build 0.7.5 fixes its staging directory to build/. Serialize packaging,
# then retain each complete generated module in a separate role directory.
mkdir .role-build-lock 2>/dev/null || { echo 'another role build is running' >&2; exit 2; }
trap 'rmdir .role-build-lock' EXIT
env -u RUSTC_WRAPPER worker-build --release --no-opt -- --locked --no-default-features --features "$role"
mkdir -p artifacts
if [[ -e "artifacts/$role" ]]; then rm -rf "artifacts/$role"; fi
cp -R build "artifacts/$role"

case "$role" in
  identity|owner|executor) cp "platform/$role-entry.mjs" "artifacts/$role/entry.mjs" ;;
  recovery-fence) cp "platform/fence-entry.mjs" "artifacts/$role/entry.mjs" ;;
esac
