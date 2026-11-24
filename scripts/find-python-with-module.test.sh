#!/usr/bin/env bash
# Cross-platform test contract: verify Windows shim rejection while preserving Linux/macOS interpreter discovery.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

write_forwarding_wrapper() {
  local wrapper="$1"
  local target="$2"
  local quoted_target
  printf -v quoted_target '%q' "$target"
  {
    printf '%s\n' '#!/usr/bin/env bash'
    printf 'exec %s "$@"\n' "$quoted_target"
  } >"$wrapper"
  chmod +x "$wrapper"
}

write_helper_launcher() {
  local wrapper="$1"
  local target="$2"
  local quoted_target
  printf -v quoted_target '%q' "$target"
  {
    printf '%s\n' '#!/usr/bin/env bash'
    printf '%s\n' 'command -v grep >/dev/null 2>&1 || exit 127'
    printf 'exec %s "$@"\n' "$quoted_target"
  } >"$wrapper"
  chmod +x "$wrapper"
}

if [[ -n "${OASIS7_TEST_PYTHON:-}" ]]; then
  REAL_PYTHON="$OASIS7_TEST_PYTHON"
else
  REAL_PYTHON="$("$ROOT_DIR/scripts/find-python-with-module.sh" ast)"
fi

if [[ ! -x "$REAL_PYTHON" ]] || ! "$REAL_PYTHON" -c 'import ast; print("ready")' | grep -Fxq ready; then
  echo "find-python-with-module.test: OASIS7_TEST_PYTHON or PATH discovery must provide a functional Python interpreter" >&2
  exit 1
fi

# Keep a persistent regression for launchers that need helper commands while
# PATH is unrestricted, then fail if the child test reuses that launcher after
# its fixtures intentionally reduce PATH.  The guard permits one child run.
if [[ -z "${OASIS7_TEST_HELPER_LAUNCHER:-}" ]]; then
  helper_launcher="$TMPDIR/helper-python"
  write_helper_launcher "$helper_launcher" "$REAL_PYTHON"
  if ! OASIS7_TEST_PYTHON="$helper_launcher" \
    OASIS7_TEST_HELPER_LAUNCHER=1 \
    bash "$SCRIPT_DIR/find-python-with-module.test.sh"; then
    echo "find-python-with-module.test: helper-dependent launcher regression failed" >&2
    exit 1
  fi
fi

# Resolve launchers (for example pyenv shims) before later fixtures reduce
# PATH.  The resolved interpreter must not depend on helper commands supplied
# by the launcher environment.
REAL_PYTHON="$("$REAL_PYTHON" -c 'import sys; print(sys.executable)')"
if [[ ! -x "$REAL_PYTHON" ]]; then
  echo "find-python-with-module.test: launcher did not expose an executable sys.executable: $REAL_PYTHON" >&2
  exit 1
fi

mkdir -p "$TMPDIR/broken-bin" "$TMPDIR/working-bin"
for name in python python3; do
  cat >"$TMPDIR/broken-bin/$name" <<'SH'
#!/usr/bin/env bash
# Simulates the Windows user-level python shim that exits 0 without executing input.
exit 0
SH
  chmod +x "$TMPDIR/broken-bin/$name"
done
for utility in bash tr dirname basename; do
  ln -s "$(command -v "$utility")" "$TMPDIR/broken-bin/$utility"
done
write_forwarding_wrapper "$TMPDIR/working-bin/python42" "$REAL_PYTHON"
generic_home="$TMPDIR/no-bundled-home"
mkdir -p "$generic_home"

selected="$(HOME="$generic_home" OASIS7_PYTHON_BIN= PATH="$TMPDIR/broken-bin:$TMPDIR/working-bin:$PATH" \
  "$ROOT_DIR/scripts/find-python-with-module.sh" ast)"
if [[ "$selected" != "$TMPDIR/working-bin/python42" ]]; then
  echo "find-python-with-module.test: generic discovery did not select python42: $selected" >&2
  exit 1
fi
if ! "$selected" -c 'import ast; print("selected")' | grep -Fxq selected; then
  echo "find-python-with-module.test: selected interpreter cannot execute Python" >&2
  exit 1
fi

# Explicit selection works without a particular agent's runtime and rejects bad
# overrides rather than silently picking the bundled or PATH interpreter.
override_selected="$(HOME="$generic_home" OASIS7_PYTHON_BIN="$TMPDIR/working-bin/python42" \
  PATH="$TMPDIR/broken-bin" "$ROOT_DIR/scripts/find-python-with-module.sh" ast)"
test "$override_selected" = "$TMPDIR/working-bin/python42"
if HOME="$generic_home" OASIS7_PYTHON_BIN="$TMPDIR/broken-bin/python" \
  PATH="$TMPDIR/broken-bin:$TMPDIR/working-bin" \
  "$ROOT_DIR/scripts/find-python-with-module.sh" ast >/dev/null 2>&1; then
  echo "find-python-with-module.test: invalid explicit interpreter accepted" >&2
  exit 1
fi

# The Codex bundled runtime uses a Unix bin/python3 layout.  Isolate HOME and
# PATH so this contract exercises the bundled candidate rather than a host
# installation or the generic future-python shim above.
bundled_home="$TMPDIR/bundled-home"
bundled_python_dir="$bundled_home/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin"
mkdir -p "$bundled_python_dir"
write_forwarding_wrapper "$bundled_python_dir/python3" "$REAL_PYTHON"
bundled_selected="$(HOME="$bundled_home" OASIS7_PYTHON_BIN= PATH="$TMPDIR/broken-bin" \
  "$ROOT_DIR/scripts/find-python-with-module.sh" ast)"
if [[ "$bundled_selected" != "$bundled_python_dir/python3" ]]; then
  echo "find-python-with-module.test: did not select Unix bundled runtime: $bundled_selected" >&2
  exit 1
fi

echo "find-python-with-module.test: OK"
