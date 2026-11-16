#!/usr/bin/env bash
# Cross-platform test contract: large doc scans must avoid Windows argv limits while preserving Linux/macOS results.
set -euo pipefail

# This suite creates its own Git history. Hosted CI's real-repository range
# must not leak into fixture calls; individual range-error cases set their own.
unset OASIS7_PRODUCT_DOC_BASE OASIS7_PRODUCT_DOC_HEAD GITHUB_EVENT_NAME GITHUB_EVENT_PATH GITHUB_ACTIONS

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if REAL_RG="$(command -v rg 2>/dev/null)"; then
  :
else
  REAL_RG="$TMPDIR/rg-grep-backend"
  cat >"$REAL_RG" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
  if [[ "$argument" == *F* ]]; then
    exec grep "$@"
  fi
done
exec grep -E "$@"
SH
  chmod +x "$REAL_RG"
fi

if [[ -n "${OASIS7_TEST_PYTHON:-}" ]]; then
  REAL_PYTHON="$OASIS7_TEST_PYTHON"
else
  REAL_PYTHON="$("$ROOT_DIR/scripts/find-python-with-module.sh" markdown_it)"
fi

FIXTURE="$TMPDIR/repo"
mkdir -p "$FIXTURE/scripts" "$FIXTURE/doc/.governance" "$FIXTURE/doc/testing" "$FIXTURE/doc/many" "$FIXTURE/doc/devlog" "$FIXTURE/.agents/roles/templates" "$FIXTURE/.agents/roles" "$TMPDIR/bin"
if [[ ! -x "$REAL_PYTHON" ]] || ! "$REAL_PYTHON" -c 'import markdown_it; print("ready")' | grep -Fxq ready; then
  echo "doc-governance-check.test: OASIS7_TEST_PYTHON or PATH discovery must provide a functional Python interpreter" >&2
  exit 1
fi
cp "$ROOT_DIR/scripts/doc-governance-check.sh" "$FIXTURE/scripts/doc-governance-check.sh"
cp "$ROOT_DIR/scripts/find-python-with-module.sh" "$FIXTURE/scripts/find-python-with-module.sh"
cp "$ROOT_DIR/scripts/doc-governance-scope.py" "$FIXTURE/scripts/"
cp "$ROOT_DIR/scripts/product_doc_markdown.py" "$FIXTURE/scripts/"
cat >"$FIXTURE/scripts/product-doc-governance-check.py" <<'PY'
#!/usr/bin/env python3
raise SystemExit(0)
PY
cat >"$FIXTURE/scripts/fixture_check.py" <<'PY'
import json
import os
from pathlib import Path
import sys

def check():
    name = Path(sys.argv[0]).stem
    args = sys.argv[1:]
    full = "--full-corpus" in args
    assert ("--warn-only" in args) == full
    if full:
        assert not set(args) & {"--base", "--head", "--worktree"}
    else:
        assert {"--base", "--head", "--worktree"} <= set(args)
    if os.environ.get("CONTENT_CHECK_LOG"):
        with Path(os.environ["CONTENT_CHECK_LOG"]).open("a", encoding="utf-8") as stream:
            stream.write(json.dumps({"name": name, "args": args}) + "\n")
    if os.environ.get("CONTENT_FAIL_CHECKER") == name and os.environ.get("CONTENT_FAIL_SCOPE") == ("full" if full else "scoped"):
        raise SystemExit(2 if full else 1)
PY
cat >"$FIXTURE/scripts/product-doc-content-check.py" <<'PY'
#!/usr/bin/env python3
import os
from pathlib import Path
import sys

from fixture_check import check
check()

args_file = os.environ.get("PRODUCT_DOC_CONTENT_ARGS")
if args_file:
    Path(args_file).write_text("\n".join(sys.argv[1:]) + "\n", encoding="utf-8")
print("product-doc-content: checked 0: reason=fixture")
if "--worktree" in sys.argv and os.environ.get("PRODUCT_DOC_FAIL_CHANGED") == "1":
    raise SystemExit(1)
