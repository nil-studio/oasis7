#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/release-candidate-bundle.sh create [options]
  ./scripts/release-candidate-bundle.sh validate --bundle <path> [options]

Purpose:
  Freeze and validate one machine-readable release candidate bundle so
  public_testnet_rehearsal/staging/canary promotion can reference the same truth.

Create options:
  --bundle <path>                    Output bundle json path (required)
  --candidate-id <id>                Candidate id (required)
  --track <name>                     Intended track: public_testnet_rehearsal|staging|canary
  --runtime-build-ref <path>         Runtime build artifact path/ref (required)
  --world-snapshot-ref <path>        World snapshot path/ref (required)
  --generated-world-sidecar-ref <path>
                                      Generated simulator map sidecar path/ref
  --world-generation-provenance-ref <path>
                                      Generated map provenance path/ref
  --governance-manifest-ref <path>   Governance manifest path/ref (required)
  --validator-pair-provenance-ref <path>
                                      Signed package/world/node provenance receipt
  --evidence-ref <path>              Evidence path/ref; repeatable
  --note <text>                      Free-form note; repeatable
  --allow-dirty-worktree             Allow create on dirty git worktree

Validate options:
  --bundle <path>                    Bundle json path (required)
  --check-git-head                   Require bundle git commit to equal current HEAD
  --check-clean-worktree             Require clean git worktree

Examples:
  ./scripts/release-candidate-bundle.sh create \
    --bundle output/release-candidates/rc.json \
    --candidate-id public-testnet-rehearsal-20260324-01 \
    --runtime-build-ref output/builds/oasis7_chain_runtime.tar.zst \
    --world-snapshot-ref output/worlds/reward-runtime-execution-world \
    --governance-manifest-ref /path/to/public_manifest.json \
    --evidence-ref doc/testing/evidence/governance-registry-live-world-drill-finality-2026-03-24.md

  ./scripts/release-candidate-bundle.sh validate \
    --bundle output/release-candidates/rc.json \
    --check-git-head \
    --check-clean-worktree
USAGE
}

mode=${1:-}
if [[ -z "$mode" ]]; then
  usage >&2
  exit 2
fi
shift || true

candidate_id=""
track="public_testnet_rehearsal"
bundle_path=""
runtime_build_ref=""
world_snapshot_ref=""
generated_world_sidecar_ref=""
world_generation_provenance_ref=""
governance_manifest_ref=""
validator_pair_provenance_ref=""
allow_dirty_worktree=0
check_git_head=0
check_clean_worktree=0
declare -a evidence_refs=()
declare -a notes=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle)
      bundle_path=${2:-}
      shift 2
      ;;
    --candidate-id)
      candidate_id=${2:-}
      shift 2
      ;;
    --track)
      track=${2:-}
      shift 2
      ;;
    --runtime-build-ref)
      runtime_build_ref=${2:-}
      shift 2
      ;;
    --world-snapshot-ref)
      world_snapshot_ref=${2:-}
      shift 2
      ;;
    --generated-world-sidecar-ref)
      generated_world_sidecar_ref=${2:-}
      shift 2
      ;;
    --world-generation-provenance-ref)
      world_generation_provenance_ref=${2:-}
      shift 2
      ;;
    --governance-manifest-ref)
      governance_manifest_ref=${2:-}
      shift 2
      ;;
    --validator-pair-provenance-ref)
      validator_pair_provenance_ref=${2:-}
      shift 2
      ;;
    --evidence-ref)
      evidence_refs+=("${2:-}")
      shift 2
      ;;
    --note)
      notes+=("${2:-}")
      shift 2
      ;;
    --allow-dirty-worktree)
      allow_dirty_worktree=1
      shift
      ;;
    --check-git-head)
      check_git_head=1
      shift
      ;;
    --check-clean-worktree)
      check_clean_worktree=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_non_empty() {
  local flag=$1
  local value=$2
  if [[ -z "$value" ]]; then
    echo "error: missing required option: $flag" >&2
    exit 2
  fi
}

require_supported_track() {
  case "$track" in
    public_testnet_rehearsal|staging|canary)
      ;;
    shared_devnet|shared_network)
      echo "error: unsupported --track: $track" >&2
      echo "hint: use --track public_testnet_rehearsal for rehearsal promotion gates" >&2
      exit 2
      ;;
    *)
      echo "error: unsupported --track: $track" >&2
      echo "supported tracks: public_testnet_rehearsal, staging, canary" >&2
      exit 2
      ;;
  esac
}

