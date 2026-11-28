#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
checker="$repo_root/scripts/check-launcher-p2p-dependency-surface.sh"
ci_tests="$repo_root/scripts/ci-tests.sh"

required_gate_line='  run bash ./scripts/check-launcher-p2p-dependency-surface.test.sh'
function_body() {
  local name="$1"
  awk -v start="$name() {" '
    $0 == start { capture=1; next }
    capture && $0 == "}" { exit }
    capture { print }
  ' "$ci_tests"
}
require_exact_route() {
  local label="$1" body="$2" route="$3" expected_count="$4"
  local actual_count
  actual_count=$(grep -F -c -- "$route" <<<"$body" || true)
  if [[ "$actual_count" -ne "$expected_count" ]]; then
    echo "$label must contain $expected_count route(s) for: $route (found $actual_count)" >&2
    exit 1
  fi
}

baseline_cargo_tooling=$(function_body run_cargo_tooling_baseline_contract_tests)
cargo_tooling=$(function_body run_cargo_tooling_contract_tests)
full_capabilities=$(function_body run_all_required_gate_capability_contract_tests)
group_dispatch=$(function_body run_group)

require_exact_route "Cargo-tooling baseline" "$baseline_cargo_tooling" "$required_gate_line" 1
require_exact_route "Cargo-tooling group" "$cargo_tooling" "run_cargo_tooling_baseline_contract_tests" 1
require_exact_route "Cargo-tooling group" "$cargo_tooling" "$required_gate_line" 0
require_exact_route "Required group dispatch" "$group_dispatch" 'cargo_tooling_contracts) run_cargo_tooling_contract_tests ;;' 1
require_exact_route "Full required suite" "$full_capabilities" "run_cargo_tooling_contract_tests" 1

python3 - "$repo_root/.github/workflows/rust.yml" "$repo_root/scripts/ci-required-scope.json" <<'PYTHON'
import json
from pathlib import Path
import re
import sys

workflow = Path(sys.argv[1]).read_text()
config = json.loads(Path(sys.argv[2]).read_text())
def validate_execution(workflow):
    jobs = dict(re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)',
                           workflow.split('\njobs:\n', 1)[1], re.M | re.S))
    job = jobs.get('cargo-tooling-contracts', '')
    for expected in ('needs: select', "needs.select.result == 'success'",
                     "needs.select.outputs.run_cargo_tooling_contracts == 'true'",
                     "fromJSON(needs.select.outputs.plan).scope == 'full'",
                     'bash ./scripts/ci-tests.sh required --group "cargo_tooling_contracts" --repo-root "$GITHUB_WORKSPACE"'):
        if expected not in job:
            raise ValueError("Cargo tooling job is missing selected group execution: " + expected)
    if 'strategy:' in job or 'ci-authority' in job or 'matrix.group' in job:
        raise ValueError("Cargo tooling job must execute the candidate group without a singleton matrix")
    needs = re.search(r'    needs: \[(.*)\]', jobs.get('required-gate', ''))
    if not needs or 'cargo-tooling-contracts' not in needs.group(1).split(', '):
        raise ValueError("final required-gate omits Cargo tooling results")

if '\n  cargo-tooling-contracts:\n' in workflow:
    try:
        validate_execution(workflow)
    except ValueError as exc:
        raise SystemExit(str(exc))

    # Isolated mutations must fail: route, selection and failure aggregation all matter.
    for changed in (
            workflow.replace("needs.select.result == 'success'", 'true'),
            workflow.replace("fromJSON(needs.select.outputs.plan).scope == 'full'", 'false'),
            workflow.replace('bash ./scripts/ci-tests.sh required --group "cargo_tooling_contracts"',
                             'bash "$RUNNER_TEMP/ci-authority/ci-tests.sh" required --group "cargo_tooling_contracts"'),
            workflow.replace(', cargo-tooling-contracts,', ', ')):
        try:
            validate_execution(changed)
        except ValueError:
            continue
        raise SystemExit("Cargo tooling contract accepted a broken execution fixture")
