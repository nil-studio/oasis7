#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

workflow_toolchain_install=$(awk '
/^[[:space:]]*- name: Install pinned Rust toolchain$/ { in_step=1; next }
in_step && /^[[:space:]]*- name:/ { in_step=0 }
in_step && /rustup toolchain install/ { print; exit }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_toolchain_install" != *"--profile minimal"* ]]; then
  echo "compile metrics workflow must install Rust with the minimal profile" >&2
  exit 1
fi
if [[ "$workflow_toolchain_install" == *"--profile default"* ]]; then
  echo "compile metrics workflow must not install Rust with the default profile" >&2
  exit 1
fi

workflow_checkout_depth=$(awk '
/^[[:space:]]*- uses: actions\/checkout@[0-9a-f]{40}([[:space:]]|$)/ { in_step=1; next }
in_step && /^[[:space:]]*- name:/ { in_step=0 }
in_step && /fetch-depth:/ { print $2; exit }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_checkout_depth" != "1" ]]; then
  echo "compile metrics workflow must use a shallow current checkout" >&2
  exit 1
fi
if grep -Eq 'fetch-depth:[[:space:]]*0' .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must not request a full-history checkout" >&2
  exit 1
fi
compile_metrics_timeout=$(awk '
/^  compile-metrics:/ { in_job=1; next }
in_job && /^  [^[:space:]]/ { exit }
in_job && /^[[:space:]]+timeout-minutes:/ { print $2; exit }
' .github/workflows/compile-metrics.yml)
if [[ "$compile_metrics_timeout" != "45" ]]; then
  echo "compile metrics matrix job must cap runner time at 45 minutes" >&2
  exit 1
fi
if ! grep -Eq '^[[:space:]]*CARGO_REGISTRIES_CRATES_IO_PROTOCOL:[[:space:]]*sparse[[:space:]]*$' .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must pin the sparse crates.io protocol" >&2
  exit 1
fi
if grep -Eq 'COMPILE_METRICS_MAX_[A-Z0-9_]+' .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must not define unused threshold environment defaults" >&2
  exit 1
fi
workflow_cargo_check_threshold=$(awk '
/^[[:space:]]*max_cargo_check_regression_pct:[[:space:]]*$/ { in_block=1; next }
in_block && /^      [a-zA-Z0-9_]+:/ { exit }
in_block && /^[[:space:]]*default:/ { print $2; exit }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_cargo_check_threshold" != '"25"' ]]; then
  echo "compile metrics workflow must default to a cold cargo check regression threshold of 25%" >&2
  exit 1
fi
workflow_cargo_build_release_threshold=$(awk '
/^[[:space:]]*max_cargo_build_release_regression_pct:[[:space:]]*$/ { in_block=1; next }
in_block && /^      [a-zA-Z0-9_]+:/ { exit }
in_block && /^[[:space:]]*default:/ { print $2; exit }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_cargo_build_release_threshold" != '"25"' ]]; then
  echo "compile metrics workflow must default to a cold cargo build --release regression threshold of 25%" >&2
  exit 1
fi
python3 - <<'PY'
from pathlib import Path

source = Path(".github/workflows/compile-metrics.yml").read_text(encoding="utf-8")
enforce = source[source.index("      - name: Enforce compile metrics gate"):source.index("\n  summarize:")]
launcher_start = enforce.index('          if [[ "${METRIC_TARGET}" == "launcher" ]]; then')
launcher_end = enforce.index("\n          fi", launcher_start) + len("\n          fi")
launcher_block = enforce[launcher_start:launcher_end]
release_flag = "--max-cargo-build-release-regression-pct"
if release_flag not in launcher_block:
    raise SystemExit("release-build regression threshold must be gated to the launcher target")
if enforce.count(release_flag) != 1:
    raise SystemExit("release-build regression threshold must not be passed outside the launcher target")
PY
if ! grep -Eq '^[[:space:]]*- oasis7_default_features[[:space:]]*$' .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must expose the oasis7 default-feature target" >&2
  exit 1
fi
workflow_oasis7_default_target=$(awk '
/^[[:space:]]*oasis7_default_features\)[[:space:]]*$/ { in_case=1; next }
in_case && /^[[:space:]]*;;/ { exit }
in_case { print }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_oasis7_default_target" != *'package="oasis7"'* ]]; then
  echo "oasis7 default-feature target must measure package oasis7" >&2
  exit 1
fi
if [[ "$workflow_oasis7_default_target" != *'extra_args+=(--check-only)'* ]]; then
  echo "oasis7 default-feature target must use check-only measurement" >&2
  exit 1
fi
if [[ "$workflow_oasis7_default_target" == *'--no-default-features'* ]]; then
  echo "oasis7 default-feature target must retain Cargo default features" >&2
  exit 1
fi
if ! grep -Eq '^[[:space:]]*- oasis7_node_default_features[[:space:]]*$' .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must expose the oasis7_node default-feature target" >&2
  exit 1
fi
workflow_oasis7_node_default_target=$(awk '
/^[[:space:]]*oasis7_node_default_features\)[[:space:]]*$/ { in_case=1; next }
in_case && /^[[:space:]]*;;/ { exit }
in_case { print }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_oasis7_node_default_target" != *'package="oasis7_node"'* ]]; then
  echo "oasis7_node default-feature target must measure package oasis7_node" >&2
  exit 1
fi
if [[ "$workflow_oasis7_node_default_target" != *'extra_args+=(--check-only)'* ]]; then
  echo "oasis7_node default-feature target must use check-only measurement" >&2
  exit 1
fi
if [[ "$workflow_oasis7_node_default_target" == *'--no-default-features'* ]]; then
  echo "oasis7_node default-feature target must retain Cargo default features" >&2
  exit 1
fi
if ! grep -q 'time\.monotonic_ns()' scripts/ci-compile-metrics.sh; then
  echo "compile metrics timing must use a monotonic clock" >&2
  exit 1
fi
if grep -q 'time\.time_ns()' scripts/ci-compile-metrics.sh; then
  echo "compile metrics elapsed timing must not use adjustable wall-clock time" >&2
  exit 1
fi

# A linked checkout must be deregistered before its temporary directory is
# removed.  Deleting the directory alone leaks .git/worktrees metadata.
python3 - <<'PY'
from pathlib import Path

source = Path("scripts/ci-compile-metrics.sh").read_text(encoding="utf-8")
cleanup = source[source.index("cleanup() {"):source.index("trap cleanup EXIT")]
remove_pos = cleanup.find("worktree remove")
rm_pos = cleanup.find("rm -rf")
if remove_pos < 0 or rm_pos < 0 or remove_pos > rm_pos:
    raise SystemExit("compile-metrics cleanup must git worktree remove before rm -rf")
PY
if ! grep -Eq 'git fetch --no-tags --depth=1 origin -- "\$\{BASELINE_REF\}"' .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must fetch an optional baseline explicitly" >&2
  exit 1
fi
if ! grep -Eq "FETCH_HEAD\\^\\{commit\\}" .github/workflows/compile-metrics.yml; then
  echo "compile metrics workflow must resolve fetched baseline provenance" >&2
  exit 1
fi
workflow_baseline_fetch_step=$(awk '
/^[[:space:]]*- name: Fetch baseline ref$/ { in_step=1; next }
in_step && /^[[:space:]]*- name:/ { exit }
in_step { print }
' .github/workflows/compile-metrics.yml)
if [[ "$workflow_baseline_fetch_step" != *"if:"* || "$workflow_baseline_fetch_step" != *"baseline_ref !="* ]]; then
  echo "baseline fetch must be conditional on a requested baseline ref" >&2
  exit 1
fi
# These literals intentionally assert GitHub expressions instead of expanding
# them as Bash variables.
# shellcheck disable=SC2016
if [[ "$workflow_baseline_fetch_step" != *'BASELINE_REF: ${{ inputs.baseline_ref }}'* ]]; then
  echo "baseline fetch must pass the input through an environment variable" >&2
  exit 1
fi
workflow_baseline_resolver_step=$(awk '
/^[[:space:]]*- name: Resolve baseline ref$/ { in_step=1; next }
in_step && /^[[:space:]]*- name:/ { exit }
in_step { print }
' .github/workflows/compile-metrics.yml)
# shellcheck disable=SC2016
if [[ "$workflow_baseline_resolver_step" != *'BASELINE_REF: ${{ inputs.baseline_ref }}'* ]]; then
  echo "baseline resolver must pass the input through an environment variable" >&2
  exit 1
fi
# shellcheck disable=SC2016
if [[ "$workflow_baseline_resolver_step" != *'baseline_ref="${BASELINE_REF}"'* ]]; then
  echo "baseline resolver must read the environment variable literally" >&2
  exit 1
fi
# shellcheck disable=SC2016
if [[ "$workflow_baseline_resolver_step" == *'baseline_ref="${{ inputs.baseline_ref }}"'* ]]; then
  echo "baseline resolver must not interpolate workflow input into Bash" >&2
  exit 1
fi

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

malicious_ref_marker="$tmp_dir/malicious-ref-marker"
malicious_ref="evil\$(touch ${malicious_ref_marker})"
resolved_malicious_ref=$(
  BASELINE_REF="$malicious_ref" \
    bash -c 'set -euo pipefail; baseline_ref="${BASELINE_REF}"; printf "%s" "$baseline_ref"'
)
if [[ "$resolved_malicious_ref" != "$malicious_ref" || -e "$malicious_ref_marker" ]]; then
  echo "baseline resolver must preserve malicious-looking refs literally" >&2
  exit 1
fi

option_ref_marker="$tmp_dir/option-ref-marker"
option_ref="--upload-pack=touch ${option_ref_marker}"
option_ref_repo="$tmp_dir/option-ref-repo"
option_ref_origin="$tmp_dir/option-ref-origin.git"
git init --bare --quiet "$option_ref_origin"
git init --quiet "$option_ref_repo"
git -C "$option_ref_repo" remote add origin "$option_ref_origin"
if git -C "$option_ref_repo" fetch --no-tags --depth=1 origin -- "$option_ref" 2>/dev/null; then
  echo "option-shaped baseline ref unexpectedly fetched" >&2
  exit 1
fi
if [[ -e "$option_ref_marker" ]]; then
  echo "baseline fetch must not execute option-shaped refs" >&2
  exit 1
fi

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"
host_target=$(rustc -vV | sed -n 's/^host: //p')
if [[ -z "$host_target" ]]; then
  echo "unable to resolve the Rust host target for the compile metrics contract" >&2
  exit 1
fi
cat >"$fake_bin/cargo" <<'FAKE_CARGO'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${FAKE_CARGO_LOG:?}"
printf '%s\n' "${CARGO_HOME:-<unset>}" >>"${FAKE_CARGO_HOME_LOG:?}"
printf '%s\n' "${CARGO_INCREMENTAL:-<unset>}" >>"${FAKE_CARGO_INCREMENTAL_LOG:?}"
printf '%s\n' "${CARGO_PROFILE_DEV_DEBUG:-<unset>}" >>"${FAKE_CARGO_PROFILE_DEV_DEBUG_LOG:?}"
printf '%s\n' "${CARGO_PROFILE_TEST_DEBUG:-<unset>}" >>"${FAKE_CARGO_PROFILE_TEST_DEBUG_LOG:?}"
printf '%s\n' "${RUSTC_WRAPPER:-<unset>}" >>"${FAKE_RUSTC_WRAPPER_LOG:?}"
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  printf '%s\n' "$CARGO_TARGET_DIR" >>"${FAKE_CARGO_TARGET_LOG:?}"
fi
case "${1:-}" in
  tree)
    if [[ "$*" != *"--offline"* || "$*" != *"--locked"* ]]; then
      echo "dependency-tree query must be locked and offline: $*" >&2
      exit 1
    fi
    if [[ "$*" == *"-i wasmtime"* || "$*" == *"-i oasis7_wasm_executor"* ]]; then
      exit 1
    fi
    case "${FAKE_CARGO_TREE_FIXTURE:-none}" in
      all)
        printf 'fake-package\nfake-dependency\nfake-dependency (*)\nwasmtime v99.0.0\noasis7_wasm_executor v0.1.0\n'
        ;;
      none)
        printf 'fake-package\nfake-dependency\n'
        ;;
      *)
        echo "unexpected dependency-tree fixture: ${FAKE_CARGO_TREE_FIXTURE}" >&2
        exit 1
        ;;
    esac
    ;;
  fetch)
    expected_target="${FAKE_HOST_TARGET:?}"
    if [[ "$*" != *"--locked"* || "$*" != *"--target ${expected_target}"* ]]; then
      echo "compile metrics fetch must be locked to the runner host target: $*" >&2
      exit 1
    fi
    ;;
  check)
    ;;
  build)
    mkdir -p "${CARGO_TARGET_DIR:?}/release"
    : >"${CARGO_TARGET_DIR}/release/fake_library"
    ;;
  *)
    echo "unexpected cargo command: $*" >&2
    exit 1
    ;;
esac
FAKE_CARGO
chmod +x "$fake_bin/cargo"

cat >"$fake_bin/git" <<'FAKE_GIT'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "rev-parse" && "${2:-}" == "--verify" ]]; then
  printf '%s\n' "${FAKE_BASELINE_COMMIT_OID:-${FAKE_COMMIT_OID:?}}"
  exit 0
fi
if [[ "${1:-}" == "-C" && "${3:-}" == "rev-parse" ]]; then
  if [[ "${2:-}" == */baseline-worktree && -n "${FAKE_BASELINE_COMMIT_OID:-}" ]]; then
    printf '%s\n' "$FAKE_BASELINE_COMMIT_OID"
  else
    printf '%s\n' "${FAKE_COMMIT_OID:?}"
  fi
  exit 0