ensure_existing_path() {
  local flag=$1
  local value=$2
  if [[ ! -e "$value" ]]; then
    echo "error: $flag not found: $value" >&2
    exit 2
  fi
}

ensure_clean_worktree() {
  if [[ -n "$(git status --short)" ]]; then
    echo "error: git worktree is dirty; candidate bundle requires pinned source truth" >&2
    echo "hint: commit/stash changes or pass --allow-dirty-worktree for local-only rehearsal" >&2
    exit 1
  fi
}

path_metadata_json() {
  local path_value=$1
  python3 - "$repo_root" "$path_value" <<'PY'
import hashlib
import json
import pathlib
import sys

repo_root = pathlib.Path(sys.argv[1]).resolve()
target = pathlib.Path(sys.argv[2]).expanduser().resolve()

if not target.exists():
    raise SystemExit(f"missing path: {target}")

def file_sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def display_path(path: pathlib.Path) -> str:
    try:
        return path.relative_to(repo_root).as_posix()
    except ValueError:
        return str(path)

meta = {
    "ref": display_path(target),
    "resolved_path": str(target),
}

if target.is_file():
    stat = target.stat()
    meta.update(
        {
            "kind": "file",
            "sha256": file_sha256(target),
            "size_bytes": stat.st_size,
        }
    )
elif target.is_dir():
    combined = hashlib.sha256()
    file_count = 0
    total_bytes = 0
    for child in sorted(p for p in target.rglob("*") if p.is_file()):
        rel = child.relative_to(target).as_posix()
        digest = file_sha256(child)
        stat = child.stat()
        combined.update(rel.encode("utf-8"))
        combined.update(b"\0")
        combined.update(digest.encode("ascii"))
        combined.update(b"\0")
        combined.update(str(stat.st_size).encode("ascii"))
        combined.update(b"\n")
        file_count += 1
        total_bytes += stat.st_size
    meta.update(
        {
            "kind": "directory",
            "sha256_tree": combined.hexdigest(),
            "file_count": file_count,
            "total_bytes": total_bytes,
        }
    )
else:
    meta.update({"kind": "other"})

print(json.dumps(meta, ensure_ascii=True))
PY
}