PY
cat >"$FIXTURE/scripts/doc-core-content-check.py" <<'PY'
#!/usr/bin/env python3
from fixture_check import check
check()
print("doc-core-content: fixture scope flags OK")
PY
cat >"$FIXTURE/scripts/system-design-traceability-check.py" <<'PY'
#!/usr/bin/env python3
from fixture_check import check
check()
print("system-design-traceability: checked 0: reason=fixture")
PY
chmod +x "$FIXTURE/scripts/doc-governance-check.sh"

cat >"$FIXTURE/doc/testing/prd.md" <<'DOC'
## 目标
## 范围
## 接口 / 数据
## 里程碑
## 风险
DOC
for number in $(seq 1 80); do
  printf 'fixture document %s\n' "$number" >"$FIXTURE/doc/many/doc-$number.md"
done
printf '%s\n' 'fixture documentation landing page' >"$FIXTURE/doc/README.md"
printf '%s\n' 'fixture many-doc landing page' >"$FIXTURE/doc/many/README.md"
# Archived prose and optional handoff notes do not create role-format gates.
cat >"$FIXTURE/doc/devlog/README.md" <<'DOC'
# Fixture retired archive

## Historical observation / archived_contributor

Preserve the original attribution without adding a current role card.
DOC
printf '%s\n' '# testing fixture landing page' >"$FIXTURE/doc/testing/README.md"
cat >"$FIXTURE/doc/testing/upstream.md" <<'DOC'
# Fixture upstream

<a id="fixture-req"></a>
## Fixture requirement
DOC
printf '%s\n' '# fixture test source' >"$FIXTURE/doc/testing/fixture.test.py"
cat >"$FIXTURE/doc/testing/design.md" <<'DOC'
# Testing module overview
## Architecture
## Component responsibilities
## Main flow
## Data and state flow
## Topic navigation
DOC

cat >"$FIXTURE/doc/testing/changed.design.md" <<'DOC'
# Fixture system design

### 2.1 需求承接与分配表