fi
if [[ "${1:-}" == "-C" && "${3:-}" == "ls-files" ]]; then
  # The fake baseline checkout is intentionally a minimal disposable fixture;
  # keep its source fingerprint deterministic without falling back to the
  # real Git binary for an empty synthetic worktree.
  if [[ " $* " == *" --others "* ]]; then
    exit 0
  fi
  printf 'README.md\0'
  exit 0
fi
if [[ "${1:-}" == "worktree" && "${2:-}" == "add" ]]; then
  if [[ -n "${FAKE_GIT_WORKTREE_ADD_LOG:-}" ]]; then
    printf '%s\n' "$*" >>"$FAKE_GIT_WORKTREE_ADD_LOG"
  fi
  mkdir -p "${4:?}"
  printf 'fake baseline fixture\n' >"${4}/README.md"
  exit 0
fi
if [[ "${1:-}" == "-C" && "${3:-}" == "worktree" && "${4:-}" == "remove" && "${5:-}" == "--force" ]]; then
  rm -rf "${6:?}"
  exit 0
fi
echo "unexpected git command: $*" >&2
exit 1
FAKE_GIT
chmod +x "$fake_bin/git"

out_dir="$tmp_dir/metrics"
expected_commit_oid=$(git rev-parse HEAD)
expected_baseline_oid=$(git rev-parse HEAD^)
FAKE_CARGO_LOG="$tmp_dir/cargo.log" \
FAKE_CARGO_HOME_LOG="$tmp_dir/cargo-home.log" \
FAKE_CARGO_INCREMENTAL_LOG="$tmp_dir/cargo-incremental.log" \
FAKE_CARGO_PROFILE_DEV_DEBUG_LOG="$tmp_dir/cargo-profile-dev-debug.log" \
FAKE_CARGO_PROFILE_TEST_DEBUG_LOG="$tmp_dir/cargo-profile-test-debug.log" \
FAKE_RUSTC_WRAPPER_LOG="$tmp_dir/cargo-rustc-wrapper.log" \
FAKE_CARGO_TARGET_LOG="$tmp_dir/cargo-target.log" \
FAKE_HOST_TARGET="$host_target" \
FAKE_COMMIT_OID="$expected_commit_oid" \
FAKE_CARGO_TREE_FIXTURE=all \
CARGO_INCREMENTAL=1 \
CARGO_PROFILE_DEV_DEBUG=2 \
CARGO_PROFILE_TEST_DEBUG=2 \
RUSTC_WRAPPER=hostile-wrapper \
PATH="$fake_bin:$PATH" \
  ./scripts/ci-compile-metrics.sh \
    --package fake_library \
    --out-dir "$out_dir" \
    --check-only \
    --no-default-features

python3 - "$out_dir/current.metrics.json" "$out_dir/comparison.json" "$out_dir/summary.md" "$expected_commit_oid" <<'PY'
import json
from pathlib import Path
import sys

metrics = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
comparison = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
summary = Path(sys.argv[3]).read_text(encoding="utf-8")
expected_commit_oid = sys.argv[4]
assert metrics["package_count"] == 4
assert metrics["commit_oid"] == expected_commit_oid
assert metrics["binary"] is None
assert metrics["check_only"] is True
assert metrics["no_default_features"] is True
assert metrics["wasmtime_present"] is True
assert metrics["wasm_executor_present"] is True
assert metrics["cargo_check_seconds"] >= 0
assert metrics["cargo_build_release_seconds"] is None
assert metrics["release_binary_bytes"] is None
assert comparison["metric_rows"] == []
assert comparison["current_commit_oid"] == expected_commit_oid
assert comparison["baseline_commit_oid"] is None
assert comparison["baseline_ref"] is None
assert "not measured (check-only package)" in summary
assert expected_commit_oid in summary
PY

if grep -Eq 'build' "$tmp_dir/cargo.log"; then
  echo "check-only compile metrics unexpectedly invoked cargo build" >&2
  exit 1
fi
cold_check_calls=$(grep -c '^check ' "$tmp_dir/cargo.log" || true)
if [[ "$cold_check_calls" != "1" ]]; then
  echo "warm-disabled check-only measurement must invoke exactly one cold check: $cold_check_calls" >&2
  exit 1
fi

python3 - \
  "$tmp_dir/cargo-incremental.log" \
  "$tmp_dir/cargo-profile-dev-debug.log" \
  "$tmp_dir/cargo-profile-test-debug.log" \
  "$tmp_dir/cargo-rustc-wrapper.log" <<'PY'
from pathlib import Path
import sys

for path_arg, variable in zip(
    sys.argv[1:],
    (
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
        "RUSTC_WRAPPER",
    ),
):
    values = [line for line in Path(path_arg).read_text(encoding="utf-8").splitlines() if line]
    expected = "<unset>" if variable == "RUSTC_WRAPPER" else "0"
    if not values or set(values) != {expected}:
        raise SystemExit(f"{variable} was not normalized for every Cargo call: {values}")
print("compile environment (current-only): all Cargo calls normalized")
PY

release_out_dir="$tmp_dir/release-metrics"
FAKE_CARGO_LOG="$tmp_dir/release-cargo.log" \
FAKE_CARGO_HOME_LOG="$tmp_dir/release-cargo-home.log" \
FAKE_CARGO_INCREMENTAL_LOG="$tmp_dir/release-cargo-incremental.log" \
FAKE_CARGO_PROFILE_DEV_DEBUG_LOG="$tmp_dir/release-cargo-profile-dev-debug.log" \
FAKE_CARGO_PROFILE_TEST_DEBUG_LOG="$tmp_dir/release-cargo-profile-test-debug.log" \
FAKE_RUSTC_WRAPPER_LOG="$tmp_dir/release-cargo-rustc-wrapper.log" \
FAKE_CARGO_TARGET_LOG="$tmp_dir/release-cargo-target.log" \
FAKE_HOST_TARGET="$host_target" \
FAKE_COMMIT_OID="$expected_commit_oid" \
FAKE_CARGO_TREE_FIXTURE=all \
CARGO_INCREMENTAL=1 \
CARGO_PROFILE_DEV_DEBUG=2 \
CARGO_PROFILE_TEST_DEBUG=2 \
RUSTC_WRAPPER=hostile-wrapper \
PATH="$fake_bin:$PATH" \
  ./scripts/ci-compile-metrics.sh \
    --package fake_library \
    --out-dir "$release_out_dir" \
    --binary fake_library \
    --no-default-features

if ! grep -Eq '^build ' "$tmp_dir/release-cargo.log"; then
  echo "release compile metrics did not invoke cargo build" >&2
  exit 1
fi
if ! grep -Eq '^build .*--no-default-features([[:space:]]|$)' "$tmp_dir/release-cargo.log"; then
  echo "release compile metrics must pass --no-default-features to cargo build" >&2
  exit 1
fi

python3 - \
  "$tmp_dir/release-cargo-incremental.log" \
  "$tmp_dir/release-cargo-profile-dev-debug.log" \
  "$tmp_dir/release-cargo-profile-test-debug.log" \
  "$tmp_dir/release-cargo-rustc-wrapper.log" <<'PY'
from pathlib import Path
import sys

for path_arg, variable in zip(
    sys.argv[1:],
    (
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
        "RUSTC_WRAPPER",
    ),
):
    values = [line for line in Path(path_arg).read_text(encoding="utf-8").splitlines() if line]
    expected = "<unset>" if variable == "RUSTC_WRAPPER" else "0"
    if not values or set(values) != {expected}:
        raise SystemExit(f"{variable} was not normalized for every release Cargo call: {values}")
print("compile environment (release): all Cargo calls normalized")
PY

python3 - "$tmp_dir/cargo.log" <<'PY'
from pathlib import Path
import sys

commands = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
measured = [command for command in commands if command.split(maxsplit=1)[0] in {"tree", "fetch", "check", "build"}]
if not measured:
    raise SystemExit("no cargo invocations recorded")
unlocked = [command for command in measured if "--locked" not in command.split()]
if unlocked:
    raise SystemExit(f"unlocked cargo invocations: {unlocked}")
timed = [command for command in measured if command.split(maxsplit=1)[0] in {"check", "build"}]
offline_missing = [command for command in timed if "--offline" not in command.split()]
if offline_missing:
    raise SystemExit(f"timed cargo invocations must be offline: {offline_missing}")
tree_commands = [command for command in measured if command.split(maxsplit=1)[0] == "tree"]
if len(tree_commands) != 1:
    raise SystemExit(f"expected one dependency-tree query for a check-only run: {tree_commands}")
tree_offline_missing = [command for command in tree_commands if "--offline" not in command.split()]
if tree_offline_missing:
    raise SystemExit(f"dependency-tree queries must be offline: {tree_offline_missing}")
fetch_positions = [index for index, command in enumerate(measured) if command.split(maxsplit=1)[0] == "fetch"]
tree_positions = [index for index, command in enumerate(measured) if command.split(maxsplit=1)[0] == "tree"]
if not fetch_positions or not tree_positions or fetch_positions[0] > tree_positions[0]:
    raise SystemExit(f"host-target fetch must precede dependency-tree query: {measured}")
if any("-i" in command.split() for command in tree_commands):
    raise SystemExit(f"dependency-tree query unexpectedly used inverse traversal: {tree_commands}")
print("dependency-tree queries (current-only): 1")
PY

if ! grep -Fq 'cargo build --offline --locked' scripts/ci-compile-metrics.sh; then
  echo "release compile metrics must run cargo build offline" >&2
  exit 1
fi

baseline_out_dir="$tmp_dir/metrics-with-baseline"
baseline_cargo_home="$tmp_dir/shared-cargo-home"
FAKE_CARGO_LOG="$tmp_dir/baseline-cargo.log" \
FAKE_CARGO_HOME_LOG="$tmp_dir/baseline-cargo-home.log" \
FAKE_CARGO_INCREMENTAL_LOG="$tmp_dir/baseline-cargo-incremental.log" \
FAKE_CARGO_PROFILE_DEV_DEBUG_LOG="$tmp_dir/baseline-cargo-profile-dev-debug.log" \
FAKE_CARGO_PROFILE_TEST_DEBUG_LOG="$tmp_dir/baseline-cargo-profile-test-debug.log" \
FAKE_RUSTC_WRAPPER_LOG="$tmp_dir/baseline-cargo-rustc-wrapper.log" \
FAKE_CARGO_TARGET_LOG="$tmp_dir/baseline-cargo-target.log" \
FAKE_HOST_TARGET="$host_target" \
FAKE_COMMIT_OID="$expected_commit_oid" \
FAKE_BASELINE_COMMIT_OID="$expected_commit_oid" \
FAKE_GIT_WORKTREE_ADD_LOG="$tmp_dir/baseline-git-worktree-add.log" \
FAKE_CARGO_TREE_FIXTURE=none \
CARGO_INCREMENTAL=1 \
CARGO_PROFILE_DEV_DEBUG=2 \
CARGO_PROFILE_TEST_DEBUG=2 \
RUSTC_WRAPPER=hostile-wrapper \
CARGO_HOME="$baseline_cargo_home" PATH="$fake_bin:$PATH" \
  ./scripts/ci-compile-metrics.sh \
    --package fake_library \
    --out-dir "$baseline_out_dir" \
    --check-only \
    --baseline-ref HEAD

python3 - \
  "$tmp_dir/baseline-cargo-incremental.log" \
  "$tmp_dir/baseline-cargo-profile-dev-debug.log" \
  "$tmp_dir/baseline-cargo-profile-test-debug.log" \
  "$tmp_dir/baseline-cargo-rustc-wrapper.log" <<'PY'
from pathlib import Path
import sys

for path_arg, variable in zip(
    sys.argv[1:],
    (
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
        "RUSTC_WRAPPER",
    ),
):
    values = [line for line in Path(path_arg).read_text(encoding="utf-8").splitlines() if line]
    expected = "<unset>" if variable == "RUSTC_WRAPPER" else "0"
    if not values or set(values) != {expected}:
        raise SystemExit(f"{variable} was not normalized for every current/baseline Cargo call: {values}")
print("compile environment (current+baseline): all Cargo calls normalized")
PY

python3 - "$tmp_dir/baseline-git-worktree-add.log" "$expected_commit_oid" <<'PY'
from pathlib import Path
import sys

worktree_adds = [line for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line]
expected_baseline_ref = sys.argv[2]
if not worktree_adds or worktree_adds[-1].split()[-1] != expected_baseline_ref:
    raise SystemExit(
        "symbolic baseline ref was not normalized before worktree creation: "
        f"{worktree_adds}"
    )
print("symbolic baseline ref normalization: OK")
PY

python3 - \
  "$baseline_out_dir/baseline.metrics.json" \
  "$baseline_out_dir/comparison.json" \
  "$expected_commit_oid" <<'PY'
import json
from pathlib import Path
import sys

metrics = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert metrics["wasmtime_present"] is False
assert metrics["wasm_executor_present"] is False

comparison = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
expected_baseline_ref = sys.argv[3]
expected_identity = {
    "package": "fake_library",
    "binary": None,
    "check_only": True,
    "no_default_features": False,
    "warm_check_enabled": False,
}
assert comparison["measurement_identity"] == expected_identity
assert {
    field: comparison["current"][field] for field in expected_identity
} == expected_identity
assert {
    field: comparison["baseline"][field] for field in expected_identity
} == expected_identity
assert comparison["current_commit_oid"] == expected_baseline_ref
assert comparison["baseline_ref"] == expected_baseline_ref
assert comparison["baseline_commit_oid"] == expected_baseline_ref
print("measurement identity matching contract: OK")
PY

