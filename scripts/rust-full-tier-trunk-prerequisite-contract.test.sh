#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOW="$ROOT_DIR/.github/workflows/rust.yml"

python3 - "$WORKFLOW" <<'PY'
from pathlib import Path
import re
import sys
import tempfile

workflow_path = Path(sys.argv[1])
job_header = re.compile(r"^  (?P<name>[A-Za-z0-9_-]+):\s*$")
run_key = re.compile(r"^(?P<indent>\s*)(?:-\s+)?run:\s*(?P<body>.*)$")
target = "CI_VERBOSE=1 ./scripts/ci-tests.sh full"
mixed_target = 'python3 scripts/ci-trusted-full-regression.py full "$SOURCE_SHA"'
launcher = (workflow_path.parents[2] / 'scripts/ci-trusted-full-regression.py').read_text()
trunk_install = 'bash scripts/install-ci-trunk.sh "$RUNNER_TEMP/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz" "$RUNNER_TEMP/ci-tools/bin"'
trunk_verify = 'trunk --version | grep -Fqx "trunk ${TRUNK_VERSION}"'
helper = (workflow_path.parents[2] / 'scripts/install-ci-trunk.sh').read_text()
if ('sha256sum -c' not in helper
        or '[[ "$("$stage/trunk" --version)" == "trunk $version" ]]' not in helper
        or 'version=0.21.14' not in helper):
    raise SystemExit('rust.yml contract: shared Trunk installer must verify archive and exact version')
expected_jobs = {"full-regression"}


def repository_trunk_version():
    release_workflow_path = workflow_path.with_name("release-packages.yml")
    release_lines = release_workflow_path.read_text(encoding="utf-8").splitlines()
    versions = [
        match.group("version")
        for line in release_lines
        if (match := re.match(r"^  TRUNK_VERSION: (?P<version>[0-9]+\.[0-9]+\.[0-9]+)$", line))
    ]
    if len(versions) != 1:
        raise SystemExit(
            "rust.yml contract: release workflow must define exactly one pinned TRUNK_VERSION"
        )
    return versions[0]


expected_trunk_version = repository_trunk_version()

def run_commands(job_lines):
    """Return (line number, shell text) for each YAML run key and body."""
    commands = []
    position = 0
    while position < len(job_lines):
        line_number, line = job_lines[position]
        match = run_key.match(line)
        if not match:
            position += 1
            continue

        body = match.group("body").strip()
        commands.append((line_number, body))
        if body in {"|", ">", "|-", "|+", ">-", ">+"}:
            run_indent = len(match.group("indent"))
            position += 1
            while position < len(job_lines):
                body_line_number, body_line = job_lines[position]
                if body_line.strip() and len(body_line) - len(body_line.lstrip()) <= run_indent:
                    break
                commands.append((body_line_number, body_line.strip()))
                position += 1
            continue
        position += 1
    return commands

def validate_workflow(path, required_jobs):
    lines = path.read_text(encoding="utf-8").splitlines()
    try:
        jobs_start = next(index for index, line in enumerate(lines) if line == "jobs:")
    except StopIteration:
        raise SystemExit(f"rust.yml contract: {path} is missing jobs: mapping")

    # Keep the parser intentionally small, but support both YAML step forms:
    # `run: ...` under a named step and the valid inline `- run: ...` form.
    local_jobs = {}
    current_name = None
    for index in range(jobs_start + 1, len(lines)):
        match = job_header.match(lines[index])
        if match:
            current_name = match.group("name")
            local_jobs[current_name] = []
            continue
        if current_name is not None:
            local_jobs[current_name].append((index, lines[index]))

    top_level_versions = [
        match.group("version")
        for line in lines
        if (match := re.match(r"^  TRUNK_VERSION: (?P<version>[0-9]+\.[0-9]+\.[0-9]+)$", line))
    ]
    top_level_version = top_level_versions[0] if len(top_level_versions) == 1 else None
    failures = []
    full_jobs = []
    for job_name, job_lines in local_jobs.items():
        commands = [
            (line_number, command.strip())
            for line_number, command in run_commands(job_lines)
            if command.strip() and not command.strip().startswith("#")
        ]
        invocation_lines = [
            (line_number, command)
            for line_number, command in commands
            if ((target in command and "--command" not in command)
                or (job_name == "full-regression" and command == mixed_target))
        ]
        if not invocation_lines:
            continue

        full_jobs.append(job_name)
        invocation_line = invocation_lines[0][0]
        if any(command == mixed_target for _, command in invocation_lines):
            expected_bind = "bind(argv, canonical(TEMP / 'ci-tools/bin/trunk'), readonly=True)"
            expected_verify = f'test "$(trunk --version)" = "trunk {expected_trunk_version}"'
            expected_run = "subprocess.run(['./scripts/ci-tests.sh', 'full-extended']"
            bind_at = launcher.find(expected_bind)
            verify_at = launcher.find(expected_verify)
            run_at = launcher.find(expected_run)
            if len(invocation_lines) != 1 or not 0 <= bind_at < verify_at < run_at:
                failures.append(f"{job_name}: mixed caller lacks the read-only, version-checked Trunk path")
        if top_level_version != expected_trunk_version:
            failures.append(
                f"{job_name}: top-level TRUNK_VERSION is "
                f"{top_level_version or '<missing>'}; expected {expected_trunk_version}"
            )
        preceding_commands = [
            (line_number, command)
            for line_number, command in commands
            if line_number < invocation_line
        ]
        if not any(trunk_install in command for _, command in preceding_commands):
            failures.append(
                f"{job_name}: full-tier invocation at rust.yml:{invocation_line + 1} "
                f"lacks preceding pinned `{trunk_install}`"
            )
        if not any(trunk_verify in command or trunk_install in command for _, command in preceding_commands):
            failures.append(
                f"{job_name}: full-tier invocation at rust.yml:{invocation_line + 1} "
                f"lacks preceding `{trunk_verify}` verification"
            )

    missing_expected_jobs = sorted(required_jobs - set(full_jobs))
    failures.extend(
        f"{job}: expected full-tier job invocation was not found"
        for job in missing_expected_jobs
    )
    return full_jobs, failures


