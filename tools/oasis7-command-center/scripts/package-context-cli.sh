#!/bin/bash
set -euo pipefail
# Build from the same checkout as the App. No network download or PATH lookup at runtime.
repo_root=$(git rev-parse --show-toplevel)
app_root="$repo_root/tools/oasis7-command-center"
requested_target=""
if (($#)); then
  if [[ $# == 2 && "$1" == --target ]]; then requested_target="$2";
  elif [[ $# == 1 && "$1" == --target=* ]]; then requested_target="${1#--target=}";
  else echo "Usage: package-context-cli.sh [--target RUST_TRIPLE]" >&2; exit 1; fi
  if [[ -z "$requested_target" ]]; then echo "--target requires a Rust target triple" >&2; exit 1; fi
fi
target=${requested_target:-$(rustc -vV | sed -n 's/^host: //p')}
if [[ "$target" == universal-apple-darwin || ! "$target" =~ ^[a-zA-Z0-9_]+(-[a-zA-Z0-9_]+){2,}$ ]]; then
  echo "A single Rust target triple is required; universal/custom target packaging is unsupported" >&2; exit 1
fi

require_committed_inputs() {
  local untracked ignored_inputs
  if ! untracked=$(git -C "$repo_root" ls-files --others --exclude-standard) \
    || ! ignored_inputs=$(git -C "$repo_root" ls-files --others --ignored --exclude-standard -- \
      crates/oasis7_context crates/oasis7_secure_envelope ':(exclude)**/target/**'); then
    echo "Cannot verify the source inventory; no CLI catalog can be published." >&2
    exit 1
  fi
  if ! git -C "$repo_root" diff --quiet || ! git -C "$repo_root" diff --cached --quiet \
    || [[ -n "$untracked" || -n "$ignored_inputs" ]]; then
    echo "Package from committed inputs only; tracked changes and untracked files cannot identify HEAD." >&2
    exit 1
  fi
}
require_committed_inputs
head=$(git -C "$repo_root" rev-parse HEAD)
release="context-$head"
# Cargo's emitted executable is authoritative, including configured target directories.
binary=$(OASIS7_SOURCE_HEAD_OID="$head" OASIS7_RELEASE_ID="$release" OASIS7_BUILD_TARGET="$target" \
  "$repo_root/scripts/cargo-dev.sh" build --locked --release -p oasis7_context \
    --message-format=json-render-diagnostics --target "$target" | python3 -c '
import json, sys
executables = set()
for line in sys.stdin:
    message = json.loads(line)
    target = message.get("target", {})
    if (message.get("reason") == "compiler-artifact"
            and target.get("name") == "oasis7_context"
            and "bin" in target.get("kind", [])
            and message.get("executable")):
        executables.add(message["executable"])
if len(executables) != 1:
    raise SystemExit("Expected one Cargo executable artifact for oasis7_context")
print(executables.pop())
')
# Do not publish if the checkout changed while Cargo was reading its inputs.
require_committed_inputs
if [[ "$(git -C "$repo_root" rev-parse HEAD)" != "$head" ]]; then
  echo "Source HEAD changed during the CLI build; no catalog published." >&2
  exit 1
fi
out="$app_root/src-tauri/resources/context-cli"
mkdir -p "$out"
staging=$(mktemp "$out/.context-cli.XXXXXX")
trap 'rm -f "$staging"' EXIT
cp "$binary" "$staging"
# The macOS bundler must clear attributes and sign this generated resource.
chmod 755 "$staging"
mv -f "$staging" "$out/oasis7_context"
python3 - "$out" "$head" "$target" "$release" <<'PYTHON'
import hashlib,json,pathlib,sys
root=pathlib.Path(sys.argv[1]); data=(root/'oasis7_context').read_bytes()
(root/'catalog.json').write_text(json.dumps(dict(schema_version=1,tool_id='oasis7_context',source_head_oid=sys.argv[2],target=sys.argv[3],release_id=sys.argv[4],file='oasis7_context',sha256=hashlib.sha256(data).hexdigest(),size_bytes=len(data)),sort_keys=True,separators=(',',':'))+'\n')
PYTHON