python3 - "$tmp_dir/baseline-cargo-home.log" "$baseline_cargo_home" "$tmp_dir/baseline-cargo-target.log" <<'PY'
from pathlib import Path
import sys

homes = [line for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line]
expected = str(Path(sys.argv[2]).expanduser().resolve())
if not homes:
    raise SystemExit("baseline run did not record any Cargo home paths")
if set(homes) != {expected}:
    raise SystemExit(f"compile metrics did not reuse caller Cargo home: {homes}")

targets = [line for line in Path(sys.argv[3]).read_text(encoding="utf-8").splitlines() if line]
if len(set(targets)) != 2:
    raise SystemExit(f"current and baseline target directories were not isolated: {targets}")
PY

python3 - "$tmp_dir/baseline-cargo.log" <<'PY'
from pathlib import Path
import sys

commands = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
tree_positions = [index for index, command in enumerate(commands) if command.split(maxsplit=1)[0] == "tree"]
tree_commands = [commands[index] for index in tree_positions]
if len(tree_commands) != 2:
    raise SystemExit(
        "expected one dependency-tree query per checkout in a baseline run: "
        f"{tree_commands}"
    )
if any("-i" in command.split() for command in tree_commands):
    raise SystemExit(f"dependency-tree queries unexpectedly used inverse traversal: {tree_commands}")
if any("--offline" not in command.split() for command in tree_commands):
    raise SystemExit(f"dependency-tree queries must be offline: {tree_commands}")
fetch_positions = [index for index, command in enumerate(commands) if command.split(maxsplit=1)[0] == "fetch"]
if len(fetch_positions) != len(tree_commands):
    raise SystemExit(
        "expected one host-target fetch per checkout in a baseline run: "
        f"fetches={fetch_positions}, trees={tree_commands}"
    )
for checkout_index, tree_position in enumerate(tree_positions):
    segment_start = 0 if checkout_index == 0 else tree_positions[checkout_index - 1] + 1
    segment = commands[segment_start : tree_position + 1]
    segment_fetches = [
        index
        for index, command in enumerate(segment, start=segment_start)
        if command.split(maxsplit=1)[0] == "fetch"
    ]
    if len(segment_fetches) != 1 or segment_fetches[0] >= tree_position:
        raise SystemExit(
            "each checkout segment must fetch before its dependency-tree query: "
            f"segment={segment}"
        )
print("dependency-tree queries (current+baseline): 2")
PY

python3 - "$tmp_dir/comparison.json" "$expected_commit_oid" "$expected_baseline_oid" <<'PY'
import json
from copy import deepcopy
from pathlib import Path
import subprocess
import sys

comparison_path = Path(sys.argv[1])
current_commit_oid = sys.argv[2]
baseline_commit_oid = sys.argv[3]
matching_comparison = {
    "measurement_identity": {
        "package": "fake_library",
        "binary": None,
        "check_only": True,
        "no_default_features": False,
    },
    "current_commit_oid": current_commit_oid,
    "current": {
        "package": "fake_library",
        "binary": None,
        "check_only": True,
        "no_default_features": False,
        "commit_oid": current_commit_oid,
        "wasmtime_present": False,
    },
    "baseline_ref": baseline_commit_oid,
    "baseline_commit_oid": baseline_commit_oid,
    "baseline": {
        "package": "fake_library",
        "binary": None,
        "check_only": True,
        "no_default_features": False,
        "commit_oid": baseline_commit_oid,
        "package_count": 10,
    },
    "metric_rows": [
        {
            "metric": "package_count",
            "baseline": 10,
            "current": 20,
            "delta": 10,
            "percent": 100.0,
        }
    ],
}


def run_gate(payload):
    comparison_path.write_text(json.dumps(payload), encoding="utf-8")
    return subprocess.run(
        [
            "python3",
            "scripts/ci-compile-metrics-gate.py",
            "--comparison",
            str(comparison_path),
            "--max-package-count-regression-pct",
            "100",
        ],
        capture_output=True,
        text=True,
        check=False,
    )


def run_gate_without_thresholds(payload):
    comparison_path.write_text(json.dumps(payload), encoding="utf-8")
    return subprocess.run(
        [
            "python3",
            "scripts/ci-compile-metrics-gate.py",
            "--comparison",
            str(comparison_path),
        ],
        capture_output=True,
        text=True,
        check=False,
    )


def assert_exact_gate_failure(payload, expected_failure, label):
    result = run_gate_without_thresholds(payload)
    expected_output = f"gate: FAIL: {expected_failure}\n"
    if result.returncode != 1 or result.stdout != expected_output or result.stderr:
        raise SystemExit(
            f"{label} did not fail closed with exact output: "
            f"{result.stdout}{result.stderr}"
        )


comparison_path.write_text(json.dumps(matching_comparison), encoding="utf-8")