def fixture_workflow(step_lines, job_name="full-caller"):
    return "\n".join(
        [
            "name: trunk prerequisite fixture",
            "on:",
            "  workflow_dispatch:",
            "env:",
            f"  TRUNK_VERSION: {expected_trunk_version}",
            "jobs:",
            f"  {job_name}:",
            "    runs-on: ubuntu-24.04",
            "    steps:",
            *step_lines,
            "",
        ]
    )


def check_fixture(name, contents, expected_pass, required_jobs=frozenset({"full-caller"})):
    with tempfile.TemporaryDirectory(prefix="rust-full-tier-trunk-") as temp_dir:
        path = Path(temp_dir) / "rust.yml"
        path.write_text(contents, encoding="utf-8")
        _, fixture_failures = validate_workflow(path, required_jobs)
    if expected_pass and fixture_failures:
        return [f"{name}: expected pass, got {fixture_failures}"]
    if expected_pass:
        fixture_results.append(f"{name}: correctly accepted")
        return []
    if not expected_pass and not fixture_failures:
        return [f"{name}: expected contract failure, but fixture passed"]
    fixture_results.append(f"{name}: correctly rejected")
    return []


production_full_jobs, failures = validate_workflow(workflow_path, expected_jobs)
fixture_results = []
fixture_failures = []
fixture_failures.extend(
    check_fixture(
        "inline full caller without trunk",
        fixture_workflow([f"      - run: {target}"]),
        expected_pass=False,
    )
)
fixture_failures.extend(
    check_fixture(
        "unpinned trunk install",
        fixture_workflow(
            [
                "      - name: Install trunk",
                "        run: cargo install trunk --locked",
                f"      - name: Verify trunk",
                f"        run: {trunk_verify}",
                f"      - name: Run full test tier",
                f"        run: {target}",
            ]
        ),
        expected_pass=False,
    )
)
fixture_failures.extend(
    check_fixture(
        "wrong-version trunk install",
        fixture_workflow(
            [
                "      - name: Install trunk",
                '        run: cargo install trunk --locked --version "999.99.99"',
                f"      - name: Verify trunk",
                f"        run: {trunk_verify}",
                f"      - name: Run full test tier",
                f"        run: {target}",
            ]
        ),
        expected_pass=False,
    )
)
fixture_failures.extend(
    check_fixture(
        "repository pinned trunk install",
        fixture_workflow(
            [
                "      - name: Install trunk",
                f"        run: {trunk_install}",
                f"      - name: Run full test tier",
                f"        run: {target}",
            ]
        ),
        expected_pass=True,
    )
)
for commands, expected_pass in (
    ([f"      - run: {trunk_install}", f"      - run: {mixed_target}"], True),
    ([f"      - run: {mixed_target}", f"      - run: {trunk_install}"], False),
    ([f"      - run: {trunk_install}", '      - run: python3 scripts/ci-trusted-full-regression.py governance "$SOURCE_SHA"'], False),
):
    fixture_failures.extend(check_fixture(
        "mixed Trunk prerequisite", fixture_workflow(commands, "full-regression"),
        expected_pass=expected_pass, required_jobs={"full-regression"},
    ))
failures.extend(fixture_failures)

for fixture_result in fixture_results:
    print(f"fixture: {fixture_result}")

if failures:
    print("rust-full-tier-trunk-prerequisite contract: FAIL", file=sys.stderr)
    for failure in failures:
        print(f"- {failure}", file=sys.stderr)
    raise SystemExit(1)

print(
    "rust-full-tier-trunk-prerequisite contract: passed "
    f"({len(production_full_jobs)} full-tier jobs use the pinned trunk prerequisite)"
)
PY