| upstream | obligation | local design | owner | excluded |
| --- | --- | --- | --- | --- |
| [fixture requirement](upstream.md#fixture-req) | fixture obligation | [fixture local](#fixture-local) | repository_health_engineer | fixture exclusion |

### Fixture Local

Fixture local design.

### 11.1 验证映射表

| upstream | local design | obligation | validation | evidence | uncovered |
| --- | --- | --- | --- | --- | --- |
| [fixture requirement](upstream.md#fixture-req) | [fixture local](#fixture-local) | fixture obligation | [fixture test](fixture.test.py) | fixture evidence | fixture uncovered |
DOC
cat >"$FIXTURE/doc/README.md" <<'DOC'
fixture documentation landing page
[History](devlog/README.md)
[Many](many/README.md)
[Testing](testing/README.md)
DOC
printf '%s\n' '# repository health role' >"$FIXTURE/.agents/roles/repository_health_engineer.md"
cat >"$FIXTURE/.agents/roles/templates/handoff-brief.md" <<'DOC'
# Optional handoff notes

- From Role: `task_owner`
- To Role: `review_session`
DOC

cat >"$TMPDIR/bin/rg" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >>"${RG_INVOCATION_LOG:?}"
printf '\n' >>"${RG_INVOCATION_LOG:?}"
has_files_from=0
file_args=0
for argument in "$@"; do
  [[ "$argument" == "--files-from" ]] && has_files_from=1
  [[ "$argument" == *.md ]] && file_args=$((file_args + 1))
done
if [[ "$has_files_from" == "0" && "$file_args" -gt 64 ]]; then
  echo "fixture rg: direct file argv is too large" >&2
  exit 126
fi
exec "${REAL_RG:?}" "$@"
SH
chmod +x "$TMPDIR/bin/rg"
cat >"$TMPDIR/bin/python3" <<'SH'
#!/usr/bin/env bash
if [[ "$#" -gt 64 ]]; then
  echo "fixture python3: direct document argv is too large" >&2
  exit 126
fi
exec "${OASIS7_TEST_PYTHON:?}" "$@"
SH
chmod +x "$TMPDIR/bin/python3"

git -C "$FIXTURE" init -q -b main
git -C "$FIXTURE" config user.email test@example.invalid
git -C "$FIXTURE" config user.name Test
mkdir -p "$FIXTURE/doc/testing/nested"
printf '%s\n' '# historical fixture source' >"$FIXTURE/doc/testing/nested/historical-source.md"
git -C "$FIXTURE" add .
git -C "$FIXTURE" commit -qm fixture
fixture_history_commit="$(git -C "$FIXTURE" rev-parse HEAD^{commit})"
rm "$FIXTURE/doc/testing/nested/historical-source.md"
printf 'Pinned repository history: eng-cc/oasis7@%s:doc/testing/nested/historical-source.md#fixture.\n' "$fixture_history_commit" >>"$FIXTURE/doc/README.md"
printf 'Pinned GitHub history: [fixture](https://github.com/eng-cc/oasis7/blob/%s/doc/testing/nested/historical-source.md).\n' "$fixture_history_commit" >>"$FIXTURE/doc/README.md"
cp "$FIXTURE/doc/README.md" "$TMPDIR/doc-readme-pinned-history.md"

set +e
(
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/check.out" 2>"$TMPDIR/check.err"
status=$?
set -e

if [[ "$status" -ne 0 ]]; then
  echo "doc-governance-check.test: fixture check failed" >&2
  cat "$TMPDIR/check.out" >&2
  cat "$TMPDIR/check.err" >&2
  exit 1
fi
if grep -Fq 'direct file argv is too large' "$TMPDIR/check.err"; then
  echo "doc-governance-check.test: document scan passed a large direct file argv to rg" >&2
  exit 1
fi
if [[ "$(wc -l < "$TMPDIR/rg.log")" -lt 2 ]]; then
  echo "doc-governance-check.test: document scan did not split its rg invocation into bounded batches" >&2
  exit 1
fi
if ! grep -Fqx 'doc-governance-check: OK' "$TMPDIR/check.out"; then
  echo "doc-governance-check.test: fixture output missing successful governance verdict" >&2
  cat "$TMPDIR/check.out" >&2
  cat "$TMPDIR/check.err" >&2
  exit 1
fi

# Use the real baseline caller; unrelated gates are isolated fixture no-ops.
for gate in unified-world-code-terminology-scan.test.sh lint-skills.sh check-windows-paths.sh check-script-executable-bits.sh; do
  printf '#!/usr/bin/env bash\nexit 0\n' >"$FIXTURE/scripts/$gate"
  chmod +x "$FIXTURE/scripts/$gate"
done
printf 'raise SystemExit(0)\n' >"$FIXTURE/scripts/validate-codex-agent-config.py"
# Remove the inventoried input immediately before real awk opens it. This
# exercises an actual read error without platform-dependent chmod semantics.
REAL_AWK="$(command -v awk)"
cat >"$TMPDIR/bin/awk" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${LENGTH_INPUT_FAILURE:-0}" == 1 && -f doc/many/doc-1.md ]]; then
  mv doc/many/doc-1.md doc/many/doc-1.unreadable
fi
exec "${REAL_AWK:?}" "$@"
SH
chmod +x "$TMPDIR/bin/awk"

# Length is editorial guidance, including through the full-corpus caller.
# The same boundary documents must still fail for real reference/navigation/read
# errors. The content stub tests error propagation, not content parsing.
cp "$FIXTURE/doc/many/doc-1.md" "$TMPDIR/length-original.md"
cp "$FIXTURE/doc/README.md" "$TMPDIR/length-navigation.md"
for length in 999 1000 1001; do
  for mode in default full baseline; do
    command=(./scripts/doc-governance-check.sh)
    case "$mode" in
      full) command+=(--full-corpus) ;;
      baseline) command=(bash "$ROOT_DIR/scripts/ci-tests.sh" required --group baseline --repo-root "$FIXTURE") ;;
    esac
    for defect in none reference structure content input; do
      "$REAL_PYTHON" - "$FIXTURE/doc/many/doc-1.md" "$length" "$defect" <<'PY_LENGTH'
from pathlib import Path
import sys

path, length, defect = Path(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
lines = ["# Length boundary fixture"] + ["Readable fixture paragraph." for _ in range(length - 1)]
if defect == "reference":
    lines[-1] = "Missing reference: doc/testing/nested/length-missing.md"
path.write_text("\n".join(lines) + "\n", encoding="utf-8")
assert len(path.read_text(encoding="utf-8").splitlines()) == length
PY_LENGTH
      cp "$TMPDIR/length-navigation.md" "$FIXTURE/doc/README.md"
      content_failure=0
      input_failure=0
      case "$defect" in
        none) expected='doc-governance-check: OK' ;;
        reference) expected='references missing markdown path: doc/testing/nested/length-missing.md' ;;
        structure)
          sed -i.bak '/\[Many\]/d' "$FIXTURE/doc/README.md"
          rm -f "$FIXTURE/doc/README.md.bak"
          expected='top-level directory navigation failed' ;;
        content) content_failure=1; expected='product document content contract failed' ;;
        input)
          input_failure=1
          expected='markdown line count scan failed' ;;
      esac
      status=0
      (
        cd "$FIXTURE"
        OASIS7_PRODUCT_DOC_BASE="$fixture_history_commit" OASIS7_PRODUCT_DOC_HEAD="$fixture_history_commit" OASIS7_TEST_PYTHON="$REAL_PYTHON" PRODUCT_DOC_FAIL_CHANGED="$content_failure" LENGTH_INPUT_FAILURE="$input_failure" REAL_AWK="$REAL_AWK" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" "${command[@]}"
      ) >"$TMPDIR/length-$length-$mode-$defect.out" 2>&1 || status=$?
      if [[ "$defect" == input ]]; then
        mv "$FIXTURE/doc/many/doc-1.unreadable" "$FIXTURE/doc/many/doc-1.md"
      fi
      output="$TMPDIR/length-$length-$mode-$defect.out"
      if { [[ "$defect" == none ]] && [[ "$status" != 0 ]]; } || { [[ "$defect" != none ]] && [[ "$status" == 0 ]]; }; then
        echo "doc-governance-check.test: length=$length mode=$mode defect=$defect unexpected status=$status" >&2
        cat "$output" >&2
        exit 1
      fi
      grep -Fq "$expected" "$output"
      if [[ "$length" == 1001 && "$defect" != input ]]; then
        grep -Fq 'advisory: doc/many/doc-1.md exceeds 1000 lines (1001)' "$output"
      elif grep -Fq 'advisory: doc/many/doc-1.md' "$output"; then
        echo "doc-governance-check.test: unexpected advisory at $length lines" >&2
        exit 1
      fi
    done
  done
