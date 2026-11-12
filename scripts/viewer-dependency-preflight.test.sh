#!/usr/bin/env bash
set -euo pipefail

# Normal provisioning is exercised only with the fixture npm implementation.
export OASIS7_LOCAL_CHECK=0
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
PREFLIGHT_SCRIPT="$ROOT_DIR/scripts/viewer-dependency-preflight.sh"

tmp_root="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT

new_fixture() {
  local fixture_root="$1"
  local viewer_dir="$fixture_root/crates/oasis7_viewer"
  mkdir -p "$viewer_dir"
  viewer_dir="$(cd "$viewer_dir" && pwd -P)"
  printf '{"name":"oasis7-viewer-ui"}\n' > "$viewer_dir/package.json"
  printf '{"name":"oasis7-viewer-ui","lockfileVersion":3}\n' > "$viewer_dir/package-lock.json"
  printf '%s\n' "$viewer_dir"
}

write_dependency_binaries() {
  local viewer_dir="$1"
  mkdir -p "$viewer_dir/node_modules/.bin"
  for command_name in vite vitest; do
    printf '#!/usr/bin/env bash\nexit 0\n' > "$viewer_dir/node_modules/.bin/$command_name"
    chmod +x "$viewer_dir/node_modules/.bin/$command_name"
  done
  printf '{"lockfileVersion":3,"packages":{}}\n' > "$viewer_dir/node_modules/.package-lock.json"
}

touch_lockfile_after_install() {
  local viewer_dir="$1"
  touch "$viewer_dir/package-lock.json"
}

write_mock_npm() {
  local bin_dir="$1"
  local mode="${2:-success}"
  mkdir -p "$bin_dir"
  cat > "$bin_dir/npm" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "\$*" >> "$bin_dir/npm.calls"
if [[ "\$1" != "--prefix" || "\$2" != "crates/oasis7_viewer" || "\$3" != "ci" ]]; then
  echo "unexpected npm args: \$*" >&2
  exit 64
fi
if [[ "$mode" == "failure" ]]; then
  echo "mock npm ci failed" >&2
  exit 17
fi
mkdir -p "\$PWD/crates/oasis7_viewer/node_modules/.bin"
for command_name in vite vitest; do
  printf '#!/usr/bin/env bash\\nexit 0\\n' > "\$PWD/crates/oasis7_viewer/node_modules/.bin/\$command_name"
  chmod +x "\$PWD/crates/oasis7_viewer/node_modules/.bin/\$command_name"
done
printf '{"lockfileVersion":3,"packages":{}}\\n' > "\$PWD/crates/oasis7_viewer/node_modules/.package-lock.json"
EOF
  chmod +x "$bin_dir/npm"
}

if [[ ! -f "$PREFLIGHT_SCRIPT" ]]; then
  echo "expected shared Viewer dependency preflight helper at $PREFLIGHT_SCRIPT" >&2
  exit 1
fi

# Ready dependencies are a no-op: no npm ci call is allowed.
ready_root="$tmp_root/ready"
ready_viewer_dir="$(new_fixture "$ready_root")"
write_dependency_binaries "$ready_viewer_dir"
ready_bin="$tmp_root/ready-bin"
write_mock_npm "$ready_bin"
ready_output="$({ PATH="$ready_bin:$PATH" "$PREFLIGHT_SCRIPT" --root "$ready_root" --purpose all; } 2>"$tmp_root/ready.stderr")"
if [[ "$ready_output" != "Viewer npm dependencies ready: $ready_viewer_dir" ]]; then
  echo "expected ready dependencies to pass, got: $ready_output" >&2
  exit 1
fi
if [[ -s "$ready_bin/npm.calls" ]]; then
  echo "ready dependency preflight unexpectedly ran npm ci" >&2
  cat "$ready_bin/npm.calls" >&2
  exit 1
fi
OASIS7_LOCAL_CHECK=1 PATH="$ready_bin:$PATH" "$PREFLIGHT_SCRIPT" --root "$ready_root" --purpose all >/dev/null
[[ ! -s "$ready_bin/npm.calls" ]]

# Local checks reject absent or stale installs without invoking npm or rewriting locks.
for condition in missing stale; do
  local_root="$tmp_root/local-$condition"
  local_viewer="$(new_fixture "$local_root")"
  local_bin="$tmp_root/local-$condition-bin"
  write_mock_npm "$local_bin"
  if [[ "$condition" == stale ]]; then
    write_dependency_binaries "$local_viewer"
    touch -t 202001010000 "$local_viewer/node_modules/.package-lock.json"
    touch -t 202101010000 "$local_viewer/package-lock.json"
    cp "$local_viewer/node_modules/.package-lock.json" "$tmp_root/local-installed-lock"
  fi
  cp "$local_viewer/package-lock.json" "$tmp_root/local-package-lock"
  if OASIS7_LOCAL_CHECK=1 PATH="$local_bin:$PATH" "$PREFLIGHT_SCRIPT" --root "$local_root" --purpose all >"$tmp_root/local.stdout" 2>"$tmp_root/local.stderr"; then
    echo "expected local $condition dependencies to block" >&2
    exit 1
  else
    status=$?
  fi
  [[ "$status" == 2 && ! -s "$local_bin/npm.calls" ]]
  grep -Fq 'automatic installation is disabled' "$tmp_root/local.stderr"
  cmp "$local_viewer/package-lock.json" "$tmp_root/local-package-lock"
  if [[ "$condition" == missing ]]; then
    [[ ! -e "$local_viewer/node_modules" ]]
  else
    cmp "$local_viewer/node_modules/.package-lock.json" "$tmp_root/local-installed-lock"
  fi
