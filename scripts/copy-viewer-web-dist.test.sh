#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
source "$ROOT_DIR/scripts/viewer-web-dist-contract.sh"
TMPDIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

viewer_root="$TMPDIR/viewer"
dist_dir="$TMPDIR/dist"
optional_payload_dir="$TMPDIR/optional-payload"
mkdir -p "$viewer_root/dist/pixel-world-bridge/webgl2"
mkdir -p "$viewer_root/dist/browser-game-client"
printf 'export const protocol = true;\n' > "$viewer_root/dist/browser-game-client/oasis7_browser_game_client.js"
printf '\0asm\n' > "$viewer_root/dist/browser-game-client/oasis7_browser_game_client_bg.wasm"

printf '<!doctype html><link rel="stylesheet" href="./viewer_terminal_shell.css"><script type="module" src="./viewer.js"></script>\n' > "$viewer_root/viewer.html"
printf '.shell { grid-template-columns: minmax(0, 1fr); }\n' > "$viewer_root/viewer_terminal_shell.css"
printf 'console.log("canonical bundle");\n' > "$viewer_root/viewer.js"
cp "$ROOT_DIR/crates/oasis7_viewer/viewer_first_agent_claim_evidence.html" \
  "$viewer_root/viewer_first_agent_claim_evidence.html"
node "$ROOT_DIR/crates/oasis7_viewer/scripts/viewer-compat-aliases.mjs" "$viewer_root" "$viewer_root/dist"
printf 'icon\n' > "$viewer_root/favicon.ico"
printf 'export const bridge = true;\n' > "$viewer_root/dist/pixel-world-bridge/pixel_world_bridge.js"
printf 'export const webgl2Bridge = true;\n' > "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge.js"
printf 'export function init() {}\n' > "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js"
printf '\0asm\n' > "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"

"$ROOT_DIR/scripts/copy-viewer-web-dist.sh" \
  --viewer-root "$viewer_root" \
  --dist-dir "$dist_dir" \
  --optional-payload-dir "$optional_payload_dir"

failures=0
expect_cmp() {
  local source=$1
  local copied=$2
  if ! cmp "$source" "$copied"; then
    echo "expected copied asset to match: $copied" >&2
    failures=1
  fi
}

expect_cmp "$viewer_root/dist/browser-game-client/oasis7_browser_game_client.js" "$dist_dir/browser-game-client/oasis7_browser_game_client.js"
expect_cmp "$viewer_root/dist/browser-game-client/oasis7_browser_game_client_bg.wasm" "$dist_dir/browser-game-client/oasis7_browser_game_client_bg.wasm"
expect_cmp "$viewer_root/viewer.js" "$dist_dir/viewer.js"
expect_cmp "$viewer_root/dist/software_safe.js" "$dist_dir/software_safe.js"
expect_cmp "$viewer_root/viewer.html" "$dist_dir/index.html"
expect_cmp "$viewer_root/viewer.html" "$dist_dir/viewer.html"
expect_cmp "$viewer_root/dist/software_safe.html" "$dist_dir/software_safe.html"
expect_cmp "$viewer_root/viewer_terminal_shell.css" "$dist_dir/viewer_terminal_shell.css"
expect_cmp "$viewer_root/viewer_first_agent_claim_evidence.html" "$dist_dir/viewer_first_agent_claim_evidence.html"
expect_cmp "$viewer_root/dist/software_safe_first_agent_claim_evidence.html" "$dist_dir/software_safe_first_agent_claim_evidence.html"
if [[ -e "$viewer_root/software_safe.html" || -e "$viewer_root/software_safe.js" \
  || -e "$viewer_root/software_safe_first_agent_claim_evidence.html" ]]; then
  echo "generated compatibility aliases must not be required in the viewer source root" >&2
  exit 1
fi
expect_cmp "$viewer_root/favicon.ico" "$dist_dir/favicon.ico"
expect_cmp "$viewer_root/dist/pixel-world-bridge/pixel_world_bridge.js" "$dist_dir/pixel-world-bridge/pixel_world_bridge.js"
expect_cmp "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge.js" "$dist_dir/pixel-world-bridge/webgl2/pixel_world_bridge.js"
expect_cmp "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js" "$dist_dir/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js"
if [[ -e "$dist_dir/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" ]]; then
  echo "optional pixel-world WASM must not be copied into the primary viewer dist" >&2
  exit 1
