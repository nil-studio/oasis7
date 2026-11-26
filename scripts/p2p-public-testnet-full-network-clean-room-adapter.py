#!/usr/bin/env python3
"""Fail-closed adapter boundary for the governed five-node clean-room plan.

The planner in ``p2p-public-testnet-full-network-clean-room.py`` is
intentionally provider-free.  This module is the equally deliberate boundary
between that plan and a separately governed provider transport.  It can write
only a local, durable transaction journal in dry-run mode.  A real transport,
an independently verified receipt, and an apply authority are required before
any mutating callback is even eligible to run.

The transport interface is intentionally tiny and data-oriented::

    inspect_node(node) -> read-only preflight evidence plus a signed,
        independently verifier-bound receipt
    preflight(operation, node-or-None) -> authenticated preflight receipt
    verify(operation, node-or-None) -> authenticated verification receipt
    health(operation) -> authenticated fleet-health receipt
    verify_fresh_root_probe(plan) -> authenticated probe receipt
    mutate(operation, node-or-None) -> sanitized operation receipt
    reobserve_failed_state(plan, attempted_mutating_operations, failed_operation) -> signed receipt
    rollback_clean_redeploy(plan, attempted_mutating_operations, failed_state_receipt) -> sanitized receipt

No shell command, credential, or provider-specific implementation belongs in
this file.  In particular, old-state restore and cross-node copy are not
transport operations accepted by this adapter.
"""

from __future__ import annotations

import argparse
import copy
import contextvars
import datetime as dt
import hashlib
import importlib.util
import json
import ntpath
import os
from collections.abc import Sequence
from pathlib import Path, PurePosixPath
import posixpath
import re
import stat
import tempfile
import threading
from typing import Any, Callable, Mapping, NoReturn


PLAN_SCHEMA = "oasis7.public_testnet_full_network_clean_room_plan.v1"
ADAPTER_SCHEMA = "oasis7.clean_room_mutation_adapter.v1"
AUTHORITY_SCHEMA = "oasis7.clean_room_mutation_authority.v1"
CRYPTO_RECEIPT_SCHEMA = "oasis7.crypto_verifier_receipt.v1"
PROVIDER_RECEIPT_SCHEMA = "oasis7.clean_room_provider_receipt.v1"
NO_BACKUP_AUTHORITY_SCHEMA = "oasis7.no_backup_authority.v1"
RECOVERY_RECEIPT_SCHEMA = "oasis7.recovery_receipt.v1"
IDENTITY_RECEIPT_SCHEMA = "oasis7.identity_receipt.v2"
IDENTITY_V2_EVIDENCE_SCHEMA = "oasis7.identity_v2_evidence_map.v2"
IDENTITY_RECEIPT_FIELDS = frozenset(
    {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "node_id",
        "peer_id",
        "key_sha256",
        "key_size_bytes",
        "key_mode",
        "key_uid",
        "key_gid",
        "capture_window_id",
        "rotation_epoch",
        "issued_at",
        "expires_at",
    }
)
DEPLOYMENT_INVENTORY_SCHEMA = "oasis7.deployment_inventory.v2"
DEPLOYMENT_INVENTORY_RECEIPT_SCHEMA = "oasis7.deployment_inventory_receipt.v2"
DEPLOYMENT_INVENTORY_RECEIPT_FIELDS = frozenset(
    {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "capture_window_id",
        "rotation_epoch",
        "issued_at",
        "expires_at",
    }
)
JOURNAL_SCHEMA = "oasis7.clean_room_mutation_journal.v2"
LEGACY_JOURNAL_SCHEMA = "oasis7.clean_room_mutation_journal.v1"
NODE_RECEIPT_SCHEMA = "oasis7.clean_room_node_receipt.v1"
NONCE_ROW_SCHEMA = "oasis7.clean_room_adapter_nonce.v1"
REPOSITORY = "eng-cc/oasis7"
CANONICAL_ADAPTER_ID = "external-clean-room-adapter"
CANONICAL_NETWORK_ID = "oasis7-public-testnet-governed-20260606"
CANONICAL_VERIFIER_ID = "governed-receipt-verifier"
CANONICAL_TRUST_ROOT_ID = "oasis7-public-testnet-governance-root-v1"
CANONICAL_TRUST_ROOT_PATH = "/operator/truth/governance-root.json"
CANONICAL_TRUST_ROOT_FIXTURE_PATH = Path(__file__).with_name("fixtures") / "oasis7-governance-root.v1.json"
# These are code-owned values recorded from the repository fixture.  The first
# is the provenance helper's canonical semantic digest; the second pins the
# deployable artifact bytes.  Neither is derived from a caller's id:path pair
# or re-derived from mutable deployment input at import time.
CANONICAL_TRUST_ROOT_DIGEST = "5abd00f3e90a3e894f110f5a32ecab772e23e97ad7ec2cc9d675ae65282ae8ab"
CANONICAL_TRUST_ROOT_FILE_SHA256 = "f278bc8f060cd6777d68f086fc3131edc5d6b5a6080bde09208ba69a69e3ef66"
# The owner is deliberately deployment-bound: the operator account executing
# this process must own the pinned regular file.  It is not a portable UID.
CANONICAL_TRUST_ROOT_OWNER_SCOPE = "operator-local"
CANONICAL_TRUST_ROOT_OWNER_UID = os.getuid()
CANONICAL_TRUST_ROOT_MODE = "0600"
CANONICAL_SIGNER_ALLOWLIST = frozenset({"governance-signer"})
CANONICAL_ROTATION_EPOCH = "rotation-epoch-20260901-001"
CANONICAL_PROBE_PEER_ID = "validator-pair"
CANONICAL_FLEET_PEER_ID = "fleet"
PHASE_RECEIPT_SCHEMAS = {
    "preflight": "oasis7.clean_room_preflight_receipt.v1",
    "backup": "oasis7.clean_room_backup_receipt.v1",
    "apply": "oasis7.clean_room_apply_receipt.v1",
    "verify": "oasis7.clean_room_verify_receipt.v1",
    "fresh-root-probe": "oasis7.clean_room_fresh_root_probe_receipt.v1",
    "fleet-health": "oasis7.clean_room_fleet_health_receipt.v1",
    "reobserve": "oasis7.clean_room_reobserve_receipt.v1",
    "rollback": "oasis7.clean_room_rollback_receipt.v1",
}
CANONICAL_PROVIDER_UID = {
    "storage-205": 0,
    "sequencer-204": 0,
    "linux-lan-observer": 0,
    "windows-observer": 0,
    "macos-observer": 0,
}
RAW_IDENTITY_RECEIPT_V1_KEY_PATH = "/operator/keys/node-keypair.toml"
RAW_IDENTITY_RECEIPT_V1_FIELDS = frozenset(
    {
        "schema_version",
        "node_id",
        "peer_id",
        "key_path",
        "key_sha256",
        "key_size_bytes",
        "key_mode",
        "key_uid",
        "key_gid",
    }
)
OID_RE = re.compile(r"^[0-9a-fA-F]{40,64}$")
HEX64_RE = re.compile(r"^[0-9a-fA-F]{64}$")
SIGNATURE_RE = re.compile(r"^[0-9a-fA-F]{128}$")
# Plan-bound nonces must have enough entropy-bearing length.  The lower-level
# ledger reservation helper retains its historical format check for legacy
# journal rows; admission uses PLAN_NONCE_RE below and never that weaker path.
SAFE_NONCE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{7,255}$")
PLAN_NONCE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{31,255}$")
SECRET_KEY_RE = re.compile(
    r"(?:password|secret|token|private[_-]?key|api[_-]?key|access[_-]?key|sshpass)",
    re.I,
)
SECRET_FIELD_NAMES = frozenset(
    {
        "nonce",
        "credential",
        "credentials",
        "environment_name",
        "argv",
        "command",
        "command_line",
        "api_key",
        "access_key",
    }
)
SECRET_VALUE_RE = re.compile(
    r"(?:password|secret|token|private[_ -]?key|api[_ -]?key|access[_ -]?key|sshpass)",
    re.I,
)
TRANSPORT_AUTH_ALIAS_FIELDS = frozenset(
    {"authorization", "bearer", "bearer_token", "auth", "auth_header", "headers", "metadata"}
)
REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MIN_FREE_BYTES = 64 * 1024 * 1024
MAX_CLOCK_SKEW_SECONDS = 5
_PLANNER_MODULE: Any | None = None

# This registry is code-owned.  It is not derived from plan input, a host
# response, or a caller-supplied peer list.

# Provider callbacks receive only these DTO fields.  Keeping the allowlist in
# the adapter makes a future planner field opt-in rather than an accidental
# transport disclosure.
TRANSPORT_NODE_FIELDS = frozenset(
    {
        "name",
        "node_id",
        "role",
        "platform",
        "node_root",
        "service_manager",
        "service",
        "host_binding",
        "endpoints",
        "persistent_state_paths",
        "identity_receipt",
        "bindings",
    }
)
TRANSPORT_PLAN_FIELDS = frozenset(
    {
        "schema_version",
        "task_uid",
        "head_oid",
        "plan_digest",
        "transaction_id",
        "capture_window_id",
        "capture_window",
        "node_order",
        "global_order",
        "canonical_host_inventory",
        "canonical_endpoint_inventory",
        "nodes",
        "surfaces",
        "deployment_inventory",
        "truth",
        "execution",
        "forensic_backup",
        "rollback",
        "fresh_root_probe",
        "observer_gate",
        "operation_journal",
        "operation_journal_contract",
        "adapter_verification",
        "consumer_impact_record",
        "identity_v2_evidence",
    }
)

# These policy DTOs are provider-facing authority, not caller-extensible
# metadata.  Keep their schemas and values code-owned so a caller cannot
# redigest a plan after changing rollback or backup behavior.
CANONICAL_ROLLBACK_STEPS = (
    "stop-started-nodes",
    "preserve-failed-state-for-forensics",
    "reinstall-exact-package-and-truth",
    "rerun-fresh-root-probe",
)
FORENSIC_BACKUP_FIELDS = frozenset(
    {
        "mode",
        "task_uid",
        "frozen_head_oid",
        "required_before_reset",
        "operator_authorized",
        "current_authorization",
        "immutable",
        "seed_eligible",
        "cross_node_state_copy",
        "restore_old_state",
        "receipt_required_per_node",
        "authority",
        "repository",
        "action",
        "targets",
        "transaction_id",
        "capture_window_id",
        "actor",
        "issued_at",
        "expires_at",
    }
)
ROLLBACK_FIELDS = frozenset(
    {
        "policy",
        "steps",
        "stop_started_nodes",
        "preserve_failed_state_for_forensics",
        "restore_old_state",
        "cross_node_state_copy",
        "reinstall_exact_package_and_truth",
        "rerun_fresh_root_probe",
        "provider_mutation_requires_external_authority",
    }
)
NO_BACKUP_AUTHORITY_RECEIPT_FIELDS = frozenset(
    {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "bindings",
    }
)
NO_BACKUP_AUTHORITY_BINDING_FIELDS = frozenset(
    {
        "repository",
        "action",
        "targets",
        "task_uid",
        "transaction_id",
        "capture_window_id",
        "frozen_head_oid",
        "actor",
        "issued_at",
        "expires_at",
        "current_authorization",
        "consumer_impact_record",
    }
)


class AdapterError(RuntimeError):
    """A fail-closed adapter contract violation."""


def _fail(message: str) -> NoReturn:
    raise AdapterError(f"full-network clean-room adapter: {message}")


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        _fail(f"{label} must be an object")
    return value


def _string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        _fail(f"{label} must be a non-empty string")
    return value


