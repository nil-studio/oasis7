#!/usr/bin/env python3
"""Prepare, externally sign, assemble, and independently verify identity v2.

This executable is intentionally a small file-boundary tool.  It never accepts
private-key material or an arbitrary provider command/endpoint.  ``prepare``
freezes the exact canonical bytes, ``sign`` resolves a non-secret provider ID
from the pinned registry, ``assemble`` checks the detached signature before
emitting an envelope, and ``verify`` independently checks every binding before
writing an admission receipt.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, NoReturn


PREFIX = b"OASIS7-IDENTITY-RECEIPT-V2\0"
RAW_SCHEMA = "oasis7.identity_receipt.v1"
PAYLOAD_SCHEMA = "oasis7.identity_receipt.v2"
TRUST_SCHEMA = "oasis7.identity_v2_trust_config.v1"
REGISTRY_SCHEMA = "oasis7.identity_v2_provider_registry.v1"
CONTEXT_SCHEMA = "oasis7.identity_v2_context.v1"
INTENT_SCHEMA = "oasis7.clean_room_plan_intent.v2"
ALGORITHM = "ed25519"
DOMAIN = "oasis7.identity_receipt.v2/signature/v1"
PROVIDER_ATTESTATION_SCHEMA_V2 = "oasis7.identity_v2_provider_attestation.v2"
PROVIDER_AUTHENTICATION_PROOF_SCHEMA_V1 = "oasis7.identity_v2_provider_authentication_proof.v1"
PROVIDER_AUTHENTICATION_CLAIMS_SCHEMA_V1 = "oasis7.identity_v2_provider_authentication_claims.v1"
PROVIDER_AUTHENTICATION_DOMAIN_V1 = "oasis7.identity-v2-provider-authentication/v1"
PROVIDER_AUTHENTICATION_PREFIX = b"OASIS7-IDENTITY-V2-PROVIDER-AUTH\0"
CANONICAL_NETWORK_ID = "oasis7-public-testnet-governed-20260606"
MAX_CLOCK_SKEW_SECONDS = 5
DEPLOYED_TRUST_CONFIG = Path("/operator/truth/identity-v2-trust-config.json")
DEPLOYED_PROVIDER_REGISTRY = Path("/operator/truth/identity-v2-provider-registry.json")
DEPLOYED_GOVERNANCE_ROOT = Path("/operator/truth/governance-root.json")
# Identity-v2 is not provisioned in this repository.  Deployment authority
# must install these independent pins before any admission-capable command can
# run; caller/registry-provided digests are consistency checks only.
PINNED_TRUST_CONFIG_SHA256: str | None = None
PINNED_PROVIDER_REGISTRY_SHA256: str | None = None
# Identity-v2 authority artifacts are operator-local regular files.  Public
# readability is intentional for trust metadata and public keys; the safety
# boundary is ownership plus the absence of group/other write access.
AUTHORITY_UNAUTHORIZED_WRITE_BITS = 0o022
ADAPTER_MODULE_NAME = "oasis7_identity_v2_adapter"
ADAPTER_PATH = Path(__file__).with_name("p2p-public-testnet-full-network-clean-room-adapter.py")
PAYLOAD_FIELDS = frozenset(
    {
        "domain_separator",
        "schema_version",
        "signer_id",
        "verifier_id",
        "trust_root_id",
        "network_id",
        "task_uid",
        "head_oid",
        "frozen_head_oid",
        "plan_digest",
        "context_digest",
        "capture_window_id",
        "rotation_epoch",
        "issued_at",
        "expires_at",
        "node_id",
        "peer_id",
        "key_sha256",
        "key_size_bytes",
        "key_mode",
        "key_uid",
        "key_gid",
        "signed_payload_sha256",
    }
)
RAW_FIELDS = frozenset(
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
CONTEXT_FIELDS = frozenset(
    {
        "schema_version",
        "network_id",
        "task_uid",
        "head_oid",
        "capture_window_id",
        "capture_start",
        "capture_end",
        "rotation_epoch",
        "issued_at",
        "expires_at",
    }
)
INTENT_FIELDS = frozenset({"schema_version", "context_digest", "adapter_action", "nodes", "peer_registry_sha256", "peer_registry_epoch"})
NODE_FIELDS = frozenset({"node_name", "node_id", "peer_id", "role", "reset_surface_ids"})
ATTESTATION_FIELDS = frozenset(
    {
        "schema_version",
        "network_id",
        "provider_id",
        "request_id",
        "signer_id",
        "public_key_sha256",
        "algorithm",
        "canonical_payload_sha256",
        "signature_sha256",
        "context_digest",
        "rotation_epoch",
        "capture_window_id",
        "issued_at",
        "expires_at",
        "task_uid",
        "head_oid",
        "proof_ref",
        "proof",
    }
)
PROOF_FIELDS = frozenset({"schema_version", "algorithm", "claims_sha256", "signature_hex"})
AUTHENTICATION_CLAIMS_FIELDS = frozenset(
    {
        "schema_version",
        "domain_separator",
        "network_id",
        "provider_id",
        "request_id",
        "signer_id",
        "public_key_sha256",
        "canonical_payload_sha256",
        "signature_sha256",
        "context_digest",
        "task_uid",
        "head_oid",
        "rotation_epoch",
        "capture_window_id",
        "issued_at",
        "expires_at",
        "proof_ref",
    }
)
HEX64 = re.compile(r"^[0-9a-f]{64}$")
OID = re.compile(r"^[0-9a-f]{40}$")
REQUEST_ID = re.compile(r"^req-v2:[0-9a-f]{64}$")
PROOF_REF = re.compile(r"^proof-v1:[0-9a-f]{64}$")
SAFE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{1,127}$")
UTC = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")


class ToolError(Exception):
    pass


def fail(message: str) -> NoReturn:
    raise ToolError(message)


def _reject_constant(value: str) -> NoReturn:
    fail(f"non-finite JSON value {value} is forbidden")


def _object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def canonical(value: Any) -> bytes:
    try:
        return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False).encode(
            "utf-8"
        )
    except (TypeError, ValueError) as error:
        fail(f"cannot canonicalize JSON: {error.__class__.__name__}")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _regular(path_value: str | Path, label: str) -> Path:
    path = Path(path_value)
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be a regular non-symlink file")
    return path


def _regular_ancestor(path: Path, label: str) -> None:
    # Inspect the path as supplied.  Resolving first would hide a symlinked
    # ancestor and turn an operator path-control failure into a false pass.
    current = path.parent
    while True:
        # macOS exposes the system temporary tree through the conventional
        # ``/var`` (and occasionally ``/tmp``) alias.  These fixed aliases are
        # safe OS paths; operator/deployment ancestors remain symlink-intolerant.
        if current.is_symlink() and current not in {Path("/var"), Path("/tmp")}:
            fail(f"{label} has an invalid ancestor")
        if not current.is_dir():
            fail(f"{label} has an invalid ancestor")
        if current.parent == current:
            break
        current = current.parent


def _authority_ancestors(path: Path, label: str) -> None:
    """Reject replaceable authority directories while allowing sticky /tmp."""

    current = path.parent
    while True:
        try:
            directory = current.stat()
        except OSError as error:
            fail(f"cannot stat {label} ancestor: {error.__class__.__name__}")
        if not stat.S_ISDIR(directory.st_mode):
            fail(f"{label} has a non-directory ancestor")
        mode = stat.S_IMODE(directory.st_mode)
        if directory.st_uid not in {0, os.getuid()}:
            fail(f"{label} has a foreign-owned replaceable ancestor")
        sticky_safe = bool(mode & stat.S_ISVTX) and directory.st_uid == 0
        if mode & AUTHORITY_UNAUTHORIZED_WRITE_BITS and not sticky_safe:
            fail(f"{label} has an unauthorized-writable ancestor")
        if current.parent == current:
            break
        current = current.parent


def read_authority_bytes(
    path_value: str | Path, label: str, *, executable: bool = False,
    identities: dict[str, tuple[int, ...]] | None = None,
) -> bytes:
    """Read one authority artifact through a metadata-bound, no-follow FD.

    The descriptor prevents a same-path replacement from changing the bytes
    being hashed/parsed.  Pre/post metadata checks reject a mutation while the
    descriptor is open; callers that execute an authority binary also compare
    its digest around the subprocess boundary.
    """

    path = _regular(path_value, label)
    _regular_ancestor(path, label)
    _authority_ancestors(path, label)
    nofollow = getattr(os, "O_NOFOLLOW", 0)
    if not nofollow:
        fail(f"platform cannot enforce no-symlink authority reads for {label}")
    try:
        descriptor = os.open(path, os.O_RDONLY | nofollow)
    except OSError as error:
        fail(f"cannot open {label} as an authority artifact: {error.__class__.__name__}")
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            fail(f"{label} must be a regular authority file")
        if before.st_uid != os.getuid():
            fail(f"{label} is not owned by the operator account")
        mode = stat.S_IMODE(before.st_mode)
        if mode & AUTHORITY_UNAUTHORIZED_WRITE_BITS:
            fail(f"{label} is writable by a non-owner account")
        if executable and not mode & stat.S_IXUSR:
            fail(f"{label} is not owner-executable")
        stream = os.fdopen(descriptor, "rb", closefd=False)
        try:
            raw = stream.read()
        finally:
            stream.close()
        after = os.fstat(descriptor)
        identity = lambda value: (
            value.st_dev,
            value.st_ino,
            value.st_size,
            value.st_mode,
            value.st_uid,
            value.st_gid,
            value.st_mtime_ns,
            value.st_ctime_ns,
        )
        if identity(before) != identity(after):
            fail(f"{label} changed while being read")
        try:
            path_after = os.stat(path, follow_symlinks=False)
        except OSError as error:
            fail(f"cannot restat {label} authority artifact: {error.__class__.__name__}")
        if (path_after.st_dev, path_after.st_ino) != (before.st_dev, before.st_ino):
            fail(f"{label} was replaced while being read")
        if identities is not None:
            identities[label] = identity(before)
        return raw
    except ToolError:
        raise
    except OSError as error:
        fail(f"cannot read {label} authority artifact: {error.__class__.__name__}")
    finally:
        os.close(descriptor)


def authority_digest(path_value: str | Path, label: str, *, executable: bool = False) -> str:
    return sha256_bytes(read_authority_bytes(path_value, label, executable=executable))


def require_authority_unchanged(
    path_value: str | Path,
    label: str,
    expected_digest: str,
    *,
    executable: bool = False,
) -> None:
    if authority_digest(path_value, label, executable=executable) != expected_digest:
        fail(f"{label} changed during use")


def read_bytes(path_value: str | Path, label: str) -> bytes:
    path = _regular(path_value, label)
    _regular_ancestor(path, label)
    try:
        return path.read_bytes()
    except OSError as error:
        fail(f"cannot read {label}: {error.__class__.__name__}")


def read_json(
    path_value: str | Path,
    label: str,
    *,
    require_canonical: bool = True,
    authority: bool = False,
) -> dict[str, Any]:
    raw = read_authority_bytes(path_value, label) if authority else read_bytes(path_value, label)
    if raw.startswith(b"\xef\xbb\xbf"):
        fail(f"{label} must not contain a BOM")
    try:
        text = raw.decode("utf-8")
        value = json.loads(
            text,
            object_pairs_hook=_object_pairs,
            parse_constant=_reject_constant,
        )
    except ToolError:
        raise
    except (UnicodeError, json.JSONDecodeError) as error:
        fail(f"{label} is not valid UTF-8 JSON: {error.__class__.__name__}")
    if not isinstance(value, dict):
        fail(f"{label} must contain a JSON object")
    if require_canonical and canonical(value) != raw:
        fail(f"{label} is not canonical JSON")
    return value


def require_exact_fields(value: dict[str, Any], fields: frozenset[str], label: str) -> None:
    if set(value) != fields:
        missing = sorted(fields - set(value))
        extra = sorted(set(value) - fields)
        fail(f"{label} fields are not exact (missing={missing}, extra={extra})")


def require_string(value: Any, label: str, *, safe: bool = False) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{label} must be a non-empty string")
    if safe and SAFE_ID.fullmatch(value) is None:
        fail(f"{label} is not a safe identifier")
    return value


def require_hex(value: Any, label: str, pattern: re.Pattern[str] = HEX64) -> str:
    if not isinstance(value, str) or pattern.fullmatch(value) is None:
        fail(f"{label} has invalid lowercase hexadecimal shape")
    return value


def require_integer(value: Any, label: str, *, positive: bool = False) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 0 or (positive and value == 0):
        fail(f"{label} must be a {'positive' if positive else 'non-negative'} integer")
    return value


def parse_timestamp(value: Any, label: str) -> datetime:
    text = require_string(value, label)
    if UTC.fullmatch(text) is None:
        fail(f"{label} must be UTC RFC3339 seconds")
    try:
        return datetime.strptime(text, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError:
        fail(f"{label} is not a valid UTC timestamp")


def validate_raw(raw_bytes: bytes) -> dict[str, Any]:
    try:
        value = json.loads(
            raw_bytes.decode("utf-8"), object_pairs_hook=_object_pairs, parse_constant=_reject_constant
        )
    except ToolError:
        raise
    except (UnicodeError, json.JSONDecodeError) as error:
        fail(f"raw-v1 is not valid UTF-8 JSON: {error.__class__.__name__}")
    if not isinstance(value, dict):
        fail("raw-v1 must contain a JSON object")
    require_exact_fields(value, RAW_FIELDS, "raw-v1")
    if value.get("schema_version") != RAW_SCHEMA:
        fail("raw-v1 schema is unsupported")
    for field in ("node_id", "peer_id", "key_path"):
        require_string(value.get(field), f"raw-v1.{field}")
    require_hex(value.get("key_sha256"), "raw-v1.key_sha256")
    require_integer(value.get("key_size_bytes"), "raw-v1.key_size_bytes", positive=True)
    require_integer(value.get("key_mode"), "raw-v1.key_mode")
    require_integer(value.get("key_uid"), "raw-v1.key_uid")
    require_integer(value.get("key_gid"), "raw-v1.key_gid")
    if value["key_mode"] != 0o600:
        fail("raw-v1.key_mode must be 0600")
    return value


def validate_trust(trust: dict[str, Any], label: str = "trust-config") -> dict[str, Any]:
    require_exact_fields(
        trust,
        frozenset({"schema_version", "network_id", "trust_root_id", "verifier_id", "algorithm", "rotation_epoch", "allowlist", "revocations"}),
        label,
    )
    if trust["schema_version"] != TRUST_SCHEMA or trust["algorithm"] != ALGORITHM:
        fail(f"{label} schema or algorithm is unsupported")
    if trust.get("network_id") != CANONICAL_NETWORK_ID:
        fail(f"{label}.network_id is not the governed deployment network")
    for field in ("network_id", "trust_root_id", "verifier_id", "rotation_epoch"):
        require_string(trust.get(field), f"{label}.{field}", safe=(field == "rotation_epoch"))
    allowlist = trust["allowlist"]
    if not isinstance(allowlist, list) or not allowlist:
        fail(f"{label}.allowlist must be non-empty")
    seen: set[str] = set()
    for index, entry in enumerate(allowlist):
        item_label = f"{label}.allowlist[{index}]"
        if not isinstance(entry, dict):
            fail(f"{item_label} must be an object")
        require_exact_fields(
            entry,
            frozenset({"signer_id", "public_key_ref", "public_key_sha256", "status", "valid_from", "valid_until"}),
            item_label,
        )
        signer = require_string(entry.get("signer_id"), f"{item_label}.signer_id", safe=True)
        if signer in seen:
            fail(f"{label} contains duplicate signer_id")
        seen.add(signer)
        require_string(entry.get("public_key_ref"), f"{item_label}.public_key_ref")
        require_hex(entry.get("public_key_sha256"), f"{item_label}.public_key_sha256")
        if entry.get("status") not in {"active", "retired", "revoked"}:
            fail(f"{item_label}.status is unsupported")
        start = parse_timestamp(entry.get("valid_from"), f"{item_label}.valid_from")
        end = parse_timestamp(entry.get("valid_until"), f"{item_label}.valid_until")
        if end <= start:
            fail(f"{item_label} validity interval is inverted")
    revocations = trust["revocations"]
    if not isinstance(revocations, list):
        fail(f"{label}.revocations must be an array")
    revocation_seen: set[str] = set()
    for index, item in enumerate(revocations):
        item_label = f"{label}.revocations[{index}]"
        if not isinstance(item, dict):
            fail(f"{item_label} must be an object")
        require_exact_fields(item, frozenset({"signer_id", "effective_at", "reason"}), item_label)
        signer = require_string(item.get("signer_id"), f"{item_label}.signer_id", safe=True)
        if signer in revocation_seen:
            fail(f"{label} contains duplicate revocation")
        revocation_seen.add(signer)
        parse_timestamp(item.get("effective_at"), f"{item_label}.effective_at")
        require_string(item.get("reason"), f"{item_label}.reason")
    return trust


def resolved_equal(first: str | Path, second: str | Path) -> bool:
    try:
        return Path(first).resolve() == Path(second).resolve()
    except OSError:
        return False


def validate_registry(
    registry: dict[str, Any], registry_path: Path, trust_path: Path, *, label: str = "provider-registry"
) -> tuple[dict[str, Any], dict[str, Any]]:
    require_exact_fields(registry, frozenset({"schema_version", "trust_config_path", "trust_config_sha256", "providers", "verifier"}), label)
    if registry["schema_version"] != REGISTRY_SCHEMA:
        fail(f"{label} schema is unsupported")
    if not resolved_equal(registry["trust_config_path"], trust_path):
        fail(f"{label} trust-config path is not the pinned path")
    trust_bytes = read_authority_bytes(trust_path, "trust-config")
    require_hex(registry.get("trust_config_sha256"), f"{label}.trust_config_sha256")
    if registry["trust_config_sha256"] != sha256_bytes(trust_bytes):
        fail(f"{label} trust-config digest mismatch")
    trust = validate_trust(read_json(trust_path, "trust-config", authority=True))
    providers = registry["providers"]
    if not isinstance(providers, list) or not providers:
        fail(f"{label}.providers must be a non-empty array")
    ids: set[str] = set()
    provider_adapter_paths: list[Path] = []
    for index, provider in enumerate(providers):
        item_label = f"{label}.providers[{index}]"
        if not isinstance(provider, dict):
            fail(f"{item_label} must be an object")
        require_exact_fields(
            provider,
            frozenset({"provider_id", "adapter_path", "adapter_sha256", "signer_id", "public_key_ref", "public_key_sha256", "algorithm"}),
            item_label,
        )
        provider_id = require_string(provider.get("provider_id"), f"{item_label}.provider_id", safe=True)
        if provider_id in ids:
            fail(f"{label} contains duplicate provider_id")
        ids.add(provider_id)
        require_string(provider.get("adapter_path"), f"{item_label}.adapter_path")
        adapter = _regular(provider["adapter_path"], f"{item_label}.adapter_path")
        read_authority_bytes(adapter, f"{item_label}.adapter_path", executable=True)
        provider_adapter_paths.append(adapter)
        require_hex(provider.get("adapter_sha256"), f"{item_label}.adapter_sha256")
        if provider["adapter_sha256"] != sha256_bytes(read_authority_bytes(adapter, f"{item_label}.adapter_path", executable=True)):
            fail(f"{item_label}.adapter_sha256 mismatch")
        if provider.get("algorithm") != ALGORITHM:
            fail(f"{item_label}.algorithm is unsupported")
        require_string(provider.get("signer_id"), f"{item_label}.signer_id", safe=True)
        require_string(provider.get("public_key_ref"), f"{item_label}.public_key_ref")
        public_key = _regular(provider["public_key_ref"], f"{item_label}.public_key_ref")
        public_bytes = read_authority_bytes(public_key, f"{item_label}.public_key_ref")
        if len(public_bytes) != 32:
            fail(f"{item_label}.public_key_ref must contain a raw 32-byte Ed25519 key")
        require_hex(provider.get("public_key_sha256"), f"{item_label}.public_key_sha256")
        if provider["public_key_sha256"] != sha256_bytes(public_bytes):
            fail(f"{item_label}.public_key_sha256 mismatch")
        trusted = [item for item in trust["allowlist"] if item["signer_id"] == provider["signer_id"]]
        if len(trusted) != 1 or not resolved_equal(trusted[0]["public_key_ref"], public_key):
            fail(f"{item_label} is not bound to the trust allowlist")
        if trusted[0]["public_key_sha256"] != provider["public_key_sha256"]:
            fail(f"{item_label} public key is not bound to the trust allowlist")
    verifier = registry["verifier"]
    if not isinstance(verifier, dict):
        fail(f"{label}.verifier must be an object")
    require_exact_fields(verifier, frozenset({"executable_path", "executable_sha256"}), f"{label}.verifier")
    require_string(verifier.get("executable_path"), f"{label}.verifier.executable_path")
    verifier_path = _regular(verifier["executable_path"], f"{label}.verifier.executable_path")
    read_authority_bytes(verifier_path, f"{label}.verifier.executable_path", executable=True)
    forbidden_paths = (
        Path(__file__),
        Path(__file__).with_name("p2p-public-testnet-identity-receipt-v2.py"),
        Path(__file__).with_name("p2p-public-testnet-full-network-clean-room.py"),
        Path(__file__).with_name("p2p-public-testnet-full-network-clean-room-adapter.py"),
        *provider_adapter_paths,
    )
    if any(resolved_equal(verifier_path, forbidden) for forbidden in forbidden_paths):
        fail(f"{label}.verifier executable is not an independent verifier")
    require_hex(verifier.get("executable_sha256"), f"{label}.verifier.executable_sha256")
    if verifier["executable_sha256"] != sha256_bytes(read_authority_bytes(verifier_path, f"{label}.verifier.executable_path", executable=True)):
        fail(f"{label}.verifier.executable_sha256 mismatch")
    return trust, registry


def _load_adapter_module() -> Any:
    """Load the repository-owned adapter that owns the live root validator."""

    existing = sys.modules.get(ADAPTER_MODULE_NAME)
    if existing is not None:
        return existing
    if ADAPTER_PATH.is_symlink() or not ADAPTER_PATH.is_file():
        fail("repository-owned trust-root validator is unavailable")
    spec = importlib.util.spec_from_file_location(ADAPTER_MODULE_NAME, ADAPTER_PATH)
    if spec is None or spec.loader is None:
        fail("repository-owned trust-root validator cannot be loaded")
    module = importlib.util.module_from_spec(spec)
    sys.modules[ADAPTER_MODULE_NAME] = module
    try:
        spec.loader.exec_module(module)
    except Exception as error:
        sys.modules.pop(ADAPTER_MODULE_NAME, None)
        fail(f"repository-owned trust-root validator failed to load: {error.__class__.__name__}")
    return module


def _validate_governance_root_pin() -> None:
    """Reuse the adapter's code-owned path/content/metadata root validator."""

    adapter = _load_adapter_module()
    adapter_root = Path(adapter.CANONICAL_TRUST_ROOT_PATH)
    if adapter_root.resolve() != DEPLOYED_GOVERNANCE_ROOT.resolve():
        fail("identity-v2 and adapter trust-root paths are not the same code-owned path")
    try:
        adapter.validate_live_trust_root_file()
    except SystemExit as error:
        fail(f"canonical governance-root validation failed: {error}")
    except Exception as error:
        fail(f"canonical governance-root validation failed: {error.__class__.__name__}")