fi
cmp "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" \
  "$optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm"
python3 - "$dist_dir/optional-payloads.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = manifest["pixel_world_bridge_bindgen_bg.wasm"]
assert payload == {
    "available": False,
    "reason": "separate_artifact",
}, payload
PY

configured_dist_dir="$TMPDIR/configured-dist"
configured_optional_payload_dir="$TMPDIR/configured-optional-payload"
configured_public_path="../viewer-optional-payload/pixel_world_bridge_bindgen_bg.wasm"
mkdir -p "$configured_optional_payload_dir"
"$ROOT_DIR/scripts/copy-viewer-web-dist.sh" \
  --viewer-root "$viewer_root" \
  --dist-dir "$configured_dist_dir" \
  --optional-payload-dir "$configured_optional_payload_dir" \
  --optional-payload-public-path "$configured_public_path"
python3 - "$configured_dist_dir/optional-payloads.json" "$configured_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm" "$configured_public_path" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = manifest["pixel_world_bridge_bindgen_bg.wasm"]
content = Path(sys.argv[2]).read_bytes()
assert payload == {
    "available": True,
    "path": sys.argv[3],
    "sha256": hashlib.sha256(content).hexdigest(),
    "size_bytes": len(content),
    "delivery": "separate_artifact",
    "provenance": "viewer-web-build",
}, payload
PY

public_delivery_dir="$TMPDIR/public-delivery"
public_dist_dir="$public_delivery_dir/web-dist"
public_optional_payload_dir="$public_delivery_dir/viewer-optional-payload"
public_archive="$TMPDIR/oasis7-viewer-web-delivery.tar.gz"
mkdir -p "$public_optional_payload_dir"
"$ROOT_DIR/scripts/copy-viewer-web-dist.sh" \
  --viewer-root "$viewer_root" \
  --dist-dir "$public_dist_dir" \
  --optional-payload-dir "$public_optional_payload_dir"
python3 - "$public_dist_dir/optional-payloads.json" "$public_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = manifest["pixel_world_bridge_bindgen_bg.wasm"]
assert payload == {
    "available": False,
    "reason": "separate_artifact",
}, payload
PY
"$ROOT_DIR/scripts/package-viewer-web-delivery.sh" \
  --web-dist "$public_dist_dir" \
  --optional-payload-dir "$public_optional_payload_dir" \
  --out-file "$public_archive"
printf 'AppleDouble sidecar that must never enter the delivery archive\n' \
  > "$public_dist_dir/._viewer-sidecar"
printf 'AppleDouble payload sidecar that must never enter the delivery archive\n' \
  > "$public_optional_payload_dir/._payload-sidecar"
python3 - "$public_dist_dir/viewer.js" "$public_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm" <<'PY'
import os
import sys

for raw_path in sys.argv[1:]:
    try:
        os.setxattr(raw_path, b"user.oasis7_test_provenance", b"must-not-be-archived")
    except (AttributeError, OSError):
        pass
PY
source_mtime_path="$ROOT_DIR/crates/oasis7_viewer/software_safe_src/pixel_world_runtime_module_wasm.test.js"
public_archive_repeat="$TMPDIR/oasis7-viewer-web-delivery-repeat.tar.gz"
python3 - "$public_dist_dir/viewer.js" "$public_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm" "$source_mtime_path" "$ROOT_DIR/scripts/package-viewer-web-delivery.sh" "$public_dist_dir" "$public_optional_payload_dir" "$public_archive_repeat" <<'PY'
import os
import subprocess
import sys

for raw_path in sys.argv[1:3]:
    os.utime(raw_path, ns=(1_700_000_000_123_456_789, 1_700_000_000_123_456_789))
source_path = sys.argv[3]
helper_path = sys.argv[4]
source_stat = os.stat(source_path)
os.utime(source_path, ns=(source_stat.st_atime_ns, 1_700_000_100_987_654_321))
try:
    subprocess.run([
        helper_path,
        "--web-dist", sys.argv[5],
        "--optional-payload-dir", sys.argv[6],
        "--out-file", sys.argv[7],
    ], check=True)
