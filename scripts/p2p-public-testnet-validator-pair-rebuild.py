#!/usr/bin/env python3
"""Governed, fail-closed validator-pair rebuild transaction executor.

The executor is deliberately local-first.  A plan contains immutable package,
world and capacity evidence; when configured with executor-bound
``human_direct_ssh`` inputs, it also performs bounded live GitHub authority and
SSH state observations before admission.  ``apply`` and ``rollback`` operate
on explicitly supplied local node roots and perform the corresponding bounded
live GitHub/SSH authority and state re-observations when configured; ``apply``
additionally requires a governed host adapter that returns a transaction-bound
startup/health receipt.  An orchestration layer may map the same transaction
to a remote host, but this tool never guesses credentials or silently falls
back to SSH.  No observer is ever touched by this pair transaction.
``quiesce`` is an evidence-only phase: it may invoke only a quiesce-only
adapter callback and never preflights, resets, stages, starts, or mutates a
node.  The separate ``human_direct_ssh`` mode is also evidence-only: it
invokes executor-owned fixed readbacks through pinned SSH after separately
authorized human stop evidence, with no generic adapter fallback.  Its
executor-bound two-role proof is mandatory for plan admission; the impact
record boolean is not a stopped-state proof.
"""

from __future__ import annotations

import argparse
import base64
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
import os
import re
import shutil
import shlex
import stat
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any, NoReturn


PLAN_SCHEMA = "oasis7.validator_pair_rebuild_plan.v1"
SCHEMA = "oasis7.validator_pair_rebuild_transaction.v1"
QUIESCENCE_REQUEST_SCHEMA = "oasis7.validator_pair_rebuild_quiescence_request.v1"
QUIESCENCE_PROOF_SCHEMA = "oasis7.validator_pair_rebuild_quiescence_proof.v1"
QUIESCENCE_MAX_AGE_SECONDS = 300
MUTATION_ORDER = ["storage-205", "sequencer-204"]
STARTUP_ORDER = ["sequencer-204", "storage-205"]
IDENTITY_METADATA_FIELDS = (
    "key_path",
    "key_sha256",
    "key_size_bytes",
    "key_mode",
    "key_uid",
    "key_gid",
)
IDENTITY_EXPECTED_REQUIRED_FIELDS = ("key_mode", "key_uid", "key_gid")
EXPECTED_LISTENERS = {
    "storage-205": {"6632", "6832"},
    "sequencer-204": {"6631", "6831"},
}
RESET_SURFACES = [
    "data/execution-records",
    "data/execution-world",
    "data/execution-world-simulator-mirror",
    "data/storage",
    "data/runtime-root",
    "data/replication-root",
    "output/chain-runtime",
    "output/node-distfs",
]
REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
REPOSITORY_EXECUTABLE_RELATIVE = "scripts/p2p-public-testnet-validator-pair-rebuild.py"
REPOSITORY_EXECUTABLE = REPOSITORY_ROOT / REPOSITORY_EXECUTABLE_RELATIVE
REPOSITORY_EXECUTABLE_SCHEMA = "oasis7.validator_pair_rebuild_repository_executable.v1"

# This is a repository-owned description of the only production validator
# pair which this executor may observe.  It is deliberately separate from the
# operator's SSH config and contains public host-key fingerprints only; no
# private key, token, or credential path is stored here.
DEPLOYMENT_INVENTORY_RELATIVE = "scripts/public-testnet-validator-pair-inventory.v1.json"
DEPLOYMENT_INVENTORY = REPOSITORY_ROOT / DEPLOYMENT_INVENTORY_RELATIVE
DEPLOYMENT_INVENTORY_SCHEMA = "oasis7.public_testnet_validator_pair_inventory.v1"
PRODUCTION_STACK_ROOT = "/opt/oasis7/p2p-testnet"
SERVICE_READBACK_COMMAND = f"{PRODUCTION_STACK_ROOT}/current/bin/service-readback --read-only"
PRODUCTION_KNOWN_HOSTS_PATH = "/opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-known-hosts"
# Kept separate from the inventory assertion so hermetic unit tests can
# replace the executor module's canonical path without changing production
# inventory truth.  No CLI or environment input can override this value.
EXECUTOR_KNOWN_HOSTS_PATH = Path(PRODUCTION_KNOWN_HOSTS_PATH)
NONCE_LEDGER_DEFAULT = "/var/lib/oasis7/p2p-public-testnet/validator-pair-nonces.jsonl"
NONCE_LEDGER_ENV = "OASIS7_VALIDATOR_PAIR_NONCE_LEDGER"

# ``human_direct_ssh`` is intentionally a separate provider from the legacy
# callback adapter.  These values are the complete remote contract; callers
# may bind a host and a human stop receipt, but may not choose a service,
# command, root, or remote status endpoint.
HUMAN_DIRECT_SSH_SCHEMA = "oasis7.human_direct_ssh_receipt.v1"
HUMAN_DIRECT_SSH_REQUEST_SCHEMA = "oasis7.human_direct_ssh_request.v1"
HUMAN_DIRECT_SSH_MAX_AGE_SECONDS = 24 * 60 * 60
HUMAN_DIRECT_SSH_QUIET_WINDOW_SECONDS = 2.0
AUTHORIZED_GITHUB_AUTHOR_ASSOCIATIONS = frozenset({"OWNER", "MEMBER", "COLLABORATOR"})
HUMAN_DIRECT_SSH_CANONICAL = {
    "sequencer-204": {
        "request_role": "sequencer",
        "node_id": "triad-testnet-sequencer",
        "host": "root@39.104.204.172",
        "root": PRODUCTION_STACK_ROOT,
        "service": "oasis7-triad-sequencer.service",
        "ports": {"6631", "6831"},
        "host_key_fingerprint": "SHA256:7NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
    },
    "storage-205": {
        "request_role": "storage",
        "node_id": "triad-testnet-storage",
        "host": "root@39.104.205.67",
        "root": PRODUCTION_STACK_ROOT,
        "service": "oasis7-triad-storage.service",
        "ports": {"6632", "6832"},
        "host_key_fingerprint": "SHA256:1SVgiaT5JLCw8PsPpVfLE9UyWNf82IJDZsiE7LAa1gI",
    },
}

ADAPTER_CALLBACK_SCHEMA = "oasis7.validator_pair_rebuild_adapter_callback.v1"
ADAPTER_CALLBACK_TARGETS = {
    "preflight": "preflight_receipt",
    "backup": "backup_receipt",
    "apply": "host_receipt",
    "rollback": "rollback_receipt",
}


def reset_surface_digest() -> str:
    """Return the digest of the one canonical destructive target set."""
    return hashlib.sha256(
        json.dumps(RESET_SURFACES, ensure_ascii=True, separators=(",", ":")).encode()
    ).hexdigest()


def backup_non_seed_binding() -> dict[str, Any]:
    """Describe the forensic-only backup boundary copied into receipts."""
    return {
        "forensic_only": True,
        "seed_eligible": False,
        "restore_deleted_chain_state": False,
    }


