#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
script_path="$repo_root/scripts/check-standalone-tool-lockfiles.sh"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT
real_git="$(command -v git)"

fixture_repo="$tmp_dir/repo"
mkdir -p "$fixture_repo/tools/valid_tool/src"
mkdir -p "$fixture_repo/crates/oasis7_builtin_wasm_modules/valid_module/src"
cd "$fixture_repo"
git init -q
git config user.email "test@example.invalid"
git config user.name "Test User"

mkdir -p tools/workspace_member/src
cat >Cargo.toml <<'TOML'
[workspace]
members = ["tools/workspace_member"]
resolver = "2"
TOML
cat >tools/workspace_member/Cargo.toml <<'TOML'
[package]
name = "workspace_member"
version = "0.1.0"
edition = "2021"

[dependencies]
TOML
cat >tools/workspace_member/src/main.rs <<'RS'
fn main() {}
RS
env -u RUSTC_WRAPPER cargo generate-lockfile --manifest-path Cargo.toml

cat >tools/valid_tool/Cargo.toml <<'TOML'
[package]
name = "valid_tool"
version = "0.1.0"
edition = "2021"

[dependencies]

[workspace]
TOML
cat >tools/valid_tool/src/main.rs <<'RS'
fn main() {}
RS
env -u RUSTC_WRAPPER cargo generate-lockfile --manifest-path tools/valid_tool/Cargo.toml
cat >crates/oasis7_builtin_wasm_modules/valid_module/Cargo.toml <<'TOML'
[package]
name = "valid_module"
version = "0.1.0"
edition = "2021"

[dependencies]

[workspace]
TOML
cat >crates/oasis7_builtin_wasm_modules/valid_module/src/lib.rs <<'RS'
pub fn valid() {}
RS
env -u RUSTC_WRAPPER cargo generate-lockfile \
  --manifest-path crates/oasis7_builtin_wasm_modules/valid_module/Cargo.toml
git add Cargo.toml Cargo.lock \
  tools/workspace_member/Cargo.toml tools/workspace_member/src/main.rs \
  tools/valid_tool/Cargo.toml tools/valid_tool/Cargo.lock tools/valid_tool/src/main.rs \
  crates/oasis7_builtin_wasm_modules/valid_module/Cargo.toml \
  crates/oasis7_builtin_wasm_modules/valid_module/Cargo.lock \
  crates/oasis7_builtin_wasm_modules/valid_module/src/lib.rs
git commit -q -m "valid tool"

valid_out="$tmp_dir/valid.out"
fake_bin="$tmp_dir/fake-bin"
mkdir -p "$fake_bin"
cat >"$fake_bin/git" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "ls-files" ]]; then
  echo "git ls-files $*" >>"$OASIS7_GIT_CALL_LOG"
fi
exec "$OASIS7_REAL_GIT" "$@"
SH
chmod +x "$fake_bin/git"

OASIS7_STANDALONE_TOOL_REPO_ROOT="$fixture_repo" \
  OASIS7_REAL_GIT="$real_git" \
  OASIS7_GIT_CALL_LOG="$tmp_dir/git-calls.log" \
  PATH="$fake_bin:$PATH" \
  "$script_path" >"$valid_out"
grep -q "ok: standalone lockfiles are locked and manifest-consistent (2 manifests)" "$valid_out"
grep -q "checking standalone lockfile: tools/valid_tool/Cargo.toml" "$valid_out"
grep -q "checking standalone lockfile: crates/oasis7_builtin_wasm_modules/valid_module/Cargo.toml" "$valid_out"
if grep -q "checking standalone lockfile: tools/workspace_member/Cargo.toml" "$valid_out"; then
  echo "root workspace member without a standalone lockfile was checked as standalone" >&2
  exit 1
fi
test "$(grep -c "^git ls-files " "$tmp_dir/git-calls.log")" -eq 1