finally:
    os.utime(source_path, ns=(source_stat.st_atime_ns, source_stat.st_mtime_ns))
PY
python3 - "$public_archive" "$public_archive_repeat" "$public_dist_dir" <<'PY'
import json
import hashlib
import sys
import tarfile
from pathlib import Path

first_path = Path(sys.argv[1])
second_path = Path(sys.argv[2])
web_dist = Path(sys.argv[3])
first_bytes = first_path.read_bytes()
second_bytes = second_path.read_bytes()
assert first_bytes == second_bytes, (
    hashlib.sha256(first_bytes).hexdigest(),
    hashlib.sha256(second_bytes).hexdigest(),
)
assert first_bytes[4:8] == b"\0\0\0\0", "gzip mtime must be zero"
assert b"SCHILY.xattr" not in first_bytes
assert b"LIBARCHIVE.xattr" not in first_bytes
assert b"com.apple.provenance" not in first_bytes

expected = {
    path.relative_to(web_dist).as_posix()
    for path in web_dist.rglob("*")
    if path.is_file() and not any(part.startswith("._") for part in path.relative_to(web_dist).parts)
}
expected.update(name + ".gz" for name in list(expected) if name.endswith((".js", ".css")))
payload_hash = hashlib.sha256(b"\0asm\n").hexdigest()
expected.add(f"viewer-optional-payload/{payload_hash}.wasm")
expected.add(f"viewer-optional-payload/{payload_hash}.wasm.gz")
with tarfile.open(first_path, "r:gz") as archive:
    members = archive.getmembers()
    names = [member.name for member in members]
    assert names == sorted(names), names
    assert set(names) == expected, (set(names) ^ expected)
    for member in members:
        assert not any(part.startswith("._") for part in Path(member.name).parts), member.name
        assert member.pax_headers == {}, member.pax_headers
        assert member.uid == 0 and member.gid == 0, member
        assert member.uname == "" and member.gname == "", member
        assert member.mtime == 0 and member.mode == 0o644, member
        if member.name == ".oasis7-viewer-dist-manifest.json":
            delivery_manifest = json.load(archive.extractfile(member))
            assert "sourceLatestPath" not in delivery_manifest, delivery_manifest
            assert "sourceLatestMtimeNs" not in delivery_manifest, delivery_manifest
PY
extract_dir="$TMPDIR/public-delivery-extracted"
mkdir -p "$extract_dir"
tar -xzf "$public_archive" -C "$extract_dir"
python3 - "$extract_dir/optional-payloads.json" "$extract_dir" <<'PY'
import json
import hashlib
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = manifest["pixel_world_bridge_bindgen_bg.wasm"]
resolved = (Path(sys.argv[1]).parent / payload["path"]).resolve()
content = resolved.read_bytes()
assert payload["available"] is True, payload
assert payload["path"] == f"viewer-optional-payload/{hashlib.sha256(content).hexdigest()}.wasm"
import gzip
assert gzip.decompress(Path(str(resolved) + ".gz").read_bytes()) == content
assert payload["encodings"]["gzip"]["size_bytes"] == Path(str(resolved) + ".gz").stat().st_size
assert payload["sha256"] == hashlib.sha256(content).hexdigest(), payload
assert payload["size_bytes"] == len(content), payload
assert payload["delivery"] == "separate_artifact", payload
assert payload["provenance"] == "viewer-web-build", payload
assert resolved == (Path(sys.argv[2]) / payload["path"]).resolve()
assert resolved.is_file(), resolved
assert not (Path(sys.argv[2]) / "pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm").exists()
PY

missing_viewer_root="$TMPDIR/missing-viewer"
missing_dist_dir="$TMPDIR/missing-dist"
missing_optional_payload_dir="$TMPDIR/missing-optional-payload"
cp -R "$viewer_root" "$missing_viewer_root"
rm "$missing_viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"
mkdir -p "$missing_optional_payload_dir"
printf 'stale bytes from another build\n' \
  > "$missing_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm"

"$ROOT_DIR/scripts/copy-viewer-web-dist.sh" \
  --viewer-root "$missing_viewer_root" \
  --dist-dir "$missing_dist_dir" \
  --optional-payload-dir "$missing_optional_payload_dir"

if [[ -e "$missing_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm" ]]; then
  echo "missing optional pixel-world WASM must not create a staged payload" >&2
  exit 1