done

# Missing dependencies trigger exactly one npm ci, then pass revalidation.
missing_root="$tmp_root/missing"
missing_viewer_dir="$(new_fixture "$missing_root")"
missing_bin="$tmp_root/missing-bin"
write_mock_npm "$missing_bin"
missing_output="$({ PATH="$missing_bin:$PATH" "$PREFLIGHT_SCRIPT" --root "$missing_root" --purpose all; } 2>"$tmp_root/missing.stderr")"
if [[ "$missing_output" != "Viewer npm dependencies ready: $missing_viewer_dir" ]]; then
  echo "expected missing dependencies to install and revalidate, got: $missing_output" >&2
  cat "$tmp_root/missing.stderr" >&2
  exit 1
fi
if [[ "$(wc -l < "$missing_bin/npm.calls" | tr -d ' ')" != "1" ]]; then
  echo "expected exactly one npm ci for missing dependencies" >&2
  cat "$missing_bin/npm.calls" >&2
  exit 1
fi

# A lockfile newer than node_modules triggers exactly one refresh.
stale_root="$tmp_root/stale"
stale_viewer_dir="$(new_fixture "$stale_root")"
write_dependency_binaries "$stale_viewer_dir"
touch -t 202001010000 "$stale_viewer_dir/node_modules/.package-lock.json"
touch -t 202101010000 "$stale_viewer_dir/package-lock.json"
stale_bin="$tmp_root/stale-bin"
write_mock_npm "$stale_bin"
stale_output="$({ PATH="$stale_bin:$PATH" "$PREFLIGHT_SCRIPT" --root "$stale_root" --purpose all; } 2>"$tmp_root/stale.stderr")"
if [[ "$stale_output" != "Viewer npm dependencies ready: $stale_viewer_dir" ]]; then
  echo "expected stale dependencies to install and revalidate, got: $stale_output" >&2
  cat "$tmp_root/stale.stderr" >&2
  exit 1
fi
if [[ "$(wc -l < "$stale_bin/npm.calls" | tr -d ' ')" != "1" ]]; then
  echo "expected exactly one npm ci for stale dependencies" >&2
  cat "$stale_bin/npm.calls" >&2
  exit 1
fi

# A failed install fails clearly and does not retry.
failed_root="$tmp_root/failed"
failed_viewer_dir="$(new_fixture "$failed_root")"
failed_bin="$tmp_root/failed-bin"
write_mock_npm "$failed_bin" failure
if PATH="$failed_bin:$PATH" "$PREFLIGHT_SCRIPT" --root "$failed_root" --purpose build 2>"$tmp_root/failed.stderr"; then
  echo "expected failed npm ci to fail preflight" >&2
  exit 1
fi
if [[ "$(wc -l < "$failed_bin/npm.calls" | tr -d ' ')" != "1" ]]; then
  echo "expected exactly one npm ci attempt after install failure" >&2
  cat "$failed_bin/npm.calls" >&2
  exit 1
fi
if ! grep -Fqx "error: Viewer npm dependency install failed; run npm --prefix crates/oasis7_viewer ci and retry." "$tmp_root/failed.stderr"; then
  echo "expected clear failed-install error" >&2
  cat "$tmp_root/failed.stderr" >&2
  exit 1
fi

for wrapper in \
  scripts/build-viewer-software-safe.sh \
  scripts/viewer-performance-probe.sh \
  scripts/viewer-pixel-world-fragment-visual-smoke.sh \
  scripts/verify-gameplay-attraction-automation.sh; do
  if ! grep -Fq 'viewer_dependency_preflight' "$ROOT_DIR/$wrapper"; then
    echo "expected $wrapper to reuse shared Viewer dependency preflight" >&2
    exit 1
  fi
done
for wrapper in scripts/worktree-harness.sh scripts/run-producer-playtest.sh; do
  if grep -Fq 'viewer_dependency_preflight' "$ROOT_DIR/$wrapper"; then
    echo "expected $wrapper to defer dependency ensure to the actual Viewer build authority" >&2
    exit 1
  fi
done
if [[ "$(grep -c 'viewer_dependency_preflight' "$ROOT_DIR/scripts/build-viewer-software-safe.sh")" != "1" ]]; then
  echo "expected build-viewer-software-safe.sh to remain the single Viewer build ensure authority" >&2
  exit 1
fi
if ! grep -Fq 'ensure:dependencies' "$ROOT_DIR/crates/oasis7_viewer/package.json"; then
  echo "expected Viewer package to expose an ensure-dependencies script" >&2
  exit 1
fi

viewer_ci_group="$tmp_root/viewer-ci-group.sh"
sed -n '/^run_oasis7_viewer_software_safe_feedback_contract_tests() {/,/^}/p' \
  "$ROOT_DIR/scripts/ci-tests.sh" > "$viewer_ci_group"
if [[ "$(grep -c 'viewer_dependency_preflight' "$viewer_ci_group" || true)" != "1" ]]; then
  echo "expected ci-tests Viewer group to call shared dependency ensure exactly once" >&2
  cat "$viewer_ci_group" >&2
  exit 1
fi
ensure_line=$(grep -n 'viewer_dependency_preflight' "$viewer_ci_group" | cut -d: -f1)
npm_line=$(grep -n 'run npm --prefix crates/oasis7_viewer' "$viewer_ci_group" | head -n1 | cut -d: -f1)
if [[ -z "$npm_line" || "$ensure_line" -ge "$npm_line" ]]; then
  echo "expected ci-tests dependency ensure before the first Viewer npm test" >&2
  cat "$viewer_ci_group" >&2
  exit 1
fi

echo "viewer dependency preflight tests passed"