for invalid in ("NaN", "inf", "-1"):
    result = subprocess.run(
        [
            "python3",
            "scripts/ci-compile-metrics-gate.py",
            "--comparison",
            str(comparison_path),
            "--max-package-count-regression-pct",
            invalid,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode == 0:
        raise SystemExit(f"invalid threshold accepted: {invalid}")
    if "finite and non-negative" not in result.stderr:
        raise SystemExit(
            f"invalid threshold error omitted contract wording for {invalid}: "
            f"{result.stderr!r}"
        )

passing = run_gate(matching_comparison)
if passing.returncode != 0:
    raise SystemExit(f"valid threshold unexpectedly failed: {passing.stdout}{passing.stderr}")

missing_metric_rows = deepcopy(matching_comparison)
missing_metric_rows.pop("metric_rows")
assert_exact_gate_failure(
    missing_metric_rows,
    "comparison is missing metric_rows",
    "omitted metric_rows",
)

explicit_empty_metric_rows = deepcopy(matching_comparison)
explicit_empty_metric_rows["metric_rows"] = []
explicit_empty = run_gate_without_thresholds(explicit_empty_metric_rows)
expected_empty_output = "gate: PASS: compile metrics are within configured thresholds\n"
if (
    explicit_empty.returncode != 0
    or explicit_empty.stdout != expected_empty_output
    or explicit_empty.stderr
):
    raise SystemExit(
        "explicit empty metric_rows did not remain distinguishable from omission: "
        f"{explicit_empty.stdout}{explicit_empty.stderr}"
    )

non_array_metric_rows = deepcopy(matching_comparison)
non_array_metric_rows["metric_rows"] = None
assert_exact_gate_failure(
    non_array_metric_rows,
    "comparison metric_rows must be a JSON array",
    "non-array metric_rows",
)

non_object_metric_row = deepcopy(matching_comparison)
non_object_metric_row["metric_rows"] = [[]]
assert_exact_gate_failure(
    non_object_metric_row,
    "comparison metric_rows entry 0 must be a JSON object",
    "non-object metric row",
)

unsupported_metric_row = deepcopy(matching_comparison)
unsupported_metric_row["metric_rows"] = [{"metric": "unsupported"}]
assert_exact_gate_failure(
    unsupported_metric_row,
    "comparison metric_rows entry 0 metric is unsupported: unsupported",
    "unsupported metric row",
)

empty_metric_name = deepcopy(matching_comparison)
empty_metric_name["metric_rows"] = [{"metric": ""}]
assert_exact_gate_failure(
    empty_metric_name,
    "comparison metric_rows entry 0 metric must be a non-empty string",
    "empty metric name",
)

invalid_numeric_fields = (
    ("baseline", "finite non-negative number"),
    ("current", "finite non-negative number"),
    ("delta", "finite number"),
    ("percent", "finite number"),
)
for field, numeric_contract in invalid_numeric_fields:
    invalid_numeric = deepcopy(matching_comparison)
    invalid_numeric["metric_rows"][0][field] = "not-a-number"
    assert_exact_gate_failure(
        invalid_numeric,
        f"comparison row package_count {field} must be a {numeric_contract}",
        f"invalid unthresholded {field}",
    )

current_only_invalid_numeric = deepcopy(matching_comparison)
current_only_invalid_numeric["baseline"] = None
current_only_invalid_numeric["baseline_ref"] = None
current_only_invalid_numeric["baseline_commit_oid"] = None
current_only_invalid_numeric["metric_rows"][0]["current"] = "not-a-number"
assert_exact_gate_failure(
    current_only_invalid_numeric,
    "comparison row package_count current must be a finite non-negative number",
    "invalid current-only numeric field",
)

missing_metric_name = deepcopy(matching_comparison)
missing_metric_name["metric_rows"] = [{}]
assert_exact_gate_failure(
    missing_metric_name,
    "comparison metric_rows entry 0 metric must be a non-empty string",
    "missing metric name",
)

duplicate_metric_rows_exact = deepcopy(matching_comparison)
duplicate_metric_rows_exact["metric_rows"].append(
    deepcopy(duplicate_metric_rows_exact["metric_rows"][0])
)
assert_exact_gate_failure(
    duplicate_metric_rows_exact,
    "comparison metric_rows contains duplicate metric row for package_count",
    "duplicate metric rows",
)

duplicate_metric_rows = deepcopy(matching_comparison)
duplicate_metric_rows["metric_rows"].append(
    deepcopy(duplicate_metric_rows["metric_rows"][0])
)
duplicate = run_gate(duplicate_metric_rows)
if (
    duplicate.returncode == 0
    or "comparison metric_rows contains duplicate metric row for package_count"
    not in duplicate.stdout
    or duplicate.stderr
):
    raise SystemExit(
        "duplicate metric rows unexpectedly passed or used wrong error: "
        f"{duplicate.stdout}{duplicate.stderr}"
    )

malformed_metric_rows = deepcopy(matching_comparison)
malformed_metric_rows["metric_rows"] = [{}]
malformed = run_gate(malformed_metric_rows)
if (
    malformed.returncode == 0
    or "comparison metric_rows entry 0 metric must be a non-empty string"
    not in malformed.stdout
    or malformed.stderr
):
    raise SystemExit(
        "malformed metric row unexpectedly passed or used wrong error: "
        f"{malformed.stdout}{malformed.stderr}"
    )

stale_current = deepcopy(matching_comparison)
stale_current["current_commit_oid"] = baseline_commit_oid
stale_current["current"]["commit_oid"] = baseline_commit_oid
stale = run_gate(stale_current)
if (
    stale.returncode == 0
    or "current metrics commit_oid does not match repository HEAD" not in stale.stdout
    or "comparison current_commit_oid does not match repository HEAD" not in stale.stdout
):
    raise SystemExit(
        "stale current commit provenance unexpectedly passed or used wrong error: "
        f"{stale.stdout}{stale.stderr}"
    )

missing_identity = dict(matching_comparison)
missing_identity.pop("measurement_identity")
missing = run_gate(missing_identity)
if (
    missing.returncode == 0
    or "comparison is missing measurement_identity" not in missing.stdout
):
    raise SystemExit(
        "missing measurement identity unexpectedly passed or used wrong error: "
        f"{missing.stdout}{missing.stderr}"
    )

mismatched_identity = json.loads(json.dumps(matching_comparison))
mismatched_identity["baseline"]["no_default_features"] = True
mismatched = run_gate(mismatched_identity)
if (
    mismatched.returncode == 0
    or "current/baseline measurement identity mismatch" not in mismatched.stdout
):
    raise SystemExit(
        "mismatched measurement identity unexpectedly passed or used wrong error: "
        f"{mismatched.stdout}{mismatched.stderr}"
    )

missing_current_commit = deepcopy(matching_comparison)
missing_current_commit.pop("current_commit_oid")
missing = run_gate(missing_current_commit)
if (
    missing.returncode == 0
    or "comparison current_commit_oid must be a non-empty string" not in missing.stdout
):
    raise SystemExit(
        "missing current commit provenance unexpectedly passed or used wrong error: "
        f"{missing.stdout}{missing.stderr}"
    )

missing_current_metrics_commit = deepcopy(matching_comparison)
missing_current_metrics_commit["current"].pop("commit_oid")
missing = run_gate(missing_current_metrics_commit)
if (
    missing.returncode == 0
    or "current metrics commit_oid must be a non-empty string" not in missing.stdout
):
    raise SystemExit(
        "missing current metrics commit provenance unexpectedly passed or used wrong error: "
        f"{missing.stdout}{missing.stderr}"
    )

mismatched_current_commit = deepcopy(matching_comparison)
mismatched_current_commit["current_commit_oid"] = baseline_commit_oid
mismatched = run_gate(mismatched_current_commit)
if (
    mismatched.returncode == 0
    or "comparison current_commit_oid does not match current metrics commit_oid"
    not in mismatched.stdout
):
    raise SystemExit(
        "mismatched current commit provenance unexpectedly passed or used wrong error: "
        f"{mismatched.stdout}{mismatched.stderr}"
    )

missing_baseline_commit = deepcopy(matching_comparison)
missing_baseline_commit.pop("baseline_commit_oid")
missing = run_gate(missing_baseline_commit)
if (
    missing.returncode == 0
    or "comparison baseline_commit_oid must be a non-empty string with baseline metrics"
    not in missing.stdout
):
    raise SystemExit(
        "missing baseline commit provenance unexpectedly passed or used wrong error: "
        f"{missing.stdout}{missing.stderr}"
    )

mismatched_baseline_commit = deepcopy(matching_comparison)
mismatched_baseline_commit["baseline"]["commit_oid"] = current_commit_oid
mismatched = run_gate(mismatched_baseline_commit)
if (
    mismatched.returncode == 0
    or "comparison baseline_commit_oid does not match baseline metrics commit_oid"
    not in mismatched.stdout
):
    raise SystemExit(
        "mismatched baseline commit provenance unexpectedly passed or used wrong error: "
        f"{mismatched.stdout}{mismatched.stderr}"
    )

invalid_current_whitespace = deepcopy(matching_comparison)
invalid_current_whitespace["current_commit_oid"] = f" {current_commit_oid}"
invalid = run_gate(invalid_current_whitespace)
if (
    invalid.returncode == 0
    or "comparison current_commit_oid must be a canonical full Git OID" not in invalid.stdout
):
    raise SystemExit(
        "whitespace-padded current commit OID unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

invalid_current_path = deepcopy(matching_comparison)
invalid_current_path["current"]["commit_oid"] = "../HEAD"
invalid = run_gate(invalid_current_path)
if (
    invalid.returncode == 0
    or "current metrics commit_oid must be a canonical full Git OID" not in invalid.stdout
):
    raise SystemExit(
        "path-like current commit OID unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

invalid_baseline_length = deepcopy(matching_comparison)
invalid_baseline_length["baseline_commit_oid"] = baseline_commit_oid[:-1]
invalid = run_gate(invalid_baseline_length)
if (
    invalid.returncode == 0
    or "comparison baseline_commit_oid must be a canonical full Git OID" not in invalid.stdout
):
    raise SystemExit(
        "wrong-length baseline commit OID unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

invalid_baseline_ref = deepcopy(matching_comparison)
invalid_baseline_ref["baseline_ref"] = "0" * len(baseline_commit_oid)
invalid = run_gate(invalid_baseline_ref)
if (
    invalid.returncode == 0
    or "comparison baseline_ref does not resolve to a commit object" not in invalid.stdout
):
    raise SystemExit(
        "nonexistent baseline ref unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

mismatched_baseline_ref = deepcopy(matching_comparison)
mismatched_baseline_ref["baseline_ref"] = current_commit_oid
invalid = run_gate(mismatched_baseline_ref)
if (
    invalid.returncode == 0
    or "comparison baseline_ref does not match baseline_commit_oid" not in invalid.stdout
):
    raise SystemExit(
        "mismatched baseline ref unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

invalid_identity_cases = (
    (
        "boolean-string",
        {"check_only": "true"},
        "check_only must be a boolean",
    ),
    (
        "empty-package",
        {"package": ""},
        "package must be a non-empty string",
    ),
    (
        "integer-binary",
        {"binary": 7},
        "binary must be null or a non-empty string",
    ),
    (
        "check-only-with-binary",
        {"check_only": True, "binary": "fake_binary"},
        "check_only requires binary to be null",
    ),
    (
        "release-without-binary",
        {"check_only": False, "binary": None},
        "release-build requires binary to be a non-empty string",
    ),
)
for case_name, updates, expected_error in invalid_identity_cases:
    invalid = deepcopy(matching_comparison)
    for section in ("measurement_identity", "current", "baseline"):
        invalid[section].update(updates)
    result = run_gate(invalid)
    if result.returncode == 0 or expected_error not in result.stdout:
        raise SystemExit(
            f"invalid {case_name} measurement identity unexpectedly passed or "
            f"used wrong error: {result.stdout}{result.stderr}"
        )

non_finite_percent = deepcopy(matching_comparison)
non_finite_percent["metric_rows"][0]["percent"] = float("nan")
non_finite = run_gate(non_finite_percent)
expected_non_finite_output = (
    "gate: FAIL: comparison row package_count percent must be a finite number\n"
)
if (
    non_finite.returncode == 0
    or non_finite.stdout != expected_non_finite_output
    or non_finite.stderr
):
    raise SystemExit(
        "non-finite comparison percentage did not fail closed with the "
        "deterministic diagnostic: "
        f"{non_finite.stdout}{non_finite.stderr}"
    )

negative_metric_cases = (
    (
        "baseline",
        -1,
        {"current": 9, "delta": 10, "percent": -1000.0},
    ),
    (
        "current",
        -1,
        {"baseline": 10, "delta": -11, "percent": -110.0},
    ),
)
for field, value, consistent_updates in negative_metric_cases:
    negative_metric = deepcopy(matching_comparison)
    negative_metric["metric_rows"][0][field] = value
    negative_metric["metric_rows"][0].update(consistent_updates)
    result = run_gate(negative_metric)
    expected_fragment = (
        f"comparison row package_count {field} must be a finite non-negative number"
    )
    if result.returncode == 0 or expected_fragment not in result.stdout or result.stderr:
        raise SystemExit(
            f"negative comparison {field} value unexpectedly passed or used wrong error: "
            f"{result.stdout}{result.stderr}"
        )

inconsistent_delta = deepcopy(matching_comparison)
inconsistent_delta["metric_rows"][0]["delta"] = 9
invalid = run_gate(inconsistent_delta)
if (
    invalid.returncode == 0
    or "comparison row package_count delta must equal current - baseline" not in invalid.stdout
    or invalid.stderr
):
    raise SystemExit(
        "inconsistent comparison delta unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

print("ci-compile-metrics-gate threshold contract: OK")
print("measurement identity missing/mismatch contract: OK")
print("measurement identity semantic validation contract: OK")
print("non-finite metric payload contract: OK")
print("negative and inconsistent metric payload contract: OK")

fabricated_percent = deepcopy(matching_comparison)
fabricated_percent["metric_rows"][0].update(
    {
        "baseline": 10,
        "current": 20,
        "delta": 10,
        # ci-compile-metrics.sh computes ((current - baseline) / baseline) * 100.
        "percent": 1.0,
    }
)
invalid = run_gate(fabricated_percent)
if (
    invalid.returncode == 0
    or "comparison row package_count percent must equal ((current - baseline) / baseline) * 100"
    not in invalid.stdout
    or invalid.stderr
):
    raise SystemExit(
        "fabricated comparison percentage unexpectedly passed or used wrong error: "
        f"{invalid.stdout}{invalid.stderr}"
    )

print("percent arithmetic consistency contract: OK")

oversized_baseline = deepcopy(matching_comparison)
oversized_baseline["metric_rows"][0].update(
    {
        "baseline": 10**1000,
        "current": 20,
        "delta": 0,
        "percent": 0.0,
    }
)
invalid = run_gate(oversized_baseline)
expected_oversized_output = (
    "gate: FAIL: comparison row package_count baseline must be a finite non-negative number\n"
)
if (
    invalid.returncode == 0
    or invalid.stdout != expected_oversized_output
    or invalid.stderr
):
    raise SystemExit(
        "oversized comparison baseline did not fail closed with a deterministic "
        "diagnostic: "
        f"{invalid.stdout}{invalid.stderr}"
    )

print("oversized metric payload contract: OK")

oversized_percent = deepcopy(matching_comparison)
oversized_percent["metric_rows"][0].update(
    {
        "baseline": 10,
        "current": 20,
        "delta": 10,
        "percent": 10**1000,
    }
)
invalid = run_gate(oversized_percent)
expected_oversized_percent_output = (
    "gate: FAIL: comparison row package_count percent must be a finite number\n"
)
if (
    invalid.returncode == 0
    or invalid.stdout != expected_oversized_percent_output
    or invalid.stderr
):
    raise SystemExit(
        "oversized comparison percent did not fail closed with a deterministic "
        "diagnostic: "
        f"{invalid.stdout}{invalid.stderr}"
    )

print("oversized percent payload contract: OK")
PY

# V2 default compatibility: new payloads are explicit even when warm mode is
# not requested, and disabled warm timing is represented by JSON null.
python3 - "$out_dir/current.metrics.json" "$out_dir/comparison.json" <<'PY'
import json
from pathlib import Path
import sys

metrics = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
comparison = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
if metrics.get("schema_version") != 2:
    raise SystemExit("V2 default compatibility: metrics schema_version must be 2")
if metrics.get("warm_check_enabled") is not False:
    raise SystemExit(
        "V2 default compatibility: warm_check_enabled must be false without the opt-in"
    )
if metrics.get("cargo_check_warm_seconds", object()) is not None:
    raise SystemExit(
        "V2 default compatibility: disabled warm duration must be JSON null"
    )
if comparison.get("schema_version") != 2:
    raise SystemExit("V2 default compatibility: comparison schema_version must be 2")
if comparison.get("measurement_identity", {}).get("warm_check_enabled") is not False:
    raise SystemExit(
        "V2 default compatibility: comparison identity must retain disabled warm mode"
    )
if comparison.get("metric_rows") != []:
    raise SystemExit(
        "V2 default compatibility: disabled warm mode must not emit a warm row"
    )
PY

python3 - "$out_dir/summary.md" <<'PY'
from pathlib import Path
import sys

summary = Path(sys.argv[1]).read_text(encoding="utf-8")
if "- Warm/no-op `cargo check` enabled: `false` (not measured)" not in summary:
    raise SystemExit(
        "V2 default compatibility: summary must label disabled warm timing as not measured"
    )
PY

# Reuse the existing deterministic fake surfaces for warm command identity,
# artifact reuse, release ordering, and current/baseline target isolation.
v2_fake_bin="$tmp_dir/v2-bin"
mkdir -p "$v2_fake_bin"
cat >"$v2_fake_bin/cargo" <<'FAKE_V2_CARGO'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >>"${FAKE_CARGO_LOG:?}"
if [[ -n "${FAKE_CARGO_TARGET_LOG:-}" && -n "${CARGO_TARGET_DIR:-}" ]]; then
  printf '%s\n' "${CARGO_TARGET_DIR:-<unset>}" >>"$FAKE_CARGO_TARGET_LOG"
fi
if [[ -n "${FAKE_CARGO_INCREMENTAL_LOG:-}" ]]; then
  printf '%s\n' "${CARGO_INCREMENTAL:-<unset>}" >>"$FAKE_CARGO_INCREMENTAL_LOG"
fi
if [[ -n "${FAKE_CARGO_PROFILE_DEV_DEBUG_LOG:-}" ]]; then
  printf '%s\n' "${CARGO_PROFILE_DEV_DEBUG:-<unset>}" >>"$FAKE_CARGO_PROFILE_DEV_DEBUG_LOG"
fi
if [[ -n "${FAKE_CARGO_PROFILE_TEST_DEBUG_LOG:-}" ]]; then
  printf '%s\n' "${CARGO_PROFILE_TEST_DEBUG:-<unset>}" >>"$FAKE_CARGO_PROFILE_TEST_DEBUG_LOG"
fi
if [[ -n "${FAKE_RUSTC_WRAPPER_LOG:-}" ]]; then
  printf '%s\n' "${RUSTC_WRAPPER:-<unset>}" >>"$FAKE_RUSTC_WRAPPER_LOG"
fi

case "${1:-}" in
  fetch)
    [[ "$*" == *"--locked"* && "$*" == *"--target "* ]] || {
      echo "fake V2 fetch was not locked to a target" >&2
      exit 1
    }
    ;;
  tree)
    [[ "$*" == *"--offline"* && "$*" == *"--locked"* ]] || {
      echo "fake V2 tree was not offline/locked" >&2
      exit 1
    }
    printf 'fake-package\nfake-dependency\n'
    ;;
  check)
    if [[ -n "${FAKE_CARGO_CWD_LOG:-}" ]]; then
      printf '%s\n' "$PWD" >>"$FAKE_CARGO_CWD_LOG"
    fi
    check_count=0
    if [[ -f "${FAKE_CARGO_CHECK_COUNT_LOG:?}" ]]; then
      check_count=$(<"$FAKE_CARGO_CHECK_COUNT_LOG")
    fi
    check_count=$((check_count + 1))
    printf '%s\n' "$check_count" >"$FAKE_CARGO_CHECK_COUNT_LOG"
    if [[ -n "${FAKE_CHECK_REUSE_MARKER:-}" ]]; then
      marker_path="${CARGO_TARGET_DIR:?}/${FAKE_CHECK_REUSE_MARKER}"
      if [[ -e "$marker_path" ]]; then
        printf 'warm-artifact-reused\n' >>"${FAKE_CHECK_REUSE_LOG:?}"
      else
        : >"$marker_path"
        printf 'cold-artifact-created\n' >>"${FAKE_CHECK_REUSE_LOG:?}"
      fi
    fi
    if [[ "${FAKE_FAIL_CHECK_INDEX:-}" == "$check_count" ]]; then
      echo "fake V2 check failure at invocation $check_count" >&2
      exit 17
    fi
    if [[ -n "${FAKE_MUTATION_MODE:-}" ]]; then
      case "$FAKE_MUTATION_MODE" in
        content)
          printf 'mutated content\n' >>"$PWD/README.md"
          ;;
        mode)
          chmod 755 "$PWD/README.md"
          ;;
        type)
          rm -f "$PWD/README.md"
          mkdir "$PWD/README.md"
          ;;
        symlink)
          rm -f "$PWD/.v2-tracked-symlink"
          ln -s /tmp/v2-mutated-target "$PWD/.v2-tracked-symlink"
          ;;
        untracked)
          printf 'untracked mutation\n' >"$PWD/.v2-untracked-mutation"
          ;;
        ignored)
          printf 'ignored mutation\n' >"$PWD/config.toml"
          ;;
        gitlink)
          printf 'nested gitlink mutation\n' >"$PWD/third_party/agent-os/.v2-gitlink-mutation"
          ;;
        *)
          echo "unknown fake V2 mutation mode: $FAKE_MUTATION_MODE" >&2
          exit 1
          ;;
      esac
    fi
    printf 'fake V2 cargo check %s\n' "$check_count"
    ;;
  build)
    mkdir -p "${CARGO_TARGET_DIR:?}/release"
    : >"${CARGO_TARGET_DIR}/release/fake_library"
    printf 'fake V2 cargo build\n'
    ;;
  *)
    echo "unexpected fake V2 cargo command: $*" >&2
    exit 1
    ;;