done
rm "$TMPDIR/bin/awk"
cp "$TMPDIR/length-original.md" "$FIXTURE/doc/many/doc-1.md"
cp "$TMPDIR/length-navigation.md" "$FIXTURE/doc/README.md"
(
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh --help
) >"$TMPDIR/length-help.out"
grep -Fq 'over 1000 lines receive an advisory only' "$TMPDIR/length-help.out"

printf 'Moving branch history: eng-cc/oasis7@main:doc/testing/nested/historical-source.md.\n' >>"$FIXTURE/doc/README.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/moving-history.out" 2>"$TMPDIR/moving-history.err"; then
  echo "doc-governance-check.test: moving-branch history reference unexpectedly passed" >&2
  exit 1
fi
grep -Fq 'references missing markdown path: doc/testing/nested/historical-source.md' "$TMPDIR/moving-history.err"
cp "$TMPDIR/doc-readme-pinned-history.md" "$FIXTURE/doc/README.md"

printf 'Missing active documentation: doc/testing/nested/missing-active.md\n' >>"$FIXTURE/doc/README.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/missing-active-path.out" 2>"$TMPDIR/missing-active-path.err"; then
  echo "doc-governance-check.test: genuine missing active markdown path unexpectedly passed" >&2
  exit 1
fi
grep -Fq 'references missing markdown path: doc/testing/nested/missing-active.md' "$TMPDIR/missing-active-path.err"
cp "$TMPDIR/doc-readme-pinned-history.md" "$FIXTURE/doc/README.md"

if ! (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" CONTENT_CHECK_LOG="$TMPDIR/content-check.log" PRODUCT_DOC_CONTENT_ARGS="$TMPDIR/product-doc-content.args" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh --full-corpus
) >"$TMPDIR/full-corpus.out" 2>"$TMPDIR/full-corpus.err"; then
  echo "doc-governance-check.test: --full-corpus caller integration unexpectedly failed" >&2
  cat "$TMPDIR/full-corpus.out" >&2
  cat "$TMPDIR/full-corpus.err" >&2
  exit 1