if 'rust_toolchain' not in config['resources']['cargo_tooling_contracts']:
    raise SystemExit("Cargo dependency checks require a Rust toolchain")
PYTHON

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fake_bin="$tmp_dir/bin"
mkdir -p "$fake_bin"

cat >"$fake_bin/cargo" <<'FAKE_CARGO'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >>"${FAKE_CARGO_LOG:?}"

if [[ "${1:-}" != "tree" ]]; then
  echo "unexpected fake cargo command: $*" >&2
  exit 1
fi

args=("$@")
spec=""
for ((index = 0; index < ${#args[@]}; index++)); do
  if [[ "${args[index]}" == "-i" && $((index + 1)) -lt ${#args[@]} ]]; then
    spec="${args[index + 1]}"
  fi
done

case "$spec" in
  libp2p|ring@0.16.20|rustls-webpki@0.101.7|hickory-proto@0.24.4)
    printf 'error: package ID specification %s did not match any packages\n' "$spec" >&2
    exit 1
    ;;
  *)
    echo "unexpected forbidden-spec query: $spec" >&2
    exit 1
    ;;
esac
FAKE_CARGO
chmod +x "$fake_bin/cargo"

cat >"$fake_bin/rg" <<'FAKE_RG'
#!/usr/bin/env bash
echo "fake rg unavailable" >&2
exit 127
FAKE_RG
chmod +x "$fake_bin/rg"

fake_cargo_log="$tmp_dir/cargo-invocations.log"
run_output="$tmp_dir/checker-output.log"
if ! PATH="$fake_bin:$PATH" \
  FAKE_CARGO_LOG="$fake_cargo_log" \
  bash "$checker" >"$run_output" 2>&1; then
  if grep -Fq -- "fake rg unavailable" "$run_output"; then
    echo "dependency-surface checker depends on unavailable ripgrep" >&2
  fi
  echo "dependency-surface checker did not preserve its four exclusion checks" >&2
  sed -n '1,120p' "$run_output" >&2
  exit 1
fi

expected_specs=(
  "libp2p"
  "ring@0.16.20"
  "rustls-webpki@0.101.7"
  "hickory-proto@0.24.4"
)

invocation_count=$(awk 'END { print NR + 0 }' "$fake_cargo_log")
if [[ "$invocation_count" -ne "${#expected_specs[@]}" ]]; then
  echo "expected one cargo tree invocation per forbidden spec, got $invocation_count" >&2
  sed -n '1,120p' "$fake_cargo_log" >&2
  exit 1
fi

for spec in "${expected_specs[@]}"; do
  if [[ "$(grep -F -c -- "-i $spec" "$fake_cargo_log" || true)" -ne 1 ]]; then
    echo "expected exactly one cargo tree query for forbidden spec: $spec" >&2
    sed -n '1,120p' "$fake_cargo_log" >&2
    exit 1
  fi

  expected_ok="ok: oasis7_launcher_backend dependency closure excludes $spec"
  if [[ "$(grep -F -c -- "$expected_ok" "$run_output" || true)" -ne 1 ]]; then
    echo "missing preserved exclusion result for forbidden spec: $spec" >&2
    sed -n '1,120p' "$run_output" >&2
    exit 1
  fi
done

while IFS= read -r invocation; do
  if [[ "$invocation" != tree* ]]; then
    echo "fake Cargo recorded a non-tree invocation: $invocation" >&2
    exit 1
  fi
  for flag in --locked --offline; do
    case " $invocation " in
      *" $flag "*) ;;
      *)
        echo "missing required Cargo reproducibility flag $flag in invocation: $invocation" >&2
        exit 1
        ;;
    esac
  done
done <"$fake_cargo_log"

echo "ok: all launcher dependency-surface cargo tree invocations are locked and offline"