validate_bundle_python() {
  local bundle=$1
  local expected_head=""
  local require_clean="$2"
  local require_head="$3"

  if [[ "$require_head" -eq 1 ]]; then
    expected_head=$(git rev-parse HEAD)
  fi

  python3 - "$bundle" "$expected_head" "$require_clean" "$repo_root" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile

bundle_path = pathlib.Path(sys.argv[1]).resolve()
expected_head = sys.argv[2]
require_clean = sys.argv[3] == "1"
repo_root = pathlib.Path(sys.argv[4]).resolve()

with bundle_path.open("r", encoding="utf-8") as fh:
    bundle = json.load(fh)

required_top = [
    "schema_version",
    "candidate_id",
    "track",
    "git_commit",
    "git_worktree_dirty",
    "runtime_build",
    "world_snapshot",
    "governance_manifest",
    "evidence_refs",
]
for field in required_top:
    if field not in bundle:
        raise SystemExit(f"bundle missing required field: {field}")

if bundle["schema_version"] != "oasis7.release_candidate_bundle.v1":
    raise SystemExit(
        f"unsupported schema_version: {bundle['schema_version']}"
    )

supported_tracks = {"public_testnet_rehearsal", "staging", "canary"}
if bundle["track"] not in supported_tracks:
    if bundle["track"] in {"shared_devnet", "shared_network"}:
        raise SystemExit(
            "unsupported track: "
            f"{bundle['track']}; use public_testnet_rehearsal for rehearsal promotion gates"
        )
    raise SystemExit(
        f"unsupported track: {bundle['track']}; supported tracks: "
        + ", ".join(sorted(supported_tracks))
    )

if expected_head and bundle["git_commit"] != expected_head:
    raise SystemExit(
        f"git commit drift: bundle={bundle['git_commit']} current={expected_head}"
    )

if require_clean and bundle.get("git_worktree_dirty"):
    raise SystemExit("bundle captured dirty worktree but clean worktree is required")

# The recorded absolute path is provenance, not a portable locator. Rebase
# only an unambiguous repo-local identity from a different capture checkout.
# External artifacts and already-staged host paths keep their existing contract.
capture_root = bundle.get("repo_root")
capture_root = pathlib.Path(capture_root) if isinstance(capture_root, str) else None
if capture_root is not None and not capture_root.is_absolute():
    raise SystemExit("bundle repo_root must be absolute")
relocated = capture_root is not None and capture_root != repo_root


def bundle_ref_path(label: str, payload: dict) -> tuple[pathlib.Path, bool]:
    raw_ref = payload["ref"]
    raw_path = payload["resolved_path"]
    if not isinstance(raw_ref, str) or not raw_ref or not isinstance(raw_path, str) or not raw_path:
        raise SystemExit(f"{label} has invalid ref or resolved_path")
    if "\x00" in raw_ref or "\x00" in raw_path:
        raise SystemExit(f"{label} has invalid ref or resolved_path")
    recorded_path = pathlib.Path(raw_path)
    if not relocated:
        return recorded_path, False
    ref = pathlib.PurePosixPath(raw_ref)
    if ref.is_absolute() or not recorded_path.is_relative_to(capture_root):
        # Explicitly selected external or already-staged path, never guessed.
        return recorded_path, False
    if (
        "\\" in raw_ref or ":" in raw_ref or not ref.parts
        or any(part in ("", ".", "..") for part in raw_ref.split("/"))
        or capture_root / ref != recorded_path
    ):
        raise SystemExit(f"{label} does not identify the recorded repository path")
    current_path = (repo_root / ref).resolve()
    if not current_path.is_relative_to(repo_root):
        raise SystemExit(f"{label} repository path escapes current checkout")
    return current_path, True


def file_sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def verify_pair_provenance_signature(provenance_path: pathlib.Path, provenance: dict) -> None:
    signature = provenance.get("signature")
    if not isinstance(signature, dict) or signature.get("status") != "verified":
        raise SystemExit("validator_pair_provenance requires a verified signature")
    if signature.get("algorithm") != "openssl-rsa-sha256":
        raise SystemExit("validator_pair_provenance signature algorithm is unsupported")
    if not isinstance(signature.get("signer_id"), str) or not signature["signer_id"].strip():
        raise SystemExit("validator_pair_provenance signature.signer_id is required")
    def signature_file(field: str) -> pathlib.Path:
        raw = signature.get(field)
        if not isinstance(raw, str) or not raw.strip():
            raise SystemExit(f"validator_pair_provenance signature.{field} is required")
        path = pathlib.Path(raw)
        if not path.is_absolute():
            path = provenance_path.parent / path
        elif not path.exists():
            staged_fallback = provenance_path.parent / path.name
            if staged_fallback.exists():
                path = staged_fallback
        if path.is_symlink() or not path.is_file():
            raise SystemExit(f"validator_pair_provenance signature.{field} missing or symlinked: {path}")
        return path.resolve()
    signature_path = signature_file("signature_ref")
    public_key_path = signature_file("public_key_ref")
    public_key_sha = signature.get("public_key_sha256")
    if not isinstance(public_key_sha, str) or len(public_key_sha) != 64 or public_key_sha.lower() != file_sha256(public_key_path):
        raise SystemExit("validator_pair_provenance public key hash mismatch")
    body = {key: value for key, value in provenance.items() if key != "binding_digest"}
    payload = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")
    with tempfile.TemporaryDirectory(prefix="oasis7-release-provenance-") as temp_dir:
        payload_path = pathlib.Path(temp_dir) / "payload"
        payload_path.write_bytes(payload)
        try:
            result = subprocess.run(
                ["openssl", "dgst", "-sha256", "-verify", str(public_key_path), "-signature", str(signature_path), str(payload_path)],
                check=False,
                capture_output=True,
                timeout=10,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired) as error:
            raise SystemExit(f"validator_pair_provenance detached verifier unavailable: {error.__class__.__name__}")
    if result.returncode != 0:
        raise SystemExit("validator_pair_provenance detached signature verification failed")

def check_ref(label: str, payload: dict) -> pathlib.Path:
    required = ["ref", "resolved_path", "kind"]
    for field in required:
        if field not in payload:
            raise SystemExit(f"{label} missing required field: {field}")
    path, rebased = bundle_ref_path(label, payload)
    if not path.exists():
        raise SystemExit(f"{label} path missing: {path}")
    if payload["kind"] == "file":
        recorded = payload.get("sha256")
        if not recorded:
          raise SystemExit(f"{label} missing sha256")
        if not path.is_file():
            raise SystemExit(f"{label} is not a file: {path}")
        current = file_sha256(path)
        if current != recorded:
            raise SystemExit(
                f"{label} drift detected: bundle={recorded} current={current}"
            )
    elif payload["kind"] == "directory":
        recorded = payload.get("sha256_tree")
        if not recorded:
            raise SystemExit(f"{label} missing sha256_tree")
        if not path.is_dir():
            raise SystemExit(f"{label} is not a directory: {path}")
        combined = hashlib.sha256()
        file_count = 0
        total_bytes = 0
        for child in sorted(path.rglob("*")):
            if rebased and child.is_symlink():
                raise SystemExit(f"{label} relocated tree contains a symbolic link: {child}")
            if not child.is_file():
                continue
            rel = child.relative_to(path).as_posix()
            digest = file_sha256(child)
            stat = child.stat()
            combined.update(rel.encode("utf-8"))
            combined.update(b"\0")
            combined.update(digest.encode("ascii"))
            combined.update(b"\0")
            combined.update(str(stat.st_size).encode("ascii"))
            combined.update(b"\n")
            file_count += 1
            total_bytes += stat.st_size
        current = combined.hexdigest()
        if current != recorded:
            raise SystemExit(
                f"{label} drift detected: bundle={recorded} current={current}"
            )
        if "file_count" in payload and payload["file_count"] != file_count:
            raise SystemExit(
                f"{label} file_count drift: bundle={payload['file_count']} current={file_count}"
            )
        if "total_bytes" in payload and payload["total_bytes"] != total_bytes:
            raise SystemExit(
                f"{label} total_bytes drift: bundle={payload['total_bytes']} current={total_bytes}"
            )
    else:
        raise SystemExit(f"{label} unsupported kind: {payload['kind']}")
    return path

for label in (
    "runtime_build",
    "world_snapshot",
    "governance_manifest",
):
    check_ref(label, bundle[label])

if ("generated_world_sidecar" in bundle) != ("world_generation_provenance" in bundle):
    raise SystemExit(
        "bundle generated sidecar metadata must include both generated_world_sidecar and world_generation_provenance"
    )
for label in ("generated_world_sidecar", "world_generation_provenance"):
    if label in bundle:
        check_ref(label, bundle[label])

pair_provenance = bundle.get("validator_pair_provenance")
if pair_provenance is not None:
    provenance_path = check_ref("validator_pair_provenance", pair_provenance)
    try:
        provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"validator_pair_provenance unreadable: {error}")
    if provenance.get("schema_version") != "oasis7.validator_pair_rebuild_provenance.v1":
        raise SystemExit("validator_pair_provenance schema mismatch")
    verify_pair_provenance_signature(provenance_path, provenance)
    binding = provenance.get("binding_digest")
    body = {key: value for key, value in provenance.items() if key != "binding_digest"}
    actual_binding = hashlib.sha256(
        json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    if binding != actual_binding:
        raise SystemExit("validator_pair_provenance binding_digest mismatch")

for index, item in enumerate(bundle.get("evidence_refs", []), start=1):
    if "ref" not in item or "resolved_path" not in item:
        raise SystemExit(f"evidence_refs[{index}] missing ref or resolved_path")
    label = f"evidence_refs[{index}]"
    path, rebased = bundle_ref_path(label, item)
    if rebased:
        check_ref(label, item)
    elif not path.exists():
        raise SystemExit(f"{label} path missing: {path}")

print(
    json.dumps(
        {
            "candidate_id": bundle["candidate_id"],
            "track": bundle["track"],
            "git_commit": bundle["git_commit"],
            "git_worktree_dirty": bundle["git_worktree_dirty"],
            "bundle_path": str(bundle_path),
            "validation": "ok",
        },
        ensure_ascii=True,
    )
)
PY
}