# A nested independent workspace owns one lockfile for all its members.
mkdir -p tools/nested/core/src tools/nested/host/src
cat >tools/nested/Cargo.toml <<'TOML'
[workspace]
members = ["core", "host"]
resolver = "2"
TOML
for member in core host; do
  printf '[package]\nname = "nested_%s"\nversion = "0.1.0"\nedition = "2021"\n' "$member" >"tools/nested/$member/Cargo.toml"
  echo 'pub fn ready() {}' >"tools/nested/$member/src/lib.rs"
done
env -u RUSTC_WRAPPER cargo generate-lockfile --manifest-path tools/nested/Cargo.toml
git add tools/nested
git commit -q -m "nested independent workspace"
OASIS7_STANDALONE_TOOL_REPO_ROOT="$fixture_repo" "$script_path" >"$tmp_dir/nested.out"
grep -q "ok: standalone lockfiles are locked and manifest-consistent (5 manifests)" "$tmp_dir/nested.out"

cargo_call_log="$tmp_dir/cargo-calls.log"
cat >"$fake_bin/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "locate-project" && " $* " != *" --no-deps "* ]]; then
  echo "unexpected standalone Cargo metadata invocation with Rust baseline disabled: $*" >&2
  exit 1
fi
printf '%s\n' "$*" >>"$OASIS7_CARGO_CALL_LOG"
exec "$OASIS7_REAL_CARGO" "$@"
SH
chmod +x "$fake_bin/cargo"
: >"$cargo_call_log"
structural_out="$tmp_dir/structural.out"
if ! OASIS7_STANDALONE_TOOL_REPO_ROOT="$fixture_repo" \
  OASIS7_REAL_GIT="$real_git" \
  OASIS7_GIT_CALL_LOG="$tmp_dir/git-calls-no-cargo.log" \
  OASIS7_REAL_CARGO="$(command -v cargo)" \
  OASIS7_CARGO_CALL_LOG="$cargo_call_log" \
  OASIS7_CI_RUN_RUST_BASELINE=false \
  PATH="$fake_bin:$PATH" \
  "$script_path" >"$structural_out"; then
  echo "expected structural standalone lockfile checks to pass with Rust baseline disabled" >&2
  cat "$structural_out" >&2
  exit 1
fi
grep -q "ok: standalone lockfiles are locked and manifest-consistent (5 manifests)" "$structural_out"
expected_locked_calls=0
if [[ "${OASIS7_LOCAL_CHECK:-0}" == 1 ]]; then expected_locked_calls=1; fi
if [[ "$(grep -c -- "--no-deps" "$cargo_call_log")" -ne 1 \
   || "$(grep -c -- "--locked" "$cargo_call_log" || true)" -ne "$expected_locked_calls" ]]; then
  echo "standalone lockfile structural checks invoked Cargo beyond root workspace membership discovery" >&2
  cat "$cargo_call_log" >&2
  exit 1
fi

mkdir -p tools/missing_lock/src
cat >tools/missing_lock/Cargo.toml <<'TOML'
[package]
name = "missing_lock"
version = "0.1.0"
edition = "2021"

[dependencies]
TOML
cat >tools/missing_lock/src/main.rs <<'RS'
fn main() {}
RS
git add tools/missing_lock/Cargo.toml tools/missing_lock/src/main.rs
if OASIS7_STANDALONE_TOOL_REPO_ROOT="$fixture_repo" "$script_path" >"$tmp_dir/missing-lock.out" 2>&1; then
  echo "expected manifest without tracked lockfile to fail" >&2
  exit 1
fi
grep -q "standalone tool lockfile missing: tools/missing_lock/Cargo.lock" "$tmp_dir/missing-lock.out"
git reset -q --hard HEAD

mkdir -p tools/orphan_lock
cp tools/valid_tool/Cargo.lock tools/orphan_lock/Cargo.lock
git add tools/orphan_lock/Cargo.lock
if OASIS7_STANDALONE_TOOL_REPO_ROOT="$fixture_repo" "$script_path" >"$tmp_dir/orphan-lock.out" 2>&1; then
  echo "expected orphan lockfile to fail" >&2
  exit 1
fi
grep -q "standalone manifest missing for lockfile: tools/orphan_lock/Cargo.lock" "$tmp_dir/orphan-lock.out"

echo "check-standalone-tool-lockfiles.test: OK"
