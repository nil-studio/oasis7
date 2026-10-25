#!/usr/bin/env bash
set -euo pipefail
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=${1:-$(cd "$script_dir/.." && pwd)}
cd "$repo_root"
case "$(uname -s)" in Linux|Darwin) ;; *) echo 'formal native process tests require Linux or Darwin' >&2; exit 2 ;; esac
formal_python=$(bash "$script_dir/find-python-with-module.sh" tomllib)
bundle_parent=$(mktemp -d)
trap 'rm -rf "$bundle_parent"' EXIT
"$formal_python" "$script_dir/ci-test-artifacts.py" build --repo-root "$repo_root" --bundle "$bundle_parent/bundle"
failed=0
for suite in runtime-protocol world-history healthy-set storage-recovery lifecycle-cli; do
  "$formal_python" "$script_dir/ci-test-artifacts.py" run --repo-root "$repo_root" --bundle "$bundle_parent/bundle" --suite "$suite" || failed=1
done
exit "$failed"