fi
python3 - "$missing_dist_dir/optional-payloads.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = manifest["pixel_world_bridge_bindgen_bg.wasm"]
assert payload["available"] is False, payload
assert payload["reason"] == "source_missing", payload
assert "path" not in payload, payload
PY

in_place_dir="$TMPDIR/in-place-viewer"
in_place_optional_payload_dir="$TMPDIR/in-place-optional-payload"
cp -R "$viewer_root" "$in_place_dir"

for pass in first second; do
  "$ROOT_DIR/scripts/copy-viewer-web-dist.sh" \
    --viewer-root "$in_place_dir" \
    --dist-dir "$in_place_dir" \
    --optional-payload-dir "$in_place_optional_payload_dir"

  if [[ ! -e "$in_place_dir/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" ]]; then
    echo "in-place split pass '$pass' removed canonical source WASM" >&2
    exit 1
  fi
  cmp "$viewer_root/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm" \
    "$in_place_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm"
  python3 - "$in_place_dir/optional-payloads.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert manifest == {
    "pixel_world_bridge_bindgen_bg.wasm": {
        "available": False,
        "reason": "separate_artifact",
    }
}, manifest
PY
done

rm "$in_place_dir/dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"
printf 'stale bytes from another build\n' \
  > "$in_place_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm"
"$ROOT_DIR/scripts/copy-viewer-web-dist.sh" \
  --viewer-root "$in_place_dir" \
  --dist-dir "$in_place_dir" \
  --optional-payload-dir "$in_place_optional_payload_dir"
if [[ -e "$in_place_optional_payload_dir/pixel_world_bridge_bindgen_bg.wasm" ]]; then
  echo "stale staged optional WASM must be removed when source is missing" >&2
  exit 1
fi
python3 - "$in_place_dir/optional-payloads.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = manifest["pixel_world_bridge_bindgen_bg.wasm"]
assert payload == {"available": False, "reason": "source_missing"}, payload
PY

if ! viewer_web_dist_contract_pairs | grep -Fxq "viewer_terminal_shell.css viewer_terminal_shell.css"; then
  echo "viewer dist contract must include viewer_terminal_shell.css" >&2
  failures=1
fi
if ! viewer_web_dist_required_files | grep -Fxq "viewer_terminal_shell.css"; then
  echo "viewer dist required files must include viewer_terminal_shell.css" >&2
  failures=1
fi

fingerprint_repo="$TMPDIR/fingerprint-repo"
mkdir -p "$fingerprint_repo/crates/oasis7_viewer"
printf '.shell { grid-template-columns: minmax(0, 1fr); }\n' > "$fingerprint_repo/crates/oasis7_viewer/viewer_terminal_shell.css"
before_fingerprint="$(viewer_web_dist_source_metadata_json "$fingerprint_repo" | python3 -c 'import json,sys; print(json.load(sys.stdin)["sourceFingerprint"])')"
printf '\n.shell { overflow-wrap: anywhere; }\n' >> "$fingerprint_repo/crates/oasis7_viewer/viewer_terminal_shell.css"
after_fingerprint="$(viewer_web_dist_source_metadata_json "$fingerprint_repo" | python3 -c 'import json,sys; print(json.load(sys.stdin)["sourceFingerprint"])')"
if [[ "$before_fingerprint" == "$after_fingerprint" ]]; then
  echo "viewer dist freshness fingerprint must include viewer_terminal_shell.css" >&2
  failures=1
fi

if ((failures)); then
  exit 1
fi


# Protocol artifacts are mandatory and must never silently disappear from releases.
missing_protocol_root="$TMPDIR/missing-protocol-viewer"
cp -R "$viewer_root" "$missing_protocol_root"
rm "$missing_protocol_root/dist/browser-game-client/oasis7_browser_game_client_bg.wasm"
if "$ROOT_DIR/scripts/copy-viewer-web-dist.sh" --viewer-root "$missing_protocol_root" --dist-dir "$TMPDIR/missing-protocol-dist" --optional-payload-dir "$optional_payload_dir" > /dev/null 2>&1; then
  echo "missing browser Game Client protocol must fail delivery" >&2
  exit 1
fi

echo "copy-viewer-web-dist.test: OK"
