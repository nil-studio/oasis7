#!/usr/bin/env bash
# Cross-platform maintenance: preserve Windows Git Bash/PowerShell and Linux/macOS Python discovery behavior.
set -euo pipefail

if [[ $# -ne 1 ]] || [[ ! "$1" =~ ^[A-Za-z_][A-Za-z0-9_.]*$ ]]; then
  echo "Usage: find-python-with-module.sh <python-module>" >&2
  exit 2
fi

MODULE="$1"

supports_module() {
  local candidate="$1"
  local output
  [[ -x "$candidate" ]] || return 1
  output="$("$candidate" - "$MODULE" 2>/dev/null <<'PY'
import importlib
import sys

importlib.import_module(sys.argv[1])
print("oasis7-python-module-ok")
PY
)" || return 1
  output="${output%$'\r'}"
  [[ "$output" == "oasis7-python-module-ok" ]]
}

emit_if_supported() {
  local candidate="$1"
  if supports_module "$candidate"; then
    (cd "$(dirname "$candidate")" && printf '%s/%s\n' "$PWD" "$(basename "$candidate")")
    exit 0
  fi
}

# An explicit interpreter works across Coding Agent environments that do not
# expose their bundled Python on PATH. A bad override must not silently fall back.
if [[ -n "${OASIS7_PYTHON_BIN:-}" ]]; then
  if ! supports_module "$OASIS7_PYTHON_BIN"; then
    echo "find-python-with-module: OASIS7_PYTHON_BIN cannot import ${MODULE}" >&2
    exit 1
  fi
  emit_if_supported "$OASIS7_PYTHON_BIN"
fi

for generic_name in python python3; do
  if candidate="$(command -v "$generic_name" 2>/dev/null)"; then
    emit_if_supported "$candidate"
  fi
done

while IFS= read -r path_dir; do
  [[ -n "$path_dir" ]] || path_dir="."
  for candidate in "$path_dir"/python*; do
    [[ -e "$candidate" ]] || continue
    emit_if_supported "$candidate"
  done
done < <(printf '%s' "$PATH" | tr ':' '\n')

echo "find-python-with-module: no Python interpreter on PATH can import ${MODULE}; set OASIS7_PYTHON_BIN to an explicit interpreter path" >&2
exit 1