esac
FAKE_V2_CARGO
chmod +x "$v2_fake_bin/cargo"

v2_run_metrics() {
  local run_out="$1" run_log="$2" run_count="$3" run_reuse="$4" run_target="$5" run_cwd="$6"
  shift 6
  FAKE_CARGO_LOG="$run_log" \
  FAKE_CARGO_CHECK_COUNT_LOG="$run_count" \
  FAKE_CHECK_REUSE_LOG="$run_reuse" \
  FAKE_CHECK_REUSE_MARKER="v2-check-artifact.marker" \
  FAKE_CARGO_TARGET_LOG="$run_target" \
  FAKE_CARGO_CWD_LOG="$run_cwd" \
  FAKE_CARGO_INCREMENTAL_LOG="${run_log}.incremental" \
  FAKE_CARGO_PROFILE_DEV_DEBUG_LOG="${run_log}.profile-dev-debug" \
  FAKE_CARGO_PROFILE_TEST_DEBUG_LOG="${run_log}.profile-test-debug" \
  FAKE_RUSTC_WRAPPER_LOG="${run_log}.rustc-wrapper" \
  FAKE_HOST_TARGET="$host_target" \
  FAKE_COMMIT_OID="$expected_commit_oid" \
  FAKE_BASELINE_COMMIT_OID="$expected_commit_oid" \
  PATH="$v2_fake_bin:$fake_bin:$PATH" \
    ./scripts/ci-compile-metrics.sh --package fake_library --out-dir "$run_out" "$@"
}

# A caller-controlled TMPDIR may accidentally point inside the checkout.  The
# harness must reject that unsafe layout (or relocate its run-owned targets)
# before a Cargo target is created there.
inside_tmp_parent=$(mktemp -d "$repo_root/.compile-metrics-contract-tmpdir.XXXXXX")
inside_tmp_out="$tmp_dir/v2-tmpdir-inside-out"
inside_tmp_target_log="$tmp_dir/v2-tmpdir-inside-target.log"
inside_tmp_status=0
if TMPDIR="$inside_tmp_parent" \
  v2_run_metrics \
    "$inside_tmp_out" \
    "$tmp_dir/v2-tmpdir-inside-cargo.log" \
    "$tmp_dir/v2-tmpdir-inside-count" \
    "$tmp_dir/v2-tmpdir-inside-reuse.log" \
    "$inside_tmp_target_log" \
    "$tmp_dir/v2-tmpdir-inside-cwd.log" \
    --check-only; then
  inside_tmp_status=0
else
  inside_tmp_status=$?