def _authority_scope(trust_path: Path, registry_path: Path, registry: dict[str, Any]) -> str:
    """Require the code-owned deployed trust path and governance-root pin."""

    if trust_path.resolve() != DEPLOYED_TRUST_CONFIG.resolve():
        fail("trust-config is not the canonical deployed authority path")
    if registry_path.resolve() != DEPLOYED_PROVIDER_REGISTRY.resolve():
        fail("provider registry is not the canonical deployed authority path")
    if not PINNED_TRUST_CONFIG_SHA256 or not PINNED_PROVIDER_REGISTRY_SHA256:
        fail("identity-v2 trust-config/provider-registry anchors are not provisioned")
    if sha256_bytes(read_authority_bytes(trust_path, "trust-config")) != PINNED_TRUST_CONFIG_SHA256:
        fail("identity-v2 trust-config content digest is not independently pinned")
    if sha256_bytes(read_authority_bytes(registry_path, "provider-registry")) != PINNED_PROVIDER_REGISTRY_SHA256:
        fail("identity-v2 provider-registry content digest is not independently pinned")
    _validate_governance_root_pin()
    return "deployed-governance-root"


def find_provider(registry: dict[str, Any], provider_id: str) -> dict[str, Any]:
    require_string(provider_id, "provider-ref", safe=True)
    matches = [provider for provider in registry["providers"] if provider["provider_id"] == provider_id]
    if len(matches) != 1:
        fail("provider-ref is not an allowlisted provider ID")
    return matches[0]


