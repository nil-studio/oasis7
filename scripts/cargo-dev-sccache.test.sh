#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Optional real compiler proof: requires installed sccache and cached itoa.
if [[ "${1:-}" == "--real" ]]; then
  python3 - "$ROOT_DIR" <<'PYTHON'
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile

root = Path(sys.argv[1])
with tempfile.TemporaryDirectory(prefix="oasis7-sccache-proof-") as directory:
    fixture = Path(directory)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    env = dict(os.environ, CI="", OASIS7_CARGO_SCCACHE="1",
               SCCACHE_DIR=str(fixture / "cache"), SCCACHE_SERVER_PORT=str(port))
    try:
        previous_hits = 0
        for name in ("a", "b"):
            repo = fixture / name
            (repo / "src").mkdir(parents=True)
            (repo / "scripts").mkdir()
            (repo / "Cargo.toml").write_text(
                '[package]\nname="cache_proof"\nversion="0.1.0"\nedition="2021"\n'
                '[dependencies]\nitoa="1"\n')
            (repo / "src/lib.rs").write_text(
                'pub fn format(n: u64) -> String { itoa::Buffer::new().format(n).to_owned() }\n')
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            for filename in ("cargo-dev.sh", "cargo-cache.py", "find-python-with-module.sh"):
                shutil.copy2(root / "scripts" / filename, repo / "scripts" / filename)
            subprocess.run([str(repo / "scripts/cargo-dev.sh"), "build", "--offline"],
                           cwd=repo, env=env, check=True)
            stats = json.loads(subprocess.check_output(
                ["sccache", "--show-stats", "--stats-format=json"], env=env))
            hits = sum(stats["stats"]["cache_hits"]["counts"].values())
            if name == "b" and hits <= previous_hits:
                raise RuntimeError("second source/target path did not reuse a Rust compilation")
            previous_hits = hits
        print(f"cargo-dev-sccache real proof: {hits} cache hit(s)")
    finally:
        subprocess.run(["sccache", "--stop-server"], env=env, check=False,
                       stdout=subprocess.DEVNULL)
PYTHON
  exit 0
fi
FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/repo/scripts" "$FIXTURE/bin"
cp "$ROOT_DIR/scripts/cargo-dev.sh" "$ROOT_DIR/scripts/find-python-with-module.sh" "$ROOT_DIR/scripts/cargo-cache.py" "$FIXTURE/repo/scripts/"
git -C "$FIXTURE/repo" init -q
cat >"$FIXTURE/bin/rustc" <<'EOF'
#!/usr/bin/env bash
[[ "${1:-}" == "-vV" ]] || exit 1
printf '%s\n' 'rustc 1.96.0-fixture' 'host: x86_64-unknown-linux-gnu' 'release: 1.96.0-fixture'
EOF
cat >"$FIXTURE/bin/sccache" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
cat >"$FIXTURE/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "wrapper=${RUSTC_WRAPPER:-unset}" "incremental=${CARGO_INCREMENTAL:-unset}" "target=${CARGO_TARGET_DIR:-unset}" "dev-debug=${CARGO_PROFILE_DEV_DEBUG:-unset}" "test-debug=${CARGO_PROFILE_TEST_DEBUG:-unset}" "$@"
EOF
chmod +x "$FIXTURE/bin/"*
cd "$FIXTURE/repo"
export PATH="$FIXTURE/bin:$PATH"
export CI= OASIS7_CARGO_SCCACHE=1 OASIS7_CARGO_DEV_MODE=lean
export RUSTC_WRAPPER=hostile-wrapper CARGO_TARGET_DIR=hostile-target CARGO_INCREMENTAL=1
expected_target="$(./scripts/cargo-dev.sh --print-target-dir)"
result="$(./scripts/cargo-dev.sh run --bin example -- --target-dir application-argument)"
[[ "$result" == *"wrapper=$FIXTURE/bin/sccache"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
[[ "$result" == *"incremental=0"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
[[ "$result" == *"target=unset"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
[[ "$result" == *$'run\n--target-dir\n'* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
[[ "$result" == *$'--\n--target-dir\napplication-argument' ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
[[ "$result" == *"dev-debug=0"* && "$result" == *"test-debug=0"* ]] || exit 1
for mode in build test; do
  result="$(./scripts/cargo-dev.sh "$mode")"
  [[ "$result" == *"wrapper=$FIXTURE/bin/sccache"* && "$result" == *"incremental=0"* && "$result" == *"target=unset"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
done
for sccache in 0 1; do
  result="$(OASIS7_CARGO_SCCACHE="$sccache" CARGO_INCREMENTAL=0 ./scripts/cargo-dev.sh check -p example --all-targets)"
  [[ "$result" == *"wrapper=unset"* && "$result" == *"incremental=1"* && "$result" == *"target=$expected_target"* && "$result" == *"dev-debug=0"* && "$result" == *"test-debug=0"* ]] || { echo "lean check must use incremental compilation without sccache: $result" >&2; exit 1; }
  [[ "$result" == *$'check\n-p\nexample\n--all-targets' ]] || { echo "unexpected check arguments: $result" >&2; exit 1; }
done
result="$(./scripts/cargo-dev.sh clippy)"
[[ "$result" == *"wrapper=unset"* && "$result" == *"incremental=0"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
for mode in build check; do
  result="$(OASIS7_CARGO_DEV_MODE=debug ./scripts/cargo-dev.sh "$mode")"
  [[ "$result" == *"wrapper=unset"* && "$result" == *"incremental=1"* && "$result" == *"dev-debug=2"* && "$result" == *"test-debug=2"* ]] || exit 1
done
for setting in 'CI=1' 'CI=true' 'OASIS7_CARGO_SCCACHE=0'; do
  result="$(env "$setting" ./scripts/cargo-dev.sh build)"
  [[ "$result" == *"wrapper=unset"* && "$result" != *"target=unset"* && "$result" == *"incremental=$([[ "$setting" == CI=* ]] && echo 1 || echo 0)"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
done
for ci in 1 true; do
  result="$(CI="$ci" CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=1 CARGO_PROFILE_TEST_DEBUG=2 ./scripts/cargo-dev.sh check)"
  [[ "$result" == *"wrapper=unset"* && "$result" == *"incremental=0"* && "$result" == *"target=$expected_target"* && "$result" == *"dev-debug=1"* && "$result" == *"test-debug=2"* ]] || { echo "CI check must preserve the caller's profile environment: $result" >&2; exit 1; }
done
# Simulate a missing installation even when the host has sccache.
command() {
  if [[ "${1:-}" == "-v" && "${2:-}" == "sccache" ]]; then return 1; fi
  builtin command "$@"
}
export -f command
result="$(./scripts/cargo-dev.sh build)"
[[ "$result" == *"wrapper=unset"* && "$result" != *"target=unset"* && "$result" == *"incremental=0"* ]] || { echo "unexpected cargo environment or arguments: $result" >&2; exit 1; }
unset -f command
echo "cargo-dev-sccache.test: OK"