case "$mode" in
  create)
    require_non_empty "--bundle" "$bundle_path"
    require_non_empty "--candidate-id" "$candidate_id"
    require_supported_track
    require_non_empty "--runtime-build-ref" "$runtime_build_ref"
    require_non_empty "--world-snapshot-ref" "$world_snapshot_ref"
    require_non_empty "--governance-manifest-ref" "$governance_manifest_ref"
    ensure_existing_path "--runtime-build-ref" "$runtime_build_ref"
    ensure_existing_path "--world-snapshot-ref" "$world_snapshot_ref"
    if [[ -n "$generated_world_sidecar_ref" || -n "$world_generation_provenance_ref" ]]; then
      require_non_empty "--generated-world-sidecar-ref" "$generated_world_sidecar_ref"
      require_non_empty "--world-generation-provenance-ref" "$world_generation_provenance_ref"
      ensure_existing_path "--generated-world-sidecar-ref" "$generated_world_sidecar_ref"
      ensure_existing_path "--world-generation-provenance-ref" "$world_generation_provenance_ref"
    fi
    ensure_existing_path "--governance-manifest-ref" "$governance_manifest_ref"

    if [[ "$allow_dirty_worktree" -eq 0 ]]; then
      ensure_clean_worktree
    fi

    mkdir -p "$(dirname "$bundle_path")"
    git_commit=$(git rev-parse HEAD)
    git_worktree_dirty=false
    if [[ -n "$(git status --short)" ]]; then
      git_worktree_dirty=true
    fi

    runtime_build_meta=$(path_metadata_json "$runtime_build_ref")
    world_snapshot_meta=$(path_metadata_json "$world_snapshot_ref")
    generated_world_sidecar_meta="null"
    world_generation_provenance_meta="null"
    if [[ -n "$generated_world_sidecar_ref" ]]; then
      generated_world_sidecar_meta=$(path_metadata_json "$generated_world_sidecar_ref")
    fi
    if [[ -n "$world_generation_provenance_ref" ]]; then
      world_generation_provenance_meta=$(path_metadata_json "$world_generation_provenance_ref")
    fi
    governance_manifest_meta=$(path_metadata_json "$governance_manifest_ref")
    validator_pair_provenance_meta="null"
    if [[ -n "$validator_pair_provenance_ref" ]]; then
      ensure_existing_path "--validator-pair-provenance-ref" "$validator_pair_provenance_ref"
      validator_pair_provenance_meta=$(path_metadata_json "$validator_pair_provenance_ref")
    fi

    evidence_json="[]"
    if [[ "${#evidence_refs[@]}" -gt 0 ]]; then
      args=()
      for item in "${evidence_refs[@]}"; do
        ensure_existing_path "--evidence-ref" "$item"
        args+=("$(path_metadata_json "$item")")
      done
      evidence_json=$(printf '%s\n' "${args[@]}" | jq -s '.')
    fi

    notes_json="[]"
    if [[ "${#notes[@]}" -gt 0 ]]; then
      notes_json=$(printf '%s\n' "${notes[@]}" | jq -R . | jq -s '.')
    fi

    jq -n \
      --arg schema_version "oasis7.release_candidate_bundle.v1" \
      --arg candidate_id "$candidate_id" \
      --arg track "$track" \
      --arg git_commit "$git_commit" \
      --argjson git_worktree_dirty "$git_worktree_dirty" \
      --arg created_at "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
      --arg repo_root "$repo_root" \
      --argjson runtime_build "$runtime_build_meta" \
      --argjson world_snapshot "$world_snapshot_meta" \
      --argjson generated_world_sidecar "$generated_world_sidecar_meta" \
      --argjson world_generation_provenance "$world_generation_provenance_meta" \
      --argjson governance_manifest "$governance_manifest_meta" \
      --argjson validator_pair_provenance "$validator_pair_provenance_meta" \
      --argjson evidence_refs "$evidence_json" \
      --argjson notes "$notes_json" \
      '{
        schema_version: $schema_version,
        candidate_id: $candidate_id,
        track: $track,
        created_at: $created_at,
        repo_root: $repo_root,
        git_commit: $git_commit,
        git_worktree_dirty: $git_worktree_dirty,
        runtime_build: $runtime_build,
        world_snapshot: $world_snapshot,
        generated_world_sidecar: $generated_world_sidecar,
        world_generation_provenance: $world_generation_provenance,
        governance_manifest: $governance_manifest,
        validator_pair_provenance: $validator_pair_provenance,
        evidence_refs: $evidence_refs,
        notes: $notes
      } | with_entries(select(.value != null))' >"$bundle_path"
    echo "release candidate bundle: $bundle_path"
    ;;
  validate)
    require_non_empty "--bundle" "$bundle_path"
    ensure_existing_path "--bundle" "$bundle_path"
    if [[ "$check_clean_worktree" -eq 1 ]]; then
      ensure_clean_worktree
    fi
    validation_json=$(validate_bundle_python "$bundle_path" "$check_clean_worktree" "$check_git_head")
    echo "$validation_json"
    ;;
  *)
    echo "error: unsupported mode: $mode" >&2
    usage >&2
    exit 2
    ;;
esac
