#!/usr/bin/env python3
"""RED contract tests for the proposed identity-receipt-v2 executable.

These tests intentionally target the not-yet-present
``p2p-public-testnet-identity-v2-signing-tool.py``.  All authority material is
created inside a temporary directory.  The provider fixture keeps its private
key in that temporary custody boundary and signs with the host's OpenSSL 3
Ed25519 implementation; it is never a deployment provider or a readiness
fixture.

The test-only provider protocol is deliberately narrow: the executable must
resolve ``--provider-ref`` through the pinned registry, then invoke the pinned
adapter with a request JSON containing the payload path and signed context.
There is no caller-selected command, endpoint, private-key option, or
``verified=true`` shortcut.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "scripts" / "p2p-public-testnet-identity-v2-signing-tool.py"


def _supports_ed25519(path: Path) -> bool:
    """Check that a candidate can run the real Ed25519 fixture operations."""
    result = subprocess.run(
        [str(path), "genpkey", "-algorithm", "ED25519", "-out", os.devnull],
        capture_output=True,
    )
    return result.returncode == 0


def resolve_openssl() -> Path:
    """Resolve an executable OpenSSL implementation with Ed25519 support."""
    candidates: list[Path] = []
    discovered = shutil.which("openssl")
    if discovered is not None:
        candidates.append(Path(discovered))
    candidates.extend(
        Path(candidate)
        for candidate in (
            "/opt/homebrew/bin/openssl",
            "/usr/local/bin/openssl",
            "/usr/bin/openssl",
        )
    )
    seen: set[Path] = set()
    for path in candidates:
        resolved = path.resolve()
        if resolved in seen:
            continue
        seen.add(resolved)
        if resolved.is_file() and os.access(resolved, os.X_OK) and _supports_ed25519(resolved):
            return resolved
    raise RuntimeError("test prerequisite missing: no executable OpenSSL with Ed25519 support")


OPENSSL = resolve_openssl()
PREFIX = b"OASIS7-IDENTITY-RECEIPT-V2\0"
AUTH_PREFIX = b"OASIS7-IDENTITY-V2-PROVIDER-AUTH\0"
NETWORK_ID = "oasis7-public-testnet-governed-20260606"
TRUST_ROOT_ID = "oasis7-public-testnet-governance-root-v1"
VERIFIER_ID = "governed-receipt-verifier"
SIGNER_ID = "identity-v2-ephemeral-test-signer"
PROVIDER_ID = "ephemeral-test-custody"
ROTATION_EPOCH = "identity-v2-rotation-0001"
TASK_UID = "task_174f0a5a87394012b071171cc4a52372"
NODE_ID = "triad-testnet-sequencer"
PEER_ID = "12D3KooWIdentityV2TestPeer"

# Test-only child process loader. It imports the real tool and adapter, then
# redirects only their authority constants to temporary files. The production
# CLI is still exercised through its unchanged ``main(argv)`` entrypoint, and
# no production environment variable or command-line bypass is introduced.
CHILD_HARNESS = r'''
import importlib.util
import os
import sys
from pathlib import Path

tool_path = Path(sys.argv[1])
governance_root = Path(sys.argv[2])
trust_config = Path(sys.argv[3])
provider_registry = Path(sys.argv[4])
trusted_config_sha256 = sys.argv[5]
trusted_registry_sha256 = sys.argv[6]
adapter_path = tool_path.with_name("p2p-public-testnet-full-network-clean-room-adapter.py")


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise SystemExit("cannot load test harness module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


adapter = load("oasis7_identity_v2_adapter", adapter_path)
adapter.CANONICAL_TRUST_ROOT_PATH = str(governance_root)
adapter.CANONICAL_TRUST_ROOT_FILE_SHA256 = "f278bc8f060cd6777d68f086fc3131edc5d6b5a6080bde09208ba69a69e3ef66"
adapter.CANONICAL_TRUST_ROOT_OWNER_UID = os.getuid()
adapter.CANONICAL_TRUST_ROOT_MODE = "0600"

tool = load("oasis7_identity_v2_signing_tool", tool_path)
tool.DEPLOYED_TRUST_CONFIG = trust_config
tool.DEPLOYED_PROVIDER_REGISTRY = provider_registry
tool.PINNED_TRUST_CONFIG_SHA256 = trusted_config_sha256
tool.PINNED_PROVIDER_REGISTRY_SHA256 = trusted_registry_sha256
tool.DEPLOYED_GOVERNANCE_ROOT = governance_root
raise SystemExit(tool.main(sys.argv[7:]))
'''


def canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    )


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def digest_file(path: Path) -> str:
    return digest_bytes(path.read_bytes())


def write_json(path: Path, value: Any) -> None:
    path.write_bytes(canonical(value))


def provider_source(private_key: Path) -> str:
    """Return an isolated custody adapter using real OpenSSL Ed25519."""

    return f'''#!/usr/bin/env python3
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

OPENSSL = {str(OPENSSL)!r}
PRIVATE_KEY = {str(private_key)!r}
AUTH_PREFIX = b"OASIS7-IDENTITY-V2-PROVIDER-AUTH\\0"

def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")

parser = argparse.ArgumentParser()
parser.add_argument("--request", required=True)
parser.add_argument("--signature-out", required=True)
parser.add_argument("--attestation-out", required=True)
args = parser.parse_args()
request = json.loads(Path(args.request).read_text(encoding="utf-8"))
payload_path = Path(request["payload_path"])
payload = payload_path.read_bytes()
signature_path = Path(args.signature_out)
subprocess.run(
    [OPENSSL, "pkeyutl", "-sign", "-inkey", PRIVATE_KEY, "-rawin", "-in", str(payload_path), "-out", str(signature_path)],
    check=True,
)
signature = signature_path.read_bytes()
if len(signature) != 64:
    raise SystemExit("test provider produced a non-Ed25519 signature")
proof_ref = "proof-v1:" + hashlib.sha256(("provider-proof:" + request["request_id"]).encode()).hexdigest()
claims = {{
    "schema_version": "oasis7.identity_v2_provider_authentication_claims.v1",
    "domain_separator": "oasis7.identity-v2-provider-authentication/v1",
    "network_id": request["network_id"],
    "provider_id": request["provider_id"],
    "request_id": request["request_id"],
    "signer_id": request["signer_id"],
    "public_key_sha256": request["public_key_sha256"],
    "canonical_payload_sha256": hashlib.sha256(payload).hexdigest(),
    "signature_sha256": hashlib.sha256(signature).hexdigest(),
    "context_digest": request["context_digest"],
    "task_uid": request["task_uid"],
    "head_oid": request["head_oid"],
    "rotation_epoch": request["rotation_epoch"],
    "capture_window_id": request["capture_window_id"],
    "issued_at": request["issued_at"],
    "expires_at": request["expires_at"],
    "proof_ref": proof_ref,
}}
claims_path = signature_path.with_name("provider-auth-claims.bin")
claims_path.write_bytes(AUTH_PREFIX + canonical(claims))
proof_signature_path = signature_path.with_name("provider-auth-signature.raw")
subprocess.run(
    [OPENSSL, "pkeyutl", "-sign", "-inkey", PRIVATE_KEY, "-rawin", "-in", str(claims_path), "-out", str(proof_signature_path)],
    check=True,
)
proof_signature = proof_signature_path.read_bytes()
if len(proof_signature) != 64:
    raise SystemExit("test provider produced a non-Ed25519 authentication signature")
attestation = {{
    "schema_version": "oasis7.identity_v2_provider_attestation.v2",
    "network_id": request["network_id"],
    "provider_id": request["provider_id"],
    "request_id": request["request_id"],
    "signer_id": request["signer_id"],
    "public_key_sha256": request["public_key_sha256"],
    "algorithm": "ed25519",
    "canonical_payload_sha256": hashlib.sha256(payload).hexdigest(),
    "signature_sha256": hashlib.sha256(signature).hexdigest(),
    "context_digest": request["context_digest"],
    "task_uid": request["task_uid"],
    "head_oid": request["head_oid"],
    "rotation_epoch": request["rotation_epoch"],
    "capture_window_id": request["capture_window_id"],
    "issued_at": request["issued_at"],
    "expires_at": request["expires_at"],
    "proof_ref": proof_ref,
    "proof": {{
        "schema_version": "oasis7.identity_v2_provider_authentication_proof.v1",
        "algorithm": "ed25519",
        "claims_sha256": hashlib.sha256(canonical(claims)).hexdigest(),
        "signature_hex": proof_signature.hex(),
    }},
}}
Path(args.attestation_out).write_bytes(json.dumps(attestation, sort_keys=True, separators=(",", ":")).encode("utf-8"))
'''


def verifier_source(
    invocation_marker: Path,
    tool: Path,
    governance_root: Path,
    trust_config: Path,
    provider_registry: Path,
) -> str:
    """Return a test-only independently invoked verifier process."""

    return f'''#!/usr/bin/env python3
import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path

MARKER = {str(invocation_marker)!r}
OPENSSL = {str(OPENSSL)!r}
PREFIX = b"OASIS7-IDENTITY-RECEIPT-V2\\0"

parser = argparse.ArgumentParser()
parser.add_argument("command", choices=("verify",))
parser.add_argument("--mode", required=True, choices=("current_admission", "historical_audit"))
parser.add_argument("--envelope", required=True)
parser.add_argument("--attestation", required=True)
parser.add_argument("--raw-v1", required=True)
parser.add_argument("--context", required=True)
parser.add_argument("--plan-intent", required=True)
parser.add_argument("--trust-config", required=True)
parser.add_argument("--provider-registry", required=True)
parser.add_argument("--out", required=True)
parser.add_argument("--verification-out", required=True)
args = parser.parse_args()
Path(MARKER).write_text("invoked\\n", encoding="utf-8")

def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")

def digest(value):
    return hashlib.sha256(value).hexdigest()

envelope_path = Path(args.envelope)
raw_path = Path(args.raw_v1)
context = json.loads(Path(args.context).read_text(encoding="utf-8"))
intent = json.loads(Path(args.plan_intent).read_text(encoding="utf-8"))
trust = json.loads(Path(args.trust_config).read_text(encoding="utf-8"))
registry_path = Path(args.provider_registry)
registry = json.loads(registry_path.read_text(encoding="utf-8"))
envelope = json.loads(envelope_path.read_text(encoding="utf-8"))
attestation = json.loads(Path(args.attestation).read_text(encoding="utf-8"))
signed = dict((key, envelope[key]) for key in envelope if key not in set(("signature_hex", "canonical_digest", "authenticated", "verified", "historical_only", "apply_authorized")))
payload = PREFIX + canonical(signed)
provider = next(item for item in registry["providers"] if item["signer_id"] == envelope["signer_id"])
public_key_der = Path(args.out).with_name("independent-public-key.der")
public_key_der.write_bytes(b"\\x30\\x2a\\x30\\x05\\x06\\x03\\x2b\\x65\\x70\\x03\\x21\\x00" + Path(provider["public_key_ref"]).read_bytes())
signature_path = Path(args.out).with_name("independent-signature.raw")
signature_path.write_bytes(bytes.fromhex(envelope["signature_hex"]))
payload_path = Path(args.out).with_name("independent-payload.bin")
payload_path.write_bytes(payload)
subprocess.run(
    [OPENSSL, "pkeyutl", "-verify", "-pubin", "-inkey", str(public_key_der), "-keyform", "DER", "-rawin", "-in", str(payload_path), "-sigfile", str(signature_path)],
    check=True,
    capture_output=True,
)

output = dict(signed)
output["signature_hex"] = envelope["signature_hex"]
output["canonical_digest"] = envelope["canonical_digest"]
output["authenticated"] = True
output["verified"] = True
historical_only = args.mode == "historical_audit"
output["historical_only"] = historical_only
output["apply_authorized"] = not historical_only
now = datetime.now(timezone.utc).replace(microsecond=0)
receipt = dict(
    schema_version="oasis7.identity_v2_verification_receipt.v1",
    mode=args.mode,
    evaluation_time=now.strftime("%Y-%m-%dT%H:%M:%SZ"),
    raw_v1_sha256=digest(raw_path.read_bytes()),
    canonical_payload_sha256=digest(payload),
    envelope_sha256=digest(envelope_path.read_bytes()),
    signer_id=envelope["signer_id"],
    public_key_sha256=provider["public_key_sha256"],
    trust_config_sha256=digest(Path(args.trust_config).read_bytes()),
    provider_registry_sha256=digest(registry_path.read_bytes()),
    verifier_executable_sha256=registry["verifier"]["executable_sha256"],
    network_id=envelope["network_id"],
    proof_ref=attestation["proof_ref"],
    proof_claims_sha256=attestation["proof"]["claims_sha256"],
    task_uid=envelope["task_uid"],
    head_oid=envelope["head_oid"],
    node_id=envelope["node_id"],
    peer_id=envelope["peer_id"],
    capture_window_id=envelope["capture_window_id"],
    rotation_epoch=envelope["rotation_epoch"],
    historical_only=historical_only,
    apply_authorized=not historical_only,
    authority_scope="deployed-governance-root",
    verified=True,
)
Path(args.out).write_bytes(canonical(output))
Path(args.verification_out).write_bytes(canonical(receipt))
'''


class IdentityV2SigningToolContractTests(unittest.TestCase):
    """Acceptance contract for the future prepare/sign/assemble/verify tool."""

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="oasis7-identity-v2-test-")
        self.root = Path(self.temp.name)
        self.raw = self.root / "raw-v1.json"
        self.context = self.root / "context.json"
        self.intent = self.root / "plan-intent.json"
        self.template = self.root / "unsigned-template.json"
        self.trust = self.root / "identity-v2-trust.json"
        self.registry = self.root / "identity-v2-provider-registry.json"
        self.public_key = self.root / "public-key.raw"
        self.public_key_pem = self.root / "public-key.pem"
        self.private_key = self.root / "ephemeral-custody-key.pem"
        self.provider = self.root / "ephemeral-provider.py"
        self.verifier = self.root / "pinned-verifier.py"
        self.verifier_invocation_marker = self.root / "verifier-invoked.marker"
        self.governance_root = self.root / "governance-root.json"
        self.now = datetime.now(timezone.utc).replace(microsecond=0)
        self.capture_start = (self.now - timedelta(seconds=10)).isoformat().replace("+00:00", "Z")
        self.capture_end = (self.now + timedelta(minutes=10)).isoformat().replace("+00:00", "Z")
        self.issued_at = self.now.isoformat().replace("+00:00", "Z")
        self.expires_at = (self.now + timedelta(minutes=5)).isoformat().replace("+00:00", "Z")

        self._generate_ephemeral_ed25519_material()
        self._write_provider()
        self.governance_root.write_bytes(
            (ROOT / "scripts" / "fixtures" / "oasis7-governance-root.v1.json").read_bytes()
        )
        self.governance_root.chmod(0o600)
        self._write_context_and_intent()
        self._write_raw_v1()
        self._write_trust_and_registry()
        self._write_template()

        # RED guard: all behavioral tests must fail because the production
        # executable is missing, not because a fixture silently skipped.
        self.assertTrue(
            TOOL.is_file(),
            "RED: missing proposed executable scripts/p2p-public-testnet-identity-v2-signing-tool.py",
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _generate_ephemeral_ed25519_material(self) -> None:
        self.assertTrue(OPENSSL.is_file(), f"test prerequisite missing: {OPENSSL}")
        self._openssl("genpkey", "-algorithm", "ED25519", "-out", str(self.private_key))
        self._openssl("pkey", "-in", str(self.private_key), "-pubout", "-out", str(self.public_key_pem))
        der = self.root / "public-key.der"
        self._openssl("pkey", "-in", str(self.private_key), "-pubout", "-outform", "DER", "-out", str(der))
        der_bytes = der.read_bytes()
        self.assertGreaterEqual(len(der_bytes), 32)
        self.public_key.write_bytes(der_bytes[-32:])

    def _openssl(self, *args: str) -> None:
        result = subprocess.run([str(OPENSSL), *args], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def _write_provider(self) -> None:
        self.provider.write_text(provider_source(self.private_key), encoding="utf-8")
        self.provider.chmod(self.provider.stat().st_mode | stat.S_IXUSR)
        self.verifier.write_text(
            verifier_source(
                self.verifier_invocation_marker,
                TOOL,
                self.governance_root,
                self.trust,
                self.registry,
            ),
            encoding="utf-8",
        )
        self.verifier.chmod(self.verifier.stat().st_mode | stat.S_IXUSR)

    def _write_context_and_intent(self) -> None:
        spec = importlib.util.spec_from_file_location("signing_fixture_peers", TOOL.with_name("p2p-public-testnet-peer-registry.py"))
        peer_authority = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(peer_authority)
        self.peer_registry = self.root / "managed-peers.json"
        peers = {name: PEER_ID if node_id == NODE_ID else f"FixturePeer{index}"
                 for index, (name, node_id) in enumerate(peer_authority.NODE_IDS.items())}
        peer_value = {"schema_version": peer_authority.SCHEMA, "network_id": peer_authority.NETWORK_ID,
                      "registry_epoch": "fixture-peer-epoch-1",
                      "nodes": [{"node_name": name, "node_id": node_id, "peer_id": peers[name]}
                                for name, node_id in peer_authority.NODE_IDS.items()]}
        write_json(self.peer_registry, peer_value)
        self.peer_registry.chmod(0o600)
        context = {
            "schema_version": "oasis7.identity_v2_context.v1",
            "network_id": NETWORK_ID,
            "task_uid": TASK_UID,
            "head_oid": "e" * 40,
            "capture_window_id": "capture-window-identity-v2-001",
            "capture_start": self.capture_start,
            "capture_end": self.capture_end,
            "rotation_epoch": ROTATION_EPOCH,
            "issued_at": self.issued_at,
            "expires_at": self.expires_at,
        }
        write_json(self.context, context)
        context_digest = digest_file(self.context)
        intent = {
            "schema_version": "oasis7.clean_room_plan_intent.v2",
            "context_digest": context_digest,
            "peer_registry_sha256": digest_file(self.peer_registry),
            "peer_registry_epoch": peer_value["registry_epoch"],
            "adapter_action": "public-testnet-governed-rebuild",
            "nodes": [
                {
                    "node_name": name,
                    "node_id": peer_authority.NODE_IDS[name],
                    "peer_id": peers[name],
                    "role": "validator" if name in {"storage-205", "sequencer-204"} else "observer",
                    "reset_surface_ids": ["config", "execution", "world"],
                } for name in sorted(peers)
            ],
        }
        write_json(self.intent, intent)
        self.context_digest = context_digest
        self.plan_digest = digest_file(self.intent)

    def _write_raw_v1(self) -> None:
        # Deliberate noncanonical whitespace and key order prove that the raw
        # digest binds bytes, while admission still validates its parsed shape.
        raw = {
            "schema_version": "oasis7.identity_receipt.v1",
            "node_id": NODE_ID,
            "peer_id": PEER_ID,
            "key_path": "config/node-keypair.toml",
            "key_sha256": "1" * 64,
            "key_size_bytes": 321,
            "key_mode": 384,
            "key_uid": 0,
            "key_gid": 0,
        }
        self.raw.write_bytes((json.dumps(raw, ensure_ascii=True, indent=2) + "\n").encode("utf-8"))
        self.raw_digest = digest_file(self.raw)

    def _write_trust_and_registry(self) -> None:
        trust = {
            "schema_version": "oasis7.identity_v2_trust_config.v1",
            "network_id": NETWORK_ID,
            "trust_root_id": TRUST_ROOT_ID,
            "verifier_id": VERIFIER_ID,
            "algorithm": "ed25519",
            "rotation_epoch": ROTATION_EPOCH,
            "allowlist": [
                {
                    "signer_id": SIGNER_ID,
                    "public_key_ref": str(self.public_key),
                    "public_key_sha256": digest_file(self.public_key),
                    "status": "active",
                    "valid_from": self.capture_start,
                    "valid_until": self.expires_at,
                }
            ],
            "revocations": [],
        }
        write_json(self.trust, trust)
        registry = {
            "schema_version": "oasis7.identity_v2_provider_registry.v1",
            "trust_config_path": str(self.trust),
            "trust_config_sha256": digest_file(self.trust),
            "providers": [
                {
                    "provider_id": PROVIDER_ID,
                    "adapter_path": str(self.provider),
                    "adapter_sha256": digest_file(self.provider),
                    "signer_id": SIGNER_ID,
                    "public_key_ref": str(self.public_key),
                    "public_key_sha256": digest_file(self.public_key),
                    "algorithm": "ed25519",
                }
            ],
            "verifier": {
                "executable_path": str(self.verifier),
                "executable_sha256": digest_file(self.verifier),
            },
        }
        write_json(self.registry, registry)

    def _write_template(self) -> None:
        write_json(
            self.template,
            {
                "domain_separator": "oasis7.identity_receipt.v2/signature/v1",
                "schema_version": "oasis7.identity_receipt.v2",
                "signer_id": SIGNER_ID,
                "verifier_id": VERIFIER_ID,
                "trust_root_id": TRUST_ROOT_ID,
                "network_id": NETWORK_ID,
                "task_uid": TASK_UID,
                "head_oid": "e" * 40,
                "frozen_head_oid": "e" * 40,
                "plan_digest": self.plan_digest,
                "context_digest": self.context_digest,
                "capture_window_id": "capture-window-identity-v2-001",
                "rotation_epoch": ROTATION_EPOCH,
                "issued_at": self.issued_at,
                "expires_at": self.expires_at,
                "node_id": NODE_ID,
                "peer_id": PEER_ID,
                "key_sha256": "1" * 64,
                "key_size_bytes": 321,
                "key_mode": "0600",
                "key_uid": 0,
                "key_gid": 0,
                "signed_payload_sha256": self.raw_digest,
            },
        )

    def _env(self) -> dict[str, str]:
        # Do not expose the caller's credential-bearing environment to the
        # future tool or its provider.  The fixture uses only its own paths.
        return {"PATH": os.environ.get("PATH", ""), "PYTHONIOENCODING": "utf-8"}

    def _run(
        self,
        *args: str,
        authority_pins: tuple[str, str] | None = None,
    ) -> subprocess.CompletedProcess[bytes]:
        trusted_config_sha256, trusted_registry_sha256 = authority_pins or (
            digest_file(self.trust),
            digest_file(self.registry),
        )
        harness = CHILD_HARNESS
        if hasattr(self, "peer_registry"):
            peer_setup = (
                f"tool._peer_registry_authority().REGISTRY_PATH = Path({str(self.peer_registry)!r})\n"
                f"tool._peer_registry_authority().REGISTRY_SHA256 = {digest_file(self.peer_registry)!r}\n"
            )
            harness = harness.replace("raise SystemExit(tool.main", peer_setup + "raise SystemExit(tool.main")
        return subprocess.run(
            [
                sys.executable,
                "-c",
                harness,
                str(TOOL),
                str(self.governance_root),
                str(self.trust),
                str(self.registry),
                trusted_config_sha256,
                trusted_registry_sha256,
                *args,
            ],
            cwd=ROOT,
            env=self._env(),
            capture_output=True,
        )

    def _run_unpatched(self, *args: str) -> subprocess.CompletedProcess[bytes]:
        return subprocess.run(
            [sys.executable, str(TOOL), *args],
            cwd=ROOT,
            env=self._env(),
            capture_output=True,
        )

    def _prepare_args(self, payload: Path, manifest: Path) -> list[str]:
        return [
            "prepare",
            "--raw-v1",
            str(self.raw),
            "--template",
            str(self.template),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--payload-out",
            str(payload),
            "--manifest-out",
            str(manifest),
        ]

    def _prepare(self, stem: str = "flow") -> tuple[Path, Path]:
        payload = self.root / f"{stem}.payload.bin"
        manifest = self.root / f"{stem}.prepare.json"
        result = self._run(*self._prepare_args(payload, manifest))
        self._assert_success(result)
        return payload, manifest

    def _sign(self, payload: Path, manifest: Path, stem: str = "flow") -> tuple[Path, Path]:
        signature = self.root / f"{stem}.signature.hex"
        attestation = self.root / f"{stem}.attestation.json"
        result = self._run(
            "sign",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--provider-registry",
            str(self.registry),
            "--provider-ref",
            PROVIDER_ID,
            "--signature-out",
            str(signature),
            "--attestation-out",
            str(attestation),
        )
        self._assert_success(result)
        return signature, attestation

    def _assemble(
        self, payload: Path, manifest: Path, signature: Path, attestation: Path, stem: str = "flow"
    ) -> Path:
        envelope = self.root / f"{stem}.envelope.json"
        result = self._run(
            "assemble",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--signature",
            str(signature),
            "--attestation",
            str(attestation),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(envelope),
        )
        self._assert_success(result)
        return envelope

    def _verify(
        self,
        envelope: Path,
        raw: Path | None = None,
        mode: str = "current_admission",
        attestation: Path | None = None,
    ) -> tuple[Path, Path]:
        verified = self.root / f"{mode}.verified.json"
        receipt = self.root / f"{mode}.verification.json"
        result = self._run(
            "verify",
            "--mode",
            mode,
            "--envelope",
            str(envelope),
            "--attestation",
            str(attestation or self.root / envelope.name.replace(".envelope.json", ".attestation.json")),
            "--raw-v1",
            str(raw or self.raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(verified),
            "--verification-out",
            str(receipt),
        )
        self._assert_success(result)
        return verified, receipt

    def _assert_success(self, result: subprocess.CompletedProcess[bytes]) -> None:
        self.assertEqual(
            result.returncode,
            0,
            "expected executable success; stderr=" + result.stderr.decode("utf-8", "replace"),
        )

    def _assert_rejected_no_output(self, result: subprocess.CompletedProcess[bytes], *outputs: Path) -> None:
        self.assertNotEqual(result.returncode, 0, "malformed/tampered input unexpectedly succeeded")
        for output in outputs:
            self.assertFalse(output.exists(), f"rejected operation created output: {output}")

    def _prepare_sign_assemble(self, stem: str = "flow") -> tuple[Path, Path, Path, Path, Path]:
        payload, manifest = self._prepare(stem)
        signature, attestation = self._sign(payload, manifest, stem)
        envelope = self._assemble(payload, manifest, signature, attestation, stem)
        return payload, manifest, signature, attestation, envelope

    def test_expired_historical_evidence_is_forensic_only(self) -> None:
        from unittest.mock import patch
        old_now = self.now - timedelta(days=1)
        context = json.loads(self.context.read_text())
        for field in ("capture_start", "capture_end", "issued_at", "expires_at"):
            context[field] = (datetime.fromisoformat(context[field].replace("Z", "+00:00")) - timedelta(days=1)).isoformat().replace("+00:00", "Z")
        write_json(self.context, context)
        self.capture_start, self.capture_end = context["capture_start"], context["capture_end"]
        self.issued_at, self.expires_at = context["issued_at"], context["expires_at"]
        self.context_digest = digest_file(self.context)
        trust = json.loads(self.trust.read_text())
        trust["allowlist"][0]["valid_from"] = self.capture_start
        trust["allowlist"][0]["valid_until"] = self.expires_at
        write_json(self.trust, trust)
        registry = json.loads(self.registry.read_text())
        registry["trust_config_sha256"] = digest_file(self.trust)
        write_json(self.registry, registry)
        intent = json.loads(self.intent.read_text())
        intent["context_digest"] = digest_file(self.context)
        write_json(self.intent, intent)
        self.plan_digest = digest_file(self.intent)
        self._write_template()
        clock_patch = "\nfrom datetime import datetime, timezone\nclass HistoricalClock(datetime):\n    @classmethod\n    def now(cls, tz=None):\n        return datetime.fromisoformat(" + repr(old_now.isoformat()) + ")\ntool.datetime = HistoricalClock\n"
        historical_harness = CHILD_HARNESS.replace("raise SystemExit(tool.main", clock_patch + "raise SystemExit(tool.main")
        with patch.dict(globals(), {"CHILD_HARNESS": historical_harness}):
            _, _, _, _, envelope = self._prepare_sign_assemble("expired-history")
        verified, receipt = self._verify(envelope, mode="historical_audit")
        self.assertTrue(json.loads(verified.read_text())["historical_only"])
        self.assertFalse(json.loads(receipt.read_text())["apply_authorized"])
        current_output, current_receipt = self.root / "expired-current.json", self.root / "expired-current-receipt.json"
        current = self._run(
            "verify", "--mode", "current_admission", "--envelope", str(envelope),
            "--attestation", str(self.root / "expired-history.attestation.json"),
            "--raw-v1", str(self.raw), "--context", str(self.context),
            "--plan-intent", str(self.intent), "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry), "--out", str(current_output),
            "--verification-out", str(current_receipt),
        )
        self._assert_rejected_no_output(current, current_output, current_receipt)
        self.assertIn(b"stale", current.stderr)

    def test_prepare_is_deterministic_and_binds_exact_raw_context_and_intent(self) -> None:
        payload_a, manifest_a = self._prepare("a")
        payload_b, manifest_b = self._prepare("b")
        self.assertEqual(payload_a.read_bytes(), payload_b.read_bytes())
        first = json.loads(manifest_a.read_text(encoding="utf-8"))
        second = json.loads(manifest_b.read_text(encoding="utf-8"))
        for field in (
            "canonical_payload_sha256",
            "raw_v1_sha256",
            "context_digest",
            "plan_digest",
            "task_uid",
            "head_oid",
            "node_id",
            "peer_id",
            "capture_window_id",
            "rotation_epoch",
            "trust_config_sha256",
            "provider_registry_sha256",
            "public_key_sha256",
            "verifier_executable_sha256",
        ):
            self.assertEqual(first[field], second[field], field)
        self.assertEqual(first["raw_v1_sha256"], self.raw_digest)
        self.assertEqual(first["context_digest"], self.context_digest)
        self.assertEqual(first["plan_digest"], self.plan_digest)

    def test_prepare_rejects_group_writable_authority_without_outputs(self) -> None:
        """Authority files cannot be replaceable by a non-owner account."""
        self.trust.chmod(0o664)
        payload = self.root / "unsafe-authority.payload.bin"
        manifest = self.root / "unsafe-authority.prepare.json"
        result = self._run(*self._prepare_args(payload, manifest))
        self._assert_rejected_no_output(result, payload, manifest)

    def test_prepare_rejects_group_writable_public_key_without_outputs(self) -> None:
        """Public readability is valid; non-owner write access is not."""
        self.public_key.chmod(0o664)
        payload = self.root / "unsafe-public-key.payload.bin"
        manifest = self.root / "unsafe-public-key.prepare.json"
        result = self._run(*self._prepare_args(payload, manifest))
        self._assert_rejected_no_output(result, payload, manifest)

    def test_prepare_allows_owner_written_public_key_with_public_readability(self) -> None:
        """The authority guard does not cargo-cult a 0600 mode onto public keys."""
        self.public_key.chmod(0o644)
        payload, manifest = self._prepare("public-readable")
        self.assertTrue(payload.is_file())
        self.assertTrue(manifest.is_file())

    def test_prepare_rejects_group_writable_provider_or_verifier_without_outputs(self) -> None:
        """Executable authority artifacts require an owner-only write boundary."""
        for path, stem in ((self.provider, "unsafe-provider"), (self.verifier, "unsafe-verifier")):
            with self.subTest(path=path.name):
                path.chmod(0o766)
                payload = self.root / f"{stem}.payload.bin"
                manifest = self.root / f"{stem}.prepare.json"
                result = self._run(*self._prepare_args(payload, manifest))
                self._assert_rejected_no_output(result, payload, manifest)

    def test_verify_rejects_small_order_identity_key_forgery(self) -> None:
        """A small-order public point must not validate R=identity,S=0."""
        import importlib.util

        spec = importlib.util.spec_from_file_location("identity_v2_tool_small_order", TOOL)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        identity_point = b"\x01" + b"\x00" * 31
        forged_signature = identity_point + b"\x00" * 32
        self.assertFalse(module.verify_ed25519(identity_point, b"arbitrary message", forged_signature))

    def test_verify_ed25519_matches_rfc8032_vectors_and_affine_law(self) -> None:
        """Extended-coordinate scalar mult must match RFC 8032 §7.1 and affine addition."""
        spec = importlib.util.spec_from_file_location("identity_v2_tool_rfc8032", TOOL)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        # RFC 8032 §7.1 TEST 1 (empty message) and TEST 2 (1-byte 0x72 message)
        vectors = (
            (
                bytes.fromhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"),
                b"",
                bytes.fromhex(
                    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
                    "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
                ),
            ),
            (
                bytes.fromhex("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
                b"\x72",
                bytes.fromhex(
                    "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
                    "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
                ),
            ),
        )
        for pub, msg, sig in vectors:
            self.assertTrue(module.verify_ed25519(pub, msg, sig))
            self.assertFalse(module.verify_ed25519(pub, msg + b"\x00", sig))
        p2 = module._edwards(module.B, module.B)
        p5 = module._edwards(module._edwards(p2, p2), module.B)
        self.assertEqual(module._scalarmult(module.B, 0), (0, 1))
        self.assertEqual(module._scalarmult(module.B, 2), p2)
        self.assertEqual(module._scalarmult(module.B, 5), p5)

    def test_prepare_rejects_output_alias_to_input_without_deleting_it(self) -> None:
        """A derived output cannot replace a caller input before validation."""
        original = self.raw.read_bytes()
        manifest = self.root / "aliased-input.prepare.json"
        result = self._run(*self._prepare_args(self.raw, manifest))
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.raw.is_file())
        self.assertEqual(self.raw.read_bytes(), original)
        self.assertFalse(manifest.exists())

    def test_prepare_preserves_existing_derived_outputs_on_validation_failure(self) -> None:
        """Failed retries retain the last complete derived evidence pair."""
        payload, manifest = self._prepare("preserve-existing")
        payload_before = payload.read_bytes()
        manifest_before = manifest.read_bytes()
        context = json.loads(self.context.read_text(encoding="utf-8"))
        context["network_id"] = "attacker-network"
        write_json(self.context, context)
        result = self._run(*self._prepare_args(payload, manifest))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(payload.read_bytes(), payload_before)
        self.assertEqual(manifest.read_bytes(), manifest_before)

    def test_current_admission_rejects_envelope_epoch_not_matching_trust(self) -> None:
        """Current admission must use the trust config's current rotation epoch."""
        _, _, _, attestation, envelope = self._prepare_sign_assemble("stale-rotation")
        trust = json.loads(self.trust.read_text(encoding="utf-8"))
        trust["rotation_epoch"] = "identity-v2-rotation-0002"
        write_json(self.trust, trust)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["trust_config_sha256"] = digest_file(self.trust)
        write_json(self.registry, registry)
        verified = self.root / "stale-rotation.verified.json"
        receipt = self.root / "stale-rotation.verification.json"
        result = self._run(
            "verify",
            "--mode", "current_admission",
            "--envelope", str(envelope),
            "--attestation", str(attestation),
            "--raw-v1", str(self.raw),
            "--context", str(self.context),
            "--plan-intent", str(self.intent),
            "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry),
            "--out", str(verified),
            "--verification-out", str(receipt),
        )
        self._assert_rejected_no_output(result, verified, receipt)

    def test_authority_descriptor_reader_rejects_writable_ancestor(self) -> None:
        """Authority reads must bind metadata and the opened descriptor."""
        import importlib.util
        from unittest.mock import patch

        spec = importlib.util.spec_from_file_location("identity_v2_tool_test", TOOL)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        unsafe_root = self.root / "unsafe-parent"
        unsafe_root.mkdir()
        unsafe_root.chmod(0o777)
        target = unsafe_root / "authority.json"
        target.write_bytes(b"{}")
        target.chmod(0o644)
        with self.assertRaises(module.ToolError):
            module.read_authority_bytes(target, "authority")

        unsafe_root.chmod(0o700)
        replacement = unsafe_root / "replacement.json"
        replacement.write_bytes(b'{"replacement":true}')
        replacement.chmod(0o644)
        calls = 0
        real_fstat = module.os.fstat

        def replace_after_open(descriptor: int) -> Any:
            nonlocal calls
            calls += 1
            result = real_fstat(descriptor)
            if calls == 1:
                target.unlink()
                replacement.rename(target)
            return result

        target.write_bytes(b'{"original":true}')
        target.chmod(0o644)
        with patch.object(module.os, "fstat", side_effect=replace_after_open):
            with self.assertRaises(module.ToolError):
                module.read_authority_bytes(target, "authority")
        self.assertEqual(calls, 2)

    def test_prepare_rejects_context_network_different_from_governed_network(self) -> None:
        """A self-consistent context cannot select a different deployment network."""
        context = json.loads(self.context.read_text(encoding="utf-8"))
        context["network_id"] = "attacker-network"
        write_json(self.context, context)
        self.context_digest = digest_file(self.context)
        intent = json.loads(self.intent.read_text(encoding="utf-8"))
        intent["context_digest"] = self.context_digest
        write_json(self.intent, intent)
        self.plan_digest = digest_file(self.intent)
        self._write_template()

        payload = self.root / "network-mismatch.payload.bin"
        manifest = self.root / "network-mismatch.prepare.json"
        result = self._run(*self._prepare_args(payload, manifest))
        self._assert_rejected_no_output(result, payload, manifest)

    def test_verify_rejects_forged_context_network_even_with_recalculated_signature(self) -> None:
        """Verification must bind the signed envelope to the governed network."""
        _, _, _, baseline_attestation, _ = self._prepare_sign_assemble("forged-network-baseline")
        context = json.loads(self.context.read_text(encoding="utf-8"))
        context["network_id"] = "attacker-network"
        write_json(self.context, context)
        self.context_digest = digest_file(self.context)
        intent = json.loads(self.intent.read_text(encoding="utf-8"))
        intent["context_digest"] = self.context_digest
        write_json(self.intent, intent)
        self.plan_digest = digest_file(self.intent)
        self._write_template()

        # Build a self-consistent forged payload directly so this regression
        # reaches verify even after prepare is correctly fenced.
        forged_payload = PREFIX + canonical(json.loads(self.template.read_text(encoding="utf-8")))
        payload_path = self.root / "forged-network.payload.bin"
        payload_path.write_bytes(forged_payload)
        signature_path = self.root / "forged-network.signature.bin"
        subprocess.run(
            [
                str(OPENSSL),
                "pkeyutl",
                "-sign",
                "-inkey",
                str(self.private_key),
                "-rawin",
                "-in",
                str(payload_path),
                "-out",
                str(signature_path),
            ],
            check=True,
            capture_output=True,
        )
        envelope = json.loads(self.template.read_text(encoding="utf-8"))
        envelope["signature_hex"] = signature_path.read_bytes().hex()
        envelope["canonical_digest"] = digest_bytes(
            canonical({**envelope, "signature_hex": envelope["signature_hex"]})
        )
        envelope["authenticated"] = False
        envelope["verified"] = False
        envelope_path = self.root / "forged-network.envelope.json"
        write_json(envelope_path, envelope)
        verified_path = self.root / "forged-network.verified.json"
        receipt_path = self.root / "forged-network.verification.json"
        result = self._run(
            "verify",
            "--mode",
            "current_admission",
            "--envelope",
            str(envelope_path),
            "--attestation",
            str(baseline_attestation),
            "--raw-v1",
            str(self.raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(verified_path),
            "--verification-out",
            str(receipt_path),
        )
        self._assert_rejected_no_output(result, verified_path, receipt_path)

    def test_openssl_selection_supports_host_ed25519_operations(self) -> None:
        """The selected host binary must support the real Ed25519 fixture."""
        self.assertTrue(OPENSSL.is_file())
        self.assertTrue(os.access(OPENSSL, os.X_OK))
        self.assertTrue(_supports_ed25519(OPENSSL))

    def test_real_ed25519_sign_assemble_verify_preserves_one_immutable_payload(self) -> None:
        payload, manifest, signature, attestation, envelope = self._prepare_sign_assemble()
        payload_bytes = payload.read_bytes()
        signature_bytes = bytes.fromhex(signature.read_text(encoding="utf-8").strip())
        self.assertEqual(len(signature_bytes), 64)
        self._openssl_verify(payload, signature_bytes)
        verified, verification = self._verify(envelope)
        self.assertEqual(payload.read_bytes(), payload_bytes)
        self.assertEqual(json.loads(manifest.read_text())["canonical_payload_sha256"], digest_bytes(payload_bytes))
        self.assertEqual(json.loads(attestation.read_text())["canonical_payload_sha256"], digest_bytes(payload_bytes))
        output = json.loads(verified.read_text(encoding="utf-8"))
        receipt = json.loads(verification.read_text(encoding="utf-8"))
        self.assertTrue(output["authenticated"])
        self.assertTrue(output["verified"])
        self.assertEqual(output["network_id"], NETWORK_ID)
        self.assertTrue(receipt["apply_authorized"])
        self.assertEqual(receipt["network_id"], NETWORK_ID)
        self.assertEqual(receipt["canonical_payload_sha256"], digest_bytes(payload_bytes))

    def test_v2_attestation_has_bound_proof_and_fresh_request_challenge(self) -> None:
        _, _, _, attestation_a, _ = self._prepare_sign_assemble("proof-a")
        _, _, _, attestation_b, _ = self._prepare_sign_assemble("proof-b")
        first = json.loads(attestation_a.read_text(encoding="utf-8"))
        second = json.loads(attestation_b.read_text(encoding="utf-8"))
        self.assertEqual(first["schema_version"], "oasis7.identity_v2_provider_attestation.v2")
        self.assertEqual(first["network_id"], NETWORK_ID)
        self.assertRegex(first["request_id"], r"^req-v2:[0-9a-f]{64}$")
        self.assertRegex(first["proof_ref"], r"^proof-v1:[0-9a-f]{64}$")
        self.assertEqual(
            set(first["proof"]),
            {"schema_version", "algorithm", "claims_sha256", "signature_hex"},
        )
        self.assertEqual(
            first["proof"]["schema_version"],
            "oasis7.identity_v2_provider_authentication_proof.v1",
        )
        self.assertEqual(first["proof"]["algorithm"], "ed25519")
        self.assertRegex(first["proof"]["claims_sha256"], r"^[0-9a-f]{64}$")
        self.assertRegex(first["proof"]["signature_hex"], r"^[0-9a-f]{128}$")
        self.assertNotEqual(first["request_id"], second["request_id"])

    def test_attestation_proof_signature_tamper_rejects_assemble(self) -> None:
        payload, manifest, signature, attestation, _ = self._prepare_sign_assemble("proof-tamper")
        value = json.loads(attestation.read_text(encoding="utf-8"))
        original = value["proof"]["signature_hex"]
        value["proof"]["signature_hex"] = ("0" if original[0] != "0" else "1") + original[1:]
        write_json(attestation, value)
        envelope = self.root / "proof-tamper-rejected.envelope.json"
        result = self._run(
            "assemble",
            "--payload", str(payload),
            "--manifest", str(manifest),
            "--signature", str(signature),
            "--attestation", str(attestation),
            "--provider-registry", str(self.registry),
            "--out", str(envelope),
        )
        self._assert_rejected_no_output(result, envelope)

    def test_verify_requires_authenticated_attestation_input(self) -> None:
        _, _, _, _, envelope = self._prepare_sign_assemble("mandatory-attestation")
        verified = self.root / "missing-attestation.verified.json"
        receipt = self.root / "missing-attestation.verification.json"
        result = self._run(
            "verify",
            "--mode", "current_admission",
            "--envelope", str(envelope),
            "--raw-v1", str(self.raw),
            "--context", str(self.context),
            "--plan-intent", str(self.intent),
            "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry),
            "--out", str(verified),
            "--verification-out", str(receipt),
        )
        self._assert_rejected_no_output(result, verified, receipt)

    def test_v1_attestation_schema_cannot_be_silently_reused(self) -> None:
        payload, manifest, signature, attestation, _ = self._prepare_sign_assemble("v1-reject")
        value = json.loads(attestation.read_text(encoding="utf-8"))
        value["schema_version"] = "oasis7.identity_v2_provider_attestation.v1"
        envelope = self.root / "v1-reject.envelope.json"
        existing_envelope = envelope.read_bytes()
        write_json(attestation, value)
        result = self._run(
            "assemble",
            "--payload", str(payload),
            "--manifest", str(manifest),
            "--signature", str(signature),
            "--attestation", str(attestation),
            "--provider-registry", str(self.registry),
            "--out", str(envelope),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(envelope.read_bytes(), existing_envelope)

    def _openssl_verify(self, payload: Path, signature_bytes: bytes) -> None:
        signature = self.root / "independent-check.signature"
        signature.write_bytes(signature_bytes)
        result = subprocess.run(
            [
                str(OPENSSL),
                "pkeyutl",
                "-verify",
                "-pubin",
                "-inkey",
                str(self.public_key_pem),
                "-rawin",
                "-in",
                str(payload),
                "-sigfile",
                str(signature),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_assemble_requires_explicit_unchanged_payload(self) -> None:
        payload, manifest, signature, attestation, _ = self._prepare_sign_assemble()
        envelope = self.root / "tampered-payload.envelope.json"
        original = payload.read_bytes()
        payload.write_bytes(original[:-1] + bytes([original[-1] ^ 1]))
        result = self._run(
            "assemble",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--signature",
            str(signature),
            "--attestation",
            str(attestation),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(envelope),
        )
        self._assert_rejected_no_output(result, envelope)

    def test_current_admission_rejects_retired_key_but_historical_is_non_authorizing(self) -> None:
        _, _, _, _, envelope = self._prepare_sign_assemble("retired")
        trust = json.loads(self.trust.read_text(encoding="utf-8"))
        trust["allowlist"][0]["status"] = "retired"
        write_json(self.trust, trust)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["trust_config_sha256"] = digest_file(self.trust)
        write_json(self.registry, registry)
        current_verified = self.root / "retired-current.verified.json"
        current_receipt = self.root / "retired-current.verification.json"
        current = self._run(
            "verify",
            "--mode",
            "current_admission",
            "--envelope",
            str(envelope),
            "--attestation",
            str(self.root / "retired.attestation.json"),
            "--raw-v1",
            str(self.raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(current_verified),
            "--verification-out",
            str(current_receipt),
        )
        self._assert_rejected_no_output(current, current_verified, current_receipt)
        historical_verified, historical_receipt = self._verify(envelope, mode="historical_audit")
        historical = json.loads(historical_verified.read_text(encoding="utf-8"))
        receipt = json.loads(historical_receipt.read_text(encoding="utf-8"))
        self.assertTrue(historical["historical_only"])
        self.assertFalse(historical["apply_authorized"])
        self.assertTrue(receipt["historical_only"])
        self.assertFalse(receipt["apply_authorized"])

    def test_current_admission_rejects_revoked_key_but_historical_receipt_cannot_apply(self) -> None:
        _, _, _, _, envelope = self._prepare_sign_assemble("revoked")
        trust = json.loads(self.trust.read_text(encoding="utf-8"))
        trust["allowlist"][0]["status"] = "revoked"
        trust["revocations"] = [
            {
                "signer_id": SIGNER_ID,
                "effective_at": self.expires_at,
                "reason": "test-only rotation",
            }
        ]
        write_json(self.trust, trust)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["trust_config_sha256"] = digest_file(self.trust)
        write_json(self.registry, registry)
        current_verified = self.root / "revoked-current.verified.json"
        current_receipt = self.root / "revoked-current.verification.json"
        current = self._run(
            "verify",
            "--mode",
            "current_admission",
            "--envelope",
            str(envelope),
            "--attestation",
            str(self.root / "revoked.attestation.json"),
            "--raw-v1",
            str(self.raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(current_verified),
            "--verification-out",
            str(current_receipt),
        )
        self._assert_rejected_no_output(current, current_verified, current_receipt)
        historical_verified, historical_receipt = self._verify(envelope, mode="historical_audit")
        historical = json.loads(historical_verified.read_text(encoding="utf-8"))
        receipt = json.loads(historical_receipt.read_text(encoding="utf-8"))
        self.assertTrue(historical["historical_only"])
        self.assertFalse(historical["apply_authorized"])
        self.assertTrue(receipt["historical_only"])
        self.assertFalse(receipt["apply_authorized"])

    def test_provider_registry_and_executable_public_key_digests_are_pinned(self) -> None:
        payload, manifest, _, _, envelope = self._prepare_sign_assemble("pins")
        original = self.provider.read_bytes()
        self.provider.write_bytes(original + b"\n# tampered adapter\n")
        signature = self.root / "pins.signature.hex"
        attestation = self.root / "pins.attestation.json"
        existing_signature = signature.read_bytes()
        existing_attestation = attestation.read_bytes()
        result = self._run(
            "sign",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--provider-registry",
            str(self.registry),
            "--provider-ref",
            PROVIDER_ID,
            "--signature-out",
            str(signature),
            "--attestation-out",
            str(attestation),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(signature.read_bytes(), existing_signature)
        self.assertEqual(attestation.read_bytes(), existing_attestation)

        self.provider.write_bytes(original)
        self.verifier.write_bytes(self.verifier.read_bytes() + b"tamper\n")
        verified = self.root / "verifier-pin.verified.json"
        receipt = self.root / "verifier-pin.verification.json"
        result = self._run(
            "verify",
            "--mode",
            "current_admission",
            "--envelope",
            str(envelope),
            "--attestation",
            str(self.root / "pins.attestation.json"),
            "--raw-v1",
            str(self.raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(verified),
            "--verification-out",
            str(receipt),
        )
        self._assert_rejected_no_output(result, verified, receipt)

    def test_verifier_pin_rejects_nonexec_and_substituted_paths_before_output(self) -> None:
        _, _, _, _, envelope = self._prepare_sign_assemble("verifier-path")
        original_mode = self.verifier.stat().st_mode
        self.verifier.chmod(original_mode & ~stat.S_IXUSR)
        nonexec_verified = self.root / "nonexec-verifier.verified.json"
        nonexec_receipt = self.root / "nonexec-verifier.verification.json"
        result = self._run(
            "verify",
            "--mode", "current_admission",
            "--envelope", str(envelope),
            "--attestation", str(self.root / "verifier-path.attestation.json"),
            "--raw-v1", str(self.raw),
            "--context", str(self.context),
            "--plan-intent", str(self.intent),
            "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry),
            "--out", str(nonexec_verified),
            "--verification-out", str(nonexec_receipt),
        )
        self._assert_rejected_no_output(result, nonexec_verified, nonexec_receipt)

        self.verifier.chmod(original_mode)
        trusted_config_sha256 = digest_file(self.trust)
        trusted_registry_sha256 = digest_file(self.registry)
        substituted = self.root / "substituted-verifier.py"
        substituted.write_bytes(self.verifier.read_bytes())
        substituted.chmod(substituted.stat().st_mode | stat.S_IXUSR)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["verifier"] = {
            "executable_path": str(substituted),
            "executable_sha256": digest_file(substituted),
        }
        write_json(self.registry, registry)
        substituted_verified = self.root / "substituted-verifier.verified.json"
        substituted_receipt = self.root / "substituted-verifier.verification.json"
        result = self._run(
            "verify",
            "--mode", "current_admission",
            "--envelope", str(envelope),
            "--attestation", str(self.root / "verifier-path.attestation.json"),
            "--raw-v1", str(self.raw),
            "--context", str(self.context),
            "--plan-intent", str(self.intent),
            "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry),
            "--out", str(substituted_verified),
            "--verification-out", str(substituted_receipt),
            authority_pins=(trusted_config_sha256, trusted_registry_sha256),
        )
        self._assert_rejected_no_output(result, substituted_verified, substituted_receipt)

    def test_verify_invokes_registry_selected_verifier_before_admission_output(self) -> None:
        """Verification must execute the registry-selected independent verifier."""
        _, _, _, _, envelope = self._prepare_sign_assemble("verifier-invocation")
        self.verifier_invocation_marker.unlink(missing_ok=True)

        verified, receipt = self._verify(envelope, mode="current_admission")

        self.assertTrue(self.verifier_invocation_marker.exists())
        self.assertTrue(verified.is_file())
        self.assertTrue(receipt.is_file())

    def test_prepare_and_sign_second_publication_failure_preserves_pair(self) -> None:
        from unittest.mock import patch
        for command in ("prepare", "sign"):
            for existing in (False, True):
                with self.subTest(command=command, existing=existing):
                    stem = f"publication-{command}-{existing}"
                    if command == "prepare":
                        first, second = self._prepare(stem)
                        args = self._prepare_args(first, second)
                    else:
                        payload, manifest = self._prepare(stem)
                        first, second = self._sign(payload, manifest, stem)
                        args = ["sign", "--payload", str(payload), "--manifest", str(manifest),
                            "--provider-registry", str(self.registry), "--provider-ref", PROVIDER_ID,
                            "--signature-out", str(first), "--attestation-out", str(second)]
                    if existing:
                        # A byte-distinct retained output detects replacement
                        # even when this deterministic fixture reissues a payload.
                        first.write_bytes(first.read_bytes() + b"\n")
                        previous = (first.read_bytes(), second.read_bytes())
                    else:
                        first.unlink()
                        second.unlink()
                    injection = '''
original_replace = tool.os.replace
publication_failed = False
def fail_second_replace(source, destination):
    global publication_failed
    if not publication_failed and str(destination) == %r:
        publication_failed = True
        raise OSError("injected second publication failure")
    return original_replace(source, destination)
tool.os.replace = fail_second_replace
''' % str(second)
                    harness = CHILD_HARNESS.replace("raise SystemExit(tool.main", injection + "raise SystemExit(tool.main")
                    self.assertNotEqual(harness, CHILD_HARNESS)
                    with patch.dict(globals(), {"CHILD_HARNESS": harness}):
                        result = self._run(*args)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b"cannot write", result.stderr)
                    if existing:
                        self.assertEqual(second.read_bytes(), previous[1])
                        self.assertEqual(first.read_bytes(), previous[0])
                    else:
                        self.assertFalse(second.exists())
                        self.assertFalse(first.exists())

    def test_verify_second_publication_failure_preserves_existing_pair(self) -> None:
        from unittest.mock import patch
        _, _, _, _, envelope = self._prepare_sign_assemble("pair-publication")
        verified, receipt = self._verify(envelope)
        # Retain an earlier generation; the second attempted publication must
        # not leave a mixed pair when its destination replacement fails.
        original_envelope = verified.read_bytes() + b"\n"
        verified.write_bytes(original_envelope)
        original_receipt = receipt.read_bytes()
        injection = r'''
original_replace = tool.os.replace
def fail_receipt_replace(source, destination):
    if str(destination) == sys.argv[sys.argv.index("--verification-out") + 1]:
        raise OSError("injected second publication failure")
    return original_replace(source, destination)
tool.os.replace = fail_receipt_replace
'''
        harness = CHILD_HARNESS.replace("raise SystemExit(tool.main", injection + "raise SystemExit(tool.main")
        self.assertNotEqual(harness, CHILD_HARNESS)
        with patch.dict(globals(), {"CHILD_HARNESS": harness}):
            result = self._run("verify", "--mode", "current_admission",
                "--envelope", str(envelope), "--attestation", str(self.root / "pair-publication.attestation.json"),
                "--raw-v1", str(self.raw), "--context", str(self.context),
                "--plan-intent", str(self.intent), "--trust-config", str(self.trust),
                "--provider-registry", str(self.registry), "--out", str(verified),
                "--verification-out", str(receipt))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"cannot write", result.stderr)
        self.assertEqual(receipt.read_bytes(), original_receipt)
        self.assertEqual(verified.read_bytes(), original_envelope)

    def test_verify_rejects_registry_verifier_failure_without_outputs(self) -> None:
        """A failed registry verifier cannot be replaced by local verification."""
        _, _, _, _, envelope = self._prepare_sign_assemble("verifier-failure")
        self.verifier.write_text("#!/usr/bin/env python3\nimport sys\nsys.exit(17)\n", encoding="utf-8")
        self.verifier.chmod(self.verifier.stat().st_mode | stat.S_IXUSR)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["verifier"]["executable_sha256"] = digest_file(self.verifier)
        write_json(self.registry, registry)
        verified = self.root / "verifier-failure.verified.json"
        receipt = self.root / "verifier-failure.verification.json"

        result = self._run(
            "verify",
            "--mode", "current_admission",
            "--envelope", str(envelope),
            "--attestation", str(self.root / "verifier-failure.attestation.json"),
            "--raw-v1", str(self.raw),
            "--context", str(self.context),
            "--plan-intent", str(self.intent),
            "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry),
            "--out", str(verified),
            "--verification-out", str(receipt),
            authority_pins=(digest_file(self.trust), digest_file(self.registry)),
        )

        self._assert_rejected_no_output(result, verified, receipt)

    def test_verify_rejects_registry_verifier_output_mismatch_without_outputs(self) -> None:
        """A successful verifier cannot submit a receipt for another envelope."""
        _, _, _, _, envelope = self._prepare_sign_assemble("verifier-output-mismatch")
        self.verifier.write_text(
            self.verifier.read_text(encoding="utf-8")
            + '\nvalue = json.loads(Path(args.out).read_text(encoding="utf-8"))\n'
            + 'value["network_id"] = "attacker-network"\n'
            + 'Path(args.out).write_bytes(canonical(value))\n',
            encoding="utf-8",
        )
        self.verifier.chmod(self.verifier.stat().st_mode | stat.S_IXUSR)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["verifier"]["executable_sha256"] = digest_file(self.verifier)
        write_json(self.registry, registry)
        verified = self.root / "verifier-output-mismatch.verified.json"
        receipt = self.root / "verifier-output-mismatch.verification.json"

        result = self._run(
            "verify",
            "--mode", "current_admission",
            "--envelope", str(envelope),
            "--attestation", str(self.root / "verifier-output-mismatch.attestation.json"),
            "--raw-v1", str(self.raw),
            "--context", str(self.context),
            "--plan-intent", str(self.intent),
            "--trust-config", str(self.trust),
            "--provider-registry", str(self.registry),
            "--out", str(verified),
            "--verification-out", str(receipt),
            authority_pins=(digest_file(self.trust), digest_file(self.registry)),
        )

        self._assert_rejected_no_output(result, verified, receipt)

    def test_prepare_rejects_future_issued_at_before_any_output(self) -> None:
        """A receipt issued more than the five-second skew window is never prepared."""
        future = (datetime.now(timezone.utc) + timedelta(seconds=10)).replace(microsecond=0)
        context = json.loads(self.context.read_text(encoding="utf-8"))
        context["issued_at"] = future.isoformat().replace("+00:00", "Z")
        context["expires_at"] = (future + timedelta(minutes=5)).isoformat().replace("+00:00", "Z")
        context["capture_end"] = (future + timedelta(minutes=10)).isoformat().replace("+00:00", "Z")
        self.issued_at = context["issued_at"]
        self.expires_at = context["expires_at"]
        write_json(self.context, context)
        self.context_digest = digest_file(self.context)
        intent = json.loads(self.intent.read_text(encoding="utf-8"))
        intent["context_digest"] = self.context_digest
        write_json(self.intent, intent)
        self.plan_digest = digest_file(self.intent)
        self._write_template()

        payload = self.root / "future-issued.payload.bin"
        manifest = self.root / "future-issued.prepare.json"
        result = self._run(*self._prepare_args(payload, manifest))
        self._assert_rejected_no_output(result, payload, manifest)
        self.assertIn(b"future", result.stderr.lower())

    def test_verify_rejects_future_issued_at_for_current_and_historical_modes(self) -> None:
        """Verification never admits a receipt issued beyond the skew window."""
        _, _, _, _, envelope = self._prepare_sign_assemble("future-verify")
        future = (datetime.now(timezone.utc) + timedelta(seconds=10)).replace(microsecond=0)
        context = json.loads(self.context.read_text(encoding="utf-8"))
        context["issued_at"] = future.isoformat().replace("+00:00", "Z")
        context["expires_at"] = (future + timedelta(minutes=5)).isoformat().replace("+00:00", "Z")
        context["capture_end"] = (future + timedelta(minutes=10)).isoformat().replace("+00:00", "Z")
        write_json(self.context, context)
        for mode in ("current_admission", "historical_audit"):
            verified = self.root / f"future-{mode}.verified.json"
            receipt = self.root / f"future-{mode}.verification.json"
            result = self._run(
                "verify",
                "--mode", mode,
                "--envelope", str(envelope),
                "--attestation", str(self.root / "future-verify.attestation.json"),
                "--raw-v1", str(self.raw),
                "--context", str(self.context),
                "--plan-intent", str(self.intent),
                "--trust-config", str(self.trust),
                "--provider-registry", str(self.registry),
                "--out", str(verified),
                "--verification-out", str(receipt),
            )
            with self.subTest(mode=mode):
                self._assert_rejected_no_output(result, verified, receipt)
                self.assertIn(b"future", result.stderr.lower())

    def test_no_caller_selected_provider_command_or_endpoint(self) -> None:
        payload, manifest = self._prepare("provider-selection")
        signature = self.root / "rogue.signature.hex"
        attestation = self.root / "rogue.attestation.json"
        result = self._run(
            "sign",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--provider-registry",
            str(self.registry),
            "--provider-ref",
            PROVIDER_ID,
            "--provider-command",
            "/tmp/rogue-provider",
            "--provider-endpoint",
            "https://rogue.invalid/sign",
            "--signature-out",
            str(signature),
            "--attestation-out",
            str(attestation),
        )
        self._assert_rejected_no_output(result, signature, attestation)

        unknown_signature = self.root / "unknown-provider.signature.hex"
        unknown_attestation = self.root / "unknown-provider.attestation.json"
        result = self._run(
            "sign",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--provider-registry",
            str(self.registry),
            "--provider-ref",
            "/tmp/arbitrary-provider",
            "--signature-out",
            str(unknown_signature),
            "--attestation-out",
            str(unknown_attestation),
        )
        self._assert_rejected_no_output(result, unknown_signature, unknown_attestation)

    def test_template_verified_flags_and_derived_fields_have_no_authority(self) -> None:
        template = json.loads(self.template.read_text(encoding="utf-8"))
        template.update(
            {
                "authenticated": True,
                "verified": True,
                "signature_hex": "a" * 128,
                "canonical_digest": "b" * 64,
            }
        )
        write_json(self.template, template)
        payload = self.root / "template-flags.payload.bin"
        manifest = self.root / "template-flags.prepare.json"
        result = self._run(*self._prepare_args(payload, manifest))
        self._assert_rejected_no_output(result, payload, manifest)

    def test_duplicate_unknown_and_malformed_json_reject_before_any_output(self) -> None:
        cases = {
            "duplicate": b'{"schema_version":"oasis7.identity_v2_context.v1","schema_version":"bad"}',
            "unknown": canonical({"schema_version": "oasis7.identity_v2_context.v1", "unexpected": True}),
            "malformed": b"not-json",
        }
        for name, data in cases.items():
            with self.subTest(case=name):
                context = self.root / f"{name}.context.json"
                context.write_bytes(data)
                payload = self.root / f"{name}.payload.bin"
                manifest = self.root / f"{name}.prepare.json"
                args = self._prepare_args(payload, manifest)
                args[args.index("--context") + 1] = str(context)
                result = self._run(*args)
                self._assert_rejected_no_output(result, payload, manifest)

    def test_wrong_domain_and_cross_pair_raw_context_intent_reject_without_output(self) -> None:
        template = json.loads(self.template.read_text(encoding="utf-8"))
        template["domain_separator"] = "wrong-domain"
        write_json(self.template, template)
        wrong_domain_payload = self.root / "wrong-domain.payload.bin"
        wrong_domain_manifest = self.root / "wrong-domain.prepare.json"
        result = self._run(*self._prepare_args(wrong_domain_payload, wrong_domain_manifest))
        self._assert_rejected_no_output(result, wrong_domain_payload, wrong_domain_manifest)

        # Restore the template, produce a real flow, then pair it with a raw-v1
        # capture belonging to another peer.  The verifier must hash and reject
        # the bytes before it can emit a receipt.
        self._write_template()
        _, _, _, _, envelope = self._prepare_sign_assemble("cross-pair")
        other_raw = self.root / "other-node.raw-v1.json"
        other_raw.write_bytes(
            self.raw.read_bytes().replace(PEER_ID.encode("utf-8"), b"12D3KooWOtherPeer")
        )
        verified = self.root / "cross-pair.verified.json"
        receipt = self.root / "cross-pair.verification.json"
        result = self._run(
            "verify",
            "--mode",
            "current_admission",
            "--envelope",
            str(envelope),
            "--attestation",
            str(self.root / "cross-pair.attestation.json"),
            "--raw-v1",
            str(other_raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(verified),
            "--verification-out",
            str(receipt),
        )
        self._assert_rejected_no_output(result, verified, receipt)

    def test_signature_payload_key_and_envelope_tamper_reject_without_output(self) -> None:
        payload, manifest, signature, attestation, envelope = self._prepare_sign_assemble("tamper")
        original = signature.read_text(encoding="utf-8").strip()
        signature.write_text(("0" if original[0] != "0" else "1") + original[1:] + "\n", encoding="utf-8")
        assembled = self.root / "tampered-signature.envelope.json"
        result = self._run(
            "assemble",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--signature",
            str(signature),
            "--attestation",
            str(attestation),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(assembled),
        )
        self._assert_rejected_no_output(result, assembled)

        tampered_envelope = self.root / "tampered-envelope.json"
        envelope_value = json.loads(envelope.read_text(encoding="utf-8"))
        envelope_value["node_id"] = "triad-testnet-storage"
        write_json(tampered_envelope, envelope_value)
        verified = self.root / "tampered-envelope.verified.json"
        receipt = self.root / "tampered-envelope.verification.json"
        result = self._run(
            "verify",
            "--mode",
            "current_admission",
            "--envelope",
            str(tampered_envelope),
            "--attestation",
            str(self.root / "tamper.attestation.json"),
            "--raw-v1",
            str(self.raw),
            "--context",
            str(self.context),
            "--plan-intent",
            str(self.intent),
            "--trust-config",
            str(self.trust),
            "--provider-registry",
            str(self.registry),
            "--out",
            str(verified),
            "--verification-out",
            str(receipt),
        )
        self._assert_rejected_no_output(result, verified, receipt)

    def test_wrong_present_governance_root_rejects_before_provider_command(self) -> None:
        """A present root with altered content cannot reach custody signing."""
        payload, manifest = self._prepare("root-hardening")
        original = self.governance_root.read_bytes()
        marker = b'"root_id": "oasis7-public-testnet-governance-root-v1"'
        self.assertIn(marker, original)
        self.governance_root.write_bytes(
            original.replace(marker, b'"root_id": "wrong-test-root"', 1)
        )
        self.governance_root.chmod(0o600)
        signature = self.root / "root-hardening.signature.hex"
        attestation = self.root / "root-hardening.attestation.json"
        result = self._run(
            "sign",
            "--payload",
            str(payload),
            "--manifest",
            str(manifest),
            "--provider-registry",
            str(self.registry),
            "--provider-ref",
            PROVIDER_ID,
            "--signature-out",
            str(signature),
            "--attestation-out",
            str(attestation),
        )
        self._assert_rejected_no_output(result, signature, attestation)

    def test_signer_validity_interval_must_cover_issuance_in_both_modes(self) -> None:
        """A signer becoming valid after issuance cannot authorize either mode."""
        _, _, _, _, envelope = self._prepare_sign_assemble("validity-at-issuance")
        trust = json.loads(self.trust.read_text(encoding="utf-8"))
        trust["allowlist"][0]["valid_from"] = (
            self.now + timedelta(seconds=1)
        ).isoformat().replace("+00:00", "Z")
        trust["allowlist"][0]["valid_until"] = (
            self.now + timedelta(minutes=9)
        ).isoformat().replace("+00:00", "Z")
        write_json(self.trust, trust)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["trust_config_sha256"] = digest_file(self.trust)
        write_json(self.registry, registry)
        for mode in ("current_admission", "historical_audit"):
            with self.subTest(mode=mode):
                verified = self.root / f"validity-{mode}.verified.json"
                receipt = self.root / f"validity-{mode}.verification.json"
                result = self._run(
                    "verify",
                    "--mode",
                    mode,
                    "--envelope",
                    str(envelope),
                    "--attestation",
                    str(self.root / "validity-at-issuance.attestation.json"),
                    "--raw-v1",
                    str(self.raw),
                    "--context",
                    str(self.context),
                    "--plan-intent",
                    str(self.intent),
                    "--trust-config",
                    str(self.trust),
                    "--provider-registry",
                    str(self.registry),
                    "--out",
                    str(verified),
                    "--verification-out",
                    str(receipt),
                )
                self._assert_rejected_no_output(result, verified, receipt)

    def test_copied_registry_path_is_not_an_independent_deployment_anchor(self) -> None:
        """A caller-copied registry cannot become the deployment registry."""
        copied_registry = self.root / "copied-provider-registry.json"
        copied_registry.write_bytes(self.registry.read_bytes())
        payload = self.root / "copied-registry.payload.bin"
        manifest = self.root / "copied-registry.prepare.json"
        args = self._prepare_args(payload, manifest)
        args[args.index("--provider-registry") + 1] = str(copied_registry)
        result = self._run(*args)
        self._assert_rejected_no_output(result, payload, manifest)

    def test_mutated_trust_config_is_not_accepted_by_caller_updated_registry_digest(self) -> None:
        """The registry's consistency digest cannot replace an independent config pin."""
        trusted_config_sha256 = digest_file(self.trust)
        trusted_registry_sha256 = digest_file(self.registry)
        trust = json.loads(self.trust.read_text(encoding="utf-8"))
        trust["network_id"] = "caller-mutated-network"
        write_json(self.trust, trust)
        registry = json.loads(self.registry.read_text(encoding="utf-8"))
        registry["trust_config_sha256"] = digest_file(self.trust)
        write_json(self.registry, registry)
        payload = self.root / "config-anchor.payload.bin"
        manifest = self.root / "config-anchor.prepare.json"
        result = self._run(
            *self._prepare_args(payload, manifest),
            authority_pins=(trusted_config_sha256, trusted_registry_sha256),
        )
        self._assert_rejected_no_output(result, payload, manifest)

    def test_unpatched_cli_rejects_temporary_authority_without_outputs(self) -> None:
        """The real CLI has no test-only deployment-authority bypass."""
        payload = self.root / "unpatched.payload.bin"
        manifest = self.root / "unpatched.prepare.json"
        result = self._run_unpatched(*self._prepare_args(payload, manifest))
        self._assert_rejected_no_output(result, payload, manifest)


class AuthorityAncestorOwnershipTests(unittest.TestCase):
    def test_foreign_owned_parent_rejected_with_safe_directory_controls(self):
        from unittest.mock import patch
        import importlib.util
        spec = importlib.util.spec_from_file_location("protected_owner_test", TOOL)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "public-metadata"
            target.write_bytes(b"public metadata")
            target.chmod(0o644)
            real_stat = Path.stat
            def parent_stat(path, *args, **kwargs):
                metadata = real_stat(path, *args, **kwargs)
                if path == root:
                    values = list(metadata)
                    values[0] = stat.S_IFDIR | mode
                    values[4] = owner
                    return os.stat_result(values)
                return metadata
            for owner, mode, allowed in ((os.getuid() + 1, 0o755, False),
                                         (os.getuid(), 0o777, False),
                                         (os.getuid(), 0o755, True),
                                         (0, 0o1777, True)):
                with self.subTest(owner=owner, mode=mode), patch.object(Path, "stat", parent_stat):
                    if allowed:
                        self.assertEqual(module.read_authority_bytes(target, "public metadata"), b"public metadata")
                    else:
                        with self.assertRaises(module.ToolError):
                            module.read_authority_bytes(target, "public metadata")


if __name__ == "__main__":
    unittest.main()