def validate_context(context: dict[str, Any], *, now: datetime | None = None, historical_audit: bool = False) -> dict[str, Any]:
    require_exact_fields(context, CONTEXT_FIELDS, "context")
    if context["schema_version"] != CONTEXT_SCHEMA:
        fail("context schema is unsupported")
    for field in ("network_id", "task_uid", "capture_window_id", "rotation_epoch"):
        require_string(context.get(field), f"context.{field}", safe=(field in {"task_uid", "capture_window_id", "rotation_epoch"}))
    if context["network_id"] != CANONICAL_NETWORK_ID:
        fail("context.network_id is not the governed deployment network")
    head = require_string(context.get("head_oid"), "context.head_oid")
    if OID.fullmatch(head) is None:
        fail("context.head_oid must be 40 lowercase hexadecimal characters")
    start = parse_timestamp(context.get("capture_start"), "context.capture_start")
    end = parse_timestamp(context.get("capture_end"), "context.capture_end")
    issued = parse_timestamp(context.get("issued_at"), "context.issued_at")
    expires = parse_timestamp(context.get("expires_at"), "context.expires_at")
    if not start <= issued < expires <= end:
        fail("context capture/freshness ordering is invalid")
    current = now or datetime.now(timezone.utc)
    if issued > current + timedelta(seconds=MAX_CLOCK_SKEW_SECONDS):
        fail("context issued_at is in the future")
    if not historical_audit and expires <= current:
        fail("context freshness window is stale")
    return context


_PEER_REGISTRY_MODULE = None


