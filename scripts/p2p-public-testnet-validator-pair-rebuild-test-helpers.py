#!/usr/bin/env python3
"""Shared fixture and helper base class for validator pair rebuild contract tests."""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
EXECUTOR = ROOT / "scripts" / "p2p-public-testnet-validator-pair-rebuild.py"
WRAPPER = ROOT / "scripts" / "p2p-public-testnet-rebuild-validators.sh"
PROVENANCE = ROOT / "scripts" / "p2p-public-testnet-validator-pair-provenance.py"
RELEASE_BUNDLE = ROOT / "scripts" / "release-candidate-bundle.sh"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_digest(value: dict) -> str:
    body = {key: item for key, item in value.items() if key != "canonical_digest"}
    return hashlib.sha256(json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


CANONICAL_RESET_SURFACES = [
    "data/execution-records",
    "data/execution-world",
    "data/execution-world-simulator-mirror",
    "data/storage",
    "data/runtime-root",
    "data/replication-root",
    "output/chain-runtime",
    "output/node-distfs",
]


FROZEN_HEAD_OID = subprocess.run(
    ["git", "-C", str(ROOT), "rev-parse", "--verify", "HEAD^{commit}"],
    check=True,
    text=True,
    capture_output=True,
).stdout.strip()


class ValidatorPairRebuildContractBase(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory(prefix="oasis7-pair-rebuild-contract-")
        self.root = Path(self.tmp.name)
        self.external_fixture_paths: list[Path] = []
        self._hermetic_environment_restore: dict[str, str | None] = {}
        self.executor_launcher = self.root / "executor-test-launcher.py"
        self.executor_launcher.write_text(
            f"""#!{sys.executable}
import importlib.util
import os
import sys
from pathlib import Path

executor = Path({str(EXECUTOR)!r})
spec = importlib.util.spec_from_file_location("pair_rebuild_test_executor", executor)
if spec is None or spec.loader is None:
    raise SystemExit("unable to load validator-pair executor")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.EXECUTOR_KNOWN_HOSTS_PATH = Path(os.environ["OASIS7_TEST_EXECUTOR_KNOWN_HOSTS_PATH"])
sys.argv = [str(executor), *sys.argv[1:]]
raise SystemExit(module.main())
""",
            encoding="utf-8",
        )
        self.executor_launcher.chmod(0o755)
        self.package = self.root / "package"
        self.package.mkdir()
        runtime = self.package / "oasis7_chain_runtime"
        runtime.write_bytes(b"runtime-261\n")
        self.runtime_sha = sha256(runtime)
        self.runtime_size = runtime.stat().st_size
        (self.package / "BUILDINFO").write_text(
            "run_id=3218\ncommit=" + "a" * 40 + "\npackage_version=0.0.0+testnet.261\n",
            encoding="utf-8",
        )
        (self.package / "SHA256SUMS").write_text(
            f"{self.runtime_sha}  oasis7_chain_runtime\n",
            encoding="utf-8",
        )
        self.governed = self.root / "governed"
        self.governed.mkdir()
        for name, content in {
            "manifest.json": '{"network_id":"oasis7-public-testnet-governed-20260606","chain_id":"oasis7-public-testnet-governed-20260606"}\n',
            "genesis.json": '{"world_id":"oasis7-public-testnet-governed-20260606","chain_id":"oasis7-public-testnet-governed-20260606"}\n',
            "registry.json": '{"validators":[{"node_id":"triad-testnet-sequencer"},{"node_id":"triad-testnet-storage"}]}\n',
            "bootstrap.txt": "/ip4/127.0.0.1/tcp/6831/p2p/peer-sequencer-204\n/ip4/127.0.0.1/tcp/6832/p2p/peer-storage-205\n",
            "world.json": '{"snapshot":true}\n',
        }.items():
            (self.governed / name).write_text(content, encoding="utf-8")
        self.provenance = self.root / "provenance.json"
        self.signing_key = self.root / "attestor-key.pem"
        self.public_key = self.root / "attestor-public.pem"
        self.signature = self.root / "attestor-signature.bin"
        subprocess.run(
            [
                "openssl",
                "genpkey",
                "-algorithm",
                "RSA",
                "-pkeyopt",
                "rsa_keygen_bits:2048",
                "-out",
                str(self.signing_key),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        subprocess.run(
            ["openssl", "pkey", "-in", str(self.signing_key), "-pubout", "-out", str(self.public_key)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self._write_provenance()
        self.trust_root = self.root / "provenance-trust-root.json"
        trust_root = {
            "schema_version": "oasis7.validator_pair_provenance_trust_root.v1",
            "allowlist": [
                {
                    "signer_id": "testnet-package-attestor",
                    "algorithm": "openssl-rsa-sha256",
                    "public_key_sha256": sha256(self.public_key),
                }
            ],
        }
        trust_root["root_digest"] = canonical_digest(trust_root)
        self.trust_root.write_text(json.dumps(trust_root, indent=2) + "\n", encoding="utf-8")
        self.identity_receipts = self.root / "identity-receipts.json"
        identity_paths = []
        for role in ("storage-205", "sequencer-204"):
            identity_path = self.root / f"identity-{role}.json"
            signature_path = self.root / f"identity-{role}.sig"
            self._write_attestation(
                identity_path,
                signature_path,
                "oasis7.validator_identity_receipt.v1",
                role,
                f"peer-{role}",
                node_id="triad-testnet-storage" if role == "storage-205" else "triad-testnet-sequencer",
            )
            identity_paths.append(str(identity_path))
        self.identity_receipts.write_text(json.dumps({"receipts": identity_paths}, indent=2) + "\n", encoding="utf-8")
        self.rebuild_proof = self.root / "sequencer-204-rebuild-proof.json"
        self._write_attestation(
            self.rebuild_proof,
            self.root / "sequencer-204-rebuild-proof.sig",
            "oasis7.validator_pair_rebuild_proof.v1",
            "sequencer-204",
            "peer-sequencer-204",
            "triad-testnet-sequencer",
        )
        self.impact = self.root / "consumer-impact.json"
        self.impact.write_text(
            json.dumps(
                {
                    "impact": "none",
                    "evidence_source": "local contract fixture",
                    "timestamp": "2026-08-18T00:00:00Z",
                    "validators_already_stopped": True,
                    "outage_update_channel": "n/a",
                    "recovery_update_checkpoint": "n/a",
                    "producer_wording_approval": "n/a",
                    "decision": "proceed",
                }
            ),
            encoding="utf-8",
        )
        self.nodes = {}
        for role in ("storage-205", "sequencer-204"):
            node = self.root / role
            (node / "data").mkdir(parents=True)
            (node / "config").mkdir()
            (node / "current" / "bin").mkdir(parents=True)
            (node / "current" / "bin" / "oasis7_chain_runtime").write_bytes(b"runtime-old\n")
            (node / "config" / "node.env").write_text("NODE_ID=" + role + "\n", encoding="utf-8")
            self.nodes[role] = node
        self.capacity = self.root / "capacity.json"
        self.capacity.write_text(
            json.dumps(
                {
                    role: {
                        "free_bytes": 10_000_000,
                        "free_inodes": 100_000,
                        "same_filesystem": True,
                    }
                    for role in self.nodes
                }
            ),
            encoding="utf-8",
        )
        self.quiescence_id = "fixture-quiescence-3481"
        self.quiescence_proof = self.root / "quiescence-proof.json"
        self._write_quiescence_fixture()
        self.out = self.root / "out"

    def tearDown(self) -> None:
        for path in self.external_fixture_paths:
            path.unlink(missing_ok=True)
        for name, previous in self._hermetic_environment_restore.items():
            if previous is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = previous
        self.tmp.cleanup()

    def _install_hermetic_human_direct_environment(self, fixture: dict[str, Path | str]) -> None:
        """Expose the setUp direct-SSH fixture to legacy subprocess callers.

        Older plan/apply tests invoke ``subprocess.run`` directly and do not
        receive the newer helper's explicit environment.  Their persisted
        executor audit still causes a live direct re-observation, so the
        inherited environment must point at the same loopback-only fake GitHub
        and SSH binaries.  Rejection tests continue to pass their own env via
        ``_run_live_human_direct_ssh`` and are unaffected.
        """
        values = {
            "PATH": f"{fixture['bin']}:{os.environ.get('PATH', '')}",
            "HUMAN_DIRECT_SSH_ARGV_LOG": str(fixture["ssh_args"]),
            "HUMAN_DIRECT_SSH_COMMAND_LOG": str(fixture["ssh_commands"]),
            "HUMAN_DIRECT_SSH_SEAM_LOG": str(fixture["ssh_seam"]),
            "HUMAN_DIRECT_SSH_SSHPASS_LOG": str(fixture["sshpass"]),
            "HUMAN_DIRECT_SSH_SYSTEMCTL_LOG": str(fixture["systemctl"]),
            "HUMAN_DIRECT_SSH_SERVICE_LOG": str(fixture["service"]),
            "HUMAN_DIRECT_SSH_FAKE_STATE": str(fixture["state"]),
            "HUMAN_DIRECT_SSH_SECRET": str(fixture["secret"]),
            "OASIS7_VALIDATOR_PAIR_NONCE_LEDGER": str(fixture["nonce_ledger"]),
            "HUMAN_DIRECT_SSH_GITHUB_RESPONSE": str(fixture["github_response"]),
            "HUMAN_DIRECT_SSH_GITHUB_LOG": str(fixture["github_calls"]),
            "OASIS7_TEST_EXECUTOR_KNOWN_HOSTS_PATH": str(fixture["known_hosts"]),
        }
        for name, value in values.items():
            if name not in self._hermetic_environment_restore:
                self._hermetic_environment_restore[name] = os.environ.get(name)
            os.environ[name] = value
        if "HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE" not in self._hermetic_environment_restore:
            self._hermetic_environment_restore["HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE"] = os.environ.get(
                "HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE"
            )
        os.environ.pop("HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE", None)

    def _write_provenance(self, *, signature: bool = True, extra_governed: Path | None = None) -> None:
        governed = {
            name: {
                "path": str(path),
                "sha256": sha256(path),
                "size_bytes": path.stat().st_size,
                "entry_count": 1,
                "link_count": 0,
                "dir_count": 0,
                "file_count": 1,
            }
            for name, path in (
                ("manifest", self.governed / "manifest.json"),
                ("genesis", self.governed / "genesis.json"),
                ("registry", self.governed / "registry.json"),
                ("bootstrap", self.governed / "bootstrap.txt"),
                ("world", self.governed / "world.json"),
            )
        }
        if extra_governed is not None:
            governed["unexpected"] = {
                "path": str(extra_governed),
                "sha256": sha256(extra_governed),
                "size_bytes": extra_governed.stat().st_size,
                "entry_count": 1,
                "link_count": 0,
                "dir_count": 0,
                "file_count": 1,
            }
        payload = {
            "schema_version": "oasis7.validator_pair_rebuild_provenance.v1",
            "network_id": "oasis7-public-testnet-governed-20260606",
            "chain_id": "oasis7-public-testnet-governed-20260606",
            "package": {
                "run_id": "3218",
                "commit": "a" * 40,
                "package_version": "0.0.0+testnet.261",
                "runtime_sha256": self.runtime_sha,
                "runtime_size_bytes": self.runtime_size,
                "buildinfo_sha256": sha256(self.package / "BUILDINFO"),
                "sha256sums_sha256": sha256(self.package / "SHA256SUMS"),
            },
            "governed": governed,
            "signature": (
                {
                    "status": "verified",
                    "signer_id": "testnet-package-attestor",
                    "algorithm": "openssl-rsa-sha256",
                    "signature_ref": str(self.signature),
                    "public_key_ref": str(self.public_key),
                    "public_key_sha256": sha256(self.public_key),
                }
                if signature
                else None
            ),
        }
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
        payload["binding_digest"] = hashlib.sha256(canonical.encode()).hexdigest()
        self.provenance.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        if signature:
            payload_path = self.root / "provenance-signing-payload.bin"
            payload_path.write_bytes(canonical.encode())
            subprocess.run(
                ["openssl", "dgst", "-sha256", "-sign", str(self.signing_key), "-out", str(self.signature), str(payload_path)],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )

    def _write_attestation(self, path: Path, signature_path: Path, schema: str, role: str, peer_id: str, node_id: str) -> None:
        value = {
            "schema_version": schema,
            "role": role,
            "peer_id": peer_id,
            "node_id": node_id,
            "signer_id": "testnet-package-attestor",
            "algorithm": "openssl-rsa-sha256",
            "public_key_ref": str(self.public_key),
            "public_key_sha256": sha256(self.public_key),
            "signature_ref": str(signature_path),
            "trust_root_digest": json.loads(self.trust_root.read_text(encoding="utf-8"))["root_digest"],
        }
        body = {key: item for key, item in value.items() if key not in {"signature_ref", "public_key_ref"}}
        payload_path = path.with_suffix(path.suffix + ".payload")
        payload_path.write_bytes(json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode())
        subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", str(self.signing_key), "-out", str(signature_path), str(payload_path)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        payload_path.unlink()
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")

    def _write_host_adapter(self) -> Path:
        adapter = self.root / "host-adapter.py"
        adapter.write_text(
            """#!/usr/bin/env python3
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

transaction = json.loads(Path(sys.argv[sys.argv.index('--transaction') + 1]).read_text())
phase = sys.argv[sys.argv.index('--phase') + 1]
nodes = {}
for role in transaction['mutation_order']:
    nodes[role] = {
        'role': role,
        'root': transaction['nodes'][role]['root'],
        'active': phase == 'apply',
        'running': phase == 'apply',
        'service_state': 'running' if phase == 'apply' else 'stopped',
        'independently_observed': True,
    }
    if phase == 'quiesce':
        nodes[role].update({'active': False, 'running': False})
    elif phase == 'preflight':
        nodes[role].update({
            'preflight_verified': True,
            'runtime_executable': True,
            'repair_rebuild_helper_executable': True,
            'generated_world_dir_contract': True,
            'governance_registry_importer_executable': True,
            'python_available': True,
            'tar_available': True,
            'systemd_available': True,
            'process_inspection_available': True,
            'preflight_observer_mutation': False,
        })
    elif phase == 'backup':
        nodes[role].update({'backup_verified': True})
    elif phase == 'rollback':
        nodes[role].update({'rollback_verified': True})
    else:
        nodes[role].update({
            'healthz_ok': True,
            'nrestarts': 0,
            'oom_panic_segfault': False,
            'runtime_sha256': transaction['package']['runtime_sha256'],
            'runtime_size_bytes': transaction['package']['runtime_size_bytes'],
            'listeners': ['6631', '6831'] if role == 'sequencer-204' else ['6632', '6832'],
            'full_chain_status_called': False,
        })
binding = transaction['adapter_binding']
evidence = binding['evidence_bindings']
print(json.dumps({
    'schema_version': 'oasis7.validator_pair_rebuild_host_receipt.v2',
    'phase': phase,
    'operation': 'quiesce-only' if phase == 'quiesce' else None,
    'quiescence_id': transaction.get('quiescence_request', {}).get('quiescence_id') if phase == 'quiesce' else None,
    'plan_digest': transaction['plan_digest'],
    'transaction_id': transaction['transaction_id'],
    'request_digest': transaction.get('quiescence_request', {}).get('request_digest') if phase == 'quiesce' else None,
    'impact_record_sha256': transaction.get('quiescence_request', {}).get('impact_record_sha256') if phase == 'quiesce' else None,
    'captured_at': datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z'),
    'mutation_order': transaction['mutation_order'],
    'startup_order': transaction['startup_order'],
    'nodes': nodes,
    'observer_mutation': False,
    'repository_executable': transaction['adapter_binding']['repository_executable'],
    'sequencer_proof_url': transaction['proof']['sequencer_proof_url'],
    'evidence_bindings': evidence,
    'identity_receipts': evidence['identity_receipts'],
    'sequencer_rebuild_proof': evidence['sequencer_rebuild_proof'],
}))
""",
            encoding="utf-8",
        )
        adapter.chmod(0o755)
        return adapter

    def _write_failure_host_adapter(self, phase_log: Path, *failed_phases: str) -> Path:
        """Wrap the valid fixture adapter with deterministic phase failures."""
        adapter = self._write_host_adapter()
        source = adapter.read_text(encoding="utf-8")
        phase_log_literal = repr(str(phase_log))
        failed_phases_literal = repr(set(failed_phases))
        source = source.replace(
            "phase = sys.argv[sys.argv.index('--phase') + 1]\n",
            "phase = sys.argv[sys.argv.index('--phase') + 1]\n"
            f"with Path({phase_log_literal}).open('a', encoding='utf-8') as handle:\n"
            "    handle.write(phase + '\\n')\n"
            f"if phase in {failed_phases_literal}:\n"
            "    raise SystemExit(42)\n",
            1,
        )
        adapter.write_text(source, encoding="utf-8")
        adapter.chmod(0o755)
        return adapter

    def _write_quiescence_fixture(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(
            transaction_id=self.quiescence_id,
            impact_path=self.impact,
        )
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.human_direct_fixture = fixture
        self._install_hermetic_human_direct_environment(fixture)
        self.quiescence_proof = Path(fixture["out"]) / "quiescence-proof.json"
        self.assertTrue(self.quiescence_proof.exists(), result.stdout)

    def _write_quiescence_adapter(self, *, active: bool = False) -> Path:
        adapter = self.root / "quiescence-host-adapter.py"
        adapter.write_text(
            f"""#!/usr/bin/env python3
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

transaction = json.loads(Path(sys.argv[sys.argv.index('--transaction') + 1]).read_text())
nodes = {{}}
for role in transaction['mutation_order']:
    nodes[role] = {{
        'role': role,
        'root': transaction['nodes'][role]['root'],
        'active': {str(active)},
        'running': {str(active)},
        'service_state': 'active' if {str(active)} else 'stopped',
        'independently_observed': True,
    }}
print(json.dumps({{
    'schema_version': 'oasis7.validator_pair_rebuild_quiescence_receipt.v1',
    'phase': 'quiesce',
    'operation': 'quiesce-only',
    'quiescence_id': transaction['quiescence_id'],
    'transaction_id': transaction['transaction_id'],
    'request_digest': transaction['request_digest'],
    'impact_record_sha256': transaction['impact_record_sha256'],
    'captured_at': datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z'),
    'mutation_order': transaction['mutation_order'],
    'startup_order': transaction['startup_order'],
    'nodes': nodes,
    'observer_mutation': False,
}}))
""",
            encoding="utf-8",
        )
        adapter.chmod(0o755)
        return adapter

    def _base_args(self) -> list[str]:
        return [
            str(WRAPPER),
            "plan",
            "--package-dir",
            str(self.package),
            "--provenance",
            str(self.provenance),
            "--trust-root",
            str(self.trust_root),
            "--identity-receipts",
            str(self.identity_receipts),
            "--sequencer-rebuild-proof",
            str(self.rebuild_proof),
            "--sequencer-proof-url",
            "http://127.0.0.1:6631/v1/chain/rebuild-proof",
            "--consumer-impact-record",
            str(self.impact),
            "--stopped-quiescence-proof",
            str(self.quiescence_proof),
            "--quiescence-id",
            self.quiescence_id,
            "--capacity-json",
            str(self.capacity),
            "--node",
            f"storage-205=local:{self.nodes['storage-205']}",
            "--node",
            f"sequencer-204=local:{self.nodes['sequencer-204']}",
            "--out-dir",
            str(self.out),
        ]

    def _apply_args(
        self,
        transaction_path: Path,
        adapter: Path | None = None,
        fixture: dict[str, Path | str] | None = None,
    ) -> list[str]:
        """Build an apply command with explicit hermetic direct-observation routing."""
        direct_fixture = fixture or self.human_direct_fixture
        args = [
            str(WRAPPER),
            "apply",
            "--transaction",
            str(transaction_path),
        ]
        if adapter is not None:
            args.extend(("--host-adapter", str(adapter)))
        args.extend(
            (
                "--request",
                str(direct_fixture["request"]),
                "--known-hosts",
                str(direct_fixture["known_hosts"]),
            )
        )
        return args

    def _write_human_direct_ssh_fixture(
        self,
        *,
        transaction_id: str = "human-direct-ssh-tx-3481",
        impact_path: Path | None = None,
        state: str = "quiet",
        stale: bool = False,
        expired: bool = False,
        replayed: bool = False,
        mismatched: str | None = None,
        full_status: bool = False,
    ) -> dict[str, Path | str]:
        """Build a completely local human_direct_ssh fixture.

        The direct-SSH protocol is deliberately exercised through fake
        executables. A test which accidentally reaches a real ssh, sshpass,
        curl, or systemctl binary therefore fails closed instead of touching
        a developer or testnet host.
        """
        fixture = Path(tempfile.mkdtemp(prefix="human-direct-ssh-", dir=str(self.root)))
        bin_dir = fixture / "bin"
        bin_dir.mkdir(parents=True)
        known_hosts = fixture / "known_hosts"
        known_hosts.write_text(
            "39.104.204.172 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILBk7BgiUTvJZD/LSJLRHUw4eGFFe3eyUs3eG2ZU0xxj\n"
            "39.104.205.67 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILU4G7oGjBJ+tzaZh4nQJjGkPKM1gXQZYT3GWJLMDhZC\n",
            encoding="utf-8",
        )
        known_hosts.chmod(0o600)
        fake_python = bin_dir / "python3"
        fake_python.write_text(
            f"""#!{sys.executable}
import os
import sys
from pathlib import Path

args = sys.argv[1:]
if args and Path(args[0]).resolve() == Path({str(EXECUTOR)!r}).resolve():
    launcher = Path({str(self.executor_launcher)!r})
    os.execv({sys.executable!r}, [{sys.executable!r}, str(launcher), *args[1:]])
os.execv({sys.executable!r}, [{sys.executable!r}, *args])
""",
            encoding="utf-8",
        )
        fake_python.chmod(0o755)
        nonce_ledger = self.root.parent / f"oasis7-human-direct-nonce-{len(self.external_fixture_paths)}.jsonl"
        nonce_ledger.write_text("", encoding="utf-8")
        nonce_ledger.chmod(0o600)
        self.external_fixture_paths.append(nonce_ledger)
        authority = fixture / "stop-authority.json"
        nonce = "human-direct-ssh-nonce-3481"
        issued_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
        issued_at_text = issued_at.isoformat().replace("+00:00", "Z")
        expires_at_text = (issued_at + dt.timedelta(minutes=15)).isoformat().replace("+00:00", "Z")
        authority_payload = {
            "schema_version": "oasis7.human_stop_authority.v1",
            "task_uid": "task_0aa7659e0fd74e1a8e4bb27d7dc2416a",
            "transaction_id": transaction_id,
            "nonce": nonce,
            "authorized": True,
            "stopped_at": issued_at_text,
            "actor": "human-operator-fixture",
            "source": "live_github_task_readback",
            "authenticated": True,
            "live_readback": True,
            "repository": "eng-cc/oasis7",
            "issue_number": 3481,
            "action": "validator_pair_rebuild_stop",
            "targets": ["storage-205", "sequencer-204"],
        }
        authority.write_text(json.dumps(authority_payload, sort_keys=True) + "\n", encoding="utf-8")
        request = {
            "schema_version": "oasis7.human_direct_ssh_request.v1",
            "mode": "human_direct_ssh",
            "task_uid": authority_payload["task_uid"],
            "transaction_id": transaction_id,
            "request_digest": "",
            "nonce": nonce,
            "issued_at": issued_at_text,
            "expires_at": expires_at_text,
            "stop_authority_path": str(authority),
            "stop_authority_sha256": sha256(authority),
            "inventory_path": str(ROOT / "scripts" / "public-testnet-validator-pair-inventory.v1.json"),
            "inventory_sha256": sha256(ROOT / "scripts" / "public-testnet-validator-pair-inventory.v1.json"),
            "nonce_ledger_path": str(nonce_ledger),
            "known_hosts_path": str(known_hosts),
            "observer_mutation": False,
            "quiescence_id": transaction_id,
            "impact_record_path": str(impact_path) if impact_path is not None else None,
            "impact_record_sha256": sha256(impact_path) if impact_path is not None else None,
            "hosts": {
                "sequencer": {
                    "role": "sequencer",
                    "host": "root@39.104.204.172",
                    "root": "/opt/oasis7/p2p-testnet",
                    "service": "oasis7-triad-sequencer.service",
                    "host_key_fingerprint": "SHA256:7NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
                },
                "storage": {
                    "role": "storage",
                    "host": "root@39.104.205.67",
                    "root": "/opt/oasis7/p2p-testnet",
                    "service": "oasis7-triad-storage.service",
                    "host_key_fingerprint": "SHA256:1SVgiaT5JLCw8PsPpVfLE9UyWNf82IJDZsiE7LAa1gI",
                },
            },
            "readback": {
                "process_command": "ps -eo pid=,args=",
                "listener_command": "ss -ltn",
                "service_readback_command": "/opt/oasis7/p2p-testnet/current/bin/service-readback --read-only",
                "quiet_window_seconds": 2,
            },
        }
        if stale:
            request["issued_at"] = "2000-01-01T00:00:00Z"
            request["expires_at"] = "2000-01-01T00:01:00Z"
        if expired:
            request["expires_at"] = "2000-01-01T00:01:00Z"
        if replayed:
            request["nonce"] = "human-direct-ssh-replayed-nonce"
            request["replay_of_transaction_id"] = "human-direct-ssh-tx-3479"
        if mismatched == "transaction":
            request["transaction_id"] = "human-direct-ssh-tx-other"
        elif mismatched == "request":
            request["request_digest"] = "f" * 64
        elif mismatched == "fingerprint":
            request["hosts"]["sequencer"]["host_key_fingerprint"] = "SHA256:not-the-pinned-key"
        elif mismatched == "role":
            request["hosts"]["sequencer"]["role"] = "root"
        elif mismatched == "root":
            request["hosts"]["sequencer"]["root"] = "/tmp/caller-owned-root"
        elif mismatched == "service":
            request["hosts"]["sequencer"]["service"] = "caller-owned.service"
        elif mismatched == "command":
            request["readback"]["process_command"] = "echo caller-owned-command"
        elif mismatched == "observer":
            request["observer_mutation"] = True
        if full_status:
            request["full_status_url"] = "http://sequencer.example/v1/chain/status"
            request["full_status_http_status"] = 204
        body_value = {
            "schema_version": "oasis7.validator_pair_rebuild_live_authority.v1",
            "authorized": True,
            "task_uid": authority_payload["task_uid"],
            "action": "validator_pair_rebuild",
            "transaction_id": authority_payload["transaction_id"],
            "nonce": authority_payload["nonce"],
            "inventory_sha256": request["inventory_sha256"],
            "head_oid": FROZEN_HEAD_OID,
            "issued_at": issued_at_text,
            "expires_at": expires_at_text,
        }
        body = json.dumps(body_value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
        request["github_live"] = {
            "provider": "github",
            "repository": "eng-cc/oasis7",
            "issue_number": 3481,
            "comment_id": 5452142815,
            "actor": "human-operator-fixture",
            "body_sha256": hashlib.sha256(body.encode()).hexdigest(),
            "task_uid": request["task_uid"],
            "action": "validator_pair_rebuild",
            "transaction_id": request["transaction_id"],
            "nonce": request["nonce"],
            "inventory_sha256": request["inventory_sha256"],
            "head_oid": body_value["head_oid"],
            "issued_at": request["issued_at"],
            "expires_at": request["expires_at"],
        }
        if not request["request_digest"]:
            request["request_digest"] = hashlib.sha256(
                json.dumps(
                    {key: value for key, value in request.items() if key != "request_digest"},
                    ensure_ascii=True,
                    sort_keys=True,
                    separators=(",", ":"),
                ).encode()
            ).hexdigest()
        request_path = fixture / "request.json"
        request_path.write_text(json.dumps(request, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        out_dir = fixture / "out"
        out_dir.mkdir()
        ssh_args_log = fixture / "ssh-argv.jsonl"
        ssh_command_log = fixture / "ssh-command.log"
        ssh_seam_log = fixture / "ssh-seam.jsonl"
        sshpass_log = fixture / "sshpass-argv.jsonl"
        systemctl_log = fixture / "systemctl.log"
        service_log = fixture / "service-readback.log"
        receipt_path = out_dir / "receipt.json"
        fake_ssh = bin_dir / "ssh"
        fake_ssh.write_text(
            """#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]
secret = os.environ.get("HUMAN_DIRECT_SSH_SECRET", "")
argv_log = Path(os.environ["HUMAN_DIRECT_SSH_ARGV_LOG"])
command_log = Path(os.environ["HUMAN_DIRECT_SSH_COMMAND_LOG"])
seam_log = Path(os.environ["HUMAN_DIRECT_SSH_SEAM_LOG"])
with argv_log.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(args, sort_keys=True) + "\\n")
command = args[-1] if args else ""
with command_log.open("a", encoding="utf-8") as handle:
    handle.write(command + "\\n")
with seam_log.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({
        "environment_binding_present": bool(secret),
        "temporary_fd_present": bool(os.environ.get("HUMAN_DIRECT_SSH_SECRET_FD")),
    }) + "\\n")
if secret and any(secret in value for value in args):
    print("fixture rejected secret in ssh argv", file=sys.stderr)
    raise SystemExit(97)
state = os.environ.get("HUMAN_DIRECT_SSH_FAKE_STATE", "quiet")
count = len(command_log.read_text(encoding="utf-8").splitlines())
service_active = state == "active"
process_active = state in ("active", "active-process") or (
    state == "canonical-process-flapping" and count >= 8
)
listener_active = state in ("active", "active-listener") or (
    state == "canonical-listener-flapping" and count >= 9
)
if "ps -eo" in command or "pgrep" in command:
    print("1234 oasis7_chain_runtime --active" if process_active else (f"quiet-observation-{count}" if state == "flapping" else ""))
elif "ss -ltn" in command or "lsof" in command:
    print("LISTEN 0 128 0.0.0.0:6631" if listener_active else "")
else:
    print(json.dumps({
        "schema_version": "oasis7.human_direct_ssh_readback.v1",
        "active": service_active,
        "running": service_active,
        "listeners": ["6631"] if service_active else [],
        "service_state": "active" if service_active else "stopped",
        "independently_observed": True,
    }, sort_keys=True))
""",
            encoding="utf-8",
        )
        fake_ssh.chmod(0o755)
        fake_sshpass = bin_dir / "sshpass"
        fake_sshpass.write_text(
            """#!/usr/bin/env python3
import json
import os
import shutil
import sys
from pathlib import Path

with Path(os.environ["HUMAN_DIRECT_SSH_SSHPASS_LOG"]).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({"argv": sys.argv[1:], "secret_env": bool(os.environ.get("SSHPASS"))}) + "\\n")
if len(sys.argv) < 2 or sys.argv[1] != "-e":
    raise SystemExit(96)
os.execv(shutil.which("ssh") or "ssh", sys.argv[2:])
""",
            encoding="utf-8",
        )
        fake_sshpass.chmod(0o755)
        fake_systemctl = bin_dir / "systemctl"
        fake_systemctl.write_text(
            """#!/usr/bin/env python3
import os
import sys
from pathlib import Path

with Path(os.environ["HUMAN_DIRECT_SSH_SYSTEMCTL_LOG"]).open("a", encoding="utf-8") as handle:
    handle.write(" ".join(sys.argv[1:]) + "\\n")
raise SystemExit(97)
""",
            encoding="utf-8",
        )
        fake_systemctl.chmod(0o755)
        fake_service = bin_dir / "service-readback"
        fake_service.write_text(
            """#!/usr/bin/env python3
import json
import os
from pathlib import Path

Path(os.environ["HUMAN_DIRECT_SSH_SERVICE_LOG"]).open("a", encoding="utf-8").close()
print(json.dumps({"read_only": True, "state": os.environ.get("HUMAN_DIRECT_SSH_FAKE_STATE", "quiet")}))
""",
            encoding="utf-8",
        )
        fake_service.chmod(0o755)
        fake_curl = bin_dir / "curl"
        fake_curl.write_text(
            """#!/usr/bin/env python3
import sys
print("curl is forbidden in human_direct_ssh fixture", file=sys.stderr)
raise SystemExit(98)
""",
            encoding="utf-8",
        )
        fake_curl.chmod(0o755)
        github_response = fixture / "github-live-comment.json"
        github_response.write_text(
            json.dumps(
                {
                    "id": 5452142815,
                    "user": {"login": "human-operator-fixture"},
                    "body": body,
                    "created_at": issued_at_text,
                    "updated_at": issued_at_text,
                    "deleted": False,
                    "revoked": False,
                    "author_association": "MEMBER",
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        github_calls = fixture / "github-api-argv.jsonl"
        fake_gh = bin_dir / "gh"
        fake_gh.write_text(
            """#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

with Path(os.environ["HUMAN_DIRECT_SSH_GITHUB_LOG"]).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(sys.argv[1:], sort_keys=True) + "\\n")
if os.environ.get("HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE") == "1":
    print("fake GitHub API unavailable", file=sys.stderr)
    raise SystemExit(73)
print(Path(os.environ["HUMAN_DIRECT_SSH_GITHUB_RESPONSE"]).read_text(encoding="utf-8"), end="")
""",
            encoding="utf-8",
        )
        fake_gh.chmod(0o755)
        return {
            "fixture": fixture,
            "bin": bin_dir,
            "known_hosts": known_hosts,
            "nonce_ledger": nonce_ledger,
            "request": request_path,
            "out": out_dir,
            "ssh_args": ssh_args_log,
            "ssh_commands": ssh_command_log,
            "ssh_seam": ssh_seam_log,
            "sshpass": sshpass_log,
            "systemctl": systemctl_log,
            "service": service_log,
            "github_response": github_response,
            "github_calls": github_calls,
            "receipt": receipt_path,
            "secret": "human-direct-ssh-fixture-secret-3481",
            "state": state,
        }

    def _human_direct_ssh_env(self, fixture: dict[str, Path | str]) -> dict[str, str]:
        env = os.environ.copy()
        env["PATH"] = f"{fixture['bin']}:{env.get('PATH', '')}"
        env["HUMAN_DIRECT_SSH_ARGV_LOG"] = str(fixture["ssh_args"])
        env["HUMAN_DIRECT_SSH_COMMAND_LOG"] = str(fixture["ssh_commands"])
        env["HUMAN_DIRECT_SSH_SEAM_LOG"] = str(fixture["ssh_seam"])
        env["HUMAN_DIRECT_SSH_SSHPASS_LOG"] = str(fixture["sshpass"])
        env["HUMAN_DIRECT_SSH_SYSTEMCTL_LOG"] = str(fixture["systemctl"])
        env["HUMAN_DIRECT_SSH_SERVICE_LOG"] = str(fixture["service"])
        env["HUMAN_DIRECT_SSH_FAKE_STATE"] = str(fixture["state"])
        env["HUMAN_DIRECT_SSH_SECRET"] = str(fixture["secret"])
        env["OASIS7_VALIDATOR_PAIR_NONCE_LEDGER"] = str(fixture["nonce_ledger"])
        env["HUMAN_DIRECT_SSH_GITHUB_RESPONSE"] = str(fixture["github_response"])
        env["HUMAN_DIRECT_SSH_GITHUB_LOG"] = str(fixture["github_calls"])
        env["OASIS7_TEST_EXECUTOR_KNOWN_HOSTS_PATH"] = str(fixture["known_hosts"])
        return env

    def _human_direct_ssh_args(self, fixture: dict[str, Path | str], *extra: str) -> list[str]:
        return [
            str(WRAPPER),
            "human_direct_ssh",
            "--request",
            str(fixture["request"]),
            "--known-hosts",
            str(fixture["known_hosts"]),
            "--out-dir",
            str(fixture["out"]),
            "--credential-env",
            "HUMAN_DIRECT_SSH_SECRET",
            *extra,
        ]

    def _run_human_direct_ssh(self, fixture: dict[str, Path | str], *extra: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            self._human_direct_ssh_args(fixture, *extra),
            text=True,
            capture_output=True,
            env=self._human_direct_ssh_env(fixture),
        )