fi
grep -Fxq -- '--full-corpus' "$TMPDIR/product-doc-content.args"

# Every strict scoped check runs before the separate historical warning pass.
"$REAL_PYTHON" - "$TMPDIR/content-check.log" "$FIXTURE" <<'PY'
import json
from pathlib import Path
import subprocess
import sys

calls = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
root = sys.argv[2]
head = subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip()
names = ["product-doc-content-check", "doc-core-content-check", "system-design-traceability-check"]
assert [call["name"] for call in calls] == names + names, calls
for call in calls[:3]:
    assert call["args"] == ["--repo-root", root, "--base", head, "--head", head, "--worktree"], call
for call in calls[3:]:
    assert call["args"] == ["--warn-only", "--repo-root", root, "--full-corpus"], call
PY

# Both scoped failures and full-corpus input/execution errors remain hard failures.
for checker in product-doc-content-check doc-core-content-check system-design-traceability-check; do
  for scope in scoped full; do
    if (
      cd "$FIXTURE"
      OASIS7_TEST_PYTHON="$REAL_PYTHON" CONTENT_FAIL_CHECKER="$checker" CONTENT_FAIL_SCOPE="$scope" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh --full-corpus
    ) >"$TMPDIR/$checker-$scope.out" 2>"$TMPDIR/$checker-$scope.err"; then
      echo "doc-governance-check.test: --full-corpus masked $checker $scope failure" >&2
      exit 1
    fi
    grep -Fq 'doc-governance-check: failed with' "$TMPDIR/$checker-$scope.out"
  done
done

# Full mode cannot make a partial explicit range silently fall back to HEAD.
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" OASIS7_PRODUCT_DOC_BASE=HEAD OASIS7_PRODUCT_DOC_HEAD= RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh --full-corpus
) >"$TMPDIR/partial-range.out" 2>"$TMPDIR/partial-range.err"; then
  echo "doc-governance-check.test: --full-corpus masked partial explicit range" >&2
  exit 1
fi
grep -Fq 'explicit base/head must be supplied together' "$TMPDIR/partial-range.err"

if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" PRODUCT_DOC_FAIL_CHANGED=1 RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh --full-corpus
) >"$TMPDIR/full-changed-range-failure.out" 2>"$TMPDIR/full-changed-range-failure.err"; then
  echo "doc-governance-check.test: full-corpus mode masked a changed-range product failure" >&2
  exit 1
fi

# Historical professional-design warnings must not mask the strict changed-scope
# gate: retain the trusted range and reject an actual broken validation source.
fixture_base="$(git -C "$FIXTURE" rev-parse HEAD^{commit})"
fixture_head="$fixture_base"
cp "$ROOT_DIR/scripts/doc_content_warnings.py" "$FIXTURE/scripts/doc_content_warnings.py"
cp "$ROOT_DIR/scripts/product_doc_markdown.py" "$FIXTURE/scripts/product_doc_markdown.py"
cp "$ROOT_DIR/scripts/system-design-traceability-check.py" "$FIXTURE/scripts/system-design-traceability-check.real.py"
cat >"$FIXTURE/scripts/system-design-traceability-check.py" <<'PY'
#!/usr/bin/env python3
import os
from pathlib import Path
import runpy
import sys

if "--worktree" in sys.argv and os.environ.get("SYSTEM_DESIGN_TRACEABILITY_ARGS"):
    Path(os.environ["SYSTEM_DESIGN_TRACEABILITY_ARGS"]).write_text(
        "\n".join(sys.argv[1:]) + "\n", encoding="utf-8"
    )
runpy.run_path(str(Path(__file__).with_name("system-design-traceability-check.real.py")), run_name="__main__")
PY
chmod +x "$FIXTURE/scripts/system-design-traceability-check.py"
sed -i 's/(fixture.test.py)/(missing.test.py)/' "$FIXTURE/doc/testing/changed.design.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" OASIS7_PRODUCT_DOC_BASE="$fixture_base" OASIS7_PRODUCT_DOC_HEAD="$fixture_head" SYSTEM_DESIGN_TRACEABILITY_ARGS="$TMPDIR/system-design.args" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh --full-corpus
) >"$TMPDIR/full-corpus-system-design.out" 2>"$TMPDIR/full-corpus-system-design.err"; then
  echo "doc-governance-check.test: --full-corpus allowed a broken changed validation source" >&2
  exit 1