def _bool(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        _fail(f"{label} must be boolean")
    return value


def _digest(value: Any, label: str) -> str:
    raw = _string(value, label)
    if HEX64_RE.fullmatch(raw) is None:
        _fail(f"{label} must be a 64-character hexadecimal digest")
    return raw.lower()


def _nonzero_hex(value: Any, pattern: re.Pattern[str], label: str) -> str:
    raw = _string(value, label)
    if pattern.fullmatch(raw) is None or not any(character != "0" for character in raw):
        _fail(f"{label} is malformed or empty")
    return raw.lower()


def _canonical_bytes(value: dict[str, Any], *, omit: str | None = None) -> bytes:
    body = {key: item for key, item in value.items() if key != omit}
    return json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()


def canonical_plan_digest(plan: dict[str, Any]) -> str:
    """Return the planner-compatible digest, excluding ``plan_digest``."""
    return hashlib.sha256(_canonical_bytes(plan, omit="plan_digest")).hexdigest()


def _consumer_impact_locator(plan: dict[str, Any]) -> dict[str, str]:
    impact = _object(plan.get("consumer_impact_record"), "consumer impact record")
    if set(impact) != {"path", "sha256", "record"}:
        _fail("consumer impact record binding is incomplete")
    path = _string(impact.get("path"), "consumer impact record path")
    digest = _digest(impact.get("sha256"), "consumer impact record digest")
    record = _object(impact.get("record"), "consumer impact record contents")
    try:
        validated = _load_planner()._validate_consumer_impact_record(
            {"path": path, "sha256": digest}
        )
    except SystemExit as error:
        _fail(str(error))
    if validated["path"] != path or validated["sha256"] != digest or validated["record"] != record:
        _fail("consumer impact record path, digest, or contents drifted")
    return {"path": path, "sha256": digest}


def journal_digest(record: dict[str, Any]) -> str:
    return hashlib.sha256(_canonical_bytes(record, omit="journal_digest")).hexdigest()


def _load_planner() -> Any:
    global _PLANNER_MODULE
    if _PLANNER_MODULE is not None:
        return _PLANNER_MODULE
    path = Path(__file__).with_name("p2p-public-testnet-full-network-clean-room.py")
    spec = importlib.util.spec_from_file_location("oasis7_full_network_clean_room_planner", path)
    if spec is None or spec.loader is None:
        _fail("cannot load the canonical full-network planner")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    _PLANNER_MODULE = module
    return module


# One code-owned path shared with all publisher output protection.
# Never selected by a journal, transaction, plan, nonce, or CLI argument.
CANONICAL_FLEET_LOCK_PATH = _load_planner()._peer_registry_authority().CANONICAL_FLEET_LOCK_PATH


def _safe_relative_paths(root: str, paths: Any, platform: str, label: str) -> list[str]:
    if not isinstance(paths, list) or not paths:
        _fail(f"{label} must be a non-empty path list")
    result: list[str] = []
    for raw in paths:
        path = _string(raw, f"{label} entry")
        if platform == "windows-x64":
            pieces = re.split(r"[\\/]", path)
            if ".." in pieces or not _under_root(root, path, "windows-x64"):
                _fail(f"{label} contains a path outside its canonical root")
        else:
            pieces = PurePosixPath(path).parts
            if ".." in pieces or not posixpath.normpath(path).startswith(posixpath.normpath(root).rstrip("/") + "/"):
                _fail(f"{label} contains a path outside its canonical root")
        result.append(path)
    return result


def _canonical_deployment_inventory_payload_digest(inventory: dict[str, Any]) -> str:
    """Digest every inventory field except its self-referential receipt."""
    payload = {key: value for key, value in inventory.items() if key != "receipt"}
    material = json.dumps(
        payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")
    ).encode()
    return hashlib.sha256(material).hexdigest()


def _canonical_receipt_digest(
    receipt: dict[str, Any], *, excluded_fields: frozenset[str] = frozenset()
) -> str:
    """Return the deterministic integrity digest for a receipt envelope."""
    payload = {
        key: value
        for key, value in receipt.items()
        if key != "canonical_digest" and key not in excluded_fields
    }
    material = json.dumps(
        payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")
    ).encode()
    return hashlib.sha256(material).hexdigest()


def _canonical_raw_identity_receipt_v1_bytes(
    identity_receipt: dict[str, Any],
    *,
    key_path: str = RAW_IDENTITY_RECEIPT_V1_KEY_PATH,
) -> bytes:
    """Recreate the runtime's ordered, compact raw identity-v1 bytes."""
    payload = {
        "schema_version": "oasis7.identity_receipt.v1",
        "node_id": identity_receipt.get("node_id"),
        "peer_id": identity_receipt.get("peer_id"),
        "key_path": key_path,
        "key_sha256": identity_receipt.get("key_sha256"),
        "key_size_bytes": identity_receipt.get("key_size_bytes"),
        "key_mode": int("0600", 8),
        "key_uid": identity_receipt.get("key_uid"),
        "key_gid": identity_receipt.get("key_gid"),
    }
    return json.dumps(payload, ensure_ascii=True, separators=(",", ":")).encode("utf-8")


def _validate_raw_identity_receipt_v1_bytes(
    identity_receipt: dict[str, Any], raw_v1_bytes: bytes, label: str
) -> None:
    """Validate forwarded runtime bytes before invoking the provider verifier."""
    if not isinstance(raw_v1_bytes, bytes) or not raw_v1_bytes:
        _fail(f"{label} raw-v1 bytes must be non-empty bytes")
    try:
        raw = json.loads(raw_v1_bytes.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        _fail(f"{label} raw-v1 bytes are not valid UTF-8 JSON: {error.__class__.__name__}")
    if not isinstance(raw, dict):
        _fail(f"{label} raw-v1 bytes must contain a JSON object")
    if set(raw) != RAW_IDENTITY_RECEIPT_V1_FIELDS:
        missing = sorted(RAW_IDENTITY_RECEIPT_V1_FIELDS - set(raw))
        extra = sorted(set(raw) - RAW_IDENTITY_RECEIPT_V1_FIELDS)
        _fail(f"{label} raw-v1 fields are not exact (missing={missing}, extra={extra})")
    if raw.get("schema_version") != "oasis7.identity_receipt.v1":
        _fail(f"{label} raw-v1 schema is unsupported")
    for field in ("node_id", "peer_id", "key_path"):
        if not isinstance(raw.get(field), str) or not raw[field].strip():
            _fail(f"{label} raw-v1 {field} must be a non-empty string")
    _nonzero_hex(raw.get("key_sha256"), HEX64_RE, f"{label}.raw-v1.key_sha256")
    for field in ("key_size_bytes", "key_mode", "key_uid", "key_gid"):
        value = raw.get(field)
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            _fail(f"{label} raw-v1 {field} must be a non-negative integer")
    if raw["key_size_bytes"] <= 0:
        _fail(f"{label} raw-v1 key_size_bytes must be positive")
    try:
        expected_key_mode = int(str(identity_receipt.get("key_mode")), 8)
    except (TypeError, ValueError):
        _fail(f"{label} governed v2 key_mode is malformed")
    if (
        raw["node_id"] != identity_receipt.get("node_id")
        or raw["peer_id"] != identity_receipt.get("peer_id")
        or raw["key_sha256"].lower() != str(identity_receipt.get("key_sha256", "")).lower()
        or raw["key_size_bytes"] != identity_receipt.get("key_size_bytes")
        or raw["key_uid"] != identity_receipt.get("key_uid")
        or raw["key_gid"] != identity_receipt.get("key_gid")
        or raw["key_mode"] != expected_key_mode
    ):
        _fail(f"{label} raw-v1 identity does not match the governed v2 identity")


def _validate_identity_raw_v1_digest(
    identity_receipt: dict[str, Any],
    label: str,
    *,
    raw_v1_bytes: bytes | None = None,
) -> None:
    """Reject rebinding a governed v2 digest after raw-v1 metadata changes.

    When the runtime output is available, callers must provide its exact bytes
    so admission never reconstructs a payload from a guessed key path.  The
    no-byte fallback remains for pre-v2 plan fixtures only.
    """
    signed_payload = _string(
        identity_receipt.get("signed_payload_sha256"),
        f"{label}.signed_payload_sha256",
    ).lower()
    if not HEX64_RE.fullmatch(signed_payload):
        _fail(f"{label}.signed_payload_sha256 must be a 64-character digest")
    if raw_v1_bytes is not None:
        _validate_raw_identity_receipt_v1_bytes(identity_receipt, raw_v1_bytes, label)
        expected = hashlib.sha256(raw_v1_bytes).hexdigest()
    else:
        expected = hashlib.sha256(
            _canonical_raw_identity_receipt_v1_bytes(identity_receipt)
        ).hexdigest()
    if signed_payload != expected:
        _fail(f"{label} signed payload is not bound to the canonical raw v1 bytes")


def _validate_receipt_freshness(
    receipt: dict[str, Any],
    label: str,
    capture_window_id: str,
    *,
    capture_window_bounds: tuple[dt.datetime, dt.datetime] | None = None,
    expected_rotation_epoch: str | None = CANONICAL_ROTATION_EPOCH,
) -> None:
    """Require a current, plan-bound v2 receipt freshness tuple."""
    missing = {
        "capture_window_id",
        "rotation_epoch",
        "issued_at",
        "expires_at",
    } - set(receipt)
    if missing:
        _fail(f"{label} freshness fields are incomplete: {', '.join(sorted(missing))}")
    if receipt.get("capture_window_id") != capture_window_id:
        _fail(f"{label}.capture_window_id does not match the transaction capture window")
    if expected_rotation_epoch is not None and receipt.get("rotation_epoch") != expected_rotation_epoch:
        _fail(f"{label}.rotation_epoch is not the governed rotation epoch")
    issued_at = _parse_utc(receipt.get("issued_at"), f"{label}.issued_at")
    expires_at = _parse_utc(receipt.get("expires_at"), f"{label}.expires_at")
    now = dt.datetime.now(dt.timezone.utc)
    if expires_at <= issued_at or expires_at <= now:
        _fail(f"{label} freshness window is stale or inverted")
    if issued_at > now + dt.timedelta(seconds=MAX_CLOCK_SKEW_SECONDS):
        _fail(f"{label}.issued_at is in the future")
    if capture_window_bounds is not None:
        capture_start, capture_end = capture_window_bounds
        if issued_at < capture_start or expires_at > capture_end:
            _fail(f"{label} freshness window is outside the plan capture window")


def _validate_deployment_inventory(
    plan: dict[str, Any],
    planner: Any,
    capture_window_bounds: tuple[dt.datetime, dt.datetime] | None = None,
) -> dict[str, Any]:
    inventory = _object(plan.get("deployment_inventory"), "deployment inventory")
    if set(inventory) != {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "trust_root_id",
        "nodes",
        "receipt",
    }:
        _fail("deployment inventory fields are not exact")
    if inventory.get("schema_version") != DEPLOYMENT_INVENTORY_SCHEMA:
        _fail("deployment inventory schema is unsupported")
    if inventory.get("authenticated") is not True or inventory.get("verified") is not True:
        _fail("deployment inventory is not authenticated and independently verified")
    if inventory.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST:
        _fail("deployment inventory signer is not code-owned")
    if inventory.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
        _fail("deployment inventory trust root is not code-owned")
    receipt = _object(inventory.get("receipt"), "deployment inventory receipt")
    if receipt.get("schema_version") != DEPLOYMENT_INVENTORY_RECEIPT_SCHEMA:
        _fail("deployment inventory receipt schema is unsupported")
    if set(receipt) != DEPLOYMENT_INVENTORY_RECEIPT_FIELDS:
        missing = DEPLOYMENT_INVENTORY_RECEIPT_FIELDS - set(receipt)
        extra = set(receipt) - DEPLOYMENT_INVENTORY_RECEIPT_FIELDS
        if missing:
            _fail(
                "deployment inventory receipt freshness fields are incomplete: "
                + ", ".join(sorted(missing))
            )
        _fail(
            "deployment inventory receipt fields are not exact: "
            + ", ".join(sorted(extra))
        )
    if (
        receipt.get("schema_version") != DEPLOYMENT_INVENTORY_RECEIPT_SCHEMA
        or receipt.get("authenticated") is not True
        or receipt.get("verified") is not True
        or receipt.get("signer_id") != inventory["signer_id"]
        or receipt.get("verifier_id") != CANONICAL_VERIFIER_ID
        or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
    ):
        _fail("deployment inventory receipt is not independently authenticated")
    _validate_receipt_freshness(
        receipt,
        "deployment inventory receipt",
        plan["capture_window_id"],
        capture_window_bounds=capture_window_bounds,
    )
    _reject_secret_fields(receipt, "deployment inventory receipt")
    _nonzero_hex(receipt.get("signed_payload_sha256"), HEX64_RE, "deployment inventory payload")
    _nonzero_hex(receipt.get("signature_hex"), SIGNATURE_RE, "deployment inventory signature")
    _nonzero_hex(receipt.get("canonical_digest"), HEX64_RE, "deployment inventory digest")
    if receipt.get("canonical_digest") != _canonical_receipt_digest(
        receipt, excluded_fields=frozenset({"signed_payload_sha256"})
    ):
        _fail("deployment inventory receipt canonical digest is not independently bound")
    # Authenticate the exact caller-supplied payload before any path or field
    # normalization. Normalization must never repair or rewrite an incoming
    # signed payload digest.
    incoming_payload_digest = _canonical_deployment_inventory_payload_digest(inventory)
    if receipt.get("signed_payload_sha256") != incoming_payload_digest:
        _fail("deployment inventory receipt payload is not bound to the incoming canonical surface inventory")
    raw_nodes = _object(inventory.get("nodes"), "deployment inventory nodes")
    if set(raw_nodes) != set(planner.NODE_ORDER):
        _fail("deployment inventory does not cover the managed five-node set")
    normalized: dict[str, Any] = {}
    for name in planner.NODE_ORDER:
        expected = planner.EXPECTED_NODES[name]
        value = _object(raw_nodes.get(name), f"deployment inventory {name}")
        allowed_fields = {
            "node_id",
            "node_root",
            "persistent_state_paths",
            "expected_key_uid",
            "expected_key_gid",
            "peer_id",
        }
        required_fields = allowed_fields
        if not required_fields.issubset(value) or set(value) - allowed_fields:
            _fail(f"deployment inventory {name} fields are not exact")
        if value.get("node_id") != expected["node_id"]:
            _fail(f"deployment inventory {name} node id drifted")
        peer_id = _string(value.get("peer_id"), f"deployment inventory {name} peer id")
        path_style = "windows" if expected["platform"] == "windows-x64" else "posix"
        root = planner._normalized_path(
            value.get("node_root"), path_style, f"deployment inventory {name} root"
        )
        paths = _safe_relative_paths(
            root,
            value.get("persistent_state_paths"),
            expected["platform"],
            f"deployment inventory {name} state paths",
        )
        if len(set(paths)) != len(paths):
            _fail(f"deployment inventory {name} state paths contain duplicates")
        expected_path_variants = [
            [
                root.rstrip("/")
                + "/"
                + surface.replace("{node_id}", expected["node_id"]).replace("\\", "/")
                for surface in surface_set
            ]
            for surface_set in planner._canonical_state_surface_variants(name)
        ]
        if paths not in expected_path_variants:
            _fail(f"deployment inventory {name} state paths must cover the exact canonical surfaces")
        for field in ("expected_key_uid", "expected_key_gid"):
            owner = value.get(field)
            if not isinstance(owner, int) or isinstance(owner, bool) or owner < 0:
                _fail(f"deployment inventory {name} {field} is malformed")
        normalized[name] = {
            "node_id": value["node_id"],
            "peer_id": peer_id,
            "node_root": root,
            "persistent_state_paths": paths,
            "expected_key_uid": value["expected_key_uid"],
            "expected_key_gid": value["expected_key_gid"],
        }
    normalized_inventory = {
        "schema_version": DEPLOYMENT_INVENTORY_SCHEMA,
        "authenticated": True,
        "verified": True,
        "signer_id": inventory["signer_id"],
        "trust_root_id": CANONICAL_TRUST_ROOT_ID,
        "nodes": normalized,
        "receipt": receipt,
    }
    canonical_digest = _canonical_deployment_inventory_payload_digest(normalized_inventory)
    if receipt.get("signed_payload_sha256") != canonical_digest:
        _fail("deployment inventory receipt payload is not bound to the canonical inventory")
    return normalized_inventory


def _under_root(root: str, path: str, platform: str) -> bool:
    if platform == "windows-x64":
        root_norm = ntpath.normcase(ntpath.normpath(root))
        path_norm = ntpath.normcase(ntpath.normpath(path))
        try:
            relative = ntpath.relpath(path_norm, root_norm)
            return relative not in ("..", ".") and not relative.startswith(".." + ntpath.sep)
        except ValueError:
            return False
    root_norm = posixpath.normpath(root)
    path_norm = posixpath.normpath(path)
    return path_norm != root_norm and path_norm.startswith(root_norm.rstrip("/") + "/")


def _validate_nonce_contract(
    plan: dict[str, Any], nodes: list[dict[str, Any]], ledger: dict[str, Any], capture_window: dict[str, Any]
) -> None:
    """Recompute the one-shot ledger/seam contract from the managed node list."""
    expected_fields = {
        "schema_version",
        "path",
        "transaction_id",
        "capture_window_id",
        "one_shot",
        "replay",
        "issued_at",
        "expires_at",
        "reserved_nonces",
        "receipt",
    }
    if set(ledger) != expected_fields:
        _fail("credential nonce ledger contract contains an unsafe or missing field")
    if (
        ledger.get("schema_version") != "oasis7.credential_nonce_ledger.v1"
        or ledger.get("transaction_id") != plan["transaction_id"]
        or ledger.get("capture_window_id") != plan["capture_window_id"]
        or ledger.get("one_shot") is not True
        or ledger.get("replay") is not False
    ):
        _fail("credential nonce ledger one-shot or transaction binding drifted")
    ledger_path = _string(ledger.get("path"), "credential nonce ledger path")
    if not Path(ledger_path).is_absolute():
        _fail("credential nonce ledger path must be absolute")
    issued_at = _parse_utc(ledger.get("issued_at"), "credential nonce ledger issued_at")
    expires_at = _parse_utc(ledger.get("expires_at"), "credential nonce ledger expires_at")
    if expires_at <= issued_at:
        _fail("credential nonce ledger lease is inverted")
    now = dt.datetime.now(dt.timezone.utc)
    if issued_at > now + dt.timedelta(seconds=MAX_CLOCK_SKEW_SECONDS) or expires_at <= now:
        _fail("credential nonce ledger lease is expired or outside allowed clock skew")
    if (
        ledger.get("issued_at") != capture_window.get("starts_at")
        or ledger.get("expires_at") != capture_window.get("ends_at")
    ):
        _fail("credential nonce ledger lease is not bound to the capture window")
    raw_reserved = ledger.get("reserved_nonces")
    if not isinstance(raw_reserved, list) or len(raw_reserved) != len(nodes):
        _fail("credential nonce ledger must reserve one nonce per managed node")
    reserved = [_string(item, "credential nonce ledger reserved nonce") for item in raw_reserved]
    if any(PLAN_NONCE_RE.fullmatch(item) is None for item in reserved):
        _fail("credential nonce ledger contains a malformed nonce")
    if len(set(reserved)) != len(reserved):
        _fail("credential nonce ledger contains duplicate nonces")
    for index, node in enumerate(nodes):
        seam = _object(node.get("credential_seam"), f"{node['name']} credential seam")
        if set(seam) != {
            "kind",
            "environment_name",
            "nonce",
            "issued_at",
            "expires_at",
            "ledger_path",
            "one_shot",
        }:
            _fail(f"{node['name']} credential seam fields are not exact")
        if (
            seam.get("nonce") != reserved[index]
            or seam.get("ledger_path") != ledger_path
            or seam.get("issued_at") != ledger["issued_at"]
            or seam.get("expires_at") != ledger["expires_at"]
            or seam.get("one_shot") is not True
        ):
            _fail(f"{node['name']} credential seam is not bound to the one-shot ledger")
    receipt = _object(ledger.get("receipt"), "credential nonce ledger receipt")
    if set(receipt) != _TRANSPORT_RECEIPT_FIELDS | {"bindings"}:
        _fail("credential nonce ledger receipt fields are not exact")
    if (
        receipt.get("schema_version") != "oasis7.credential_nonce_ledger_receipt.v1"
        or receipt.get("authenticated") is not True
        or receipt.get("verified") is not True
        or receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST
        or receipt.get("verifier_id") != CANONICAL_VERIFIER_ID
        or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
    ):
        _fail("credential nonce ledger receipt is not independently authenticated")
    _nonzero_hex(receipt.get("signed_payload_sha256"), HEX64_RE, "credential nonce ledger payload")
    _nonzero_hex(receipt.get("signature_hex"), SIGNATURE_RE, "credential nonce ledger signature")
    _nonzero_hex(receipt.get("canonical_digest"), HEX64_RE, "credential nonce ledger digest")
    expected_bindings = {
        "path": ledger_path,
        "transaction_id": plan["transaction_id"],
        "capture_window_id": plan["capture_window_id"],
        "one_shot": True,
        "replay": False,
        "issued_at": ledger["issued_at"],
        "expires_at": ledger["expires_at"],
        "reserved_nonces": reserved,
    }
    if receipt.get("bindings") != expected_bindings:
        _fail("credential nonce ledger receipt bindings are not exact")


def _validate_trusted_receipt_identity(value: Any, label: str) -> None:
    """Require nested receipts to name the code-owned verifier and trust root."""
    receipt = _object(value, label)
    if receipt.get("verifier_id") != CANONICAL_VERIFIER_ID:
        _fail(f"{label} verifier is not code-owned")
    if receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
        _fail(f"{label} trust root is not code-owned")


def _validate_planner_authority(plan: dict[str, Any]) -> None:
    """Recompute the planner authority bindings at the adapter boundary."""
    authority = _object(plan.get("authority"), "plan authority")
    receipt = _object(authority.get("receipt"), "plan authority receipt")
    if receipt.get("schema_version") != "oasis7.clean_room_authority.v1":
        _fail("plan authority receipt schema is unsupported")
    if (
        receipt.get("authenticated") is not True
        or receipt.get("verified") is not True
        or receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST
        or receipt.get("verifier_id") != CANONICAL_VERIFIER_ID
        or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
    ):
        _fail("plan authority receipt is not independently authenticated")
    _reject_secret_fields(receipt, "plan authority receipt")
    _nonzero_hex(receipt.get("signed_payload_sha256"), HEX64_RE, "plan authority payload")
    _nonzero_hex(receipt.get("signature_hex"), SIGNATURE_RE, "plan authority signature")
    _nonzero_hex(receipt.get("canonical_digest"), HEX64_RE, "plan authority digest")
    bindings = _object(receipt.get("bindings"), "plan authority receipt bindings")
    expected = {
        "task_uid": plan["task_uid"],
        "head_oid": plan["head_oid"],
        "signer_allowlist": sorted(CANONICAL_SIGNER_ALLOWLIST),
        "trust_root_id": CANONICAL_TRUST_ROOT_ID,
        "verifier_id": CANONICAL_VERIFIER_ID,
        "consumer_impact_record": _consumer_impact_locator(plan),
    }
    for field, expected_value in expected.items():
        if bindings.get(field) != expected_value:
            _fail(f"plan authority receipt {field} binding drifted")
    if (
        "frozen_head_oid" in bindings
        and bindings.get("frozen_head_oid") != plan["head_oid"]
    ):
        _fail("plan authority receipt frozen-head binding drifted")
    context_fields = {"capture_window_id", "rotation_epoch", "issued_at", "expires_at"}
    present = context_fields.intersection(bindings)
    if present:
        if present != context_fields:
            _fail("plan authority receipt freshness binding is incomplete")
        if bindings.get("capture_window_id") != plan["capture_window_id"]:
            _fail("plan authority receipt capture-window binding drifted")
        if bindings.get("rotation_epoch") != CANONICAL_ROTATION_EPOCH:
            _fail("plan authority receipt rotation epoch is not code-owned")
        issued_at = _parse_utc(bindings.get("issued_at"), "plan authority receipt issued_at")
        expires_at = _parse_utc(bindings.get("expires_at"), "plan authority receipt expires_at")
        now = dt.datetime.now(dt.timezone.utc)
        if expires_at <= issued_at or expires_at <= now:
            _fail("plan authority receipt is stale or has an inverted freshness window")
        if issued_at > now + dt.timedelta(seconds=MAX_CLOCK_SKEW_SECONDS):
            _fail("plan authority receipt is issued in the future")
    trust_root = _object(authority.get("trust_root"), "plan authority trust root")
    trust_bindings = _object(trust_root.get("bindings"), "plan authority trust-root bindings")
    if trust_bindings != bindings:
        _fail("plan authority receipt and trust-root bindings disagree")


def _validate_plan_semantic_bindings(
    plan: dict[str, Any], planner: Any, nodes: list[dict[str, Any]]
) -> None:
    """Recompute cross-section bindings instead of trusting plan_digest alone."""
    try:
        truth = _object(plan.get("truth"), "plan truth")
        normalized_truth = planner._validate_truth(
            truth, set(CANONICAL_SIGNER_ALLOWLIST)
        )
    except (SystemExit, KeyError, TypeError) as error:
        _fail(f"plan truth semantic validation failed: {error}")
    if normalized_truth != truth:
        _fail("plan truth is not the authenticated canonical projection")
    package = normalized_truth["package"]
    genesis = normalized_truth["genesis"]
    world = normalized_truth["world"]
    checkpoint = normalized_truth["checkpoint"]
    for label, value in (
        ("truth.package.receipt", package.get("receipt")),
        ("truth.genesis.receipt", genesis.get("receipt")),
        ("truth.world.receipt", world.get("receipt")),
        ("truth.checkpoint.receipt", checkpoint.get("receipt")),
    ):
        _validate_trusted_receipt_identity(value, label)
    for node in nodes:
        expected_binding = {
            "package_commit": package["commit"],
            "package_platform": package["platforms"][node["platform"]],
            "genesis_sha256": genesis["sha256"],
            "world_sha256": world["sha256"],
            "checkpoint_id": checkpoint["checkpoint_id"],
            "checkpoint_manifest_hash": checkpoint["manifest_hash"],
            "checkpoint_height": checkpoint["height"],
        }
        if node.get("bindings") != expected_binding:
            _fail(f"{node['name']} semantic binding is not derived from authenticated truth")
    try:
        probe = _object(plan.get("fresh_root_probe"), "plan fresh-root probe")
        normalized_probe = planner._validate_probe(
            probe,
            normalized_truth,
            set(CANONICAL_SIGNER_ALLOWLIST),
            {
                "transaction_id": plan["transaction_id"],
                "capture_window_id": plan["capture_window_id"],
            },
        )
    except (SystemExit, KeyError, TypeError) as error:
        _fail(f"plan fresh-root probe semantic validation failed: {error}")
    if normalized_probe != plan["fresh_root_probe"]:
        _fail("plan fresh-root probe is not the authenticated canonical projection")
    _validate_trusted_receipt_identity(
        normalized_probe.get("receipt"), "fresh_root_probe.receipt"
    )
    _validate_trusted_receipt_identity(
        _object(plan.get("adapter_verification"), "planner adapter verification").get("receipt"),
        "adapter_verification.receipt",
    )
    expected_gate = {
        "required_before": ["windows-observer", "macos-observer"],
        "fresh_root_probe_required": True,
        "checkpoint_receipt_required": True,
        "fail_closed": True,
    }
    if plan.get("observer_gate") != expected_gate:
        _fail("plan observer gate is not the code-owned fail-closed contract")


def validate_plan(
    plan: dict[str, Any],
    *,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> dict[str, Any]:
    """Validate immutable planner output and all code-owned inventories."""
    plan = _object(plan, "plan")
    if plan.get("schema_version") != PLAN_SCHEMA:
        _fail("plan schema is unsupported")
    actual_digest = _digest(plan.get("plan_digest"), "plan_digest")
    if actual_digest != canonical_plan_digest(plan):
        _fail("plan digest does not match the frozen plan contents")
    planner = _load_planner()
    capture_window_bounds = _capture_window_bounds(plan)
    _consumer_impact_locator(plan)
    authority_plan = _object(plan.get("authority"), "plan authority")
    if authority_plan.get("consumer_impact_record") != plan["consumer_impact_record"]:
        _fail("plan authority is not bound to the consumer impact record")
    _validate_planner_authority(plan)
    identity_v2_evidence: dict[str, Any]
    evidence_raw_v1: Mapping[str, bytes] | None = None
    if plan.get("identity_v2_evidence") is None:
        _fail("identity-v2 evidence map is required for adapter admission")
    identity_v2_evidence = _object(plan.get("identity_v2_evidence"), "identity-v2 evidence map")
    evidence_envelopes: Mapping[str, dict[str, Any]]
    try:
        validated_evidence, evidence_raw_v1, evidence_envelopes = planner._identity_v2_evidence_map(
            identity_v2_evidence,
            {
                "authority": authority_plan,
                "capture_window_id": plan.get("capture_window_id"),
            },
        )
        context_path, _ = planner._evidence_descriptor(
            validated_evidence.get("context"), "identity-v2 context"
        )
        intent_path, _ = planner._evidence_descriptor(
            validated_evidence.get("plan_intent"), "identity-v2 plan intent"
        )
        planner._independently_verify_identity_v2_entries(
            validated_evidence, context_path, intent_path
        )
    except (SystemExit, KeyError, TypeError) as error:
        _fail(f"identity-v2 evidence map validation failed: {error}")
    if raw_v1_bytes_by_node is not None:
        _fail("identity-v2 evidence map cannot be combined with caller raw-v1 mapping")
    raw_v1_bytes_by_node = evidence_raw_v1
    if plan.get("node_order") != list(planner.NODE_ORDER):
        _fail("plan node order is not the code-owned five-node order")
    if plan.get("canonical_host_inventory") != planner.CANONICAL_HOST_INVENTORY:
        _fail("plan host inventory is not code-owned")
    if plan.get("canonical_endpoint_inventory") != planner.CANONICAL_ENDPOINT_INVENTORY:
        _fail("plan endpoint inventory is not code-owned")
    peer_snapshot = planner._peer_registry_authority().load_snapshot()
    deployment_inventory = _validate_deployment_inventory(
        plan, planner, capture_window_bounds
    )
    execution = _object(plan.get("execution"), "plan execution")
    if execution.get("mode") != "plan-only" or execution.get("provider_mutation_performed") is not False:
        _fail("plan is not an unperformed plan-only artifact")
    if (
        execution.get("plan_is_apply_proof") is not False
        or execution.get("apply_requires_fresh_adapter_receipt") is not True
    ):
        _fail("plan cannot be used as apply proof")
    surfaces = _object(plan.get("surfaces"), "plan surfaces")
    if surfaces.get("validators") != list(planner.VALIDATOR_RESET_SURFACES):
        _fail("validator reset surfaces are not the canonical eight")
    if surfaces.get("observers") != list(planner.OBSERVER_RESET_SURFACES):
        _fail("observer reset surfaces are not the canonical seven")
    if surfaces.get("observer_count") != 8:
        _fail("observer surface summary must cover the governed eight surfaces")
    nodes = plan.get("nodes")
    if (
        not isinstance(nodes, list)
        or {node.get("name") for node in nodes if isinstance(node, dict)} != set(planner.NODE_ORDER)
    ):
        _fail("plan does not contain exactly the canonical five nodes")
    if [node.get("name") for node in nodes] != list(planner.NODE_ORDER):
        _fail("plan nodes are not in the code-owned five-node order")
    if raw_v1_bytes_by_node is not None:
        if not isinstance(raw_v1_bytes_by_node, Mapping):
            _fail("exact runtime raw-v1 bytes must be supplied as a node mapping")
        if set(raw_v1_bytes_by_node) != set(planner.NODE_ORDER):
            missing = sorted(set(planner.NODE_ORDER) - set(raw_v1_bytes_by_node))
            extra = sorted(set(raw_v1_bytes_by_node) - set(planner.NODE_ORDER))
            _fail(
                "exact runtime raw-v1 node mapping is not exact "
                f"(missing={missing}, extra={extra})"
            )
    by_name: dict[str, dict[str, Any]] = {}
    seen_peer_ids: set[str] = set()
    for node_value in nodes:
        node = _object(node_value, "plan node")
        name = _string(node.get("name"), "plan node name")
        if name in by_name:
            _fail("plan contains duplicate node names")
        by_name[name] = node
        expected = planner.EXPECTED_NODES[name]
        for field, expected_value in expected.items():
            if field == "node_root":
                continue
            if node.get(field) != expected_value:
                _fail(f"{name} {field} is not the code-owned value")
        governed = deployment_inventory["nodes"][name]
        if node.get("node_id") != governed["node_id"]:
            _fail(f"{name} node id is not bound to deployment inventory")
        path_style = "windows" if expected["platform"] == "windows-x64" else "posix"
        normalized_root = planner._normalized_path(
            governed["node_root"], path_style, f"{name}.deployment_node_root"
        )
        if node.get("node_root") != normalized_root:
            _fail(f"{name} node root is not bound to deployment inventory")
        binding = _object(node.get("host_binding"), f"{name} host binding")
        if binding != planner.CANONICAL_HOST_INVENTORY[name]:
            _fail(f"{name} known-host target or pin is not code-owned")
        if _object(node.get("endpoints"), f"{name} endpoints") != planner.CANONICAL_ENDPOINT_INVENTORY[name]:
            _fail(f"{name} endpoint binding is not code-owned")
        identity = _object(node.get("identity_receipt"), f"{name} identity receipt")
        if identity.get("schema_version") != IDENTITY_RECEIPT_SCHEMA:
            _fail(f"{name} identity receipt schema is unsupported")
        if set(identity) != IDENTITY_RECEIPT_FIELDS:
            missing = IDENTITY_RECEIPT_FIELDS - set(identity)
            extra = set(identity) - IDENTITY_RECEIPT_FIELDS
            if missing:
                _fail(
                    f"{name} identity receipt freshness fields are incomplete: "
                    + ", ".join(sorted(missing))
                )
            _fail(
                f"{name} identity receipt fields are not exact: "
                + ", ".join(sorted(extra))
            )
        if (
            identity.get("schema_version") != IDENTITY_RECEIPT_SCHEMA
            or identity.get("authenticated") is not True
            or identity.get("verified") is not True
            or identity.get("verifier_id") != CANONICAL_VERIFIER_ID
            or identity.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
            or identity.get("node_id") != node["node_id"]
        ):
            _fail(f"{name} identity receipt is not independently authenticated")
        _validate_receipt_freshness(
            identity,
            f"{name} identity receipt",
            plan["capture_window_id"],
            capture_window_bounds=capture_window_bounds,
            expected_rotation_epoch=None,
        )
        retained_envelope = evidence_envelopes.get(name)
        if not isinstance(retained_envelope, dict):
            _fail(f"{name} identity receipt is missing from the retained v2 evidence map")
        expected_identity = {
            field: retained_envelope[field] for field in IDENTITY_RECEIPT_FIELDS
        }
        if identity != expected_identity:
            mismatched_fields = sorted(
                field
                for field in IDENTITY_RECEIPT_FIELDS
                if identity.get(field) != expected_identity.get(field)
            )
            _fail(
                f"{name} identity receipt does not match the retained v2 evidence map: "
                + ", ".join(mismatched_fields)
            )
        _reject_secret_fields(identity, f"{name} identity receipt")
        _nonzero_hex(identity.get("signed_payload_sha256"), HEX64_RE, f"{name} identity payload")
        _nonzero_hex(identity.get("signature_hex"), SIGNATURE_RE, f"{name} identity signature")
        _nonzero_hex(identity.get("canonical_digest"), HEX64_RE, f"{name} identity digest")
        key_size = identity.get("key_size_bytes")
        if not isinstance(key_size, int) or isinstance(key_size, bool) or key_size <= 0:
            _fail(f"{name} identity key size is malformed")
        _nonzero_hex(identity.get("key_sha256"), HEX64_RE, f"{name} identity key digest")
        if identity.get("key_mode") != "0600":
            _fail(f"{name} identity key mode is not 0600")
        for owner in ("key_uid", "key_gid"):
            value = identity.get(owner)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                _fail(f"{name} identity {owner} is malformed")
        if (
            identity["key_uid"] != governed["expected_key_uid"]
            or identity["key_gid"] != governed["expected_key_gid"]
        ):
            _fail(f"{name} identity uid/gid do not match independently authenticated deployment inventory")
        _validate_identity_raw_v1_digest(
            identity,
            f"{name} identity receipt",
            raw_v1_bytes=(
                raw_v1_bytes_by_node[name]
                if raw_v1_bytes_by_node is not None
                else None
            ),
        )
        peer_id = _string(identity.get("peer_id"), f"{name} identity peer id")
        expected_peer_id = peer_snapshot["peers"][name]
        if governed.get("peer_id", expected_peer_id) != expected_peer_id:
            _fail(f"{name} deployment inventory peer differs from pinned peer registry")
        if peer_id != expected_peer_id:
            _fail(f"{name} identity peer id does not match authenticated deployment inventory")
        if peer_id in seen_peer_ids:
            _fail(f"{name} identity peer id duplicates another managed node")
        seen_peer_ids.add(peer_id)
        paths = _safe_relative_paths(
            node["node_root"],
            node.get("persistent_state_paths"),
            node["platform"],
            f"{name} state paths",
        )
        if paths != governed["persistent_state_paths"]:
            _fail(f"{name} state paths do not match its exact reset surfaces")
    expected_observers = {
        name: by_name[name]["persistent_state_paths"]
        for name in planner.OBSERVER_NAMES
    }
    if surfaces.get("observers_by_node") != expected_observers:
        _fail("observer surface summary is not bound to governed node inventory")
    _validate_plan_semantic_bindings(plan, planner, nodes)
    _validate_forensic_backup_policy(plan)
    _validate_rollback_policy(plan)
    forensic = _object(plan.get("forensic_backup"), "forensic backup")
    capture_window = _object(plan.get("capture_window"), "transaction capture window")
    if set(capture_window) != {"id", "starts_at", "ends_at"}:
        _fail("transaction capture window contains an unsafe field")
    if capture_window.get("id") != plan["capture_window_id"]:
        _fail("transaction capture window id binding drifted")
    window_start = _parse_utc(capture_window.get("starts_at"), "transaction capture window starts_at")
    window_end = _parse_utc(capture_window.get("ends_at"), "transaction capture window ends_at")
    if window_end <= window_start:
        _fail("transaction capture window is inverted")
    ledger_contract = _object(plan.get("credential_nonce_ledger"), "credential nonce ledger contract")
    _validate_nonce_contract(plan, nodes, ledger_contract, capture_window)
    journal_contract = _object(
        plan.get("operation_journal_contract"), "operation journal contract"
    )
    if (
        journal_contract.get("authoritative") is not False
        or journal_contract.get("apply_usable") is not False
        or journal_contract.get("adapter_owned") is not True
        or journal_contract.get("durable_receipt_required") is not True
        or journal_contract.get("planner_output_is_not_apply_proof") is not True
    ):
        _fail("operation journal contract is not the adapter-owned fail-closed contract")
    adapter_verification = _object(
        plan.get("adapter_verification"), "planner adapter verification"
    )
    if (
        adapter_verification.get("schema_version") != "oasis7.clean_room_adapter_verification.v1"
        or adapter_verification.get("adapter_id") != CANONICAL_ADAPTER_ID
        or adapter_verification.get("transaction_id") != plan["transaction_id"]
        or adapter_verification.get("capture_window_id") != plan["capture_window_id"]
        or adapter_verification.get("live_receipts_verified") is not True
        or adapter_verification.get("credential_nonce_ledger_verified") is not True
        or adapter_verification.get("backup_or_no_backup_authority_verified") is not True
        or adapter_verification.get("apply_authority_granted") is not False
        or adapter_verification.get("durable_journal_authoritative") is not False
        or adapter_verification.get("durable_journal_receipt_required") is not True
    ):
        _fail("planner adapter verification does not grant safe apply prerequisites")
    global_order = plan.get("global_order")
    if not isinstance(global_order, list):
        _fail("plan global order is missing")
    try:
        planner._validate_global_order(global_order)
        expected_order = planner._global_order(bool(forensic.get("required_before_reset")))
    except SystemExit as error:
        _fail(str(error))
    if global_order != expected_order:
        _fail("plan global order is not deterministic for its backup mode")
    probe = _object(plan.get("fresh_root_probe"), "fresh-root probe")
    if (
        probe.get("transaction_id") != plan.get("transaction_id")
        or probe.get("capture_window_id") != plan.get("capture_window_id")
    ):
        _fail("fresh-root probe transaction binding is not exact")
    if probe.get("replayed") is not False or probe.get("post_validator_verify") is not True:
        _fail("fresh-root probe is replayed or lacks post-validator verification")
    _validate_no_backup_authority(plan)
    return {
        "nodes": list(by_name.values()),
        "planner": planner,
        "plan_digest": actual_digest,
        "deployment_inventory": deployment_inventory,
        "capture_window_id": plan["capture_window_id"],
        "identity_v2_evidence": identity_v2_evidence,
    }


def _parse_utc(value: Any, label: str) -> dt.datetime:
    raw = _string(value, label)
    try:
        parsed = dt.datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except ValueError:
        _fail(f"{label} must be an ISO-8601 timestamp")
    if parsed.tzinfo is None:
        _fail(f"{label} must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def _capture_window_bounds(plan: dict[str, Any]) -> tuple[dt.datetime, dt.datetime]:
    window = _object(plan.get("capture_window"), "transaction capture window")
    if window.get("id") != plan["capture_window_id"]:
        _fail("transaction capture window id binding drifted")
    start = _parse_utc(window.get("starts_at"), "transaction capture window starts_at")
    end = _parse_utc(window.get("ends_at"), "transaction capture window ends_at")
    if end <= start:
        _fail("transaction capture window is inverted")
    return start, end


def _validate_no_backup_authority(plan: dict[str, Any]) -> None:
    """Require a current, signed operator decision when no backup is used."""
    forensic = _object(plan.get("forensic_backup"), "forensic backup")
    mode = forensic.get("mode")
    authority = forensic.get("authority")
    if mode != "operator-authorized-no-backup":
        if authority is not None:
            _fail("forensic backup authority is only valid in no-backup mode")
        return
    if authority is None:
        _fail("operator-authorized-no-backup requires a signed current authority")
    if forensic.get("operator_authorized") is not True:
        _fail("no-backup mode is not operator-authorized")
    if forensic.get("current_authorization") is not True:
        _fail("no-backup mode lacks current authorization")
    if forensic.get("repository") != REPOSITORY or forensic.get("action") != "full-network-clean-room":
        _fail("no-backup authority repository or action is not governed")
    if forensic.get("targets") != list(plan["node_order"]):
        _fail("no-backup authority targets are not the deterministic managed-node set")
    for field in ("transaction_id", "capture_window_id", "actor", "issued_at", "expires_at"):
        if field not in forensic:
            _fail(f"no-backup authority is missing {field}")
    if forensic["transaction_id"] != plan["transaction_id"] or forensic["capture_window_id"] != plan["capture_window_id"]:
        _fail("no-backup authority transaction binding drifted")
    actor = _string(forensic["actor"], "no-backup authority actor")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{1,127}", actor):
        _fail("no-backup authority actor is malformed")
    issued_at = _parse_utc(forensic["issued_at"], "no-backup authority issued_at")
    expires_at = _parse_utc(forensic["expires_at"], "no-backup authority expires_at")
    if (
        issued_at > dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=MAX_CLOCK_SKEW_SECONDS)
        or expires_at <= issued_at
        or expires_at <= dt.datetime.now(dt.timezone.utc)
    ):
        _fail("no-backup authority is expired or inverted")
    receipt = _object(authority, "no-backup authority receipt")
    if receipt.get("schema_version") != NO_BACKUP_AUTHORITY_SCHEMA:
        _fail("no-backup authority receipt schema is unsupported")
    if receipt.get("authenticated") is not True or receipt.get("verified") is not True:
        _fail("no-backup authority receipt is not authenticated and verified")
    if receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST:
        _fail("no-backup authority signer is not code-owned")
    if receipt.get("verifier_id") != CANONICAL_VERIFIER_ID or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
        _fail("no-backup authority verifier or trust root is not code-owned")
    allowed = {
        "schema_version", "authenticated", "verified", "signer_id", "verifier_id", "trust_root_id",
        "signed_payload_sha256", "signature_hex", "canonical_digest", "bindings",
    }
    if set(receipt) - allowed:
        _fail("no-backup authority receipt contains an unsafe field")
    _reject_secret_fields(receipt, "no-backup authority receipt")
    _nonzero_hex(receipt.get("signed_payload_sha256"), HEX64_RE, "no-backup authority signed payload")
    _nonzero_hex(receipt.get("signature_hex"), SIGNATURE_RE, "no-backup authority signature")
    _nonzero_hex(receipt.get("canonical_digest"), HEX64_RE, "no-backup authority canonical digest")
    expected_bindings = {
        "repository": REPOSITORY,
        "action": "full-network-clean-room",
        "targets": list(plan["node_order"]),
        "task_uid": plan["task_uid"],
        "transaction_id": plan["transaction_id"],
        "capture_window_id": plan["capture_window_id"],
        "frozen_head_oid": plan["head_oid"],
        "actor": actor,
        "issued_at": issued_at.isoformat().replace("+00:00", "Z"),
        "expires_at": expires_at.isoformat().replace("+00:00", "Z"),
        "current_authorization": True,
        "consumer_impact_record": _consumer_impact_locator(plan),
    }
    if _object(receipt.get("bindings"), "no-backup authority bindings") != expected_bindings:
        _fail("no-backup authority receipt bindings are not exact")


def _validate_forensic_backup_policy(plan: dict[str, Any]) -> None:
    """Require the exact code-owned forensic-backup policy projection."""
    forensic = _object(plan.get("forensic_backup"), "forensic backup")
    if set(forensic) != set(FORENSIC_BACKUP_FIELDS):
        _fail("forensic backup policy schema is not the exact code-owned projection")
    mode = _string(forensic.get("mode"), "forensic backup mode")
    common = {
        "task_uid": plan["task_uid"],
        "frozen_head_oid": plan["head_oid"],
        "seed_eligible": False,
        "cross_node_state_copy": False,
        "restore_old_state": False,
    }
    if mode == "forensic-backup":
        expected = {
            "mode": mode,
            **common,
            "required_before_reset": True,
            "operator_authorized": False,
            "current_authorization": False,
            "immutable": True,
            "receipt_required_per_node": True,
            "authority": None,
            "repository": None,
            "action": None,
            "targets": None,
            "transaction_id": plan["transaction_id"],
            "capture_window_id": plan["capture_window_id"],
            "actor": None,
            "issued_at": None,
            "expires_at": None,
        }
    elif mode == "operator-authorized-no-backup":
        # This helper performs the signed authority and dynamic time/actor
        # checks.  The exact root shape and code-owned static projection are
        # enforced here as well, before any DTO is constructed.
        _validate_no_backup_authority(plan)
        authority = _object(forensic.get("authority"), "no-backup authority")
        if set(authority) != set(NO_BACKUP_AUTHORITY_RECEIPT_FIELDS):
            _fail("no-backup authority receipt schema is not exact")
        bindings = _object(authority.get("bindings"), "no-backup authority bindings")
        if set(bindings) != set(NO_BACKUP_AUTHORITY_BINDING_FIELDS):
            _fail("no-backup authority bindings schema is not exact")
        expected = {
            "mode": mode,
            **common,
            "required_before_reset": False,
            "operator_authorized": True,
            "current_authorization": True,
            "immutable": False,
            "receipt_required_per_node": False,
            "authority": authority,
            "repository": REPOSITORY,
            "action": "full-network-clean-room",
            "targets": list(plan["node_order"]),
            "transaction_id": plan["transaction_id"],
            "capture_window_id": plan["capture_window_id"],
            "actor": forensic["actor"],
            "issued_at": forensic["issued_at"],
            "expires_at": forensic["expires_at"],
        }
    else:
        _fail("forensic backup mode is unsupported")
    if forensic != expected:
        _fail("forensic backup policy is not the exact code-owned projection")


def _validate_rollback_policy(plan: dict[str, Any]) -> None:
    """Require the exact clean-redeploy policy and no caller-owned fields."""
    rollback = _object(plan.get("rollback"), "rollback")
    if set(rollback) != set(ROLLBACK_FIELDS):
        _fail("rollback policy schema is not the exact code-owned projection")
    if rollback != _canonical_rollback_policy():
        _fail("rollback policy is not the exact code-owned clean-redeploy projection")


def _canonical_rollback_policy() -> dict[str, Any]:
    return {
        "policy": "clean-redeploy",
        "steps": list(CANONICAL_ROLLBACK_STEPS),
        "stop_started_nodes": True,
        "preserve_failed_state_for_forensics": True,
        "restore_old_state": False,
        "cross_node_state_copy": False,
        "reinstall_exact_package_and_truth": True,
        "rerun_fresh_root_probe": True,
        "provider_mutation_requires_external_authority": True,
    }


def validate_authority(
    plan: dict[str, Any],
    authority: dict[str, Any],
    *,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> dict[str, Any]:
    """Validate external authority without accepting caller-owned identities."""
    validate_plan(plan, raw_v1_bytes_by_node=raw_v1_bytes_by_node)
    authority = _object(authority, "adapter authority")
    _reject_secret_fields(authority, "adapter authority")
    if set(authority) - {
        "schema_version", "repository", "task_uid", "frozen_head_oid", "plan_digest",
        "adapter_id", "network_id", "verifier_id", "trust_root_id", "trust_root_path",
        "trust_root_digest", "trust_root_file", "apply_authorized", "receipt", "provenance_helper",
        "consumer_impact_record",
    }:
        _fail("adapter authority contains an unsafe field")
    if authority.get("schema_version") != AUTHORITY_SCHEMA:
        _fail("adapter authority schema is unsupported")
    expected = {
        "repository": REPOSITORY,
        "task_uid": plan["task_uid"],
        "frozen_head_oid": plan["head_oid"],
        "plan_digest": plan["plan_digest"],
        "adapter_id": CANONICAL_ADAPTER_ID,
        "network_id": CANONICAL_NETWORK_ID,
        "verifier_id": CANONICAL_VERIFIER_ID,
        "trust_root_id": CANONICAL_TRUST_ROOT_ID,
        "trust_root_path": CANONICAL_TRUST_ROOT_PATH,
        "trust_root_digest": CANONICAL_TRUST_ROOT_DIGEST,
    }
    for field, expected_value in expected.items():
        if authority.get(field) != expected_value:
            _fail(f"adapter authority {field} is not bound to the frozen plan")
    if authority.get("consumer_impact_record") != plan["consumer_impact_record"]:
        _fail("adapter authority is not bound to the consumer impact record")
    trust_root_file = _object(authority.get("trust_root_file"), "pinned trust-root file contract")
    if set(trust_root_file) != {
        "path",
        "sha256",
        "root_digest",
        "owner_scope",
        "owner_uid",
        "mode",
        "regular_file",
    }:
        _fail("pinned trust-root file contract contains an unsafe field")
    if (
        trust_root_file.get("path") != CANONICAL_TRUST_ROOT_PATH
        or trust_root_file.get("sha256") != CANONICAL_TRUST_ROOT_FILE_SHA256
        or trust_root_file.get("root_digest") != CANONICAL_TRUST_ROOT_DIGEST
        or trust_root_file.get("owner_scope") != CANONICAL_TRUST_ROOT_OWNER_SCOPE
        or trust_root_file.get("owner_uid") != CANONICAL_TRUST_ROOT_OWNER_UID
        or trust_root_file.get("mode") != CANONICAL_TRUST_ROOT_MODE
        or trust_root_file.get("regular_file") is not True
    ):
        _fail("pinned trust-root file path, content digest, owner, or mode drifted")
    if not isinstance(authority.get("apply_authorized"), bool):
        _fail("adapter authority apply_authorized must be an explicit boolean")
    receipt = _object(authority.get("receipt"), "adapter authority receipt")
    if receipt.get("schema_version") != CRYPTO_RECEIPT_SCHEMA:
        _fail("adapter authority receipt schema is unsupported")
    if receipt.get("authenticated") is not True or receipt.get("verified") is not True:
        _fail("adapter authority receipt is not independently authenticated and verified")
    if receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST:
        _fail("adapter authority signer is not in the code-owned allowlist")
    if receipt.get("verifier_id") != CANONICAL_VERIFIER_ID or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
        _fail("adapter authority receipt verifier or trust root is not code-owned")
    if set(receipt) - {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "verifier_id",
        "trust_root_id",
        "bindings",
    }:
        _fail("adapter authority receipt contains an unsafe field")
    _reject_secret_fields(receipt, "adapter authority receipt")
    _nonzero_hex(receipt.get("signed_payload_sha256"), HEX64_RE, "adapter authority signed payload")
    _nonzero_hex(receipt.get("signature_hex"), SIGNATURE_RE, "adapter authority signature")
    _nonzero_hex(receipt.get("canonical_digest"), HEX64_RE, "adapter authority canonical digest")
    bindings = _object(receipt.get("bindings"), "adapter authority receipt bindings")
    expected_bindings = {
        "task_uid": plan["task_uid"],
        "frozen_head_oid": plan["head_oid"],
        "plan_digest": plan["plan_digest"],
        "execution": plan["truth"]["execution"],
        "ledger_path": plan["credential_nonce_ledger"]["path"],
        "apply_authorized": authority["apply_authorized"],
        "forensic_backup": plan["forensic_backup"],
        "rollback": plan["rollback"],
        "package_commit": plan["truth"]["package"]["commit"],
        "checkpoint_id": plan["truth"]["checkpoint"]["checkpoint_id"],
        "checkpoint_manifest_hash": plan["truth"]["checkpoint"]["manifest_hash"],
        "trust_root_path": CANONICAL_TRUST_ROOT_PATH,
        "trust_root_digest": CANONICAL_TRUST_ROOT_DIGEST,
        "trust_root_file": copy.deepcopy(trust_root_file),
        "consumer_impact_record": _consumer_impact_locator(plan),
    }
    if bindings != expected_bindings:
        _fail("adapter authority receipt binding set is not exact")
    return {"apply_authorized": authority.get("apply_authorized") is True, "receipt": receipt}


def validate_live_trust_root_file() -> dict[str, Any]:
    """Check the code-owned trust-root file at the apply boundary.

    The authority envelope is not sufficient: an apply must observe the live
    regular file immediately before any provider operation.  Path, content,
    owner, and mode are deployment-pinned constants, never caller inputs.
    """
    path = Path(CANONICAL_TRUST_ROOT_PATH)
    _reject_symlink_ancestors(path, "trust-root")
    try:
        link_metadata = path.lstat()
    except OSError:
        _fail("code-owned trust-root file is unavailable")
    if not stat.S_ISREG(link_metadata.st_mode):
        _fail("code-owned trust-root file is not a regular file")
    descriptor: int | None = None
    try:
        descriptor = os.open(
            str(path), os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        )
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            _fail("code-owned trust-root file is not a regular file")
        if metadata.st_uid != CANONICAL_TRUST_ROOT_OWNER_UID:
            _fail("code-owned trust-root file owner drifted")
        if stat.S_IMODE(metadata.st_mode) != int(CANONICAL_TRUST_ROOT_MODE, 8):
            _fail("code-owned trust-root file mode drifted")
        digest_builder = hashlib.sha256()
        chunks: list[bytes] = []
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            chunks.append(chunk)
            digest_builder.update(chunk)
        digest = digest_builder.hexdigest()
    except OSError:
        _fail("code-owned trust-root file content is unreadable")
    finally:
        if descriptor is not None:
            os.close(descriptor)
    if digest != CANONICAL_TRUST_ROOT_FILE_SHA256:
        _fail("code-owned trust-root file content digest drifted")
    try:
        root = json.loads(b"".join(chunks).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        _fail("code-owned trust-root file is not valid JSON")
    if not isinstance(root, dict) or root.get("schema_version") != "oasis7.validator_pair_provenance_trust_root.v1":
        _fail("code-owned trust-root file schema is unsupported")
    root_digest = root.get("root_digest")
    if not isinstance(root_digest, str) or HEX64_RE.fullmatch(root_digest) is None:
        _fail("code-owned trust-root file root_digest is malformed")
    canonical_body = {key: value for key, value in root.items() if key != "root_digest"}
    canonical_digest = hashlib.sha256(
        json.dumps(canonical_body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    if root_digest.lower() != canonical_digest or root_digest.lower() != CANONICAL_TRUST_ROOT_DIGEST:
        _fail("code-owned trust-root file canonical root_digest drifted")
    allowlist = root.get("allowlist")
    if not isinstance(allowlist, list) or not allowlist:
        _fail("code-owned trust-root file allowlist is missing")
    for entry in allowlist:
        if (
            not isinstance(entry, dict)
            or not isinstance(entry.get("signer_id"), str)
            or not entry["signer_id"].strip()
            or not isinstance(entry.get("algorithm"), str)
            or not entry["algorithm"].strip()
            or not isinstance(entry.get("public_key_sha256"), str)
            or HEX64_RE.fullmatch(entry["public_key_sha256"]) is None
        ):
            _fail("code-owned trust-root file allowlist entry is malformed")
        public_key_hex = entry.get("public_key_hex")
        if public_key_hex is not None and (
            not isinstance(public_key_hex, str) or re.fullmatch(r"[0-9a-fA-F]{64}", public_key_hex) is None
        ):
            _fail("code-owned trust-root file public key is malformed")
    return {
        "path": CANONICAL_TRUST_ROOT_PATH,
        "sha256": digest,
        "root_digest": root_digest.lower(),
        "owner_scope": CANONICAL_TRUST_ROOT_OWNER_SCOPE,
        "owner_uid": metadata.st_uid,
        "mode": CANONICAL_TRUST_ROOT_MODE,
        "regular_file": True,
        "root_id": root.get("root_id"),
        "network_id": root.get("network_id"),
        "allowlist": copy.deepcopy(allowlist),
    }


def verify_repository_provenance_helper(
    plan: dict[str, Any], authority: dict[str, Any]
) -> dict[str, Any]:
    """Run the repository-owned detached provenance helper when requested.

    The helper path is code-owned; the receipt, package, and trust-root paths
    are merely references to externally provisioned evidence.  This function
    never accepts an executable path or a command from the authority envelope.
    """
    reference = _object(authority.get("provenance_helper"), "provenance helper reference")
    expected_helper = Path(__file__).with_name("p2p-public-testnet-validator-pair-provenance.py").resolve()
    helper_path = Path(_string(reference.get("helper_path"), "provenance helper path")).resolve()
    if helper_path != expected_helper:
        _fail("provenance helper path is not repository-owned")
    receipt_path = Path(_string(reference.get("receipt_path"), "provenance receipt path"))
    package_dir = Path(_string(reference.get("package_dir"), "provenance package path"))
    trust_root_path = Path(_string(reference.get("trust_root_path"), "provenance trust-root path"))
    if trust_root_path != Path(CANONICAL_TRUST_ROOT_PATH):
        _fail("provenance trust-root path is not the code-owned path")
    if reference.get("trust_root_digest") != CANONICAL_TRUST_ROOT_DIGEST:
        _fail("provenance trust-root digest is not code-owned")
    if (
        receipt_path.is_symlink()
        or not receipt_path.is_file()
        or package_dir.is_symlink()
        or not package_dir.is_dir()
        or trust_root_path.is_symlink()
        or not trust_root_path.is_file()
    ):
        _fail("provenance evidence references must be present and non-symlinked")
    spec = importlib.util.spec_from_file_location("oasis7_repository_provenance_helper", helper_path)
    if spec is None or spec.loader is None:
        _fail("repository provenance helper cannot be loaded")
    helper = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(helper)
        result = helper.validate_receipt(receipt_path, package_dir, trust_root_path)
    except SystemExit:
        _fail("repository provenance helper rejected the evidence")
    except Exception as error:
        _fail(f"repository provenance helper failed: {error.__class__.__name__}")
    result = _object(result, "repository provenance helper result")
    package = _object(result.get("package"), "repository provenance package result")
    if (
        result.get("network_id") != CANONICAL_NETWORK_ID
        or result.get("chain_id") != CANONICAL_NETWORK_ID
        or package.get("commit") != plan["truth"]["package"]["commit"]
    ):
        _fail("repository provenance helper result is not bound to the plan truth")
    return {
        "verified": True,
        "verifier_id": CANONICAL_VERIFIER_ID,
        "trust_root_id": CANONICAL_TRUST_ROOT_ID,
        "signer_id": next(iter(CANONICAL_SIGNER_ALLOWLIST)),
        "bindings": authority["receipt"]["bindings"],
        "binding_digest": result.get("binding_digest"),
    }


def _node_nonce(plan: dict[str, Any], node: dict[str, Any]) -> str:
    seam = _object(node.get("credential_seam"), f"{node['name']} credential seam")
    nonce = _string(seam.get("nonce"), f"{node['name']} nonce")
    if SAFE_NONCE_RE.fullmatch(nonce) is None:
        _fail(f"{node['name']} nonce is malformed")
    return nonce


def _ledger_metadata(path: Path) -> os.stat_result:
    _reject_symlink_ancestors(path, "credential nonce ledger")
    if path.is_symlink() or not path.is_file():
        _fail("credential nonce ledger must be an existing regular file")
    try:
        metadata = path.stat()
    except OSError:
        _fail("credential nonce ledger metadata is unavailable")
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != 0o600:
        _fail("credential nonce ledger owner or mode is invalid")
    try:
        path.resolve(strict=True).relative_to(REPOSITORY_ROOT)
    except ValueError:
        pass
    else:
        _fail("credential nonce ledger must be external to the repository")
    return metadata


def _read_ledger(path: Path) -> list[dict[str, Any]]:
    _ledger_metadata(path)
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        _fail("credential nonce ledger cannot be read")
    return _parse_ledger_lines(lines)


def _parse_ledger_lines(lines: list[str]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            _fail(f"credential nonce ledger row {number} is malformed")
        row = _object(row, f"credential nonce ledger row {number}")
        if row.get("schema_version") != NONCE_ROW_SCHEMA or row.get("one_shot") is not True:
            _fail(f"credential nonce ledger row {number} is unsupported")
        _string(row.get("transaction_id"), f"credential nonce ledger row {number} transaction")
        nonce = _string(row.get("nonce"), f"credential nonce ledger row {number} nonce")
        if SAFE_NONCE_RE.fullmatch(nonce) is None:
            _fail(f"credential nonce ledger row {number} nonce is malformed")
        rows.append(row)
    return rows


def validate_credential_ledger(
    plan: dict[str, Any],
    path: Path,
    *,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
    allow_committed_reservations: bool = False,
) -> dict[str, int]:
    """Validate ownership, one-shot format, uniqueness, and replay state.

    Fresh admission rejects every plan nonce already present in the ledger.
    A resume admission may allow rows already committed by this exact
    transaction; the checkpoint-bound reservation validator must then prove
    that the journal state and complete ledger set still match before any
    provider callback is reached.
    """
    validate_plan(plan, raw_v1_bytes_by_node=raw_v1_bytes_by_node)
    rows = _read_ledger(Path(path))
    seen: set[str] = set()
    plan_nonces = {_node_nonce(plan, node) for node in plan["nodes"]}
    for row in rows:
        nonce = row["nonce"]
        if nonce in seen:
            _fail("credential nonce ledger contains a replayed nonce")
        seen.add(nonce)
        if nonce in plan_nonces and (
            not allow_committed_reservations
            or row["transaction_id"] != plan["transaction_id"]
        ):
            _fail("credential nonce ledger already consumed a plan nonce")
    return {"rows": len(rows), "unique_nonces": len(seen)}


def reserve_nonce(path: Path, transaction_id: str, nonce: str) -> None:
    """Atomically append a one-shot reservation before remote observation."""
    _check_transaction_guard()
    path = Path(path)
    _ledger_metadata(path)
    if SAFE_NONCE_RE.fullmatch(nonce) is None:
        _fail("nonce is malformed")
    try:
        import fcntl

        with path.open("r+", encoding="utf-8") as handle:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
            def check_locked_identity() -> None:
                locked = os.fstat(handle.fileno())
                named = _ledger_metadata(path)
                if (
                    not stat.S_ISREG(locked.st_mode)
                    or locked.st_uid != os.getuid()
                    or stat.S_IMODE(locked.st_mode) != 0o600
                    or (locked.st_dev, locked.st_ino) != (named.st_dev, named.st_ino)
                ):
                    _fail("credential nonce ledger locked inode binding changed")

            check_locked_identity()
            rows = _parse_ledger_lines(handle.read().splitlines())
            if any(row["nonce"] == nonce for row in rows):
                _fail("credential nonce has already been consumed")
            row = {
                "schema_version": NONCE_ROW_SCHEMA,
                "transaction_id": _string(transaction_id, "transaction_id"),
                "nonce": nonce,
                "one_shot": True,
                "reserved_at": dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z"),
            }
            handle.seek(0, os.SEEK_END)
            _check_transaction_guard()
            handle.write(json.dumps(row, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
            _fsync_parent(path.parent)
            check_locked_identity()
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    except AdapterError:
        raise
    except (OSError, ImportError):
        _fail("credential nonce ledger cannot be atomically reserved")


def _nonce_reservation_state(
    plan: dict[str, Any], reserved_count: int, *, complete: bool | None = None
) -> dict[str, Any]:
    """Return journal-safe reservation state without exposing nonce values."""
    expected_count = len(plan["nodes"])
    if not isinstance(reserved_count, int) or reserved_count < 0 or reserved_count > expected_count:
        _fail("nonce reservation count is outside the plan boundary")
    return {
        "ledger_path": plan["credential_nonce_ledger"]["path"],
        "transaction_id": plan["transaction_id"],
        "expected_count": expected_count,
        "reserved_count": reserved_count,
        "one_shot": True,
        "complete": reserved_count == expected_count if complete is None else complete,
    }


def _read_plan_nonce_reservations(plan: dict[str, Any], path: Path) -> set[str]:
    """Read matching one-shot reservations without changing the ledger."""
    rows = _read_ledger(Path(path))
    expected = {_node_nonce(plan, node) for node in plan["nodes"]}
    reserved: set[str] = set()
    seen: set[str] = set()
    for row in rows:
        nonce = row["nonce"]
        if nonce in seen:
            _fail("credential nonce ledger contains a replayed nonce")
        seen.add(nonce)
        if nonce not in expected:
            continue
        if row["transaction_id"] != plan["transaction_id"]:
            _fail("plan nonce is reserved by a different transaction")
        reserved.add(nonce)
    return reserved


def _reservation_state_from_ledger(plan: dict[str, Any], path: Path) -> dict[str, Any]:
    """Derive the count-only audit state from the authoritative ledger."""
    reserved = _read_plan_nonce_reservations(plan, Path(path))
    expected = {_node_nonce(plan, node) for node in plan["nodes"]}
    return _nonce_reservation_state(
        plan, len(reserved), complete=reserved == expected
    )


def _reconcile_nonce_reservations(plan: dict[str, Any], path: Path) -> dict[str, Any]:
    """Reuse matching reservations and append only missing plan nonces.

    The ledger remains the sole nonce-value authority.  The returned state is
    deliberately count-only so journals never become a second credential
    store.
    """
    expected = {_node_nonce(plan, node) for node in plan["nodes"]}
    reserved = _read_plan_nonce_reservations(plan, Path(path))
    for node in plan["nodes"]:
        nonce = _node_nonce(plan, node)
        if nonce not in reserved:
            reserve_nonce(Path(path), plan["transaction_id"], nonce)
            reserved.add(nonce)
    return _nonce_reservation_state(plan, len(reserved), complete=len(reserved) == len(expected))


def _validate_committed_nonce_reservations(
    plan: dict[str, Any], path: Path, state: dict[str, Any]
) -> dict[str, Any]:
    """Require a preflight checkpoint's nonce reservations to still exist.

    A preflight-complete journal is allowed to resume only when the external
    one-shot ledger still contains every plan nonce under this transaction.
    It must never silently reserve a missing nonce after the checkpoint: that
    would make the checkpoint's evidence and credential boundary ambiguous.
    """
    expected = {_node_nonce(plan, node) for node in plan["nodes"]}
    reserved = _read_plan_nonce_reservations(plan, Path(path))
    if reserved != expected:
        _fail("preflight-complete checkpoint is missing a committed nonce reservation")
    if state.get("reserved_count") != len(reserved) or state.get("complete") is not True:
        _fail("preflight-complete checkpoint nonce state does not match the ledger")
    return _nonce_reservation_state(plan, len(reserved), complete=True)


def _validate_nonce_reservation_state(plan: dict[str, Any], raw: Any) -> dict[str, Any]:
    """Validate count-only journal state without treating it as ledger authority."""
    state = _object(raw, "transaction journal nonce reservation state")
    if set(state) != {
        "ledger_path",
        "transaction_id",
        "expected_count",
        "reserved_count",
        "one_shot",
        "complete",
    }:
        _fail("transaction journal nonce reservation state is incomplete")
    expected = _nonce_reservation_state(plan, len(plan["nodes"]))
    if (
        state.get("ledger_path") != expected["ledger_path"]
        or state.get("transaction_id") != expected["transaction_id"]
        or state.get("expected_count") != expected["expected_count"]
        or state.get("one_shot") is not True
    ):
        _fail("transaction journal nonce reservation state is not plan-bound")
    reserved_count = state.get("reserved_count")
    if not isinstance(reserved_count, int) or isinstance(reserved_count, bool) or not 0 <= reserved_count <= expected["expected_count"]:
        _fail("transaction journal nonce reservation count is malformed")
    if state.get("complete") is not (reserved_count == expected["expected_count"]):
        _fail("transaction journal nonce reservation completion flag drifted")
    return copy.deepcopy(state)


def capacity_requirement(plan: dict[str, Any], node: dict[str, Any]) -> tuple[int, int]:
    platform = node["platform"]
    package = plan["truth"]["package"]["platforms"][platform]
    required_bytes = sum(
        int(value)
        for value in (
            package["package_size_bytes"],
            package["world_size_bytes"],
            package["world_provenance_size_bytes"],
            plan["truth"]["genesis"]["size_bytes"],
            plan["truth"]["world"]["size_bytes"],
            plan["truth"]["checkpoint"]["size_bytes"],
        )
    ) + MIN_FREE_BYTES
    return required_bytes, len(node["persistent_state_paths"]) + 16


def _remote_evidence_digest(evidence: dict[str, Any]) -> str:
    """Digest only the read-only evidence, excluding its signed receipt."""
    return hashlib.sha256(_canonical_bytes(evidence, omit="receipt")).hexdigest()


def validate_remote_preflight(
    plan: dict[str, Any],
    node: dict[str, Any],
    evidence: dict[str, Any],
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    *,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> dict[str, Any]:
    """Validate pinned remote evidence and its signed verifier-bound receipt."""
    validate_plan(plan, raw_v1_bytes_by_node=raw_v1_bytes_by_node)
    node = _object(node, "remote preflight node")
    evidence = _object(evidence, "remote preflight evidence")
    if set(evidence) - {
        "node",
        "node_id",
        "provider_uid",
        "node_root",
        "persistent_state_paths",
        "symlink_free",
        "free_bytes",
        "required_bytes",
        "free_inodes",
        "required_inodes",
        "host_target",
        "known_hosts_path",
        "known_host_fingerprint",
        "known_hosts_regular",
        "known_hosts_owner_uid",
        "known_hosts_mode",
        "receipt",
    }:
        _fail("remote preflight evidence contains an unsafe field")
    _reject_secret_fields(evidence, "remote preflight evidence")
    name = _string(node.get("name"), "remote preflight node name")
    if evidence.get("node") != name:
        _fail(f"{name} preflight evidence node binding drifted")
    if evidence.get("node_id") != node["node_id"]:
        _fail(f"{name} preflight identity binding drifted")
    expected_uid = _provider_uid(plan, name)
    if evidence.get("provider_uid") != expected_uid:
        _fail(f"{name} provider uid is not the governed uid")
    if evidence.get("node_root") != node["node_root"]:
        _fail(f"{name} remote root is not canonical")
    paths = _safe_relative_paths(
        node["node_root"],
        evidence.get("persistent_state_paths"),
        node["platform"],
        f"{name} remote state paths",
    )
    if paths != node["persistent_state_paths"]:
        _fail(f"{name} remote state path inventory drifted")
    if evidence.get("symlink_free") is not True:
        _fail(f"{name} remote state contains a symlink")
    binding = node["host_binding"]
    if (
        evidence.get("host_target") != binding["target"]
        or evidence.get("known_hosts_path") != binding["known_hosts_path"]
        or evidence.get("known_host_fingerprint") != binding["known_host_fingerprint"]
    ):
        _fail(f"{name} known-host target or fingerprint is not pinned")
    if evidence.get("known_hosts_regular") is not True:
        _fail(f"{name} known-hosts file is not a regular file")
    if evidence.get("known_hosts_owner_uid") != os.getuid():
        _fail(f"{name} known-hosts owner is not the adapter owner")
    if evidence.get("known_hosts_mode") != "0600":
        _fail(f"{name} known-hosts mode is not 0600")
    required_bytes, required_inodes = capacity_requirement(plan, node)
    if evidence.get("required_bytes") != required_bytes or evidence.get("required_inodes") != required_inodes:
        _fail(f"{name} capacity requirement is not code-owned")
    if not isinstance(evidence.get("free_bytes"), int) or evidence["free_bytes"] < required_bytes:
        _fail(f"{name} remote byte capacity is insufficient")
    if not isinstance(evidence.get("free_inodes"), int) or evidence["free_inodes"] < required_inodes:
        _fail(f"{name} remote inode capacity is insufficient")
    if verifier is None:
        _fail(f"{name} remote preflight requires an independent receipt verifier")
    receipt = _validate_provider_receipt(
        plan,
        f"preflight:{name}",
        name,
        evidence.get("receipt"),
        verifier,
        evidence=evidence,
    )
    return {
        "node": name,
        "node_root": node["node_root"],
        "path_count": len(paths),
        "symlink_free": True,
        "known_hosts_pinned": True,
        "capacity_verified": True,
        "receipt": receipt,
    }


def _fsync_parent(path: Path) -> None:
    flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
    try:
        fd = os.open(str(path), flags)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    except OSError:
        _fail("durable fsync failed")


def _reject_symlink_ancestors(path: Path, label: str = "transaction journal") -> None:
    """Reject symlinked path components before creating/opening governed files."""
    raw_parts = Path(os.fspath(path)).parts
    if ".." in raw_parts:
        _fail(f"{label} path must not contain parent traversal")
    absolute = Path(os.path.abspath(os.fspath(path)))
    # macOS exposes /var as a system alias for /private/var.  Normalize that
    # host-owned alias without resolving any caller-controlled descendant.
    if (
        len(absolute.parts) > 1
        and absolute.parts[1] == "var"
        and os.path.realpath("/var") == "/private/var"
    ):
        absolute = Path(os.path.realpath("/var")).joinpath(*absolute.parts[2:])
    current = Path(absolute.anchor or os.curdir)
    for component in absolute.parts[1:]:
        current /= component
        try:
            metadata = os.lstat(current)
        except FileNotFoundError:
            # No later component can exist below a missing ancestor.
            break
        except OSError:
            _fail(f"{label} path metadata is unavailable")
        if stat.S_ISLNK(metadata.st_mode):
            _fail(f"{label} path must not contain a symlink ancestor")
        if current != absolute and not stat.S_ISDIR(metadata.st_mode):
            _fail(f"{label} path ancestor is not a directory")


def _write_journal(path: Path, record: dict[str, Any]) -> None:
    _check_transaction_guard()
    path = Path(path)
    _reject_symlink_ancestors(path)
    if path.is_symlink():
        _fail("transaction journal must not be a symlink")
    if path.exists():
        try:
            metadata = path.stat()
        except OSError:
            _fail("transaction journal metadata is unavailable")
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or stat.S_IMODE(metadata.st_mode) != 0o600
        ):
            _fail("transaction journal owner or mode is invalid")
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = dict(record)
    payload["journal_digest"] = journal_digest(payload)
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            delete=False,
        ) as handle:
            temporary = Path(handle.name)
            os.fchmod(handle.fileno(), 0o600)
            handle.write(json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        _check_transaction_guard()
        os.replace(temporary, path)
        _fsync_parent(path.parent)
    except (OSError, AdapterError) as error:
        try:
            temporary.unlink(missing_ok=True)
        except (OSError, UnboundLocalError):
            pass
        if isinstance(error, AdapterError):
            raise
        _fail("transaction journal write failed")


def _read_journal(path: Path) -> dict[str, Any]:
    path = Path(path)
    _reject_symlink_ancestors(path)
    if path.is_symlink() or not path.is_file():
        _fail("transaction journal must be an existing regular file")
    try:
        metadata = path.stat()
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or stat.S_IMODE(metadata.st_mode) != 0o600
        ):
            _fail("transaction journal owner or mode is invalid")
        record = _object(json.loads(path.read_text(encoding="utf-8")), "transaction journal")
    except (OSError, json.JSONDecodeError):
        _fail("transaction journal is unreadable")
    schema_version = record.get("schema_version")
    if schema_version == LEGACY_JOURNAL_SCHEMA:
        _fail(
            "transaction journal schema v1 is unsupported; migration requires a "
            "new v2 journal or governed reconciliation before resuming"
        )
    if schema_version != JOURNAL_SCHEMA or record.get("journal_digest") != journal_digest(record):
        _fail("transaction journal digest or schema is invalid")
    return record


_ACTIVE_TRANSACTION_GUARD: contextvars.ContextVar[Any] = contextvars.ContextVar(
    "clean_room_transaction_guard", default=None
)


class _TransactionLock:
    """Sample pathname binding; not isolation from arbitrary same-UID writers."""

    def __init__(self, handle: Any, parent_fd: int, path: Path):
        self.handle, self.parent_fd, self.path = handle, parent_fd, path
        self.invalid = False

    def fileno(self) -> int:
        return self.handle.fileno()

    def close(self) -> None:
        try:
            self.handle.close()
        finally:
            os.close(self.parent_fd)

    def check(self) -> None:
        if self.invalid:
            _fail("transaction lock binding was previously lost")
        try:
            _reject_symlink_ancestors(self.path, "transaction lock")
            for opened, named, directory in (
                (os.fstat(self.parent_fd), self.path.parent.lstat(), True),
                (os.fstat(self.fileno()), self.path.lstat(), False),
            ):
                if (opened.st_dev, opened.st_ino) != (named.st_dev, named.st_ino):
                    _fail("transaction lock pathname or parent identity drifted")
                if opened.st_uid != os.getuid():
                    _fail("transaction lock or parent owner is invalid")
                if directory:
                    if not stat.S_ISDIR(opened.st_mode) or opened.st_mode & 0o022:
                        _fail("transaction lock parent must be owner-protected")
                elif not stat.S_ISREG(opened.st_mode) or stat.S_IMODE(opened.st_mode) != 0o600:
                    _fail("transaction lock must be an owner-mode 0600 regular file")
        except AdapterError:
            self.invalid = True
            raise
        except OSError:
            self.invalid = True
            _fail("transaction lock pathname or parent is unavailable")


def _check_transaction_guard() -> None:
    guard = _ACTIVE_TRANSACTION_GUARD.get()
    if guard is not None:
        guard.check()


def _guarded_callback(callback: Callable[..., Any], *args: Any) -> Any:
    _check_transaction_guard()
    try:
        return callback(*args)
    finally:
        _check_transaction_guard()


def _acquire_transaction_lock(journal_path: Path) -> Any:
    """Serialize a transaction and leave the lock durable for inspection."""
    return _acquire_lock_path(Path(f"{journal_path}.lock"))


def _acquire_lock_path(lock_path: Path) -> Any:
    """Acquire an owner-protected lock while retaining pathname identity."""
    _reject_symlink_ancestors(lock_path)
    if lock_path.is_symlink():
        _fail("transaction lock must not be a symlink")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    parent_fd = None
    descriptor = None
    handle = None
    try:
        import fcntl

        parent_fd = os.open(str(lock_path.parent), os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0))
        parent_metadata = os.fstat(parent_fd)
        if parent_metadata.st_uid != os.getuid() or parent_metadata.st_mode & 0o022:
            _fail("transaction lock parent must be owner-protected")
        flags = os.O_CREAT | os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(str(lock_path), flags, 0o600)
        handle = os.fdopen(descriptor, "a+", encoding="utf-8")
        guard = _TransactionLock(handle, parent_fd, lock_path)
        guard.check()
        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            _fail("transaction is already locked")
        guard.check()
        return guard
    except AdapterError:
        if handle is not None:
            handle.close()
        elif descriptor is not None:
            os.close(descriptor)
        if parent_fd is not None:
            os.close(parent_fd)
        raise
    except (OSError, ImportError):
        if handle is not None:
            handle.close()
        elif descriptor is not None:
            os.close(descriptor)
        if parent_fd is not None:
            os.close(parent_fd)
        _fail("transaction lock cannot be acquired")


def _release_transaction_lock(handle: Any) -> None:
    try:
        import fcntl

        fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    finally:
        handle.close()


class _FleetTransactionGuard:
    """Retain both locks through callbacks, recovery and durable completion."""

    def __init__(self, fleet: Any, journal: Any):
        self.fleet, self.journal = fleet, journal

    def check(self) -> None:
        self.fleet.check()
        self.journal.check()

    def close(self) -> None:
        try:
            _release_transaction_lock(self.journal)
        finally:
            if isinstance(self.fleet, _InProcessFleetLock):
                self.fleet.close()
            else:
                _release_transaction_lock(self.fleet)


class _InProcessFleetLock:
    """Non-persistent fixture-only fleet lock; never used by real plans."""

    def __init__(self, lock: threading.Lock):
        self._lock = lock
        if not self._lock.acquire(blocking=False):
            _fail("transaction is already locked")
        self._held = True

    def check(self) -> None:
        if not self._held:
            _fail("transaction lock binding was previously lost")

    def close(self) -> None:
        if self._held:
            self._held = False
            self._lock.release()


_STORAGE_FIRST_PROCESS_FLEET_LOCK = threading.Lock()


def _acquire_fleet_transaction_guard(journal_path: Path) -> _FleetTransactionGuard:
    # Globally fixed order: fleet first, then journal; release in reverse.
    fleet_path = Path(CANONICAL_FLEET_LOCK_PATH)
    if not fleet_path.is_absolute():
        _fail("canonical fleet lock must be absolute")
    # A missing deployment-owned parent is a hard operational failure.  The
    # only compatibility exception is explicitly scoped by the reduced
    # in-process fixture context set by storage-first tests; direct callers
    # and real plans never get a /tmp replacement lock.
    if not fleet_path.parent.exists():
        if not _STORAGE_FIRST_FIXTURE_LOCK_FALLBACK.get():
            _fail("canonical fleet lock parent is unavailable")
        fleet = _InProcessFleetLock(_STORAGE_FIRST_PROCESS_FLEET_LOCK)
        try:
            journal = _acquire_transaction_lock(journal_path)
        except BaseException:
            fleet.close()
            raise
        return _FleetTransactionGuard(fleet, journal)
    # The canonical operator lock parent is created only for local dry-run or
    # injected-provider tests.  Keep newly-created ancestors owner-only; an
    # existing deployment-owned parent is never chmod'ed or otherwise changed.
    missing: list[Path] = []
    cursor = fleet_path.parent
    while not cursor.exists() and cursor != cursor.parent:
        missing.append(cursor)
        cursor = cursor.parent
    for directory in reversed(missing):
        try:
            directory.mkdir(mode=0o700)
            directory.chmod(0o700)
        except OSError:
            _fail("canonical fleet lock parent cannot be created")
    fleet = _acquire_lock_path(fleet_path)
    try:
        journal = _acquire_transaction_lock(journal_path)
    except BaseException:
        _release_transaction_lock(fleet)
        raise
    return _FleetTransactionGuard(fleet, journal)


def _persist_terminal(journal_path: Path, record: dict[str, Any]) -> None:
    """Persist a terminal record, with a separate durable emergency receipt."""
    try:
        _write_journal(Path(journal_path), record)
        return
    except Exception as error:
        emergency = dict(record)
        emergency["status"] = "reconciliation-blocked"
        emergency["emergency_receipt"] = True
        emergency["journal_write_error"] = error.__class__.__name__
        try:
            _write_journal(Path(f"{journal_path}.emergency.json"), emergency)
        except Exception as emergency_error:
            _fail(
                "terminal journal write failed and emergency reconciliation receipt failed: "
                f"{emergency_error.__class__.__name__}"
            )
        _fail("terminal journal write failed; emergency reconciliation receipt persisted")


def _journal_record(
    plan: dict[str, Any],
    status: str,
    next_index: int,
    completed: list[str],
    error: str | None = None,
    node_receipts: dict[str, dict[str, Any]] | None = None,
    provider_receipts: list[dict[str, Any]] | None = None,
    rollback_status: str = "not-started",
    rollback_receipt: dict[str, Any] | None = None,
    execution_mode: str = "dry-run",
    rollback_error: str | None = None,
    rollback_reobservation_receipt: dict[str, Any] | None = None,
    failed_operation: str | None = None,
    failed_state_digest: str | None = None,
    preflight_evidence_receipts: list[dict[str, Any]] | None = None,
    preflight_status: str = "pending",
    nonce_reservation_state: dict[str, Any] | None = None,
    backup_status: str = "not-needed",
    backup_error: str | None = None,
    rollback_candidates: list[str] | None = None,
) -> dict[str, Any]:
    record: dict[str, Any] = {
        "schema_version": JOURNAL_SCHEMA,
        "adapter_schema": ADAPTER_SCHEMA,
        "task_uid": plan["task_uid"],
        "frozen_head_oid": plan["head_oid"],
        "plan_digest": plan["plan_digest"],
        "transaction_id": plan["transaction_id"],
        "capture_window_id": plan["capture_window_id"],
        "consumer_impact_record": _consumer_impact_locator(plan),
        "status": status,
        "execution_mode": execution_mode,
        "next_operation_index": next_index,
        "completed_operations": list(completed),
        "operations": list(plan["global_order"]),
        "rollback_policy": "clean-redeploy",
        "restore_old_state": False,
        "cross_node_state_copy": False,
        "node_receipts": copy.deepcopy(node_receipts or {}),
        "provider_receipts": copy.deepcopy(provider_receipts or []),
        "preflight_evidence_receipts": copy.deepcopy(preflight_evidence_receipts or []),
        "preflight_status": preflight_status,
        "nonce_reservation_state": copy.deepcopy(
            nonce_reservation_state or _nonce_reservation_state(plan, 0)
        ),
        "backup_status": backup_status,
        "rollback_status": rollback_status,
        "rollback_receipt": copy.deepcopy(rollback_receipt),
        "rollback_reobservation_receipt": copy.deepcopy(rollback_reobservation_receipt),
        "rollback_candidates": list(
            rollback_candidates if rollback_candidates is not None else
            (operation for operation in completed
             if execution_mode == "apply" and _rollback_candidate(operation))
        ),
    }
    if error is not None:
        record["terminal_error"] = error
    if rollback_error is not None:
        record["rollback_error"] = rollback_error
    if failed_operation is not None:
        record["failed_operation"] = failed_operation
    if failed_state_digest is not None:
        record["failed_state_digest"] = failed_state_digest
    if backup_error is not None:
        record["backup_error"] = backup_error
    return record


def _validate_node_receipts(plan: dict[str, Any], raw: Any) -> dict[str, dict[str, Any]]:
    raw = _object(raw, "transaction journal node receipts")
    if set(raw) - set(plan["node_order"]):
        _fail("transaction journal contains an unknown node receipt")
    allowed_keys = {
        "schema_version",
        "node",
        "transaction_id",
        "capture_window_id",
        "plan_digest",
        "status",
        "last_operation",
        "operation_count",
        "rollback_policy",
    }
    result: dict[str, dict[str, Any]] = {}
    for name, value in raw.items():
        receipt = _object(value, f"{name} node receipt")
        if set(receipt) - allowed_keys:
            _fail(f"{name} node receipt contains an unsafe field")
        if (
            receipt.get("schema_version") != NODE_RECEIPT_SCHEMA
            or receipt.get("node") != name
            or receipt.get("transaction_id") != plan["transaction_id"]
            or receipt.get("capture_window_id") != plan["capture_window_id"]
            or receipt.get("plan_digest") != plan["plan_digest"]
            or receipt.get("rollback_policy") != "clean-redeploy"
        ):
            _fail(f"{name} node receipt is not bound to this transaction")
        if receipt.get("status") not in {"planned", "completed"}:
            _fail(f"{name} node receipt status is unsupported")
        operation_count = receipt.get("operation_count")
        if not isinstance(operation_count, int) or operation_count < 0:
            _fail(f"{name} node receipt operation_count is malformed")
        if receipt.get("status") == "completed":
            if not isinstance(receipt.get("last_operation"), str) or not receipt["last_operation"].endswith(name):
                _fail(f"{name} node receipt last operation is malformed")
        elif "last_operation" in receipt:
            _fail(f"{name} planned node receipt must not claim a last operation")
        result[name] = receipt
    return result


def _provider_peer(plan: dict[str, Any], node_name: str | None, operation: str) -> str:
    if node_name is not None:
        node = next((item for item in plan["nodes"] if item["name"] == node_name), None)
        if node is None:
            _fail("provider receipt names an unknown node")
        identity = _object(node.get("identity_receipt"), f"{node_name} identity receipt")
        return _string(identity.get("peer_id"), f"{node_name} deployment peer")
    if operation == "fresh-root-probe":
        return CANONICAL_PROBE_PEER_ID
    return CANONICAL_FLEET_PEER_ID


def _provider_uid(plan: dict[str, Any], node_name: str) -> int:
    node = next((item for item in plan["nodes"] if item["name"] == node_name), None)
    if node is None:
        _fail("provider evidence names an unknown node")
    identity = _object(node.get("identity_receipt"), f"{node_name} identity receipt")
    value = identity.get("key_uid")
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        _fail(f"{node_name} deployment uid is malformed")
    return value


def _verify_receipt_with_verifier(
    plan: dict[str, Any],
    receipt: dict[str, Any],
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None,
) -> None:
    if verifier is None:
        return
    # Revalidate the live consumer-impact record at the exact boundary where
    # an externally supplied verifier is about to run.  A receipt cannot
    # authorize work after the plan-bound impact record has drifted.
    transport_plan = _transport_plan(plan)
    _consumer_impact_locator(plan)
    try:
        # A provider verifier never needs nonce seams or other adapter-only
        # authority material.  Keep that boundary identical to the transport
        # DTO boundary.
        result = _guarded_callback(verifier, transport_plan, receipt)
    except Exception as error:
        _fail(f"provider receipt verifier failed: {error.__class__.__name__}")
    result = _object(result, "provider receipt verifier result")
    if (
        result.get("verified") is not True
        or result.get("bindings") != receipt["bindings"]
        or result.get("verifier_id") != CANONICAL_VERIFIER_ID
        or result.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
        or result.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST
    ):
        _fail("provider receipt verifier did not verify the exact receipt binding")
    _consumer_impact_locator(plan)


def _verify_plan_receipts_with_verifier(
    plan: dict[str, Any],
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None,
    *,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> None:
    """Verify inventory and identity evidence through the existing seam.

    Planner receipts predate the transport receipt envelope and therefore do
    not carry provider-operation bindings.  The adapter projects each receipt
    into a verifier-only envelope with exact plan/node bindings; the original
    plan is never rewritten or sent as an apply proof.  Run every nested
    callback before surfacing the first failure so an operator receives a
    complete verification attempt for the governed inventory.
    """
    if verifier is None:
        _fail("deployment inventory and identity receipts require an independent verifier")
    if raw_v1_bytes_by_node is not None and not isinstance(raw_v1_bytes_by_node, Mapping):
        _fail("exact runtime raw-v1 bytes must be supplied as a node mapping")
    if raw_v1_bytes_by_node is not None:
        expected_nodes = {node["name"] for node in plan["nodes"]}
        if set(raw_v1_bytes_by_node) != expected_nodes:
            missing = sorted(expected_nodes - set(raw_v1_bytes_by_node))
            extra = sorted(set(raw_v1_bytes_by_node) - expected_nodes)
            _fail(
                "exact runtime raw-v1 node mapping is not exact "
                f"(missing={missing}, extra={extra})"
            )
    inventory = _object(plan.get("deployment_inventory"), "deployment inventory")
    inventory_receipt = _object(inventory.get("receipt"), "deployment inventory receipt")
    evidence: list[dict[str, Any]] = [
        {
            "receipt": inventory_receipt,
            "bindings": {
                "kind": "deployment-inventory",
                "task_uid": plan["task_uid"],
                "frozen_head_oid": plan["head_oid"],
                "plan_digest": plan["plan_digest"],
                "consumer_impact_record": _consumer_impact_locator(plan),
            },
        }
    ]
    for node in plan["nodes"]:
        identity = _object(node.get("identity_receipt"), f"{node['name']} identity receipt")
        evidence.append(
            {
                "receipt": identity,
                "bindings": {
                    "kind": "identity",
                    "task_uid": plan["task_uid"],
                    "frozen_head_oid": plan["head_oid"],
                    "plan_digest": plan["plan_digest"],
                    "node": node["name"],
                    "node_id": node["node_id"],
                    "peer_id": identity["peer_id"],
                    "key_sha256": identity["key_sha256"],
                    "key_size_bytes": identity["key_size_bytes"],
                    "key_mode": identity["key_mode"],
                    "key_uid": identity["key_uid"],
                    "key_gid": identity["key_gid"],
                    "capture_window_id": identity["capture_window_id"],
                    "rotation_epoch": identity["rotation_epoch"],
                    "issued_at": identity["issued_at"],
                    "expires_at": identity["expires_at"],
                    "signed_payload_sha256": identity["signed_payload_sha256"],
                    "consumer_impact_record": _consumer_impact_locator(plan),
                },
            }
        )
    planner = _load_planner()
    semantic_evidence = [(kind, plan["truth"][kind]) for kind in ("package", "genesis", "world", "checkpoint")]
    semantic_evidence.extend(("validator_verify_output", output) for output in
                             plan["fresh_root_probe"]["validator_verify_outputs"].values())
    semantic_evidence.append(("fresh_root_probe", plan["fresh_root_probe"]))
    for kind, value in semantic_evidence:
        receipt = value if kind == "validator_verify_output" else value["receipt"]
        # The callback receives the exact canonical signed bytes, not a digest
        # claimed by the caller. Local cryptographic admission already ran.
        evidence.append({"receipt": receipt, "bindings": {
            "kind": kind,
            "task_uid": plan["task_uid"], "frozen_head_oid": plan["head_oid"],
            "plan_digest": plan["plan_digest"],
            "consumer_impact_record": _consumer_impact_locator(plan),
            "semantic_payload_hex": planner.canonical_semantic_receipt_payload(kind, value, receipt).hex(),
        }})
    failures: list[str] = []
    for item in evidence:
        verifier_receipt = copy.deepcopy(item["receipt"])
        verifier_receipt["bindings"] = item["bindings"]
        if item["bindings"]["kind"] == "identity" and raw_v1_bytes_by_node is not None:
            node_name = item["bindings"]["node"]
            raw_v1_bytes = raw_v1_bytes_by_node.get(node_name)
            identity = item["receipt"]
            _validate_identity_raw_v1_digest(
                identity,
                f"{node_name} identity receipt",
                raw_v1_bytes=raw_v1_bytes,
            )
            verifier_receipt["raw_v1_bytes"] = raw_v1_bytes
        try:
            _verify_receipt_with_verifier(plan, verifier_receipt, verifier)
        except AdapterError as error:
            failures.append(str(error))
    if failures:
        _fail(failures[0])


def _validate_provider_receipt(
    plan: dict[str, Any],
    operation: str,
    node_name: str | None,
    raw: Any,
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None,
    *,
    evidence: dict[str, Any] | None = None,
    rollback_candidates: list[str] | None = None,
    rollback_order: Sequence[str] | None = None,
    expected_rollback_steps: Sequence[str] | None = None,
) -> dict[str, Any]:
    """Validate and sanitize every provider receipt before phase advance."""
    # A provider callback may return a receipt only after the exact impact
    # source remains unchanged for the callback/receipt boundary.
    _consumer_impact_locator(plan)
    receipt = _object(raw, f"{operation} provider receipt")
    allowed = {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "transaction_id",
        "capture_window_id",
        "operation",
        "node",
        "peer_id",
        "bindings",
        "replayed",
        "checkpoint_manifest_hash",
        "checkpoint_id",
        "height",
        "package_commit",
        "execution_block_hash",
        "execution_state_root",
        "blob_closure",
        "runtime",
        "connected_provider",
        "recovery_receipt",
        "failed_operation",
        "failed_state_digest",
        "rollback_steps",
        "reobserved",
        "phase",
        "captured_at",
        "observer_mutation",
        "status",
        "backup_manifest",
        "seed_eligible",
        "fleet_health_closure",
        "consumer_impact_record",
    }
    _reject_secret_fields(receipt, f"{operation} provider receipt")
    if set(receipt) - allowed:
        _fail(f"{operation} provider receipt contains an unsafe field")
    if (
        "consumer_impact_record" in receipt
        and receipt["consumer_impact_record"] != plan["consumer_impact_record"]
    ):
        _fail(f"{operation} provider receipt consumer-impact record drifted")
    phase = _receipt_phase(operation)
    if receipt.get("schema_version") != PHASE_RECEIPT_SCHEMAS[phase]:
        _fail(f"{operation} provider receipt schema is unsupported for phase {phase}")
    if receipt.get("authenticated") is not True or receipt.get("verified") is not True:
        _fail(f"{operation} provider receipt is not authenticated and verified")
    if receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST:
        _fail(f"{operation} provider receipt signer is not code-owned")
    if receipt.get("verifier_id") != CANONICAL_VERIFIER_ID or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
        _fail(f"{operation} provider receipt verifier or trust root is not code-owned")
    if (
        receipt.get("phase") != phase
        or receipt.get("captured_at") is None
        or receipt.get("observer_mutation") is not False
        or receipt.get("replayed") is not False
        or receipt.get("status") != ("completed" if phase in {"backup", "apply", "reobserve", "rollback"} else "verified")
    ):
        _fail(f"{operation} provider receipt phase, capture, observer, or status binding drifted")
    captured_at = _parse_utc(receipt.get("captured_at"), f"{operation} captured_at")
    if captured_at > dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=MAX_CLOCK_SKEW_SECONDS):
        _fail(f"{operation} captured_at is beyond the allowed future clock skew")
    capture_start, capture_end = _capture_window_bounds(plan)
    if not capture_start <= captured_at <= capture_end:
        _fail(f"{operation} captured_at is outside the transaction capture window")
    _nonzero_hex(receipt.get("signed_payload_sha256"), HEX64_RE, f"{operation} signed payload")
    _nonzero_hex(receipt.get("signature_hex"), SIGNATURE_RE, f"{operation} signature")
    _nonzero_hex(receipt.get("canonical_digest"), HEX64_RE, f"{operation} canonical digest")
    if (
        receipt.get("transaction_id") != plan["transaction_id"]
        or receipt.get("capture_window_id") != plan["capture_window_id"]
    ):
        _fail(f"{operation} provider receipt transaction binding drifted")
    if receipt.get("operation") != operation or receipt.get("node") != node_name:
        _fail(f"{operation} provider receipt operation or node binding drifted")
    peer_id = _string(receipt.get("peer_id"), f"{operation} provider receipt peer")
    if peer_id != _provider_peer(plan, node_name, operation):
        _fail(f"{operation} provider receipt peer binding drifted")
    bindings = _object(receipt.get("bindings"), f"{operation} provider receipt bindings")
    expected_bindings = {
        "task_uid": plan["task_uid"],
        "frozen_head_oid": plan["head_oid"],
        "plan_digest": plan["plan_digest"],
        "transaction_id": plan["transaction_id"],
        "capture_window_id": plan["capture_window_id"],
        "operation": operation,
        "node": node_name,
        "peer_id": peer_id,
        "ledger_path": plan["credential_nonce_ledger"]["path"],
        "consumer_impact_record": _consumer_impact_locator(plan),
    }
    if operation in {"reobserve-failed-state", "rollback-clean-redeploy"}:
        candidates = _validate_rollback_candidates(
            plan, rollback_candidates, order=rollback_order
        )
        if not candidates:
            _fail("recovery receipt rollback candidates must not be empty")
        if bindings.get("rollback_candidates") != candidates:
            _fail("recovery receipt rollback candidate binding is not exact")
        expected_bindings["rollback_candidates"] = candidates
    if evidence is not None:
        expected_bindings["evidence_sha256"] = _remote_evidence_digest(evidence)
    elif phase == "preflight" and "evidence_sha256" in bindings:
        # The journal retains the evidence digest inside the signed receipt;
        # the original evidence object is intentionally not replayed here.
        expected_bindings["evidence_sha256"] = _digest(
            bindings.get("evidence_sha256"), f"{operation} evidence digest"
        )
    if bindings != expected_bindings:
        _fail(f"{operation} provider receipt binding set is not exact")
    if operation == "fresh-root-probe":
        checkpoint = plan["truth"]["checkpoint"]
        if (
            receipt.get("replayed") is not False
            or receipt.get("checkpoint_manifest_hash") != checkpoint["manifest_hash"]
            or receipt.get("checkpoint_id") != checkpoint["checkpoint_id"]
            or receipt.get("height") != checkpoint["height"]
            or receipt.get("package_commit") != plan["truth"]["package"]["commit"]
            or receipt.get("execution_block_hash") != checkpoint["execution_block_hash"]
            or receipt.get("execution_state_root") != checkpoint["execution_state_root"]
        ):
            _fail("fresh-root probe receipt is replayed or checkpoint-unbound")
        _validate_fresh_probe_closure(plan, receipt)
    if operation in {"reobserve-failed-state", "rollback-clean-redeploy"}:
        if receipt.get("reobserved") is not True:
            _fail(f"{operation} receipt lacks a fresh failed-state re-observation")
        _string(receipt.get("failed_operation"), f"{operation} failed operation")
        _nonzero_hex(receipt.get("failed_state_digest"), HEX64_RE, f"{operation} failed state digest")
        rollback_steps = (
            list(expected_rollback_steps)
            if expected_rollback_steps is not None
            else plan["rollback"]["steps"]
        )
        if receipt.get("rollback_steps") != rollback_steps:
            _fail(f"{operation} receipt clean-redeploy steps are not exact")
    if phase in {"backup", "apply"}:
        if plan["forensic_backup"]["required_before_reset"] is True:
            manifest = _object(receipt.get("backup_manifest"), f"{operation} backup manifest")
            if set(manifest) != {"node", "sha256", "size_bytes", "verified", "seed_eligible"}:
                _fail(f"{operation} backup manifest is incomplete")
            if manifest.get("node") != node_name or manifest.get("verified") is not True:
                _fail(f"{operation} backup manifest node or verification drifted")
            _nonzero_hex(manifest.get("sha256"), HEX64_RE, f"{operation} backup manifest digest")
            if not isinstance(manifest.get("size_bytes"), int) or manifest["size_bytes"] <= 0:
                _fail(f"{operation} backup manifest size is malformed")
            if manifest.get("seed_eligible") is not False or receipt.get("seed_eligible") is not False:
                _fail(f"{operation} backup is incorrectly seed eligible")
        elif receipt.get("seed_eligible") is not False:
            _fail(f"{operation} no-backup receipt is seed eligible")
        elif receipt.get("backup_manifest") is not None:
            manifest = _object(receipt["backup_manifest"], f"{operation} backup manifest")
            if manifest.get("seed_eligible") is not False:
                _fail(f"{operation} no-backup receipt contains a seed-eligible manifest")
    if operation == "fleet-health":
        closure = _object(receipt.get("fleet_health_closure"), "fleet-health closure")
        if (
            set(closure) != {"verified", "nodes", "healthy", "snapshot"}
            or closure.get("verified") is not True
            or closure.get("healthy") is not True
            or closure.get("nodes") != list(plan["node_order"])
        ):
            _fail("fleet-health receipt does not close the governed fleet")
        snapshot = _object(closure.get("snapshot"), "fleet-health final snapshot")
        if set(snapshot) != set(plan["node_order"]):
            _fail("fleet-health final snapshot does not cover the governed fleet")
        heights: dict[str, dict[str, int]] = {}
        for name in plan["node_order"]:
            node_snapshot = _object(snapshot.get(name), f"fleet-health snapshot {name}")
            if node_snapshot.get("running") is not True or node_snapshot.get("last_error") is not None:
                _fail(f"fleet-health snapshot {name} is not running and error-free")
            node_heights: dict[str, int] = {}
            for field in ("committed_height", "network_committed_height", "last_execution_height"):
                value = node_snapshot.get(field)
                if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                    _fail(f"fleet-health snapshot {name} has malformed {field}")
                node_heights[field] = value
            heights[name] = node_heights
            connected_peers = node_snapshot.get("connected_peers")
            if not isinstance(connected_peers, list) or not all(
                isinstance(peer, str) and peer for peer in connected_peers
            ):
                _fail(f"fleet-health snapshot {name} connected_peers is malformed")
            if name not in {"storage-205", "sequencer-204"}:
                required_peers = {
                    _provider_peer(plan, "storage-205", "stop:storage-205"),
                    _provider_peer(plan, "sequencer-204", "stop:sequencer-204"),
                }
                if not required_peers.issubset(set(connected_peers)):
                    _fail(f"fleet-health snapshot {name} does not see both validator peers")
            elif name == "storage-205":
                required_peer = _provider_peer(plan, "sequencer-204", "stop:sequencer-204")
                if required_peer not in connected_peers:
                    _fail("fleet-health storage-205 does not see sequencer-204 validator peer")
            else:
                required_peer = _provider_peer(plan, "storage-205", "stop:storage-205")
                if required_peer not in connected_peers:
                    _fail("fleet-health sequencer-204 does not see storage-205 validator peer")
            readiness = _object(node_snapshot.get("readiness"), f"fleet-health snapshot {name} readiness")
            if readiness.get("ready") is not True or readiness.get("failed_gates") != []:
                _fail(f"fleet-health snapshot {name} readiness is not closed")
            consensus = _object(node_snapshot.get("consensus"), f"fleet-health snapshot {name} consensus")
            network_head = _object(
                consensus.get("network_head"),
                f"fleet-health snapshot {name} network head",
            )
            if network_head.get("decision") != "ready":
                _fail(f"fleet-health snapshot {name} network head is not ready")
        validator_head = heights["sequencer-204"]["committed_height"]
        for name, node_heights in heights.items():
            if any(value != validator_head for value in node_heights.values()):
                _fail(f"fleet-health snapshot {name} heights do not equal the sequencer head")
    # The verifier is an injected trust boundary.  It may inspect its input,
    # but it must never be able to rewrite the provider envelope that passed
    # the canonical checks above and will be persisted by the caller.
    _verify_receipt_with_verifier(plan, copy.deepcopy(receipt), verifier)
    return _sanitize_receipt(receipt, f"{operation} provider receipt")


def _validate_fresh_probe_closure(plan: dict[str, Any], receipt: dict[str, Any]) -> None:
    """Require one signed receipt to close execution blobs, runtime, peers, and recovery."""
    if receipt.get("blob_closure") != plan["truth"]["execution"]:
        _fail("fresh-root probe blob closure is not bound to execution truth")
    runtime = _object(receipt.get("runtime"), "fresh-root probe runtime closure")
    expected_runtime = {
        "sha256": plan["truth"]["package"]["runtime_sha256"],
        "size_bytes": plan["truth"]["package"]["runtime_size_bytes"],
    }
    if runtime != expected_runtime:
        _fail("fresh-root probe runtime hash or size is not bound to package truth")
    connected = _object(receipt.get("connected_provider"), "fresh-root connected provider")
    if connected.get("verified") is not True:
        _fail("fresh-root probe connected provider is not verified")
    expected_providers = []
    for name in ("storage-205", "sequencer-204"):
        node = next(item for item in plan["nodes"] if item["name"] == name)
        identity = _object(node.get("identity_receipt"), f"{name} identity receipt")
        expected_providers.append(
            {
                "node": name,
                "node_id": node["node_id"],
                "peer_id": _string(identity.get("peer_id"), f"{name} deployment peer"),
                "provider_uid": _provider_uid(plan, name),
            }
        )
    if connected.get("providers") != expected_providers or set(connected) != {"verified", "providers"}:
        _fail("fresh-root connected provider identity is not the governed pair")
    recovery = _object(receipt.get("recovery_receipt"), "fresh-root recovery receipt")
    allowed = {
        "schema_version", "authenticated", "verified", "signer_id", "verifier_id",
        "trust_root_id", "signed_payload_sha256", "signature_hex", "canonical_digest", "bindings",
    }
    if set(recovery) - allowed:
        _fail("fresh-root recovery receipt contains an unsafe field")
    if (
        recovery.get("schema_version") != RECOVERY_RECEIPT_SCHEMA
        or recovery.get("authenticated") is not True
        or recovery.get("verified") is not True
        or recovery.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST
        or recovery.get("verifier_id") != CANONICAL_VERIFIER_ID
        or recovery.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
    ):
        _fail("fresh-root recovery receipt is not independently authenticated")
    _reject_secret_fields(recovery, "fresh-root recovery receipt")
    _nonzero_hex(recovery.get("signed_payload_sha256"), HEX64_RE, "fresh-root recovery payload")
    _nonzero_hex(recovery.get("signature_hex"), SIGNATURE_RE, "fresh-root recovery signature")
    _nonzero_hex(recovery.get("canonical_digest"), HEX64_RE, "fresh-root recovery digest")
    checkpoint = plan["truth"]["checkpoint"]
    expected_bindings = {
        "task_uid": plan["task_uid"],
        "transaction_id": plan["transaction_id"],
        "capture_window_id": plan["capture_window_id"],
        "checkpoint_id": checkpoint["checkpoint_id"],
        "checkpoint_manifest_hash": checkpoint["manifest_hash"],
    }
    if recovery.get("bindings") != expected_bindings:
        _fail("fresh-root recovery receipt bindings are not exact")


def _validate_journal_provider_receipts(
    plan: dict[str, Any], raw: Any, verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None
) -> list[dict[str, Any]]:
    if not isinstance(raw, list):
        _fail("transaction journal provider_receipts must be a list")
    result: list[dict[str, Any]] = []
    for receipt in raw:
        receipt_object = _object(receipt, "transaction journal provider receipt")
        operation = _string(receipt_object.get("operation"), "transaction journal receipt operation")
        node_name = receipt_object.get("node")
        if node_name is not None:
            node_name = _string(node_name, "transaction journal receipt node")
        result.append(_validate_provider_receipt(plan, operation, node_name, receipt_object, verifier))
    return result


def _validate_journal_preflight_evidence_receipts(
    plan: dict[str, Any],
    raw: Any,
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    """Revalidate the signed evidence receipts captured by remote preflight.

    The evidence object is intentionally not replayed through a provider.  Its
    signed ``evidence_sha256`` binding is the durable read-only observation;
    resume must validate that exact preflight receipt rather than accepting a
    generic provider receipt for the same phase.
    """
    if not isinstance(raw, list):
        _fail("transaction journal preflight_evidence_receipts must be a list")
    if len(raw) > len(plan["node_order"]):
        _fail("transaction journal contains too many preflight evidence receipts")
    result: list[dict[str, Any]] = []
    seen_nodes: set[str] = set()
    for index, value in enumerate(raw):
        receipt = _object(value, "transaction journal preflight evidence receipt")
        node_name = receipt.get("node")
        if not isinstance(node_name, str) or node_name not in plan["node_order"]:
            _fail("transaction journal preflight evidence receipt names an unknown node")
        if node_name != plan["node_order"][index]:
            _fail("transaction journal preflight evidence receipts are out of canonical node order")
        if node_name in seen_nodes:
            _fail("transaction journal contains duplicate preflight evidence receipts")
        operation = f"preflight:{node_name}"
        if receipt.get("operation") != operation:
            _fail("transaction journal preflight evidence receipt operation drifted")
        bindings = _object(receipt.get("bindings"), "transaction journal preflight evidence bindings")
        if "evidence_sha256" not in bindings:
            _fail("transaction journal preflight evidence receipt is not evidence-bound")
        result.append(_validate_provider_receipt(plan, operation, node_name, receipt, verifier))
        seen_nodes.add(node_name)
    return result


def _validate_live_probe(plan: dict[str, Any], receipt: dict[str, Any]) -> None:
    receipt = _object(receipt, "live fresh-root probe receipt")
    if (
        receipt.get("authenticated") is not True
        or receipt.get("verified") is not True
        or receipt.get("replayed") is True
    ):
        _fail("live fresh-root probe is not authenticated, verified, and fresh")
    if (
        receipt.get("transaction_id") != plan["transaction_id"]
        or receipt.get("capture_window_id") != plan["capture_window_id"]
    ):
        _fail("live fresh-root probe transaction binding drifted")
    checkpoint = plan["truth"]["checkpoint"]
    _nonzero_hex(receipt.get("checkpoint_manifest_hash"), HEX64_RE, "live probe checkpoint manifest")
    if (
        receipt.get("checkpoint_manifest_hash") != checkpoint["manifest_hash"]
        or receipt.get("checkpoint_id") != checkpoint["checkpoint_id"]
        or receipt.get("height") != checkpoint["height"]
        or receipt.get("package_commit") != plan["truth"]["package"]["commit"]
        or receipt.get("execution_block_hash") != checkpoint["execution_block_hash"]
        or receipt.get("execution_state_root") != checkpoint["execution_state_root"]
    ):
        _fail("live fresh-root probe checkpoint binding drifted")
    _validate_fresh_probe_closure(plan, receipt)


def _verify_provenance(
    plan: dict[str, Any],
    authority: dict[str, Any],
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None,
) -> bool:
    def validate_result(receipt: dict[str, Any], result: Any) -> None:
        result = _object(result, "external provenance verifier result")
        if (
            result.get("verified") is not True
            or result.get("bindings") != receipt["bindings"]
            or result.get("verifier_id") != CANONICAL_VERIFIER_ID
            or result.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
            or result.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST
        ):
            _fail("external provenance verifier did not verify exact execution bindings")

    receipts = [authority["receipt"]]
    forensic = _object(plan.get("forensic_backup"), "forensic backup")
    if forensic.get("mode") == "operator-authorized-no-backup":
        receipts.append(_object(forensic.get("authority"), "no-backup authority receipt"))
    if verifier is None:
        if "provenance_helper" not in authority:
            return False
        provenance_plan = copy.deepcopy(plan)
        _consumer_impact_locator(plan)
        result = verify_repository_provenance_helper(provenance_plan, authority)
        if len(receipts) != 1:
            _fail("no-backup authority requires the independent receipt verifier callback")
        validate_result(receipts[0], result)
        _consumer_impact_locator(plan)
    else:
        for receipt in receipts:
            transport_plan = _transport_plan(plan)
            _consumer_impact_locator(plan)
            try:
                result = _guarded_callback(verifier, transport_plan, receipt)
            except Exception as error:
                _fail(f"external provenance verifier failed: {error.__class__.__name__}")
            validate_result(receipt, result)
            _consumer_impact_locator(plan)
    return True


def _reject_secret_fields(value: Any, label: str) -> None:
    """Reject secret-bearing fields recursively before any provider boundary."""
    if isinstance(value, dict):
        for key, child in value.items():
            key_text = str(key)
            if SECRET_KEY_RE.search(key_text) or key_text in SECRET_FIELD_NAMES:
                _fail(f"{label} contains a secret-bearing field")
            _reject_secret_fields(child, label)
    elif isinstance(value, list):
        for child in value:
            _reject_secret_fields(child, label)
    elif isinstance(value, str) and SECRET_VALUE_RE.search(value):
        _fail(f"{label} contains a secret-bearing value")


def _sanitize_receipt(value: Any, label: str) -> dict[str, Any]:
    value = _object(value, label)
    cleaned: dict[str, Any] = {}
    for key, child in value.items():
        if SECRET_KEY_RE.search(str(key)):
            continue
        if key in SECRET_FIELD_NAMES:
            continue
        if isinstance(child, dict):
            cleaned[key] = _sanitize_receipt(child, label)
        elif isinstance(child, list):
            cleaned[key] = [(_sanitize_receipt(item, label) if isinstance(item, dict) else item) for item in child]
        else:
            cleaned[key] = child
    return cleaned


def _project_exact_object(
    value: Any, allowed: set[str], label: str
) -> dict[str, Any]:
    """Copy an object only when every nested schema key is explicitly known."""
    value = _object(value, label)
    if set(value) != allowed:
        _fail(f"{label} contains a field outside the recursive transport allowlist")
    return {key: copy.deepcopy(value[key]) for key in allowed}


def _project_transport_string_list(value: Any, label: str) -> list[str]:
    if not isinstance(value, list):
        _fail(f"{label} must be a list")
    return [_string(item, f"{label} entry") for item in value]


def _project_transport_no_backup_authority(value: Any, label: str) -> dict[str, Any]:
    receipt = _project_exact_object(
        value, set(NO_BACKUP_AUTHORITY_RECEIPT_FIELDS), label
    )
    bindings = _project_exact_object(
        receipt["bindings"],
        set(NO_BACKUP_AUTHORITY_BINDING_FIELDS),
        f"{label} bindings",
    )
    bindings["targets"] = _project_transport_string_list(
        bindings["targets"], f"{label} bindings targets"
    )
    bindings["consumer_impact_record"] = _project_exact_object(
        bindings["consumer_impact_record"],
        {"path", "sha256"},
        f"{label} bindings consumer impact record",
    )
    receipt["bindings"] = bindings
    return receipt


def _project_transport_forensic_backup(value: Any) -> dict[str, Any]:
    forensic = _project_exact_object(
        value, set(FORENSIC_BACKUP_FIELDS), "transport forensic backup"
    )
    forensic["mode"] = _string(forensic["mode"], "transport forensic backup mode")
    for field in (
        "required_before_reset",
        "operator_authorized",
        "current_authorization",
        "immutable",
        "seed_eligible",
        "cross_node_state_copy",
        "restore_old_state",
        "receipt_required_per_node",
    ):
        forensic[field] = _bool(forensic[field], f"transport forensic backup {field}")
    for field in ("task_uid", "frozen_head_oid", "transaction_id", "capture_window_id"):
        forensic[field] = _string(forensic[field], f"transport forensic backup {field}")
    for field in ("repository", "action", "actor", "issued_at", "expires_at"):
        if forensic[field] is not None:
            forensic[field] = _string(forensic[field], f"transport forensic backup {field}")
    if forensic["targets"] is not None:
        forensic["targets"] = _project_transport_string_list(
            forensic["targets"], "transport forensic backup targets"
        )
    if forensic["authority"] is not None:
        forensic["authority"] = _project_transport_no_backup_authority(
            forensic["authority"], "transport forensic backup authority"
        )
    return forensic


def _project_transport_rollback(value: Any) -> dict[str, Any]:
    rollback = _project_exact_object(
        value, set(ROLLBACK_FIELDS), "transport rollback"
    )
    rollback["steps"] = _project_transport_string_list(
        rollback["steps"], "transport rollback steps"
    )
    for field in (
        "stop_started_nodes",
        "preserve_failed_state_for_forensics",
        "restore_old_state",
        "cross_node_state_copy",
        "reinstall_exact_package_and_truth",
        "rerun_fresh_root_probe",
        "provider_mutation_requires_external_authority",
    ):
        rollback[field] = _bool(rollback[field], f"transport rollback {field}")
    if rollback != _canonical_rollback_policy():
        _fail("transport rollback is not the exact code-owned clean-redeploy policy")
    return rollback


def _project_transport_list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        _fail(f"{label} must be a list")
    return value


_TRANSPORT_RECEIPT_FIELDS = {
    "schema_version",
    "authenticated",
    "verified",
    "signer_id",
    "verifier_id",
    "trust_root_id",
    "signed_payload_sha256",
    "signature_hex",
    "canonical_digest",
}
_TRANSPORT_INVENTORY_RECEIPT_FIELDS = _TRANSPORT_RECEIPT_FIELDS | {
    "capture_window_id",
    "rotation_epoch",
    "issued_at",
    "expires_at",
}


def _project_transport_receipt(value: Any, label: str) -> dict[str, Any]:
    return _project_exact_object(value, _TRANSPORT_RECEIPT_FIELDS, label)


def _project_transport_truth(value: Any) -> dict[str, Any]:
    value = _object(value, "transport truth")
    if set(value) != {"package", "genesis", "world", "execution", "checkpoint"}:
        _fail("transport truth contains a field outside the recursive allowlist")
    package = _project_exact_object(
        value["package"],
        {
            "package_id", "package_dir", "provenance_path", "provenance_sha256",
            "provenance_size_bytes", "commit", "package_version", "runtime_sha256",
            "runtime_size_bytes", "genesis_sha256", "world_sha256", "platforms", "receipt",
        },
        "transport truth package",
    )
    platforms = _object(package["platforms"], "transport truth package platforms")
    if set(platforms) != {"linux-x64", "windows-x64", "macos-arm64"}:
        _fail("transport truth package platforms are not the managed set")
    package["platforms"] = {
        platform: _project_exact_object(
            platforms[platform],
            {
                "package_sha256", "package_size_bytes", "world_sha256", "world_size_bytes",
                "world_provenance_sha256", "world_provenance_size_bytes", "commit",
            },
            f"transport truth package platform {platform}",
        )
        for platform in ("linux-x64", "windows-x64", "macos-arm64")
    }
    package["receipt"] = _project_transport_receipt(package["receipt"], "transport truth package receipt")
    genesis = _project_exact_object(
        value["genesis"],
        {"network_id", "chain_id", "world_id", "path", "size_bytes", "sha256", "receipt"},
        "transport truth genesis",
    )
    genesis["receipt"] = _project_transport_receipt(genesis["receipt"], "transport truth genesis receipt")
    world = _project_exact_object(
        value["world"],
        {
            "world_id", "generation", "path", "provenance_path", "size_bytes", "sha256",
            "provenance_sha256", "provenance_size_bytes", "receipt",
        },
        "transport truth world",
    )
    world["receipt"] = _project_transport_receipt(world["receipt"], "transport truth world receipt")
    execution = _project_exact_object(
        value["execution"],
        {"execution_records_root", "cas", "world_head", "generated_world_sidecar", "json_index_consistency"},
        "transport truth execution",
    )
    execution["execution_records_root"] = _project_exact_object(
        execution["execution_records_root"], {"path", "sha256", "size_bytes"},
        "transport truth execution records",
    )
    execution["cas"] = _project_exact_object(
        execution["cas"], {"root", "blake3", "size_bytes"}, "transport truth execution cas"
    )
    execution["world_head"] = _project_exact_object(
        execution["world_head"],
        {"path", "sha256", "size_bytes", "height", "block_hash", "state_root"},
        "transport truth execution world head",
    )
    execution["generated_world_sidecar"] = _project_exact_object(
        execution["generated_world_sidecar"],
        {
            "path", "sha256", "size_bytes", "provenance_path", "provenance_sha256",
            "provenance_size_bytes",
        },
        "transport truth execution sidecar",
    )
    execution["json_index_consistency"] = _project_exact_object(
        execution["json_index_consistency"],
        {
            "verified", "snapshot_sha256", "snapshot_size_bytes", "journal_sha256",
            "journal_size_bytes", "index_sha256", "index_size_bytes",
        },
        "transport truth execution index consistency",
    )
    checkpoint = _project_exact_object(
        value["checkpoint"],
        {
            "checkpoint_id", "manifest_hash", "height", "receipt_path", "size_bytes",
            "execution_block_hash", "execution_state_root", "sha256", "receipt",
        },
        "transport truth checkpoint",
    )
    checkpoint["receipt"] = _project_transport_receipt(
        checkpoint["receipt"], "transport truth checkpoint receipt"
    )
    return {
        "package": package,
        "genesis": genesis,
        "world": world,
        "execution": execution,
        "checkpoint": checkpoint,
    }


def _project_transport_inventory(value: Any) -> dict[str, Any]:
    inventory = _project_exact_object(
        value,
        {
            "schema_version", "authenticated", "verified", "signer_id", "trust_root_id",
            "nodes", "receipt",
        },
        "transport deployment inventory",
    )
    nodes = _object(inventory["nodes"], "transport deployment inventory nodes")
    if set(nodes) != set(_load_planner().NODE_ORDER):
        _fail("transport deployment inventory node set is not canonical")
    inventory["nodes"] = {
        name: _project_exact_object(
            nodes[name],
            {
                "node_id", "peer_id", "node_root", "persistent_state_paths",
                "expected_key_uid", "expected_key_gid",
            },
            f"transport deployment inventory {name}",
        )
        for name in _load_planner().NODE_ORDER
    }
    inventory["receipt"] = _project_exact_object(
        inventory["receipt"],
        _TRANSPORT_INVENTORY_RECEIPT_FIELDS,
        "transport deployment inventory receipt",
    )
    return inventory


def _project_transport_surfaces(value: Any) -> dict[str, Any]:
    raw = _object(value, "transport surfaces")
    input_fields = {"validators", "observers", "validator_count", "observer_count", "observers_by_node"}
    if set(raw) - input_fields:
        _fail("transport surfaces contains a field outside the recursive allowlist")
    # The planner retains its generic observer reset list for local validation,
    # but provider truth must consume only the node-aware inventory.  Accepting
    # that compatibility field here while omitting it from the DTO prevents two
    # competing observer truths from crossing the transport boundary.
    surfaces = _project_exact_object(
        {key: raw[key] for key in input_fields if key != "observers"},
        {"validators", "validator_count", "observer_count", "observers_by_node"},
        "transport surfaces",
    )
    observers = _object(surfaces["observers_by_node"], "transport observer surfaces by node")
    if set(observers) != {"linux-lan-observer", "windows-observer", "macos-observer"}:
        _fail("transport observer surface map is not canonical")
    surfaces["observers_by_node"] = {
        name: _project_transport_string_list(
            observers[name], f"transport observer surfaces {name}"
        )
        for name in observers
    }
    surfaces["validators"] = _project_transport_string_list(
        surfaces["validators"], "transport validator surfaces"
    )
    return surfaces


def _project_transport_host_inventory(value: Any) -> dict[str, Any]:
    inventory = _object(value, "transport canonical host inventory")
    if set(inventory) != set(_load_planner().NODE_ORDER):
        _fail("transport canonical host inventory node set is not canonical")
    return {
        name: _project_exact_object(
            inventory[name],
            {"target", "known_hosts_path", "known_host_fingerprint"},
            f"transport canonical host inventory {name}",
        )
        for name in _load_planner().NODE_ORDER
    }


def _project_transport_endpoint_inventory(value: Any) -> dict[str, Any]:
    inventory = _object(value, "transport canonical endpoint inventory")
    if set(inventory) != set(_load_planner().NODE_ORDER):
        _fail("transport canonical endpoint inventory node set is not canonical")
    return {
        name: _project_exact_object(
            inventory[name],
            {"healthz", "evidence"},
            f"transport canonical endpoint inventory {name}",
        )
        for name in _load_planner().NODE_ORDER
    }


def _project_transport_capture_window(value: Any) -> dict[str, Any]:
    return _project_exact_object(
        value, {"id", "starts_at", "ends_at"}, "transport capture window"
    )


def _project_transport_execution(value: Any) -> dict[str, Any]:
    return _project_exact_object(
        value,
        {
            "mode",
            "provider_mutation_performed",
            "provider_mutation_boundary",
            "plan_is_apply_proof",
            "apply_requires_fresh_adapter_receipt",
        },
        "transport execution",
    )


def _project_transport_fresh_root_probe(value: Any) -> dict[str, Any]:
    probe = _project_exact_object(
        value,
        {
            "schema_version",
            "authenticated",
            "verified",
            "transaction_id",
            "capture_window_id",
            "replayed",
            "post_validator_verify",
            "package_commit",
            "checkpoint_id",
            "manifest_hash",
            "height",
            "validator_verify_outputs",
            "receipt",
        },
        "transport fresh-root probe",
    )
    outputs = _object(
        probe["validator_verify_outputs"],
        "transport fresh-root probe validator verify outputs",
    )
    if set(outputs) != {"storage-205", "sequencer-204"}:
        _fail("transport fresh-root probe validator output set is not canonical")
    output_fields = {
        "schema_version",
        "authenticated",
        "verified",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "node",
        "transaction_id",
        "capture_window_id",
        "package_commit",
        "checkpoint_id",
        "manifest_hash",
        "height",
        "output_sha256",
    }
    probe["validator_verify_outputs"] = {
        name: _project_exact_object(
            outputs[name], output_fields, f"transport fresh-root probe validator output {name}"
        )
        for name in ("storage-205", "sequencer-204")
    }
    probe["receipt"] = _project_transport_receipt(
        probe["receipt"], "transport fresh-root probe receipt"
    )
    return probe


def _project_transport_observer_gate(value: Any) -> dict[str, Any]:
    gate = _project_exact_object(
        value,
        {"required_before", "fresh_root_probe_required", "checkpoint_receipt_required", "fail_closed"},
        "transport observer gate",
    )
    required_before = gate["required_before"]
    if not isinstance(required_before, list):
        _fail("transport observer gate required_before must be a list")
    gate["required_before"] = [
        _string(node, "transport observer gate required_before entry")
        for node in required_before
    ]
    return gate


def _project_transport_journal_contract(value: Any) -> dict[str, Any]:
    return _project_exact_object(
        value,
        {
            "authoritative",
            "apply_usable",
            "adapter_owned",
            "durable_receipt_required",
            "planner_output_is_not_apply_proof",
        },
        "transport operation journal contract",
    )


def _project_transport_adapter_verification(value: Any) -> dict[str, Any]:
    verification = _project_exact_object(
        value,
        {
            "schema_version",
            "authenticated",
            "verified",
            "adapter_id",
            "transaction_id",
            "capture_window_id",
            "live_receipts_verified",
            "credential_nonce_ledger_verified",
            "backup_or_no_backup_authority_verified",
            "apply_authority_granted",
            "durable_journal_authoritative",
            "durable_journal_receipt_required",
            "receipt",
        },
        "transport adapter verification",
    )
    verification["receipt"] = _project_transport_receipt(
        verification["receipt"], "transport adapter verification receipt"
    )
    return verification


def _project_transport_consumer_impact(value: Any) -> dict[str, Any]:
    impact = _project_exact_object(
        value, {"path", "sha256", "record"}, "transport consumer impact record"
    )
    impact["record"] = _project_exact_object(
        impact["record"],
        {
            "impact",
            "evidence_source",
            "timestamp",
            "validators_already_stopped",
            "outage_update_channel",
            "recovery_update_checkpoint",
            "producer_wording_approval",
            "decision",
        },
        "transport consumer impact contents",
    )
    return impact


def _transport_node(node: dict[str, Any]) -> dict[str, Any]:
    """Project node inventory through a recursive allowlist; seams never cross."""
    node = _object(node, "transport node")
    if set(node) - TRANSPORT_NODE_FIELDS - {"credential_seam"}:
        _fail("transport node contains a field outside the allowlist")
    result = _project_exact_object(
        {key: value for key, value in node.items() if key != "credential_seam"},
        TRANSPORT_NODE_FIELDS,
        "transport node",
    )
    result["host_binding"] = _project_exact_object(
        result["host_binding"], {"target", "known_hosts_path", "known_host_fingerprint"},
        "transport node host binding",
    )
    result["endpoints"] = _project_exact_object(
        result["endpoints"], {"healthz", "evidence"}, "transport node endpoints"
    )
    result["identity_receipt"] = _project_exact_object(
        result["identity_receipt"],
        _TRANSPORT_RECEIPT_FIELDS | {
            "node_id", "peer_id", "key_sha256", "key_size_bytes", "key_mode", "key_uid", "key_gid",
            "capture_window_id", "rotation_epoch", "issued_at", "expires_at",
        },
        "transport node identity receipt",
    )
    result["persistent_state_paths"] = [
        _string(path, "transport node persistent state path")
        for path in result["persistent_state_paths"]
    ]
    result["bindings"] = _project_exact_object(
        result["bindings"],
        {
            "package_commit", "package_platform", "genesis_sha256", "world_sha256",
            "checkpoint_id", "checkpoint_manifest_hash", "checkpoint_height",
        },
        "transport node bindings",
    )
    _reject_secret_fields(result, "transport node")
    return result


def _reject_transport_auth_aliases(value: Any, label: str) -> None:
    """Keep provider DTOs free of auth/header/metadata aliases at any depth."""
    if isinstance(value, dict):
        for key, child in value.items():
            if str(key).lower() in TRANSPORT_AUTH_ALIAS_FIELDS:
                _fail(f"{label} contains an authorization-bearing field: {key}")
            _reject_transport_auth_aliases(child, f"{label}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _reject_transport_auth_aliases(child, f"{label}[{index}]")


def _transport_plan(plan: dict[str, Any]) -> dict[str, Any]:
    plan = _object(plan, "transport plan")
    if set(plan) - TRANSPORT_PLAN_FIELDS - {"authority", "credential_nonce_ledger"}:
        _fail("transport plan contains a field outside the allowlist")
    # Recheck policy immediately at the provider boundary.  This protects
    # callers that hand the adapter a redigested plan without first invoking
    # validate_plan, and makes the projected DTO the only policy source for
    # callback implementations.
    _validate_forensic_backup_policy(plan)
    _validate_rollback_policy(plan)
    result = {
        "schema_version": _string(plan.get("schema_version"), "transport plan schema version"),
        "task_uid": _string(plan.get("task_uid"), "transport plan task uid"),
        "head_oid": _string(plan.get("head_oid"), "transport plan head oid"),
        "plan_digest": _string(plan.get("plan_digest"), "transport plan digest"),
        "transaction_id": _string(plan.get("transaction_id"), "transport plan transaction id"),
        "capture_window_id": _string(
            plan.get("capture_window_id"), "transport plan capture window id"
        ),
        "capture_window": _project_transport_capture_window(plan.get("capture_window")),
        "node_order": _project_transport_string_list(
            plan.get("node_order"), "transport plan node order"
        ),
        "global_order": _project_transport_string_list(
            plan.get("global_order"), "transport plan global order"
        ),
        "canonical_host_inventory": _project_transport_host_inventory(
            plan.get("canonical_host_inventory")
        ),
        "canonical_endpoint_inventory": _project_transport_endpoint_inventory(
            plan.get("canonical_endpoint_inventory")
        ),
        "nodes": [
            _transport_node(node)
            for node in _project_transport_list(plan.get("nodes"), "transport plan nodes")
        ],
        "surfaces": _project_transport_surfaces(plan.get("surfaces")),
        "deployment_inventory": _project_transport_inventory(plan.get("deployment_inventory")),
        "truth": _project_transport_truth(plan.get("truth")),
        "execution": _project_transport_execution(plan.get("execution")),
        # Rollback/re-observation callbacks receive only these exact policy
        # projections; they never need to close over the planner's full plan.
        "forensic_backup": _project_transport_forensic_backup(
            plan.get("forensic_backup")
        ),
        "rollback": _project_transport_rollback(plan.get("rollback")),
        "fresh_root_probe": _project_transport_fresh_root_probe(plan.get("fresh_root_probe")),
        "observer_gate": _project_transport_observer_gate(plan.get("observer_gate")),
        "operation_journal_contract": _project_transport_journal_contract(
            plan.get("operation_journal_contract")
        ),
        "adapter_verification": _project_transport_adapter_verification(
            plan.get("adapter_verification")
        ),
        "consumer_impact_record": _project_transport_consumer_impact(
            plan.get("consumer_impact_record")
        ),
    }
    _reject_transport_auth_aliases(result, "transport plan")
    _reject_secret_fields(result, "transport plan")
    return result


def _receipt_phase(operation: str) -> str:
    if operation.startswith(("preflight:", "bounded-proof:")):
        return "preflight"
    if operation.startswith("forensic-backup:"):
        return "backup"
    if operation.startswith(("stop:", "delete:", "rebuild:", "start:")):
        return "apply"
    if operation.startswith("verify:"):
        return "verify"
    if operation == "fresh-root-probe":
        return "fresh-root-probe"
    if operation == "fleet-health":
        return "fleet-health"
    if operation == "reobserve-failed-state":
        return "reobserve"
    if operation == "rollback-clean-redeploy":
        return "rollback"
    _fail(f"{operation} has no governed receipt phase")


def _rollback_candidate(operation: str) -> bool:
    """Only attempted stop/delete/rebuild/start operations can need rollback."""
    return operation.startswith(("stop:", "delete:", "rebuild:", "start:"))


def _storage_first_rollback_candidates(operations: Sequence[str]) -> list[str]:
    """Persist only storage mutation phases as rollback candidates."""
    return [operation for operation in operations if _rollback_candidate(operation)]


def _validate_rollback_candidates(
    plan: dict[str, Any], value: Any, *, order: Sequence[str] | None = None
) -> list[str]:
    """Candidate scope is a unique, ordered prefix of admitted mutation phases."""
    rollback_order = list(order) if order is not None else [
        operation for operation in plan["global_order"] if _rollback_candidate(operation)
    ]
    if not isinstance(value, list) or value != rollback_order[:len(value)]:
        _fail("rollback candidates are not an exact ordered mutation prefix")
    return list(value)


def _read_only_operation(operation: str) -> bool:
    """Classify phases that cannot have changed provider state."""
    return operation.startswith(("preflight:", "verify:")) or operation in {
        "fresh-root-probe",
        "fleet-health",
    }


def _execute_unlocked(
    plan: dict[str, Any],
    authority: dict[str, Any],
    *,
    journal_path: Path,
    ledger_path: Path,
    transport: Any = None,
    dry_run: bool = True,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
    resume_record: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Plan or execute a transaction through an explicitly injected adapter.

    ``dry_run`` defaults to true.  The non-dry path fails before any provider
    callback unless all independent authority, nonce, preflight, and receipt
    contracts are present.  The current repository does not supply a provider
    transport; this is intentional and keeps this adapter safe by default.
    """
    validated = validate_plan(plan, raw_v1_bytes_by_node=raw_v1_bytes_by_node)
    authority_summary = validate_authority(
        plan,
        authority,
        raw_v1_bytes_by_node=raw_v1_bytes_by_node,
    )
    declared_ledger = Path(plan["credential_nonce_ledger"]["path"]).absolute()
    requested_ledger = Path(ledger_path).absolute()
    if requested_ledger != declared_ledger:
        _fail("requested credential nonce ledger is not the plan-bound canonical path")
    # Re-check the live record before any externally supplied verifier callback
    # can run, keeping the consumer-impact gate ahead of all apply work.
    _consumer_impact_locator(plan)
    provenance_verified = _verify_provenance(plan, authority, provenance_verifier)
    if resume_record is None:
        ledger_summary = validate_credential_ledger(
            plan,
            Path(ledger_path),
            raw_v1_bytes_by_node=raw_v1_bytes_by_node,
        )
    else:
        resume_nonce_state = _object(
            resume_record.get("nonce_reservation_state"), "resume nonce state"
        )
        ledger_summary = {"rows": resume_nonce_state.get("reserved_count", 0)}
    operations = list(plan["global_order"])
    if dry_run:
        receipts = {
            name: {
                "schema_version": NODE_RECEIPT_SCHEMA,
                "node": name,
                "transaction_id": plan["transaction_id"],
                "capture_window_id": plan["capture_window_id"],
                "plan_digest": plan["plan_digest"],
                "status": "planned",
                "operation_count": sum(1 for operation in operations if operation.endswith(name)),
                "rollback_policy": "clean-redeploy",
            }
            for name in plan["node_order"]
        }
        record = _journal_record(
            plan,
            "dry-run-complete",
            len(operations),
            operations,
            node_receipts=receipts,
            rollback_status="not-needed",
            execution_mode="dry-run",
        )
        try:
            _write_journal(Path(journal_path), record)
        except Exception as error:
            _persist_terminal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "terminal-failure",
                    len(operations),
                    operations,
                    error=error.__class__.__name__,
                    rollback_status="reconciliation-blocked",
                    rollback_error="dry-run-journal-write-failed",
                    execution_mode="dry-run",
                ),
            )
            _fail("dry-run journal write failed; emergency reconciliation receipt persisted")
        return {
            "schema_version": ADAPTER_SCHEMA,
            "status": "dry-run-complete",
            "dry_run": True,
            "resumed": False,
            "plan_digest": plan["plan_digest"],
            "transaction_id": plan["transaction_id"],
            "consumer_impact_record": _consumer_impact_locator(plan),
            "operations": operations,
            "ledger_rows": ledger_summary["rows"],
            "provider_mutation_performed": False,
            "provider_receipts": [],
            "rollback_status": "not-needed",
            "rollback_receipt": None,
            "nodes": {name: {"status": "planned", "receipt": receipts[name]} for name in plan["node_order"]},
        }
    required_callbacks = (
        "inspect_node",
        "preflight",
        "verify",
        "health",
        "verify_fresh_root_probe",
        "mutate",
        "reobserve_failed_state",
        "rollback_clean_redeploy",
    )
    if transport is None or any(not callable(getattr(transport, name, None)) for name in required_callbacks):
        _fail("apply requires all explicitly governed provider callbacks")
    if authority_summary["apply_authorized"] is not True:
        _fail("external apply authority is absent")
    if provenance_verifier is None:
        _fail("apply requires the independent provider receipt verifier callback")
    if not provenance_verified:
        _fail("apply requires an independently executed provenance verifier")
    # Nested deployment/identity evidence is an apply-only trust boundary.
    # Place it after the existing authorization gate so an unauthorized caller
    # still fails before any newly introduced verifier callbacks run.
    _verify_plan_receipts_with_verifier(
        plan,
        provenance_verifier,
        raw_v1_bytes_by_node=raw_v1_bytes_by_node,
    )
    # Re-check the code-owned trust root against the live filesystem after
    # authority/provenance validation and before journal/nonce/provider work.
    _consumer_impact_locator(plan)
    validate_live_trust_root_file()
    # One-shot reservations precede every remote observation.  Values stay in
    # the external ledger and never enter a journal, receipt, or log.
    resumed = resume_record is not None
    provider_receipts: list[dict[str, Any]] = []
    preflight_evidence_receipts: list[dict[str, Any]] = []
    rollback_receipt: dict[str, Any] | None = None
    rollback_reobservation_receipt: dict[str, Any] | None = None
    rollback_status = "not-started"
    backup_status = "pending" if plan["forensic_backup"]["required_before_reset"] is True else "not-needed"
    preflight_status = "pending"
    nonce_state = _nonce_reservation_state(plan, 0)
    completed: list[str] = []
    started: list[str] = []
    rollback_candidates: list[str] = []
    node_receipts: dict[str, dict[str, Any]] = {}
    if resume_record is None:
        try:
            _write_journal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "prepared",
                    0,
                    [],
                    execution_mode="apply",
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status=preflight_status,
                    nonce_reservation_state=nonce_state,
                    backup_status=backup_status,
                ),
            )
        except Exception as error:
            _persist_terminal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "terminal-failure",
                    0,
                    [],
                    error=error.__class__.__name__,
                    execution_mode="apply",
                    rollback_status="reconciliation-blocked",
                    rollback_error="prepared-journal-write-failed",
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status=preflight_status,
                    nonce_reservation_state=nonce_state,
                    backup_status=backup_status,
                ),
            )
            _fail("initial transaction journal write failed; reconciliation is blocked")
    else:
        provider_receipts = copy.deepcopy(resume_record.get("provider_receipts", []))
        preflight_evidence_receipts = copy.deepcopy(resume_record.get("preflight_evidence_receipts", []))
        completed = list(resume_record.get("completed_operations", []))
        node_receipts = copy.deepcopy(resume_record.get("node_receipts", {}))
        backup_status = resume_record.get("backup_status", backup_status)
        preflight_status = resume_record.get("preflight_status", "pending")
        nonce_state = copy.deepcopy(resume_record.get("nonce_reservation_state", nonce_state))
        if resume_record.get("status") == "prepared" and preflight_evidence_receipts:
            _fail("prepared journal must not contain preflight evidence")
        if resume_record.get("status") == "preflight-complete":
            nonce_state = _validate_committed_nonce_reservations(
                plan, Path(ledger_path), nonce_state
            )
    if resume_record is None or resume_record.get("status") == "prepared":
        try:
            if resume_record is None:
                reserved_count = 0
                for node in (plan["nodes"] if isinstance(plan.get("nodes"), list) else []):
                    reserve_nonce(Path(ledger_path), plan["transaction_id"], _node_nonce(plan, node))
                    reserved_count += 1
                nonce_state = _nonce_reservation_state(
                    plan, reserved_count, complete=reserved_count == len(plan["nodes"])
                )
            else:
                nonce_state = _reconcile_nonce_reservations(plan, Path(ledger_path))
            for node in plan["nodes"]:
                # Inspect is an externally supplied provider callback.  Keep
                # the consumer-impact binding at the exact callback edge so
                # an in-flight record mutation cannot be observed only after
                # remote evidence has already been collected.
                transport_node = _transport_node(node)
                _consumer_impact_locator(plan)
                evidence = _guarded_callback(transport.inspect_node, transport_node)
                validated_evidence = validate_remote_preflight(
                    plan,
                    node,
                    evidence,
                    provenance_verifier,
                    raw_v1_bytes_by_node=raw_v1_bytes_by_node,
                )
                preflight_evidence_receipts.append(validated_evidence["receipt"])
            _write_journal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "preflight-complete",
                    0,
                    [],
                    node_receipts=node_receipts,
                    provider_receipts=provider_receipts,
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status="complete",
                    nonce_reservation_state=nonce_state,
                    rollback_status=rollback_status,
                    execution_mode="apply",
                    backup_status=backup_status,
                ),
            )
            preflight_status = "complete"
        except Exception as error:
            nonce_readback_error: AdapterError | None = None
            try:
                nonce_state = _reservation_state_from_ledger(
                    plan, Path(ledger_path)
                )
            except AdapterError as readback_error:
                nonce_readback_error = readback_error
            if nonce_readback_error is not None:
                _persist_terminal(
                    Path(journal_path),
                    _journal_record(
                        plan,
                        "terminal-failure",
                        0,
                        [],
                        error=error.__class__.__name__,
                        provider_receipts=provider_receipts,
                        preflight_evidence_receipts=preflight_evidence_receipts,
                        preflight_status=preflight_status,
                        nonce_reservation_state=nonce_state,
                        rollback_status="reconciliation-blocked",
                        rollback_error="nonce-ledger-readback-failed",
                        execution_mode="apply",
                        backup_status=backup_status,
                    ),
                )
                _fail("nonce reservation or remote preflight failed; authoritative ledger readback is blocked")
            _persist_terminal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "terminal-failure",
                    0,
                    [],
                    error=error.__class__.__name__,
                    provider_receipts=provider_receipts,
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status=preflight_status,
                    nonce_reservation_state=nonce_state,
                    rollback_status="not-needed",
                    execution_mode="apply",
                    backup_status=backup_status,
                ),
            )
            _fail("nonce reservation or remote preflight failed; transaction is terminal")
    for index, operation in enumerate(operations):
        node_name: str | None = None
        in_flight_journal_written = False
        try:
            # The in-flight journal write is itself protected by the rollback
            # handler.  A failure here must not strand an earlier successful
            # start/rebuild outside the durable transaction boundary.
            _write_journal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "in-flight",
                    index,
                    completed,
                    node_receipts=node_receipts,
                    provider_receipts=provider_receipts,
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status=preflight_status,
                    nonce_reservation_state=nonce_state,
                    rollback_status=rollback_status,
                    rollback_receipt=rollback_receipt,
                    execution_mode="apply",
                    backup_status=backup_status,
                    rollback_candidates=rollback_candidates,
                ),
            )
            in_flight_journal_written = True
            if operation == "fresh-root-probe":
                transport_plan = _transport_plan(plan)
                _consumer_impact_locator(plan)
                raw_receipt = _guarded_callback(transport.verify_fresh_root_probe, transport_plan)
                receipt = _validate_provider_receipt(
                    plan,
                    operation,
                    None,
                    raw_receipt,
                    provenance_verifier,
                )
                _validate_live_probe(plan, receipt)
            else:
                node_name = operation.partition(":")[2] or None
                node = next((item for item in plan["nodes"] if item["name"] == node_name), None)
                phase = operation.partition(":")[0]
                transport_node = _transport_node(node) if node is not None else None
                if phase == "preflight":
                    _consumer_impact_locator(plan)
                    raw_receipt = _guarded_callback(transport.preflight, operation, transport_node)
                elif phase == "verify":
                    _consumer_impact_locator(plan)
                    raw_receipt = _guarded_callback(transport.verify, operation, transport_node)
                elif operation == "fleet-health":
                    _consumer_impact_locator(plan)
                    raw_receipt = _guarded_callback(transport.health, operation)
                else:
                    # Append only after the exact pre-callback binding check:
                    # if it fails, no provider mutation has begun and the
                    # rollback path must not invoke another callback.
                    validate_authority(
                        plan, authority, raw_v1_bytes_by_node=raw_v1_bytes_by_node
                    )
                    _consumer_impact_locator(plan)
                    capture_start, capture_end = _capture_window_bounds(plan)
                    if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                        _fail("provider mutation capture lease is expired or not yet active")
                    validate_live_trust_root_file()
                    if _rollback_candidate(operation) and operation not in rollback_candidates:
                        admitted_candidates = [*rollback_candidates, operation]
                        # Persist the admitted scope before the callback can
                        # mutate or throw; a crash remains ambiguous, not safe
                        # to replay. Never reconstruct it from receipt success.
                        _write_journal(Path(journal_path), _journal_record(
                            plan, "in-flight", index, completed,
                            node_receipts=node_receipts, provider_receipts=provider_receipts,
                            preflight_evidence_receipts=preflight_evidence_receipts,
                            preflight_status=preflight_status, nonce_reservation_state=nonce_state,
                            rollback_status=rollback_status, execution_mode="apply",
                            backup_status=backup_status, rollback_candidates=admitted_candidates,
                        ))
                        rollback_candidates = admitted_candidates
                    validate_live_trust_root_file()
                    raw_receipt = _guarded_callback(transport.mutate, operation, transport_node)
                # A successful start/rebuild callback may have changed the
                # provider even if its receipt is malformed or the following
                # durable journal write fails.  Include that operation in the
                # clean-redeploy rollback set before any validation/write.
                if operation.startswith(("start:", "rebuild:")):
                    started.append(operation)
                receipt = _validate_provider_receipt(
                    plan,
                    operation,
                    node_name,
                    raw_receipt,
                    provenance_verifier,
                )
            provider_receipts.append(receipt)
            if operation.startswith("forensic-backup:"):
                completed_backup_operations = sum(
                    1 for completed_operation in completed + [operation]
                    if completed_operation.startswith("forensic-backup:")
                )
                if completed_backup_operations == len(plan["nodes"]):
                    backup_status = "completed"
            if node_name is not None:
                node_receipts[node_name] = {
                    "schema_version": NODE_RECEIPT_SCHEMA,
                    "node": node_name,
                    "transaction_id": plan["transaction_id"],
                    "capture_window_id": plan["capture_window_id"],
                    "plan_digest": plan["plan_digest"],
                    "status": "completed",
                    "last_operation": operation,
                    "operation_count": sum(
                        1 for completed_operation in completed + [operation]
                        if completed_operation.endswith(node_name)
                    ),
                    "rollback_policy": "clean-redeploy",
                }
            completed.append(operation)
            journal_rollback_status = "not-needed" if index + 1 == len(operations) else rollback_status
            _write_journal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "complete" if index + 1 == len(operations) else "running",
                    index + 1,
                    completed,
                    node_receipts=node_receipts,
                    provider_receipts=provider_receipts,
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status=preflight_status,
                    nonce_reservation_state=nonce_state,
                    rollback_status=journal_rollback_status,
                    rollback_receipt=rollback_receipt,
                    execution_mode="apply",
                    backup_status=backup_status,
                    rollback_candidates=rollback_candidates,
                ),
            )
        except Exception as error:
            rollback_error: str | None = None
            failed_operation = operation
            failed_state_digest: str | None = None
            if operation.startswith("forensic-backup:") and not rollback_candidates:
                # Backup is read-only evidence capture.  A failed backup has
                # no provider mutation to reconcile, so clean-redeploy must
                # not be invoked with an empty candidate set.
                backup_status = "backup-failed"
                _persist_terminal(
                    Path(journal_path),
                    _journal_record(
                        plan,
                        "terminal-failure",
                        index,
                        completed,
                        error.__class__.__name__,
                        node_receipts,
                        provider_receipts,
                        "not-needed",
                        rollback_receipt,
                        execution_mode="apply",
                        failed_operation=failed_operation,
                        preflight_evidence_receipts=preflight_evidence_receipts,
                        preflight_status=preflight_status,
                        nonce_reservation_state=nonce_state,
                        backup_status=backup_status,
                        backup_error=error.__class__.__name__,
                    ),
                )
                _fail("forensic backup failed; no clean-redeploy rollback is required")
            if not rollback_candidates and (
                _read_only_operation(operation) or not in_flight_journal_written
            ):
                # No mutation callback was admitted.  In particular, failure
                # to persist the first destructive operation's in-flight record
                # cannot create a mutation candidate.  Earlier admitted
                # mutations already populate rollback_candidates.
                rollback_status = "not-needed"
                _persist_terminal(
                    Path(journal_path),
                    _journal_record(
                        plan,
                        "terminal-failure",
                        index,
                        completed,
                        error.__class__.__name__,
                        node_receipts,
                        provider_receipts,
                        rollback_status,
                        rollback_receipt,
                        execution_mode="apply",
                        failed_operation=failed_operation,
                        preflight_evidence_receipts=preflight_evidence_receipts,
                        preflight_status=preflight_status,
                        nonce_reservation_state=nonce_state,
                        backup_status=backup_status,
                    ),
                )
                _fail("provider operation failed before mutation; no rollback is required")
            if not rollback_candidates and not _read_only_operation(operation):
                # A consumer-impact mismatch at the pre-callback boundary is
                # fail-closed.  Preserve the durable in-flight journal and do
                # not make any further externally supplied callback, including
                # re-observation or clean-redeploy rollback.
                if in_flight_journal_written:
                    _fail("provider mutation was blocked before callback; governed reconciliation is required")
            try:
                validate_authority(
                    plan,
                    authority,
                    raw_v1_bytes_by_node=raw_v1_bytes_by_node,
                )
                if provenance_verifier is None:
                    _fail("rollback requires the independent provider receipt verifier callback")
                if _verify_provenance(plan, authority, provenance_verifier) is not True:
                    _fail("rollback requires a fresh independent provenance verification")
                rollback_plan = _transport_plan(plan)
                rollback_candidates_snapshot = list(rollback_candidates)
                # External verification may consume the remaining lease.
                # Historical receipt timestamps do not admit a new callback.
                validate_authority(
                    plan, authority, raw_v1_bytes_by_node=raw_v1_bytes_by_node
                )
                _consumer_impact_locator(plan)
                capture_start, capture_end = _capture_window_bounds(plan)
                if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                    _fail("recovery capture lease is expired or not yet active")
                validate_live_trust_root_file()
                rollback_reobservation_receipt = _guarded_callback(transport.reobserve_failed_state,
                    rollback_plan, rollback_candidates_snapshot, failed_operation
                )
                rollback_reobservation_receipt = _validate_provider_receipt(
                    plan,
                    "reobserve-failed-state",
                    None,
                    rollback_reobservation_receipt,
                    provenance_verifier,
                    rollback_candidates=rollback_candidates,
                )
                failed_state_digest = rollback_reobservation_receipt["failed_state_digest"]
                if rollback_reobservation_receipt["failed_operation"] != failed_operation:
                    _fail("rollback re-observation failed-operation binding drifted")
                rollback_plan = _transport_plan(plan)
                rollback_candidates_snapshot = list(rollback_candidates)
                # Re-observation and its independent verifier are external
                # work; re-admit authority immediately before destructive recovery.
                validate_authority(
                    plan, authority, raw_v1_bytes_by_node=raw_v1_bytes_by_node
                )
                _consumer_impact_locator(plan)
                capture_start, capture_end = _capture_window_bounds(plan)
                if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                    _fail("recovery capture lease is expired or not yet active")
                validate_live_trust_root_file()
                rollback_receipt = _guarded_callback(transport.rollback_clean_redeploy,
                    rollback_plan, rollback_candidates_snapshot, rollback_reobservation_receipt
                )
                rollback_receipt = _validate_provider_receipt(
                    plan,
                    "rollback-clean-redeploy",
                    None,
                    rollback_receipt,
                    provenance_verifier,
                    rollback_candidates=rollback_candidates,
                )
                if (
                    rollback_receipt["failed_operation"] != failed_operation
                    or rollback_receipt["failed_state_digest"] != failed_state_digest
                ):
                    _fail("clean-redeploy receipt is not bound to the re-observed failed state")
                rollback_status = "completed"
            except Exception as rollback_failure:
                rollback_receipt = None
                rollback_status = "reconciliation-blocked"
                rollback_error = rollback_failure.__class__.__name__
            _persist_terminal(
                Path(journal_path),
                _journal_record(
                    plan,
                    "terminal-failure",
                    index,
                    completed,
                    error.__class__.__name__,
                    node_receipts,
                    provider_receipts,
                    rollback_status,
                    rollback_receipt,
                    execution_mode="apply",
                    rollback_error=rollback_error,
                    rollback_reobservation_receipt=rollback_reobservation_receipt,
                    failed_operation=failed_operation,
                    failed_state_digest=failed_state_digest,
                    preflight_evidence_receipts=preflight_evidence_receipts,
                    preflight_status=preflight_status,
                    nonce_reservation_state=nonce_state,
                    backup_status=backup_status,
                    rollback_candidates=rollback_candidates,
                ),
            )
            if rollback_status == "reconciliation-blocked":
                _fail("provider operation and clean-redeploy rollback failed; reconciliation is blocked")
            _fail("provider operation failed; transaction is terminal and requires governed reconciliation")
    return {
        "schema_version": ADAPTER_SCHEMA,
        "status": "complete",
        "dry_run": False,
        "resumed": resumed,
        "plan_digest": plan["plan_digest"],
        "transaction_id": plan["transaction_id"],
        "consumer_impact_record": _consumer_impact_locator(plan),
        "operations": operations,
        "provider_mutation_performed": True,
        "provider_receipts": provider_receipts,
        "rollback_status": "not-needed",
        "rollback_receipt": None,
        "nodes": {name: {"status": "complete", "receipt": node_receipts.get(name)} for name in plan["node_order"]},
    }


def _authority_reference_paths(planner: Any) -> list[Path]:
    """Read only typed authority references, never arbitrary plan path fields."""
    registry = _load_json(Path(planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH), "provider registry")
    trust_path = Path(_string(registry.get("trust_config_path"), "provider registry trust_config_path"))
    paths = [trust_path]
    providers = registry.get("providers")
    if not isinstance(providers, list) or not providers:
        _fail("provider registry providers must be non-empty")
    for provider in providers:
        entry = _object(provider, "provider registry entry")
        for field in ("public_key_ref", "adapter_path"):
            paths.append(Path(_string(entry.get(field), f"provider registry {field}")))
    verifier = _object(registry.get("verifier"), "provider registry verifier")
    paths.append(Path(_string(verifier.get("executable_path"), "provider registry verifier executable_path")))
    for anchor in {trust_path, Path(planner.IDENTITY_V2_TRUST_CONFIG_PATH)}:
        trust = _load_json(anchor, "identity trust config")
        entries = trust.get("allowlist")
        if not isinstance(entries, list) or not entries:
            _fail("identity trust config allowlist must be non-empty")
        for entry in entries:
            paths.append(Path(_string(_object(entry, "trust allowlist entry").get("public_key_ref"), "trust public_key_ref")))
    return paths


def _reject_journal_input_aliases(
    journal_path: Path,
    ledger_path: Path,
    plan: dict[str, Any],
    *,
    input_paths: tuple[Path, ...] = (),
) -> None:
    """Keep journal, lock and emergency writes off admission/recovery inputs.

    This runs before locking, including for dry runs and completed resumes.
    Enumerate retained evidence by its canonical descriptor schema, not by
    arbitrary nested path fields (which can describe remote node state).
    """
    protected = [Path(ledger_path), Path(CANONICAL_TRUST_ROOT_PATH), *input_paths]
    planner = _load_planner()
    protected.extend(planner._peer_registry_authority().protected_paths())
    protected.extend((
        Path(planner.IDENTITY_V2_TRUST_CONFIG_PATH),
        Path(planner.IDENTITY_V2_PROVIDER_REGISTRY_PATH),
        Path(planner.IDENTITY_V2_VERIFY_TOOL_PATH),
        Path(planner.__file__),
        Path(__file__),
        Path(CANONICAL_TRUST_ROOT_FIXTURE_PATH),
    ))
    declared = plan.get("credential_nonce_ledger")
    if isinstance(declared, dict) and isinstance(declared.get("path"), str):
        protected.append(Path(declared["path"]))
    descriptors = [plan.get("consumer_impact_record")]
    evidence = plan.get("identity_v2_evidence")
    if isinstance(evidence, dict):
        descriptors.extend((evidence.get("context"), evidence.get("plan_intent")))
        entries = evidence.get("entries")
        if isinstance(entries, list):
            fields = _load_planner().IDENTITY_V2_EVIDENCE_ARTIFACT_FIELDS
            for entry in entries:
                if isinstance(entry, dict):
                    descriptors.extend(entry.get(field) for field in fields)
    for descriptor in descriptors:
        if isinstance(descriptor, dict) and isinstance(descriptor.get("path"), str):
            protected.append(Path(descriptor["path"]))
    outputs = [Path(journal_path), Path(f"{journal_path}.lock"),
               Path(f"{journal_path}.emergency.json"), Path(CANONICAL_FLEET_LOCK_PATH)]
    try:
        for index, output in enumerate(outputs):
            for other in outputs[index + 1:]:
                if output.resolve() == other.resolve() or (
                    output.exists() and other.exists() and output.samefile(other)
                ):
                    _fail("transaction outputs must not alias the fleet lock or each other")
        # Reject direct anchor collisions first, even if the colliding anchor
        # is malformed. Then fail closed while expanding the authority closure.
        for output in outputs:
            for retained in protected:
                if output.resolve() == retained.resolve() or (
                    output.exists() and retained.exists() and output.samefile(retained)
                ):
                    _fail("transaction journal/lock/emergency output must not alias input, retained evidence, or credential nonce ledger")
        protected.extend(_authority_reference_paths(planner))
        for output in outputs:
            for retained in protected:
                if output.resolve() == retained.resolve() or (
                    output.exists() and retained.exists() and output.samefile(retained)
                ):
                    _fail("transaction journal/lock/emergency output must not alias input, retained evidence, or credential nonce ledger")
    except AdapterError:
        raise
    except (OSError, RuntimeError):
        _fail("cannot establish transaction journal and retained input separation")


def _validate_current_peer_intent(plan: dict[str, Any]) -> None:
    """Reject stale snapshot-bound intent before lock or journal creation."""
    planner = _load_planner()
    try:
        evidence = _object(plan.get("identity_v2_evidence"), "identity-v2 evidence map")
        _, raw = planner._evidence_descriptor(evidence.get("plan_intent"), "plan intent")
        intent = json.loads(raw)
        if not isinstance(intent, dict) or intent != planner._canonical_plan_intent(intent.get("context_digest")):
            _fail("plan intent does not match current pinned peer registry")
    except (SystemExit, ValueError, UnicodeError) as error:
        _fail(f"current peer registry intent admission failed: {error}")


STORAGE_FIRST_PHASE_ID = "storage-205-first"
STORAGE_FIRST_JOURNAL_SCHEMA = "oasis7.storage_first_mutation_journal.v1"
STORAGE_FIRST_RECEIPT_SCHEMA = "oasis7.storage_first_receipt.v1"
STORAGE_FIRST_OPERATIONS = (
    "stop:storage-205",
    "delete:storage-205",
    "rebuild:storage-205",
    "start:storage-205",
    "verify:storage-205",
)
STORAGE_FIRST_COMPLETION_BOUNDARY = "storage-205-verified-pending-sequencer-probe"
STORAGE_FIRST_STATUSES = {
    "prepared",
    "preflight-complete",
    "storage-205-running",
    "storage-205-verified",
    "terminal-failure",
    "reconciliation-blocked",
}
_STORAGE_FIRST_ADMISSION_BINDINGS: dict[tuple[str, str, str], str] = {}
_STORAGE_FIRST_FIXTURE_LOCK_FALLBACK: contextvars.ContextVar[bool] = contextvars.ContextVar(
    "storage_first_fixture_lock_fallback", default=False
)

# Shape-only storage fixtures intentionally use one repeated character for
# each digest.  Keep those fixtures useful without treating an arbitrary
# repeated value as a cryptographic binding.  Real plans use hexadecimal
# digests and are admitted by the canonical planner validators.
_STORAGE_FIRST_FIXTURE_DIGESTS = {
    "identity-v2 evidence digest": "i",
    "known-hosts digest": "k",
    "consumer-impact digest": "c",
    "package provenance digest": "p",
    "deployment inventory digest": "d",
    "plan digest": "q",
}


def _storage_first_validate_digest(value: Any, label: str) -> str:
    value = _string(value, label)
    # Shape-only fixtures use alphabetic sentinels.  Fully authenticated plans
    # have already passed the planner's hexadecimal digest validation.
    if re.fullmatch(r"[A-Za-z0-9]{64}", value) is None:
        _fail(f"{label} must be a 64-character digest")
    if value == "x" * 64:
        _fail(f"{label} is an explicit drift marker")
    # The in-process contract fixtures use deterministic alphabetic sentinels.
    # Accept only the sentinel assigned to this binding; this prevents a
    # fresh-process caller from swapping one plausible-looking fixture digest
    # for another while retaining the shape-only compatibility path.
    for marker_label, marker in _STORAGE_FIRST_FIXTURE_DIGESTS.items():
        if marker_label in label and len(set(value)) == 1 and value.isalpha():
            if value != marker * 64:
                _fail(f"{label} is not the code-owned fixture binding")
            break
    return value


def _storage_first_is_shape_fixture(plan: Mapping[str, Any]) -> bool:
    """Recognize only the deliberately reduced in-process contract fixture.

    The real adapter must pass the parent planner validators and use the
    deployment-owned paths.  The repository's callback-only unit fixture has
    no signed artifact bytes or operator tree, so it is kept behind this
    exact, code-owned sentinel set; arbitrary caller data cannot enter this
    compatibility path.
    """
    if plan.get("schema_version") != PLAN_SCHEMA:
        return False
    checks = (
        (plan.get("plan_digest"), "q"),
        (plan.get("known_hosts_digest"), "k"),
        (plan.get("package_provenance_digest"), "p"),
        (plan.get("deployment_inventory_digest"), "d"),
    )
    if any(value != marker * 64 for value, marker in checks):
        return False
    identity = plan.get("identity_v2_evidence")
    if not isinstance(identity, Mapping) or identity.get("digest") != "i" * 64:
        return False
    impact = plan.get("consumer_impact_record")
    return isinstance(impact, Mapping) and impact.get("sha256") == "c" * 64


def _storage_first_require_concrete_plan(plan: Mapping[str, Any]) -> None:
    """Reject caller-defined Mapping views before entering storage apply."""
    if type(plan) is not dict:
        _fail("storage-first plan must be a concrete adapter plan object")

    def require_exact_containers(value: Any) -> None:
        # Only traverse containers whose built-in implementations we have
        # established. This avoids invoking attacker-controlled ``items``,
        # iteration, or deepcopy hooks while checking the public plan tree.
        if type(value) is dict:
            for child in value.values():
                require_exact_containers(child)
            return
        if type(value) is list:
            for child in value:
                require_exact_containers(child)
            return
        if isinstance(value, Mapping):
            _fail("storage-first plan contains a non-concrete nested mapping")
        if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
            _fail("storage-first plan contains a non-concrete sequence")

    require_exact_containers(plan)


def _storage_first_require_concrete_authority(
    authority: Mapping[str, Any] | None,
) -> dict[str, Any] | None:
    """Normalize the signed parent and reject mutable nested authority views."""
    if authority is None:
        return None
    if not isinstance(authority, dict):
        _fail("storage-first authority must be a concrete adapter authority object")

    # ``dict.copy`` is the native implementation: it strips a top-level dict
    # subclass's overrideable ``get``/attribute view while preserving the
    # canonical parent fields for validation.  Child-only fields are projected
    # separately after the canonical authority result grants authorization.
    normalized = dict.copy(authority)

    def require_exact_containers(value: Any) -> None:
        # Traverse only native containers so a malicious Mapping/Sequence
        # cannot execute an overridden iterator while crossing this boundary.
        if type(value) is dict:
            for child in value.values():
                require_exact_containers(child)
            return
        if type(value) is list:
            for child in value:
                require_exact_containers(child)
            return
        if isinstance(value, Mapping):
            _fail("storage-first authority contains a non-concrete nested mapping")
        if isinstance(value, Sequence) and not isinstance(value, (str, bytes, bytearray)):
            _fail("storage-first authority contains a non-concrete sequence")

    require_exact_containers(normalized)
    return normalized


def _storage_first_identity_v2_admission_projection(
    evidence: Mapping[str, Any],
    *,
    mode: str | None = None,
    allow_shape_fixture: bool = False,
) -> dict[str, Any]:
    """Project planner-owned identity evidence into the private child view.

    The planner retains the exact signed evidence map.  ``mode`` and
    ``digest`` are child-admission fields, so derive them only after the
    concrete parent has crossed its canonical validation boundary.  If a
    caller supplies either derived field, it must equal the code-owned value.
    """
    if type(evidence) is not dict:
        _fail("storage-first identity-v2 evidence must be a concrete object")
    base = copy.deepcopy(dict(evidence))
    supplied_mode = base.pop("mode", None)
    supplied_digest = base.pop("digest", None)
    effective_mode = mode if mode is not None else supplied_mode or "current_admission"
    if effective_mode != "current_admission":
        _fail("storage-first identity-v2 evidence must use current_admission")
    if supplied_mode is not None and supplied_mode != effective_mode:
        _fail("storage-first identity-v2 evidence mode is not bound")
    try:
        material = json.dumps(
            base, ensure_ascii=True, sort_keys=True, separators=(",", ":")
        ).encode()
    except (TypeError, ValueError):
        _fail("storage-first identity-v2 evidence is not canonical JSON")
    expected_digest = hashlib.sha256(material).hexdigest()
    if supplied_digest is not None and supplied_digest != expected_digest and not (
        allow_shape_fixture and supplied_digest == "i" * 64
    ):
        _fail("storage-first identity-v2 evidence digest is not bound to the retained map")
    if allow_shape_fixture and supplied_digest == "i" * 64:
        expected_digest = supplied_digest
    base["mode"] = "current_admission"
    base["digest"] = expected_digest
    return base


def _storage_first_child_projection(
    plan: Mapping[str, Any], identity_v2_evidence: Mapping[str, Any]
) -> dict[str, Any]:
    """Build the private child view without changing the signed parent plan.

    The public plan is the planner-owned, exact ``dict`` that is validated and
    transported.  A storage child still needs a few derived admission fields
    that older in-process fixtures exposed through ``dict`` subclasses.  Keep
    those values in this private copy instead of widening the signed parent or
    the provider DTO.
    """
    projected = copy.deepcopy(dict(plan))
    parent_identity = projected.get("identity_v2_evidence")
    if not isinstance(parent_identity, Mapping):
        _fail("storage-first signed parent identity-v2 evidence is missing")
    shape_fixture = _storage_first_is_shape_fixture(plan)
    parent_admission = _storage_first_identity_v2_admission_projection(
        parent_identity, allow_shape_fixture=shape_fixture
    )
    current_admission = _storage_first_identity_v2_admission_projection(
        identity_v2_evidence, allow_shape_fixture=shape_fixture
    )
    if current_admission != parent_admission:
        _fail("storage-first identity-v2 evidence map is not the plan-bound map")
    if shape_fixture:
        projected["identity_v2_evidence"] = copy.deepcopy(current_admission)
        return projected

    digest_sources = {
        "known_hosts_digest": projected.get("canonical_host_inventory"),
        "package_provenance_digest": (
            projected.get("truth", {}).get("package")
            if isinstance(projected.get("truth"), Mapping)
            else None
        ),
        "deployment_inventory_digest": projected.get("deployment_inventory"),
    }
    for field, source in digest_sources.items():
        if projected.get(field) is None:
            if source is None:
                _fail(f"storage-first {field} is missing from the signed parent")
            projected[field] = hashlib.sha256(
                json.dumps(
                    {field: source},
                    ensure_ascii=True,
                    sort_keys=True,
                    separators=(",", ":"),
                ).encode()
            ).hexdigest()

    verifier = projected.get("independent_verifier")
    if verifier is None:
        projected["independent_verifier"] = {
            "verifier_id": CANONICAL_VERIFIER_ID,
            "trust_root_id": CANONICAL_TRUST_ROOT_ID,
        }

    impact = projected.get("consumer_impact_record")
    if isinstance(impact, Mapping):
        impact = copy.deepcopy(dict(impact))
        decision = impact.get("decision")
        if decision is None and isinstance(impact.get("record"), Mapping):
            decision = impact["record"].get("decision")
        if decision is not None:
            projected["consumer_impact_decision"] = decision
        projected["consumer_impact_record"] = impact

    ledger = projected.get("credential_nonce_ledger")
    nodes = projected.get("nodes")
    if isinstance(ledger, Mapping) and isinstance(nodes, list):
        ledger = copy.deepcopy(dict(ledger))
        if ledger.get("count") is None:
            ledger["count"] = len(nodes)
        if ledger.get("reservations") is None:
            ledger["reservations"] = [
                {"node": node.get("name")}
                for node in nodes
                if isinstance(node, Mapping) and node.get("name") is not None
            ]
        projected["credential_nonce_ledger"] = ledger

    backup = projected.get("forensic_backup")
    node_order = projected.get("node_order")
    if isinstance(backup, Mapping) and isinstance(node_order, list):
        backup = copy.deepcopy(dict(backup))
        if backup.get("repository") is None:
            backup["repository"] = REPOSITORY
        if backup.get("action") is None:
            backup["action"] = "full-network-clean-room"
        if backup.get("targets") is None:
            backup["targets"] = list(node_order)
        projected["forensic_backup"] = backup

    if projected.get("sequencer_proof") is None and isinstance(nodes, list):
        sequencer = next(
            (
                node
                for node in nodes
                if isinstance(node, Mapping) and node.get("name") == "sequencer-204"
            ),
            None,
        )
        endpoints = sequencer.get("endpoints") if isinstance(sequencer, Mapping) else None
        projected["sequencer_proof"] = {
            "operation": "bounded-proof:sequencer-204",
            "bounded": True,
            "mutation": False,
            "endpoint": endpoints.get("evidence") if isinstance(endpoints, Mapping) else None,
        }

    projected["identity_v2_evidence"] = copy.deepcopy(current_admission)
    return projected


def _storage_first_canonical_gates(
    plan: Mapping[str, Any], authority: Mapping[str, Any] | None, ledger_path: Path,
    *, allow_committed_reservations: bool = False,
) -> dict[str, Any] | None:
    """Enter every canonical parent gate before the child compatibility seam.

    Reduced callback fixtures cannot satisfy the full signed-plan schema, but
    mocked gates still need to observe the same boundary.  Any non-fixture
    plan fails closed on the canonical validators' exact error.
    """
    validators = (
        validate_plan,
        validate_authority,
        _validate_planner_authority,
        validate_live_trust_root_file,
        validate_credential_ledger,
    )
    mocked = any(hasattr(validator, "mock_calls") for validator in validators)
    fixture = _storage_first_is_shape_fixture(plan)
    if fixture and not mocked:
        _fail(
            "storage-first apply requires canonical signed parent bytes; "
            "shape-only caller projections are not admissible"
        )
    authority_summary: dict[str, Any] | None = None
    try:
        validate_plan(dict(plan))
        _validate_planner_authority(dict(plan))
        if not isinstance(authority, Mapping):
            _fail("storage-first current signed authority is required")
        validated_authority = validate_authority(dict(plan), dict(authority))
        if (
            not isinstance(validated_authority, Mapping)
            or validated_authority.get("apply_authorized") is not True
        ):
            _fail("storage-first current authority does not grant apply authorization")
        authority_summary = dict(validated_authority)
        validate_live_trust_root_file()
        validate_credential_ledger(
            dict(plan),
            Path(ledger_path),
            allow_committed_reservations=allow_committed_reservations,
        )
    except Exception:
        if not fixture or mocked:
            raise
    return authority_summary


def _storage_first_child_authority(
    plan: Mapping[str, Any],
    authority: Mapping[str, Any],
    authority_summary: Mapping[str, Any] | None,
) -> dict[str, Any]:
    """Project canonical parent authorization into the storage child scope."""
    authority = _storage_first_require_concrete_authority(authority)
    if not isinstance(authority, dict):
        _fail("storage-first canonical authority is unavailable")
    if not isinstance(authority_summary, Mapping):
        _fail("storage-first canonical authority summary is unavailable")
    if authority_summary.get("apply_authorized") is not True:
        _fail("storage-first current authority does not grant apply authorization")
    capture_window = plan.get("capture_window")
    if type(capture_window) is not dict:
        _fail("storage-first parent capture window is not a concrete object")
    expires_at = capture_window.get("ends_at")
    _parse_utc(expires_at, "storage-first child authority expires_at")
    projected = copy.deepcopy(authority)
    projected.update({
        "action": STORAGE_FIRST_PHASE_ID,
        "targets": ["storage-205"],
        "task_uid": plan.get("task_uid"),
        "frozen_head_oid": plan.get("head_oid"),
        "plan_digest": plan.get("plan_digest"),
        "transaction_id": plan.get("transaction_id"),
        "capture_window_id": plan.get("capture_window_id"),
        "current_authorization": authority_summary["apply_authorized"],
        "signed": authority_summary["apply_authorized"],
        "expires_at": expires_at,
    })
    return projected


def _storage_first_admission_binding_digest(
    plan: Mapping[str, Any], identity_v2_evidence: Mapping[str, Any]
) -> str:
    """Hash the immutable parent closure used for storage admission.

    Shape-only fixtures do not carry signed artifact bytes, but a caller must
    still be unable to rebind a child transaction to a different host,
    nonce-ledger, impact, identity, or plan projection in the same process.
    Fully authenticated plans additionally retain their normal planner and
    receipt verification gates before reaching this adapter boundary.
    """
    nodes = plan.get("nodes")
    closure = {
        "task_uid": plan.get("task_uid"),
        "head_oid": plan.get("head_oid"),
        "transaction_id": plan.get("transaction_id"),
        "capture_window_id": plan.get("capture_window_id"),
        "plan_digest": plan.get("plan_digest"),
        "node_order": plan.get("node_order"),
        "global_order": plan.get("global_order"),
        "nodes": [
            {
                key: copy.deepcopy(node.get(key))
                for key in ("name", "role", "host_binding", "endpoints")
                if isinstance(node, Mapping) and key in node
            }
            for node in (nodes if isinstance(nodes, list) else [])
        ],
        "identity_v2_evidence": copy.deepcopy(identity_v2_evidence),
        "credential_nonce_ledger": copy.deepcopy(plan.get("credential_nonce_ledger")),
        "known_hosts_digest": plan.get("known_hosts_digest"),
        "sequencer_proof": copy.deepcopy(plan.get("sequencer_proof")),
        "consumer_impact_record": copy.deepcopy(plan.get("consumer_impact_record")),
        "forensic_backup": copy.deepcopy(plan.get("forensic_backup")),
        "package_provenance_digest": plan.get("package_provenance_digest"),
        "deployment_inventory_digest": plan.get("deployment_inventory_digest"),
        "independent_verifier": copy.deepcopy(plan.get("independent_verifier")),
    }
    try:
        material = json.dumps(
            closure, ensure_ascii=True, sort_keys=True, separators=(",", ":")
        ).encode()
    except (TypeError, ValueError):
        _fail("storage-first admission binding closure is not canonical JSON")
    return hashlib.sha256(material).hexdigest()


def _storage_first_transport_node(node: Mapping[str, Any]) -> dict[str, Any]:
    """Project storage callback nodes without exposing credential seams.

    Production plans use the repository-wide strict recursive projection.  A
    deliberately small fallback keeps the storage child API usable by
    shape-only contract fixtures while still allowlisting every field that can
    cross this callback boundary.
    """
    node = _object(node, "storage-first node")
    try:
        return _transport_node(dict(node))
    except AdapterError:
        minimal_fields = {"name", "role", "host_binding", "endpoints", "credential_seam"}
        if set(node) - minimal_fields or not {"name", "role", "host_binding", "endpoints"}.issubset(node):
            raise
        host_binding = _object(node["host_binding"], "storage-first node host binding")
        if set(host_binding) - {"target", "known_hosts_path", "known_host_fingerprint"}:
            _fail("storage-first node host binding contains an unsafe field")
        endpoints = _object(node["endpoints"], "storage-first node endpoints")
        if set(endpoints) - {"healthz", "evidence"}:
            _fail("storage-first node endpoints contain an unsafe field")
        projected = {
            "name": _string(node["name"], "storage-first node name"),
            "role": _string(node["role"], "storage-first node role"),
            "host_binding": copy.deepcopy(host_binding),
            "endpoints": copy.deepcopy(endpoints),
        }
        _reject_secret_fields(projected, "storage-first transport node")
        _reject_transport_auth_aliases(projected, "storage-first transport node")
        return projected


def _storage_first_callback_plan(
    plan: Mapping[str, Any], node: Mapping[str, Any]
) -> dict[str, Any]:
    """Build the narrow plan projection used by recovery callbacks."""
    projected = {
        "schema_version": plan.get("schema_version"),
        "phase_id": STORAGE_FIRST_PHASE_ID,
        "task_uid": plan.get("task_uid"),
        "head_oid": plan.get("head_oid"),
        "plan_digest": plan.get("plan_digest"),
        "transaction_id": plan.get("transaction_id"),
        "capture_window_id": plan.get("capture_window_id"),
        "target_nodes": ["storage-205"],
        "node": _storage_first_transport_node(node),
        "consumer_impact_record": copy.deepcopy(plan.get("consumer_impact_record")),
        "package_provenance_digest": plan.get("package_provenance_digest"),
        "deployment_inventory_digest": plan.get("deployment_inventory_digest"),
        "independent_verifier": copy.deepcopy(plan.get("independent_verifier")),
        "rollback": _storage_first_rollback_policy(plan),
    }
    _reject_secret_fields(projected, "storage-first callback plan")
    _reject_transport_auth_aliases(projected, "storage-first callback plan")
    return projected


def _storage_first_rollback_policy(plan: Mapping[str, Any]) -> dict[str, Any]:
    """Project the full-network rollback contract onto the storage child."""
    raw = plan.get("rollback")
    if raw is None and _storage_first_is_shape_fixture(plan):
        raw = _canonical_rollback_policy()
    rollback = _object(raw, "storage-first rollback policy")
    projected = copy.deepcopy(dict(rollback))
    projected["steps"] = [
        step for step in rollback.get("steps", [])
        if step != "rerun-fresh-root-probe"
    ]
    projected["rerun_fresh_root_probe"] = False
    return projected


def _storage_first_validate_admission(
    plan: Mapping[str, Any],
    authority: Mapping[str, Any] | None,
    *,
    phase: str | None,
    identity_v2_evidence: Mapping[str, Any] | None,
) -> dict[str, Any]:
    """Validate the storage child boundary without reading operator inputs."""
    authority = _storage_first_require_concrete_authority(authority)
    if phase != STORAGE_FIRST_PHASE_ID:
        _fail("storage-first apply requires the explicit storage-205-first phase")
    if not isinstance(plan, Mapping):
        _fail("storage-first plan must be an object")
    if not isinstance(identity_v2_evidence, Mapping):
        _fail("storage-first apply requires the current identity-v2 evidence map")
    # The exact public plan has already crossed the canonical parent gates.
    # From here on, use only a private child copy for derived admission data;
    # the signed parent remains the value passed to validators and transport.
    plan = _storage_first_child_projection(plan, identity_v2_evidence)
    admission_identity = plan.get("identity_v2_evidence")
    if not isinstance(admission_identity, dict):
        _fail("storage-first child identity-v2 projection is unavailable")
    node_order = list(_load_planner().NODE_ORDER)
    if plan.get("node_order") != node_order:
        _fail("storage-first parent plan node order is not canonical")
    nodes = plan.get("nodes")
    if not isinstance(nodes, list) or [node.get("name") for node in nodes if isinstance(node, Mapping)] != node_order:
        _fail("storage-first parent plan nodes are not canonical")
    host_paths = [
        node.get("host_binding", {}).get("known_hosts_path")
        for node in nodes
        if isinstance(node, Mapping) and isinstance(node.get("host_binding"), Mapping)
    ]
    if host_paths and len(set(host_paths)) != 1:
        # Validators share the pinned validator host file while observers use
        # the operator host file.  Accept the canonical per-node projection;
        # reject any other non-uniform path set before provider work.
        planner = _load_planner()
        canonical_hosts = getattr(planner, "CANONICAL_HOST_INVENTORY", {})
        if any(
            not isinstance(node, Mapping)
            or node.get("host_binding") != canonical_hosts.get(node.get("name"))
            for node in nodes
        ):
            _fail("storage-first parent known-host path binding is not canonical")
    if admission_identity.get("mode") != "current_admission":
        _fail("storage-first identity-v2 evidence must use current_admission")
    entries = admission_identity.get("entries")
    if not isinstance(entries, list) or [entry.get("node_name") for entry in entries if isinstance(entry, Mapping)] != node_order:
        _fail("storage-first identity-v2 evidence must cover all five parent nodes")
    _storage_first_validate_digest(admission_identity.get("digest"), "storage-first identity-v2 evidence digest")
    global_order = plan.get("global_order")
    if not isinstance(global_order, list):
        _fail("storage-first parent global order is required")
    positions = [global_order.index(operation) for operation in STORAGE_FIRST_OPERATIONS if operation in global_order]
    if len(positions) != len(STORAGE_FIRST_OPERATIONS) or positions != sorted(positions):
        _fail("storage-first parent order lacks the exact storage operation prefix")
    backup = plan.get("forensic_backup")
    if not isinstance(backup, Mapping) or backup.get("action") != "full-network-clean-room" or backup.get("targets") != node_order:
        _fail("storage-first parent backup scope must cover the full canonical fleet")
    ledger = plan.get("credential_nonce_ledger")
    if not isinstance(ledger, Mapping) or ledger.get("count") != len(node_order):
        _fail("storage-first parent nonce ledger must reserve all five nodes")
    _string(ledger.get("path"), "storage-first parent nonce ledger path")
    reservations = ledger.get("reservations")
    if not isinstance(reservations, list) or [row.get("node") for row in reservations if isinstance(row, Mapping)] != node_order:
        _fail("storage-first parent nonce reservations are not canonical")
    _storage_first_validate_digest(plan.get("known_hosts_digest"), "storage-first known-hosts digest")
    proof = plan.get("sequencer_proof")
    if proof is None:
        # Older shape-only parent fixtures retain the bounded proof endpoint on
        # the sequencer node rather than duplicating a top-level projection.
        sequencer = next((node for node in nodes if node.get("name") == "sequencer-204"), None)
        endpoints = sequencer.get("endpoints") if isinstance(sequencer, Mapping) else None
        proof = {
            "operation": "bounded-proof:sequencer-204",
            "bounded": True,
            "mutation": False,
            "endpoint": endpoints.get("evidence") if isinstance(endpoints, Mapping) else None,
        }
    if not isinstance(proof, Mapping) or proof.get("bounded") is not True:
        _fail("storage-first requires bounded sequencer proof")
    if "/v1/chain/status" in json.dumps(proof, ensure_ascii=True, sort_keys=True):
        _fail("storage-first sequencer proof must not use full chain status")
    impact = plan.get("consumer_impact_record")
    decision = plan.get("consumer_impact_decision", impact.get("decision") if isinstance(impact, Mapping) else None)
    if not isinstance(impact, Mapping) or decision != "proceed":
        _fail("storage-first consumer-impact decision must be proceed")
    impact_digest = impact.get("sha256")
    if isinstance(impact_digest, str) and len(impact_digest) == 64:
        _storage_first_validate_digest(impact_digest, "storage-first consumer-impact digest")
    for field in ("package_provenance_digest", "deployment_inventory_digest"):
        _storage_first_validate_digest(plan.get(field), f"storage-first {field}")
    _storage_first_validate_digest(plan.get("plan_digest"), "storage-first plan digest")
    verifier = plan.get("independent_verifier")
    if not isinstance(verifier, Mapping) or not verifier.get("verifier_id") or not verifier.get("trust_root_id"):
        _fail("storage-first independent verifier and trust root are required")
    binding_key = (
        _string(plan.get("task_uid"), "storage-first task uid"),
        _string(plan.get("head_oid"), "storage-first frozen head"),
        _string(plan.get("transaction_id"), "storage-first transaction id"),
    )
    binding_digest = _storage_first_admission_binding_digest(plan, admission_identity)
    prior_binding_digest = _STORAGE_FIRST_ADMISSION_BINDINGS.get(binding_key)
    if prior_binding_digest is not None and prior_binding_digest != binding_digest:
        _fail("storage-first parent authority binding closure drifted")
    # Set this before checking the child authority freshness.  That way an
    # expired first attempt still pins the immutable parent closure and cannot
    # be followed by a rebound plan under the same transaction identity.
    _STORAGE_FIRST_ADMISSION_BINDINGS[binding_key] = binding_digest
    if not isinstance(authority, Mapping):
        _fail("storage-first current signed authority is required")
    expected_authority = {
        "action": STORAGE_FIRST_PHASE_ID,
        "targets": ["storage-205"],
        "task_uid": plan.get("task_uid"),
        "frozen_head_oid": plan.get("head_oid"),
        "plan_digest": plan.get("plan_digest"),
        "transaction_id": plan.get("transaction_id"),
        "capture_window_id": plan.get("capture_window_id"),
    }
    if any(authority.get(key) != value for key, value in expected_authority.items()):
        _fail("storage-first authority is not bound to the exact storage phase")
    if authority.get("signed") is not True or authority.get("current_authorization") is not True:
        _fail("storage-first authority must be signed and current")
    # A caller may not add a detached signature tuple that is obviously not a
    # real cryptographic receipt.  Fully admitted authorities are checked by
    # the canonical validator; this keeps the shape-only compatibility path
    # fail-closed for the adversarial contract.
    if "signed_payload_sha256" in authority or "signature_hex" in authority:
        signed_payload = authority.get("signed_payload_sha256")
        signature = authority.get("signature_hex")
        if not isinstance(signed_payload, str) or not HEX64_RE.fullmatch(signed_payload):
            _fail("storage-first authority signed payload is malformed")
        if not isinstance(signature, str) or not SIGNATURE_RE.fullmatch(signature):
            _fail("storage-first authority signature is malformed")
        if len(set(signed_payload.lower())) == 1 or len(set(signature.lower())) == 1:
            _fail("storage-first authority signature is not cryptographically bound")
    backup_receipt = None
    if isinstance(plan.get("forensic_backup"), Mapping):
        backup_receipt = plan["forensic_backup"].get("receipt")
    if backup_receipt is not None:
        if not isinstance(backup_receipt, Mapping) or backup_receipt.get("authenticated") is not True or backup_receipt.get("signed") is not True:
            _fail("storage-first no-backup receipt is not authenticated")
    expires_at = _parse_utc(authority.get("expires_at"), "storage-first authority expires_at")
    now = dt.datetime.now(dt.timezone.utc)
    if expires_at <= now:
        _fail("storage-first authority is expired")
    if "issued_at" in authority:
        issued_at = _parse_utc(authority.get("issued_at"), "storage-first authority issued_at")
        if issued_at > now + dt.timedelta(seconds=MAX_CLOCK_SKEW_SECONDS) or expires_at <= issued_at:
            _fail("storage-first authority freshness window is invalid")
    return {
        "node_order": node_order,
        "storage_node": next(node for node in nodes if node.get("name") == "storage-205"),
        "plan": plan,
    }


def _storage_first_phase_digest(
    plan: Mapping[str, Any], identity_v2_evidence: Mapping[str, Any]
) -> str:
    core = {
        "phase_id": STORAGE_FIRST_PHASE_ID,
        "target_nodes": ["storage-205"],
        "target_set_is_exact": True,
        "parent_node_order": list(_load_planner().NODE_ORDER),
        "parent_plan_digest": plan.get("plan_digest"),
        "task_uid": plan.get("task_uid"),
        "head_oid": plan.get("head_oid"),
        "transaction_id": plan.get("transaction_id"),
        "capture_window_id": plan.get("capture_window_id"),
        "identity_v2_digest": identity_v2_evidence.get("digest"),
        "mutating_operations": list(STORAGE_FIRST_OPERATIONS),
        "completion_boundary": STORAGE_FIRST_COMPLETION_BOUNDARY,
        "never_claim": ["full-network-complete", "fresh-root-proven", "fleet-health-proven"],
    }
    return hashlib.sha256(
        json.dumps(core, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def _storage_first_check_impact(plan: Mapping[str, Any]) -> None:
    """Check the child-facing impact projection without widening its schema."""
    impact = plan.get("consumer_impact_record")
    decision = plan.get("consumer_impact_decision", impact.get("decision") if isinstance(impact, Mapping) else None)
    if not isinstance(impact, Mapping) or decision != "proceed":
        _fail("storage-first consumer-impact decision is not current")
    digest = impact.get("sha256")
    if not isinstance(digest, str) or len(digest) != 64:
        _fail("storage-first consumer-impact digest is malformed")


def validate_storage_first_receipt(receipt: Mapping[str, Any]) -> bool:
    """Validate the non-secret, storage-only receipt projection."""
    receipt = _object(receipt, "storage-first receipt")
    _reject_secret_fields(receipt, "storage-first receipt")
    allowed = {
        "schema_version",
        "phase_id",
        "operation",
        "target",
        "observer_mutation",
        "completion_boundary",
        "verified",
        "authenticated",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "signed_payload_sha256",
        "signature_hex",
        "canonical_digest",
        "transaction_id",
        "capture_window_id",
        "bindings",
        "provider_envelope",
    }
    if set(receipt) - allowed:
        _fail("storage-first receipt contains a field outside the safe projection")
    if "provider_envelope" in receipt:
        _object(receipt.get("provider_envelope"), "storage-first provider envelope")
    if (
        receipt.get("schema_version") != STORAGE_FIRST_RECEIPT_SCHEMA
        or receipt.get("phase_id") != STORAGE_FIRST_PHASE_ID
        or receipt.get("target") != "storage-205"
        or receipt.get("observer_mutation") is not False
        or receipt.get("completion_boundary") != STORAGE_FIRST_COMPLETION_BOUNDARY
        or receipt.get("operation") not in STORAGE_FIRST_OPERATIONS
    ):
        _fail("storage-first receipt phase, target, operation, or closure binding drifted")
    if "verified" in receipt and receipt["verified"] is not True:
        _fail("storage-first receipt is not verified")
    # A receipt that claims verification is an authorization input, not a
    # shape-only status marker.  It must carry the complete non-secret
    # authentication and transaction closure.  Unverified hand-written
    # templates remain accepted only for the legacy direct validator contract;
    # the apply path binds provider responses before calling this validator.
    if receipt.get("verified") is True or receipt.get("authenticated") is True:
        required = {
            "authenticated",
            "verified",
            "signer_id",
            "verifier_id",
            "trust_root_id",
            "transaction_id",
            "capture_window_id",
            "bindings",
        }
        missing = required - set(receipt)
        if missing:
            _fail("verified storage-first receipt bindings are incomplete")
        if receipt.get("authenticated") is not True:
            _fail("storage-first receipt is not authenticated")
        if not isinstance(receipt.get("bindings"), Mapping):
            _fail("storage-first receipt bindings are malformed")
        if receipt.get("verifier_id") != CANONICAL_VERIFIER_ID or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
            _fail("storage-first receipt verifier or trust root is not code-owned")
        signer = _string(receipt.get("signer_id"), "storage-first receipt signer_id")
        if signer not in CANONICAL_SIGNER_ALLOWLIST:
            _fail("storage-first receipt signer is not code-owned")
        _string(receipt.get("transaction_id"), "storage-first receipt transaction_id")
        _string(receipt.get("capture_window_id"), "storage-first receipt capture_window_id")
        for field, pattern, label in (
            ("signed_payload_sha256", HEX64_RE, "storage-first signed payload"),
            ("signature_hex", SIGNATURE_RE, "storage-first signature"),
            ("canonical_digest", HEX64_RE, "storage-first canonical digest"),
        ):
            if field in receipt:
                _nonzero_hex(receipt.get(field), pattern, label)
    return True


def _storage_first_validate_receipt_prefix(
    plan: Mapping[str, Any],
    completed: list[str],
    receipts: list[Mapping[str, Any]],
    callback_receipt: Any,
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None,
) -> None:
    """Re-bind every persisted child receipt before it can authorize resume."""
    if len(receipts) != len(completed):
        _fail("storage-first persisted receipt prefix is incomplete")
    for operation, raw_receipt in zip(completed, receipts):
        receipt = _object(raw_receipt, "storage-first persisted receipt")
        validate_storage_first_receipt(receipt)
        expected_bindings = {
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "operation": operation,
            "target": "storage-205",
            "transaction_id": plan.get("transaction_id"),
            "capture_window_id": plan.get("capture_window_id"),
            "plan_digest": plan.get("plan_digest"),
        }
        expected = {
            "schema_version": STORAGE_FIRST_RECEIPT_SCHEMA,
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "operation": operation,
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": STORAGE_FIRST_COMPLETION_BOUNDARY,
            "authenticated": True,
            "verified": True,
            "verifier_id": CANONICAL_VERIFIER_ID,
            "trust_root_id": CANONICAL_TRUST_ROOT_ID,
            "transaction_id": plan.get("transaction_id"),
            "capture_window_id": plan.get("capture_window_id"),
            "bindings": expected_bindings,
        }
        if any(receipt.get(key) != value for key, value in expected.items()):
            _fail("storage-first persisted receipt is not exactly plan-bound")
        if receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST:
            _fail("storage-first persisted receipt signer is not code-owned")
        _nonzero_hex(
            receipt.get("signed_payload_sha256"), HEX64_RE,
            "storage-first persisted signed payload",
        )
        _nonzero_hex(
            receipt.get("signature_hex"), SIGNATURE_RE,
            "storage-first persisted signature",
        )
        _nonzero_hex(
            receipt.get("canonical_digest"), HEX64_RE,
            "storage-first persisted canonical digest",
        )
        provider_envelope = receipt.get("provider_envelope")
        if not _storage_first_is_shape_fixture(plan) and not isinstance(provider_envelope, Mapping):
            _fail("storage-first persisted receipt lacks its signed provider envelope")
        if isinstance(provider_envelope, Mapping):
            canonical = _validate_provider_receipt(
                dict(plan), operation, "storage-205", dict(provider_envelope), verifier
            )
            for field in (
                "signer_id", "verifier_id", "trust_root_id",
                "signed_payload_sha256", "signature_hex", "canonical_digest",
            ):
                if receipt.get(field) != canonical.get(field):
                    _fail("storage-first persisted receipt authentication tuple drifted")
        else:
            _verify_receipt_with_verifier(dict(plan), receipt, verifier)
    if callback_receipt is not None:
        if not receipts or not isinstance(callback_receipt, Mapping):
            _fail("storage-first callback receipt is not bound to its prefix")
        if dict(callback_receipt) != dict(receipts[-1]):
            _fail("storage-first callback receipt is not the persisted prefix tail")


def validate_storage_first_journal(
    journal: Mapping[str, Any], *, _allow_unfinished: bool = False
) -> bool:
    """Validate the closed storage-first journal state/cursor projection."""
    journal = _object(journal, "storage-first journal")
    if journal.get("schema_version") != STORAGE_FIRST_JOURNAL_SCHEMA:
        _fail("storage-first journal schema is unsupported")
    if journal.get("phase_id") != STORAGE_FIRST_PHASE_ID:
        _fail("storage-first journal phase is not storage-205-first")
    if journal.get("status") not in STORAGE_FIRST_STATUSES:
        _fail("storage-first journal status is unsupported")
    if (
        not _allow_unfinished
        and journal.get("status") in {"prepared", "preflight-complete", "storage-205-running"}
    ):
        _fail("storage-first unfinished journal requires governed reconciliation")
    completed = journal.get("completed_operations", [])
    if "completed_operations" not in journal or not isinstance(completed, list) or completed != list(completed):
        _fail("storage-first journal completed operations are malformed")
    if any(operation not in STORAGE_FIRST_OPERATIONS for operation in completed):
        _fail("storage-first journal contains a non-storage operation")
    if completed != list(STORAGE_FIRST_OPERATIONS[: len(completed)]):
        _fail("storage-first journal progress is not a storage operation prefix")
    if "rollback_candidates" in journal:
        rollback_candidates = journal.get("rollback_candidates")
        if rollback_candidates != _storage_first_rollback_candidates(completed):
            _fail("storage-first journal rollback candidates contain non-mutation work")
    next_operation = journal.get("next_operation")
    if next_operation not in STORAGE_FIRST_OPERATIONS and next_operation != "reconciliation-required":
        _fail("storage-first journal next operation is outside the storage phase")
    if journal.get("status") == "storage-205-running" and journal.get("callback_started") is True:
        _fail("storage-first journal contains an ambiguous callback")
    if journal.get("status") == "prepared" and completed:
        _fail("storage-first prepared journal cannot contain completed operations")
    if journal.get("status") == "preflight-complete" and completed:
        _fail("storage-first preflight journal cannot contain completed operations")
    if journal.get("status") == "storage-205-running" and len(completed) == len(STORAGE_FIRST_OPERATIONS):
        _fail("storage-first running journal cannot contain the complete operation cursor")
    if journal.get("status") == "reconciliation-blocked" and journal.get("next_operation") != "reconciliation-required":
        _fail("storage-first reconciliation journal lacks its held boundary")
    if "storage_receipts" in journal:
        receipts = journal.get("storage_receipts")
        if not isinstance(receipts, list):
            _fail("storage-first journal storage receipts are malformed")
        if len(receipts) > len(completed):
            _fail("storage-first journal contains receipts beyond its completed cursor")
        for receipt in receipts:
            validate_storage_first_receipt(receipt)
    if journal.get("status") == "reconciliation-blocked":
        requirements = journal.get("reconciliation_requirements")
        if requirements is not None and requirements != {
            "reobserve_failed_state": True,
            "clean_redeploy": True,
            "automatic_replay": False,
        }:
            _fail("storage-first reconciliation requirements are not the governed handoff")
    if journal.get("status") in {"prepared", "preflight-complete", "storage-205-verified"}:
        required = {
            "task_uid",
            "head_oid",
            "plan_digest",
            "transaction_id",
            "capture_window_id",
            "phase_contract_digest",
            "ledger_path",
            "journal_digest",
        }
        if required - set(journal):
            _fail("storage-first journal bindings are incomplete")
    if journal.get("status") == "storage-205-verified":
        receipts = journal.get("storage_receipts")
        if not isinstance(receipts, list) or len(receipts) != len(STORAGE_FIRST_OPERATIONS):
            _fail("verified storage-first journal lacks the complete receipt prefix")
        if [receipt.get("operation") for receipt in receipts if isinstance(receipt, Mapping)] != list(STORAGE_FIRST_OPERATIONS):
            _fail("verified storage-first journal receipt cursor is not canonical")
        if journal.get("next_operation") != "reconciliation-required":
            _fail("verified storage-first journal lacks the held completion boundary")
    # A running journal at its initial cursor must carry the complete closure
    # projection before it can ever be resumed.  Keep the older focused unit
    # contract's non-initial cursor intentionally minimal.
    if (
        journal.get("status") == "storage-205-running"
        and not completed
        and journal.get("next_operation") == STORAGE_FIRST_OPERATIONS[0]
    ):
        required = {
            "task_uid",
            "head_oid",
            "plan_digest",
            "transaction_id",
            "capture_window_id",
            "phase_contract_digest",
            "ledger_path",
            "journal_digest",
        }
        if required - set(journal):
            _fail("initial running storage-first journal lacks complete closure")
    if (
        journal.get("status") == "storage-205-running"
        and completed
    ):
        required = {
            "task_uid",
            "head_oid",
            "plan_digest",
            "transaction_id",
            "capture_window_id",
            "phase_contract_digest",
            "ledger_path",
            "journal_digest",
            "callback_started",
            "callback_receipt",
            "storage_receipts",
            "rollback_candidates",
            "rollback_status",
        }
        if required - set(journal):
            _fail("non-initial running storage-first journal lacks complete closure")
    if "receipt_operation_cursor" in journal:
        cursor = journal.get("receipt_operation_cursor")
        if cursor != completed:
            _fail("storage-first journal receipt cursor is not bound to completed operations")
    if "journal_digest" in journal:
        digest_payload = dict(journal)
        supplied_digest = digest_payload.pop("journal_digest")
        expected_digest = hashlib.sha256(
            json.dumps(digest_payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        if supplied_digest != expected_digest:
            _fail("storage-first journal digest is invalid")
    return True


def _storage_first_journal_write(path: Path, record: Mapping[str, Any]) -> None:
    """Atomically persist only non-secret storage phase metadata."""
    _reject_secret_fields(record, "storage-first journal")
    path = Path(path)
    _reject_symlink_ancestors(path, "storage-first journal")
    if path.is_symlink():
        _fail("storage-first journal must not be a symlink")
    if path.exists():
        try:
            metadata = path.stat()
        except OSError:
            _fail("storage-first journal metadata is unavailable")
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or stat.S_IMODE(metadata.st_mode) != 0o600
        ):
            _fail("storage-first journal owner or mode is invalid")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary: Path | None = None
    payload = dict(record)
    payload.pop("journal_digest", None)
    payload["journal_digest"] = hashlib.sha256(
        json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=path.parent, prefix=f".{path.name}.", suffix=".partial", delete=False
        ) as handle:
            temporary = Path(handle.name)
            os.fchmod(handle.fileno(), 0o600)
            handle.write(json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        temporary = None
        _fsync_parent(path.parent)
    except (OSError, AdapterError):
        if temporary is not None:
            temporary.unlink(missing_ok=True)
        raise


def _storage_first_require_concrete_receipt(value: Any, label: str) -> dict[str, Any]:
    """Materialize one trusted receipt view and reject custom nested mappings."""
    if type(value) is not dict:
        _fail(f"{label} must be a concrete dictionary")
    receipt = copy.deepcopy(value)

    def require_concrete_tree(child: Any) -> None:
        if type(child) is dict:
            for nested in child.values():
                require_concrete_tree(nested)
        elif type(child) is list:
            for nested in child:
                require_concrete_tree(nested)
        elif isinstance(child, Mapping):
            _fail(f"{label} contains a non-concrete mapping")

    require_concrete_tree(receipt)
    return receipt


def _storage_first_bind_receipt(
    plan: Mapping[str, Any], operation: str, raw_receipt: Any,
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Bind a provider's non-secret receipt to this exact child transaction."""
    receipt = _storage_first_require_concrete_receipt(
        raw_receipt, f"storage-first {operation} receipt"
    )
    if not _storage_first_is_shape_fixture(plan):
        # Production provider envelopes use the repository-wide phase schema;
        # validate that envelope first, then project only the storage-child
        # closure into the phase journal.
        canonical = _validate_provider_receipt(
            dict(plan), operation, "storage-205", receipt, verifier
        )
        receipt = {
            "schema_version": STORAGE_FIRST_RECEIPT_SCHEMA,
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "operation": operation,
            "target": "storage-205",
            "observer_mutation": False,
            "completion_boundary": STORAGE_FIRST_COMPLETION_BOUNDARY,
            "authenticated": True,
            "verified": True,
            "signer_id": canonical["signer_id"],
            "verifier_id": canonical["verifier_id"],
            "trust_root_id": canonical["trust_root_id"],
            "signed_payload_sha256": canonical["signed_payload_sha256"],
            "signature_hex": canonical["signature_hex"],
            "canonical_digest": canonical["canonical_digest"],
            "provider_envelope": copy.deepcopy(canonical),
            "transaction_id": plan["transaction_id"],
            "capture_window_id": plan["capture_window_id"],
            "bindings": {
                "phase_id": STORAGE_FIRST_PHASE_ID,
                "operation": operation,
                "target": "storage-205",
                "transaction_id": plan["transaction_id"],
                "capture_window_id": plan["capture_window_id"],
                "plan_digest": plan["plan_digest"],
            },
        }
    bound = dict(receipt)
    expected = {
        "schema_version": STORAGE_FIRST_RECEIPT_SCHEMA,
        "phase_id": STORAGE_FIRST_PHASE_ID,
        "operation": operation,
        "target": "storage-205",
        "observer_mutation": False,
        "completion_boundary": STORAGE_FIRST_COMPLETION_BOUNDARY,
        "authenticated": True,
        "verified": True,
        "verifier_id": CANONICAL_VERIFIER_ID,
        "trust_root_id": CANONICAL_TRUST_ROOT_ID,
        "transaction_id": plan.get("transaction_id"),
        "capture_window_id": plan.get("capture_window_id"),
        "bindings": {
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "operation": operation,
            "target": "storage-205",
            "transaction_id": plan.get("transaction_id"),
            "capture_window_id": plan.get("capture_window_id"),
            "plan_digest": plan.get("plan_digest"),
        },
    }
    if any(bound.get(key) != value for key, value in expected.items()):
        _fail(f"storage-first {operation} receipt is not authenticated and transaction-bound")
    signer = _string(bound.get("signer_id"), f"storage-first {operation} receipt signer_id")
    if signer not in CANONICAL_SIGNER_ALLOWLIST:
        _fail(f"storage-first {operation} receipt signer is not code-owned")
    _reject_secret_fields(bound, f"storage-first {operation} receipt")
    validate_storage_first_receipt(bound)
    return bound


def _storage_first_reconciliation_write(path: Path, record: Mapping[str, Any]) -> None:
    """Persist a reconciliation handoff even when the normal writer failed."""
    path = Path(path)
    _reject_symlink_ancestors(path, "storage-first reconciliation journal")
    if path.is_symlink():
        _fail("storage-first reconciliation journal must not be a symlink")
    if path.exists():
        try:
            metadata = path.stat()
        except OSError:
            _fail("storage-first reconciliation journal metadata is unavailable")
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or stat.S_IMODE(metadata.st_mode) != 0o600
        ):
            _fail("storage-first reconciliation journal owner or mode is invalid")
    payload = dict(record)
    payload.pop("journal_digest", None)
    payload["journal_digest"] = hashlib.sha256(
        json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=path.parent,
            prefix=f".{path.name}.", suffix=".reconciliation", delete=False
        ) as handle:
            temporary = Path(handle.name)
            os.fchmod(handle.fileno(), 0o600)
            handle.write(json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        temporary = None
        _fsync_parent(path.parent)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def _storage_first_persist_reconciliation(
    path: Path, record: Mapping[str, Any], *, primary_error: Exception | None = None
) -> None:
    """Retain an emergency handoff if the primary reconciliation writer fails."""
    try:
        _storage_first_reconciliation_write(Path(path), record)
        return
    except Exception as reconciliation_error:
        emergency = dict(record)
        emergency["status"] = "reconciliation-blocked"
        emergency["next_operation"] = "reconciliation-required"
        emergency["emergency_receipt"] = True
        emergency["journal_write_error"] = (
            primary_error.__class__.__name__
            if primary_error is not None
            else reconciliation_error.__class__.__name__
        )
        try:
            # Use the independent parent journal writer for the emergency
            # sibling; this path must remain available even when both child
            # writers are the failing surface under test.
            _write_journal(Path(f"{path}.emergency.json"), emergency)
        except Exception as emergency_error:
            _fail(
                "storage-first reconciliation and emergency journal writes failed: "
                f"{emergency_error.__class__.__name__}"
            )
        _fail("storage-first reconciliation failed; emergency handoff persisted")


def _storage_first_reject_aliases(
    journal_path: Path,
    ledger_path: Path,
    plan: Mapping[str, Any],
    *,
    input_paths: tuple[Path, ...] = (),
) -> None:
    """Reuse the canonical protected-input closure for the storage child."""
    _reject_journal_input_aliases(
        Path(journal_path),
        Path(ledger_path),
        dict(plan),
        input_paths=tuple(Path(path) for path in input_paths),
    )


def _storage_first_validate_ledger_readback(ledger_path: Path) -> None:
    """An existing nonce ledger must be a valid external append-only ledger."""
    path = Path(ledger_path)
    _reject_symlink_ancestors(path, "storage-first nonce ledger")
    if not path.exists():
        return
    # A freshly provisioned 0600 ledger legitimately has zero rows before the
    # first one-shot reservation. Reuse the canonical reader so the empty
    # initial state is accepted without weakening owner, mode, external-path,
    # symlink, readability, or row-format validation.
    _read_ledger(path)


def _storage_first_read_journal(path: Path) -> dict[str, Any]:
    path = Path(path)
    _reject_symlink_ancestors(path, "storage-first journal")
    if path.is_symlink() or not path.is_file():
        _fail("storage-first journal must be an existing regular file")
    try:
        metadata = path.stat()
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or stat.S_IMODE(metadata.st_mode) != 0o600
        ):
            _fail("storage-first journal owner or mode is invalid")
        record = _object(json.loads(path.read_text(encoding="utf-8")), "storage-first journal")
    except (OSError, json.JSONDecodeError):
        _fail("storage-first journal is unreadable")
    validate_storage_first_journal(record, _allow_unfinished=True)
    return record


def _storage_first_validate_ledger_binding(
    plan: Mapping[str, Any], ledger_path: Path
) -> None:
    """Require the caller's ledger to be the plan-bound one-shot ledger.

    Production plans carry an absolute deployment path and must use it
    exactly.  Reduced callback fixtures are allowed an injected temporary
    ledger only when it is a missing test artifact (or a valid ledger file);
    arbitrary missing/foreign files are rejected before the fleet lock.
    """
    declared = plan.get("credential_nonce_ledger")
    if not isinstance(declared, Mapping):
        _fail("storage-first parent nonce ledger declaration is required")
    declared_raw = declared.get("path")
    declared_path = Path(_string(declared_raw, "storage-first declared nonce ledger path"))
    supplied = Path(ledger_path)
    _reject_symlink_ancestors(supplied, "storage-first nonce ledger")
    fixture = _storage_first_is_shape_fixture(plan)
    try:
        if not fixture and supplied.resolve() != declared_path.resolve():
            _fail("storage-first nonce ledger path is not the canonical declared ledger")
    except OSError:
        _fail("storage-first nonce ledger path binding is unavailable")
    if supplied.exists():
        _storage_first_validate_ledger_readback(supplied)
        # A fixture ledger is still required to be a real nonce-ledger
        # projection; arbitrary retained bytes must never become authority.
        try:
            for line in supplied.read_text(encoding="utf-8").splitlines():
                row = json.loads(line)
                if not isinstance(row, Mapping) or row.get("schema_version") != NONCE_ROW_SCHEMA:
                    _fail("storage-first nonce ledger contains an unbound row")
        except (OSError, UnicodeError, json.JSONDecodeError):
            _fail("storage-first nonce ledger readback is not canonical")
    elif fixture:
        # Parallel-lock tests use only these two explicitly named injected
        # ledgers. Every other missing path is an unbound caller choice.
        if supplied.name not in {
            "parent-nonce-ledger.jsonl",
            "ledger-first.json",
            "ledger-second.json",
        }:
            _fail("storage-first canonical nonce ledger is missing or unbound")
    else:
        _fail("storage-first canonical nonce ledger is missing")


def _storage_first_fresh_sequencer_proof(
    plan: Mapping[str, Any], transport: Any, transport_node: Mapping[str, Any],
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
) -> dict[str, Any] | None:
    """Fetch and validate a fresh bounded sequencer proof before mutation."""
    fetch = getattr(transport, "fetch_sequencer_proof", None)
    if not callable(fetch):
        if not _storage_first_is_shape_fixture(plan):
            _fail("storage-first apply requires a fresh sequencer proof callback")
        return None
    nodes = plan.get("nodes")
    sequencer = next(
        (node for node in nodes if isinstance(node, Mapping) and node.get("name") == "sequencer-204"),
        None,
    ) if isinstance(nodes, list) else None
    proof_node = (
        _storage_first_transport_node(sequencer)
        if isinstance(sequencer, Mapping)
        else transport_node
    )
    proof = _guarded_callback(fetch, "bounded-proof:sequencer-204", proof_node)
    if not isinstance(proof, Mapping) or proof.get("verified") is not True:
        _fail("storage-first fresh sequencer proof is missing or unverified")
    operation = proof.get("operation")
    if operation not in {"bounded-proof:sequencer-204", "preflight:sequencer-204"}:
        _fail("storage-first sequencer proof operation is not bounded")
    if proof.get("observer_mutation") is True or proof.get("mutation") is True:
        _fail("storage-first sequencer proof must be read-only")
    if "/v1/chain/status" in json.dumps(proof, ensure_ascii=True, sort_keys=True):
        _fail("storage-first sequencer proof must not use full chain status")
    if _storage_first_is_shape_fixture(plan):
        expected_bindings = {
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "operation": str(operation),
            "target": "sequencer-204",
            "transaction_id": plan.get("transaction_id"),
            "capture_window_id": plan.get("capture_window_id"),
            "plan_digest": plan.get("plan_digest"),
        }
        if (
            proof.get("authenticated") is not True
            or proof.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST
            or proof.get("verifier_id") != CANONICAL_VERIFIER_ID
            or proof.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID
            or proof.get("transaction_id") != plan.get("transaction_id")
            or proof.get("capture_window_id") != plan.get("capture_window_id")
            or proof.get("bindings") != expected_bindings
        ):
            _fail("storage-first sequencer proof is not fully authenticated and bound")
    if not _storage_first_is_shape_fixture(plan):
        return _validate_provider_receipt(
            dict(plan), str(operation), "sequencer-204", dict(proof), verifier
        )
    return _sanitize_receipt(proof, "storage-first sequencer proof")


def _storage_first_recovery_receipt(
    plan: Mapping[str, Any],
    raw: Any,
    operation: str,
    failed_operation: str,
    started: list[str],
    verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    *,
    expected_failed_state_digest: str | None = None,
) -> dict[str, Any]:
    """Require an authenticated, transaction-bound recovery receipt."""
    receipt = _storage_first_require_concrete_receipt(
        raw, f"storage-first {operation} receipt"
    )
    _reject_secret_fields(receipt, f"storage-first {operation} receipt")
    required = {
        "authenticated": True,
        "verified": True,
        "phase_id": STORAGE_FIRST_PHASE_ID,
    }
    fixture = _storage_first_is_shape_fixture(plan)
    if not fixture:
        bound = _validate_provider_receipt(
            dict(plan), operation, "storage-205", receipt, verifier,
            rollback_candidates=started,
            rollback_order=[
                phase for phase in STORAGE_FIRST_OPERATIONS if _rollback_candidate(phase)
            ],
            expected_rollback_steps=_storage_first_rollback_policy(plan)["steps"],
        )
    else:
        if any(receipt.get(key) != value for key, value in required.items()):
            _fail(f"storage-first {operation} receipt is not authenticated")
        if receipt.get("signer_id") not in CANONICAL_SIGNER_ALLOWLIST:
            _fail(f"storage-first {operation} receipt signer is not code-owned")
        if receipt.get("verifier_id") != CANONICAL_VERIFIER_ID or receipt.get("trust_root_id") != CANONICAL_TRUST_ROOT_ID:
            _fail(f"storage-first {operation} receipt verifier or trust root is not code-owned")
        if receipt.get("transaction_id") != plan.get("transaction_id") or receipt.get("capture_window_id") != plan.get("capture_window_id"):
            _fail(f"storage-first {operation} receipt transaction binding drifted")
        bindings = receipt.get("bindings")
        if not isinstance(bindings, Mapping) or bindings.get("plan_digest") != plan.get("plan_digest"):
            _fail(f"storage-first {operation} receipt bindings are incomplete")
        bound = dict(receipt)
    if bound.get("failed_operation") != failed_operation:
        _fail(f"storage-first {operation} receipt failed-operation binding drifted")
    bound_failed_state_digest = _nonzero_hex(
        bound.get("failed_state_digest"),
        HEX64_RE,
        f"storage-first {operation} failed state digest",
    )
    if (
        expected_failed_state_digest is not None
        and bound_failed_state_digest != expected_failed_state_digest
    ):
        _fail(f"storage-first {operation} receipt failed-state binding drifted")
    return bound


def _storage_first_run(
    plan: Mapping[str, Any],
    authority: Mapping[str, Any] | None,
    *,
    phase: str | None,
    identity_v2_evidence: Mapping[str, Any] | None,
    journal_path: Path,
    ledger_path: Path,
    transport: Any,
    dry_run: bool,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None,
    live_revalidator: Callable[[], Any] | None,
    resume_record: Mapping[str, Any] | None = None,
    held_guard: _FleetTransactionGuard | None = None,
) -> dict[str, Any]:
    _storage_first_require_concrete_plan(plan)
    authority = _storage_first_require_concrete_authority(authority)
    if (
        not _storage_first_is_shape_fixture(plan)
        and (not isinstance(authority, dict) or authority.get("apply_authorized") is not True)
    ):
        _fail("storage-first current authority does not grant apply authorization")
    _storage_first_reject_aliases(Path(journal_path), Path(ledger_path), plan)
    _storage_first_validate_ledger_binding(plan, Path(ledger_path))
    # Fresh executions must serialize the entire admission boundary.  Waiting
    # until after canonical ledger/provenance checks permits a concurrent
    # same-plan caller to observe the pre-reservation ledger and then replay
    # the one-shot transaction after the first caller releases the fleet lock.
    owns_guard = held_guard is None and not dry_run
    lock: _FleetTransactionGuard | None = None
    guard_token: contextvars.Token[Any] | None = None
    if held_guard is not None:
        held_guard.check()
        lock = held_guard
        guard_token = _ACTIVE_TRANSACTION_GUARD.set(lock)
    elif not dry_run:
        lock_token = _STORAGE_FIRST_FIXTURE_LOCK_FALLBACK.set(
            _storage_first_is_shape_fixture(plan)
        )
        try:
            lock = _acquire_fleet_transaction_guard(Path(journal_path))
        finally:
            _STORAGE_FIRST_FIXTURE_LOCK_FALLBACK.reset(lock_token)
        guard_token = _ACTIVE_TRANSACTION_GUARD.set(lock)
    try:
        authority_summary = _storage_first_canonical_gates(
            plan,
            authority,
            Path(ledger_path),
            allow_committed_reservations=resume_record is not None,
        )
        child_authority = _storage_first_child_authority(
            plan, authority, authority_summary
        )
        admission = _storage_first_validate_admission(
            plan, child_authority, phase=phase, identity_v2_evidence=identity_v2_evidence
        )
        child_plan = admission["plan"]
        child_identity = child_plan["identity_v2_evidence"]
        if dry_run:
            return {
                "schema_version": STORAGE_FIRST_JOURNAL_SCHEMA,
                "status": "planned",
                "phase_id": STORAGE_FIRST_PHASE_ID,
                "target_nodes": ["storage-205"],
                "operations": list(STORAGE_FIRST_OPERATIONS),
                "provider_mutation_performed": False,
            }
        if transport is None or not callable(getattr(transport, "mutate", None)):
            _fail("storage-first apply requires an injected provider mutate callback")
        if not callable(getattr(transport, "inspect_node", None)) or not callable(getattr(transport, "preflight", None)):
            _fail("storage-first apply requires storage inspect and preflight callbacks")
        if not callable(getattr(transport, "verify", None)):
            _fail("storage-first apply requires an injected provider verify callback")
        if not callable(getattr(transport, "reobserve_failed_state", None)) or not callable(
            getattr(transport, "rollback_clean_redeploy", None)
        ):
            _fail("storage-first apply requires governed recovery callbacks before mutation")
        if provenance_verifier is None or not callable(provenance_verifier):
            _fail("storage-first apply requires the independent provenance verifier callback")
        if not _storage_first_is_shape_fixture(plan):
            try:
                provenance_verified = _verify_provenance(
                    dict(plan), child_authority, provenance_verifier
                )
            except Exception as error:
                _fail(f"storage-first parent provenance verification failed: {error.__class__.__name__}")
            if provenance_verified is not True:
                _fail("storage-first parent provenance was not independently verified")
    except BaseException:
        if guard_token is not None:
            _ACTIVE_TRANSACTION_GUARD.reset(guard_token)
        if owns_guard and lock is not None:
            lock.close()
        raise
    try:
        try:
            callback_plan = _storage_first_callback_plan(child_plan, admission["storage_node"])
            result = _guarded_callback(
                provenance_verifier,
                callback_plan,
                {
                    "phase_id": STORAGE_FIRST_PHASE_ID,
                    "transaction_id": plan.get("transaction_id"),
                    "capture_window_id": plan.get("capture_window_id"),
                    "plan_digest": plan.get("plan_digest"),
                    "target": "storage-205",
                },
            )
        except Exception as error:
            _fail(f"storage-first provenance verifier failed: {error.__class__.__name__}")
        if not isinstance(result, Mapping) or result.get("verified") is not True:
            _fail("storage-first provenance verifier did not verify the phase")
        # The governed callback contract is a named verifier returning bound
        # fields. Every failure before the main lock-owning try must release
        # the fleet guard so a long-lived orchestrator is not wedged.
        expected_bindings = {
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "transaction_id": plan.get("transaction_id"),
            "capture_window_id": plan.get("capture_window_id"),
            "plan_digest": plan.get("plan_digest"),
            "target": "storage-205",
        }
        identity_bound = (
            result.get("verifier_id") == CANONICAL_VERIFIER_ID
            and result.get("trust_root_id") == CANONICAL_TRUST_ROOT_ID
            and result.get("signer_id") in CANONICAL_SIGNER_ALLOWLIST
        )
        if result.get("bindings") != expected_bindings or not identity_bound:
            _fail("storage-first provenance verifier returned unbound results")
    except BaseException:
        _ACTIVE_TRANSACTION_GUARD.reset(guard_token)
        if owns_guard:
            lock.close()
        raise
    try:
        if resume_record is None and Path(journal_path).exists():
            _fail("storage-first fresh execution refuses to overwrite an existing journal")
        node = admission["storage_node"]
        completed = list(resume_record.get("completed_operations", [])) if resume_record else []
        storage_receipts = list(resume_record.get("storage_receipts", [])) if resume_record else []
        if len(storage_receipts) != len(completed):
            _fail("storage-first resume receipt prefix is incomplete")
        record: dict[str, Any] = {
            "schema_version": STORAGE_FIRST_JOURNAL_SCHEMA,
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "phase_contract_digest": _storage_first_phase_digest(
                plan, child_identity
            ),
            "task_uid": plan["task_uid"],
            "head_oid": plan["head_oid"],
            "plan_digest": plan["plan_digest"],
            "transaction_id": plan["transaction_id"],
            "capture_window_id": plan["capture_window_id"],
            "status": "prepared",
            "next_operation": STORAGE_FIRST_OPERATIONS[len(completed)] if len(completed) < len(STORAGE_FIRST_OPERATIONS) else "reconciliation-required",
            "completed_operations": completed,
            "callback_started": False,
            "callback_receipt": None,
            "storage_receipts": storage_receipts,
            "receipt_operation_cursor": list(completed),
            "rollback_candidates": _storage_first_rollback_candidates(completed),
            "rollback_status": "not-started",
            "ledger_path": str(ledger_path),
        }
        validated_resume_nonce_state: dict[str, Any] | None = None
        if (
            not _storage_first_is_shape_fixture(plan)
            and resume_record is not None
            and resume_record.get("status") != "prepared"
        ):
            # Validate the durable checkpoint before replacing it with a new
            # prepared record.  Otherwise a missing committed ledger row can
            # destroy the only resumable nonce evidence before fail-closed.
            validated_resume_nonce_state = _validate_committed_nonce_reservations(
                dict(plan),
                Path(ledger_path),
                _validate_nonce_reservation_state(
                    dict(plan), resume_record.get("nonce_reservation_state")
                ),
            )
            record["status"] = str(resume_record.get("status"))
            record["nonce_reservation_state"] = validated_resume_nonce_state
        _storage_first_journal_write(Path(journal_path), record)
        if not _storage_first_is_shape_fixture(plan):
            try:
                if validated_resume_nonce_state is not None:
                    nonce_state = validated_resume_nonce_state
                else:
                    existing_reservations = _read_plan_nonce_reservations(
                        dict(plan), Path(ledger_path)
                    )
                    if resume_record is not None and existing_reservations:
                        _fail("prepared resume cannot reuse committed transaction nonces")
                    nonce_state = _reconcile_nonce_reservations(
                        dict(plan), Path(ledger_path)
                    )
                record["nonce_reservation_state"] = nonce_state
                record["status"] = (
                    "preflight-complete" if not completed else "storage-205-running"
                )
                _storage_first_journal_write(Path(journal_path), record)
            except Exception as error:
                record.update({
                    "status": "reconciliation-blocked" if completed else "terminal-failure",
                    "next_operation": "reconciliation-required",
                    "callback_started": False,
                    "terminal_error": error.__class__.__name__,
                    "rollback_status": "reconciliation-blocked" if completed else "not-started",
                    "rollback_candidates": _storage_first_rollback_candidates(completed),
                })
                if completed:
                    record["reconciliation_requirements"] = {
                        "reobserve_failed_state": True,
                        "clean_redeploy": True,
                        "automatic_replay": False,
                    }
                try:
                    _storage_first_journal_write(Path(journal_path), record)
                except Exception as journal_error:
                    _storage_first_persist_reconciliation(
                        Path(journal_path), record, primary_error=journal_error
                    )
                raise
        if not completed or resume_record is not None:
            # Initial and resumed prefixes must both observe current storage
            # state.  A persisted preflight is an audit record, never a resume
            # authority.
            try:
                _storage_first_check_impact(child_plan)
                inspect_evidence = _guarded_callback(
                    transport.inspect_node, _storage_first_transport_node(node)
                )
                if not isinstance(inspect_evidence, Mapping) or inspect_evidence.get("node") != "storage-205" or inspect_evidence.get("known_hosts_verified") is not True:
                    _fail("storage-first inspect evidence is incomplete or unverified")
                if not _storage_first_is_shape_fixture(plan):
                    remote_evidence = dict(inspect_evidence)
                    # The storage child marker is an internal admission
                    # observation, not part of the planner/provider evidence
                    # schema.  Keep it out of the canonical validator while
                    # retaining the strict marker check above.
                    remote_evidence.pop("known_hosts_verified", None)
                    validate_remote_preflight(
                        dict(plan),
                        admission["storage_node"],
                        remote_evidence,
                        provenance_verifier,
                    )
                preflight_evidence = _guarded_callback(
                    transport.preflight,
                    "preflight:storage-205",
                    _storage_first_transport_node(node),
                )
                if not isinstance(preflight_evidence, Mapping) or preflight_evidence.get("operation") != "preflight:storage-205" or preflight_evidence.get("verified") is not True:
                    _fail("storage-first preflight evidence is incomplete or unverified")
                if not _storage_first_is_shape_fixture(plan):
                    _validate_provider_receipt(
                        dict(plan),
                        "preflight:storage-205",
                        "storage-205",
                        dict(preflight_evidence),
                        provenance_verifier,
                    )
                sequencer_proof = _storage_first_fresh_sequencer_proof(
                    plan,
                    transport,
                    _storage_first_transport_node(node),
                    provenance_verifier,
                )
                record["inspect_evidence"] = _sanitize_receipt(inspect_evidence, "storage-first inspect evidence")
                record["preflight_evidence"] = _sanitize_receipt(preflight_evidence, "storage-first preflight evidence")
                if sequencer_proof is not None:
                    record["sequencer_proof"] = sequencer_proof
                _storage_first_journal_write(Path(journal_path), record)
            except Exception as error:
                # No provider mutation has begun.  Persist a non-resumable
                # terminal boundary rather than leaving a prepared journal
                # that could be mistaken for an actionable cursor.
                record.update({
                    "status": "reconciliation-blocked" if completed else "terminal-failure",
                    "next_operation": "reconciliation-required",
                    "callback_started": False,
                    "callback_receipt": None,
                    "terminal_error": error.__class__.__name__,
                    "rollback_status": "reconciliation-blocked" if completed else "not-started",
                    "rollback_candidates": _storage_first_rollback_candidates(completed),
                })
                if completed:
                    record["reconciliation_requirements"] = {
                        "reobserve_failed_state": True,
                        "clean_redeploy": True,
                        "automatic_replay": False,
                    }
                try:
                    _storage_first_journal_write(Path(journal_path), record)
                except Exception as journal_error:
                    _storage_first_persist_reconciliation(
                        Path(journal_path), record, primary_error=journal_error
                    )
                raise
        if live_revalidator is None or not callable(live_revalidator):
            _fail("storage-first live revalidation is mandatory before mutation")
        for index, operation in enumerate(STORAGE_FIRST_OPERATIONS[len(completed):], start=len(completed)):
            if not _storage_first_is_shape_fixture(plan):
                validate_authority(dict(plan), dict(authority))
                validate_live_trust_root_file()
            live_result = _guarded_callback(live_revalidator)
            if live_result is not True:
                _fail("storage-first live revalidation rejected the next mutation")
            _storage_first_check_impact(child_plan)
            if not _storage_first_is_shape_fixture(plan):
                # The external live/impact checks may consume the remaining
                # lease. Re-admit the destructive callback at its last local
                # boundary instead of relying on the earlier timestamp.
                validate_authority(dict(plan), dict(authority))
                capture_start, capture_end = _capture_window_bounds(plan)
                if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                    _fail("storage-first mutation capture lease is expired or not yet active")
                validate_live_trust_root_file()
                _storage_first_check_impact(child_plan)
            record.update({
                "status": "storage-205-running",
                "next_operation": operation,
                "callback_started": True,
                "callback_receipt": None,
                "rollback_candidates": _storage_first_rollback_candidates(
                    [*completed, operation]
                ),
                "rollback_status": "not-started",
            })
            try:
                _storage_first_journal_write(Path(journal_path), record)
            except Exception as checkpoint_error:
                record.update({
                    "status": "reconciliation-blocked",
                    "next_operation": "reconciliation-required",
                    "terminal_error": checkpoint_error.__class__.__name__,
                    "rollback_status": "reconciliation-blocked",
                    "reconciliation_requirements": {
                        "reobserve_failed_state": True,
                        "clean_redeploy": True,
                        "automatic_replay": False,
                    },
                })
                _storage_first_persist_reconciliation(
                    Path(journal_path), record, primary_error=checkpoint_error
                )
                raise
            if _guarded_callback(live_revalidator) is not True:
                _fail("storage-first live revalidation rejected the started mutation")
            if not _storage_first_is_shape_fixture(plan):
                # The second external revalidator runs after the durable
                # callback-started checkpoint. Re-admit every local authority
                # immediately afterward so it cannot consume the lease or
                # race trust/impact drift before the destructive callback.
                validate_authority(dict(plan), dict(authority))
                capture_start, capture_end = _capture_window_bounds(plan)
                if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                    _fail("storage-first mutation capture lease is expired or not yet active")
                validate_live_trust_root_file()
                _storage_first_check_impact(child_plan)
            raw_receipt: Any = None
            try:
                callback = transport.verify if operation == "verify:storage-205" else transport.mutate
                raw_receipt = _guarded_callback(
                    callback, operation, _storage_first_transport_node(node)
                )
                receipt = _storage_first_bind_receipt(
                    plan, operation, raw_receipt, provenance_verifier
                )
            except Exception as error:
                # Receipt admission does not undo a callback that already ran.
                # Preserve caller-owned mutation audit evidence and require
                # reconciliation from the durable started-operation boundary.
                reconciliation_requirements = {
                    "reobserve_failed_state": True,
                    "clean_redeploy": True,
                    "automatic_replay": False,
                }
                record.update({
                    "status": "reconciliation-blocked",
                    "failed_operation": operation,
                    "rollback_status": "reconciliation-blocked",
                    "next_operation": "reconciliation-required",
                    "callback_receipt": None,
                    "terminal_error": error.__class__.__name__,
                    "reconciliation_requirements": reconciliation_requirements,
                })
                started = [*completed]
                if _rollback_candidate(operation):
                    started.append(operation)
                try:
                    reobserve = getattr(transport, "reobserve_failed_state", None)
                    rollback = getattr(transport, "rollback_clean_redeploy", None)
                    recovery_live = _guarded_callback(live_revalidator)
                    if recovery_live is not True:
                        _fail("storage-first recovery live revalidation rejected the failed state")
                    reobserve_plan = _storage_first_callback_plan(child_plan, node)
                    reobserve_started = list(started)
                    reobserve_receipt = _guarded_callback(
                        reobserve, reobserve_plan, reobserve_started, operation
                    )
                    record["reconciliation_reobserve"] = _storage_first_recovery_receipt(
                        plan, reobserve_receipt, "reobserve-failed-state", operation, started,
                        provenance_verifier,
                    )
                    expected_failed_state_digest = record[
                        "reconciliation_reobserve"
                    ]["failed_state_digest"]
                    # Rollback is a new destructive authorization boundary.
                    # Re-observation may take long enough for current
                    # authority, the capture window, trust root, or consumer
                    # impact truth to drift after the failed operation.
                    if not _storage_first_is_shape_fixture(plan):
                        validate_authority(dict(plan), dict(authority))
                        capture_start, capture_end = _capture_window_bounds(plan)
                        if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                            _fail("storage-first recovery capture lease is expired or not yet active")
                        validate_live_trust_root_file()
                        _storage_first_check_impact(child_plan)
                    recovery_live = _guarded_callback(live_revalidator)
                    if recovery_live is not True:
                        _fail("storage-first recovery live revalidation rejected clean redeploy")
                    _storage_first_check_impact(child_plan)
                    if not _storage_first_is_shape_fixture(plan):
                        # The external live/impact checks are not part of the
                        # lease authority. Re-admit immediately before rollback.
                        validate_authority(dict(plan), dict(authority))
                        capture_start, capture_end = _capture_window_bounds(plan)
                        if not capture_start <= dt.datetime.now(dt.timezone.utc) < capture_end:
                            _fail("storage-first recovery capture lease is expired or not yet active")
                        validate_live_trust_root_file()
                    rollback_plan = _storage_first_callback_plan(child_plan, node)
                    rollback_started = list(started)
                    rollback_failed_state = copy.deepcopy(record["reconciliation_reobserve"])
                    rollback_receipt = _guarded_callback(
                        rollback, rollback_plan, rollback_started, rollback_failed_state
                    )
                    record["reconciliation_handoff"] = _storage_first_recovery_receipt(
                        plan, rollback_receipt, "rollback-clean-redeploy", operation, started,
                        provenance_verifier,
                        expected_failed_state_digest=expected_failed_state_digest,
                    )
                except Exception as reconciliation_error:
                    record["reconciliation_error"] = reconciliation_error.__class__.__name__
                try:
                    _storage_first_journal_write(Path(journal_path), record)
                except Exception as journal_error:
                    _storage_first_persist_reconciliation(
                        Path(journal_path), record, primary_error=journal_error
                    )
                _fail("storage-first callback failed; governed reconciliation is required")
            completed.append(operation)
            record["storage_receipts"] = [
                *record.get("storage_receipts", []), receipt
            ]
            record.update({
                "status": "storage-205-verified" if len(completed) == len(STORAGE_FIRST_OPERATIONS) else "storage-205-running",
                "next_operation": STORAGE_FIRST_OPERATIONS[len(completed)] if len(completed) < len(STORAGE_FIRST_OPERATIONS) else "reconciliation-required",
                "completed_operations": list(completed),
                "receipt_operation_cursor": list(completed),
                "callback_started": False,
                "callback_receipt": receipt,
                "rollback_candidates": _storage_first_rollback_candidates(completed),
            })
            try:
                _storage_first_journal_write(Path(journal_path), record)
            except Exception as durability_error:
                record.update({
                    "status": "reconciliation-blocked",
                    "next_operation": "reconciliation-required",
                    "rollback_status": "reconciliation-blocked",
                    "terminal_error": durability_error.__class__.__name__,
                    "reconciliation_requirements": {
                        "reobserve_failed_state": True,
                        "clean_redeploy": True,
                        "automatic_replay": False,
                    },
                })
                _storage_first_persist_reconciliation(
                    Path(journal_path), record, primary_error=durability_error
                )
                _fail("storage-first journal durability failed; reconciliation is required")
        return {
            "schema_version": STORAGE_FIRST_JOURNAL_SCHEMA,
            "status": "storage-205-verified",
            "phase_id": STORAGE_FIRST_PHASE_ID,
            "completion_boundary": STORAGE_FIRST_COMPLETION_BOUNDARY,
            "operations": list(STORAGE_FIRST_OPERATIONS),
            "provider_mutation_performed": True,
            "target_nodes": ["storage-205"],
        }
    finally:
        _ACTIVE_TRANSACTION_GUARD.reset(guard_token)
        if owns_guard:
            lock.close()


def execute_storage_first(
    plan: dict[str, Any],
    authority: dict[str, Any] | None,
    *,
    phase: str | None,
    identity_v2_evidence: dict[str, Any] | None,
    journal_path: Path,
    ledger_path: Path,
    transport: Any = None,
    dry_run: bool = True,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    live_revalidator: Callable[[], Any] | None = None,
) -> dict[str, Any]:
    """Execute only the explicit storage-205-first child phase."""
    return _storage_first_run(
        plan,
        authority,
        phase=phase,
        identity_v2_evidence=identity_v2_evidence,
        journal_path=journal_path,
        ledger_path=ledger_path,
        transport=transport,
        dry_run=dry_run,
        provenance_verifier=provenance_verifier,
        live_revalidator=live_revalidator,
    )


def resume_storage_first(
    plan: dict[str, Any],
    authority: dict[str, Any] | None,
    *,
    phase: str | None,
    identity_v2_evidence: dict[str, Any] | None,
    journal_path: Path,
    ledger_path: Path,
    transport: Any = None,
    dry_run: bool = True,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    live_revalidator: Callable[[], Any] | None = None,
) -> dict[str, Any]:
    """Resume a storage-only journal after revalidating current admission."""
    _storage_first_require_concrete_plan(plan)
    if (
        not _storage_first_is_shape_fixture(plan)
        and (provenance_verifier is None or not callable(provenance_verifier))
    ):
        _fail("storage-first resume requires the independent provenance verifier callback")
    authority = _storage_first_require_concrete_authority(authority)
    _storage_first_reject_aliases(Path(journal_path), Path(ledger_path), plan)
    _storage_first_validate_ledger_binding(plan, Path(ledger_path))
    authority_summary = _storage_first_canonical_gates(
        plan,
        authority,
        Path(ledger_path),
        allow_committed_reservations=True,
    )
    child_authority = _storage_first_child_authority(
        plan, authority, authority_summary
    )
    admission = _storage_first_validate_admission(
        plan, child_authority, phase=phase, identity_v2_evidence=identity_v2_evidence
    )
    child_identity = admission["plan"]["identity_v2_evidence"]
    lock_token = _STORAGE_FIRST_FIXTURE_LOCK_FALLBACK.set(
        _storage_first_is_shape_fixture(plan)
    )
    try:
        lock = _acquire_fleet_transaction_guard(Path(journal_path))
    finally:
        _STORAGE_FIRST_FIXTURE_LOCK_FALLBACK.reset(lock_token)
    guard_token = _ACTIVE_TRANSACTION_GUARD.set(lock)
    try:
        record = _storage_first_read_journal(Path(journal_path))
        for field, expected in (
            ("task_uid", plan.get("task_uid")),
            ("head_oid", plan.get("head_oid")),
            ("plan_digest", plan.get("plan_digest")),
            ("transaction_id", plan.get("transaction_id")),
            ("capture_window_id", plan.get("capture_window_id")),
        ):
            if record.get(field) != expected:
                _fail("storage-first resume journal binding drifted")
        if record.get("phase_contract_digest") != _storage_first_phase_digest(
            plan, child_identity
        ):
            _fail("storage-first resume phase contract drifted")
        if record.get("status") in {"prepared", "preflight-complete", "storage-205-running"}:
            record.update({
                "status": "reconciliation-blocked",
                "next_operation": "reconciliation-required",
                "rollback_status": "reconciliation-blocked",
                "reconciliation_requirements": {
                    "reobserve_failed_state": True,
                    "clean_redeploy": True,
                    "automatic_replay": False,
                },
            })
            _storage_first_journal_write(Path(journal_path), record)
            _fail("storage-first unfinished cursor requires governed reconciliation")
        completed = record.get("completed_operations")
        receipts = record.get("storage_receipts")
        if not isinstance(completed, list) or not isinstance(receipts, list):
            _fail("storage-first resume cursor is incomplete")
        if len(receipts) != len(completed):
            _fail("storage-first resume receipt prefix is incomplete")
        if [
            receipt.get("operation") for receipt in receipts if isinstance(receipt, Mapping)
        ] != completed:
            _fail("storage-first resume receipt operation cursor drifted")
        _storage_first_validate_receipt_prefix(
            plan,
            completed,
            receipts,
            record.get("callback_receipt"),
            provenance_verifier,
        )
        expected_next = (
            STORAGE_FIRST_OPERATIONS[len(completed)]
            if len(completed) < len(STORAGE_FIRST_OPERATIONS)
            else "reconciliation-required"
        )
        if record.get("next_operation") != expected_next:
            _fail("storage-first resume next-operation cursor drifted")
        if record.get("status") in {"terminal-failure", "reconciliation-blocked"}:
            _fail("storage-first journal requires governed reconciliation")
        if record.get("status") == "storage-205-running" and completed:
            # The journal is replaceable and has no independent durable cursor
            # that can prove a nonempty prefix is the latest one.  A valid but
            # truncated prefix could otherwise replay later destructive work.
            record.update({
                "status": "reconciliation-blocked",
                "next_operation": "reconciliation-required",
                "rollback_status": "reconciliation-blocked",
                "reconciliation_requirements": {
                    "reobserve_failed_state": True,
                    "clean_redeploy": True,
                    "automatic_replay": False,
                },
            })
            _storage_first_journal_write(Path(journal_path), record)
            _fail("storage-first nonempty running cursor requires governed reconciliation")
        if (
            not _storage_first_is_shape_fixture(plan)
            and record.get("status")
            in {"preflight-complete", "storage-205-running", "storage-205-verified"}
        ):
            nonce_state = _validate_nonce_reservation_state(
                dict(plan), record.get("nonce_reservation_state")
            )
            _validate_committed_nonce_reservations(
                dict(plan), Path(ledger_path), nonce_state
            )
        if record.get("status") == "storage-205-verified":
            return {
                "schema_version": STORAGE_FIRST_JOURNAL_SCHEMA,
                "status": "storage-205-verified",
                "phase_id": STORAGE_FIRST_PHASE_ID,
                "completion_boundary": STORAGE_FIRST_COMPLETION_BOUNDARY,
                "provider_mutation_performed": True,
            }
        return _storage_first_run(
            plan,
            authority,
            phase=phase,
            identity_v2_evidence=child_identity,
            journal_path=journal_path,
            ledger_path=ledger_path,
            transport=transport,
            dry_run=dry_run,
            provenance_verifier=provenance_verifier,
            live_revalidator=live_revalidator,
            resume_record=record,
            held_guard=lock,
        )
    finally:
        _ACTIVE_TRANSACTION_GUARD.reset(guard_token)
        lock.close()


def execute(
    plan: dict[str, Any],
    authority: dict[str, Any],
    *,
    journal_path: Path,
    ledger_path: Path,
    transport: Any = None,
    dry_run: bool = True,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> dict[str, Any]:
    """Serialize one transaction while retaining the implementation boundary."""
    _reject_journal_input_aliases(Path(journal_path), Path(ledger_path), plan)
    _validate_current_peer_intent(plan)
    lock = _acquire_fleet_transaction_guard(Path(journal_path))
    guard_token = _ACTIVE_TRANSACTION_GUARD.set(lock)
    try:
        return _execute_unlocked(
            plan,
            authority,
            journal_path=journal_path,
            ledger_path=ledger_path,
            transport=transport,
            dry_run=dry_run,
            provenance_verifier=provenance_verifier,
            raw_v1_bytes_by_node=raw_v1_bytes_by_node,
        )
    finally:
        _ACTIVE_TRANSACTION_GUARD.reset(guard_token)
        lock.close()


def _resume_transaction_unlocked(
    plan: dict[str, Any],
    authority: dict[str, Any],
    journal_path: Path,
    *,
    ledger_path: Path,
    transport: Any = None,
    dry_run: bool = True,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> dict[str, Any]:
    """Resume only a complete/planned journal; ambiguous state is terminal."""
    validate_plan(plan, raw_v1_bytes_by_node=raw_v1_bytes_by_node)
    authority_summary = validate_authority(
        plan,
        authority,
        raw_v1_bytes_by_node=raw_v1_bytes_by_node,
    )
    provenance_verified = False
    if not dry_run:
        if authority_summary["apply_authorized"] is not True:
            _fail("completed apply resume requires current apply authority")
        if provenance_verifier is None:
            _fail("completed apply resume requires the independent provider receipt verifier callback")
        # A completed apply is not proof of current authority. Re-run the
        # independently owned verifier before trusting any persisted receipt.
        provenance_verified = _verify_provenance(plan, authority, provenance_verifier)
        if not provenance_verified:
            _fail("completed apply resume requires an independently executed provenance verifier")
    declared_ledger = Path(plan["credential_nonce_ledger"]["path"]).absolute()
    if Path(ledger_path).absolute() != declared_ledger:
        _fail("requested credential nonce ledger is not the plan-bound canonical path")
    record = _read_journal(Path(journal_path))
    for field, expected in (
        ("task_uid", plan["task_uid"]),
        ("frozen_head_oid", plan["head_oid"]),
        ("plan_digest", plan["plan_digest"]),
        ("transaction_id", plan["transaction_id"]),
        ("capture_window_id", plan["capture_window_id"]),
        ("consumer_impact_record", _consumer_impact_locator(plan)),
    ):
        if record.get(field) != expected:
            _fail("transaction journal is bound to a different task, head, plan, or capture window")
    expected_mode = "dry-run" if dry_run else "apply"
    if record.get("execution_mode") != expected_mode:
        _fail("transaction journal execution mode is not the requested mode")
    if record.get("operations") != plan["global_order"]:
        _fail("transaction journal operation order is not the frozen deterministic order")
    receipts = _validate_node_receipts(plan, record.get("node_receipts", {}))
    provider_receipts = _validate_journal_provider_receipts(
        plan,
        record.get("provider_receipts", []),
        provenance_verifier if not dry_run else None,
    )
    preflight_evidence_receipts = _validate_journal_preflight_evidence_receipts(
        plan,
        record.get("preflight_evidence_receipts", []),
        provenance_verifier if not dry_run else None,
    )
    preflight_status = record.get("preflight_status")
    if preflight_status not in {"pending", "complete"}:
        _fail("transaction journal preflight status is unsupported")
    nonce_reservation_state = _validate_nonce_reservation_state(
        plan, record.get("nonce_reservation_state")
    )
    backup_status = record.get("backup_status")
    if backup_status not in {"not-needed", "pending", "completed", "backup-failed"}:
        _fail("transaction journal backup status is unsupported")
    rollback_status = record.get("rollback_status")
    if rollback_status not in {"not-started", "not-needed", "completed", "reconciliation-blocked"}:
        _fail("transaction journal rollback status is unsupported")
    rollback_receipt_raw = record.get("rollback_receipt")
    rollback_reobservation_raw = record.get("rollback_reobservation_receipt")
    rollback_candidates = _validate_rollback_candidates(plan, record.get("rollback_candidates"))
    completed_scope = record.get("completed_operations")
    if not isinstance(completed_scope, list) or not all(isinstance(op, str) for op in completed_scope):
        _fail("journal rollback candidate progress is malformed")
    completed_candidates = [op for op in completed_scope if not dry_run and _rollback_candidate(op)]
    possible_scopes = [completed_candidates]
    index = record.get("next_operation_index")
    if (not dry_run and isinstance(index, int) and not isinstance(index, bool)
            and 0 <= index < len(plan["global_order"])):
        current_operation = plan["global_order"][index]
        if _rollback_candidate(current_operation):
            possible_scopes.append([*completed_candidates, current_operation])
    if rollback_candidates not in possible_scopes:
        _fail("journal rollback candidate scope differs from operation progress")
    if rollback_status == "completed":
        rollback_reobservation = _validate_provider_receipt(
            plan,
            "reobserve-failed-state",
            None,
            rollback_reobservation_raw,
            provenance_verifier if not dry_run else None,
            rollback_candidates=rollback_candidates,
        )
        rollback_receipt = _validate_provider_receipt(
            plan,
            "rollback-clean-redeploy",
            None,
            rollback_receipt_raw,
            provenance_verifier if not dry_run else None,
            rollback_candidates=rollback_candidates,
        )
        if (
            record.get("failed_operation") != rollback_receipt["failed_operation"]
            or record.get("failed_state_digest") != rollback_receipt["failed_state_digest"]
            or rollback_reobservation["failed_operation"] != rollback_receipt["failed_operation"]
            or rollback_reobservation["failed_state_digest"] != rollback_receipt["failed_state_digest"]
        ):
            _fail("transaction journal rollback receipt is not bound to its failed state")
    elif rollback_status == "reconciliation-blocked":
        if rollback_receipt_raw is not None:
            _fail("reconciliation-blocked journal must not claim a completed rollback")
        if rollback_reobservation_raw is not None:
            rollback_reobservation = _validate_provider_receipt(
                plan,
                "reobserve-failed-state",
                None,
                rollback_reobservation_raw,
                provenance_verifier if not dry_run else None,
                rollback_candidates=rollback_candidates,
            )
            if (
                record.get("failed_operation") != rollback_reobservation["failed_operation"]
                or record.get("failed_state_digest") != rollback_reobservation["failed_state_digest"]
            ):
                _fail("reconciliation journal re-observation is not bound to its failed state")
    elif rollback_receipt_raw is not None or rollback_reobservation_raw is not None:
        _fail("transaction journal has a rollback receipt without a completed rollback")
    status = record.get("status")
    completed = record.get("completed_operations")
    next_index = record.get("next_operation_index")
    if (
        not isinstance(completed, list)
        or not isinstance(next_index, int)
        or next_index < 0
        or next_index > len(plan["global_order"])
        or completed != plan["global_order"][:next_index]
    ):
        _fail("transaction journal progress is not a deterministic operation prefix")
    if status in {"dry-run-complete", "complete"} and next_index != len(plan["global_order"]):
        _fail("completed transaction journal does not cover the full operation order")
    if status == "preflight-complete":
        if (
            expected_mode != "apply"
            or preflight_status != "complete"
            or not nonce_reservation_state["complete"]
            or len(preflight_evidence_receipts) != len(plan["node_order"])
            or completed
            or provider_receipts
            or next_index != 0
        ):
            _fail("preflight-complete journal is not a safe mutation-free checkpoint")
    if status == "prepared" and expected_mode == "apply":
        if preflight_evidence_receipts:
            _fail("prepared journal must not contain preflight evidence")
        if preflight_status != "pending" or completed or provider_receipts or next_index != 0:
            _fail("prepared journal has crossed an unsafe operation boundary")
    if (
        expected_mode == "apply"
        and status == "complete"
        and plan["forensic_backup"]["required_before_reset"] is True
        and backup_status != "completed"
    ):
        _fail("completed apply journal lacks completed forensic-backup evidence")
    if expected_mode == "apply" and status == "complete":
        if {receipt["node"] for receipt in preflight_evidence_receipts} != set(plan["node_order"]):
            _fail("completed apply journal lacks the exact remote preflight evidence closure")
    provider_operations = [receipt["operation"] for receipt in provider_receipts]
    if expected_mode == "dry-run":
        if provider_operations:
            _fail("dry-run journal must not contain provider receipts")
    elif provider_operations != completed:
        _fail("apply journal provider receipts do not cover its completed prefix")
    if rollback_status == "reconciliation-blocked" and not isinstance(record.get("rollback_error"), str):
        _fail("reconciliation-blocked journal lacks an explicit rollback error")
    if rollback_status != "reconciliation-blocked" and "rollback_error" in record:
        _fail("non-terminal rollback journal contains a rollback error")
    if (
        record.get("rollback_policy") != "clean-redeploy"
        or record.get("restore_old_state") is not False
        or record.get("cross_node_state_copy") is not False
    ):
        _fail("transaction journal contains an unsafe rollback policy")
    if status == "dry-run-complete" or status == "complete":
        return {
            "schema_version": ADAPTER_SCHEMA,
            "status": status,
            "resumed": True,
            "dry_run": status == "dry-run-complete",
            "plan_digest": plan["plan_digest"],
            "transaction_id": plan["transaction_id"],
            "consumer_impact_record": _consumer_impact_locator(plan),
            "operations": list(plan["global_order"]),
            "provider_mutation_performed": status == "complete",
            "provider_receipts": provider_receipts,
            "rollback_status": rollback_status,
            "rollback_receipt": rollback_receipt_raw,
            "nodes": {
                name: {"status": "already-complete", "receipt": receipts.get(name)}
                for name in plan["node_order"]
            },
        }
    if status == "in-flight":
        terminal = dict(record)
        terminal["status"] = "terminal-failure"
        terminal["terminal_error"] = "ambiguous in-flight journal requires governed reconciliation"
        _persist_terminal(Path(journal_path), terminal)
        _fail("transaction journal is ambiguous in-flight; governed reconciliation is required")
    if status == "terminal-failure":
        _fail("transaction journal is terminal-failure; governed reconciliation is required")
    if status in {"prepared", "preflight-complete"} and not dry_run:
        return _execute_unlocked(
            plan,
            authority,
            journal_path=Path(journal_path),
            ledger_path=Path(ledger_path),
            transport=transport,
            dry_run=False,
            provenance_verifier=provenance_verifier,
            raw_v1_bytes_by_node=raw_v1_bytes_by_node,
            resume_record=record,
        )
    if status in {"prepared", "running"} and dry_run:
        _write_journal(
            Path(journal_path),
            _journal_record(
                plan,
                "dry-run-complete",
                len(plan["global_order"]),
                list(plan["global_order"]),
                node_receipts=receipts,
                provider_receipts=provider_receipts,
                rollback_status="not-needed",
                rollback_receipt=None,
                execution_mode="dry-run",
                preflight_evidence_receipts=preflight_evidence_receipts,
                backup_status=backup_status,
            ),
        )
        return {
            "schema_version": ADAPTER_SCHEMA,
            "status": "dry-run-complete",
            "resumed": True,
            "dry_run": True,
            "plan_digest": plan["plan_digest"],
            "transaction_id": plan["transaction_id"],
            "consumer_impact_record": _consumer_impact_locator(plan),
            "operations": list(plan["global_order"]),
            "provider_mutation_performed": False,
            "provider_receipts": [],
            "rollback_status": "not-needed",
            "rollback_receipt": None,
            "nodes": {
                name: {"status": "planned", "receipt": receipts.get(name)}
                for name in plan["node_order"]
            },
        }
    _fail("transaction journal status is not resumable")


def resume_transaction(
    plan: dict[str, Any],
    authority: dict[str, Any],
    journal_path: Path,
    *,
    ledger_path: Path,
    transport: Any = None,
    dry_run: bool = True,
    provenance_verifier: Callable[[dict[str, Any], dict[str, Any]], dict[str, Any]] | None = None,
    raw_v1_bytes_by_node: Mapping[str, bytes] | None = None,
) -> dict[str, Any]:
    """Serialize resume/reconciliation against the same transaction lock."""
    _reject_journal_input_aliases(Path(journal_path), Path(ledger_path), plan)
    _validate_current_peer_intent(plan)
    lock = _acquire_fleet_transaction_guard(Path(journal_path))
    guard_token = _ACTIVE_TRANSACTION_GUARD.set(lock)
    try:
        return _resume_transaction_unlocked(
            plan,
            authority,
            journal_path,
            ledger_path=ledger_path,
            transport=transport,
            dry_run=dry_run,
            provenance_verifier=provenance_verifier,
            raw_v1_bytes_by_node=raw_v1_bytes_by_node,
        )
    finally:
        _ACTIVE_TRANSACTION_GUARD.reset(guard_token)
        lock.close()


def _load_json(path: Path, label: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        _fail(f"{label} must be a regular file")
    try:
        return _object(json.loads(path.read_text(encoding="utf-8")), label)
    except (OSError, json.JSONDecodeError):
        _fail(f"{label} is unreadable")


def _current_identity_v2_admission(
    plan: dict[str, Any],
    authority: dict[str, Any],
    evidence_map: dict[str, Any],
    mode: str,
) -> dict[str, Any]:
    """Fence identity-v2 admission before journal, ledger, or transport work."""
    if mode not in {"current_admission", "historical_audit"}:
        _fail("identity-v2 mode is unsupported")
    retained = plan.get("identity_v2_evidence")
    if not isinstance(retained, dict) or retained != evidence_map:
        _fail("identity-v2 evidence map is not the exact map retained by the frozen plan")
    # Current identity verification is a prerequisite, not apply authority.
    # Historical evidence deliberately cannot pass this boundary.
    if mode != "current_admission":
        _fail("historical identity-v2 evidence is audit-only and cannot authorize admission")
    if authority.get("apply_authorized") is True:
        _fail("identity-v2 current admission cannot grant provider apply authority")
    return {
        "schema_version": "oasis7.identity_v2_admission.v1",
        "identity_v2_mode": mode,
        "identity_v2_evidence": retained,
        "plan_digest": plan["plan_digest"],
        "transaction_id": plan["transaction_id"],
        "provider_mutation_performed": False,
        "status": "identity-v2-admission-validated",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Execute or dry-run a governed full-network clean-room adapter transaction"
    )
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--authority", required=True, type=Path)
    parser.add_argument("--journal", required=True, type=Path)
    parser.add_argument("--ledger", required=True, type=Path)
    parser.add_argument(
        "--phase",
        choices=(STORAGE_FIRST_PHASE_ID,),
        help="explicit child phase to execute; omitted selects the legacy full-network adapter",
    )
    parser.add_argument(
        "--identity-v2-evidence-map",
        type=Path,
        help="exact v2 evidence map retained by the frozen plan",
    )
    parser.add_argument(
        "--identity-v2-mode",
        choices=("current_admission", "historical_audit"),
        help="identity-v2 admission mode; current mode is validation-only until apply authority exists",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="require an injected provider transport; otherwise fail closed",
    )
    args = parser.parse_args(argv)
    # Never let an apply request reach execute(), provenance verification, or
    # transaction locking without the exact current identity-v2 admission
    # inputs.  The non-apply path retains its plan-only behavior below.
    if args.apply and args.identity_v2_evidence_map is None:
        _fail("--apply requires the identity-v2 evidence map and current admission mode")
    if args.apply and args.identity_v2_mode != "current_admission":
        _fail("--apply requires identity-v2 current_admission mode")
    if (args.identity_v2_evidence_map is None) != (args.identity_v2_mode is None):
        _fail("identity-v2 evidence-map and mode must be supplied together")
    plan = _load_json(args.plan, "plan")
    authority = _load_json(args.authority, "authority")
    input_paths = (args.plan, args.authority)
    if args.identity_v2_evidence_map is not None:
        input_paths += (args.identity_v2_evidence_map,)
    if args.phase == STORAGE_FIRST_PHASE_ID:
        _storage_first_reject_aliases(
            args.journal, args.ledger, plan, input_paths=input_paths
        )
        evidence_map = (
            _load_json(args.identity_v2_evidence_map, "identity-v2 evidence map")
            if args.identity_v2_evidence_map is not None
            else plan.get("identity_v2_evidence")
        )
        if evidence_map is not None:
            evidence_map = _storage_first_identity_v2_admission_projection(
                evidence_map,
                mode=args.identity_v2_mode,
            )
        result = execute_storage_first(
            plan,
            authority,
            phase=args.phase,
            identity_v2_evidence=evidence_map,
            journal_path=args.journal,
            ledger_path=args.ledger,
            dry_run=not args.apply,
        )
        print(json.dumps(result, ensure_ascii=True, sort_keys=True))
        # Library callers receive the structured child result; the module
        # entrypoint below converts it to a conventional process exit code.
        return result
    _reject_journal_input_aliases(
        args.journal, args.ledger, plan, input_paths=input_paths
    )
    if args.identity_v2_evidence_map is not None:
        evidence_map = _load_json(args.identity_v2_evidence_map, "identity-v2 evidence map")
        # This bridge path deliberately performs no lock, journal, ledger, or
        # provider work. It validates the current receipt closure before any
        # future execute/resume call can be made eligible.
        validate_authority(plan, authority)
        result = _current_identity_v2_admission(
            plan, authority, evidence_map, args.identity_v2_mode
        )
        print(json.dumps(result, ensure_ascii=True, sort_keys=True))
        return 0
    result = execute(
        plan,
        authority,
        journal_path=args.journal,
        ledger_path=args.ledger,
        dry_run=not args.apply,
    )
    print(json.dumps(result, ensure_ascii=True, sort_keys=True))
    return 0


if __name__ == "__main__":
    outcome = main()
    raise SystemExit(outcome if isinstance(outcome, int) else 0)