def _peer_registry_authority():
    global _PEER_REGISTRY_MODULE
    if _PEER_REGISTRY_MODULE is None:
        path = Path(__file__).with_name("p2p-public-testnet-peer-registry.py")
        spec = importlib.util.spec_from_file_location("managed_peer_registry", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        _PEER_REGISTRY_MODULE = module
    return _PEER_REGISTRY_MODULE


def validate_intent(intent: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    if intent.get("schema_version") == "oasis7.clean_room_plan_intent.v1":
        fail("legacy plan-intent schema lacks current peer registry binding")
    require_exact_fields(intent, INTENT_FIELDS, "plan-intent")
    if intent["schema_version"] != INTENT_SCHEMA:
        fail("plan-intent schema is unsupported")
    try:
        snapshot = _peer_registry_authority().load_snapshot()
    except SystemExit as error:
        fail(str(error))
    if (intent["peer_registry_sha256"] != snapshot["sha256"]
            or intent["peer_registry_epoch"] != snapshot["registry_epoch"]):
        fail("plan-intent peer registry digest or epoch is not current")
    if intent["context_digest"] != sha256_bytes(canonical(context)):
        fail("plan-intent context digest mismatch")
    require_string(intent.get("adapter_action"), "plan-intent.adapter_action", safe=True)
    nodes = intent["nodes"]
    if not isinstance(nodes, list) or not nodes:
        fail("plan-intent.nodes must be a non-empty array")
    names: set[str] = set()
    peers: set[str] = set()
    previous = ""
    for index, node in enumerate(nodes):
        label = f"plan-intent.nodes[{index}]"
        if not isinstance(node, dict):
            fail(f"{label} must be an object")
        require_exact_fields(node, NODE_FIELDS, label)
        name = require_string(node.get("node_name"), f"{label}.node_name", safe=True)
        if name in names or name < previous:
            fail("plan-intent nodes must be sorted and unique")
        names.add(name)
        previous = name
        require_string(node.get("node_id"), f"{label}.node_id", safe=True)
        peer = require_string(node.get("peer_id"), f"{label}.peer_id", safe=True)
        if (name not in snapshot["peers"] or peer != snapshot["peers"][name]
                or node["node_id"] != _peer_registry_authority().NODE_IDS[name]):
            fail("plan-intent node peer does not match current registry")
        if peer in peers:
            fail("plan-intent peers must be unique")
        peers.add(peer)
        require_string(node.get("role"), f"{label}.role", safe=True)
        surfaces = node.get("reset_surface_ids")
        if not isinstance(surfaces, list) or not all(isinstance(item, str) and item for item in surfaces):
            fail(f"{label}.reset_surface_ids must contain strings")
        if surfaces != sorted(set(surfaces)):
            fail(f"{label}.reset_surface_ids must be sorted and unique")
    if names != set(snapshot["peers"]):
        fail("plan-intent must cover the complete peer registry")
    return intent


def validate_template(template: dict[str, Any], raw: dict[str, Any], context: dict[str, Any], plan_digest: str, trust: dict[str, Any]) -> dict[str, Any]:
    require_exact_fields(template, PAYLOAD_FIELDS, "template")
    if template.get("domain_separator") != DOMAIN or template.get("schema_version") != PAYLOAD_SCHEMA:
        fail("template domain or schema is unsupported")
    signer = require_string(template.get("signer_id"), "template.signer_id", safe=True)
    matches = [entry for entry in trust["allowlist"] if entry["signer_id"] == signer]
    if len(matches) != 1:
        fail("template signer is not allowlisted")
    require_string(template.get("verifier_id"), "template.verifier_id")
    if template["verifier_id"] != trust["verifier_id"]:
        fail("template verifier is not governed")
    if template.get("trust_root_id") != trust["trust_root_id"]:
        fail("template trust root is not governed")
    for field in ("task_uid", "capture_window_id", "rotation_epoch", "node_id", "peer_id"):
        require_string(template.get(field), f"template.{field}", safe=(field in {"task_uid", "capture_window_id", "rotation_epoch"}))
    for field in ("head_oid", "frozen_head_oid"):
        if OID.fullmatch(require_string(template.get(field), f"template.{field}")) is None:
            fail(f"template.{field} must be a lowercase commit OID")
    if template["head_oid"] != template["frozen_head_oid"] or template["head_oid"] != context["head_oid"]:
        fail("template head binding mismatch")
    if template["task_uid"] != context["task_uid"] or template["capture_window_id"] != context["capture_window_id"] or template["rotation_epoch"] != context["rotation_epoch"]:
        fail("template context binding mismatch")
    if template["network_id"] != context["network_id"] or template["network_id"] != trust["network_id"]:
        fail("template network binding mismatch")
    if template["plan_digest"] != plan_digest or template["context_digest"] != sha256_bytes(canonical(context)):
        fail("template plan/context digest mismatch")
    issued = parse_timestamp(template.get("issued_at"), "template.issued_at")
    expires = parse_timestamp(template.get("expires_at"), "template.expires_at")
    if issued != parse_timestamp(context["issued_at"], "context.issued_at") or expires != parse_timestamp(context["expires_at"], "context.expires_at"):
        fail("template freshness binding mismatch")
    if not parse_timestamp(context["capture_start"], "context.capture_start") <= issued < expires <= parse_timestamp(context["capture_end"], "context.capture_end"):
        fail("template freshness is outside capture window")
    for field in ("key_size_bytes", "key_uid", "key_gid"):
        require_integer(template.get(field), f"template.{field}")
    require_hex(template.get("key_sha256"), "template.key_sha256")
    if template["key_sha256"] != raw["key_sha256"] or template["key_size_bytes"] != raw["key_size_bytes"] or template["key_uid"] != raw["key_uid"] or template["key_gid"] != raw["key_gid"] or template["key_mode"] != "0600" or raw["key_mode"] != 0o600:
        fail("template key metadata is not bound to raw-v1")
    if template["node_id"] != raw["node_id"] or template["peer_id"] != raw["peer_id"]:
        fail("template node/peer is not bound to raw-v1")
    require_hex(template.get("signed_payload_sha256"), "template.signed_payload_sha256")
    return template


def payload_bytes(payload: dict[str, Any]) -> bytes:
    require_exact_fields(payload, PAYLOAD_FIELDS, "payload")
    return PREFIX + canonical(payload)


def _atomic_write(path_value: str | Path, value: bytes, label: str) -> None:
    path = Path(path_value)
    if path.exists() and path.is_symlink():
        fail(f"{label} must not be a symlink")
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=f".{path.name}.", delete=False) as temporary:
            temporary_path = Path(temporary.name)
            temporary.write(value)
            temporary.flush()
            os.fsync(temporary.fileno())
        temporary_path.chmod(0o600)
        os.replace(temporary_path, path)
    except OSError as error:
        try:
            temporary_path.unlink(missing_ok=True)
        except (NameError, OSError):
            pass
        fail(f"cannot write {label}: {error.__class__.__name__}")


def _publish_verification_pair(outputs: tuple[tuple[Path, bytes], ...]) -> None:
    """Stage both outputs and undo completed replacements on caught failure.

    This is failure recovery, not crash-atomic visibility across two paths.
    """
    staged: list[tuple[Path, Path, Path | None]] = []
    published: list[tuple[Path, Path | None]] = []
    directories: list[Path] = []
    retain_backups = False
    try:
        for destination, payload in outputs:
            if destination.is_symlink():
                fail("verification output must not be a symlink")
            destination.parent.mkdir(parents=True, exist_ok=True)
            directory = Path(tempfile.mkdtemp(
                dir=destination.parent, prefix=f".{destination.name}.pair-"
            ))
            directories.append(directory)
            staged_path = directory / "new"
            _atomic_write(staged_path, payload, "staged verification output")
            backup = None
            if destination.exists():
                backup = directory / "previous"
                _atomic_write(backup, destination.read_bytes(), "verification output backup")
                backup.chmod(stat.S_IMODE(destination.stat().st_mode))
            staged.append((destination, staged_path, backup))
        for destination, staged_path, backup in staged:
            os.replace(staged_path, destination)
            published.append((destination, backup))
    except (OSError, ToolError):
        for destination, backup in reversed(published):
            try:
                if backup is None:
                    destination.unlink(missing_ok=True)
                else:
                    os.replace(backup, destination)
            except OSError:
                # Retain backups if restoration itself cannot complete.
                retain_backups = True
                fail("cannot write verification pair; restoration failed; retained backups require reconciliation")
        fail("cannot write verification pair; previous outputs restored")
    finally:
        for directory in directories:
            # A restoration failure must retain its recovery artifacts.
            if not retain_backups:
                shutil.rmtree(directory)


def _code_owned_paths() -> tuple[Path, ...]:
    return (
        *_peer_registry_authority().publisher_protected_paths(),
        DEPLOYED_TRUST_CONFIG,
        DEPLOYED_PROVIDER_REGISTRY,
        DEPLOYED_GOVERNANCE_ROOT,
        Path(__file__),
        ADAPTER_PATH,
        Path(__file__).with_name("p2p-public-testnet-identity-receipt-v2.py"),
        Path(__file__).with_name("p2p-public-testnet-full-network-clean-room.py"),
        Path(__file__).with_name("p2p-public-testnet-full-network-clean-room-adapter.py"),
    )


def _registry_artifact_paths(registry: dict[str, Any]) -> list[Path]:
    """Extract registry-referenced authority paths for output alias checks."""

    paths: list[Path] = []
    trust_path = registry.get("trust_config_path")
    if isinstance(trust_path, str):
        paths.append(Path(trust_path))
    providers = registry.get("providers")
    if isinstance(providers, list):
        for provider in providers:
            if not isinstance(provider, dict):
                continue
            for field in ("adapter_path", "public_key_ref"):
                value = provider.get(field)
                if isinstance(value, str):
                    paths.append(Path(value))
    verifier = registry.get("verifier")
    if isinstance(verifier, dict) and isinstance(verifier.get("executable_path"), str):
        paths.append(Path(verifier["executable_path"]))
    return paths


def _reject_output_aliases(
    outputs: list[tuple[str | Path, str]], protected: list[tuple[str | Path, str]]
) -> None:
    """Reject output aliases before any transaction cleanup or validation."""

    protected_paths = [(Path(path), label) for path, label in protected]
    protected_paths.extend((path, "code-owned path") for path in _code_owned_paths())
    seen: dict[Path, str] = {}
    for output_value, output_label in outputs:
        output = Path(output_value)
        if output.is_symlink():
            fail(f"{output_label} must not be a symlink")
        resolved = output.resolve()
        if resolved in seen:
            fail(f"{output_label} aliases {seen[resolved]}")
        seen[resolved] = output_label
        for protected_path, protected_label in protected_paths:
            same_file = False
            try:
                same_file = output.exists() and protected_path.exists() and os.path.samefile(output, protected_path)
            except OSError:
                pass
            if resolved == protected_path.resolve() or same_file:
                fail(f"{output_label} aliases protected {protected_label}")


def _clear_derived_output(path_value: str | Path, label: str) -> None:
    """Validate a derived output without deleting prior evidence.

    Successful commands atomically replace their derived outputs.  Keeping an
    existing regular output until that point preserves prior evidence when a
    retry fails validation; aliases are rejected by _reject_output_aliases.
    """

    path = Path(path_value)
    if not path.exists():
        return
    if path.is_symlink() or not path.is_file():
        fail(f"{label} must be an absent regular output or a regular file")
    # Leave prior evidence in place until the complete command succeeds and
    # _atomic_write can replace it atomically.


# RFC 8032 Ed25519 verifier.  It is used only for independent verification;
# signing remains in the external custody adapter.
Q = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493
D = (-121665 * pow(121666, -1, Q)) % Q
D2 = (2 * D) % Q
I = pow(2, (Q - 1) // 4, Q)


def _xrecover(y: int) -> int:
    xx = (y * y - 1) * pow(D * y * y + 1, -1, Q) % Q
    x = pow(xx, (Q + 3) // 8, Q)
    if (x * x - xx) % Q:
        x = x * I % Q
    if (x * x - xx) % Q:
        fail("invalid Ed25519 point")
    return Q - x if x & 1 else x


BY = 4 * pow(5, -1, Q) % Q
BX = _xrecover(BY)
B = (BX, BY)


def _edwards(first: tuple[int, int], second: tuple[int, int]) -> tuple[int, int]:
    x1, y1 = first
    x2, y2 = second
    xy = D * x1 * x2 * y1 * y2 % Q
    denominator_x = pow(1 + xy, -1, Q)
    denominator_y = pow(1 - xy, -1, Q)
    return ((x1 * y2 + x2 * y1) * denominator_x % Q, (y1 * y2 + x1 * x2) * denominator_y % Q)


def _ext_add(
    first: tuple[int, int, int, int], second: tuple[int, int, int, int]
) -> tuple[int, int, int, int]:
    x1, y1, z1, t1 = first
    x2, y2, z2, t2 = second
    a = ((y1 - x1) * (y2 - x2)) % Q
    b = ((y1 + x1) * (y2 + x2)) % Q
    c = (t1 * D2 * t2) % Q
    d = (2 * z1 * z2) % Q
    e = b - a
    f = d - c
    g = d + c
    h = b + a
    return (e * f) % Q, (g * h) % Q, (f * g) % Q, (e * h) % Q


def _ext_double(point: tuple[int, int, int, int]) -> tuple[int, int, int, int]:
    x1, y1, z1, _ = point
    a = (x1 * x1) % Q
    b = (y1 * y1) % Q
    c = (2 * z1 * z1) % Q
    h = a + b
    s = x1 + y1
    e = (h - s * s) % Q
    g = a - b
    f = c + g
    return (e * f) % Q, (g * h) % Q, (f * g) % Q, (e * h) % Q


def _scalarmult(point: tuple[int, int], scalar: int) -> tuple[int, int]:
    x, y = point[0] % Q, point[1] % Q
    result = (0, 1, 1, 0)
    current = (x, y, 1, (x * y) % Q)
    while scalar:
        if scalar & 1:
            result = _ext_add(result, current)
        scalar >>= 1
        if scalar:
            current = _ext_double(current)
    rx, ry, rz, _ = result
    inv_z = pow(rz, -1, Q)
    return (rx * inv_z) % Q, (ry * inv_z) % Q


def _encodepoint(point: tuple[int, int]) -> bytes:
    x, y = point
    encoded = bytearray(y.to_bytes(32, "little"))
    encoded[31] |= (x & 1) << 7
    return bytes(encoded)


def _decodepoint(encoded: bytes) -> tuple[int, int]:
    if len(encoded) != 32:
        fail("invalid Ed25519 point length")
    value = int.from_bytes(encoded, "little")
    sign = value >> 255
    y = value & ((1 << 255) - 1)
    if y >= Q:
        fail("invalid Ed25519 point encoding")
    x = _xrecover(y)
    if (x & 1) != sign:
        x = Q - x
    return x, y


def _decode_prime_order_point(encoded: bytes) -> tuple[int, int]:
    """Decode an Ed25519 point and require the prime-order subgroup."""

    decoded = _decodepoint(encoded)
    point = (decoded[0] % Q, decoded[1] % Q)
    if _encodepoint(point) != encoded:
        fail("Ed25519 point encoding is not canonical")
    if _scalarmult(point, L) != (0, 1):
        fail("Ed25519 point is not in the prime-order subgroup")
    if point == (0, 1):
        fail("Ed25519 point has small order")
    return point


def verify_ed25519(public_key: bytes, message: bytes, signature: bytes) -> bool:
    if len(public_key) != 32 or len(signature) != 64:
        return False
    try:
        public_point = _decode_prime_order_point(public_key)
        r_bytes = signature[:32]
        r_point = _decode_prime_order_point(r_bytes)
        scalar = int.from_bytes(signature[32:], "little")
        if scalar >= L:
            return False
        challenge = int.from_bytes(hashlib.sha512(r_bytes + public_key + message).digest(), "little") % L
        left = _encodepoint(_scalarmult(B, scalar))
        right = _encodepoint(_edwards(r_point, _scalarmult(public_point, challenge)))
        return left == right
    except ToolError:
        return False


def _manifest_check(manifest: dict[str, Any], payload_path: Path, registry_path: Path) -> None:
    require_string(manifest.get("schema_version"), "prepare-manifest.schema_version")
    if manifest["schema_version"] != "oasis7.identity_v2_prepare_manifest.v1":
        fail("prepare manifest schema is unsupported")
    require_hex(manifest.get("canonical_payload_sha256"), "prepare-manifest.canonical_payload_sha256")
    require_integer(manifest.get("payload_size_bytes"), "prepare-manifest.payload_size_bytes", positive=True)
    require_string(manifest.get("payload_path"), "prepare-manifest.payload_path")
    require_string(manifest.get("provider_registry_path"), "prepare-manifest.provider_registry_path")
    require_string(manifest.get("trust_config_path"), "prepare-manifest.trust_config_path")
    require_string(manifest.get("provider_ref"), "prepare-manifest.provider_ref", safe=True)
    require_string(manifest.get("network_id"), "prepare-manifest.network_id")
    exact = read_bytes(payload_path, "payload")
    if len(exact) != manifest["payload_size_bytes"] or sha256_bytes(exact) != manifest["canonical_payload_sha256"]:
        fail("payload bytes do not match prepare manifest")
    if manifest.get("payload_path") and not resolved_equal(manifest["payload_path"], payload_path):
        fail("payload path is not the path frozen by prepare")
    if manifest.get("provider_registry_path") and not resolved_equal(manifest["provider_registry_path"], registry_path):
        fail("provider registry path is not the path frozen by prepare")
    require_hex(manifest.get("provider_registry_sha256"), "prepare-manifest.provider_registry_sha256")
    if manifest["provider_registry_sha256"] != sha256_bytes(read_authority_bytes(registry_path, "provider-registry")):
        fail("provider registry bytes do not match prepare manifest")


def _provider_authentication_claims(attestation: dict[str, Any], payload: bytes, signature: bytes) -> dict[str, Any]:
    """Build the exact canonical claims covered by the provider proof."""

    return {
        "schema_version": PROVIDER_AUTHENTICATION_CLAIMS_SCHEMA_V1,
        "domain_separator": PROVIDER_AUTHENTICATION_DOMAIN_V1,
        "network_id": attestation["network_id"],
        "provider_id": attestation["provider_id"],
        "request_id": attestation["request_id"],
        "signer_id": attestation["signer_id"],
        "public_key_sha256": attestation["public_key_sha256"],
        "canonical_payload_sha256": sha256_bytes(payload),
        "signature_sha256": sha256_bytes(signature),
        "context_digest": attestation["context_digest"],
        "task_uid": attestation["task_uid"],
        "head_oid": attestation["head_oid"],
        "rotation_epoch": attestation["rotation_epoch"],
        "capture_window_id": attestation["capture_window_id"],
        "issued_at": attestation["issued_at"],
        "expires_at": attestation["expires_at"],
        "proof_ref": attestation["proof_ref"],
    }


def _provider_attestation(
    attestation: dict[str, Any],
    request: dict[str, Any],
    signature: bytes,
    payload: bytes,
    public_key: bytes,
) -> None:
    """Validate v2 proof shape, bindings, and the same-key auth signature."""

    require_exact_fields(attestation, ATTESTATION_FIELDS, "provider-attestation")
    if attestation.get("schema_version") != PROVIDER_ATTESTATION_SCHEMA_V2:
        fail("provider attestation schema is unsupported")
    for field in ("provider_id", "signer_id", "task_uid", "rotation_epoch", "capture_window_id"):
        require_string(attestation.get(field), f"provider-attestation.{field}", safe=True)
    require_string(attestation.get("network_id"), "provider-attestation.network_id")
    require_string(attestation.get("context_digest"), "provider-attestation.context_digest")
    require_string(attestation.get("request_id"), "provider-attestation.request_id")
    if REQUEST_ID.fullmatch(attestation["request_id"]) is None:
        fail("provider-attestation.request_id must be a v2 CSPRNG challenge")
    require_string(attestation.get("proof_ref"), "provider-attestation.proof_ref")
    if PROOF_REF.fullmatch(attestation["proof_ref"]) is None:
        fail("provider-attestation.proof_ref is not a bounded reference")
    if OID.fullmatch(require_string(attestation.get("head_oid"), "provider-attestation.head_oid")) is None:
        fail("provider-attestation.head_oid must be a lowercase commit OID")
    parse_timestamp(attestation.get("issued_at"), "provider-attestation.issued_at")
    parse_timestamp(attestation.get("expires_at"), "provider-attestation.expires_at")
    if parse_timestamp(attestation["issued_at"], "provider-attestation.issued_at") >= parse_timestamp(
        attestation["expires_at"], "provider-attestation.expires_at"
    ):
        fail("provider-attestation freshness is inverted")
    require_hex(attestation.get("public_key_sha256"), "provider-attestation.public_key_sha256")
    require_hex(attestation.get("canonical_payload_sha256"), "provider-attestation.canonical_payload_sha256")
    require_hex(attestation.get("signature_sha256"), "provider-attestation.signature_sha256")
    if attestation.get("algorithm") != ALGORITHM:
        fail("provider attestation algorithm is unsupported")
    if len(public_key) != 32 or sha256_bytes(public_key) != attestation["public_key_sha256"]:
        fail("provider attestation public key binding mismatch")
    for field in (
        "network_id", "provider_id", "request_id", "signer_id", "public_key_sha256",
        "context_digest", "task_uid", "head_oid", "rotation_epoch", "capture_window_id",
        "issued_at", "expires_at",
    ):
        if attestation[field] != request[field]:
            fail("provider attestation context mismatch")
    if attestation["network_id"] != CANONICAL_NETWORK_ID:
        fail("provider attestation network is not the governed deployment network")
    if attestation["canonical_payload_sha256"] != sha256_bytes(payload) or attestation["signature_sha256"] != sha256_bytes(signature):
        fail("provider attestation digest mismatch")
    proof = attestation.get("proof")
    if not isinstance(proof, dict):
        fail("provider attestation proof must be an object")
    require_exact_fields(proof, PROOF_FIELDS, "provider-attestation.proof")
    if proof.get("schema_version") != PROVIDER_AUTHENTICATION_PROOF_SCHEMA_V1 or proof.get("algorithm") != ALGORITHM:
        fail("provider attestation proof schema or algorithm is unsupported")
    require_hex(proof.get("claims_sha256"), "provider-attestation.proof.claims_sha256")
    require_hex(proof.get("signature_hex"), "provider-attestation.proof.signature_hex", re.compile(r"^[0-9a-f]{128}$"))
    claims = _provider_authentication_claims(attestation, payload, signature)
    require_exact_fields(claims, AUTHENTICATION_CLAIMS_FIELDS, "provider-authentication-claims")
    claims_bytes = canonical(claims)
    if proof["claims_sha256"] != sha256_bytes(claims_bytes):
        fail("provider attestation proof claims digest mismatch")
    if not verify_ed25519(public_key, PROVIDER_AUTHENTICATION_PREFIX + claims_bytes, bytes.fromhex(proof["signature_hex"])):
        fail("provider attestation authentication proof signature is invalid")


def command_prepare(args: argparse.Namespace) -> None:
    outputs = [(args.payload_out, "payload output"), (args.manifest_out, "prepare manifest output")]
    protected = [
        (args.raw_v1, "raw-v1"),
        (args.template, "template"),
        (args.context, "context"),
        (args.plan_intent, "plan-intent"),
        (args.trust_config, "trust-config"),
        (args.provider_registry, "provider-registry"),
    ]
    _reject_output_aliases(outputs, protected)
    _clear_derived_output(args.payload_out, "payload output")
    _clear_derived_output(args.manifest_out, "prepare manifest output")
    raw_bytes = read_bytes(args.raw_v1, "raw-v1")
    raw = validate_raw(raw_bytes)
    context = validate_context(read_json(args.context, "context"))
    intent = validate_intent(read_json(args.plan_intent, "plan-intent"), context)
    trust_path = _regular(args.trust_config, "trust-config")
    registry_path = _regular(args.provider_registry, "provider-registry")
    registry_value = read_json(registry_path, "provider-registry", authority=True)
    _reject_output_aliases(outputs, protected + [(path, "registry authority artifact") for path in _registry_artifact_paths(registry_value)])
    trust, registry = validate_registry(registry_value, registry_path, trust_path)
    _authority_scope(trust_path, registry_path, registry)
    template = validate_template(read_json(args.template, "template"), raw, context, sha256_bytes(canonical(intent)), trust)
    if template["signed_payload_sha256"] != sha256_bytes(raw_bytes):
        fail("template raw-v1 digest mismatch")
    payload = payload_bytes(template)
    matching_providers = [provider for provider in registry["providers"] if provider["signer_id"] == template["signer_id"]]
    if len(matching_providers) != 1:
        fail("template signer has no unique provider registry binding")
    matching_provider = matching_providers[0]
    manifest = {
        "schema_version": "oasis7.identity_v2_prepare_manifest.v1",
        "canonical_payload_sha256": sha256_bytes(payload),
        "payload_size_bytes": len(payload),
        "payload_path": str(Path(args.payload_out).resolve()),
        "raw_v1_sha256": sha256_bytes(raw_bytes),
        "raw_v1_size_bytes": len(raw_bytes),
        "template_sha256": sha256_bytes(read_bytes(args.template, "template")),
        "context_digest": sha256_bytes(canonical(context)),
        "plan_digest": sha256_bytes(canonical(intent)),
        "task_uid": template["task_uid"],
        "head_oid": template["head_oid"],
        "frozen_head_oid": template["frozen_head_oid"],
        "node_id": template["node_id"],
        "peer_id": template["peer_id"],
        "capture_window_id": template["capture_window_id"],
        "rotation_epoch": template["rotation_epoch"],
        "issued_at": template["issued_at"],
        "expires_at": template["expires_at"],
        "signer_id": template["signer_id"],
        "verifier_id": template["verifier_id"],
        "trust_root_id": template["trust_root_id"],
        "algorithm": ALGORITHM,
        "trust_config_path": str(trust_path.resolve()),
        "trust_config_sha256": sha256_bytes(read_authority_bytes(trust_path, "trust-config")),
        "provider_registry_path": str(registry_path.resolve()),
        "provider_registry_sha256": sha256_bytes(read_authority_bytes(registry_path, "provider-registry")),
        "provider_ref": matching_provider["provider_id"],
        "public_key_sha256": matching_provider["public_key_sha256"],
        "verifier_executable_sha256": registry["verifier"]["executable_sha256"],
        "network_id": template["network_id"],
    }
    _publish_verification_pair((
        (Path(args.payload_out), payload),
        (Path(args.manifest_out), canonical(manifest)),
    ))


def command_sign(args: argparse.Namespace) -> None:
    outputs = [(args.signature_out, "signature output"), (args.attestation_out, "provider attestation output")]
    protected = [
        (args.payload, "payload"),
        (args.manifest, "prepare manifest"),
        (args.provider_registry, "provider-registry"),
    ]
    _reject_output_aliases(outputs, protected)
    _clear_derived_output(args.signature_out, "signature output")
    _clear_derived_output(args.attestation_out, "attestation output")
    payload_path = _regular(args.payload, "payload")
    manifest_path = _regular(args.manifest, "prepare manifest")
    manifest = read_json(manifest_path, "prepare manifest")
    registry_path = _regular(args.provider_registry, "provider-registry")
    registry_value = read_json(registry_path, "provider-registry", authority=True)
    manifest_trust_path = manifest.get("trust_config_path")
    if isinstance(manifest_trust_path, str):
        protected.append((manifest_trust_path, "trust-config"))
    _reject_output_aliases(outputs, protected + [(path, "registry authority artifact") for path in _registry_artifact_paths(registry_value)])
    _manifest_check(manifest, payload_path, registry_path)
    trust_path = _regular(manifest["trust_config_path"], "trust-config")
    _, registry = validate_registry(registry_value, registry_path, trust_path)
    _authority_scope(trust_path, registry_path, registry)
    provider = find_provider(registry, args.provider_ref)
    trust_digest_before = authority_digest(trust_path, "trust-config")
    registry_digest_before = authority_digest(registry_path, "provider-registry")
    provider_digest_before = authority_digest(provider["adapter_path"], "provider custody adapter", executable=True)
    payload = read_bytes(payload_path, "payload")
    request = {
        "network_id": manifest["network_id"],
        "provider_id": provider["provider_id"],
        "request_id": "req-v2:" + secrets.token_hex(32),
        "signer_id": provider["signer_id"],
        "public_key_sha256": provider["public_key_sha256"],
        "payload_path": str(payload_path.resolve()),
        "canonical_payload_sha256": sha256_bytes(payload),
        "context_digest": manifest["context_digest"],
        "task_uid": manifest["task_uid"],
        "head_oid": manifest["head_oid"],
        "rotation_epoch": manifest["rotation_epoch"],
        "capture_window_id": manifest["capture_window_id"],
        "issued_at": manifest.get("issued_at", ""),
        "expires_at": manifest.get("expires_at", ""),
    }
    with tempfile.TemporaryDirectory(prefix="oasis7-identity-v2-provider-") as directory:
        temp = Path(directory)
        request_path = temp / "request.json"
        raw_signature_path = temp / "signature.raw"
        attestation_path = temp / "attestation.json"
        _atomic_write(request_path, canonical(request), "provider request")
        environment = {"PATH": os.environ.get("PATH", ""), "PYTHONIOENCODING": "utf-8"}
        try:
            completed = subprocess.run(
                [provider["adapter_path"], "--request", str(request_path), "--signature-out", str(raw_signature_path), "--attestation-out", str(attestation_path)],
                capture_output=True,
                env=environment,
                cwd=str(temp),
                timeout=30,
            )
        except (OSError, subprocess.TimeoutExpired):
            fail("provider custody adapter failed")
        require_authority_unchanged(trust_path, "trust-config", trust_digest_before)
        require_authority_unchanged(registry_path, "provider-registry", registry_digest_before)
        require_authority_unchanged(
            provider["adapter_path"],
            "provider custody adapter",
            provider_digest_before,
            executable=True,
        )
        if completed.returncode != 0:
            fail("provider custody adapter failed")
        signature = read_bytes(raw_signature_path, "provider signature")
        if len(signature) != 64:
            fail("provider signature must be exactly 64 bytes")
        attestation = read_json(attestation_path, "provider attestation")
        public_key = read_authority_bytes(provider["public_key_ref"], "provider public key")
        _provider_attestation(attestation, request, signature, payload, public_key)
        if not verify_ed25519(public_key, payload, signature):
            fail("provider signature failed independent Ed25519 verification")
        _publish_verification_pair((
            (Path(args.signature_out), signature.hex().encode("ascii")),
            (Path(args.attestation_out), canonical(attestation)),
        ))


def _payload_from_file(payload_path: Path, manifest: dict[str, Any]) -> dict[str, Any]:
    payload = read_bytes(payload_path, "payload")
    _manifest_check(manifest, payload_path, Path(manifest["provider_registry_path"]))
    if not payload.startswith(PREFIX):
        fail("payload domain prefix mismatch")
    try:
        value = json.loads(payload[len(PREFIX) :].decode("utf-8"), object_pairs_hook=_object_pairs, parse_constant=_reject_constant)
    except (UnicodeError, json.JSONDecodeError) as error:
        fail(f"payload body is not canonical JSON: {error.__class__.__name__}")
    if not isinstance(value, dict) or canonical(value) != payload[len(PREFIX) :]:
        fail("payload body is not canonical JSON")
    require_exact_fields(value, PAYLOAD_FIELDS, "payload")
    if sha256_bytes(payload) != manifest["canonical_payload_sha256"]:
        fail("payload digest mismatch")
    if value.get("network_id") != manifest.get("network_id") or value.get("network_id") != CANONICAL_NETWORK_ID:
        fail("payload network binding mismatch")
    return value


def command_assemble(args: argparse.Namespace) -> None:
    outputs = [(args.out, "envelope output")]
    protected = [
        (args.payload, "payload"),
        (args.manifest, "prepare manifest"),
        (args.signature, "signature"),
        (args.attestation, "provider attestation"),
        (args.provider_registry, "provider-registry"),
    ]
    _reject_output_aliases(outputs, protected)
    _clear_derived_output(args.out, "envelope output")
    payload_path = _regular(args.payload, "payload")
    manifest = read_json(args.manifest, "prepare manifest")
    registry_path = _regular(args.provider_registry, "provider-registry")
    registry_value = read_json(registry_path, "provider-registry", authority=True)
    trust_value = manifest.get("trust_config_path")
    if isinstance(trust_value, str):
        protected.append((trust_value, "trust-config"))
    _reject_output_aliases(outputs, protected + [(path, "registry authority artifact") for path in _registry_artifact_paths(registry_value)])
    _manifest_check(manifest, payload_path, registry_path)
    trust_path = _regular(manifest["trust_config_path"], "trust-config")
    trust, registry = validate_registry(registry_value, registry_path, trust_path)
    authority_scope = _authority_scope(trust_path, registry_path, registry)
    payload = read_bytes(payload_path, "payload")
    fields = _payload_from_file(payload_path, manifest)
    signature_text = read_bytes(args.signature, "signature").decode("ascii")
    if not re.fullmatch(r"[0-9a-f]{128}", signature_text):
        fail("signature must be lowercase 128-hex detached Ed25519 bytes")
    signature = bytes.fromhex(signature_text)
    attestation = read_json(args.attestation, "provider attestation")
    provider = find_provider(registry, attestation.get("provider_id"))
    if manifest.get("provider_ref") != provider["provider_id"]:
        fail("provider attestation does not match the pinned prepare provider")
    request = {
        "network_id": manifest["network_id"],
        "provider_id": provider["provider_id"],
        "request_id": attestation.get("request_id"),
        "signer_id": provider["signer_id"],
        "public_key_sha256": provider["public_key_sha256"],
        "context_digest": manifest["context_digest"],
        "task_uid": manifest["task_uid"],
        "head_oid": manifest["head_oid"],
        "rotation_epoch": manifest["rotation_epoch"],
        "capture_window_id": manifest["capture_window_id"],
        "issued_at": manifest.get("issued_at", ""),
        "expires_at": manifest.get("expires_at", ""),
    }
    public_key = read_authority_bytes(provider["public_key_ref"], "provider public key")
    _provider_attestation(attestation, request, signature, payload, public_key)
    if not verify_ed25519(public_key, payload, signature):
        fail("detached signature failed independent Ed25519 verification")
    envelope = dict(fields)
    envelope["signature_hex"] = signature_text
    envelope["canonical_digest"] = sha256_bytes(canonical(envelope))
    envelope["authenticated"] = False
    envelope["verified"] = False
    _atomic_write(args.out, canonical(envelope), "unsigned identity envelope")


def validate_envelope(value: dict[str, Any]) -> dict[str, Any]:
    allowed = set(PAYLOAD_FIELDS) | {"signature_hex", "canonical_digest", "authenticated", "verified", "historical_only", "apply_authorized"}
    if not set(value) <= allowed or not PAYLOAD_FIELDS <= set(value):
        fail("identity envelope fields are not exact")
    require_hex(value.get("canonical_digest"), "envelope.canonical_digest")
    signature = value.get("signature_hex")
    if not isinstance(signature, str) or re.fullmatch(r"[0-9a-f]{128}", signature) is None:
        fail("envelope.signature_hex must be lowercase 128-hex")
    if not isinstance(value.get("authenticated"), bool) or not isinstance(value.get("verified"), bool):
        fail("envelope verdict fields must be booleans")
    for field in ("historical_only", "apply_authorized"):
        if field in value and not isinstance(value[field], bool):
            fail(f"envelope.{field} must be boolean")
    return value


def _validate_bindings(
    envelope: dict[str, Any], raw: dict[str, Any], raw_bytes: bytes, context: dict[str, Any], intent: dict[str, Any], trust: dict[str, Any]
) -> tuple[dict[str, Any], bytes, dict[str, Any]]:
    if envelope.get("domain_separator") != DOMAIN or envelope.get("schema_version") != PAYLOAD_SCHEMA:
        fail("envelope domain or schema is unsupported")
    if envelope["head_oid"] != envelope["frozen_head_oid"] or envelope["head_oid"] != context["head_oid"]:
        fail("envelope head binding mismatch")
    if envelope["task_uid"] != context["task_uid"] or envelope["capture_window_id"] != context["capture_window_id"] or envelope["rotation_epoch"] != context["rotation_epoch"]:
        fail("envelope context binding mismatch")
    if envelope["network_id"] != context["network_id"] or envelope["network_id"] != trust["network_id"]:
        fail("envelope network binding mismatch")
    if envelope["issued_at"] != context["issued_at"] or envelope["expires_at"] != context["expires_at"]:
        fail("envelope freshness binding mismatch")
    issued = parse_timestamp(envelope["issued_at"], "envelope.issued_at")
    expires = parse_timestamp(envelope["expires_at"], "envelope.expires_at")
    if not parse_timestamp(context["capture_start"], "context.capture_start") <= issued < expires <= parse_timestamp(context["capture_end"], "context.capture_end"):
        fail("envelope freshness is outside capture window")
    plan_digest = sha256_bytes(canonical(intent))
    if envelope["plan_digest"] != plan_digest or envelope["context_digest"] != sha256_bytes(canonical(context)):
        fail("envelope plan/context digest mismatch")
    if envelope["signed_payload_sha256"] != sha256_bytes(raw_bytes):
        fail("envelope raw-v1 digest mismatch")
    if envelope["node_id"] != raw["node_id"] or envelope["peer_id"] != raw["peer_id"]:
        fail("envelope node/peer binding mismatch")
    for field in ("key_sha256", "key_size_bytes", "key_uid", "key_gid"):
        if envelope[field] != raw[field]:
            fail(f"envelope {field} binding mismatch")
    if envelope["key_mode"] != "0600" or raw["key_mode"] != 0o600:
        fail("envelope key mode binding mismatch")
    signer_matches = [entry for entry in trust["allowlist"] if entry["signer_id"] == envelope["signer_id"]]
    if len(signer_matches) != 1:
        fail("envelope signer is not allowlisted")
    signed = {key: envelope[key] for key in PAYLOAD_FIELDS}
    exact_payload = payload_bytes(signed)
    if envelope["canonical_digest"] != sha256_bytes(canonical({**signed, "signature_hex": envelope["signature_hex"]})):
        fail("envelope canonical digest mismatch")
    return signer_matches[0], exact_payload, signed


def _verifier_command(
    verifier_path: Path, args: argparse.Namespace, out: Path, verification_out: Path
) -> list[str]:
    """Build the fixed file-oriented command for the registry verifier."""
    return [
        str(verifier_path),
        "verify",
        "--mode",
        args.mode,
        "--envelope",
        str(Path(args.envelope).resolve()),
        "--attestation",
        str(Path(args.attestation).resolve()),
        "--raw-v1",
        str(Path(args.raw_v1).resolve()),
        "--context",
        str(Path(args.context).resolve()),
        "--plan-intent",
        str(Path(args.plan_intent).resolve()),
        "--trust-config",
        str(Path(args.trust_config).resolve()),
        "--provider-registry",
        str(Path(args.provider_registry).resolve()),
        "--out",
        str(out),
        "--verification-out",
        str(verification_out),
    ]


def _invoke_registry_verifier(
    args: argparse.Namespace, verifier_path: Path
) -> tuple[dict[str, Any], dict[str, Any]]:
    """Run the pinned verifier and return its canonical output pair."""
    with tempfile.TemporaryDirectory(prefix="oasis7-identity-v2-verifier-") as directory:
        temporary_root = Path(directory)
        verified_path = temporary_root / "verified-envelope.json"
        receipt_path = temporary_root / "verification.json"
        environment = {"PATH": os.environ.get("PATH", ""), "PYTHONIOENCODING": "utf-8"}
        trust_digest_before = authority_digest(args.trust_config, "trust-config")
        registry_digest_before = authority_digest(args.provider_registry, "provider-registry")
        verifier_digest_before = authority_digest(
            verifier_path,
            "registry-selected verifier",
            executable=True,
        )
        try:
            completed = subprocess.run(
                _verifier_command(verifier_path, args, verified_path, receipt_path),
                capture_output=True,
                env=environment,
                cwd=str(temporary_root),
                timeout=30,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            fail(f"registry-selected verifier failed: {error.__class__.__name__}")
        require_authority_unchanged(args.trust_config, "trust-config", trust_digest_before)
        require_authority_unchanged(args.provider_registry, "provider-registry", registry_digest_before)
        require_authority_unchanged(
            verifier_path,
            "registry-selected verifier",
            verifier_digest_before,
            executable=True,
        )
        if completed.returncode != 0:
            fail("registry-selected verifier failed")
        return (
            read_json(verified_path, "registry verifier output"),
            read_json(receipt_path, "registry verifier receipt"),
        )


def _assert_registry_verification(
    expected_output: dict[str, Any],
    expected_receipt: dict[str, Any],
    actual_output: dict[str, Any],
    actual_receipt: dict[str, Any],
) -> None:
    """Require the independent receipt to describe this exact verification."""
    if actual_output != expected_output:
        fail("registry verifier output does not match independently recomputed bindings")
    if set(actual_receipt) != set(expected_receipt):
        fail("registry verifier receipt fields are not exact")
    for field, expected in expected_receipt.items():
        if field != "evaluation_time" and actual_receipt.get(field) != expected:
            fail(f"registry verifier receipt binding mismatch: {field}")
    evaluated_at = parse_timestamp(actual_receipt.get("evaluation_time"), "registry verifier evaluation_time")
    now = datetime.now(timezone.utc)
    if evaluated_at > now + timedelta(seconds=MAX_CLOCK_SKEW_SECONDS) or evaluated_at < now - timedelta(seconds=30):
        fail("registry verifier receipt is not fresh")


def command_verify(args: argparse.Namespace) -> None:
    if args.mode not in {"current_admission", "historical_audit"}:
        fail("verification mode is unsupported")
    historical_only = args.mode == "historical_audit"
    outputs = [(args.out, "verified-envelope output"), (args.verification_out, "verification receipt output")]
    protected = [
        (args.envelope, "identity envelope"),
        (args.raw_v1, "raw-v1"),
        (args.context, "context"),
        (args.plan_intent, "plan-intent"),
        (args.attestation, "provider attestation"),
        (args.trust_config, "trust-config"),
        (args.provider_registry, "provider-registry"),
    ]
    _reject_output_aliases(outputs, protected)
    _clear_derived_output(args.out, "verified-envelope output")
    _clear_derived_output(args.verification_out, "verification receipt output")
    envelope_path = _regular(args.envelope, "identity envelope")
    envelope = validate_envelope(read_json(envelope_path, "identity envelope"))
    raw_bytes = read_bytes(args.raw_v1, "raw-v1")
    raw = validate_raw(raw_bytes)
    context = validate_context(read_json(args.context, "context"), historical_audit=historical_only)
    intent = validate_intent(read_json(args.plan_intent, "plan-intent"), context)
    attestation = read_json(args.attestation, "provider attestation")
    trust_path = _regular(args.trust_config, "trust-config")
    registry_path = _regular(args.provider_registry, "provider-registry")
    registry_value = read_json(registry_path, "provider-registry", authority=True)
    _reject_output_aliases(outputs, protected + [(path, "registry authority artifact") for path in _registry_artifact_paths(registry_value)])
    trust, registry = validate_registry(registry_value, registry_path, trust_path)
    verifier_path = _regular(registry["verifier"]["executable_path"], "provider-registry.verifier.executable_path").resolve()
    authority_scope = _authority_scope(trust_path, registry_path, registry)
    signer, exact_payload, signed = _validate_bindings(envelope, raw, raw_bytes, context, intent, trust)
    provider = [entry for entry in registry["providers"] if entry["signer_id"] == signer["signer_id"]]
    if len(provider) != 1 or provider[0]["public_key_sha256"] != signer["public_key_sha256"]:
        fail("envelope signer has no matching pinned provider key")
    public_key = read_authority_bytes(provider[0]["public_key_ref"], "provider public key")
    signature = bytes.fromhex(envelope["signature_hex"])
    if not verify_ed25519(public_key, exact_payload, signature):
        fail("identity envelope signature is invalid")
    if attestation.get("provider_id") != provider[0]["provider_id"]:
        fail("provider attestation is not bound to the envelope signer")
    request = {
        "network_id": envelope["network_id"],
        "provider_id": provider[0]["provider_id"],
        "request_id": attestation.get("request_id"),
        "signer_id": envelope["signer_id"],
        "public_key_sha256": provider[0]["public_key_sha256"],
        "context_digest": envelope["context_digest"],
        "task_uid": envelope["task_uid"],
        "head_oid": envelope["head_oid"],
        "rotation_epoch": envelope["rotation_epoch"],
        "capture_window_id": envelope["capture_window_id"],
        "issued_at": envelope["issued_at"],
        "expires_at": envelope["expires_at"],
    }
    _provider_attestation(attestation, request, signature, exact_payload, public_key)
    issued = parse_timestamp(envelope["issued_at"], "envelope.issued_at")
    expires = parse_timestamp(envelope["expires_at"], "envelope.expires_at")
    valid_from = parse_timestamp(signer["valid_from"], "trust allowlist.valid_from")
    valid_until = parse_timestamp(signer["valid_until"], "trust allowlist.valid_until")
    if not valid_from <= issued < valid_until:
        fail("signer validity interval does not cover receipt issuance")
    capture_start = parse_timestamp(context["capture_start"], "context.capture_start")
    capture_end = parse_timestamp(context["capture_end"], "context.capture_end")
    now = datetime.now(timezone.utc)
    if (not historical_only and expires <= now) or issued >= expires or issued < capture_start or expires > capture_end:
        fail("identity envelope freshness is stale or inverted")
    current_status = signer["status"]
    if not historical_only and current_status != "active":
        fail("current admission rejects retired or revoked signer")
    if not historical_only and envelope["rotation_epoch"] != trust["rotation_epoch"]:
        fail("current admission rejects an envelope outside the pinned trust rotation epoch")
    if not historical_only:
        for revocation in trust["revocations"]:
            if revocation["signer_id"] == signer["signer_id"] and parse_timestamp(revocation["effective_at"], "revocation.effective_at") <= now:
                fail("current admission rejects a currently revoked signer")
    if historical_only:
        for revocation in trust["revocations"]:
            if revocation["signer_id"] == signer["signer_id"] and parse_timestamp(revocation["effective_at"], "revocation.effective_at") <= issued:
                fail("historical signer was revoked before issuance")
    envelope_sha = sha256_bytes(read_bytes(envelope_path, "identity envelope"))
    output = dict(signed)
    output["signature_hex"] = envelope["signature_hex"]
    output["canonical_digest"] = envelope["canonical_digest"]
    output["authenticated"] = True
    output["verified"] = True
    output["historical_only"] = historical_only
    output["apply_authorized"] = not historical_only
    receipt = {
        "schema_version": "oasis7.identity_v2_verification_receipt.v1",
        "mode": args.mode,
        "evaluation_time": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "raw_v1_sha256": sha256_bytes(raw_bytes),
        "canonical_payload_sha256": sha256_bytes(exact_payload),
        "envelope_sha256": envelope_sha,
        "signer_id": signer["signer_id"],
        "public_key_sha256": signer["public_key_sha256"],
        "trust_config_sha256": sha256_bytes(read_authority_bytes(trust_path, "trust-config")),
        "provider_registry_sha256": sha256_bytes(read_authority_bytes(registry_path, "provider-registry")),
        "verifier_executable_sha256": registry["verifier"]["executable_sha256"],
        "network_id": envelope["network_id"],
        "proof_ref": attestation["proof_ref"],
        "proof_claims_sha256": attestation["proof"]["claims_sha256"],
        "task_uid": envelope["task_uid"],
        "head_oid": envelope["head_oid"],
        "node_id": envelope["node_id"],
        "peer_id": envelope["peer_id"],
        "capture_window_id": envelope["capture_window_id"],
        "rotation_epoch": envelope["rotation_epoch"],
        "historical_only": historical_only,
        "apply_authorized": not historical_only,
        "authority_scope": authority_scope,
        "verified": True,
    }
    external_output, external_receipt = _invoke_registry_verifier(args, verifier_path)
    _assert_registry_verification(output, receipt, external_output, external_receipt)
    _publish_verification_pair((
        (Path(args.out), canonical(external_output)),
        (Path(args.verification_out), canonical(external_receipt)),
    ))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare")
    for option, dest in (("--raw-v1", "raw_v1"), ("--template", "template"), ("--context", "context"), ("--plan-intent", "plan_intent"), ("--trust-config", "trust_config"), ("--provider-registry", "provider_registry"), ("--payload-out", "payload_out"), ("--manifest-out", "manifest_out")):
        prepare.add_argument(option, dest=dest, required=True)
    prepare.set_defaults(handler=command_prepare)
    sign = commands.add_parser("sign")
    for option, dest in (("--payload", "payload"), ("--manifest", "manifest"), ("--provider-registry", "provider_registry"), ("--provider-ref", "provider_ref"), ("--signature-out", "signature_out"), ("--attestation-out", "attestation_out")):
        sign.add_argument(option, dest=dest, required=True)
    sign.set_defaults(handler=command_sign)
    assemble = commands.add_parser("assemble")
    for option, dest in (("--payload", "payload"), ("--manifest", "manifest"), ("--signature", "signature"), ("--attestation", "attestation"), ("--provider-registry", "provider_registry"), ("--out", "out")):
        assemble.add_argument(option, dest=dest, required=True)
    assemble.set_defaults(handler=command_assemble)
    verify = commands.add_parser("verify")
    verify.add_argument("--mode", required=True, choices=("current_admission", "historical_audit"))
    for option, dest in (("--envelope", "envelope"), ("--attestation", "attestation"), ("--raw-v1", "raw_v1"), ("--context", "context"), ("--plan-intent", "plan_intent"), ("--trust-config", "trust_config"), ("--provider-registry", "provider_registry"), ("--out", "out"), ("--verification-out", "verification_out")):
        verify.add_argument(option, dest=dest, required=True)
    verify.set_defaults(handler=command_verify)
    return root


def main(argv: list[str] | None = None) -> int:
    try:
        arguments = parser().parse_args(argv)
        arguments.handler(arguments)
        return 0
    except ToolError as error:
        print(f"error: identity-v2 signing tool: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