fi
grep -Fq 'trace-validation-missing' "$TMPDIR/full-corpus-system-design.out"
grep -Fq 'system-design-traceability: full-corpus inventoried 2 professional designs; topic-contracts=1 module-overviews=1 module-contracts=0;' "$TMPDIR/full-corpus-system-design.out"
grep -Fq '::warning::system-design-traceability: trace-validation-missing' "$TMPDIR/full-corpus-system-design.out"
grep -Fq 'module-overview: doc/testing/design.md: inventoried' "$TMPDIR/full-corpus-system-design.out"
grep -Fqx -- '--base' "$TMPDIR/system-design.args"
grep -Fqx -- "$fixture_base" "$TMPDIR/system-design.args"
grep -Fqx -- '--head' "$TMPDIR/system-design.args"
grep -Fqx -- "$fixture_head" "$TMPDIR/system-design.args"
grep -Fqx -- '--worktree' "$TMPDIR/system-design.args"

sed -i 's/(missing.test.py)/(fixture.test.py)/' "$FIXTURE/doc/testing/changed.design.md"

# Directory navigation consumes real links, not a second directory inventory.
cp "$FIXTURE/doc/README.md" "$TMPDIR/navigation-base.md"
for defect in prefix code_link wrong_entry missing_readme new_directory; do
  cp "$TMPDIR/navigation-base.md" "$FIXTURE/doc/README.md"
  case "$defect" in
    prefix) sed -i.bak 's#\[Many\](many/README.md)#doc/many/#' "$FIXTURE/doc/README.md" ;;
    code_link) sed -i.bak 's#\[Many\](many/README.md)#`[Many](many/README.md)`#' "$FIXTURE/doc/README.md" ;;
    wrong_entry) sed -i.bak 's#\[Many\](many/README.md)#[Many](many/doc-1.md)#' "$FIXTURE/doc/README.md" ;;
    missing_readme) mv "$FIXTURE/doc/many/README.md" "$FIXTURE/doc/many/entry.saved" ;;
    new_directory) mkdir -p "$FIXTURE/doc/new-topic"; printf '# New topic\n' >"$FIXTURE/doc/new-topic/README.md" ;;
  esac
  rm -f "$FIXTURE/doc/README.md.bak"
  if (
    cd "$FIXTURE"
    OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
  ) >"$TMPDIR/navigation-$defect.out" 2>&1; then
    echo "doc-governance-check.test: invalid navigation passed: $defect" >&2
    exit 1
  fi
  grep -Fq 'top-level directory navigation failed' "$TMPDIR/navigation-$defect.out"
  if [[ "$defect" == missing_readme ]]; then mv "$FIXTURE/doc/many/entry.saved" "$FIXTURE/doc/many/README.md"; fi
  if [[ "$defect" == new_directory ]]; then
    printf '\n[New topic](new-topic/README.md)\n' >>"$FIXTURE/doc/README.md"
    (cd "$FIXTURE"; OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh) >"$TMPDIR/new-directory-valid.out" 2>&1
    grep -Fxq 'doc-governance-check: OK' "$TMPDIR/new-directory-valid.out"
    rm -rf "$FIXTURE/doc/new-topic"
  fi
done
cp "$TMPDIR/navigation-base.md" "$FIXTURE/doc/README.md"

mkdir -p "$FIXTURE/doc/testing/nested"
printf '%s\n' '# forbidden ledger' >"$FIXTURE/doc/testing/nested/reintroduced.project.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/ledger.out" 2>"$TMPDIR/ledger.err"; then
  echo "doc-governance-check.test: reintroduced project ledger unexpectedly passed" >&2
  exit 1
fi
grep -Fq 'project.md / *.project.md ledgers are retired' "$TMPDIR/ledger.out"
rm -f "$FIXTURE/doc/testing/nested/reintroduced.project.md"