fi
inside_tmp_violation=""
if [[ -f "$inside_tmp_target_log" ]]; then
  while IFS= read -r target_path; do
    if [[ "$target_path" == "$repo_root"/* ]]; then
      inside_tmp_violation="$target_path"
      break
    fi
  done <"$inside_tmp_target_log"
fi
if ! rmdir "$inside_tmp_parent"; then
  echo "TMPDIR-inside-checkout fixture left unexpected data at its run-owned path: $inside_tmp_parent" >&2
  exit 1
fi
if [[ -n "$inside_tmp_violation" ]]; then
  echo "TMPDIR inside checkout created an in-checkout Cargo target: $inside_tmp_violation" >&2
  exit 1
fi
if [[ "$inside_tmp_status" -ne 0 && -e "$inside_tmp_out/current.metrics.json" ]]; then
  echo "TMPDIR-inside-checkout failure emitted successful metrics JSON" >&2
  exit 1
fi
echo "TMPDIR target isolation contract: OK"

warm_out_dir="$tmp_dir/v2-warm-check"
warm_cargo_log="$tmp_dir/v2-warm-cargo.log"
warm_check_count="$tmp_dir/v2-warm-check-count"
warm_reuse_log="$tmp_dir/v2-warm-reuse.log"
warm_target_log="$tmp_dir/v2-warm-target.log"
warm_cwd_log="$tmp_dir/v2-warm-cwd.log"
v2_run_metrics "$warm_out_dir" "$warm_cargo_log" "$warm_check_count" "$warm_reuse_log" "$warm_target_log" "$warm_cwd_log" --check-only --measure-warm-check

python3 - "$warm_out_dir/current.metrics.json" "$warm_out_dir/comparison.json" "$warm_out_dir/summary.md" "$warm_cargo_log" "$warm_reuse_log" "$warm_target_log" "$warm_cwd_log" "$repo_root" <<'PY'
import json
from pathlib import Path
import sys

metrics_path, comparison_path, summary_path, cargo_log_path, reuse_log_path, target_log_path, cwd_log_path, repo_root = sys.argv[1:]
metrics = json.loads(Path(metrics_path).read_text(encoding="utf-8"))
comparison = json.loads(Path(comparison_path).read_text(encoding="utf-8"))
commands = Path(cargo_log_path).read_text(encoding="utf-8").splitlines()
checks = [line for line in commands if line.startswith("check ")]
if len(checks) != 2 or checks[0] != checks[1]:
    raise SystemExit(f"warm check must repeat the identical command exactly once: {checks}")
if any("--offline" not in line or "--locked" not in line for line in checks):
    raise SystemExit(f"warm checks must both be offline and locked: {checks}")
check_positions = [i for i, line in enumerate(commands) if line.startswith("check ")]
if check_positions != [min(check_positions), max(check_positions)] or check_positions[1] != check_positions[0] + 1:
    raise SystemExit(f"warm check must immediately follow cold check: {commands}")
targets = Path(target_log_path).read_text(encoding="utf-8").splitlines()
if len(targets) != 2 or targets[0] != targets[1]:
    raise SystemExit(f"cold and warm checks must reuse one target: {targets}")
if Path(targets[0]).resolve().is_relative_to(Path(repo_root).resolve()):
    raise SystemExit(f"warm check target must be outside the checkout: {targets[0]}")
reuse = Path(reuse_log_path).read_text(encoding="utf-8").splitlines()
if reuse != ["cold-artifact-created", "warm-artifact-reused"]:
    raise SystemExit(f"warm check did not observe the cold artifact marker: {reuse}")
if metrics.get("schema_version") != 2 or metrics.get("warm_check_enabled") is not True:
    raise SystemExit(f"warm metrics did not declare V2 enabled mode: {metrics}")
if not isinstance(metrics.get("cargo_check_warm_seconds"), (int, float)):
    raise SystemExit(f"warm metrics did not emit a numeric duration: {metrics}")
if comparison.get("schema_version") != 2 or comparison.get("measurement_identity", {}).get("warm_check_enabled") is not True:
    raise SystemExit("warm comparison did not bind V2 enabled identity")
summary = Path(summary_path).read_text(encoding="utf-8")
if "warm" not in summary.lower() or "cold `cargo check`" not in summary:
    raise SystemExit(f"warm summary is missing warm/cold labels: {summary}")
if "No baseline ref was provided" not in summary:
    raise SystemExit("current-only warm summary must identify the missing baseline")
if Path(cwd_log_path).read_text(encoding="utf-8").splitlines()[-1] != repo_root:
    raise SystemExit("warm check did not run in the canonical checkout")
for suffix, variable in (
    (".incremental", "CARGO_INCREMENTAL"),
    (".profile-dev-debug", "CARGO_PROFILE_DEV_DEBUG"),
    (".profile-test-debug", "CARGO_PROFILE_TEST_DEBUG"),
    (".rustc-wrapper", "RUSTC_WRAPPER"),
):
    values = [
        line
        for line in Path(cargo_log_path + suffix).read_text(encoding="utf-8").splitlines()
        if line
    ]
    expected = "<unset>" if variable == "RUSTC_WRAPPER" else "0"
    if len(values) != len(commands) or set(values) != {expected}:
        raise SystemExit(
            f"{variable} was not normalized for every warm Cargo call: {values}"
        )
print("warm check command identity/reuse/schema contract: OK")
PY

release_out_dir="$tmp_dir/v2-warm-release"
release_cargo_log="$tmp_dir/v2-warm-release-cargo.log"
release_check_count="$tmp_dir/v2-warm-release-check-count"
release_reuse_log="$tmp_dir/v2-warm-release-reuse.log"
release_target_log="$tmp_dir/v2-warm-release-target.log"
release_cwd_log="$tmp_dir/v2-warm-release-cwd.log"
v2_run_metrics "$release_out_dir" "$release_cargo_log" "$release_check_count" "$release_reuse_log" "$release_target_log" "$release_cwd_log" --binary fake_library --measure-warm-check
python3 - "$release_out_dir/current.metrics.json" "$release_cargo_log" "$release_reuse_log" "$release_target_log" <<'PY'
import json
from pathlib import Path
import sys

metrics_path, cargo_log_path, reuse_log_path, target_log_path = sys.argv[1:]
metrics = json.loads(Path(metrics_path).read_text(encoding="utf-8"))
commands = Path(cargo_log_path).read_text(encoding="utf-8").splitlines()
check_positions = [i for i, command in enumerate(commands) if command.startswith("check ")]
build_positions = [i for i, command in enumerate(commands) if command.startswith("build ")]
if len(check_positions) != 2 or len(build_positions) != 1 or check_positions[1] >= build_positions[0]:
    raise SystemExit(f"release warm order must be check, check, build: {commands}")
targets = Path(target_log_path).read_text(encoding="utf-8").splitlines()
if len(targets) != 3 or targets[0] != targets[1] or targets[1] == targets[2]:
    raise SystemExit(f"release warm check/build targets are not isolated: {targets}")
if Path(reuse_log_path).read_text(encoding="utf-8").splitlines() != ["cold-artifact-created", "warm-artifact-reused"]:
    raise SystemExit("release warm checks did not reuse their check artifact")
if metrics.get("cargo_build_release_seconds") is None or metrics.get("release_binary_bytes") is None:
    raise SystemExit("warm release measurement lost existing release metrics")
print("warm release ordering/target separation contract: OK")
PY

baseline_out_dir="$tmp_dir/v2-warm-baseline"
baseline_cargo_log="$tmp_dir/v2-warm-baseline-cargo.log"
baseline_check_count="$tmp_dir/v2-warm-baseline-check-count"
baseline_reuse_log="$tmp_dir/v2-warm-baseline-reuse.log"
baseline_target_log="$tmp_dir/v2-warm-baseline-target.log"
baseline_cwd_log="$tmp_dir/v2-warm-baseline-cwd.log"
v2_run_metrics "$baseline_out_dir" "$baseline_cargo_log" "$baseline_check_count" "$baseline_reuse_log" "$baseline_target_log" "$baseline_cwd_log" --check-only --measure-warm-check --baseline-ref HEAD
python3 - "$baseline_out_dir/comparison.json" "$baseline_out_dir/summary.md" "$baseline_cargo_log" "$baseline_reuse_log" "$baseline_target_log" "$baseline_cwd_log" <<'PY'
import json
from pathlib import Path
import sys

comparison_path, summary_path, cargo_log_path, reuse_log_path, target_log_path, cwd_log_path = sys.argv[1:]
comparison = json.loads(Path(comparison_path).read_text(encoding="utf-8"))
commands = Path(cargo_log_path).read_text(encoding="utf-8").splitlines()
checks = [line for line in commands if line.startswith("check ")]
if len(checks) != 4 or len(set(checks)) != 1:
    raise SystemExit(f"current/baseline warm checks lost command identity: {checks}")
targets = Path(target_log_path).read_text(encoding="utf-8").splitlines()
if len(targets) != 4 or targets[0] != targets[1] or targets[2] != targets[3] or targets[0] == targets[2]:
    raise SystemExit(f"current/baseline warm targets are not isolated: {targets}")
cwd_paths = Path(cwd_log_path).read_text(encoding="utf-8").splitlines()
if len(cwd_paths) != 4 or len(set(cwd_paths)) != 2:
    raise SystemExit(f"current/baseline checkouts are not distinct: {cwd_paths}")
if Path(reuse_log_path).read_text(encoding="utf-8").splitlines() != ["cold-artifact-created", "warm-artifact-reused"] * 2:
    raise SystemExit("current/baseline warm reuse crossed checkout boundaries")
if comparison.get("baseline") is None or comparison.get("schema_version") != 2:
    raise SystemExit("warm baseline comparison must carry both V2 payloads")
for suffix, variable in (
    (".incremental", "CARGO_INCREMENTAL"),
    (".profile-dev-debug", "CARGO_PROFILE_DEV_DEBUG"),
    (".profile-test-debug", "CARGO_PROFILE_TEST_DEBUG"),
    (".rustc-wrapper", "RUSTC_WRAPPER"),
):
    values = [
        line
        for line in Path(cargo_log_path + suffix).read_text(encoding="utf-8").splitlines()
        if line
    ]
    expected = "<unset>" if variable == "RUSTC_WRAPPER" else "0"
    if len(values) != len(commands) or any(value != expected for value in values):
        raise SystemExit(
            f"{variable} was not normalized for paired current/baseline Cargo calls: {values}"
        )
    check_values = [
        value
        for command, value in zip(commands, values)
        if command.startswith("check ")
    ]
    if check_values[:2] != [expected] * 2 or check_values[2:] != [expected] * 2:
        raise SystemExit(
            f"{variable} was not normalized for paired current/baseline warm checks: {check_values}"
        )
summary = Path(summary_path).read_text(encoding="utf-8")
if "Compared against baseline ref" not in summary:
    raise SystemExit("paired warm summary must identify its baseline")
if "| `cargo_check_warm_seconds` |" not in summary or "Warm timing is" not in summary:
    raise SystemExit("paired warm summary must expose the warm row and warm semantics")
print("current/baseline warm target isolation contract: OK")
PY

# Gate V2 schema, legacy policy, static known rows, payload binding, report
# only behavior, zero-baseline handling, and explicit threshold behavior.
v2_gate_fixture="$tmp_dir/v2-gate-fixture.json"
python3 - "$v2_gate_fixture" "$expected_commit_oid" <<'PY'
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
oid = sys.argv[2]
identity = {
    "package": "fake_library",
    "binary": None,
    "check_only": True,
    "no_default_features": False,
    "warm_check_enabled": True,
}
current = {
    **identity,
    "schema_version": 2,
    "commit_oid": oid,
    "package_count": 10,
    "cargo_check_seconds": 1.0,
    "cargo_build_release_seconds": None,
    "release_binary_bytes": None,
    "cargo_check_warm_seconds": 0.0,
    "wasmtime_present": False,
    "wasm_executor_present": False,
}
baseline = dict(current)
comparison = {
    "schema_version": 2,
    "measurement_identity": identity,
    "current_commit_oid": oid,
    "current": current,
    "baseline_ref": oid,
    "baseline_commit_oid": oid,
    "baseline": baseline,
    "metric_rows": [
        {"metric": "package_count", "baseline": 10, "current": 10, "delta": 0, "percent": 0.0},
        {"metric": "cargo_check_seconds", "baseline": 1.0, "current": 1.0, "delta": 0.0, "percent": 0.0},
        {"metric": "cargo_check_warm_seconds", "baseline": 0.0, "current": 0.0, "delta": 0.0, "percent": None},
    ],
}
path.write_text(json.dumps(comparison), encoding="utf-8")
PY

python3 - "$v2_gate_fixture" <<'PY'
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys

comparison_path = Path(sys.argv[1])
base = json.loads(comparison_path.read_text(encoding="utf-8"))


def run(payload, *args):
    comparison_path.write_text(json.dumps(payload), encoding="utf-8")
    return subprocess.run(
        ["python3", "scripts/ci-compile-metrics-gate.py", "--comparison", str(comparison_path), *args],
        capture_output=True,
        text=True,
        check=False,
    )


def fail(payload, label, *args):
    result = run(payload, *args)
    if result.returncode == 0:
        raise SystemExit(f"{label} unexpectedly passed: {result.stdout}{result.stderr}")
    return result


report_only = run(base)
if report_only.returncode != 0:
    raise SystemExit(f"known warm report-only row must be accepted: {report_only.stdout}{report_only.stderr}")

# Current-only V2 validation must inspect the cold payload before returning a
# no-baseline SKIP.  Malformed cold fields cannot be hidden behind that result.
for field, value in (("package_count", -1), ("cargo_check_seconds", "bad")):
    malformed_current_only = deepcopy(base)
    malformed_current_only["baseline"] = None
    malformed_current_only["baseline_ref"] = None
    malformed_current_only["baseline_commit_oid"] = None
    malformed_current_only["current"][field] = value
    malformed_current_only["metric_rows"] = [{"metric": "malformed"}]
    result = run(malformed_current_only)
    output = result.stdout + result.stderr
    if (
        result.returncode == 0
        or "gate: FAIL:" not in result.stdout
        or "SKIP" in output
        or "Traceback" in output
        or result.stderr
    ):
        raise SystemExit(
            f"current-only malformed {field} was hidden behind SKIP or traceback: "
            f"{output}"
        )

# A non-object current or baseline payload must fail deterministically.  The
# gate must not dereference it while checking paired rows and print a traceback.
for section in ("current", "baseline"):
    malformed_payload = deepcopy(base)
    malformed_payload[section] = []
    result = run(malformed_payload)
    output = result.stdout + result.stderr
    if (
        result.returncode == 0
        or "gate: FAIL:" not in result.stdout
        or "Traceback" in output
        or result.stderr
    ):
        raise SystemExit(
            f"non-object {section} payload did not fail deterministically: {output}"
        )

# V2 warm fields are explicit in both modes: disabled measurements use null,
# while enabled measurements require a finite non-negative duration.
disabled_warm = deepcopy(base)
disabled_warm["measurement_identity"]["warm_check_enabled"] = False
for section in ("current", "baseline"):
    disabled_warm[section]["warm_check_enabled"] = False
    disabled_warm[section]["cargo_check_warm_seconds"] = None
disabled_warm["metric_rows"] = [
    row
    for row in disabled_warm["metric_rows"]
    if row["metric"] != "cargo_check_warm_seconds"
]
if run(disabled_warm).returncode != 0:
    raise SystemExit("valid V2 warm-disabled false/null payload unexpectedly failed")

false_non_null = deepcopy(disabled_warm)
false_non_null["current"]["cargo_check_warm_seconds"] = 0.0
fail(false_non_null, "warm-disabled false=>null invariant")
false_missing = deepcopy(disabled_warm)
false_missing["current"].pop("cargo_check_warm_seconds")
fail(false_missing, "warm-disabled missing duration invariant")
for label, value in (("null", None), ("negative", -1.0), ("nonfinite", float("nan"))):
    true_invalid = deepcopy(base)
    true_invalid["current"]["cargo_check_warm_seconds"] = value
    fail(true_invalid, f"warm-enabled true=>finite {label} invariant")

# V2 paired cold comparisons must carry the explicit disabled warm bit.  The
# unversioned V1 cold envelope remains the only legacy compatibility shape;
# schema V2 must not silently accept that envelope.
v2_cold_legacy_envelope = deepcopy(base)
v2_cold_legacy_envelope["measurement_identity"].pop("warm_check_enabled")
for section in ("current", "baseline"):
    v2_cold_legacy_envelope[section]["warm_check_enabled"] = False
    v2_cold_legacy_envelope[section]["cargo_check_warm_seconds"] = None
v2_cold_legacy_envelope["metric_rows"] = [
    row
    for row in v2_cold_legacy_envelope["metric_rows"]
    if row["metric"] != "cargo_check_warm_seconds"
]
fail(v2_cold_legacy_envelope, "V2 paired cold identity without disabled warm bit")

# Row arithmetic is schema validation, not only threshold evaluation.  A
# report-only V2 comparison with fabricated delta or percent must fail even
# when no regression threshold was selected.
for fabricated_field in ("delta", "percent"):
    fabricated_no_threshold = deepcopy(base)
    fabricated_no_threshold["metric_rows"][0][fabricated_field] = 1.0
    fail(
        fabricated_no_threshold,
        f"unthresholded fabricated package_count {fabricated_field}",
    )

unknown = deepcopy(base)
unknown["metric_rows"].append({"metric": "unknown_metric", "baseline": 1, "current": 1, "delta": 0, "percent": 0.0})
unknown_result = fail(unknown, "unknown report-only metric")
if "unsupported" not in unknown_result.stdout:
    raise SystemExit(f"unknown metric used the wrong diagnostic: {unknown_result.stdout}")

row_mismatch = deepcopy(base)
row_mismatch["metric_rows"][2]["current"] = 1.0
mismatch_result = fail(row_mismatch, "warm row payload binding")
if "warm" not in mismatch_result.stdout.lower() and "payload" not in mismatch_result.stdout.lower():
    raise SystemExit(f"warm row mismatch lacked a binding diagnostic: {mismatch_result.stdout}")

identity_mismatch = deepcopy(base)
identity_mismatch["baseline"]["warm_check_enabled"] = False
fail(identity_mismatch, "warm identity mismatch")

for label, mutation in (
    ("missing V2 schema", lambda p: p.pop("schema_version")),
    ("unknown V2 schema", lambda p: p.__setitem__("schema_version", 99)),
    ("missing warm identity", lambda p: p["current"].pop("warm_check_enabled")),
    ("missing warm value", lambda p: p["current"].pop("cargo_check_warm_seconds")),
):
    malformed = deepcopy(base)
    mutation(malformed)
    fail(malformed, label)

legacy = deepcopy(base)
legacy.pop("schema_version")
legacy["measurement_identity"].pop("warm_check_enabled")
for section in ("current", "baseline"):
    legacy[section].pop("schema_version")
    legacy[section].pop("warm_check_enabled")
    legacy[section].pop("cargo_check_warm_seconds")
legacy["metric_rows"] = legacy["metric_rows"][:2]
legacy_result = run(legacy)
if legacy_result.returncode != 0:
    raise SystemExit(f"unversioned V1 cold-only payload must remain accepted: {legacy_result.stdout}{legacy_result.stderr}")
legacy_warm_row = deepcopy(legacy)
legacy_warm_row["metric_rows"].append(base["metric_rows"][2])
fail(legacy_warm_row, "warm row against V1 payload")
legacy_warm_threshold = fail(legacy, "warm threshold against V1 payload", "--max-cargo-check-warm-regression-pct", "5")
if "warm" not in (legacy_warm_threshold.stdout + legacy_warm_threshold.stderr).lower():
    raise SystemExit("legacy warm request did not produce a warm-specific failure")

# Warm rows are report-only with a zero baseline and null/n-a percent, but a
# selected warm threshold cannot evaluate that percentage.
zero_report = run(base)
if zero_report.returncode != 0:
    raise SystemExit(f"zero-baseline report-only warm row must not block: {zero_report.stdout}{zero_report.stderr}")
zero_threshold = fail(base, "selected zero-baseline warm threshold", "--max-cargo-check-warm-regression-pct", "5")
if "zero" not in zero_threshold.stdout.lower() and "evaluate" not in zero_threshold.stdout.lower():
    raise SystemExit(f"zero-baseline threshold failure lacked deterministic diagnostic: {zero_threshold.stdout}")

threshold_base = deepcopy(base)
threshold_base["baseline"]["cargo_check_warm_seconds"] = 2.0
threshold_base["current"]["cargo_check_warm_seconds"] = 2.2
threshold_base["metric_rows"][2].update({"baseline": 2.0, "current": 2.2, "delta": 0.2, "percent": 10.0})
within = run(threshold_base, "--max-cargo-check-warm-regression-pct", "10")
if within.returncode != 0:
    raise SystemExit(f"warm threshold within bound unexpectedly failed: {within.stdout}{within.stderr}")
above = fail(threshold_base, "warm threshold above bound", "--max-cargo-check-warm-regression-pct", "5")
if "warm" not in above.stdout.lower() and "cargo check" not in above.stdout.lower():
    raise SystemExit(f"warm threshold miss lacked metric label: {above.stdout}")
fabricated = deepcopy(threshold_base)
fabricated["metric_rows"][2]["percent"] = 1.0
fail(fabricated, "fabricated warm percentage", "--max-cargo-check-warm-regression-pct", "5")
missing_warm_row = deepcopy(base)
missing_warm_row["metric_rows"] = missing_warm_row["metric_rows"][:2]
fail(missing_warm_row, "missing baseline-present warm row")
for invalid in ("NaN", "inf", "-1"):
    invalid_result = fail(base, f"invalid warm threshold {invalid}", "--max-cargo-check-warm-regression-pct", invalid)
    if "finite" not in (invalid_result.stdout + invalid_result.stderr).lower():
        raise SystemExit(f"invalid warm threshold {invalid} lacked finite-value diagnostic")
print("V2 schema/legacy/identity/row/threshold contract: OK")
PY

no_baseline_fixture="$tmp_dir/v2-no-baseline.json"
python3 - "$no_baseline_fixture" "$expected_commit_oid" <<'PY'
import json
from pathlib import Path
import sys

oid = sys.argv[2]
identity = {
    "package": "fake_library",
    "binary": None,
    "check_only": True,
    "no_default_features": False,
    "warm_check_enabled": True,
}
payload = {
    "schema_version": 2,
    "measurement_identity": identity,
    "current_commit_oid": oid,
    "current": {
        **identity,
        "schema_version": 2,
        "commit_oid": oid,
        "package_count": 10,
        "cargo_check_seconds": 1.0,
        "cargo_build_release_seconds": None,
        "cargo_build_release_seconds": None,
        "release_binary_bytes": None,
        "cargo_check_warm_seconds": 0.5,
        "wasmtime_present": False,
        "wasm_executor_present": False,
    },
    "baseline_ref": None,
    "baseline_commit_oid": None,
}
Path(sys.argv[1]).write_text(json.dumps(payload), encoding="utf-8")
PY
no_baseline_result=$(python3 scripts/ci-compile-metrics-gate.py \
  --comparison "$no_baseline_fixture" \
  --max-cargo-check-warm-regression-pct 5)
if [[ "$no_baseline_result" != *"gate: SKIP:"* || "$no_baseline_result" != *"warm"* || "$no_baseline_result" != *"not evaluated"* ]]; then
  echo "no-baseline warm threshold must produce a visible SKIP naming the unevaluated threshold: $no_baseline_result" >&2
  exit 1
fi

python3 - "$no_baseline_fixture" <<'PY'
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys

comparison_path = Path(sys.argv[1])
valid = json.loads(comparison_path.read_text(encoding="utf-8"))


def run(payload):
    comparison_path.write_text(json.dumps(payload), encoding="utf-8")
    return subprocess.run(
        [
            "python3",
            "scripts/ci-compile-metrics-gate.py",
            "--comparison",
            str(comparison_path),
            "--max-cargo-check-warm-regression-pct",
            "5",
        ],
        capture_output=True,
        text=True,
        check=False,
    )


# Paired-row validation is intentionally skipped when no baseline exists, so a
# malformed row alone must not turn an otherwise valid current-only SKIP into a
# false failure.
malformed_rows = deepcopy(valid)
malformed_rows["metric_rows"] = [{"metric": "not-a-valid-current-only-row"}]
row_result = run(malformed_rows)
if (
    row_result.returncode != 0
    or "gate: SKIP:" not in row_result.stdout
    or "Traceback" in row_result.stdout
    or row_result.stderr
):
    raise SystemExit(
        "no-baseline malformed rows must remain outside paired validation: "
        f"{row_result.stdout}{row_result.stderr}"
    )

# Current payload validation must precede that SKIP, even when rows are also
# malformed; a bad cold metric cannot be hidden by the no-baseline path.
malformed_current = deepcopy(valid)
malformed_current["current"]["package_count"] = -1
malformed_current["metric_rows"] = "not-an-array"
current_result = run(malformed_current)
if (
    current_result.returncode == 0
    or "gate: FAIL:" not in current_result.stdout
    or "SKIP" in current_result.stdout
    or "Traceback" in current_result.stdout
    or current_result.stderr
):
    raise SystemExit(
        "no-baseline malformed current payload was not rejected before SKIP: "
        f"{current_result.stdout}{current_result.stderr}"
    )
print("no-baseline validation ordering contract: OK")
PY

workflow_input_result=$(python3 - <<'PY'
from pathlib import Path

source = Path(".github/workflows/compile-metrics.yml").read_text(encoding="utf-8")
required = (
    "measure_warm_check:",
    "type: boolean",
    "default: false",
    "max_cargo_check_warm_regression_pct:",
    'default: ""',
    "--measure-warm-check",
    "--max-cargo-check-warm-regression-pct",
)
missing = [literal for literal in required if literal not in source]
if missing:
    raise SystemExit("workflow warm input/wiring missing: " + ", ".join(missing))

measure_pos = source.index("- name: Measure selected Rust compile metrics")
fetch_pos = source.index("- name: Fetch baseline ref")
validator_markers = (
    "Validate compile metrics inputs",
    "Validate compile-metrics inputs",
    "Validate inputs",
)
validator_candidates = [source.index(marker) for marker in validator_markers if marker in source]
if not validator_candidates or not any(pos < fetch_pos for pos in validator_candidates):
    raise SystemExit("workflow must validate warm/cold inputs before baseline fetch")
validator_pos = max(pos for pos in validator_candidates if pos < fetch_pos)
validator_block = source[validator_pos:fetch_pos]
if "max_cargo_check_warm_regression_pct" not in validator_block or "measure_warm_check" not in validator_block:
    raise SystemExit("workflow validator must cover warm mode and warm threshold")
if "finite" not in validator_block or "non-negative" not in validator_block:
    raise SystemExit("workflow validator must reject non-finite and negative thresholds")
if "baseline_ref" in validator_block and "if:" in validator_block:
    raise SystemExit("workflow input validation must not be baseline-conditional")
if measure_pos < validator_pos:
    raise SystemExit("workflow must validate inputs before measurement")
measure_block = source[measure_pos:source.index("- name: Append platform summary", measure_pos)]
if "inputs.measure_warm_check" not in measure_block:
    raise SystemExit("workflow must wire measure_warm_check into the harness command")
gate_block = source[source.index("- name: Enforce compile metrics gate"):source.index("  summarize:")]
if "inputs.max_cargo_check_warm_regression_pct" not in gate_block:
    raise SystemExit("workflow must pass the selected warm threshold to the gate")
print("workflow warm input validation/wiring contract: OK")
PY
)
printf '%s\n' "$workflow_input_result"

failure_run() {
  local out_path="$1" command_log="$2" count_path="$3" reuse_path="$4" target_path="$5" failure_index="$6"
  shift 6
  if FAKE_CARGO_LOG="$command_log" \
    FAKE_CARGO_CHECK_COUNT_LOG="$count_path" \
    FAKE_CHECK_REUSE_LOG="$reuse_path" \
    FAKE_CHECK_REUSE_MARKER="v2-check-artifact.marker" \
    FAKE_CARGO_TARGET_LOG="$target_path" \
    FAKE_FAIL_CHECK_INDEX="$failure_index" \
    FAKE_HOST_TARGET="$host_target" \
    FAKE_COMMIT_OID="$expected_commit_oid" \
    PATH="$v2_fake_bin:$fake_bin:$PATH" \
      ./scripts/ci-compile-metrics.sh --package fake_library --out-dir "$out_path" "$@"; then
    echo "failure fixture unexpectedly succeeded at check invocation $failure_index" >&2
    exit 1
  fi
}

cold_failure_out="$tmp_dir/v2-cold-failure"
cold_failure_log="$tmp_dir/v2-cold-failure-cargo.log"
cold_failure_count="$tmp_dir/v2-cold-failure-count"
cold_failure_reuse="$tmp_dir/v2-cold-failure-reuse.log"
cold_failure_target="$tmp_dir/v2-cold-failure-target.log"
failure_run "$cold_failure_out" "$cold_failure_log" "$cold_failure_count" "$cold_failure_reuse" "$cold_failure_target" 1 --check-only --measure-warm-check
cold_check_calls=$(grep -c '^check ' "$cold_failure_log" || true)
if [[ "$cold_check_calls" != "1" ]] || grep -q '^build ' "$cold_failure_log"; then
  echo "cold failure must stop before the warm check/release build: $(cat "$cold_failure_log")" >&2
  exit 1
fi
if [[ -e "$cold_failure_out/current.metrics.json" || ! -f "$cold_failure_out/logs/current-cargo-check.log" ]]; then
  echo "cold failure must preserve its log without emitting successful metrics JSON" >&2
  exit 1
fi
if ! grep -Fq "fake V2 check failure at invocation 1" "$cold_failure_out/logs/current-cargo-check.log"; then
  echo "cold failure log must retain the underlying Cargo failure" >&2
  exit 1
fi

warm_failure_out="$tmp_dir/v2-warm-failure"
warm_failure_log="$tmp_dir/v2-warm-failure-cargo.log"
warm_failure_count="$tmp_dir/v2-warm-failure-count"
warm_failure_reuse="$tmp_dir/v2-warm-failure-reuse.log"
warm_failure_target="$tmp_dir/v2-warm-failure-target.log"
failure_run "$warm_failure_out" "$warm_failure_log" "$warm_failure_count" "$warm_failure_reuse" "$warm_failure_target" 2 --binary fake_library --measure-warm-check
warm_check_calls=$(grep -c '^check ' "$warm_failure_log" || true)
if [[ "$warm_check_calls" != "2" ]] || grep -q '^build ' "$warm_failure_log"; then
  echo "warm failure must stop before release build and record both checks: $(cat "$warm_failure_log")" >&2
  exit 1
fi
if [[ -e "$warm_failure_out/current.metrics.json" || ! -f "$warm_failure_out/logs/current-cargo-check.log" || ! -f "$warm_failure_out/logs/current-cargo-check-warm.log" ]]; then
  echo "warm failure must preserve both check logs without emitting successful metrics JSON" >&2
  exit 1
fi
if ! grep -Fq "fake V2 check failure at invocation 2" "$warm_failure_out/logs/current-cargo-check-warm.log"; then
  echo "warm failure log must retain the underlying Cargo failure" >&2
  exit 1
fi

# Source-state mutations are tested only in disposable linked worktrees.  The
# canonical task worktree must retain its pre-existing status byte-for-byte.
canonical_status_before=$(git -C "$repo_root" status --porcelain=v1 -z | sha256sum | awk '{print $1}')
# Keep fixture worktree registrations out of the caller's Git metadata even
# when a failure or cancellation interrupts the explicit removals below.
fixture_git="$tmp_dir/worktree-fixture.git"
git clone --quiet --shared --bare "$repo_root" "$fixture_git"

mutation_fake_bin="$tmp_dir/mutation-bin"
mkdir -p "$mutation_fake_bin"
cp "$v2_fake_bin/cargo" "$mutation_fake_bin/cargo"
chmod +x "$mutation_fake_bin/cargo"
for mutation_mode in content mode type symlink untracked ignored gitlink; do
  mutation_worktree="$tmp_dir/mutation-$mutation_mode"
  git --git-dir="$fixture_git" worktree add --detach "$mutation_worktree" "$expected_commit_oid" >/dev/null
  cp "$repo_root/scripts/ci-compile-metrics.sh" "$mutation_worktree/scripts/ci-compile-metrics.sh"
  if [[ "$mutation_mode" == symlink ]]; then
    ln -s README.md "$mutation_worktree/.v2-tracked-symlink"
    git -C "$mutation_worktree" add .v2-tracked-symlink
  elif [[ "$mutation_mode" == gitlink ]]; then
    mkdir -p "$mutation_worktree/third_party/agent-os"
    printf 'stable nested fixture\n' >"$mutation_worktree/third_party/agent-os/stable.txt"
  fi
  mutation_out="$tmp_dir/mutation-out-$mutation_mode"
  mutation_log="$tmp_dir/mutation-log-$mutation_mode"
  mutation_count="$tmp_dir/mutation-count-$mutation_mode"
  mutation_reuse="$tmp_dir/mutation-reuse-$mutation_mode"
  mutation_target="$tmp_dir/mutation-target-$mutation_mode"
  mutation_error="$tmp_dir/mutation-error-$mutation_mode.log"
  if FAKE_CARGO_LOG="$mutation_log" \
    FAKE_CARGO_CHECK_COUNT_LOG="$mutation_count" \
    FAKE_CHECK_REUSE_LOG="$mutation_reuse" \
    FAKE_CARGO_TARGET_LOG="$mutation_target" \
    FAKE_MUTATION_MODE="$mutation_mode" \
    FAKE_HOST_TARGET="$host_target" \
    PATH="$mutation_fake_bin:$PATH" \
      "$mutation_worktree/scripts/ci-compile-metrics.sh" \
        --package fake_library --out-dir "$mutation_out" --check-only --measure-warm-check \
        2>"$mutation_error"; then
    echo "source mutation $mutation_mode unexpectedly produced a successful measurement" >&2
    git --git-dir="$fixture_git" worktree remove --force "$mutation_worktree" >/dev/null
    exit 1
  fi
  if [[ -e "$mutation_out/current.metrics.json" ]]; then
    echo "source mutation $mutation_mode emitted successful metrics JSON" >&2
    git --git-dir="$fixture_git" worktree remove --force "$mutation_worktree" >/dev/null
    exit 1
  fi
  expected_mutation_path="README.md"
  if [[ "$mutation_mode" == "symlink" ]]; then
    expected_mutation_path=".v2-tracked-symlink"
  elif [[ "$mutation_mode" == "untracked" ]]; then
    expected_mutation_path=".v2-untracked-mutation"
  elif [[ "$mutation_mode" == "ignored" ]]; then
    expected_mutation_path="config.toml"
  elif [[ "$mutation_mode" == "gitlink" ]]; then
    expected_mutation_path="third_party/agent-os/.v2-gitlink-mutation"
  fi
  if ! grep -Fq "$expected_mutation_path" "$mutation_error"; then
    echo "source mutation $mutation_mode diagnostic did not identify $expected_mutation_path: $(cat "$mutation_error")" >&2
    git --git-dir="$fixture_git" worktree remove --force "$mutation_worktree" >/dev/null
    exit 1
  fi
  git --git-dir="$fixture_git" worktree remove --force "$mutation_worktree" >/dev/null
  canonical_status_after=$(git -C "$repo_root" status --porcelain=v1 -z | sha256sum | awk '{print $1}')
  if [[ "$canonical_status_after" != "$canonical_status_before" ]]; then
    echo "source mutation $mutation_mode changed the canonical task worktree" >&2
    exit 1
  fi
done
echo "source/worktree mutation fingerprint contract: OK"

# Output paths that equal, descend from, or resolve through a symlink into a
# tracked source path are invalid.  A sibling external path remains valid.
safety_worktree="$tmp_dir/output-safety-worktree"
git --git-dir="$fixture_git" worktree add --detach "$safety_worktree" "$expected_commit_oid" >/dev/null
cp "$repo_root/scripts/ci-compile-metrics.sh" "$safety_worktree/scripts/ci-compile-metrics.sh"
safety_external_out="$tmp_dir/output-safety-external"
FAKE_CARGO_LOG="$tmp_dir/output-safety-external.log" \
FAKE_CARGO_CHECK_COUNT_LOG="$tmp_dir/output-safety-external-count" \
FAKE_CHECK_REUSE_LOG="$tmp_dir/output-safety-external-reuse" \
FAKE_CARGO_TARGET_LOG="$tmp_dir/output-safety-external-target" \
FAKE_HOST_TARGET="$host_target" \
PATH="$v2_fake_bin:$PATH" \
  "$safety_worktree/scripts/ci-compile-metrics.sh" \
    --package fake_library --out-dir "$safety_external_out" --check-only
for unsafe_out in \
  "$safety_worktree/scripts/v2-output" \
  "$safety_worktree/scripts"; do
  unsafe_name=$(basename "$unsafe_out")
  if FAKE_CARGO_LOG="$tmp_dir/unsafe-$unsafe_name-log" \
    FAKE_CARGO_CHECK_COUNT_LOG="$tmp_dir/unsafe-$unsafe_name-count" \
    FAKE_CHECK_REUSE_LOG="$tmp_dir/unsafe-$unsafe_name-reuse" \
    FAKE_CARGO_TARGET_LOG="$tmp_dir/unsafe-$unsafe_name-target" \
    FAKE_HOST_TARGET="$host_target" \
    PATH="$v2_fake_bin:$PATH" \
      "$safety_worktree/scripts/ci-compile-metrics.sh" \
        --package fake_library --out-dir "$unsafe_out" --check-only; then
    echo "output path inside tracked source unexpectedly succeeded: $unsafe_out" >&2
    git --git-dir="$fixture_git" worktree remove --force "$safety_worktree" >/dev/null
    exit 1
  fi
done
safety_link="$tmp_dir/output-safety-link"
ln -s "$safety_worktree/scripts" "$safety_link"
if FAKE_CARGO_LOG="$tmp_dir/unsafe-link-log" \
  FAKE_CARGO_CHECK_COUNT_LOG="$tmp_dir/unsafe-link-count" \
  FAKE_CHECK_REUSE_LOG="$tmp_dir/unsafe-link-reuse" \
  FAKE_CARGO_TARGET_LOG="$tmp_dir/unsafe-link-target" \
  FAKE_HOST_TARGET="$host_target" \
  PATH="$v2_fake_bin:$PATH" \
    "$safety_worktree/scripts/ci-compile-metrics.sh" \
      --package fake_library --out-dir "$safety_link/v2-output" --check-only; then
  echo "symlinked output path into tracked source unexpectedly succeeded" >&2
  git --git-dir="$fixture_git" worktree remove --force "$safety_worktree" >/dev/null
  exit 1
fi
git --git-dir="$fixture_git" worktree remove --force "$safety_worktree" >/dev/null
echo "output-path alias safety contract: OK"

# Parse the workflow with Ruby's standard-library YAML parser and inspect the
# resulting JSON from Python's standard library.  Keep the contract independent
# of undeclared third-party Python modules on a clean required-gate runner.
# This catches heredoc bodies that are accidentally emitted at column zero:
# textual presence checks alone cannot detect that malformed YAML structure.
workflow_json="$tmp_dir/compile-metrics-workflow.json"
if ! command -v ruby >/dev/null 2>&1; then
  echo "workflow YAML contract requires ruby with its standard-library yaml/json modules" >&2
  exit 1
fi
ruby -ryaml -rjson -e \
  'puts JSON.generate(YAML.safe_load(File.read(ARGV.fetch(0)), aliases: true))' \
  .github/workflows/compile-metrics.yml >"$workflow_json"
python3 - "$workflow_json" <<'PY'
import json
from pathlib import Path
import sys

workflow = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))

if not isinstance(workflow, dict):
    raise SystemExit("workflow YAML must parse as a mapping")
jobs = workflow.get("jobs")
if not isinstance(jobs, dict) or not isinstance(jobs.get("compile-metrics"), dict):
    raise SystemExit("workflow YAML must define the compile-metrics job")
steps = jobs["compile-metrics"].get("steps")
if not isinstance(steps, list):
    raise SystemExit("compile-metrics job must define a YAML steps list")
step_names = [step.get("name") for step in steps if isinstance(step, dict)]
gate_index = step_names.index("Enforce compile metrics gate")
upload_index = step_names.index("Upload compile metrics artifact")
if upload_index <= gate_index:
    raise SystemExit(
        "compile metrics artifact upload must occur after the gate appends its verdict"
    )
validation_steps = [
    step
    for step in steps
    if isinstance(step, dict) and step.get("name") == "Validate compile metrics inputs"
]
if len(validation_steps) != 1 or not isinstance(validation_steps[0].get("run"), str):
    raise SystemExit(
        "compile-metrics input validation must be one parsed YAML run step"
    )
print("workflow YAML parse/validation contract: OK")
PY

# Execute the validator exactly as parsed from the workflow.  Textual wiring
# checks cannot prove that malformed inputs are rejected before any measurement.
python3 - "$workflow_json" <<'PY'
import json
import os
import subprocess
from pathlib import Path
import sys

workflow = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
steps = workflow["jobs"]["compile-metrics"]["steps"]
validator = next(
    step
    for step in steps
    if isinstance(step, dict) and step.get("name") == "Validate compile metrics inputs"
)
validator_run = validator["run"]
threshold_names = (
    "MAX_PACKAGE_COUNT_REGRESSION_PCT",
    "MAX_CARGO_CHECK_REGRESSION_PCT",
    "MAX_CARGO_BUILD_RELEASE_REGRESSION_PCT",
    "MAX_RELEASE_BINARY_BYTES_REGRESSION_PCT",
    "MAX_CARGO_CHECK_WARM_REGRESSION_PCT",
)


def invoke(measure_warm_check, updates=None):
    environment = os.environ.copy()
    environment["MEASURE_WARM_CHECK"] = measure_warm_check
    for name in threshold_names:
        environment[name] = ""
    environment.update(updates or {})
    return subprocess.run(
        ["bash", "-c", validator_run],
        capture_output=True,
        text=True,
        env=environment,
        check=False,
    )


invalid_mode = invoke("maybe")
if invalid_mode.returncode == 0 or "must be a boolean" not in invalid_mode.stderr:
    raise SystemExit(
        f"workflow validator accepted an invalid warm-mode boolean: "
        f"{invalid_mode.stdout}{invalid_mode.stderr}"
    )

disabled_threshold = invoke(
    "false", {"MAX_CARGO_CHECK_WARM_REGRESSION_PCT": "5"}
)
if (
    disabled_threshold.returncode == 0
    or "requires measure_warm_check=true" not in disabled_threshold.stderr
):
    raise SystemExit(
        "workflow validator accepted a warm threshold while disabled: "
        f"{disabled_threshold.stdout}{disabled_threshold.stderr}"
    )

for name in threshold_names:
    for value in ("NaN", "-1", "not-a-number"):
        invalid_threshold = invoke("true", {name: value})
        if (
            invalid_threshold.returncode == 0
            or "finite and non-negative number" not in invalid_threshold.stderr
        ):
            raise SystemExit(
                f"workflow validator accepted {name}={value!r}: "
                f"{invalid_threshold.stdout}{invalid_threshold.stderr}"
            )

valid_cold = invoke("false")
if valid_cold.returncode != 0:
    raise SystemExit(
        f"workflow validator rejected valid cold inputs: "
        f"{valid_cold.stdout}{valid_cold.stderr}"
    )
valid_warm = invoke(
    "true", {"MAX_CARGO_CHECK_WARM_REGRESSION_PCT": "5"}
)
if valid_warm.returncode != 0:
    raise SystemExit(
        f"workflow validator rejected valid warm inputs: "
        f"{valid_warm.stdout}{valid_warm.stderr}"
    )
print("workflow input-validator executable rejection contract: OK")
PY

# Execute the parsed workflow gate step against the generated current-only
# comparison. A warm threshold is forwarded only when both the threshold and
# warm opt-in are active; the gate's explicit no-baseline SKIP makes that
# command wiring observable without a fake gate binary.
python3 - "$workflow_json" "$warm_out_dir" "$tmp_dir" <<'PY'
import json
import os
import subprocess
import sys
from pathlib import Path

workflow = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
output_dir = Path(sys.argv[2]).resolve()
scratch_dir = Path(sys.argv[3]).resolve()
steps = workflow["jobs"]["compile-metrics"]["steps"]
gate_step = next(
    step
    for step in steps
    if isinstance(step, dict) and step.get("name") == "Enforce compile metrics gate"
)
gate_run = gate_step["run"]
gate_run = gate_run.replace(
    "output/compile-metrics/${{ matrix.platform }}", str(output_dir)
)
gate_run = gate_run.replace("${{ inputs.metric_target }}", "oasis7_node_default_features")


def invoke(measure_warm_check, threshold, cold_thresholds=None):
    environment = os.environ.copy()
    summary_path = scratch_dir / f"gate-summary-{measure_warm_check}-{threshold or 'empty'}.md"
    environment.update(
        {
            "MEASURE_WARM_CHECK": measure_warm_check,
            "MEASURE_WARM": measure_warm_check,
            "METRIC_TARGET": "oasis7_node_default_features",
            "MAX_PACKAGE_COUNT_REGRESSION_PCT": "100",
            "MAX_CARGO_CHECK_REGRESSION_PCT": "",
            "MAX_CARGO_BUILD_RELEASE_REGRESSION_PCT": "",
            "MAX_RELEASE_BINARY_BYTES_REGRESSION_PCT": "",
            "MAX_CARGO_CHECK_WARM_REGRESSION_PCT": threshold,
            "GITHUB_STEP_SUMMARY": str(summary_path),
        }
    )
    if cold_thresholds is not None:
        environment.update(cold_thresholds)
    command = gate_run.replace("${{ inputs.measure_warm_check }}", measure_warm_check)
    result = subprocess.run(
        ["bash", "-c", command],
        capture_output=True,
        text=True,
        env=environment,
        check=False,
    )
    result.summary = (
        summary_path.read_text(encoding="utf-8")
        if summary_path.exists()
        else ""
    )
    return result


for measure_warm_check, threshold, expected_forwarding in (
    ("false", "5", False),
    ("true", "5", True),
    ("false", "", False),
    ("true", "", False),
):
    result = invoke(measure_warm_check, threshold)
    output = result.stdout + result.stderr + result.summary
    observed_forwarding = "warm cargo check threshold" in result.summary
    if result.returncode != 0 or observed_forwarding != expected_forwarding:
        raise SystemExit(
            "workflow gate warm-threshold forwarding mismatch for "
            f"measure_warm_check={measure_warm_check!r}, threshold={threshold!r}: "
            f"expected={expected_forwarding}, output={output}"
        )
print("workflow gate warm-threshold conditional wiring contract: OK")

empty_cold_thresholds = {
    "MAX_PACKAGE_COUNT_REGRESSION_PCT": "",
    "MAX_CARGO_CHECK_REGRESSION_PCT": "",
    "MAX_CARGO_BUILD_RELEASE_REGRESSION_PCT": "",
    "MAX_RELEASE_BINARY_BYTES_REGRESSION_PCT": "",
}
empty_result = invoke("false", "", empty_cold_thresholds)
if empty_result.returncode != 0:
    raise SystemExit(
        "workflow gate must omit empty optional cold thresholds: "
        f"{empty_result.stdout}{empty_result.stderr}{empty_result.summary}"
    )
print("workflow gate empty optional threshold omission contract: OK")
PY

# Exercise the production report serializer with deterministic payloads so the
# summary evidence matrix covers generated cold/warm labels, paired rows,
# zero-baseline n/a, and the visible no-baseline threshold SKIP.
python3 - \
  "$warm_out_dir/summary.md" \
  "$baseline_out_dir/summary.md" \
  "$no_baseline_result" \
  "$tmp_dir" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

current_summary_path = Path(sys.argv[1])
paired_summary_path = Path(sys.argv[2])
no_baseline_skip = sys.argv[3]
scratch_dir = Path(sys.argv[4])
current_summary = current_summary_path.read_text(encoding="utf-8")
paired_summary = paired_summary_path.read_text(encoding="utf-8")
if "Current cold `cargo check` seconds:" not in current_summary:
    raise SystemExit("generated summary is missing the cold cargo-check label")
if "Warm/no-op `cargo check` enabled: `true` (" not in current_summary:
    raise SystemExit("generated summary is missing warm enabled state/duration")
if "Compared against baseline ref" not in paired_summary:
    raise SystemExit("paired summary is missing baseline provenance")
if "| `cargo_check_warm_seconds` |" not in paired_summary:
    raise SystemExit("paired summary is missing the warm comparison row")
if "SKIP:" not in no_baseline_skip or "warm cargo check threshold" not in no_baseline_skip:
    raise SystemExit(
        "no-baseline gate output is missing visible warm threshold SKIP: "
        f"{no_baseline_skip}"
    )

source = Path("scripts/ci-compile-metrics.sh").read_text(encoding="utf-8")
marker = (
    'python3 - "$current_metrics_json" "$baseline_metrics_json" '
    '"$comparison_json" "$summary_md" "$baseline_ref" <<\'PY\'\n'
)
start = source.index(marker) + len(marker)
report_body = source[start : source.index("\nPY\n", start)]
oid = "a" * 40
payload = {
    "label": "current",
    "checkout_path": "/tmp/fake-checkout",
    "commit_oid": oid,
    "package": "fake_library",
    "binary": None,
    "package_count": 10,
    "wasmtime_present": False,
    "wasm_executor_present": False,
    "cargo_check_seconds": 1.0,
    "cargo_check_warm_seconds": 0.0,
    "cargo_build_release_seconds": None,
    "release_binary_bytes": None,
    "check_only": True,
    "no_default_features": False,
    "schema_version": 2,
    "warm_check_enabled": True,
}
matrix_dir = scratch_dir / "summary-matrix"
matrix_dir.mkdir()
current_path = matrix_dir / "current.metrics.json"
baseline_path = matrix_dir / "baseline.metrics.json"
comparison_path = matrix_dir / "comparison.json"
summary_path = matrix_dir / "summary.md"
current_path.write_text(json.dumps(payload), encoding="utf-8")
baseline_path.write_text(
    json.dumps({**payload, "label": "baseline"}), encoding="utf-8"
)
result = subprocess.run(
    [
        "python3",
        "-c",
        report_body,
        str(current_path),
        str(baseline_path),
        str(comparison_path),
        str(summary_path),
        oid,
    ],
    capture_output=True,
    text=True,
    check=False,
)
if result.returncode != 0:
    raise SystemExit(f"production report serializer failed: {result.stdout}{result.stderr}")
zero_summary = summary_path.read_text(encoding="utf-8")
if "| `cargo_check_warm_seconds` | `0.000` | `0.000` | `+0.000` | `n/a` |" not in zero_summary:
    raise SystemExit(
        "zero-baseline summary must render the warm percentage as n/a: "
        f"{zero_summary}"
    )
print("summary evidence matrix contract: OK")
PY

# The test fixture owns its FAKE_* behavior.  Production must remain agnostic
# to those fixture-only controls; otherwise the fake can mask real invocation
# ordering or checkout-CWD regressions.
if grep -Eq 'FAKE_[A-Z0-9_]+' scripts/ci-compile-metrics.sh; then
  echo "production compile-metrics script must not reference fixture-only FAKE_* variables" >&2
  exit 1
fi
echo "fixture-owned FAKE_* isolation contract: OK"

echo "ci-compile-metrics-contract.test: OK"