def fail(message: str) -> NoReturn:
    raise SystemExit(f"error: validator-pair rebuild: {message}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def repository_executable_identity() -> dict[str, str]:
    """Identify the repository-owned executor without claiming host attestation."""
    if REPOSITORY_EXECUTABLE.is_symlink() or not REPOSITORY_EXECUTABLE.is_file():
        fail("repository-owned validator-pair executor must be a regular file")
    return {
        "schema_version": REPOSITORY_EXECUTABLE_SCHEMA,
        "path": REPOSITORY_EXECUTABLE_RELATIVE,
        "sha256": sha256_file(REPOSITORY_EXECUTABLE),
    }


def repository_head_oid() -> str:
    """Resolve the checkout head which live authority must authorize."""
    try:
        result = subprocess.run(
            ["git", "-C", str(REPOSITORY_ROOT), "rev-parse", "--verify", "HEAD^{commit}"],
            check=False,
            text=True,
            capture_output=True,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        fail(f"capability_blocked: cannot resolve executor frozen head: {error.__class__.__name__}")
    head_oid = result.stdout.strip().lower()
    if result.returncode != 0 or not re.fullmatch(r"[0-9a-f]{40,64}", head_oid):
        fail("capability_blocked: executor frozen head is unavailable")
    return head_oid


def validate_repository_executable_identity(value: Any, label: str) -> dict[str, str]:
    expected = repository_executable_identity()
    if value != expected:
        fail(f"{label} provenance is not the repository-owned executor")
    return expected


def _load_deployment_inventory() -> dict[str, Any]:
    """Load the immutable production pair inventory from the repository.

    The inventory and its pinned known-hosts path are intentionally not CLI
    inputs.  A caller may not replace the target, role, root, service, port,
    known-hosts file, or expected public pin.
    """
    if DEPLOYMENT_INVENTORY.is_symlink() or not DEPLOYMENT_INVENTORY.is_file():
        fail("repository deployment inventory must be a regular file")
    inventory = load_json(DEPLOYMENT_INVENTORY, "repository deployment inventory")
    if inventory.get("schema_version") != DEPLOYMENT_INVENTORY_SCHEMA:
        fail("repository deployment inventory schema is unsupported")
    if inventory.get("repository") != "eng-cc/oasis7" or inventory.get("network_tier") != "public_testnet":
        fail("repository deployment inventory is not the governed public_testnet inventory")
    if inventory.get("stack_root") != PRODUCTION_STACK_ROOT:
        fail("repository deployment inventory stack root is not production")
    if inventory.get("known_hosts_path") != PRODUCTION_KNOWN_HOSTS_PATH:
        fail("repository deployment inventory known-hosts path is not canonical")
    nodes = inventory.get("nodes")
    if not isinstance(nodes, dict) or set(nodes) != set(MUTATION_ORDER):
        fail("repository deployment inventory must contain both validator roles")
    for role in MUTATION_ORDER:
        item = nodes[role]
        expected = HUMAN_DIRECT_SSH_CANONICAL[role]
        if not isinstance(item, dict):
            fail(f"repository deployment inventory entry is malformed for {role}")
        for field in ("role", "node_id", "host", "root", "service", "host_key_fingerprint"):
            if field not in item:
                fail(f"repository deployment inventory is missing {role}/{field}")
        if item.get("role") != expected["request_role"] or item.get("node_id") != expected["node_id"]:
            fail(f"repository deployment inventory role identity mismatch for {role}")
        if item.get("host") != expected["host"] or item.get("root") != expected["root"] or item.get("service") != expected["service"]:
            fail(f"repository deployment inventory target binding mismatch for {role}")
        if item.get("host_key_fingerprint") != expected["host_key_fingerprint"]:
            fail(f"repository deployment inventory public pin mismatch for {role}")
        ports = item.get("ports")
        if not isinstance(ports, list) or {str(value) for value in ports} != expected["ports"]:
            fail(f"repository deployment inventory listener binding mismatch for {role}")
        if not isinstance(item.get("host_key_fingerprint"), str) or not re.fullmatch(
            r"SHA256:[A-Za-z0-9+/]+", item["host_key_fingerprint"]
        ):
            fail(f"repository deployment inventory public pin is malformed for {role}")
    return inventory


def _nonce_ledger_path(requested_path: Any = None) -> Path:
    """Resolve an externally provisioned, executor-owned nonce ledger.

    The file is never created by this executor.  Provisioning it (and its
    parent) is an operator/deployment responsibility, so absence or weak
    permissions are capability blockers rather than reasons to fall back to
    an output-directory marker.
    """
    configured = os.environ.get(NONCE_LEDGER_ENV)
    if configured is None:
        # A request must always name the ledger which the executor was
        # provisioned to use.  The default is retained only as the production
        # deployment value; it is not an implicit per-output fallback.
        configured = NONCE_LEDGER_DEFAULT
    if not isinstance(requested_path, str) or not requested_path.strip():
        fail("capability_blocked: request is missing the executor-owned nonce ledger binding")
    raw = requested_path
    configured_path = Path(configured).expanduser()
    if not configured_path.is_absolute() or configured_path.resolve() != Path(raw).expanduser().resolve():
        fail("capability_blocked: request nonce ledger is not the executor-owned ledger")
    path = Path(raw).expanduser()
    if not path.is_absolute() or path.is_symlink() or not path.is_file():
        fail("capability_blocked: external executor-owned nonce ledger is unavailable")
    try:
        resolved = path.resolve(strict=True)
        metadata = resolved.stat()
    except OSError:
        fail("capability_blocked: external executor-owned nonce ledger is unavailable")
    if resolved.is_symlink() or metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != 0o600:
        fail("capability_blocked: external executor-owned nonce ledger ownership or mode is invalid")
    # Never accept a ledger under the repository or an output directory.  A
    # caller-created sibling file is not an external replay barrier.
    try:
        resolved.relative_to(REPOSITORY_ROOT)
    except ValueError:
        return resolved
    fail("capability_blocked: nonce ledger must be external to the repository")


def _reserve_nonce(
    *,
    nonce: str,
    task_uid: str,
    transaction_id: str,
    request_digest: str,
    authority_sha256: str,
    requested_path: Any,
    allow_existing: bool = False,
) -> Path:
    """Atomically reserve a stop nonce before any remote observation."""
    ledger = _nonce_ledger_path(requested_path)
    try:
        with ledger.open("r+", encoding="utf-8") as handle:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
            handle.seek(0)
            for line_number, line in enumerate(handle, 1):
                if not line.strip():
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    fail(f"capability_blocked: nonce ledger line {line_number} is malformed")
                if not isinstance(row, dict) or row.get("schema_version") != "oasis7.validator_pair_rebuild_nonce.v1":
                    fail(f"capability_blocked: nonce ledger line {line_number} is unsupported")
                if row.get("nonce") == nonce:
                    # Plan/apply intentionally re-observe the same
                    # transaction-bound request.  The standalone direct
                    # command remains one-shot: only its explicitly marked
                    # in-process re-observation may reuse an exact binding.
                    if allow_existing and all(
                        row.get(field) == value
                        for field, value in (
                            ("task_uid", task_uid),
                            ("transaction_id", transaction_id),
                            ("request_digest", request_digest),
                            ("authority_sha256", authority_sha256),
                        )
                    ):
                        fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
                        return ledger
                    fail("human_direct_ssh nonce has already been consumed")
            record = {
                "schema_version": "oasis7.validator_pair_rebuild_nonce.v1",
                "nonce": nonce,
                "task_uid": task_uid,
                "transaction_id": transaction_id,
                "request_digest": request_digest,
                "authority_sha256": authority_sha256,
                "reserved_at": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
            }
            handle.seek(0, os.SEEK_END)
            handle.write(json.dumps(record, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
            _fsync_parent_directory(ledger.parent)
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    except OSError as error:
        fail(f"capability_blocked: cannot reserve external nonce ledger: {error.__class__.__name__}")
    return ledger


def canonical_digest(value: dict[str, Any]) -> str:
    body = {key: item for key, item in value.items() if key != "canonical_digest"}
    return hashlib.sha256(
        json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def load_provenance_helper():
    helper_path = Path(__file__).with_name("p2p-public-testnet-validator-pair-provenance.py")
    spec = importlib.util.spec_from_file_location("oasis7_pair_provenance", helper_path)
    if spec is None or spec.loader is None:
        fail("cannot load provenance validator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {label}: {error}")
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def path_kind(path: Path) -> str:
    if path.is_symlink():
        return "symlink"
    if path.is_dir():
        return "directory"
    if path.is_file():
        return "file"
    return "missing"


def inventory_tree(path: Path) -> dict[str, Any]:
    """Hash a tree without following symlinks and count every inode class."""
    digest = hashlib.sha256()
    total = 0
    # Count the directory root as well as its children: snapshot manifests
    # and copytree both account for that inode.
    entry_count = 1
    link_count = 0
    dir_count = 1
    file_count = 0
    if path.is_symlink():
        target = os.readlink(path)
        digest.update(f"L\0{target}\n".encode())
        return {
            "sha256": digest.hexdigest(),
            "total_bytes": 0,
            "entry_count": 1,
            "link_count": 1,
            "dir_count": 0,
            "file_count": 0,
        }
    if path.is_file():
        return {
            "sha256": sha256_file(path),
            "total_bytes": path.stat().st_size,
            "entry_count": 1,
            "link_count": 0,
            "dir_count": 0,
            "file_count": 1,
        }
    if not path.exists():
        return {
            "sha256": hashlib.sha256(b"<missing>").hexdigest(),
            "total_bytes": 0,
            "entry_count": 0,
            "link_count": 0,
            "dir_count": 0,
            "file_count": 0,
        }
    if not path.is_dir():
        fail(f"unsupported path entry: {path} ({stat.filemode(path.lstat().st_mode)})")
    for child in sorted(path.rglob("*"), key=lambda item: item.relative_to(path).as_posix()):
        rel = child.relative_to(path).as_posix()
        if child.is_symlink():
            target = os.readlink(child)
            digest.update(rel.encode("utf-8"))
            digest.update(b"\0L\0")
            digest.update(target.encode("utf-8"))
            digest.update(b"\n")
            entry_count += 1
            link_count += 1
        elif child.is_dir():
            # Directory entries consume inodes even when empty.  Do not add
            # them to the digest so governed-world file digests stay compatible.
            entry_count += 1
            dir_count += 1
        elif child.is_file():
            child_sha = sha256_file(child)
            size = child.stat().st_size
            # Keep the directory digest byte-for-byte compatible with the
            # provenance helper's governed world tree digest.
            digest.update(rel.encode("utf-8"))
            digest.update(b"\0")
            digest.update(child_sha.encode("ascii"))
            digest.update(b"\0")
            digest.update(str(size).encode("ascii"))
            digest.update(b"\n")
            total += size
            entry_count += 1
            file_count += 1
        else:
            fail(f"unsupported path entry: {child} ({stat.filemode(child.lstat().st_mode)})")
    return {
        "sha256": digest.hexdigest(),
        "total_bytes": total,
        "entry_count": entry_count,
        "link_count": link_count,
        "dir_count": dir_count,
        "file_count": file_count,
    }


def hash_tree(path: Path) -> tuple[str, int, int]:
    """Compatibility tuple for callers that need digest, bytes, inode count."""
    inventory = inventory_tree(path)
    return inventory["sha256"], inventory["total_bytes"], inventory["entry_count"]


INVENTORY_FIELDS = ("entry_count", "link_count", "dir_count", "file_count", "total_bytes")


def inventory_summary(inventory: dict[str, Any], label: str) -> dict[str, int]:
    """Return the complete count/byte contract without the implementation hash."""
    if not isinstance(inventory, dict):
        fail(f"{label} inventory must be an object")
    result: dict[str, int] = {}
    for field in INVENTORY_FIELDS:
        try:
            raw_value = inventory[field] if field != "total_bytes" else inventory.get("total_bytes", inventory.get("size_bytes"))
            value = int(raw_value)
        except (KeyError, TypeError, ValueError):
            fail(f"{label} inventory field is malformed: {field}")
        if value < 0:
            fail(f"{label} inventory field must be non-negative: {field}")
        result[field] = value
    if result["entry_count"] != result["link_count"] + result["dir_count"] + result["file_count"]:
        fail(f"{label} inventory entry count does not equal its inode classes")
    return result


def tree_size(path: Path) -> int:
    return hash_tree(path)[1]


def manifest_tree(root: Path) -> list[dict[str, Any]]:
    """Return complete metadata for a root, including empty directories."""
    if not root.exists():
        return []
    entries: list[dict[str, Any]] = []
    for item in sorted([root, *root.rglob("*")], key=lambda value: value.relative_to(root.parent).as_posix()):
        rel = item.relative_to(root).as_posix() if item != root else "."
        info: dict[str, Any] = {"path": rel, "kind": path_kind(item)}
        try:
            item_stat = item.lstat()
            info.update(
                {
                    "mode": stat.S_IMODE(item_stat.st_mode),
                    "uid": item_stat.st_uid,
                    "gid": item_stat.st_gid,
                }
            )
            if item.is_symlink():
                info["target"] = os.readlink(item)
            elif item.is_file():
                info["size_bytes"] = item_stat.st_size
                info["sha256"] = sha256_file(item)
        except OSError as error:
            fail(f"cannot inventory {item}: {error}")
        entries.append(info)
    return entries


def copy_entry(source: Path, destination: Path) -> None:
    if source.is_symlink():
        _mkdir_durable(destination.parent)
        destination.symlink_to(os.readlink(source))
        _fsync_parent_directory(destination.parent)
    elif source.is_dir():
        _mkdir_durable(destination)
        for child in sorted(source.iterdir(), key=lambda item: item.name):
            copy_entry(child, destination / child.name)
        shutil.copystat(source, destination, follow_symlinks=False)
        _fsync_directory(destination)
        _fsync_parent_directory(destination.parent)
    elif source.is_file():
        _mkdir_durable(destination.parent)
        shutil.copy2(source, destination, follow_symlinks=False)
        _fsync_regular_file(destination)
    else:
        raise shutil.Error(f"cannot copy unsupported path: {source}")


def without_backup_entries(entries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        entry
        for entry in entries
        if entry["path"] == "." or (entry["path"] != "backups" and not entry["path"].startswith("backups/"))
    ]


def apply_manifest_metadata(root: Path, entries: list[dict[str, Any]]) -> None:
    """Restore mode/owner metadata without ever reading key contents."""
    for entry in sorted(entries, key=lambda item: item["path"].count("/"), reverse=True):
        path = root if entry["path"] == "." else root / entry["path"]
        if not path.exists() and not path.is_symlink():
            fail(f"backup metadata path missing: {path}")
        try:
            if path.is_symlink():
                os.lchown(path, int(entry["uid"]), int(entry["gid"]))
            else:
                os.chown(path, int(entry["uid"]), int(entry["gid"]), follow_symlinks=False)
                os.chmod(path, int(entry["mode"]), follow_symlinks=False)
        except OSError as error:
            fail(f"cannot restore backup metadata for {path}: {error}")
        _fsync_entry(path)


def remove_entry(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink()
        _fsync_parent_directory(path.parent)
    elif path.is_dir():
        shutil.rmtree(path)
        _fsync_parent_directory(path.parent)


def parse_node(raw: str) -> tuple[str, str, Path]:
    role, separator, target = raw.partition("=")
    if separator != "=" or role not in MUTATION_ORDER:
        fail(f"node must be role=local:/absolute/path: {raw}")
    scheme, separator, path = target.partition(":")
    if scheme != "local" or separator != ":" or not path:
        fail(f"only explicit local node roots are supported: {raw}")
    unresolved_root = Path(path).expanduser()
    if unresolved_root.is_symlink():
        fail(f"node root must not be a symlink: {unresolved_root}")
    root = unresolved_root.resolve()
    if not root.is_dir():
        fail(f"node root does not exist: {root}")
    return role, "local", root


def validate_current_link(root: Path, role: str) -> None:
    """Allow only the canonical root/current -> releases/<version> link."""
    current = root / "current"
    if not current.is_symlink():
        return
    target = os.readlink(current)
    if os.path.isabs(target):
        fail(f"{role} current symlink must be relative and canonical")
    resolved = (current.parent / target).resolve()
    releases_path = root / "releases"
    if releases_path.is_symlink() or not releases_path.is_dir():
        fail(f"{role} releases root must be a real directory")
    releases = releases_path.resolve()
    try:
        resolved.relative_to(releases)
    except ValueError:
        fail(f"{role} current symlink escapes releases: {current} -> {target}")
    if not resolved.is_dir():
        fail(f"{role} current symlink target is not a release directory: {resolved}")


def validate_node_symlinks(root: Path, role: str) -> None:
    validate_current_link(root, role)
    for item in root.rglob("*"):
        if item.is_symlink() and item.relative_to(root).as_posix() != "current":
            fail(f"{role} node root contains an unauthorized symlink: {item}")


def parse_nodes(raw_nodes: list[str]) -> dict[str, dict[str, Any]]:
    nodes: dict[str, dict[str, Any]] = {}
    for raw in raw_nodes:
        role, transport, root = parse_node(raw)
        if role in nodes:
            fail(f"duplicate node role: {role}")
        nodes[role] = {"role": role, "transport": transport, "root": str(root)}
    if set(nodes) != set(MUTATION_ORDER):
        fail(f"both node roles are required: {', '.join(MUTATION_ORDER)}")
    return {role: nodes[role] for role in MUTATION_ORDER}


def package_size(package_dir: Path) -> int:
    if not package_dir.is_dir():
        fail(f"package directory does not exist: {package_dir}")
    return tree_size(package_dir)


def validate_impact(path: Path, *, require_stopped: bool = False) -> dict[str, Any]:
    impact = load_json(path, "consumer impact record")
    if impact.get("decision") != "proceed":
        fail("consumer impact decision must be proceed")
    for field in (
        "impact",
        "evidence_source",
        "timestamp",
        "validators_already_stopped",
        "outage_update_channel",
        "recovery_update_checkpoint",
        "producer_wording_approval",
    ):
        if field not in impact:
            fail(f"consumer impact record missing {field}")
    if impact.get("impact") not in {"active", "none", "unknown"}:
        fail("consumer impact must be active, none, or unknown")
    if not isinstance(impact.get("evidence_source"), str) or not impact["evidence_source"].strip():
        fail("consumer impact evidence_source must be non-empty")
    _parse_timestamp(impact["timestamp"], "consumer impact timestamp")
    if not isinstance(impact["validators_already_stopped"], bool):
        fail("consumer impact validators_already_stopped must be a boolean")
    if require_stopped and impact.get("validators_already_stopped") is not True:
        fail("validators must be stopped before a pair rebuild plan")
    for field in ("outage_update_channel", "recovery_update_checkpoint", "producer_wording_approval"):
        value = impact.get(field)
        if not isinstance(value, str) or not value.strip():
            fail(f"consumer impact {field} must be non-empty")
        if impact["impact"] in {"active", "unknown"} and value.strip().lower() == "n/a":
            fail(f"consumer impact {field} cannot be n/a for active/unknown impact")
    return {
        "decision": "proceed",
        "impact": impact["impact"],
        "evidence_source": impact["evidence_source"],
        "timestamp": impact["timestamp"],
        "validators_already_stopped": impact["validators_already_stopped"],
        "outage_update_channel": impact["outage_update_channel"],
        "recovery_update_checkpoint": impact["recovery_update_checkpoint"],
        "producer_wording_approval": impact["producer_wording_approval"],
    }


def _payload_digest(value: dict[str, Any], excluded: tuple[str, ...] = ()) -> str:
    return hashlib.sha256(
        json.dumps(
            {key: item for key, item in value.items() if key not in excluded},
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()


def _fresh_quiescence_timestamp(value: Any, label: str) -> dt.datetime:
    parsed = _parse_timestamp(value, label)
    now = dt.datetime.now(dt.timezone.utc)
    if parsed > now + dt.timedelta(seconds=5) or now - parsed > dt.timedelta(seconds=QUIESCENCE_MAX_AGE_SECONDS):
        fail(f"{label} is stale or in the future")
    return parsed


def _validate_quiescence_adapter_receipt(receipt: dict[str, Any], request: dict[str, Any]) -> dict[str, Any]:
    if receipt.get("schema_version") not in {
        "oasis7.validator_pair_rebuild_quiescence_receipt.v1",
        "oasis7.validator_pair_rebuild_host_receipt.v2",
    }:
        fail("quiescence adapter returned an unsupported receipt schema")
    if receipt.get("phase") != "quiesce":
        fail("quiescence adapter receipt phase must be quiesce")
    if receipt.get("operation") != "quiesce-only":
        fail("quiescence adapter must declare a quiesce-only operation")
    if receipt.get("request_digest") != request["request_digest"]:
        fail("quiescence adapter request digest mismatch")
    receipt_transaction_id = receipt.get("transaction_id")
    if not isinstance(receipt_transaction_id, str) or not receipt_transaction_id.strip():
        fail("quiescence adapter transaction_id is required")
    if receipt.get("quiescence_id") != request["quiescence_id"] or (
        receipt_transaction_id != request["transaction_id"]
    ):
        fail("quiescence adapter quiescence identity mismatch")
    if receipt.get("impact_record_sha256") != request["impact_record_sha256"]:
        fail("quiescence adapter impact record mismatch")
    for forbidden in ("preflight", "reset", "stage", "apply", "systemd", "destructive_activity"):
        if receipt.get(forbidden) not in (None, False, "forbidden"):
            fail(f"quiescence adapter receipt contains forbidden {forbidden} activity")
    if receipt.get("observer_mutation") is not False:
        fail("quiescence adapter observer mutation is forbidden")
    if receipt.get("mutation_order") != MUTATION_ORDER or receipt.get("startup_order") != STARTUP_ORDER:
        fail("quiescence adapter receipt order mismatch")
    _fresh_quiescence_timestamp(receipt.get("captured_at"), "quiescence adapter captured_at")
    nodes = receipt.get("nodes")
    if not isinstance(nodes, dict) or set(nodes) != set(MUTATION_ORDER):
        fail("quiescence adapter receipt must cover both validator roles")
    request_nodes = request["nodes"]
    for role in MUTATION_ORDER:
        value = nodes[role]
        expected = request_nodes[role]
        if not isinstance(value, dict):
            fail(f"quiescence adapter receipt malformed for {role}")
        if value.get("role") != role or value.get("root") != expected["root"]:
            fail(f"quiescence adapter role/root binding mismatch for {role}")
        if value.get("active") is not False or value.get("running") is not False:
            fail(f"quiescence adapter stopped gate failed for {role}")
        if value.get("service_state") != "stopped":
            fail(f"quiescence adapter service state is not stopped for {role}")
        if value.get("independently_observed") is not True:
            fail(f"quiescence adapter independent observation is missing for {role}")
    return receipt


def _quiescence_request(
    impact_path: Path,
    impact: dict[str, Any],
    nodes: dict[str, dict[str, Any]],
    quiescence_id: str,
    transaction_id: str | None = None,
) -> dict[str, Any]:
    if transaction_id is None:
        transaction_id = quiescence_id
    if not isinstance(transaction_id, str) or not transaction_id.strip():
        fail("quiescence request transaction_id is required")
    request: dict[str, Any] = {
        "schema_version": QUIESCENCE_REQUEST_SCHEMA,
        "phase": "quiesce",
        "quiescence_id": quiescence_id,
        "transaction_id": transaction_id,
        "impact_record_sha256": sha256_file(impact_path),
        "consumer_impact": impact,
        "mutation_order": MUTATION_ORDER,
        "startup_order": STARTUP_ORDER,
        "nodes": {
            role: {"role": role, "transport": nodes[role]["transport"], "root": nodes[role]["root"]}
            for role in MUTATION_ORDER
        },
    }
    request["request_digest"] = _payload_digest(request)
    return request


def _quiescence_request_for_plan(plan: dict[str, Any]) -> dict[str, Any]:
    proof = plan.get("proof")
    nodes = plan.get("nodes")
    if not isinstance(proof, dict) or not isinstance(nodes, dict):
        fail("quiescence request requires the planned proof and node bindings")
    impact_path_value = proof.get("consumer_impact_record_path")
    quiescence_id = proof.get("quiescence_id")
    transaction_id = plan.get("transaction_id")
    if not isinstance(impact_path_value, str) or not impact_path_value.strip():
        fail("quiescence request impact record path is missing")
    if not isinstance(quiescence_id, str) or not quiescence_id.strip():
        fail("quiescence request quiescence_id is missing")
    if not isinstance(transaction_id, str) or not transaction_id.strip():
        fail("quiescence request transaction_id is missing")
    impact_path = Path(impact_path_value).resolve()
    impact = validate_impact(impact_path)
    if plan.get("consumer_impact") != impact:
        fail("quiescence request consumer-impact binding mismatch")
    return _quiescence_request(impact_path, impact, nodes, quiescence_id, transaction_id)


def _direct_regular_file(path_value: Any, label: str) -> Path:
    if not isinstance(path_value, str) or not path_value.strip():
        fail(f"{label} path is required")
    raw_path = Path(path_value).expanduser()
    if raw_path.is_symlink() or not raw_path.is_file():
        fail(f"{label} must be a regular file")
    return raw_path.resolve()


def _direct_canonical_known_hosts(path_value: Any, label: str = "human_direct_ssh known_hosts") -> Path:
    """Require the executor-owned production pin file and its safe ownership."""
    known_hosts = _direct_regular_file(path_value, label)
    canonical = EXECUTOR_KNOWN_HOSTS_PATH.expanduser().resolve()
    if known_hosts != canonical:
        fail("human_direct_ssh known_hosts must use the executor-owned canonical production path")
    try:
        metadata = known_hosts.stat()
    except OSError as error:
        fail(f"human_direct_ssh known_hosts metadata cannot be read: {error.__class__.__name__}")
    if metadata.st_uid != os.getuid():
        fail("human_direct_ssh known_hosts must be owned by the executor")
    if stat.S_IMODE(metadata.st_mode) != 0o600:
        fail("human_direct_ssh known_hosts must have mode 0600")
    return known_hosts


def _direct_safe_identifier(value: Any, label: str, *, minimum: int = 8) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]{%d,127}" % (minimum - 1), value):
        fail(f"{label} is malformed")
    return value


def _direct_fresh_timestamp(value: Any, label: str) -> dt.datetime:
    parsed = _parse_timestamp(value, label)
    now = dt.datetime.now(dt.timezone.utc)
    # The human request is deliberately longer lived than a single SSH
    # command, but remains bounded.  This also leaves enough room for an
    # operator to collect both readbacks without weakening the fail-closed
    # stale-request gate.
    if parsed > now + dt.timedelta(seconds=5) or now - parsed > dt.timedelta(seconds=HUMAN_DIRECT_SSH_MAX_AGE_SECONDS):
        fail(f"{label} is stale or in the future")
    return parsed


def _direct_host_name(target: str) -> str:
    if not isinstance(target, str) or not re.fullmatch(r"root@[A-Za-z0-9][A-Za-z0-9_.-]*(?::[0-9]{1,5})?", target):
        fail("human_direct_ssh host must be a root@hostname target")
    return target.split("@", 1)[1].split(":", 1)[0]


def _direct_known_host_pin(known_hosts: Path, target: str, declared: Any, role: str) -> dict[str, str]:
    if not isinstance(declared, str) or not re.fullmatch(r"SHA256:[A-Za-z0-9+/]+", declared):
        fail(f"human_direct_ssh host fingerprint is malformed for {role}")
    hostname = _direct_host_name(target)
    target_host = target.split("@", 1)[1]
    expected_host_token = f"[{target_host}]" if ":" in target_host else hostname
    host_tokens = {expected_host_token}
    matching: list[tuple[str, str, str]] = []
    try:
        lines = known_hosts.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        fail(f"human_direct_ssh known_hosts cannot be read: {error.__class__.__name__}")
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = stripped.split()
        if len(fields) < 3 or fields[0].startswith("|"):
            continue
        host_fields = fields[0].split(",")
        if not host_tokens.intersection(host_fields):
            continue
        if host_fields != [expected_host_token]:
            fail(f"human_direct_ssh known_hosts entry must pin exactly {role}")
        matching.append((host_fields[0], fields[1], fields[2]))
    if not matching:
        fail(f"human_direct_ssh known_hosts has no pinned key for {role}")
    if len(matching) != 1:
        fail(f"human_direct_ssh known_hosts must contain exactly one non-conflicting entry for {role}")
    selected: list[tuple[str, str, str]] = []
    for _, key_type, key_blob in matching:
        if key_type not in {"ssh-ed25519", "ssh-rsa", "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521"}:
            fail(f"human_direct_ssh known_hosts key type is unsupported for {role}")
        try:
            # Only OpenSSH public key blobs are accepted.  A symbolic fixture
            # string must never become an authority or fingerprint fallback.
            key_bytes = base64.b64decode(key_blob.encode("ascii"), validate=True)
        except (ValueError, UnicodeEncodeError):
            fail(f"human_direct_ssh known_hosts public key is malformed for {role}")
        computed = "SHA256:" + base64.b64encode(hashlib.sha256(key_bytes).digest()).decode("ascii").rstrip("=")
        if computed == declared:
            selected.append((key_type, key_blob, computed))
    if len(selected) != 1:
        fail(f"human_direct_ssh known_hosts must contain exactly one expected public pin for {role}")
    key_type, key_blob, computed = selected[0]
    key_bytes = base64.b64decode(key_blob.encode("ascii"), validate=True)
    return {
        "declared": declared,
        "computed": computed,
        "hostname": hostname,
        "key_type": key_type,
        "key_sha256": hashlib.sha256(key_bytes).hexdigest(),
    }


def _validate_direct_inventory_request(
    request: dict[str, Any], inventory: dict[str, Any], known_hosts: Path
) -> dict[str, Any]:
    """Bind request transport fields to repository deployment truth."""
    inventory_digest = sha256_file(DEPLOYMENT_INVENTORY)
    if request.get("inventory_path") != str(DEPLOYMENT_INVENTORY):
        fail("human_direct_ssh deployment inventory path is not repository-owned")
    if request.get("inventory_sha256") != inventory_digest:
        fail("human_direct_ssh deployment inventory digest mismatch")
    declared_known_hosts = request.get("known_hosts_path")
    declared_known_hosts_path = Path(declared_known_hosts).expanduser() if isinstance(declared_known_hosts, str) else None
    if (
        declared_known_hosts_path is None
        or declared_known_hosts_path.is_symlink()
        or declared_known_hosts_path.resolve() != EXECUTOR_KNOWN_HOSTS_PATH.expanduser().resolve()
        or known_hosts.resolve() != EXECUTOR_KNOWN_HOSTS_PATH.expanduser().resolve()
    ):
        fail("human_direct_ssh known_hosts path is not the executor-owned canonical production path")
    hosts = request.get("hosts")
    if not isinstance(hosts, dict) or set(hosts) != {"sequencer", "storage"}:
        fail("human_direct_ssh request must contain exactly sequencer and storage hosts")
    inventory_nodes = inventory["nodes"]
    for request_role, canonical_role in (("storage", "storage-205"), ("sequencer", "sequencer-204")):
        value = hosts.get(request_role)
        expected = inventory_nodes[canonical_role]
        if not isinstance(value, dict):
            fail(f"human_direct_ssh {request_role} inventory binding is malformed")
        # These are descriptive copies in the request, never authority.  If a
        # caller changes even one, reject instead of silently normalizing it.
        for field in ("role", "host", "root", "service", "host_key_fingerprint"):
            if value.get(field) != expected[field]:
                fail(f"human_direct_ssh caller inventory override is forbidden for {canonical_role}/{field}")
    readback = request.get("readback")
    if not isinstance(readback, dict):
        fail("human_direct_ssh fixed readback contract is missing")
    if readback.get("process_command") != "ps -eo pid=,args=" or readback.get("listener_command") != "ss -ltn":
        fail("human_direct_ssh process/listener command allowlist mismatch")
    if readback.get("service_readback_command") != SERVICE_READBACK_COMMAND:
        fail("human_direct_ssh service command allowlist mismatch")
    try:
        quiet_window = float(readback.get("quiet_window_seconds", HUMAN_DIRECT_SSH_QUIET_WINDOW_SECONDS))
    except (TypeError, ValueError):
        fail("human_direct_ssh quiet window is malformed")
    if not 0.1 <= quiet_window <= 30:
        fail("human_direct_ssh quiet window is outside the fixed bound")
    return {"inventory_sha256": inventory_digest, "known_hosts": str(known_hosts.resolve()), "quiet_window": quiet_window}


def _github_response_items(value: Any) -> list[dict[str, Any]]:
    """Normalize the bounded ``gh api --paginate --slurp`` response."""
    if isinstance(value, dict):
        return [value]
    if not isinstance(value, list):
        fail("human_direct_ssh live GitHub response is malformed")
    items: list[dict[str, Any]] = []
    for item in value:
        if isinstance(item, list):
            if not all(isinstance(nested, dict) for nested in item):
                fail("human_direct_ssh live GitHub response is malformed")
            items.extend(item)
        elif isinstance(item, dict):
            items.append(item)
        else:
            fail("human_direct_ssh live GitHub response is malformed")
    return items


def _read_live_github_authority(
    request: dict[str, Any], task_uid: str, transaction_id: str, nonce: str
) -> dict[str, Any]:
    """Read and validate the authenticated control-plane comment in-process.

    The request contains only the expected comment binding.  It is not a
    signed authority and is never accepted by itself.  ``gh`` is the
    repository's authenticated GitHub CLI seam; this executor neither reads
    nor prints its credentials.  The command's output is deliberately kept
    in memory and only the non-secret binding fields are returned.
    """
    expected = request.get("github_live")
    if not isinstance(expected, dict):
        fail("capability_blocked: live GitHub authority binding is required")
    required = (
        "provider",
        "repository",
        "issue_number",
        "comment_id",
        "actor",
        "body_sha256",
        "task_uid",
        "action",
        "transaction_id",
        "nonce",
        "inventory_sha256",
        "head_oid",
        "issued_at",
        "expires_at",
    )
    if any(field not in expected for field in required):
        fail("human_direct_ssh live GitHub authority binding is incomplete")
    if (
        expected.get("provider") != "github"
        or expected.get("repository") != "eng-cc/oasis7"
        or expected.get("issue_number") != 3481
        or expected.get("task_uid") != task_uid
        or expected.get("action") != "validator_pair_rebuild"
        or expected.get("transaction_id") != transaction_id
        or expected.get("nonce") != nonce
    ):
        fail("human_direct_ssh live GitHub authority task/action binding mismatch")
    if not isinstance(expected.get("comment_id"), int) or expected["comment_id"] <= 0:
        fail("human_direct_ssh live GitHub comment binding is malformed")
    if not isinstance(expected.get("actor"), str) or not expected["actor"].strip():
        fail("human_direct_ssh live GitHub actor binding is malformed")
    if not isinstance(expected.get("body_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", expected["body_sha256"]):
        fail("human_direct_ssh live GitHub body digest binding is malformed")
    if not isinstance(expected.get("inventory_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", expected["inventory_sha256"]):
        fail("human_direct_ssh live GitHub inventory binding is malformed")
    if DEPLOYMENT_INVENTORY.is_symlink() or not DEPLOYMENT_INVENTORY.is_file():
        fail("human_direct_ssh repository inventory must be a regular file")
    repository_inventory_sha256 = sha256_file(DEPLOYMENT_INVENTORY)
    if request.get("inventory_sha256") != repository_inventory_sha256:
        fail("human_direct_ssh live GitHub request inventory is not repository-bound")
    request_inventory = request.get("inventory")
    if request_inventory is None:
        if expected["inventory_sha256"] != repository_inventory_sha256 or request.get("inventory_sha256") != repository_inventory_sha256:
            fail("human_direct_ssh live GitHub inventory digest is not the repository-owned inventory")
    else:
        # A live authority may carry the control-plane's canonical inventory
        # projection.  Bind its digest to the approval, but never use its
        # host-key fields as pins: host/root/service/port targets below still
        # come exclusively from the repository-owned inventory and the
        # verified OpenSSH public material.
        descriptor_digest = hashlib.sha256(
            json.dumps(request_inventory, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        if expected["inventory_sha256"] != descriptor_digest:
            fail("human_direct_ssh live GitHub inventory digest is not repository-bound")
        if request_inventory.get("schema_version") != "oasis7.public_testnet_validator_inventory.v1":
            fail("human_direct_ssh live GitHub inventory schema is unsupported")
        descriptor_nodes = request_inventory.get("nodes")
        if not isinstance(descriptor_nodes, list) or len(descriptor_nodes) != len(MUTATION_ORDER):
            fail("human_direct_ssh live GitHub inventory must cover both validator roles")
        by_node_id: dict[str, dict[str, Any]] = {}
        for descriptor_node in descriptor_nodes:
            if not isinstance(descriptor_node, dict) or not isinstance(descriptor_node.get("node_id"), str):
                fail("human_direct_ssh live GitHub inventory node is malformed")
            if descriptor_node["node_id"] in by_node_id:
                fail("human_direct_ssh live GitHub inventory contains duplicate nodes")
            by_node_id[descriptor_node["node_id"]] = descriptor_node
        for role in MUTATION_ORDER:
            expected_node = HUMAN_DIRECT_SSH_CANONICAL[role]
            descriptor_node = by_node_id.get(expected_node["node_id"])
            if descriptor_node is None:
                fail(f"human_direct_ssh live GitHub inventory is missing {role}")
            for field, expected_value in (
                ("role", role),
                ("host", expected_node["host"]),
                ("root", expected_node["root"]),
                ("service", expected_node["service"]),
            ):
                if descriptor_node.get(field) != expected_value:
                    fail(f"human_direct_ssh live GitHub inventory {role}/{field} override is forbidden")
            if {str(value) for value in descriptor_node.get("ports", [])} != expected_node["ports"]:
                fail(f"human_direct_ssh live GitHub inventory {role}/ports override is forbidden")
    if not isinstance(expected.get("head_oid"), str) or not re.fullmatch(r"[0-9a-f]{40}", expected["head_oid"].lower()):
        fail("human_direct_ssh live GitHub head binding is malformed")
    if expected["head_oid"].lower() != repository_head_oid():
        fail("human_direct_ssh live GitHub head binding does not match the executor frozen head")

    # ``--slurp`` gives a single JSON value for paginated responses.  The
    # fake in the hermetic contract intentionally emits one object, which is
    # also accepted to keep this seam small and deterministic.
    command = [
        "gh",
        "api",
        "repos/eng-cc/oasis7/issues/3481/comments",
        "--paginate",
        "--slurp",
    ]
    # Do not let the SSH credential seam become a GitHub or child-process
    # credential seam.  Authentication is supplied by the operator's
    # already-authenticated ``gh`` installation, not by this request.  Keep
    # only process/configuration variables needed by the CLI and hermetic
    # harness; this avoids reading or forwarding arbitrary ambient secrets.
    env = {
        name: value
        for name, value in os.environ.items()
        if name
        in {
            "PATH",
            "HOME",
            "GH_CONFIG_DIR",
            "GH_HOST",
            "XDG_CONFIG_HOME",
            "HUMAN_DIRECT_SSH_GITHUB_RESPONSE",
            "HUMAN_DIRECT_SSH_GITHUB_LOG",
            "HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE",
        }
    }
    try:
        result = subprocess.run(
            command,
            check=False,
            text=True,
            capture_output=True,
            timeout=30,
            env=env,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        fail(f"capability_blocked: live GitHub authority readback failed: {error.__class__.__name__}")
    if result.returncode != 0:
        fail("capability_blocked: live GitHub authority readback was unavailable")
    try:
        raw_value = json.loads(result.stdout)
    except json.JSONDecodeError:
        fail("capability_blocked: live GitHub authority response is not JSON")
    items = _github_response_items(raw_value)
    matches = [item for item in items if item.get("id") == expected["comment_id"]]
    if len(matches) != 1:
        fail("human_direct_ssh live GitHub authority comment was not uniquely read back")
    comment = matches[0]
    created_at = comment.get("created_at")
    updated_at = comment.get("updated_at")
    if (
        ("deleted" in comment and comment.get("deleted") is not False)
        or ("revoked" in comment and comment.get("revoked") is not False)
        or not isinstance(created_at, str)
        or not isinstance(updated_at, str)
        or updated_at != created_at
    ):
        fail("human_direct_ssh live GitHub authority comment is edited, deleted, or revoked")
    user = comment.get("user")
    if not isinstance(user, dict) or user.get("login") != expected["actor"]:
        fail("human_direct_ssh live GitHub authority actor binding mismatch")
    author_association = comment.get("author_association")
    if not isinstance(author_association, str) or author_association.upper() not in AUTHORIZED_GITHUB_AUTHOR_ASSOCIATIONS:
        fail("human_direct_ssh live GitHub authority actor association is not authorized")
    body = comment.get("body")
    if not isinstance(body, str) or hashlib.sha256(body.encode()).hexdigest() != expected["body_sha256"]:
        fail("human_direct_ssh live GitHub authority body digest mismatch")
    try:
        body_value = json.loads(body)
    except json.JSONDecodeError:
        fail("human_direct_ssh live GitHub authority body is not JSON")
    if not isinstance(body_value, dict) or body_value.get("schema_version") != "oasis7.validator_pair_rebuild_live_authority.v1":
        fail("human_direct_ssh live GitHub authority body schema is unsupported")
    if body_value.get("authorized") is not True:
        fail("human_direct_ssh live GitHub authority action is not explicitly authorized")
    for field in ("task_uid", "action", "transaction_id", "nonce", "inventory_sha256", "head_oid", "issued_at", "expires_at"):
        if body_value.get(field) != expected.get(field):
            fail(f"human_direct_ssh live GitHub authority {field} binding mismatch")
    issued_at = _direct_fresh_timestamp(body_value.get("issued_at"), "human_direct_ssh live GitHub issued_at")
    expires_at = _parse_timestamp(body_value.get("expires_at"), "human_direct_ssh live GitHub expires_at")
    if expires_at <= issued_at or expires_at < dt.datetime.now(dt.timezone.utc) - dt.timedelta(seconds=5):
        fail("human_direct_ssh live GitHub authority is expired")
    # This is a live control-plane binding, not a caller-invented inventory
    # authority.  Targets, roots, services and pins are still taken only from
    # the repository-owned deployment inventory below.
    return {
        "provider": "github",
        "repository": expected["repository"],
        "issue_number": expected["issue_number"],
        "comment_id": expected["comment_id"],
        "actor": expected["actor"],
        "author_association": author_association.upper(),
        "authorized": True,
        "body_sha256": expected["body_sha256"],
        "task_uid": task_uid,
        "action": expected["action"],
        "transaction_id": transaction_id,
        "nonce": nonce,
        "inventory_sha256": expected["inventory_sha256"],
        "head_oid": expected["head_oid"].lower(),
        "issued_at": expected["issued_at"],
        "expires_at": expected["expires_at"],
    }


def _direct_authority(request: dict[str, Any], request_path: Path) -> dict[str, Any]:
    if request.get("schema_version") != HUMAN_DIRECT_SSH_REQUEST_SCHEMA or request.get("mode") != "human_direct_ssh":
        fail("human_direct_ssh request schema or mode is unsupported")
    for key in request:
        if re.search(r"(?:password|secret|token|private[_-]?key(?:$|_))", key, re.IGNORECASE):
            fail("human_direct_ssh request cannot contain credentials")
    if request.get("observer_mutation") is not False:
        fail("human_direct_ssh observer mutation is forbidden")
    credential_binding = request.get("credential_binding")
    if credential_binding is not None:
        if not isinstance(credential_binding, dict) or credential_binding.get("kind") != "temporary-fd-or-environment":
            fail("human_direct_ssh credential seam must be a temporary fd or environment binding")
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", str(credential_binding.get("environment_name", ""))):
            fail("human_direct_ssh credential environment binding is malformed")
    if any(key in request for key in ("host_adapter", "adapter", "provider_command")):
        fail("human_direct_ssh does not accept arbitrary adapter input")
    if any(key in request for key in ("full_status_url", "full_status_http_status", "full_chain_status_called", "chain_status_url")) or "/v1/chain/status" in json.dumps(request, ensure_ascii=True):
        fail("human_direct_ssh forbids full /v1/chain/status, including 204")
    if request.get("replay_of_transaction_id") is not None:
        fail("human_direct_ssh transaction request is a replay")
    task_uid = _direct_safe_identifier(request.get("task_uid"), "human_direct_ssh task_uid")
    transaction_id = _direct_safe_identifier(request.get("transaction_id"), "human_direct_ssh transaction_id")
    nonce = _direct_safe_identifier(request.get("nonce"), "human_direct_ssh nonce")
    request_digest = request.get("request_digest")
    if not isinstance(request_digest, str) or not re.fullmatch(r"[0-9a-f]{64}", request_digest):
        fail("human_direct_ssh request digest is required")
    expected_digest = hashlib.sha256(
        json.dumps(
            {key: value for key, value in request.items() if key != "request_digest"},
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    if request_digest != expected_digest:
        fail("human_direct_ssh request digest binding mismatch")
    issued_at = _direct_fresh_timestamp(request.get("issued_at"), "human_direct_ssh issued_at")
    expires_at = _parse_timestamp(request.get("expires_at"), "human_direct_ssh expires_at")
    if expires_at <= issued_at:
        fail("human_direct_ssh request expiry is not after issuance")
    if expires_at < dt.datetime.now(dt.timezone.utc) - dt.timedelta(seconds=5):
        fail("human_direct_ssh request is expired")
    github_live = _read_live_github_authority(request, task_uid, transaction_id, nonce)
    if request_path.is_symlink():
        fail("human_direct_ssh request must be a regular file")
    return {
        "task_uid": task_uid,
        "transaction_id": transaction_id,
        "nonce": nonce,
        "request_digest": request_digest,
        "issued_at": request["issued_at"],
        "expires_at": request["expires_at"],
        "authority_path": None,
        "authority_sha256": github_live["body_sha256"],
        "authority_timestamp": github_live["issued_at"],
        "github_live": github_live,
    }


def _direct_ssh_environment(args: argparse.Namespace, request: dict[str, Any]) -> tuple[dict[str, str], bool]:
    env = os.environ.copy()
    # Do not allow ambient SSHPASS to silently become a credential input.
    env.pop("SSHPASS", None)
    binding = request.get("credential_binding")
    request_env = binding.get("environment_name") if isinstance(binding, dict) else None
    credential_env = args.credential_env or request_env
    credential_fd = args.credential_fd
    if credential_env and credential_fd is not None:
        fail("human_direct_ssh accepts one credential seam, not both")
    cached_secret = getattr(args, "_credential_secret", None)
    if cached_secret is not None:
        if not isinstance(cached_secret, str) or not cached_secret.strip():
            fail("human_direct_ssh cached credential is empty or malformed")
        env["SSHPASS"] = cached_secret
        return env, True
    if credential_env:
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", credential_env) or not env.get(credential_env):
            fail("human_direct_ssh credential environment binding is unavailable")
        secret = env[credential_env]
        args._credential_secret = secret
        env["SSHPASS"] = secret
        return env, True
    if credential_fd is not None:
        if credential_fd < 0:
            fail("human_direct_ssh credential fd is invalid")
        try:
            duplicate = os.dup(credential_fd)
            with os.fdopen(duplicate, "rb", closefd=True) as handle:
                secret = handle.read(4097)
        except OSError:
            fail("human_direct_ssh credential fd is unreadable")
        if len(secret) > 4096 or not secret.strip():
            fail("human_direct_ssh credential fd is empty or oversized")
        try:
            decoded = secret.decode("utf-8", errors="strict").strip()
        except UnicodeDecodeError:
            fail("human_direct_ssh credential fd is not UTF-8")
        args._credential_secret = decoded
        env["SSHPASS"] = decoded
        return env, True
    return env, False


def _direct_ssh_call(
    target: str,
    known_hosts: Path,
    command: str,
    env: dict[str, str],
    use_credential: bool,
) -> str:
    ssh_args = ["ssh", "-o", "StrictHostKeyChecking=yes", "-o", "GlobalKnownHostsFile=/dev/null"]
    # Preserve the operator-supplied spelling for auditability while also
    # pinning the kernel-resolved path when the platform exposes a /private
    # or equivalent filesystem alias.
    for known_hosts_value in dict.fromkeys((str(known_hosts), str(known_hosts.resolve()))):
        ssh_args.extend(("-o", f"UserKnownHostsFile={known_hosts_value}"))
    if use_credential:
        ssh_args.extend(("-o", "LogLevel=ERROR"))
    else:
        ssh_args.extend(("-o", "BatchMode=yes", "-o", "LogLevel=ERROR"))
    ssh_args.extend((target, command))
    invocation = ["sshpass", "-e", *ssh_args] if use_credential else ssh_args
    try:
        result = subprocess.run(
            invocation,
            check=False,
            text=True,
            capture_output=True,
            timeout=30,
            env=env,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        fail(f"human_direct_ssh read-only SSH failed: {error.__class__.__name__}")
    if result.returncode != 0:
        fail(f"human_direct_ssh read-only SSH failed with exit {result.returncode}")
    return result.stdout.strip()


def _direct_service_readback(
    role: str,
    target: str,
    root: str,
    service: str,
    known_hosts: Path,
    env: dict[str, str],
    use_credential: bool,
) -> dict[str, Any]:
    command = f"{SERVICE_READBACK_COMMAND} --role {role} --root {shlex.quote(root)} --service {shlex.quote(service)}"
    raw = _direct_ssh_call(target, known_hosts, command, env, use_credential)
    try:
        value = json.loads(raw)
    except json.JSONDecodeError:
        fail(f"human_direct_ssh service readback is not JSON for {role}")
    if not isinstance(value, dict):
        fail(f"human_direct_ssh service readback is malformed for {role}")
    if value.get("schema_version") not in {None, "oasis7.human_direct_ssh_readback.v1"}:
        fail(f"human_direct_ssh service readback schema is unsupported for {role}")
    if value.get("active") is not False or value.get("running") is not False:
        fail(f"human_direct_ssh active service detected for {role}")
    if value.get("service_state") != "stopped":
        fail(f"human_direct_ssh service is not stopped for {role}")
    if value.get("independently_observed") is not True:
        fail(f"human_direct_ssh independent service observation is missing for {role}")
    listeners = value.get("listeners", [])
    if not isinstance(listeners, list):
        fail(f"human_direct_ssh listener readback is malformed for {role}")
    return {"service_state": "stopped", "listeners": [str(item) for item in listeners]}


def _direct_observe_node(
    canonical_role: str,
    request_role: str,
    target: str,
    root: str,
    service: str,
    ports: set[str],
    known_hosts: Path,
    env: dict[str, str],
    use_credential: bool,
) -> dict[str, Any]:
    service_value = _direct_service_readback(
        request_role, target, root, service, known_hosts, env, use_credential
    )
    process_command = "ps -eo pid=,args="
    process_output = _direct_ssh_call(target, known_hosts, process_command, env, use_credential)
    if re.search(r"oasis7_chain_runtime|start-node\.sh|" + re.escape(root), process_output, re.IGNORECASE):
        fail(f"human_direct_ssh active process detected for {canonical_role}")
    listener_command = "ss -ltn"
    listener_output = _direct_ssh_call(target, known_hosts, listener_command, env, use_credential)
    observed_ports = set(re.findall(r":([0-9]{1,5})(?:\s|$)", listener_output))
    canonical_listeners = sorted(ports.intersection(observed_ports))
    if "LISTEN" in listener_output.upper() and canonical_listeners:
        fail(f"human_direct_ssh active listener detected for {canonical_role}")
    if set(service_value["listeners"]).intersection(ports):
        fail(f"human_direct_ssh service readback reports an active listener for {canonical_role}")
    return {
        "role": canonical_role,
        "root": root,
        "active": False,
        "running": False,
        "service_state": "stopped",
        "independently_observed": True,
        "listeners": [],
        "process_command": process_command,
        "listener_command": listener_command,
        "service_readback_command": f"{SERVICE_READBACK_COMMAND} --role {request_role} --root {shlex.quote(root)} --service {shlex.quote(service)}",
        "readback_sha256": hashlib.sha256(
            json.dumps(
                {
                    "service": service_value,
                    "stack_process_present": False,
                    "canonical_listener_ports": canonical_listeners,
                },
                ensure_ascii=True,
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest(),
    }


def run_human_direct_ssh(args: argparse.Namespace, *, emit: bool = True) -> dict[str, Any]:
    if getattr(args, "host_adapter", None):
        fail("human_direct_ssh generic adapter input is forbidden")
    # Test/operations harnesses may provide a side-channel audit file to
    # prove that no systemctl helper was even opened.  It is never a command
    # input and is created empty before any SSH readback begins.
    systemctl_audit = os.environ.get("HUMAN_DIRECT_SSH_SYSTEMCTL_LOG")
    if systemctl_audit:
        audit_path = Path(systemctl_audit)
        if audit_path.is_symlink():
            fail("human_direct_ssh systemctl audit path must be a regular file")
        try:
            audit_path.parent.mkdir(parents=True, exist_ok=True)
            audit_path.touch(exist_ok=True)
        except OSError:
            fail("human_direct_ssh systemctl audit path is unavailable")
    request_path = _direct_regular_file(args.request, "human_direct_ssh request")
    request = load_json(request_path, "human_direct_ssh request")
    authority = _direct_authority(request, request_path)
    inventory = _load_deployment_inventory()
    known_hosts = _direct_canonical_known_hosts(args.known_hosts)
    known_hosts_arg = known_hosts
    inventory_binding = _validate_direct_inventory_request(request, inventory, known_hosts)
    quiescence_id = request.get("quiescence_id", authority["transaction_id"])
    if not isinstance(quiescence_id, str) or quiescence_id != authority["transaction_id"]:
        fail("human_direct_ssh quiescence transaction binding mismatch")
    impact_path_value = request.get("impact_record_path")
    impact_path: Path | None = None
    impact: dict[str, Any] | None = None
    impact_sha256: str | None = None
    if impact_path_value is not None:
        impact_path = _direct_regular_file(impact_path_value, "human_direct_ssh consumer impact record")
        impact_sha256 = sha256_file(impact_path)
        if request.get("impact_record_sha256") != impact_sha256:
            fail("human_direct_ssh consumer impact record digest mismatch")
        impact = validate_impact(impact_path)
    out_raw = Path(args.out_dir).expanduser()
    if out_raw.is_symlink():
        fail("human_direct_ssh output directory must not be a symlink")
    out_dir = out_raw.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    for existing in ("receipt.json", "quiescence-proof.json", "quiescence-adapter-receipt.json", "human-direct-ssh-nonce.json"):
        if (out_dir / existing).exists() or (out_dir / existing).is_symlink():
            fail("human_direct_ssh transaction nonce has already been used")
    hosts = request.get("hosts")
    if not isinstance(hosts, dict) or set(hosts) != {"sequencer", "storage"}:
        fail("human_direct_ssh request must contain exactly sequencer and storage hosts")
    readback = request.get("readback")
    if not isinstance(readback, dict):
        fail("human_direct_ssh fixed readback contract is missing")
    if readback.get("process_command") != "ps -eo pid=,args=" or readback.get("listener_command") != "ss -ltn":
        fail("human_direct_ssh process/listener command allowlist mismatch")
    if readback.get("service_readback_command") != SERVICE_READBACK_COMMAND:
        fail("human_direct_ssh service command allowlist mismatch")
    try:
        quiet_window = float(readback.get("quiet_window_seconds", HUMAN_DIRECT_SSH_QUIET_WINDOW_SECONDS))
    except (TypeError, ValueError):
        fail("human_direct_ssh quiet window is malformed")
    if not 0.1 <= quiet_window <= 30:
        fail("human_direct_ssh quiet window is outside the fixed bound")
    env, use_credential = _direct_ssh_environment(args, request)
    # Reserve before the first SSH read.  A failed or interrupted observation
    # therefore cannot be replayed into a different output directory.
    nonce_ledger = _reserve_nonce(
        nonce=authority["nonce"],
        task_uid=authority["task_uid"],
        transaction_id=authority["transaction_id"],
        request_digest=authority["request_digest"],
        authority_sha256=authority["authority_sha256"],
        requested_path=request.get("nonce_ledger_path"),
        allow_existing=bool(getattr(args, "reobserve", False)),
    )
    host_bindings: dict[str, dict[str, str]] = {}
    for request_role, canonical_role in (("storage", "storage-205"), ("sequencer", "sequencer-204")):
        value = hosts.get(request_role)
        expected = inventory["nodes"][canonical_role]
        if not isinstance(value, dict) or value.get("role") != request_role:
            fail(f"human_direct_ssh role binding mismatch for {canonical_role}")
        if value.get("root") != expected["root"] or value.get("service") != expected["service"]:
            fail(f"human_direct_ssh root/service allowlist mismatch for {canonical_role}")
        target = value.get("host")
        hostname = _direct_host_name(target)
        if value.get("host_key_fingerprint") is None:
            fail(f"human_direct_ssh host fingerprint is required for {canonical_role}")
        pin = _direct_known_host_pin(known_hosts_arg, target, expected["host_key_fingerprint"], canonical_role)
        host_bindings[canonical_role] = {
            "role": canonical_role,
            "request_role": request_role,
            "host": target,
            "hostname": hostname,
            "root": expected["root"],
            "service": expected["service"],
            "host_key_fingerprint": pin["declared"],
            "known_host_key_sha256": pin["key_sha256"],
        }
    first: dict[str, dict[str, Any]] = {}
    for canonical_role in ("storage-205", "sequencer-204"):
        binding = host_bindings[canonical_role]
        first[canonical_role] = _direct_observe_node(
            canonical_role,
            binding["request_role"],
            binding["host"],
            binding["root"],
            binding["service"],
            HUMAN_DIRECT_SSH_CANONICAL[canonical_role]["ports"],
            known_hosts_arg,
            env,
            use_credential,
        )
    time.sleep(quiet_window)
    second: dict[str, dict[str, Any]] = {}
    for canonical_role in ("storage-205", "sequencer-204"):
        binding = host_bindings[canonical_role]
        second[canonical_role] = _direct_observe_node(
            canonical_role,
            binding["request_role"],
            binding["host"],
            binding["root"],
            binding["service"],
            HUMAN_DIRECT_SSH_CANONICAL[canonical_role]["ports"],
            known_hosts_arg,
            env,
            use_credential,
        )
    if first != second:
        fail("human_direct_ssh quiescence did not remain stable for the quiet window")
    repository_identity = repository_executable_identity()
    captured_at = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    credential_binding = request.get("credential_binding")
    bound_environment = args.credential_env
    if bound_environment is None and isinstance(credential_binding, dict):
        bound_environment = credential_binding.get("environment_name")
    receipt: dict[str, Any] = {
        "schema_version": HUMAN_DIRECT_SSH_SCHEMA,
        "phase": "quiesce",
        "mode": "human_direct_ssh",
        "provider": "executor-owned-direct-ssh",
        "operation": "quiesce-only",
        "task_uid": authority["task_uid"],
        "transaction_id": authority["transaction_id"],
        "nonce": authority["nonce"],
        "request_digest": authority["request_digest"],
        "quiescence_id": quiescence_id,
        "impact_record_path": str(impact_path_value) if impact_path_value is not None else None,
        "impact_record_sha256": impact_sha256,
        "impact": impact["impact"] if impact is not None else None,
        "issued_at": authority["issued_at"],
        "expires_at": authority["expires_at"],
        "captured_at": captured_at,
        "stop_authority_sha256": authority["authority_sha256"],
        "github_live": authority["github_live"],
        "direct_request_path": str(request_path),
        "direct_request_sha256": sha256_file(request_path),
        "credential_binding": {
            "kind": "temporary-fd-or-environment",
            "environment_name": bound_environment,
            "secret_in_argv": False,
            "secret_in_output": False,
            "secret_in_artifact": False,
        },
        "known_hosts_path": str(known_hosts_arg),
        "known_hosts_sha256": sha256_file(known_hosts_arg),
        "deployment_inventory_path": str(DEPLOYMENT_INVENTORY),
        "deployment_inventory_sha256": inventory_binding["inventory_sha256"],
        "nonce_ledger_path": str(nonce_ledger),
        "host_fingerprints": {
            role: value["host_key_fingerprint"] for role, value in host_bindings.items()
        },
        "hosts": host_bindings,
        "commands": {
            role: [
                value["service_readback_command"],
                value["process_command"],
                value["listener_command"],
            ]
            for role, value in first.items()
        },
        "quiet_window_seconds": quiet_window,
        "mutation_order": MUTATION_ORDER,
        "startup_order": STARTUP_ORDER,
        "nodes": first,
        "observer_mutation": False,
        "repository_executable": repository_identity,
        "audit_only": True,
    }
    receipt["receipt_path"] = str(out_dir / "receipt.json")
    proof_path = out_dir / "quiescence-proof.json"
    if impact_path is not None:
        receipt["proof_path"] = str(proof_path)
    receipt["canonical_digest"] = canonical_digest(receipt)
    write_json(out_dir / "receipt.json", receipt)
    source_path = out_dir / "quiescence-adapter-receipt.json"
    write_json(source_path, receipt)
    if impact_path is not None and impact is not None and impact_sha256 is not None:
        proof: dict[str, Any] = {
            "schema_version": QUIESCENCE_PROOF_SCHEMA,
            "phase": "quiesce",
            "authority": "human_direct_ssh",
            "provider": "executor-owned-direct-ssh",
            "operation": "quiesce-only",
            "task_uid": receipt["task_uid"],
            "quiescence_id": receipt["quiescence_id"],
            "transaction_id": receipt["transaction_id"],
            "nonce": receipt["nonce"],
            "impact": impact["impact"],
            "impact_record_path": str(impact_path_value),
            "impact_record_sha256": impact_sha256,
            "request_digest": receipt["request_digest"],
            "github_live": receipt["github_live"],
            "direct_request_path": receipt["direct_request_path"],
            "direct_request_sha256": receipt["direct_request_sha256"],
            "source_receipt_path": str(source_path),
            "source_receipt_sha256": sha256_file(source_path),
            "known_hosts_path": receipt["known_hosts_path"],
            "captured_at": receipt["captured_at"],
            "mutation_order": MUTATION_ORDER,
            "startup_order": STARTUP_ORDER,
            "roles": [
                {
                    "role": role,
                    "root": receipt["nodes"][role]["root"],
                    "active": receipt["nodes"][role]["active"],
                    "running": receipt["nodes"][role]["running"],
                    "service_state": receipt["nodes"][role]["service_state"],
                    "independently_observed": receipt["nodes"][role]["independently_observed"],
                    "evidence_sha256": _payload_digest(receipt["nodes"][role]),
                }
                for role in MUTATION_ORDER
            ],
            "nodes": receipt["nodes"],
            "observer_mutation": False,
            "repository_executable": repository_identity,
            "audit_only": True,
        }
        proof["canonical_digest"] = canonical_digest(proof)
        write_json(proof_path, proof)
    if emit:
        print(json.dumps(receipt, ensure_ascii=True, sort_keys=True))
    return receipt


def _direct_reobserve_from_args(args: argparse.Namespace) -> dict[str, Any]:
    """Perform a fresh direct readback for plan/apply without trusting proof files."""
    request_path = getattr(args, "human_direct_ssh_request", None)
    if not isinstance(request_path, str) or not request_path.strip():
        fail("plan/apply requires a live human_direct_ssh request for direct re-observation")
    known_hosts = getattr(args, "known_hosts", None)
    if not isinstance(known_hosts, str) or not known_hosts.strip():
        fail("plan/apply requires the canonical pinned known-hosts file for direct re-observation")
    # The direct provider writes only audit material to an isolated temporary
    # directory.  The returned in-memory receipt is the gate; no persisted
    # proof is read back into this process or used as authority.
    with tempfile.TemporaryDirectory(prefix="oasis7-direct-reobserve-") as audit_dir:
        direct_args = argparse.Namespace(
            request=request_path,
            known_hosts=known_hosts,
            out_dir=audit_dir,
            host_adapter=None,
            credential_env=getattr(args, "credential_env", None),
            credential_fd=getattr(args, "credential_fd", None),
            reobserve=True,
        )
        cached_secret = getattr(args, "_credential_secret", None)
        if cached_secret is not None:
            direct_args._credential_secret = cached_secret
        receipt = run_human_direct_ssh(direct_args, emit=False)
        if hasattr(direct_args, "_credential_secret"):
            args._credential_secret = direct_args._credential_secret
        return receipt


def _direct_observation_record(receipt: dict[str, Any], *, reauthorized: bool = False) -> dict[str, Any]:
    """Persist the minimum direct readback needed to prove a rollback gate."""
    return {
        "provider": "executor-owned-direct-ssh",
        "audit_only": True,
        "reauthorized": reauthorized,
        "quiescence_id": receipt["quiescence_id"],
        "request_digest": receipt["request_digest"],
        "nonce": receipt["nonce"],
        "captured_at": receipt["captured_at"],
        "deployment_inventory_sha256": receipt["deployment_inventory_sha256"],
        "nodes": receipt["nodes"],
    }


def _rollback_direct_reobserve(
    transaction: dict[str, Any], direct_args: argparse.Namespace | None = None
) -> dict[str, Any]:
    """Re-authorize and re-observe the stopped boundary before rollback."""
    proof = transaction.get("proof")
    if not isinstance(proof, dict) or proof.get("direct_reobserve_required") is not True:
        fail("rollback requires live human_direct_ssh re-observation; persisted proof is audit-only")
    request_value = getattr(direct_args, "human_direct_ssh_request", None) if direct_args is not None else None
    known_hosts_value = getattr(direct_args, "known_hosts", None) if direct_args is not None else None
    if not isinstance(request_value, str) or not request_value.strip():
        request_value = proof.get("direct_request_path")
    if not isinstance(known_hosts_value, str) or not known_hosts_value.strip():
        known_hosts_value = proof.get("known_hosts_path")
    if not isinstance(request_value, str) or not request_value.strip() or not isinstance(known_hosts_value, str) or not known_hosts_value.strip():
        fail("rollback requires live human_direct_ssh re-observation before restore")
    if direct_args is None:
        direct_args = argparse.Namespace(
            human_direct_ssh_request=request_value,
            known_hosts=known_hosts_value,
            credential_env=None,
            credential_fd=None,
        )
    else:
        direct_args.human_direct_ssh_request = request_value
        direct_args.known_hosts = known_hosts_value
    requested_path = _direct_regular_file(request_value, "rollback human_direct_ssh request")
    expected_path = Path(proof.get("direct_request_path", "")).expanduser().resolve()
    if requested_path != expected_path or sha256_file(requested_path) != proof.get("direct_request_sha256"):
        fail("rollback direct request is not the request bound into the transaction")
    direct_receipt = _direct_reobserve_from_args(direct_args)
    stopped_quiescence = proof.get("stopped_quiescence")
    expected_request_digest = stopped_quiescence.get("request_digest") if isinstance(stopped_quiescence, dict) else None
    if direct_receipt.get("quiescence_id") != proof.get("quiescence_id") or direct_receipt.get("request_digest") != expected_request_digest:
        fail("rollback direct re-observation binding differs from the transaction request")
    impact_path_value = proof.get("consumer_impact_record_path")
    if not isinstance(impact_path_value, str) or not impact_path_value.strip():
        fail("rollback consumer-impact binding is missing")
    impact_path = Path(impact_path_value).resolve()
    if direct_receipt.get("impact_record_sha256") != sha256_file(impact_path) or direct_receipt.get("impact") != transaction.get("consumer_impact", {}).get("impact"):
        fail("rollback direct re-observation consumer-impact binding mismatch")
    return direct_receipt


def _direct_args_from_audit_proof(args: argparse.Namespace, proof_path: Path) -> argparse.Namespace:
    """Recover only routing metadata from an audit proof, never authority.

    Older wrapper invocations provide ``--stopped-quiescence-proof`` rather
    than the direct request flags.  The proof can identify the request and
    pinned known-hosts file so this process can perform a fresh live GitHub
    readback and SSH observation.  Its stopped-state fields, digests and
    claimed authority are deliberately not consumed as a gate.
    """
    audit_path = _direct_regular_file(str(proof_path), "human_direct_ssh audit proof")
    audit = load_json(audit_path, "human_direct_ssh audit proof")
    if audit.get("audit_only") is not True or audit.get("canonical_digest") != canonical_digest(audit):
        fail("persisted human_direct_ssh proof is not a valid executor audit record; trusted producer attestation is required")
    request_value = audit.get("direct_request_path")
    known_hosts_value = audit.get("known_hosts_path")
    request = _direct_regular_file(request_value, "human_direct_ssh audit request")
    if audit.get("direct_request_sha256") != sha256_file(request):
        fail("human_direct_ssh audit request digest mismatch")
    known_hosts = _direct_canonical_known_hosts(known_hosts_value, "human_direct_ssh audit known_hosts")
    inherited_env = getattr(args, "credential_env", None)
    if inherited_env is None:
        binding = audit.get("credential_binding")
        if isinstance(binding, dict):
            inherited_env = binding.get("environment_name")
    return argparse.Namespace(
        request=str(request),
        known_hosts=str(known_hosts),
        credential_env=inherited_env,
        credential_fd=getattr(args, "credential_fd", None),
        human_direct_ssh_request=str(request),
    )


def run_quiescence_phase(args: argparse.Namespace) -> dict[str, Any]:
    # Caller-supplied JSON adapters are not an authority boundary.  Keep the
    # parser for an explicit migration error, but never execute one or emit a
    # proof from it; human_direct_ssh is the only production quiescence mode.
    fail("capability_blocked: generic quiesce host adapters have no trusted producer provenance or independent observation; use human_direct_ssh")
    impact_path = Path(args.consumer_impact_record).resolve()
    impact = validate_impact(impact_path)
    if impact["impact"] not in {"active", "none", "unknown"}:
        fail("quiescence phase requires active, none, or unknown consumer impact")
    if impact["impact"] == "none" and impact["validators_already_stopped"] is not True:
        fail("none-impact quiescence requires validators_already_stopped=true")
    quiescence_id = args.quiescence_id
    if not isinstance(quiescence_id, str) or not quiescence_id.strip():
        fail("quiescence phase requires a non-empty quiescence id")
    nodes = parse_nodes(args.node)
    adapter = Path(args.host_adapter).expanduser()
    if adapter.is_symlink() or not adapter.is_file():
        fail(f"quiescence host adapter must be a regular file: {adapter}")
    if not adapter.stat().st_mode & 0o111:
        fail("quiescence host adapter must be executable")
    out_dir = Path(args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    request = _quiescence_request(impact_path, impact, nodes, quiescence_id)
    request_path = out_dir / "quiescence-request.json"
    write_json(request_path, request)
    try:
        result = subprocess.run(
            [str(adapter.resolve()), "--phase", "quiesce", "--transaction", str(request_path)],
            check=False,
            text=True,
            capture_output=True,
            timeout=300,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        fail(f"quiescence host adapter execution failed: {error.__class__.__name__}")
    if result.returncode != 0:
        fail(f"quiescence host adapter failed with exit {result.returncode}")
    try:
        source = json.loads(result.stdout)
    except json.JSONDecodeError:
        fail("quiescence host adapter must return one JSON receipt on stdout")
    if not isinstance(source, dict):
        fail("quiescence host adapter receipt must be a JSON object")
    _validate_quiescence_adapter_receipt(source, request)
    repository_identity = repository_executable_identity()
    if "repository_executable" in source:
        validate_repository_executable_identity(
            source["repository_executable"], "quiescence host adapter receipt producer"
        )
    source["repository_executable"] = repository_identity
    source_path = out_dir / "quiescence-adapter-receipt.json"
    write_json(source_path, source)
    roles = []
    for role in MUTATION_ORDER:
        observed = source["nodes"][role]
        roles.append(
            {
                "role": role,
                "root": observed["root"],
                "active": observed["active"],
                "running": observed["running"],
                "service_state": observed["service_state"],
                "independently_observed": observed["independently_observed"],
                "evidence_sha256": _payload_digest(observed),
            }
        )
    proof: dict[str, Any] = {
        "schema_version": QUIESCENCE_PROOF_SCHEMA,
        "phase": "quiesce",
        "quiescence_id": quiescence_id,
        "transaction_id": quiescence_id,
        "impact": impact["impact"],
        "impact_record_sha256": request["impact_record_sha256"],
        "request_digest": request["request_digest"],
        "source_receipt_path": str(source_path),
        "source_receipt_sha256": sha256_file(source_path),
        "captured_at": source["captured_at"],
        "mutation_order": MUTATION_ORDER,
        "startup_order": STARTUP_ORDER,
        "roles": roles,
        "nodes": source["nodes"],
        "observer_mutation": False,
        "repository_executable": repository_identity,
    }
    proof_path = out_dir / "quiescence-proof.json"
    write_json(proof_path, proof)
    proof["proof_path"] = str(proof_path)
    print(json.dumps(proof, ensure_ascii=True, sort_keys=True))
    return proof


def capacity_for(
    capacity: dict[str, Any],
    role: str,
    required: int,
    required_inodes: int,
    inventory: dict[str, Any],
) -> dict[str, Any]:
    value = capacity.get(role)
    if not isinstance(value, dict):
        fail(f"capacity evidence missing {role}")
    try:
        free_bytes = int(value["free_bytes"])
        free_inodes = int(value["free_inodes"])
    except (KeyError, TypeError, ValueError):
        fail(f"capacity evidence malformed for {role}")
    if free_bytes < required:
        fail(f"capacity gate failed for {role}: {free_bytes} < {required} bytes")
    if free_inodes < required_inodes:
        fail(f"capacity gate failed for {role}: {free_inodes} < {required_inodes} free inodes")
    if value.get("same_filesystem") is not True:
        fail(f"same-filesystem backup gate failed for {role}")
    inventory_value = inventory_summary(inventory, f"{role} node")
    return {
        "free_bytes": free_bytes,
        "free_inodes": free_inodes,
        "same_filesystem": True,
        "required_bytes": required,
        "required_inodes": required_inodes,
        "inventory": inventory_value,
        "verified": True,
    }


def refresh_capacity(root: Path, planned: dict[str, Any], role: str) -> dict[str, Any]:
    try:
        usage = shutil.disk_usage(root)
        statvfs = os.statvfs(root)
        free_bytes = int(usage.free)
        free_inodes = int(statvfs.f_favail)
    except OSError as error:
        fail(f"cannot recapture apply-time capacity for {role}: {error}")
    required_bytes = int(planned["required_bytes"])
    required_inodes = int(planned["required_inodes"])
    actual_inventory = inventory_summary(inventory_tree(root), f"{role} apply-time node")
    planned_inventory = inventory_summary(planned.get("inventory"), f"{role} planned node")
    if actual_inventory != planned_inventory:
        fail(f"apply-time inventory changed for {role}: {actual_inventory} != {planned_inventory}")
    if free_bytes < required_bytes:
        fail(f"apply-time capacity gate failed for {role}: {free_bytes} < {required_bytes} bytes")
    if free_inodes < required_inodes:
        fail(f"apply-time capacity gate failed for {role}: {free_inodes} < {required_inodes} free inodes")
    return {
        "planned_free_bytes": int(planned["free_bytes"]),
        "planned_free_inodes": int(planned["free_inodes"]),
        "apply_free_bytes": free_bytes,
        "apply_free_inodes": free_inodes,
        "required_bytes": required_bytes,
        "required_inodes": required_inodes,
        "same_filesystem": planned.get("same_filesystem") is True,
        "inventory": actual_inventory,
        "verified": True,
    }


def reject_full_status(url: str | None, label: str) -> None:
    if url and "/v1/chain/status" in url:
        fail(f"204 {label} must use a bounded proof endpoint; full /v1/chain/status is forbidden")


def attestation_body(value: dict[str, Any]) -> bytes:
    body = {key: item for key, item in value.items() if key not in {"signature_ref", "public_key_ref"}}
    return json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()


def _require_object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def _require_exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    actual = set(value)
    if actual != expected:
        fail(f"{label} fields mismatch: missing={sorted(expected - actual)} extra={sorted(actual - expected)}")


def _require_hex(value: Any, length: int, label: str) -> str:
    if not isinstance(value, str) or len(value) != length or any(char not in "0123456789abcdefABCDEF" for char in value):
        fail(f"{label} must be {length}-hex")
    return value.lower()


def _require_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        fail(f"{label} must be non-empty")
    return value


def _runtime_claim_bytes(raw: dict[str, Any], proof: dict[str, Any]) -> bytes:
    _require_exact_keys(
        raw,
        {
            "schema_version",
            "observed_at_unix_ms",
            "ok",
            "liveness",
            "readiness",
            "heights",
            "network_head",
            "checkpoint",
            "local_peer_id",
            "connected_peers",
            "connected_peer_count",
            "proof",
        },
        "raw rebuild proof",
    )
    if raw.get("schema_version") != "oasis7.rebuild_status.v1":
        fail("raw rebuild proof schema is unsupported")
    if not isinstance(raw.get("observed_at_unix_ms"), int) or isinstance(raw["observed_at_unix_ms"], bool):
        fail("raw rebuild proof observed_at_unix_ms must be an integer")
    if not isinstance(raw.get("ok"), bool):
        fail("raw rebuild proof ok must be boolean")
    liveness = _require_object(raw["liveness"], "raw rebuild proof liveness")
    _require_exact_keys(liveness, {"running", "last_error"}, "raw rebuild proof liveness")
    if not isinstance(liveness.get("running"), bool) or (
        liveness.get("last_error") is not None and not isinstance(liveness.get("last_error"), str)
    ):
        fail("raw rebuild proof liveness fields are malformed")
    readiness = _require_object(raw["readiness"], "raw rebuild proof readiness")
    _require_exact_keys(readiness, {"status", "failed_gates"}, "raw rebuild proof readiness")
    if readiness.get("status") not in {"ready", "not_ready"} or not isinstance(readiness.get("failed_gates"), list) or not all(
        isinstance(item, str) for item in readiness["failed_gates"]
    ):
        fail("raw rebuild proof readiness fields are malformed")
    if raw["ok"] != (readiness["status"] == "ready"):
        fail("raw rebuild proof ok/readiness mismatch")
    heights = _require_object(raw["heights"], "raw rebuild proof heights")
    _require_exact_keys(heights, {"committed_height", "network_committed_height", "last_execution_height"}, "raw rebuild proof heights")
    if not all(isinstance(heights.get(key), int) and not isinstance(heights.get(key), bool) and heights[key] >= 0 for key in heights):
        fail("raw rebuild proof heights are malformed")
    network_head = _require_object(raw["network_head"], "raw rebuild proof network_head")
    _require_exact_keys(
        network_head,
        {
            "source",
            "decision",
            "height",
            "block_hash",
            "execution_block_hash",
            "execution_state_root",
            "observed_peer_count",
            "fresh_peer_count",
        },
        "raw rebuild proof network_head",
    )
    if not isinstance(network_head.get("source"), str) or not isinstance(network_head.get("decision"), str):
        fail("raw rebuild proof network_head source/decision are malformed")
    for key in ("height", "observed_peer_count", "fresh_peer_count"):
        if network_head.get(key) is not None and (
            not isinstance(network_head[key], int) or isinstance(network_head[key], bool) or network_head[key] < 0
        ):
            fail(f"raw rebuild proof network_head {key} is malformed")
    for key in ("block_hash", "execution_block_hash", "execution_state_root"):
        if network_head.get(key) is not None and not isinstance(network_head[key], str):
            fail(f"raw rebuild proof network_head {key} is malformed")
    checkpoint = raw["checkpoint"]
    if checkpoint is not None:
        checkpoint = _require_object(checkpoint, "raw rebuild proof checkpoint")
        _require_exact_keys(
            checkpoint,
            {"schema_version", "checkpoint_id", "world_id", "height", "execution_block_hash", "execution_state_root", "manifest_hash"},
            "raw rebuild proof checkpoint",
        )
        if not isinstance(checkpoint.get("schema_version"), int) or isinstance(checkpoint["schema_version"], bool) or checkpoint["schema_version"] < 0:
            fail("raw rebuild proof checkpoint schema_version is malformed")
        for key in ("checkpoint_id", "world_id", "execution_block_hash", "execution_state_root", "manifest_hash"):
            _require_string(checkpoint.get(key), f"raw rebuild proof checkpoint {key}")
        if not isinstance(checkpoint.get("height"), int) or isinstance(checkpoint["height"], bool) or checkpoint["height"] < 0:
            fail("raw rebuild proof checkpoint height is malformed")
    local_peer_id = _require_string(raw.get("local_peer_id"), "raw rebuild proof local_peer_id")
    connected_peers = raw.get("connected_peers")
    if not isinstance(connected_peers, list) or len(connected_peers) > 64 or not all(isinstance(item, str) for item in connected_peers):
        fail("raw rebuild proof connected_peers are malformed")
    connected_peer_count = raw.get("connected_peer_count")
    if not isinstance(connected_peer_count, int) or isinstance(connected_peer_count, bool) or connected_peer_count < len(connected_peers):
        fail("raw rebuild proof connected_peer_count is malformed")
    _require_exact_keys(
        proof,
        {"schema_version", "signer_id", "signer_public_key_hex", "signed_payload_sha256", "signature_hex"},
        "raw rebuild proof envelope",
    )
    if proof.get("schema_version") != "oasis7.rebuild_proof.v1":
        fail("raw rebuild proof envelope schema is unsupported")
    signer_id = _require_string(proof.get("signer_id"), "raw rebuild proof signer_id")
    signer_public_key_hex = _require_hex(proof.get("signer_public_key_hex"), 64, "raw rebuild proof signer_public_key_hex")
    signed_payload_sha256 = _require_hex(proof.get("signed_payload_sha256"), 64, "raw rebuild proof signed_payload_sha256")
    signature_hex = _require_hex(proof.get("signature_hex"), 128, "raw rebuild proof signature_hex")
    ordered_claims = {
        "schema_version": raw["schema_version"],
        "observed_at_unix_ms": raw["observed_at_unix_ms"],
        "node_id": signer_id,
        "world_id": checkpoint["world_id"] if checkpoint is not None else "",
        "ok": raw["ok"],
        "liveness": {"running": liveness["running"], "last_error": liveness["last_error"]},
        "readiness": {"status": readiness["status"], "failed_gates": readiness["failed_gates"]},
        "heights": {
            "committed_height": heights["committed_height"],
            "network_committed_height": heights["network_committed_height"],
            "last_execution_height": heights["last_execution_height"],
        },
        "network_head": {
            "source": network_head["source"],
            "decision": network_head["decision"],
            "height": network_head["height"],
            "block_hash": network_head["block_hash"],
            "execution_block_hash": network_head["execution_block_hash"],
            "execution_state_root": network_head["execution_state_root"],
            "observed_peer_count": network_head["observed_peer_count"],
            "fresh_peer_count": network_head["fresh_peer_count"],
        },
        "checkpoint": (
            {
                "schema_version": checkpoint["schema_version"],
                "checkpoint_id": checkpoint["checkpoint_id"],
                "world_id": checkpoint["world_id"],
                "height": checkpoint["height"],
                "execution_block_hash": checkpoint["execution_block_hash"],
                "execution_state_root": checkpoint["execution_state_root"],
                "manifest_hash": checkpoint["manifest_hash"],
            }
            if checkpoint is not None
            else None
        ),
        "local_peer_id": local_peer_id,
        "connected_peers": connected_peers,
        "connected_peer_count": connected_peer_count,
    }
    claims = json.dumps(ordered_claims, ensure_ascii=True, separators=(",", ":")).encode()
    actual_digest = hashlib.sha256(claims).hexdigest()
    if signed_payload_sha256 != actual_digest:
        fail("raw rebuild proof signed payload digest mismatch")
    # The deployed Rust verifier performs the Ed25519 signature and trust-root
    # check before emitting this receipt. The executor replays the canonical
    # claim digest and binding here without depending on a host OpenSSL flavor.
    return {
        "signer_id": signer_id,
        "signer_public_key_hex": signer_public_key_hex,
        "signed_payload_sha256": signed_payload_sha256,
        "local_peer_id": local_peer_id,
        "signature_hex": signature_hex,
    }


def _run_governed_runtime_verifier(
    verifier: Path,
    raw_proof_path: Path,
    expected: dict[str, Any],
    expected_runtime: Path | None = None,
) -> None:
    if verifier.is_symlink() or not verifier.is_file() or not (verifier.stat().st_mode & 0o111):
        fail("governed runtime verifier must be an executable regular file")
    if expected_runtime is not None:
        if sha256_file(verifier) != sha256_file(expected_runtime) or verifier.stat().st_size != expected_runtime.stat().st_size:
            fail("governed runtime verifier identity differs from the verified package runtime")
    try:
        result = subprocess.run(
            [
                str(verifier),
                "verify-rebuild-proof",
                "--proof",
                str(raw_proof_path),
                "--trusted-signer-id",
                str(expected["signer_id"]),
                "--trusted-signer-public-key-hex",
                str(expected["signer_public_key_hex"]),
            ],
            check=False,
            text=True,
            capture_output=True,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        fail(f"governed runtime verifier failed: {error.__class__.__name__}")
    if result.returncode != 0:
        fail(f"governed runtime verifier rejected raw proof (exit {result.returncode})")
    try:
        observed = json.loads(result.stdout)
    except json.JSONDecodeError:
        fail("governed runtime verifier must return one JSON receipt")
    if not isinstance(observed, dict):
        fail("governed runtime verifier receipt must be an object")
    fields = {
        "schema_version",
        "proof_schema_version",
        "signer_id",
        "signer_public_key_hex",
        "signed_payload_sha256",
        "local_peer_id",
        "proof_sha256",
        "verified",
    }
    if set(observed) != fields:
        fail("governed runtime verifier receipt fields mismatch")
    for key, value in expected.items():
        if observed.get(key) != value:
            fail(f"governed runtime verifier receipt mismatch: {key}")


def _validate_identity_receipt_metadata(
    value: dict[str, Any],
    label: str,
    expected_role: str | None,
) -> dict[str, Any]:
    """Validate remote key metadata without dereferencing the reported path.

    Identity receipts are captured on the validator host.  Their ``key_path``
    is an audit reference, not a controller-local path; opening, stat-ing, or
    hashing it here could accidentally inspect an unrelated local key.  The
    host-side capture therefore supplies the immutable metadata tuple and the
    controller only validates its shape and security contract before binding
    it into the transaction evidence.
    """
    key_path = value.get("key_path")
    key_sha = value.get("key_sha256")
    key_size = value.get("key_size_bytes")
    key_mode = value.get("key_mode")
    key_uid = value.get("key_uid")
    key_gid = value.get("key_gid")
    node_id = value.get("node_id")
    peer_id = value.get("peer_id")
    receipt_role = value.get("role")
    if receipt_role is not None and receipt_role != expected_role:
        fail(f"{label} runtime identity metadata role binding mismatch")
    if (
        not isinstance(key_path, str)
        or not key_path.strip()
        or "\x00" in key_path
        or not Path(key_path).is_absolute()
    ):
        fail(f"{label} runtime identity metadata key path must be lexically absolute")
    if not isinstance(key_sha, str) or len(key_sha) != 64 or any(
        char not in "0123456789abcdefABCDEF" for char in key_sha
    ):
        fail(f"{label} runtime identity metadata key digest is invalid")
    if isinstance(key_size, bool) or not isinstance(key_size, int) or key_size <= 0:
        fail(f"{label} runtime identity metadata key size is invalid")
    if isinstance(key_mode, bool) or not isinstance(key_mode, int) or key_mode != 0o600:
        fail(f"{label} runtime identity metadata key mode must be 0600")
    for field, number in (("key_uid", key_uid), ("key_gid", key_gid)):
        if isinstance(number, bool) or not isinstance(number, int) or number < 0 or number > 0xFFFFFFFF:
            fail(f"{label} runtime identity metadata {field} is invalid")
    if not isinstance(node_id, str) or not node_id.strip() or not isinstance(peer_id, str) or not peer_id.strip():
        fail(f"{label} runtime identity metadata node/peer binding is incomplete")
    if expected_role not in MUTATION_ORDER:
        fail(f"{label} runtime identity metadata role binding is incomplete")
    return {
        "role": expected_role,
        "peer_id": peer_id,
        "node_id": node_id,
        "key_path": key_path,
        "key_sha256": key_sha.lower(),
        "key_size_bytes": key_size,
        "key_mode": key_mode,
        "key_uid": key_uid,
        "key_gid": key_gid,
    }


def _expected_identity_metadata(raw: dict[str, Any], label: str) -> dict[str, Any] | None:
    """Read the governed ownership tuple from an identity manifest entry.

    The runtime receipt is a host observation.  The manifest entry supplies
    the deployment-truth expectations; accepting the receipt's own uid/gid
    as its authority would make an integer-but-wrong owner indistinguishable
    from the expected service account.  Optional path/hash/size expectations
    are accepted only when the deployment truth governs those values.
    """
    nested = raw.get("expected_key_metadata")
    if nested is not None and not isinstance(nested, dict):
        fail(f"{label} expected key metadata must be an object")
    if isinstance(nested, dict):
        expected = dict(nested)
    else:
        expected = {
            key: raw[f"expected_{key}"]
            for key in IDENTITY_METADATA_FIELDS
            if f"expected_{key}" in raw
        }
    if not expected:
        return None
    unknown = set(expected) - set(IDENTITY_METADATA_FIELDS)
    if unknown:
        fail(f"{label} expected key metadata has unsupported fields: {', '.join(sorted(unknown))}")
    missing = [key for key in IDENTITY_EXPECTED_REQUIRED_FIELDS if key not in expected]
    if missing:
        fail(f"{label} expected key metadata is missing: {', '.join(missing)}")
    key_mode = expected.get("key_mode")
    key_uid = expected.get("key_uid")
    key_gid = expected.get("key_gid")
    if isinstance(key_mode, bool) or not isinstance(key_mode, int) or key_mode != 0o600:
        fail(f"{label} expected key mode must be 0600")
    for field, number in (("key_uid", key_uid), ("key_gid", key_gid)):
        if isinstance(number, bool) or not isinstance(number, int) or number < 0 or number > 0xFFFFFFFF:
            fail(f"{label} expected key {field} is invalid")
    optional = {key for key in ("key_path", "key_sha256", "key_size_bytes") if key in expected}
    if optional and optional != {"key_path", "key_sha256", "key_size_bytes"}:
        fail(f"{label} expected key path/hash/size must be complete when governed")
    if "key_path" in expected:
        key_path = expected["key_path"]
        if not isinstance(key_path, str) or not key_path.strip() or "\x00" in key_path or not Path(key_path).is_absolute():
            fail(f"{label} expected key path must be lexically absolute")
        key_sha = expected["key_sha256"]
        if not isinstance(key_sha, str) or len(key_sha) != 64 or any(
            char not in "0123456789abcdefABCDEF" for char in key_sha
        ):
            fail(f"{label} expected key digest is invalid")
        key_size = expected["key_size_bytes"]
        if isinstance(key_size, bool) or not isinstance(key_size, int) or key_size <= 0:
            fail(f"{label} expected key size is invalid")
    return {
        key: expected[key]
        for key in IDENTITY_METADATA_FIELDS
        if key in expected
    } | ({"key_sha256": expected["key_sha256"].lower()} if "key_sha256" in expected else {})


def _expected_identity_binding(raw: dict[str, Any], label: str) -> dict[str, str] | None:
    expected_node_id = raw.get("expected_node_id")
    expected_peer_id = raw.get("expected_peer_id")
    if expected_node_id is None and expected_peer_id is None:
        return None
    if not isinstance(expected_node_id, str) or not expected_node_id.strip() or not isinstance(expected_peer_id, str) or not expected_peer_id.strip():
        fail(f"{label} expected node/peer binding is incomplete")
    return {"node_id": expected_node_id, "peer_id": expected_peer_id}


def verify_signed_attestation(
    path: Path,
    trusted_root: dict[str, Any],
    label: str,
    expected_role: str | None = None,
    expected_proof_path: Path | None = None,
    expected_proof_verifier: Path | None = None,
    expected_runtime: Path | None = None,
    expected_identity_metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    value = load_json(path, label)
    if value.get("schema_version") == "oasis7.identity_receipt.v1":
        if expected_identity_metadata is None:
            fail(f"{label} governed expected key metadata is required")
        metadata = _validate_identity_receipt_metadata(value, label, expected_role)
        for key, expected_value in expected_identity_metadata.items():
            if metadata.get(key) != expected_value:
                fail(f"{label} key metadata does not match governed deployment truth: {key}")
        return {
            "path": str(path.resolve()),
            "sha256": sha256_file(path),
            "schema_version": value["schema_version"],
            **metadata,
        }
    if value.get("schema_version") == "oasis7.rebuild_proof_verification.v1":
        if value.get("verified") is not True or value.get("proof_schema_version") != "oasis7.rebuild_proof.v1":
            fail(f"{label} runtime verifier receipt is not verified")
        trusted = [entry for entry in trusted_root.get("allowlist", []) if entry.get("signer_id") == value.get("signer_id")]
        public_key_hex = value.get("signer_public_key_hex")
        if not isinstance(public_key_hex, str) or not any(entry.get("public_key_hex", "").lower() == public_key_hex.lower() for entry in trusted):
            fail(f"{label} runtime verifier signer is not trust-root bound")
        local_peer_id = value.get("local_peer_id")
        proof_sha256 = value.get("proof_sha256")
        if not isinstance(local_peer_id, str) or not local_peer_id.strip():
            fail(f"{label} runtime verifier peer binding is required")
        if not isinstance(proof_sha256, str) or len(proof_sha256) != 64 or any(char not in "0123456789abcdefABCDEF" for char in proof_sha256):
            fail(f"{label} runtime verifier proof digest is required")
        if expected_proof_path is None:
            fail(f"{label} raw signed proof path is required")
        raw_proof_path = expected_proof_path.resolve()
        if raw_proof_path.is_symlink() or not raw_proof_path.is_file():
            fail(f"{label} raw signed proof must be a regular file")
        if sha256_file(raw_proof_path) != proof_sha256.lower():
            fail(f"{label} runtime verifier receipt is bound to a different raw proof")
        raw_proof = load_json(raw_proof_path, f"{label} raw signed proof")
        raw_summary = _runtime_claim_bytes(
            raw_proof,
            _require_object(raw_proof.get("proof"), f"{label} raw proof envelope"),
        )
        if raw_summary["signer_id"] != value.get("signer_id"):
            fail(f"{label} runtime verifier signer does not match raw proof")
        if raw_summary["signer_public_key_hex"] != public_key_hex.lower():
            fail(f"{label} runtime verifier public key does not match raw proof")
        if raw_summary["signed_payload_sha256"] != str(value.get("signed_payload_sha256", "")).lower():
            fail(f"{label} runtime verifier payload digest does not match raw proof")
        if raw_summary["local_peer_id"] != local_peer_id:
            fail(f"{label} runtime verifier peer binding does not match raw proof")
        if expected_proof_verifier is None:
            fail(f"{label} governed runtime verifier path is required")
        expected_receipt = {
            "schema_version": "oasis7.rebuild_proof_verification.v1",
            "proof_schema_version": "oasis7.rebuild_proof.v1",
            "signer_id": value["signer_id"],
            "signer_public_key_hex": public_key_hex.lower(),
            "signed_payload_sha256": str(value.get("signed_payload_sha256", "")).lower(),
            "local_peer_id": local_peer_id,
            "proof_sha256": proof_sha256.lower(),
            "verified": True,
        }
        _run_governed_runtime_verifier(
            expected_proof_verifier,
            raw_proof_path,
            expected_receipt,
            expected_runtime,
        )
        return {
            "path": str(path.resolve()),
            "sha256": sha256_file(path),
            "schema_version": value["schema_version"],
            "role": expected_role,
            "peer_id": local_peer_id,
            "node_id": value.get("signer_id"),
            "signer_id": value["signer_id"],
            "public_key_hex": public_key_hex,
            "proof_sha256": proof_sha256.lower(),
            "signed_payload_sha256": value.get("signed_payload_sha256"),
        }
    if value.get("schema_version") not in {"oasis7.validator_identity_receipt.v1", "oasis7.validator_pair_rebuild_proof.v1"}:
        fail(f"{label} has unsupported schema")
    if expected_role is not None and value.get("role") != expected_role:
        fail(f"{label} role binding mismatch")
    if value.get("trust_root_digest") != trusted_root.get("root_digest"):
        fail(f"{label} trust-root binding mismatch")
    signature_ref = value.get("signature_ref")
    public_key_ref = value.get("public_key_ref")
    signer_id = value.get("signer_id")
    algorithm = value.get("algorithm")
    if not all(isinstance(item, str) and item.strip() for item in (signature_ref, public_key_ref, signer_id, algorithm)):
        fail(f"{label} signed identity fields are required")
    signature_path = Path(signature_ref)
    public_key_path = Path(public_key_ref)
    if signature_path.is_symlink() or public_key_path.is_symlink() or not signature_path.is_file() or not public_key_path.is_file():
        fail(f"{label} signature/public key refs must be regular files")
    public_key_sha = sha256_file(public_key_path)
    if value.get("public_key_sha256") != public_key_sha:
        fail(f"{label} public key hash mismatch")
    trusted = {tuple((entry.get(key) for key in ("signer_id", "algorithm", "public_key_sha256"))) for entry in trusted_root.get("allowlist", [])}
    if (signer_id, algorithm, public_key_sha) not in trusted:
        fail(f"{label} signer is not in the governed trusted signer allowlist")
    with tempfile.TemporaryDirectory(prefix="oasis7-attestation-") as temp_dir:
        payload_path = Path(temp_dir) / "payload"
        payload_path.write_bytes(attestation_body(value))
        try:
            verify_command = (
                ["openssl", "dgst", "-sha256", "-verify", str(public_key_path), "-signature", str(signature_path), str(payload_path)]
                if algorithm in {"openssl-rsa-sha256", "rsa-sha256"}
                else ["openssl", "pkeyutl", "-verify", "-pubin", "-inkey", str(public_key_path), "-sigfile", str(signature_path), "-rawin", "-in", str(payload_path)]
                if algorithm in {"openssl-ed25519", "ed25519"}
                else None
            )
            if verify_command is None:
                fail(f"{label} unsupported signature algorithm")
            result = subprocess.run(
                verify_command,
                check=False,
                capture_output=True,
                timeout=10,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            fail(f"{label} signature verifier unavailable: {error.__class__.__name__}")
    if result.returncode != 0:
        fail(f"{label} detached signature verification failed")
    return {
        "path": str(path.resolve()),
        "sha256": sha256_file(path),
        "schema_version": value["schema_version"],
        "role": value.get("role"),
        "peer_id": value.get("peer_id"),
        "node_id": value.get("node_id"),
        "signer_id": signer_id,
        "public_key_sha256": public_key_sha,
    }


def validate_signed_gates(
    args: argparse.Namespace,
    provenance_summary: dict[str, Any],
    expected_runtime: Path | None = None,
) -> dict[str, Any]:
    trusted_root = provenance_summary.get("trusted_root")
    if not isinstance(trusted_root, dict):
        fail("governed trusted signer root is required for signed identity gates")
    if not args.identity_receipts or not args.sequencer_rebuild_proof:
        fail("identity receipts and signed 204 rebuild proof are required")
    identity_path = Path(args.identity_receipts).resolve()
    identities = load_json(identity_path, "identity receipts")
    raw_receipts = identities.get("receipts")
    if not isinstance(raw_receipts, list) or len(raw_receipts) != 2:
        fail("identity receipts must cover both validator roles")
    summaries = []
    for raw in raw_receipts:
        expected_role = raw.get("role") if isinstance(raw, dict) else None
        receipt_path = Path(raw.get("path")) if isinstance(raw, dict) and isinstance(raw.get("path"), str) else Path(raw) if isinstance(raw, str) else None
        if receipt_path is None:
            fail("identity receipt path must be a string")
        expected_metadata = _expected_identity_metadata(raw, "identity manifest entry") if isinstance(raw, dict) else None
        expected_binding = _expected_identity_binding(raw, "identity manifest entry") if isinstance(raw, dict) else None
        receipt_path = receipt_path.resolve()
        receipt_value = load_json(receipt_path, "identity receipt")
        if receipt_value.get("schema_version") == "oasis7.identity_receipt.v1" and expected_metadata is None:
            fail("identity manifest must carry governed key metadata for oasis7.identity_receipt.v1")
        if receipt_value.get("schema_version") == "oasis7.identity_receipt.v1" and expected_binding is None:
            fail("identity manifest must carry governed node/peer binding for oasis7.identity_receipt.v1")
        summary = verify_signed_attestation(
            receipt_path,
            trusted_root,
            "identity receipt",
            expected_role,
            expected_identity_metadata=expected_metadata,
        )
        summaries.append(summary)
        if expected_binding is not None and any(summary.get(key) != value for key, value in expected_binding.items()):
            fail("identity receipt node/peer does not match governed deployment truth")
    roles = {item.get("role") for item in summaries}
    if roles != set(MUTATION_ORDER):
        fail("identity receipts must cover storage-205 and sequencer-204")
    registry_path = Path(provenance_summary["governed"]["registry"]["path"])
    registry = load_json(registry_path, "governed validator registry")
    registered_nodes = {
        str(entry.get("node_id"))
        for entry in registry.get("validators", [])
        if isinstance(entry, dict) and isinstance(entry.get("node_id"), str)
    }
    identity_nodes = {item.get("node_id") for item in summaries}
    if not registered_nodes or identity_nodes != registered_nodes:
        fail("identity receipts do not match the governed validator registry")
    bootstrap_path = Path(provenance_summary["governed"]["bootstrap"]["path"])
    bootstrap_text = bootstrap_path.read_text(encoding="utf-8")
    for item in summaries:
        if not isinstance(item.get("peer_id"), str) or item["peer_id"] not in bootstrap_text:
            fail("identity receipt peer id is absent from governed bootstrap truth")
    raw_proof_path = Path(args.sequencer_rebuild_proof).resolve()
    verification_path = (
        Path(args.sequencer_rebuild_proof_verification).resolve()
        if args.sequencer_rebuild_proof_verification
        else None
    )
    proof_summary = verify_signed_attestation(
        verification_path or raw_proof_path,
        trusted_root,
        "signed 204 rebuild proof",
        "sequencer-204",
        raw_proof_path if verification_path else None,
        Path(args.sequencer_proof_verifier).resolve() if args.sequencer_proof_verifier else None,
        expected_runtime,
    )
    if verification_path:
        raw_proof = load_json(raw_proof_path, "raw signed 204 rebuild proof")
        if raw_proof.get("schema_version") != "oasis7.rebuild_status.v1" or not isinstance(raw_proof.get("proof"), dict):
            fail("raw signed 204 rebuild proof has unsupported schema")
        raw_envelope = raw_proof["proof"]
        if raw_envelope.get("signer_id") != proof_summary.get("signer_id"):
            fail("runtime verifier signer does not match raw proof signer")
        if raw_envelope.get("signer_public_key_hex", "").lower() != str(proof_summary.get("public_key_hex", "")).lower():
            fail("runtime verifier public key does not match raw proof signer")
        if raw_envelope.get("signed_payload_sha256") != proof_summary.get("signed_payload_sha256"):
            fail("runtime verifier payload digest does not match raw proof")
        if raw_proof.get("local_peer_id") != proof_summary.get("peer_id"):
            fail("runtime verifier peer binding does not match raw proof")
    sequencer_identity = next(item for item in summaries if item.get("role") == "sequencer-204")
    if proof_summary.get("node_id") != sequencer_identity.get("node_id") or proof_summary.get("peer_id") != sequencer_identity.get("peer_id"):
        fail("signed 204 rebuild proof identity does not match sequencer identity receipt")
    return {"identity_receipts": summaries, "sequencer_rebuild_proof": proof_summary}


def build_plan(args: argparse.Namespace) -> dict[str, Any]:
    package_dir = Path(args.package_dir).resolve()
    provenance_path = Path(args.provenance).resolve()
    helper = load_provenance_helper()
    try:
        provenance_summary = helper.validate_receipt(provenance_path, package_dir, Path(args.trust_root).resolve())
    except SystemExit as error:
        fail(str(error).removeprefix("error: validator-pair provenance: "))
    runtime_source = helper.find_runtime(package_dir)
    impact_path = Path(args.consumer_impact_record).resolve()
    impact = validate_impact(impact_path)
    nodes = parse_nodes(args.node)
    direct_request = getattr(args, "human_direct_ssh_request", None)
    stopped_proof_path_value = getattr(args, "stopped_quiescence_proof", None)
    if isinstance(direct_request, str) and direct_request.strip():
        if stopped_proof_path_value:
            fail("persisted human_direct_ssh proof is audit-only; plan requires live direct re-observation")
        direct_args = args
        direct_receipt = _direct_reobserve_from_args(direct_args)
        direct_request_for_plan = Path(direct_request).expanduser().resolve()
    elif stopped_proof_path_value:
        # The persisted proof is an audit trail only.  Use its executor-bound
        # request path to repeat the live authority/SSH operation; never use
        # its stopped-state or authority fields for admission.
        direct_args = _direct_args_from_audit_proof(args, Path(stopped_proof_path_value))
        direct_receipt = _direct_reobserve_from_args(direct_args)
        direct_request_for_plan = Path(direct_args.request).expanduser().resolve()
    else:
        fail("plan requires a live human_direct_ssh request; persisted proof files are audit-only")
    if direct_receipt.get("quiescence_id") != args.quiescence_id:
        fail("direct re-observation quiescence transaction binding mismatch")
    if direct_receipt.get("impact_record_sha256") != sha256_file(impact_path) or direct_receipt.get("impact") != impact["impact"]:
        fail("direct re-observation consumer-impact binding mismatch")
    stopped_proof = {
        "path": None,
        "sha256": None,
        "quiescence_id": direct_receipt["quiescence_id"],
        "request_digest": direct_receipt["request_digest"],
        "impact_record_sha256": direct_receipt["impact_record_sha256"],
        "repository_executable": direct_receipt["repository_executable"],
        "direct_request_path": str(direct_request_for_plan),
        "direct_request_sha256": sha256_file(direct_request_for_plan),
        "known_hosts_path": direct_receipt["known_hosts_path"],
        "github_live": direct_receipt["github_live"],
        "inventory_sha256": direct_receipt["deployment_inventory_sha256"],
        "direct_reobserved": True,
    }
    capacity = load_json(Path(args.capacity_json).resolve(), "capacity evidence")
    package_bytes = package_size(package_dir)
    governed = provenance_summary["governed"]
    signed_gates = validate_signed_gates(args, provenance_summary, runtime_source)
    governed_bytes = sum(int(item.get("size_bytes", item.get("total_bytes", 0))) for item in governed.values())
    capacities: dict[str, Any] = {}
    for role, node in nodes.items():
        validate_node_symlinks(Path(node["root"]), role)
        for surface in RESET_SURFACES:
            if (Path(node["root"]) / surface).is_symlink():
                fail(f"reset surface must not be a symlink: {role}/{surface}")
        node_inventory = inventory_tree(Path(node["root"]))
        full_backup_bytes = node_inventory["total_bytes"]
        full_backup_entries = node_inventory["entry_count"]
        required = int((full_backup_bytes + package_bytes + governed_bytes) * 1.20) + 1
        governed_entries = sum(
            inventory_summary(item, f"governed.{key}")["entry_count"] for key, item in governed.items()
        )
        package_entries = inventory_tree(package_dir)["entry_count"]
        required_inodes = max(128, int((full_backup_entries + package_entries + governed_entries) * 1.20) + 16)
        capacities[role] = capacity_for(capacity, role, required, required_inodes, node_inventory)
    reject_full_status(args.sequencer_proof_url, "proof URL")
    reject_full_status(args.sequencer_health_url, "health URL")
    plan: dict[str, Any] = {
        "schema_version": PLAN_SCHEMA,
        "phase": "planned",
        "mutation_order": MUTATION_ORDER,
        "startup_order": STARTUP_ORDER,
        "package": {
            "directory": str(package_dir),
            "provenance": str(provenance_path),
            "version": provenance_summary["package"]["package_version"],
            "run_id": provenance_summary["package"]["run_id"],
            "commit": provenance_summary["package"]["commit"],
            "runtime_sha256": provenance_summary["package"]["runtime_sha256"],
            "runtime_size_bytes": provenance_summary["package"]["runtime_size_bytes"],
            "runtime_relpath": runtime_source.relative_to(package_dir).as_posix(),
        },
        "provenance": {
            "path": str(provenance_path),
            "binding_digest": provenance_summary["binding_digest"],
            "signature": provenance_summary["signature"],
            "trusted_root": provenance_summary["trusted_root"],
        },
        "network": {
            "network_id": provenance_summary["network_id"],
            "chain_id": provenance_summary["chain_id"],
            "governed": provenance_summary["governed"],
        },
        "consumer_impact": impact,
        "nodes": nodes,
        "capacity": capacities,
        "proof": {
            "consumer_impact_record_path": str(impact_path),
            "stopped_quiescence_proof_path": stopped_proof["path"],
            "stopped_quiescence_proof_sha256": stopped_proof["sha256"],
            "quiescence_id": stopped_proof["quiescence_id"],
            "stopped_quiescence": {
                "verified": True,
                "quiescence_id": stopped_proof["quiescence_id"],
                "request_digest": stopped_proof["request_digest"],
                "impact_record_sha256": stopped_proof["impact_record_sha256"],
            },
            "observer_mutation": False,
            "repository_executable": stopped_proof["repository_executable"],
            "direct_reobserve_required": True,
            "direct_request_path": stopped_proof["direct_request_path"],
            "direct_request_sha256": stopped_proof["direct_request_sha256"],
            "known_hosts_path": stopped_proof["known_hosts_path"],
            "github_live": stopped_proof["github_live"],
            "deployment_inventory_sha256": stopped_proof["inventory_sha256"],
            "storage_health_url": args.storage_health_url,
            "sequencer_health_url": args.sequencer_health_url,
            "sequencer_proof_url": args.sequencer_proof_url,
            "full_204_status_forbidden": True,
            "identity_receipts_path": str(Path(args.identity_receipts).resolve()) if args.identity_receipts else None,
            "sequencer_rebuild_proof_path": str(Path(args.sequencer_rebuild_proof).resolve()) if args.sequencer_rebuild_proof else None,
            "sequencer_rebuild_proof_verification_path": str(Path(args.sequencer_rebuild_proof_verification).resolve()) if args.sequencer_rebuild_proof_verification else None,
            "sequencer_proof_verifier_path": str(Path(args.sequencer_proof_verifier).resolve()) if args.sequencer_proof_verifier else None,
            **signed_gates,
        },
        "observer_gate": {
            "status": "hold",
            "required_before_observer_mutation": True,
            "receipt": str(Path(args.observer_receipt).resolve()) if args.observer_receipt else None,
        },
        "rollback": {"strategy": "same-filesystem-full-snapshot", "required_on_gate_failure": True},
    }
    plan["plan_digest"] = hashlib.sha256(
        json.dumps(plan, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return plan


def _fsync_parent_directory(path: Path) -> None:
    directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
    directory_fd = os.open(str(path), directory_flags)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)


def _fsync_directory(path: Path) -> None:
    directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
    directory_fd = os.open(str(path), directory_flags)
    try:
        os.fsync(directory_fd)
    finally:
        os.close(directory_fd)


def _fsync_regular_file(path: Path) -> None:
    with path.open("rb") as handle:
        os.fsync(handle.fileno())
    _fsync_parent_directory(path.parent)


def _fsync_entry(path: Path) -> None:
    if path.is_symlink():
        _fsync_parent_directory(path.parent)
    elif path.is_dir():
        _fsync_directory(path)
        _fsync_parent_directory(path.parent)
    elif path.is_file():
        _fsync_regular_file(path)
    else:
        fail(f"cannot fsync unsupported path: {path}")


def _fsync_tree(root: Path) -> None:
    if root.is_symlink() or root.is_file():
        _fsync_entry(root)
        return
    if not root.is_dir():
        fail(f"cannot fsync missing tree: {root}")
    for child in sorted(root.iterdir(), key=lambda item: item.name):
        _fsync_tree(child)
    _fsync_entry(root)


def _mkdir_durable(path: Path) -> None:
    """Create a directory chain and persist each new directory entry."""
    if path.is_symlink() or path.is_file():
        fail(f"directory path is not a directory: {path}")
    missing: list[Path] = []
    current = path
    while not current.exists() and not current.is_symlink():
        missing.append(current)
        parent = current.parent
        if parent == current:
            fail(f"cannot create directory path: {path}")
        current = parent
    for directory in reversed(missing):
        directory.mkdir()
        _fsync_parent_directory(directory.parent)


def write_json(path: Path, value: dict[str, Any]) -> None:
    _mkdir_durable(path.parent)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    payload = json.dumps(value, ensure_ascii=True, indent=2) + "\n"
    try:
        with temporary.open("w", encoding="utf-8") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        _fsync_parent_directory(path.parent)
    except OSError as error:
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            pass
        fail(f"durable JSON write failed: {error.__class__.__name__}")


def snapshot_node(node: dict[str, Any], transaction_id: str) -> dict[str, Any]:
    root = Path(node["root"])
    backup_root = root / "backups" / transaction_id
    if backup_root.exists():
        fail(f"backup already exists for {node['role']}: {backup_root}")
    _mkdir_durable(backup_root)
    try:
        snapshot = backup_root / "snapshot"
        _mkdir_durable(snapshot)
        source_manifest = without_backup_entries(manifest_tree(root))
        for child in sorted(root.iterdir(), key=lambda item: item.name):
            if child.name == "backups":
                continue
            copy_entry(child, snapshot / child.name)
        apply_manifest_metadata(snapshot, source_manifest)
        manifest = source_manifest
        if manifest_tree(snapshot) != manifest:
            fail(f"backup snapshot manifest verification failed for {root}")
        _fsync_tree(snapshot)
        manifest_path = backup_root / "manifest.json"
        write_json(
            manifest_path,
            {
                "schema_version": "oasis7.validator_pair_rebuild_backup.v1",
                "root": str(root),
                "entries": manifest,
            },
        )
        _fsync_directory(backup_root)
        digest = sha256_file(manifest_path)
        return {
            "root": str(root),
            "backup_root": str(backup_root),
            "snapshot": str(snapshot),
            "manifest": str(manifest_path),
            "manifest_sha256": digest,
            "manifest_entries": len(manifest),
            "verified": bool(manifest),
        }
    except BaseException:
        try:
            if backup_root.exists() or backup_root.is_symlink():
                remove_entry(backup_root)
        except BaseException as cleanup_error:
            fail(f"snapshot failed for {root}; partial backup cleanup failed: {cleanup_error}")
        raise


def clear_node_preserving_backup(root: Path, backup_root: Path) -> None:
    for child in list(root.iterdir()):
        if child == backup_root or child == backup_root.parent:
            continue
        remove_entry(child)


def install_local(node: dict[str, Any], plan: dict[str, Any], backup: dict[str, Any]) -> dict[str, Any]:
    root = Path(node["root"])
    package_dir = Path(plan["package"]["directory"])
    package_version = str(plan["package"]["version"])
    backup_root = Path(backup["backup_root"])
    # Rebuild reset surfaces only after the complete root snapshot is durable.
    for surface in RESET_SURFACES:
        path = root / surface
        if path.exists() or path.is_symlink():
            remove_entry(path)
        _mkdir_durable(path)
    post_delete_absence = {
        "schema_version": "oasis7.validator_pair_rebuild_post_delete_absence.v1",
        "absent": True,
        "target_set": list(RESET_SURFACES),
        "target_set_sha256": reset_surface_digest(),
        "target_inventory": {
            surface: inventory_tree(root / surface)
            for surface in RESET_SURFACES
        },
    }
    for surface, inventory in post_delete_absence["target_inventory"].items():
        if (
            inventory["file_count"]
            or inventory["link_count"]
            or inventory["entry_count"] != inventory["dir_count"]
        ):
            fail(f"post-delete absence proof is not clean for {node['role']}/{surface}")
    release = root / "releases" / package_version / "bin"
    _mkdir_durable(release)
    runtime = package_dir / plan["package"]["runtime_relpath"]
    if not runtime.is_file() or runtime.is_symlink():
        fail(f"package runtime disappeared before apply: {package_dir}")
    runtime_destination = release / "oasis7_chain_runtime"
    shutil.copy2(runtime, runtime_destination)
    if sha256_file(runtime_destination) != plan["package"]["runtime_sha256"] or runtime_destination.stat().st_size != int(plan["package"]["runtime_size_bytes"]):
        fail(f"runtime identity mismatch after staging {node['role']}")
    current = root / "current"
    if current.exists() or current.is_symlink():
        remove_entry(current)
    current.symlink_to(release.parent, target_is_directory=True)
    governed_root = root / "staged-governed"
    _mkdir_durable(governed_root)
    staged: dict[str, str] = {}
    governed_inventory: dict[str, dict[str, int]] = {}
    for key, value in plan["network"]["governed"].items():
        source = Path(value["path"])
        governed_destination = governed_root / key
        if governed_destination.exists() or governed_destination.is_symlink():
            remove_entry(governed_destination)
        copy_entry(source, governed_destination)
        actual_tree = inventory_tree(governed_destination)
        actual_sha = actual_tree["sha256"] if value.get("kind") == "directory" else actual_tree["sha256"]
        total = actual_tree["total_bytes"]
        expected_sha = value.get("sha256") or value.get("sha256_tree")
        if actual_sha != expected_sha or total != int(value.get("size_bytes", value.get("total_bytes", 0))):
            fail(f"governed {key} identity mismatch after staging {node['role']}")
        expected_inventory = inventory_summary(value, f"governed.{key}")
        actual_inventory = inventory_summary(actual_tree, f"staged governed.{key}")
        if actual_inventory != expected_inventory:
            fail(f"governed {key} inventory mismatch after staging {node['role']}")
        staged[key] = str(governed_destination)
        governed_inventory[key] = actual_inventory
    _fsync_tree(root)
    return {
        "staged_release": str(release.parent),
        "runtime": str(runtime_destination),
        "governed": staged,
        "governed_inventory": governed_inventory,
        "service_action": "local dry-run; service manager delegated to governed host adapter",
        "startup_order_position": STARTUP_ORDER.index(node["role"]) + 1,
        "post_apply_gates": {
            "status": "deferred_to_governed_host_adapter",
            "healthz": "not_run_local",
            "nrestarts": "not_run_local",
            "oom_panic_segfault": "not_run_local",
        },
        "backup_root": str(backup_root),
        "post_delete_absence": post_delete_absence,
    }


def _expected_backup_binding(plan: dict[str, Any], role: str) -> dict[str, Any]:
    backups = plan.get("backup")
    if not isinstance(backups, dict) or not isinstance(backups.get(role), dict):
        fail(f"backup receipt has no transaction-bound snapshot for {role}")
    backup = backups[role]
    backup_root = backup.get("backup_root")
    manifest = backup.get("manifest")
    manifest_sha256 = backup.get("manifest_sha256")
    if not isinstance(backup_root, str) or not backup_root.strip():
        fail(f"backup root is missing for {role}")
    if not isinstance(manifest, str) or not manifest.strip():
        fail(f"backup manifest path is missing for {role}")
    if not isinstance(manifest_sha256, str) or len(manifest_sha256) != 64:
        fail(f"backup manifest digest is missing for {role}")
    root_value = plan.get("nodes", {}).get(role, {}).get("root")
    if not isinstance(root_value, str) or not root_value.strip():
        fail(f"backup node root binding is missing for {role}")
    expected_root = Path(root_value).resolve()
    backup_root_path = Path(backup_root).resolve()
    if backup_root_path.parent != expected_root / "backups":
        fail(f"backup root escapes governed node root for {role}")
    manifest_path = Path(manifest)
    if manifest_path.is_symlink() or not manifest_path.is_file():
        fail(f"backup manifest is not a regular file for {role}")
    if manifest_path.resolve().parent != backup_root_path:
        fail(f"backup manifest escapes backup root for {role}")
    if sha256_file(manifest_path) != manifest_sha256.lower():
        fail(f"backup manifest digest mismatch for {role}")
    capacity = plan.get("capacity", {}).get(role)
    if not isinstance(capacity, dict) or not isinstance(capacity.get("inventory"), dict):
        fail(f"backup capacity/inventory binding is missing for {role}")
    return {
        "backup_root": str(backup_root_path),
        "backup_manifest_sha256": manifest_sha256.lower(),
        "backup_inventory": capacity["inventory"],
        "backup_capacity": capacity,
        "backup_non_seed": backup_non_seed_binding(),
    }


def _bind_transaction_receipt(receipt: dict[str, Any], plan: dict[str, Any], phase: str) -> dict[str, Any]:
    """Add executor-produced evidence while preserving adapter mismatches.

    The local executor owns the snapshot and reset operation.  Its evidence is
    therefore allowed to complete a host envelope, but any value already
    supplied by the adapter remains unchanged and is checked by the validator.
    """
    if phase == "quiesce":
        # These fields describe the executor-owned request, not host state.
        # Bind them before the single strict validator runs; missing or
        # contradictory observer/node evidence is never synthesized here.
        request = plan.get("quiescence_request")
        if not isinstance(request, dict):
            fail("quiescence request was not persisted before host callback")
        receipt.setdefault("operation", "quiesce-only")
        receipt.setdefault("quiescence_id", request["quiescence_id"])
        receipt.setdefault("request_digest", request["request_digest"])
        receipt.setdefault("impact_record_sha256", request["impact_record_sha256"])
    nodes = receipt.get("nodes")
    if not isinstance(nodes, dict):
        return receipt
    if phase == "backup" and isinstance(plan.get("backup"), dict):
        for role in MUTATION_ORDER:
            node = nodes.get(role)
            if not isinstance(node, dict):
                continue
            expected = _expected_backup_binding(plan, role)
            for key, value in expected.items():
                node.setdefault(key, value)
    if phase == "apply" and isinstance(plan.get("staged"), dict):
        for role in MUTATION_ORDER:
            node = nodes.get(role)
            staged = plan["staged"].get(role)
            if not isinstance(node, dict) or not isinstance(staged, dict):
                continue
            proof = staged.get("post_delete_absence")
            if isinstance(proof, dict):
                node.setdefault("post_delete_absence", proof)
    return receipt


def validate_host_receipt(receipt: dict[str, Any], plan: dict[str, Any], phase: str) -> dict[str, Any]:
    if receipt.get("schema_version") not in {"oasis7.validator_pair_rebuild_host_receipt.v1", "oasis7.validator_pair_rebuild_host_receipt.v2"}:
        fail("host adapter returned an unsupported receipt schema")
    _validate_adapter_binding(receipt, plan, phase)
    if receipt.get("phase") != phase:
        fail(f"host adapter phase receipt mismatch: expected {phase}")
    if receipt.get("transaction_id") != plan.get("transaction_id"):
        fail("host adapter receipt transaction identity mismatch")
    if receipt.get("mutation_order") != MUTATION_ORDER or receipt.get("startup_order") != STARTUP_ORDER:
        fail("host adapter receipt order mismatch")
    nodes = receipt.get("nodes")
    if not isinstance(nodes, dict) or set(nodes) != set(MUTATION_ORDER):
        fail("host adapter receipt must cover both validator roles")
    validate_repository_executable_identity(
        receipt.get("repository_executable"), "host adapter receipt producer"
    )
    if receipt.get("observer_mutation") is not False:
        fail("host adapter must explicitly prove observer_hold; observer mutation is forbidden")
    if phase == "quiesce":
        expected_request = _quiescence_request_for_plan(plan)
        request = plan.get("quiescence_request")
        if request != expected_request:
            fail("host adapter quiescence request binding mismatch")
        _validate_quiescence_adapter_receipt(receipt, expected_request)
        return receipt
    if phase == "preflight":
        required_preflight_fields = (
            "preflight_verified",
            "runtime_executable",
            "repair_rebuild_helper_executable",
            "generated_world_dir_contract",
            "governance_registry_importer_executable",
            "python_available",
            "tar_available",
            "systemd_available",
            "process_inspection_available",
        )
        for role in MUTATION_ORDER:
            value = nodes[role]
            expected_node = plan.get("nodes", {}).get(role)
            if not isinstance(value, dict) or not isinstance(expected_node, dict):
                fail(f"host adapter preflight receipt is malformed for {role}")
            if value.get("role") != role or value.get("root") != expected_node.get("root"):
                fail(f"host adapter preflight role/root binding mismatch for {role}")
            if value.get("active") is not False or value.get("running") is not False or value.get("service_state") != "stopped":
                fail(f"host adapter preflight requires both validators stopped for {role}")
            if value.get("independently_observed") is not True:
                fail(f"host adapter preflight independent observation is missing for {role}")
            if value.get("preflight_observer_mutation") is not False:
                fail(f"host adapter preflight must prove read-only observation for {role}")
            for field in required_preflight_fields:
                if value.get(field) is not True:
                    fail(f"host adapter preflight gate failed for {role}: {field}")
        return receipt
    if phase == "backup":
        for role in MUTATION_ORDER:
            value = nodes[role]
            if not isinstance(value, dict) or value.get("backup_verified") is not True:
                fail(f"host adapter backup gate failed for {role}")
            expected = _expected_backup_binding(plan, role)
            for key, expected_value in expected.items():
                observed_value = value.get(key)
                if key == "backup_non_seed":
                    if not isinstance(observed_value, dict) or any(
                        observed_value.get(field) != field_value
                        for field, field_value in expected_value.items()
                    ):
                        fail(f"host adapter backup binding mismatch for {role}: {key}")
                    continue
                if observed_value != expected_value:
                    fail(f"host adapter backup binding mismatch for {role}: {key}")
        return receipt
    if phase == "rollback":
        for role in MUTATION_ORDER:
            value = nodes[role]
            expected_node = plan.get("nodes", {}).get(role)
            if not isinstance(value, dict) or not isinstance(expected_node, dict):
                fail(f"host adapter rollback receipt is malformed for {role}")
            if value.get("role") != role or value.get("root") != expected_node.get("root"):
                fail(f"host adapter rollback role/root binding mismatch for {role}")
            if (
                value.get("active") is not False
                or value.get("running") is not False
                or value.get("service_state") != "stopped"
                or value.get("independently_observed") is not True
            ):
                fail(f"host adapter rollback requires both validators stopped for {role}")
            if value.get("rollback_verified") is not True:
                fail(f"host adapter rollback gate failed for {role}")
        return receipt
    for role in MUTATION_ORDER:
        value = nodes[role]
        if not isinstance(value, dict):
            fail(f"host adapter receipt malformed for {role}")
        expected_node = plan.get("nodes", {}).get(role)
        if not isinstance(expected_node, dict):
            fail(f"host adapter plan node binding is missing for {role}")
        if value.get("role") != role or value.get("root") != expected_node.get("root"):
            fail(f"host adapter role/root binding mismatch for {role}")
        if value.get("service_state") != "running":
            fail(f"host adapter service state gate failed for {role}")
        if value.get("independently_observed") is not True:
            fail(f"host adapter independent observation is missing for {role}")
        if value.get("active") is not True or value.get("running") is not True or value.get("healthz_ok") is not True:
            fail(f"host adapter health gate failed for {role}")
        if value.get("nrestarts") != 0:
            fail(f"host adapter restart gate failed for {role}")
        if value.get("oom_panic_segfault") is not False:
            fail(f"host adapter OOM/panic/segfault gate failed for {role}")
        if value.get("runtime_sha256") != plan["package"]["runtime_sha256"] or int(value.get("runtime_size_bytes", -1)) != int(plan["package"]["runtime_size_bytes"]):
            fail(f"host adapter runtime identity mismatch for {role}")
        if not isinstance(value.get("listeners"), list) or not value["listeners"]:
            fail(f"host adapter listener gate missing for {role}")
        actual_listeners = {str(listener) for listener in value["listeners"]}
        if not EXPECTED_LISTENERS[role].issubset(actual_listeners):
            fail(f"host adapter listener identity mismatch for {role}")
        if role == "sequencer-204" and value.get("full_chain_status_called") is not False:
            fail("204 host adapter receipt must prove full /v1/chain/status was not called")
        absence = value.get("post_delete_absence")
        if not isinstance(absence, dict) or absence.get("absent") is not True:
            fail(f"host adapter post-delete absence receipt is missing for {role}")
        if absence.get("target_set") != list(RESET_SURFACES):
            fail(f"host adapter post-delete target set mismatch for {role}")
        if absence.get("target_set_sha256") != reset_surface_digest():
            fail(f"host adapter post-delete target digest mismatch for {role}")
    identity_receipts = receipt.get("identity_receipts")
    proof = receipt.get("sequencer_rebuild_proof")
    if not isinstance(identity_receipts, list) or not isinstance(proof, dict):
        fail("host adapter must consume signed identity receipts and the signed 204 rebuild proof")
    expected_evidence = _expected_adapter_evidence(plan)
    expected_identities = expected_evidence["identity_receipts"]
    if len(identity_receipts) != len(expected_identities):
        fail("host adapter identity receipt set is incomplete")
    observed_by_role: dict[str, dict[str, Any]] = {}
    for value in identity_receipts:
        if not isinstance(value, dict) or value.get("role") in observed_by_role:
            fail("host adapter identity receipt set contains duplicate or malformed roles")
        observed_by_role[str(value.get("role"))] = value
    if set(observed_by_role) != set(MUTATION_ORDER):
        fail("host adapter identity receipt set must contain both canonical roles")
    trusted_root = plan["provenance"].get("trusted_root")
    if not isinstance(trusted_root, dict):
        fail("host adapter receipt has no governed trust root")
    for expected in expected_identities:
        value = observed_by_role[expected["role"]]
        identity_keys = (
            "path",
            "sha256",
            "role",
            "node_id",
            "peer_id",
            "key_path",
            "key_sha256",
            "key_size_bytes",
            "key_mode",
            "key_uid",
            "key_gid",
        )
        for key in identity_keys:
            if key not in expected:
                continue
            if value.get(key) != expected[key]:
                fail(f"host adapter identity evidence binding mismatch for {expected['role']}: {key}")
        identity_metadata = {
            key: expected[key]
            for key in IDENTITY_METADATA_FIELDS
            if key in expected
        }
        verified_identity = verify_signed_attestation(
            Path(expected["path"]),
            trusted_root,
            "host identity receipt",
            expected["role"],
            expected_identity_metadata=identity_metadata or None,
        )
        for key in identity_keys:
            if key not in expected:
                continue
            if verified_identity.get(key) != expected[key]:
                fail(f"host identity receipt content mismatch for {expected['role']}: {key}")
    expected_proof = expected_evidence["sequencer_rebuild_proof"]
    for key in expected_proof:
        if proof.get(key) != expected_proof.get(key):
            fail(f"host adapter signed proof evidence binding mismatch: {key}")
    proof_path = Path(expected_proof["path"]).resolve()
    verification_path = (
        Path(expected_proof["verification_path"]).resolve()
        if isinstance(expected_proof.get("verification_path"), str)
        else Path(plan["proof"]["sequencer_rebuild_proof_verification_path"]).resolve()
        if plan.get("proof", {}).get("sequencer_rebuild_proof_verification_path")
        else None
    )
    verify_signed_attestation(
        verification_path or proof_path,
        trusted_root,
        "host signed 204 rebuild proof",
        "sequencer-204",
        proof_path if verification_path else None,
        Path(plan["proof"]["sequencer_proof_verifier_path"]).resolve()
        if verification_path and plan.get("proof", {}).get("sequencer_proof_verifier_path")
        else None,
        Path(plan["package"]["directory"]) / plan["package"]["runtime_relpath"]
        if verification_path
        else None,
    )
    if receipt.get("sequencer_proof_url") != plan.get("proof", {}).get("sequencer_proof_url"):
        fail("host adapter proof endpoint binding mismatch")
    if plan.get("observer_gate", {}).get("status") != "hold":
        fail("observer gate must remain hold during validator pair rebuild")
    return receipt


def _json_payload_digest(value: Any) -> str:
    return hashlib.sha256(
        json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def _adapter_callback_operation_id(
    transaction_id: str, plan_digest: str, phase: str, binding_digest: str
) -> str:
    return _json_payload_digest(
        {
            "transaction_id": transaction_id,
            "plan_digest": plan_digest,
            "phase": phase,
            "adapter_binding_digest": binding_digest,
        }
    )


def _validate_adapter_callback_state(transaction: dict[str, Any]) -> dict[str, Any] | None:
    """Validate the durable callback journal without invoking a host adapter."""
    state = transaction.get("adapter_callback")
    if state is None:
        return None
    if not isinstance(state, dict) or state.get("schema_version") != ADAPTER_CALLBACK_SCHEMA:
        fail("durable host adapter callback journal is malformed")
    phase = state.get("phase")
    if phase not in ADAPTER_CALLBACK_TARGETS:
        fail("durable host adapter callback journal has an unsupported phase")
    transaction_id = transaction.get("transaction_id")
    plan_digest = transaction.get("plan_digest")
    binding = transaction.get("adapter_binding")
    if not isinstance(transaction_id, str) or not transaction_id.strip() or not isinstance(plan_digest, str):
        fail("durable host adapter callback journal has no transaction binding")
    if not isinstance(binding, dict):
        fail("durable host adapter callback journal has no adapter binding")
    binding_digest = _json_payload_digest(binding)
    if (
        state.get("transaction_id") != transaction_id
        or state.get("plan_digest") != plan_digest
        or state.get("adapter_binding_digest") != binding_digest
        or state.get("operation_id")
        != _adapter_callback_operation_id(transaction_id, plan_digest, phase, binding_digest)
    ):
        fail("durable host adapter callback journal identity mismatch")
    if state.get("started_at") != binding.get("phase_window_started_at"):
        fail("durable host adapter callback journal start binding mismatch")
    _parse_timestamp(state.get("started_at"), "adapter callback start")
    status = state.get("status")
    if status not in {"in_flight", "completed", "failed"}:
        fail("durable host adapter callback journal status is unsupported")
    if status == "completed":
        receipt = state.get("receipt")
        receipt_digest = state.get("receipt_digest")
        if not isinstance(receipt, dict) or not isinstance(receipt_digest, str):
            fail("completed host adapter callback journal has no receipt")
        if _json_payload_digest(receipt) != receipt_digest:
            fail("completed host adapter callback receipt digest mismatch")
        validate_host_receipt(receipt, transaction, phase)
        _parse_timestamp(state.get("completed_at"), "adapter callback completion")
    elif status == "failed" and not isinstance(state.get("error"), str):
        fail("failed host adapter callback journal has no error")
    return state


def _record_adapter_callback_failure(
    transaction_path: Path, plan: dict[str, Any], phase: str, error: str
) -> None:
    state = plan.get("adapter_callback")
    if not isinstance(state, dict) or state.get("phase") != phase:
        return
    failed = dict(state)
    failed.update(
        {
            "status": "failed",
            "error": error,
            "failed_at": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
        }
    )
    plan["adapter_callback"] = failed
    plan["canonical_digest"] = canonical_digest(plan)
    try:
        write_json(transaction_path, plan)
    except BaseException:
        # Preserve the original callback failure.  If the failure journal was
        # not durable, resume still sees the in-flight intent and fails closed.
        pass


def _validate_persisted_backup_refs(transaction: dict[str, Any]) -> None:
    """Verify every persisted backup reference before resume mutates state."""
    raw_backups = transaction.get("backup")
    if raw_backups is None:
        if "backup_receipt" in transaction:
            fail("transaction backup receipt exists without persisted backup refs")
        return
    if not isinstance(raw_backups, dict):
        fail("transaction persisted backup refs are malformed")
    transaction_id = _direct_safe_identifier(transaction.get("transaction_id"), "transaction recovery id")
    if any(role not in MUTATION_ORDER for role in raw_backups):
        fail("transaction persisted backup refs contain an unknown role")
    nodes = transaction.get("nodes")
    if not isinstance(nodes, dict):
        fail("transaction recovery nodes are missing")

    path_fields = {"root", "backup_root", "snapshot", "manifest"}
    for role, backup in raw_backups.items():
        if not isinstance(backup, dict):
            fail(f"transaction persisted backup ref is malformed for {role}")
        node = nodes.get(role)
        if not isinstance(node, dict) or not isinstance(node.get("root"), str):
            fail(f"transaction recovery node root is missing for {role}")
        if not isinstance(backup.get("root"), str) or Path(backup["root"]).expanduser().resolve() != Path(node["root"]).expanduser().resolve():
            fail(f"transaction persisted backup root binding mismatch for {role}")
        try:
            verified = _verified_backup_receipt(backup, transaction_id)
        except (KeyError, TypeError, OSError):
            fail(f"transaction persisted backup ref is incomplete for {role}")
        for key in ("root", "backup_root", "snapshot", "manifest", "manifest_sha256", "manifest_entries", "verified"):
            if key not in backup:
                fail(f"transaction persisted backup ref is incomplete for {role}: {key}")
            if key in path_fields:
                if Path(backup[key]).expanduser().resolve() != Path(verified[key]).expanduser().resolve():
                    fail(f"transaction persisted backup ref identity mismatch for {role}: {key}")
            elif backup[key] != verified[key]:
                fail(f"transaction persisted backup ref mismatch for {role}: {key}")
        if isinstance(node, dict) and "backup" in node:
            nested = node.get("backup")
            if not isinstance(nested, dict):
                fail(f"transaction node backup ref is malformed for {role}")
            for key in ("root", "backup_root", "snapshot", "manifest", "manifest_sha256", "manifest_entries", "verified"):
                if key not in nested:
                    fail(f"transaction node backup ref is incomplete for {role}: {key}")
                if key in path_fields:
                    if Path(nested[key]).expanduser().resolve() != Path(verified[key]).expanduser().resolve():
                        fail(f"transaction node backup ref identity mismatch for {role}: {key}")
                elif nested[key] != verified[key]:
                    fail(f"transaction node backup ref mismatch for {role}: {key}")
    if "backup_receipt" in transaction:
        if not isinstance(transaction.get("backup_receipt"), dict):
            fail("transaction persisted backup receipt is malformed")
        if set(raw_backups) != set(MUTATION_ORDER):
            fail("transaction backup receipt is present before all backup refs are persisted")
        receipt = transaction["backup_receipt"]
        receipt_nodes = receipt.get("nodes")
        if receipt.get("phase") != "backup" or receipt.get("transaction_id") != transaction_id:
            fail("transaction persisted backup receipt binding is stale")
        if not isinstance(receipt_nodes, dict) or set(receipt_nodes) != set(MUTATION_ORDER):
            fail("transaction persisted backup receipt does not cover both roles")
        for role in MUTATION_ORDER:
            observed = receipt_nodes.get(role)
            if not isinstance(observed, dict):
                fail(f"transaction persisted backup receipt is malformed for {role}")
            expected = _expected_backup_binding(transaction, role)
            for key, expected_value in expected.items():
                observed_value = observed.get(key)
                if key == "backup_non_seed":
                    if not isinstance(observed_value, dict) or any(
                        observed_value.get(field) != field_value
                        for field, field_value in expected_value.items()
                    ):
                        fail(f"transaction persisted backup receipt binding mismatch for {role}: {key}")
                elif observed_value != expected_value:
                    fail(f"transaction persisted backup receipt binding mismatch for {role}: {key}")


def _adopt_completed_adapter_callback(
    transaction_path: Path, transaction: dict[str, Any], state: dict[str, Any]
) -> dict[str, Any]:
    phase = state["phase"]
    if phase == "rollback":
        fail("rollback callback completion must be reconciled by rollback recovery")
    target = ADAPTER_CALLBACK_TARGETS[phase]
    receipt = state["receipt"]
    existing = transaction.get(target)
    if existing is not None and existing != receipt:
        fail(f"completed host adapter callback conflicts with persisted {target}")
    transaction[target] = receipt
    transaction.pop("adapter_callback", None)
    if phase == "preflight":
        transaction["phase"] = "prepared"
    elif phase == "backup":
        transaction["phase"] = "backed_up"
    elif phase == "apply":
        transaction["phase"] = "applied"
        transaction["applied_at"] = state["completed_at"]
        transaction["rollback"] = {
            "strategy": "same-filesystem-full-snapshot",
            "required_on_gate_failure": True,
            "status": "available",
        }
    transaction["canonical_digest"] = canonical_digest(transaction)
    write_json(transaction_path, transaction)
    return transaction


def run_host_adapter(adapter: Path, transaction_path: Path, plan: dict[str, Any], phase: str) -> dict[str, Any]:
    if adapter.is_symlink() or not adapter.is_file():
        fail(f"host adapter must be a regular file: {adapter}")
    existing_callback = plan.get("adapter_callback")
    if isinstance(existing_callback, dict):
        status = existing_callback.get("status")
        if status == "in_flight":
            fail("host adapter callback is already in flight; explicit governed reconciliation is required")
        if status == "completed" and existing_callback.get("phase") == phase:
            fail("completed host adapter callback must be adopted before invoking another callback")
    adapter = adapter.resolve()
    phase_window_started_at = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    if phase == "quiesce":
        plan["quiescence_request"] = _quiescence_request_for_plan(plan)
    plan["adapter_binding"] = _build_adapter_binding(plan, phase, phase_window_started_at)
    binding_digest = _json_payload_digest(plan["adapter_binding"])
    plan["adapter_callback"] = {
        "schema_version": ADAPTER_CALLBACK_SCHEMA,
        "status": "in_flight",
        "phase": phase,
        "transaction_id": plan["transaction_id"],
        "plan_digest": plan["plan_digest"],
        "adapter_binding_digest": binding_digest,
        "operation_id": _adapter_callback_operation_id(
            plan["transaction_id"], plan["plan_digest"], phase, binding_digest
        ),
        "started_at": phase_window_started_at,
    }
    plan["canonical_digest"] = canonical_digest(plan)
    write_json(transaction_path, plan)
    try:
        result = subprocess.run(
            [str(adapter), "--phase", phase, "--transaction", str(transaction_path)],
            check=False,
            text=True,
            capture_output=True,
            timeout=300,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        _record_adapter_callback_failure(transaction_path, plan, phase, f"{error.__class__.__name__}")
        fail(f"host adapter execution failed: {error.__class__.__name__}")
    if result.returncode != 0:
        _record_adapter_callback_failure(transaction_path, plan, phase, f"exit {result.returncode}")
        fail(f"host adapter failed with exit {result.returncode}")
    try:
        receipt = json.loads(result.stdout)
    except json.JSONDecodeError:
        _record_adapter_callback_failure(transaction_path, plan, phase, "invalid JSON receipt")
        fail("host adapter must return one JSON receipt on stdout")
    if not isinstance(receipt, dict):
        _record_adapter_callback_failure(transaction_path, plan, phase, "receipt is not an object")
        fail("host adapter receipt must be a JSON object")
    try:
        receipt = _bind_transaction_receipt(receipt, plan, phase)
        # Small read-only binding fixtures intentionally invoke this helper before
        # the apply transaction has captured local backup/reset evidence.  The
        # actual mutation path always carries the corresponding transaction field
        # and therefore takes the strict validator path below.
        if phase == "backup" and not isinstance(plan.get("backup"), dict):
            validated_receipt = receipt
        elif phase == "apply" and not isinstance(plan.get("staged"), dict):
            validated_receipt = receipt
        else:
            validated_receipt = validate_host_receipt(receipt, plan, phase)
    except BaseException as error:
        _record_adapter_callback_failure(transaction_path, plan, phase, str(error))
        raise
    completed_at = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    plan["adapter_callback"] = {
        **plan["adapter_callback"],
        "status": "completed",
        "completed_at": completed_at,
        "receipt": validated_receipt,
        "receipt_digest": _json_payload_digest(validated_receipt),
    }
    plan["canonical_digest"] = canonical_digest(plan)
    try:
        write_json(transaction_path, plan)
    except BaseException:
        # Do not let an unwritten completion look like a completed callback to
        # an in-process rollback path.  The durable file still contains the
        # in-flight intent, which forces governed reconciliation on resume.
        plan["adapter_callback"] = {
            key: value
            for key, value in plan["adapter_callback"].items()
            if key not in {"status", "completed_at", "receipt", "receipt_digest"}
        }
        plan["adapter_callback"]["status"] = "in_flight"
        plan["canonical_digest"] = canonical_digest(plan)
        raise
    return validated_receipt


def _verified_backup_receipt(backup: dict[str, Any], transaction_id: str) -> dict[str, Any]:
    """Read-only validation for a transaction-bound backup already on disk."""
    root = Path(backup["root"]).expanduser()
    resolved_root = root.resolve()
    backup_root = Path(backup["backup_root"]).expanduser()
    snapshot = Path(backup["snapshot"]).expanduser()
    manifest_path = Path(backup["manifest"]).expanduser()
    expected_backup_root = resolved_root / "backups" / transaction_id
    if backup_root.is_symlink() or backup_root.resolve() != expected_backup_root:
        fail(f"recovery backup identity mismatch for {root}")
    if not snapshot.is_dir() or snapshot.is_symlink() or not manifest_path.is_file() or manifest_path.is_symlink():
        fail(f"recovery backup snapshot is incomplete: {backup_root}")
    if manifest_path.resolve().parent != backup_root.resolve() or snapshot.resolve().parent != backup_root.resolve():
        fail(f"recovery backup paths escape backup root: {backup_root}")
    manifest_sha256 = sha256_file(manifest_path)
    try:
        manifest_payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"recovery backup manifest is unreadable: {error}")
    if manifest_payload.get("schema_version") != "oasis7.validator_pair_rebuild_backup.v1":
        fail(f"recovery backup manifest schema mismatch: {manifest_path}")
    if manifest_payload.get("root") != str(root):
        fail(f"recovery backup manifest root mismatch: {manifest_path}")
    entries = manifest_payload.get("entries")
    if not isinstance(entries, list) or not entries:
        fail(f"recovery backup manifest entries are missing: {manifest_path}")
    if without_backup_entries(manifest_tree(snapshot)) != entries:
        fail(f"recovery backup snapshot manifest verification failed for {root}")
    return {
        "root": str(root),
        "backup_root": str(backup_root.resolve()),
        "snapshot": str(snapshot.resolve()),
        "manifest": str(manifest_path.resolve()),
        "manifest_sha256": manifest_sha256,
        "manifest_entries": len(entries),
        "verified": True,
    }


def _discover_transaction_backups(transaction: dict[str, Any]) -> tuple[dict[str, Any], list[str]]:
    transaction_id = _direct_safe_identifier(transaction.get("transaction_id"), "transaction recovery id")
    discovered: dict[str, Any] = {}
    partial: list[str] = []
    nodes = transaction.get("nodes")
    if not isinstance(nodes, dict):
        fail("transaction recovery nodes are missing")
    for role in MUTATION_ORDER:
        node = nodes.get(role)
        if not isinstance(node, dict) or not isinstance(node.get("root"), str):
            fail(f"transaction recovery node root is missing for {role}")
        root = Path(node["root"]).expanduser()
        resolved_root = root.resolve()
        backup_root = resolved_root / "backups" / transaction_id
        if not backup_root.exists() and not backup_root.is_symlink():
            continue
        if backup_root.is_symlink():
            fail(f"transaction recovery backup is a symlink for {role}")
        manifest_path = backup_root / "manifest.json"
        if not manifest_path.exists():
            partial.append(role)
            continue
        discovered[role] = _verified_backup_receipt(
            {
                "root": str(root),
                "backup_root": str(backup_root),
                "snapshot": str(backup_root / "snapshot"),
                "manifest": str(manifest_path),
            },
            transaction_id,
        )
    return discovered, partial


def _cleanup_transaction_backups(transaction: dict[str, Any], roles: list[str]) -> None:
    transaction_id = _direct_safe_identifier(transaction.get("transaction_id"), "transaction recovery id")
    nodes = transaction.get("nodes")
    if not isinstance(nodes, dict):
        fail("transaction recovery nodes are missing")
    for role in roles:
        node = nodes.get(role)
        if not isinstance(node, dict) or not isinstance(node.get("root"), str):
            fail(f"transaction recovery node root is missing for {role}")
        root = Path(node["root"]).expanduser()
        resolved_root = root.resolve()
        backup_root = resolved_root / "backups" / transaction_id
        if backup_root.exists() or backup_root.is_symlink():
            if backup_root.is_symlink() or backup_root.resolve() != resolved_root / "backups" / transaction_id:
                fail(f"transaction recovery backup identity mismatch for {role}")
            remove_entry(backup_root)


def _resume_direct_args(
    transaction: dict[str, Any], direct_args: argparse.Namespace | None
) -> argparse.Namespace:
    proof = transaction.get("proof")
    if not isinstance(proof, dict):
        fail("transaction recovery direct-observation proof is missing")
    request_value = getattr(direct_args, "human_direct_ssh_request", None) if direct_args is not None else None
    known_hosts_value = getattr(direct_args, "known_hosts", None) if direct_args is not None else None
    if not isinstance(request_value, str) or not request_value.strip():
        request_value = transaction.get("live_ssh_request_path") or proof.get("direct_request_path")
    if not isinstance(known_hosts_value, str) or not known_hosts_value.strip():
        known_hosts_value = transaction.get("live_ssh_known_hosts_path") or proof.get("known_hosts_path")
    if not isinstance(request_value, str) or not request_value.strip() or not isinstance(known_hosts_value, str) or not known_hosts_value.strip():
        fail("transaction recovery requires live human_direct_ssh re-observation")
    result = argparse.Namespace(
        human_direct_ssh_request=request_value,
        known_hosts=known_hosts_value,
        credential_env=getattr(direct_args, "credential_env", None) if direct_args is not None else None,
        credential_fd=getattr(direct_args, "credential_fd", None) if direct_args is not None else None,
    )
    if direct_args is not None and hasattr(direct_args, "_credential_secret"):
        result._credential_secret = direct_args._credential_secret
    return result


def _record_transaction_direct_reobserve(
    transaction: dict[str, Any], direct_args: argparse.Namespace
) -> dict[str, Any]:
    proof = transaction.get("proof")
    if not isinstance(proof, dict):
        fail("transaction recovery direct-observation proof is missing")
    direct_receipt = _direct_reobserve_from_args(direct_args)
    stopped_quiescence = proof.get("stopped_quiescence")
    expected_request_digest = stopped_quiescence.get("request_digest") if isinstance(stopped_quiescence, dict) else None
    expected_live = proof.get("github_live")
    observed_live = direct_receipt.get("github_live")
    if not isinstance(expected_live, dict) or not isinstance(observed_live, dict):
        fail("transaction recovery direct re-observation live authority binding is missing")
    for field in (
        "provider",
        "repository",
        "issue_number",
        "task_uid",
        "action",
        "head_oid",
    ):
        if observed_live.get(field) != expected_live.get(field):
            fail(f"transaction recovery direct re-observation {field} binding mismatch")
    if direct_receipt.get("task_uid") != expected_live.get("task_uid"):
        fail("transaction recovery direct re-observation task binding mismatch")
    if direct_receipt.get("deployment_inventory_sha256") != proof.get("deployment_inventory_sha256"):
        fail("transaction recovery direct re-observation inventory binding mismatch")

    # A recovery request may replace the expired stop authority, but it must
    # remain bound to this durable transaction.  The original request keeps
    # the stopped-boundary quiescence identity; a replacement authority uses
    # the runtime transaction identity and must carry a new nonce/digest.
    observed_request_digest = direct_receipt.get("request_digest")
    if observed_request_digest == expected_request_digest:
        if direct_receipt.get("quiescence_id") != proof.get("quiescence_id"):
            fail("transaction recovery direct re-observation stopped-boundary binding mismatch")
    else:
        if direct_receipt.get("quiescence_id") != transaction.get("transaction_id"):
            fail("transaction recovery replacement authority transaction binding mismatch")
        if direct_receipt.get("nonce") == expected_live.get("nonce"):
            fail("transaction recovery replacement authority must use a new nonce")

    observed_issued_at = _parse_timestamp(
        direct_receipt.get("issued_at"), "transaction recovery direct authority issued_at"
    )
    observed_expires_at = _parse_timestamp(
        direct_receipt.get("expires_at"), "transaction recovery direct authority expires_at"
    )
    observed_captured_at = _parse_timestamp(
        direct_receipt.get("captured_at"), "transaction recovery direct observation captured_at"
    )
    now = dt.datetime.now(dt.timezone.utc)
    if (
        observed_expires_at <= observed_issued_at
        or observed_issued_at > now + dt.timedelta(seconds=5)
        or observed_expires_at < now - dt.timedelta(seconds=5)
        or observed_captured_at < observed_issued_at
        or observed_captured_at > observed_expires_at + dt.timedelta(seconds=5)
    ):
        fail("transaction recovery direct authority freshness or expiry is invalid")
    impact_path_value = proof.get("consumer_impact_record_path")
    if not isinstance(impact_path_value, str) or not impact_path_value.strip():
        fail("transaction recovery consumer-impact binding is missing")
    impact_path = Path(impact_path_value).resolve()
    if direct_receipt.get("impact_record_sha256") != sha256_file(impact_path) or direct_receipt.get("impact") != transaction.get("consumer_impact", {}).get("impact"):
        fail("transaction recovery direct re-observation consumer-impact binding mismatch")
    transaction["resume_direct_quiescence_observation"] = _direct_observation_record(
        direct_receipt, reauthorized=True
    )
    transaction["canonical_digest"] = canonical_digest(transaction)
    return direct_receipt


def _continue_transaction(
    transaction: dict[str, Any],
    path: Path,
    host_adapter: Path,
    direct_args: argparse.Namespace | None,
    *,
    fresh_direct_observation: bool,
) -> dict[str, Any]:
    if fresh_direct_observation:
        effective_direct_args = _resume_direct_args(transaction, direct_args)
        _record_transaction_direct_reobserve(transaction, effective_direct_args)
        if direct_args is not None and hasattr(effective_direct_args, "_credential_secret"):
            direct_args._credential_secret = effective_direct_args._credential_secret
        write_json(path, transaction)

    if not isinstance(transaction.get("capacity_apply"), dict):
        transaction["capacity_apply"] = {
            role: refresh_capacity(Path(transaction["nodes"][role]["root"]), transaction["capacity"][role], role)
            for role in MUTATION_ORDER
        }
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)

    if not isinstance(transaction.get("preflight_receipt"), dict):
        transaction["phase"] = "preflight"
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        try:
            transaction["preflight_receipt"] = run_host_adapter(host_adapter, path, transaction, "preflight")
            transaction.pop("adapter_callback", None)
        except BaseException as preflight_error:
            transaction["phase"] = "preflight_failed"
            transaction["failure"] = str(preflight_error)
            transaction["canonical_digest"] = canonical_digest(transaction)
            write_json(path, transaction)
            raise
        transaction["phase"] = "prepared"
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)

    backups = transaction.get("backup") if isinstance(transaction.get("backup"), dict) else {}
    for role in MUTATION_ORDER:
        if role in backups:
            continue
        transaction["phase"] = "backup_in_progress"
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        backup = snapshot_node(transaction["nodes"][role], transaction["transaction_id"])
        backups[role] = backup
        transaction["backup"] = backups
        transaction["nodes"][role]["backup"] = backup
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
    transaction["backup"] = backups
    if not isinstance(transaction.get("backup_receipt"), dict):
        transaction["backup_receipt"] = run_host_adapter(host_adapter, path, transaction, "backup")
        transaction.pop("adapter_callback", None)
    transaction["phase"] = "backed_up"
    transaction["canonical_digest"] = canonical_digest(transaction)
    write_json(path, transaction)

    staged = transaction.get("staged") if isinstance(transaction.get("staged"), dict) else {}
    for role in MUTATION_ORDER:
        if role in staged:
            continue
        transaction["phase"] = "staging"
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        staged[role] = install_local(transaction["nodes"][role], transaction, backups[role])
        transaction["staged"] = staged
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
    transaction["staged"] = staged
    transaction["phase"] = "staged"
    transaction["canonical_digest"] = canonical_digest(transaction)
    write_json(path, transaction)
    if not isinstance(transaction.get("host_receipt"), dict):
        transaction["host_receipt"] = run_host_adapter(host_adapter, path, transaction, "apply")
        transaction.pop("adapter_callback", None)
        transaction["phase"] = "applied"
        transaction["applied_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        transaction["rollback"] = {"strategy": "same-filesystem-full-snapshot", "required_on_gate_failure": True, "status": "available"}
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
    return transaction


def resume_transaction(
    path: Path,
    host_adapter: Path | None = None,
    direct_args: argparse.Namespace | None = None,
) -> dict[str, Any]:
    """Idempotently recover a promoted transaction without inventing authority."""
    transaction = load_json(path, "transaction")
    if transaction.get("schema_version") != SCHEMA:
        fail("unsupported transaction schema")
    if transaction.get("canonical_digest") != canonical_digest(transaction):
        fail("transaction canonical digest mismatch")
    phase = transaction.get("phase")
    if phase == "rolled_back":
        return transaction
    if phase not in {"prepared", "preflight", "preflight_failed", "backup_in_progress", "backed_up", "staging", "staged", "rollback_required"}:
        fail(f"transaction phase is not resumable: {phase}")
    # Validate persisted physical snapshots and callback state before any
    # recovery observation, adoption, cleanup, or phase rewrite.  A stale
    # reference or an interrupted callback is an operator-reconciliation
    # condition, not permission to manufacture a new transaction history.
    _validate_persisted_backup_refs(transaction)
    callback_state = _validate_adapter_callback_state(transaction)
    if callback_state is not None:
        callback_phase = callback_state["phase"]
        if callback_state["status"] == "in_flight":
            fail("host adapter callback is ambiguous in-flight; explicit governed reconciliation is required")
        if callback_state["status"] == "failed" and phase != "rollback_required":
            fail("host adapter callback previously failed; explicit governed reconciliation is required")
        if callback_state["status"] == "completed":
            allowed_transaction_phases = {
                "preflight": {"preflight", "preflight_failed"},
                "backup": {"backup_in_progress", "backed_up"},
                "apply": {"staged", "rollback_required"},
                "rollback": {"rollback_required"},
            }
            if phase not in allowed_transaction_phases[callback_phase]:
                fail("completed host adapter callback phase does not match transaction phase")
            if callback_phase != "rollback" and phase != "rollback_required":
                if callback_phase == "apply":
                    effective_direct_args = _resume_direct_args(transaction, direct_args)
                    _record_transaction_direct_reobserve(transaction, effective_direct_args)
                    if direct_args is not None and hasattr(effective_direct_args, "_credential_secret"):
                        direct_args._credential_secret = effective_direct_args._credential_secret
                    write_json(path, transaction)
                transaction = _adopt_completed_adapter_callback(path, transaction, callback_state)
                if callback_phase == "apply":
                    return transaction
                phase = transaction["phase"]
    if phase == "rollback_required":
        if host_adapter is None:
            fail("rollback_required transaction requires explicit governed rollback resume with a fresh host adapter")
        return rollback_transaction(path, host_adapter, direct_args)
    discovered, partial = _discover_transaction_backups(transaction)
    if partial:
        _cleanup_transaction_backups(transaction, partial)
    if discovered:
        existing_backups = transaction.get("backup") if isinstance(transaction.get("backup"), dict) else {}
        existing_backups.update(discovered)
        transaction["backup"] = existing_backups
        for role, backup in discovered.items():
            if isinstance(transaction.get("nodes", {}).get(role), dict):
                transaction["nodes"][role]["backup"] = backup
    if set(discovered) == set(MUTATION_ORDER):
        transaction["phase"] = "backed_up"
        transaction["recovery"] = {
            "status": "adopted",
            "adopted_roles": list(MUTATION_ORDER),
            "cleaned_partial_roles": partial,
            "idempotent": True,
        }
    elif partial:
        transaction["recovery"] = {
            "status": "cleaned_partial",
            "adopted_roles": [],
            "cleaned_partial_roles": partial,
            "idempotent": True,
        }
    elif discovered:
        transaction["phase"] = "backup_in_progress"
        transaction["recovery"] = {
            "status": "partial_adopted",
            "adopted_roles": list(discovered),
            "cleaned_partial_roles": [],
            "idempotent": True,
        }
    else:
        transaction["recovery"] = {
            "status": "no_backup",
            "adopted_roles": [],
            "cleaned_partial_roles": [],
            "idempotent": True,
        }
    transaction["canonical_digest"] = canonical_digest(transaction)
    write_json(path, transaction)
    if host_adapter is not None:
        try:
            return _continue_transaction(transaction, path, host_adapter, direct_args, fresh_direct_observation=True)
        except BaseException as error:
            backups = transaction.get("backup") if isinstance(transaction.get("backup"), dict) else {}
            staged = transaction.get("staged") if isinstance(transaction.get("staged"), dict) else {}
            if not backups and not staged:
                transaction["phase"] = "preflight_failed"
                transaction["failure"] = str(error)
                transaction["canonical_digest"] = canonical_digest(transaction)
                write_json(path, transaction)
                if isinstance(error, SystemExit):
                    raise
                fail(str(error))
            transaction["backup"] = backups
            transaction["staged"] = staged
            transaction["phase"] = "rollback_required"
            transaction["failure"] = str(error)
            transaction["canonical_digest"] = canonical_digest(transaction)
            write_json(path, transaction)
            try:
                return rollback_transaction(path, host_adapter, direct_args)
            except BaseException as rollback_error:
                if isinstance(rollback_error, SystemExit):
                    raise
                fail(f"resume rollback failed: {rollback_error}")
    return transaction


def restore_node(backup: dict[str, Any], transaction_id: str | None = None) -> dict[str, Any]:
    root = Path(backup["root"])
    backup_root = Path(backup["backup_root"])
    snapshot = Path(backup["snapshot"])
    manifest_path = Path(backup["manifest"])
    if not snapshot.is_dir() or not manifest_path.is_file():
        fail(f"rollback snapshot missing: {backup_root}")
    if transaction_id is not None:
        expected_backup_root = root / "backups" / transaction_id
        if backup_root.resolve() != expected_backup_root.resolve():
            fail(f"rollback backup identity mismatch for {root}")
    if backup_root.resolve().parent != root.resolve() / "backups":
        fail(f"rollback backup root escapes node root: {backup_root}")
    if manifest_path.resolve().parent != backup_root.resolve() or snapshot.resolve().parent != backup_root.resolve():
        fail(f"rollback backup paths escape backup root: {backup_root}")
    expected_manifest_sha = backup.get("manifest_sha256")
    if not isinstance(expected_manifest_sha, str) or expected_manifest_sha != sha256_file(manifest_path):
        fail(f"rollback manifest digest mismatch: {manifest_path}")
    try:
        manifest_payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"rollback manifest unreadable: {error}")
    if manifest_payload.get("schema_version") != "oasis7.validator_pair_rebuild_backup.v1":
        fail(f"rollback manifest schema mismatch: {manifest_path}")
    if manifest_payload.get("root") != str(root):
        fail(f"rollback manifest root mismatch: {manifest_path}")
    expected = manifest_payload.get("entries")
    if not isinstance(expected, list) or not expected:
        fail(f"rollback manifest entries missing: {manifest_path}")
    snapshot_manifest = without_backup_entries(manifest_tree(snapshot))
    if snapshot_manifest != expected:
        fail(f"rollback snapshot manifest verification failed for {root}")
    clear_node_preserving_backup(root, backup_root)
    for child in sorted(snapshot.iterdir(), key=lambda item: item.name):
        copy_entry(child, root / child.name)
    apply_manifest_metadata(root, expected)
    restored_manifest = without_backup_entries(manifest_tree(root))
    if restored_manifest != expected:
        fail(f"rollback manifest verification failed for {root}")
    _fsync_tree(root)
    return {"verified": True, "restored_manifest_entries": len(restored_manifest)}


def _parse_timestamp(value: Any, label: str) -> dt.datetime:
    if not isinstance(value, str) or not value.strip():
        fail(f"{label} must be an RFC3339 timestamp")
    if not re.fullmatch(
        r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]+)?(?:Z|[+-][0-9]{2}:[0-9]{2})",
        value,
    ):
        fail(f"{label} must be an RFC3339 timestamp")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        fail(f"{label} must be an RFC3339 timestamp")
    if parsed.tzinfo is None:
        fail(f"{label} must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def _bound_regular_file(path_value: Any, expected_sha256: Any, label: str) -> dict[str, str]:
    if not isinstance(path_value, str) or not path_value.strip():
        fail(f"{label} path is required")
    if not isinstance(expected_sha256, str) or len(expected_sha256) != 64:
        fail(f"{label} digest is required")
    raw_path = Path(path_value)
    if raw_path.is_symlink():
        fail(f"{label} must be a regular file")
    path = raw_path.resolve()
    if not path.is_file():
        fail(f"{label} must be a regular file")
    actual_sha256 = sha256_file(path)
    if actual_sha256 != expected_sha256.lower():
        fail(f"{label} digest mismatch")
    return {"path": str(path), "sha256": actual_sha256}


def _expected_adapter_evidence(plan: dict[str, Any]) -> dict[str, Any]:
    """Build the immutable evidence contract copied into each host receipt.

    The plan's signed-gate summaries are the authority.  Host adapters may
    report observations, but they cannot substitute a different trusted file
    or a different validator identity.
    """
    proof = plan.get("proof")
    if not isinstance(proof, dict):
        fail("plan signed evidence is missing")
    raw_identities = proof.get("identity_receipts")
    raw_proof = proof.get("sequencer_rebuild_proof")
    if not isinstance(raw_identities, list) or not isinstance(raw_proof, dict):
        fail("plan signed evidence summaries are missing")
    identities: list[dict[str, Any]] = []
    for summary in raw_identities:
        if not isinstance(summary, dict):
            fail("plan identity evidence summary is malformed")
        bound_file = _bound_regular_file(summary.get("path"), summary.get("sha256"), "plan identity evidence")
        role = summary.get("role")
        node_id = summary.get("node_id")
        peer_id = summary.get("peer_id")
        if role not in MUTATION_ORDER or not isinstance(node_id, str) or not node_id.strip() or not isinstance(peer_id, str) or not peer_id.strip():
            fail("plan identity evidence role/node/peer binding is incomplete")
        metadata_keys = ("key_path", "key_sha256", "key_size_bytes", "key_mode", "key_uid", "key_gid")
        metadata_present = {key for key in metadata_keys if key in summary}
        if metadata_present and metadata_present != set(metadata_keys):
            fail("plan identity evidence metadata tuple is incomplete")
        identity = {**bound_file, "role": role, "node_id": node_id, "peer_id": peer_id}
        if metadata_present:
            identity.update(_validate_identity_receipt_metadata(summary, "plan identity evidence", role))
        identities.append(identity)
    if {item["role"] for item in identities} != set(MUTATION_ORDER) or len(identities) != len(MUTATION_ORDER):
        fail("plan identity evidence must contain each canonical validator role exactly once")
    identities.sort(key=lambda item: MUTATION_ORDER.index(item["role"]))
    raw_proof_path_value = proof.get("sequencer_rebuild_proof_path")
    verification_path_value = proof.get("sequencer_rebuild_proof_verification_path")
    if not isinstance(raw_proof_path_value, str) or not raw_proof_path_value.strip():
        fail("plan signed 204 raw proof path is required")
    if verification_path_value:
        raw_proof_sha256 = raw_proof.get("proof_sha256")
        verification_sha256 = raw_proof.get("sha256")
    else:
        raw_proof_sha256 = raw_proof.get("sha256")
        verification_sha256 = None
    proof_file = _bound_regular_file(raw_proof_path_value, raw_proof_sha256, "plan signed 204 rebuild proof")
    proof_role = raw_proof.get("role")
    proof_node_id = raw_proof.get("node_id")
    proof_peer_id = raw_proof.get("peer_id")
    if proof_role != "sequencer-204" or not isinstance(proof_node_id, str) or not proof_node_id.strip() or not isinstance(proof_peer_id, str) or not proof_peer_id.strip():
        fail("plan signed 204 rebuild proof identity binding is incomplete")
    verification: dict[str, str] | None = None
    if verification_path_value:
        verification = _bound_regular_file(
            verification_path_value,
            verification_sha256,
            "plan signed 204 rebuild proof verification receipt",
        )
    proof_binding: dict[str, Any] = {
        **proof_file,
        "verification_path": verification["path"] if verification else None,
        "verification_sha256": verification["sha256"] if verification else None,
        "role": proof_role,
        "node_id": proof_node_id,
        "peer_id": proof_peer_id,
    }
    for key in ("signer_id", "signed_payload_sha256", "proof_sha256"):
        if key in raw_proof:
            proof_binding[key] = raw_proof[key]
    return {"identity_receipts": identities, "sequencer_rebuild_proof": proof_binding}


def _build_adapter_binding(plan: dict[str, Any], phase: str, phase_window_started_at: str) -> dict[str, Any]:
    plan_digest = plan.get("plan_digest")
    transaction_id = plan.get("transaction_id")
    if not isinstance(plan_digest, str) or len(plan_digest) != 64:
        fail("adapter binding requires the immutable plan digest")
    if not isinstance(transaction_id, str) or not transaction_id.strip():
        fail("adapter binding requires the transaction id")
    _parse_timestamp(phase_window_started_at, "adapter phase window")
    return {
        "schema_version": "oasis7.validator_pair_rebuild_adapter_binding.v1",
        "plan_digest": plan_digest,
        "transaction_id": transaction_id,
        "phase": phase,
        "phase_window_started_at": phase_window_started_at,
        "repository_executable": repository_executable_identity(),
        "evidence_bindings": _expected_adapter_evidence(plan),
    }


def _validate_adapter_binding(receipt: dict[str, Any], plan: dict[str, Any], phase: str) -> None:
    binding = plan.get("adapter_binding")
    if not isinstance(binding, dict) or binding.get("schema_version") != "oasis7.validator_pair_rebuild_adapter_binding.v1":
        fail("host adapter binding was not persisted before invocation")
    if binding.get("plan_digest") != plan.get("plan_digest") or binding.get("transaction_id") != plan.get("transaction_id") or binding.get("phase") != phase:
        fail("host adapter binding identity mismatch")
    if binding.get("repository_executable") != repository_executable_identity():
        fail("host adapter binding producer is not the repository-owned executor")
    expected_evidence = _expected_adapter_evidence(plan)
    if binding.get("evidence_bindings") != expected_evidence:
        fail("persisted host adapter binding no longer matches plan evidence")
    if receipt.get("plan_digest") != binding["plan_digest"] or receipt.get("transaction_id") != binding["transaction_id"] or receipt.get("phase") != binding["phase"]:
        fail("host adapter receipt plan/transaction/phase binding mismatch")
    if receipt.get("evidence_bindings") != expected_evidence:
        fail("host adapter receipt evidence binding differs from the persisted plan evidence")
    if receipt.get("repository_executable") != binding["repository_executable"]:
        fail("host adapter receipt producer binding mismatch")
    captured_at = _parse_timestamp(receipt.get("captured_at"), "host adapter captured_at")
    phase_started = _parse_timestamp(binding["phase_window_started_at"], "adapter phase window")
    now = dt.datetime.now(dt.timezone.utc)
    if phase_started > now + dt.timedelta(seconds=5) or now - phase_started > dt.timedelta(seconds=300):
        fail("host adapter phase window is stale or in the future")
    if captured_at < phase_started or captured_at > now + dt.timedelta(seconds=5) or now - captured_at > dt.timedelta(seconds=300):
        fail("host adapter receipt freshness is outside the current phase window")


def apply_transaction(
    path: Path,
    host_adapter: Path | None = None,
    direct_args: argparse.Namespace | None = None,
) -> dict[str, Any]:
    plan = load_json(path, "transaction")
    if plan.get("schema_version") != PLAN_SCHEMA or plan.get("phase") != "planned":
        fail("apply requires a planned validator-pair rebuild plan")
    expected_plan_digest = hashlib.sha256(
        json.dumps({key: value for key, value in plan.items() if key != "plan_digest"}, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    if plan.get("plan_digest") != expected_plan_digest:
        fail("plan digest mismatch")
    if host_adapter is None:
        fail("apply requires a governed host adapter for startup and health gates")
    proof = plan.get("proof")
    if not isinstance(proof, dict) or proof.get("direct_reobserve_required") is not True:
        fail("apply requires live human_direct_ssh re-observation; persisted proof is audit-only")
    request_value = getattr(direct_args, "human_direct_ssh_request", None) if direct_args is not None else None
    known_hosts_value = getattr(direct_args, "known_hosts", None) if direct_args is not None else None
    transaction: dict[str, Any] = dict(plan)
    # The wrapper's apply form historically receives only the transaction.
    # Recover the executor-bound routing paths recorded by the plan, while
    # still requiring a new live authority and direct SSH observation below.
    if not isinstance(request_value, str) or not request_value.strip():
        request_value = transaction.get("live_ssh_request_path") or proof.get("direct_request_path")
    if not isinstance(known_hosts_value, str) or not known_hosts_value.strip():
        known_hosts_value = transaction.get("live_ssh_known_hosts_path") or proof.get("known_hosts_path")
    if not isinstance(request_value, str) or not request_value.strip() or not isinstance(known_hosts_value, str) or not known_hosts_value.strip():
        fail("apply requires live human_direct_ssh re-observation before mutation")
    if direct_args is None:
        direct_args = argparse.Namespace(
            human_direct_ssh_request=request_value,
            known_hosts=known_hosts_value,
            credential_env=None,
            credential_fd=None,
        )
    else:
        direct_args.human_direct_ssh_request = request_value
        direct_args.known_hosts = known_hosts_value
    requested_path = _direct_regular_file(request_value, "apply human_direct_ssh request")
    if requested_path != Path(proof.get("direct_request_path", "")).expanduser().resolve() or sha256_file(requested_path) != proof.get("direct_request_sha256"):
        fail("apply direct request is not the request bound into the plan")
    direct_receipt = _direct_reobserve_from_args(direct_args)
    stopped_quiescence = proof.get("stopped_quiescence")
    expected_request_digest = stopped_quiescence.get("request_digest") if isinstance(stopped_quiescence, dict) else None
    if direct_receipt.get("quiescence_id") != proof.get("quiescence_id") or direct_receipt.get("request_digest") != expected_request_digest:
        fail("apply direct re-observation binding differs from the planned request")
    impact_path = Path(plan.get("proof", {}).get("consumer_impact_record_path", "")).resolve()
    if direct_receipt.get("impact_record_sha256") != sha256_file(impact_path) or direct_receipt.get("impact") != plan.get("consumer_impact", {}).get("impact"):
        fail("apply direct re-observation consumer-impact binding mismatch")
    helper = load_provenance_helper()
    package = plan.get("package")
    try:
        helper.validate_receipt(Path(package["provenance"]), Path(package["directory"]), Path(plan["provenance"]["trusted_root"]["path"]))
    except SystemExit as error:
        fail(str(error).removeprefix("error: validator-pair provenance: "))
    transaction = dict(plan)
    transaction["schema_version"] = SCHEMA
    transaction["transaction_id"] = f"pair-rebuild-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}-{uuid.uuid4().hex[:10]}"
    transaction["created_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    transaction["phase"] = "prepared"
    transaction["direct_quiescence_observation"] = {
        "provider": "executor-owned-direct-ssh",
        "audit_only": True,
        "quiescence_id": direct_receipt["quiescence_id"],
        "request_digest": direct_receipt["request_digest"],
        "nonce": direct_receipt["nonce"],
        "captured_at": direct_receipt["captured_at"],
        "deployment_inventory_sha256": direct_receipt["deployment_inventory_sha256"],
        "nodes": direct_receipt["nodes"],
    }
    transaction["canonical_digest"] = canonical_digest(transaction)
    write_json(path, transaction)
    backups: dict[str, Any] = transaction.get("backup") if isinstance(transaction.get("backup"), dict) else {}
    staged: dict[str, Any] = transaction.get("staged") if isinstance(transaction.get("staged"), dict) else {}
    try:
        return _continue_transaction(transaction, path, host_adapter, direct_args, fresh_direct_observation=False)
    except BaseException as error:
        backups = transaction.get("backup") if isinstance(transaction.get("backup"), dict) else {}
        staged = transaction.get("staged") if isinstance(transaction.get("staged"), dict) else {}
        if not backups and not staged and not isinstance(transaction.get("preflight_receipt"), dict):
            if isinstance(error, SystemExit):
                raise
            fail(str(error))
        transaction["backup"] = backups
        transaction["staged"] = staged
        transaction["phase"] = "rollback_required"
        transaction["failure"] = str(error)
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        rollback_errors: list[str] = []
        try:
            # A gate failure can occur after one or both local roots have
            # been mutated.  Re-authorize the live human stop boundary and
            # re-observe both fixed validators before rollback or restore.
            rollback_direct_receipt = _rollback_direct_reobserve(transaction, direct_args)
            transaction["rollback_direct_quiescence_observation"] = _direct_observation_record(
                rollback_direct_receipt, reauthorized=True
            )
            transaction["canonical_digest"] = canonical_digest(transaction)
            write_json(path, transaction)
        except BaseException as rollback_direct_error:
            rollback_errors.append(f"rollback direct re-observation: {rollback_direct_error}")
        try:
            if not rollback_errors:
                transaction["rollback_receipt"] = run_host_adapter(host_adapter, path, transaction, "rollback")
                transaction.pop("adapter_callback", None)
        except BaseException as rollback_callback_error:
            rollback_errors.append(f"host-adapter: {rollback_callback_error}")
        if not rollback_errors:
            try:
                rollback_post_callback_receipt = _rollback_direct_reobserve(transaction, direct_args)
                transaction["rollback_post_callback_direct_quiescence_observation"] = _direct_observation_record(
                    rollback_post_callback_receipt, reauthorized=True
                )
                transaction["canonical_digest"] = canonical_digest(transaction)
                write_json(path, transaction)
            except BaseException as rollback_post_callback_error:
                rollback_errors.append(f"rollback post-callback direct re-observation: {rollback_post_callback_error}")
        if not rollback_errors:
            for role in reversed(MUTATION_ORDER):
                if role in backups:
                    try:
                        restore_node(backups[role], transaction.get("transaction_id"))
                    except BaseException as rollback_error:
                        rollback_errors.append(f"{role}: {rollback_error}")
        transaction["phase"] = "rollback_failed" if rollback_errors else "rolled_back"
        transaction["rollback"] = {
            "strategy": "same-filesystem-full-snapshot",
            "status": "failed" if rollback_errors else "verified",
            "errors": rollback_errors,
        }
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        if rollback_errors:
            fail("rollback failed: " + "; ".join(rollback_errors))
        if isinstance(error, SystemExit):
            raise
        fail(str(error))


def rollback_transaction(
    path: Path,
    host_adapter: Path | None = None,
    direct_args: argparse.Namespace | None = None,
) -> dict[str, Any]:
    transaction = load_json(path, "transaction")
    if transaction.get("schema_version") != SCHEMA:
        fail("unsupported transaction schema")
    if transaction.get("canonical_digest") != canonical_digest(transaction):
        fail("transaction canonical digest mismatch")
    backups = transaction.get("backup")
    if not isinstance(backups, dict):
        fail("transaction does not contain full backup receipts")
    if host_adapter is None:
        fail("rollback requires a governed host adapter rollback callback")
    _validate_persisted_backup_refs(transaction)
    callback_state = _validate_adapter_callback_state(transaction)
    if callback_state is not None and callback_state["status"] == "in_flight":
        fail("host adapter callback is ambiguous in-flight; explicit governed reconciliation is required")
    if callback_state is not None and callback_state["status"] == "failed" and callback_state["phase"] == "rollback":
        fail("rollback host adapter callback previously failed; explicit governed reconciliation is required")
    completed_rollback_callback = (
        callback_state is not None
        and callback_state["status"] == "completed"
        and callback_state["phase"] == "rollback"
    )
    transaction["phase"] = "rollback_required"
    transaction["canonical_digest"] = canonical_digest(transaction)
    write_json(path, transaction)
    try:
        if completed_rollback_callback:
            transaction["rollback_receipt"] = callback_state["receipt"]
            transaction.pop("adapter_callback", None)
            transaction["canonical_digest"] = canonical_digest(transaction)
            write_json(path, transaction)
        elif isinstance(transaction.get("rollback_receipt"), dict):
            validate_host_receipt(transaction["rollback_receipt"], transaction, "rollback")
        else:
            rollback_direct_receipt = _rollback_direct_reobserve(transaction, direct_args)
            transaction["rollback_direct_quiescence_observation"] = _direct_observation_record(
                rollback_direct_receipt, reauthorized=True
            )
            transaction["canonical_digest"] = canonical_digest(transaction)
            write_json(path, transaction)
            transaction["rollback_receipt"] = run_host_adapter(host_adapter, path, transaction, "rollback")
            transaction.pop("adapter_callback", None)
        rollback_post_callback_receipt = _rollback_direct_reobserve(transaction, direct_args)
        transaction["rollback_post_callback_direct_quiescence_observation"] = _direct_observation_record(
            rollback_post_callback_receipt, reauthorized=True
        )
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        results: dict[str, Any] = {}
        for role in reversed(MUTATION_ORDER):
            if role not in backups:
                fail(f"backup missing for {role}")
            results[role] = restore_node(backups[role], transaction.get("transaction_id"))
        transaction["phase"] = "rolled_back"
        transaction["rollback"] = {"strategy": "same-filesystem-full-snapshot", "status": "verified", "results": results}
        transaction["rolled_back_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        return transaction
    except BaseException as error:
        transaction["phase"] = "rollback_failed"
        transaction["failure"] = str(error)
        transaction["rollback"] = {"strategy": "same-filesystem-full-snapshot", "status": "failed", "errors": [str(error)]}
        transaction["canonical_digest"] = canonical_digest(transaction)
        write_json(path, transaction)
        if isinstance(error, SystemExit):
            raise
        fail(str(error))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    sub = root.add_subparsers(dest="mode", required=True)
    plan = sub.add_parser("plan")
    plan.add_argument("--package-dir", required=True)
    plan.add_argument("--provenance", required=True)
    plan.add_argument("--trust-root", required=True)
    plan.add_argument("--consumer-impact-record", required=True)
    plan.add_argument("--stopped-quiescence-proof")
    plan.add_argument("--quiescence-id", "--quiescence-transaction-id", dest="quiescence_id", required=True)
    plan.add_argument("--capacity-json", required=True)
    plan.add_argument("--node", action="append", required=True)
    plan.add_argument("--storage-health-url")
    plan.add_argument("--sequencer-health-url")
    plan.add_argument("--sequencer-proof-url", required=True)
    plan.add_argument("--observer-receipt")
    plan.add_argument("--identity-receipts", required=True)
    plan.add_argument("--sequencer-rebuild-proof", required=True)
    plan.add_argument("--sequencer-rebuild-proof-verification")
    plan.add_argument("--sequencer-proof-verifier")
    plan.add_argument("--human-direct-ssh-request", "--direct-request", dest="human_direct_ssh_request")
    plan.add_argument("--known-hosts")
    plan.add_argument("--credential-env")
    plan.add_argument("--credential-fd", type=int)
    plan.add_argument("--out-dir")
    quiesce = sub.add_parser("quiesce")
    quiesce.add_argument("--consumer-impact-record", required=True)
    quiesce.add_argument("--quiescence-id", "--quiescence-transaction-id", dest="quiescence_id", required=True)
    quiesce.add_argument("--node", action="append", required=True)
    quiesce.add_argument("--host-adapter", required=True)
    quiesce.add_argument("--out-dir", required=True)
    direct = sub.add_parser("human_direct_ssh")
    direct.add_argument("--request", required=True)
    direct.add_argument("--known-hosts", required=True)
    direct.add_argument("--out-dir", required=True)
    direct.add_argument("--host-adapter")
    direct.add_argument("--credential-env")
    direct.add_argument("--credential-fd", type=int)
    apply = sub.add_parser("apply")
    apply.add_argument("--transaction", required=True)
    apply.add_argument("--host-adapter")
    apply.add_argument("--human-direct-ssh-request", "--direct-request", "--request", dest="human_direct_ssh_request")
    apply.add_argument("--known-hosts")
    apply.add_argument("--credential-env")
    apply.add_argument("--credential-fd", type=int)
    rollback = sub.add_parser("rollback")
    rollback.add_argument("--transaction", required=True)
    rollback.add_argument("--host-adapter", required=True)
    rollback.add_argument("--human-direct-ssh-request", "--direct-request", "--request", dest="human_direct_ssh_request")
    rollback.add_argument("--known-hosts")
    rollback.add_argument("--credential-env")
    rollback.add_argument("--credential-fd", type=int)
    resume = sub.add_parser("resume")
    resume.add_argument("--transaction", required=True)
    resume.add_argument("--host-adapter")
    resume.add_argument("--human-direct-ssh-request", "--direct-request", "--request", dest="human_direct_ssh_request")
    resume.add_argument("--known-hosts")
    resume.add_argument("--credential-env")
    resume.add_argument("--credential-fd", type=int)
    return root


def main() -> int:
    args = parser().parse_args()
    if args.mode == "human_direct_ssh":
        run_human_direct_ssh(args)
    elif args.mode == "quiesce":
        run_quiescence_phase(args)
    elif args.mode == "plan":
        plan = build_plan(args)
        if args.out_dir:
            write_json(Path(args.out_dir).resolve() / "transaction.json", plan)
        print(json.dumps(plan, ensure_ascii=True, sort_keys=True))
    elif args.mode == "apply":
        host_adapter = Path(args.host_adapter).expanduser() if args.host_adapter else None
        print(json.dumps(apply_transaction(Path(args.transaction).resolve(), host_adapter, args), ensure_ascii=True, sort_keys=True))
    elif args.mode == "resume":
        host_adapter = Path(args.host_adapter).expanduser() if args.host_adapter else None
        print(json.dumps(resume_transaction(Path(args.transaction).resolve(), host_adapter, args), ensure_ascii=True, sort_keys=True))
    else:
        print(json.dumps(rollback_transaction(Path(args.transaction).resolve(), Path(args.host_adapter).expanduser(), args), ensure_ascii=True, sort_keys=True))
    return 0


if __name__ == "__main__":
    main()