# Directory, file and dangling symlinks are independent invalid inputs.
mkdir -p "$FIXTURE/linked-target"
printf '%s\n' 'external fixture landing page' >"$FIXTURE/linked-target/README.md"
printf '%s\n' 'not a directory' >"$FIXTURE/linked-file-target"
for kind in directory file dangling; do
  case "$kind" in
    directory) target="$FIXTURE/linked-target" ;;
    file) target="$FIXTURE/linked-file-target" ;;
    dangling) target="$FIXTURE/missing-linked-target" ;;
  esac
  ln -s "$target" "$FIXTURE/doc/linked-$kind"
  if (
    cd "$FIXTURE"
    OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
  ) >"$TMPDIR/symlink-$kind.out" 2>&1; then
    echo "doc-governance-check.test: $kind documentation symlink unexpectedly passed" >&2
    exit 1
  fi
  grep -Fq "unsupported documentation symlink: doc/linked-$kind" "$TMPDIR/symlink-$kind.out"
  rm "$FIXTURE/doc/linked-$kind"
done

printf '%s\n' 'Active task status: doc/testing/nested/reintroduced.project.md' >"$FIXTURE/doc/testing/nested/active-reference.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/reference.out" 2>"$TMPDIR/reference.err"; then
  echo "doc-governance-check.test: active project-ledger reference unexpectedly passed" >&2
  exit 1
fi
grep -Fq 'references missing markdown path: doc/testing/nested/reintroduced.project.md' "$TMPDIR/reference.err"
printf '%s\n' 'Current authority: same-name project document.' >"$FIXTURE/doc/testing/nested/active-reference.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/semantic-reference.out" 2>"$TMPDIR/semantic-reference.err"; then
  :
else
  echo "doc-governance-check.test: prose advice unexpectedly blocked: semantic project-ledger reference" >&2
  exit 1
fi
grep -Fq 'advisory: review legacy project-ledger wording' "$TMPDIR/semantic-reference.out"
printf '%s\n' 'Current authority: same-named design and project.' >"$FIXTURE/doc/testing/nested/active-reference.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/stale-wording.out" 2>"$TMPDIR/stale-wording.err"; then
  :
else
  echo "doc-governance-check.test: prose advice unexpectedly blocked: stale project-ledger wording" >&2
  exit 1
fi
grep -Fq 'advisory: review legacy project-ledger wording' "$TMPDIR/stale-wording.out"
printf '%s\n' '- Corresponding GitHub Issue/Project task truth: `doc/testing/prd.md`' >"$FIXTURE/doc/testing/nested/active-reference.md"
if (
  cd "$FIXTURE"
  OASIS7_TEST_PYTHON="$REAL_PYTHON" RG_INVOCATION_LOG="$TMPDIR/rg.log" REAL_RG="$REAL_RG" PATH="$TMPDIR/bin:$PATH" ./scripts/doc-governance-check.sh
) >"$TMPDIR/false-task-truth-link.out" 2>"$TMPDIR/false-task-truth-link.err"; then
  :
else
  echo "doc-governance-check.test: prose advice unexpectedly blocked: local markdown falsely labelled as GitHub task truth" >&2
  exit 1
fi
grep -Fq 'advisory: review legacy project-ledger wording' "$TMPDIR/false-task-truth-link.out"

# Exercise the complete suite under foreign, well-formed CI OIDs as well.
# The child must still forward its own fixture HEAD, as asserted above. The
# guard limits this regression to one child, including when CI is already set.
if [[ "${OASIS7_DOC_GOVERNANCE_TEST_ENV_CHILD:-}" != 1 ]]; then
  if ! OASIS7_DOC_GOVERNANCE_TEST_ENV_CHILD=1 \
    OASIS7_PRODUCT_DOC_BASE=1111111111111111111111111111111111111111 \
    OASIS7_PRODUCT_DOC_HEAD=2222222222222222222222222222222222222222 \
    bash "$SCRIPT_DIR/doc-governance-check.test.sh" >"$TMPDIR/foreign-ci-range.out" 2>&1; then
    echo "doc-governance-check.test: inherited CI range isolation failed" >&2
    cat "$TMPDIR/foreign-ci-range.out" >&2
    exit 1
  fi
  grep -Fxq 'doc-governance-check.test: OK' "$TMPDIR/foreign-ci-range.out"
fi

echo "doc-governance-check.test: OK"
