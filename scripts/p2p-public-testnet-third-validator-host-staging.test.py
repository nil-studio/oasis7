#!/usr/bin/env python3
"""PG-02R-B GREEN contract for validator-47 host staging.

This file is intentionally tests-only.  It freezes the operator-side
contract required before validator-47 can be staged on the empty host:

* the triad inventory is an authority input and its byte digest is retained;
* the staged node.env is bound to validator-47 identity, role, ports,
  provider capability, world, manifest, and registry truth;
* stale pair identity and target drift are rejected before materialization;
* readback proves disabled plus inactive/dead/no-process/no-listener state; and
* the stable runbook and fresh-host manual provide an executable no-start,
  pair-preserving, cold-cutover, clean-redeploy rollback path.

The implementation now supplies these validator-47 host contracts.  The tests
must remain strict through future changes; in particular, a staged environment
must remain executable by the isolated launcher before any live start is
considered.
"""

from __future__ import annotations

import contextlib
import hashlib
import importlib.util
from importlib.machinery import SourceFileLoader
import io
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "scripts/public-testnet-validator-triad-inventory.v1.json"
STAGE = ROOT / "scripts/p2p-public-testnet-build-deployment-stage.sh"
BOOTSTRAP = ROOT / "scripts/p2p-public-testnet-bootstrap-fresh-validator-host.sh"
FLEET_HEALTH = ROOT / "scripts/p2p-public-testnet-fleet-health.py"
SERVICE_READBACK = ROOT / "scripts/service-readback"
START_NODE = ROOT / "scripts/p2p-triad-node-start.sh"
RUNBOOK = ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md"
RUNBOOK_PHASE_C = ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap-phase-c-rebuild.runbook.md"
MANUAL = ROOT / "doc/testing/manual/public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md"

INVENTORY_RELATIVE = "scripts/public-testnet-validator-triad-inventory.v1.json"
VALIDATOR_47_NODE_ID = "triad-testnet-validator-47"
VALIDATOR_47_SERVICE = "oasis7-triad-validator-47.service"
VALIDATOR_47_FINALITY_PUBLIC_KEY = "cf8c9c2b5637d20d0efa585f0fb7f503b19a1aaba02fb807637e67ed40919fc2"
VALIDATOR_47_ROOT_PUBLIC_KEY = "b21137667506c6c9d5eb30e2cefac73950396d1a58e70665cfdb5afec8943ec6"
VALIDATOR_47_PEER_ID = "12D3KooWCdQLY6Qm9sWqPqEhJTmPdY3Ykw1w5QnTh7qmSgYDazQZ"
GENERATED_REGISTRY_SHA256 = "8bfb4411f3895ab5f1a2a3de1bcaa08ce97567202d4198444b323ef437a88f78"
GENERATED_REGISTRY_SEMANTIC_SHA256 = "aa6f6f7f367470d3b2c7282d489422d3eef14370446aa1fe8420fc94d776950d"
BOOTSTRAP_PEERS_EVIDENCE = (
    ROOT
    / "doc/testing/evidence/"
    / "public-testnet-governed-bootstrap-validator-triad-bootstrap-peers-2026-09-15.txt"
)
SOURCE_REGISTRY_EVIDENCE = (
    ROOT
    / "doc/testing/evidence/"
    / "public-testnet-governed-bootstrap-validator-triad-registry-2026-09-15.json"
)
VALIDATOR_47_ENV = {
    "NODE_ID": VALIDATOR_47_NODE_ID,
    # ``storage`` is the executable runtime role; validator admission and
    # full-storage provider capability are separate P2P/consensus claims.
    "NODE_ROLE": "storage",
    "P2P_NODE_ROLE": "full_storage",
    "STATUS_BIND": "0.0.0.0:6634",
    "NODE_GOSSIP_BIND": "0.0.0.0:6834",
    "WORLD_ID": "oasis7-public-testnet-governed-20260606",
    "NETWORK_TIER_MANIFEST_PATH": "config/public-testnet-governed-bootstrap-manifest-2026-06-06.json",
    "GENESIS_VALIDATOR_REGISTRY_PATH": "config/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json",
    "EXECUTION_WORLD_DIR": "staged-world",
}

# These are the variables that p2p-triad-node-start.sh expands without a
# fallback.  The test intentionally removes any same-named inherited values
# before invoking the launcher so a generated node.env must provide them.
LAUNCHER_REQUIRED_ENV = (
    "CONFIG_PATH",
    "NODE_ID",
    "WORLD_ID",
    "STATUS_BIND",
    "NODE_ROLE",
    "EXECUTION_WORLD_DIR",
    "EXECUTION_RECORDS_DIR",
    "STORAGE_ROOT",
    "STORAGE_PROFILE",
    "NODE_TICK_MS",
    "POS_SLOT_DURATION_MS",
    "POS_TICKS_PER_SLOT",
    "POS_PROPOSAL_TICK_PHASE",
    "POS_MAX_PAST_SLOT_LAG",
    "REWARD_RUNTIME_EPOCH_DURATION_SECS",
    "REWARD_POINTS_PER_CREDIT",
    "NODE_GOSSIP_BIND",
)
VALIDATOR_47_REPLICATION_LISTEN = "/ip4/0.0.0.0/tcp/6834"


def load_module(path: Path, name: str):
    loader = SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load readback target: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def generated_registry_bytes() -> bytes:
    """Return the exact deterministic triad registry emitted by staging."""
    inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
    validators = []
    for name in ("sequencer-204", "storage-205", "validator-47"):
        node = inventory["nodes"][name]
        entry = {
            "node_id": node["node_id"],
            "scheme": "ed25519",
            "finality_signer_public_key": node["finality_signer_public_key"],
            "stake": node["stake"],
        }
        if name == "validator-47":
            entry.update(
                {
                    "root_public_key": node["root_public_key"],
                    "finality_public_key": node["finality_public_key"],
                    "libp2p_peer_id": node["libp2p_peer_id"],
                }
            )
        validators.append(entry)
    return (
        json.dumps(
            {
                "slot_id": "governance.finality.v1",
                "threshold": 2,
                "threshold_bps": 0,
                "quorum": {
                    "numerator": 2,
                    "denominator": 3,
                    "total_stake": 300,
                    "required_stake": 200,
                },
                "governance": {"signer_count": 3, "threshold": 2, "threshold_bps": 6667},
                "validators": validators,
            },
            ensure_ascii=True,
            indent=2,
        )
        + "\n"
    ).encode("utf-8")


def write_fake_readback_tools(
    bin_dir: Path,
    *,
    unit_file_state: str,
    main_pid: str = "0",
    ps_output: str = "  PID COMMAND\n",
) -> None:
    """Create deterministic read-only command fixtures; never touch a host."""
    systemctl = bin_dir / "systemctl"
    systemctl.write_text(
        "#!/bin/sh\n"
        "set -eu\n"
        "case \"${1:-}\" in\n"
        "  show)\n"
        "    printf '%s\\n' \\\n"
        "      LoadState=loaded \\\n"
        "      ActiveState=inactive \\\n"
        "      SubState=dead \\\n"
        f"      UnitFileState={unit_file_state} \\\n"
        f"      MainPID={main_pid} \\\n"
        "      NRestarts=0\n"
        "    ;;\n"
        f"  is-enabled) printf '%s\\n' {unit_file_state} ;;\n"
        "  is-active) printf '%s\\n' inactive ;;\n"
        "  *) exit 2 ;;\n"
        "esac\n",
        encoding="utf-8",
    )
    (bin_dir / "ps").write_text(
        "#!/bin/sh\n"
        + "cat <<'EOF'\n"
        + ps_output
        + "EOF\n",
        encoding="utf-8",
    )
    (bin_dir / "ss").write_text(
        "#!/bin/sh\nprintf 'State Recv-Q Send-Q Local Address:Port Peer Address:Port\\n'\n",
        encoding="utf-8",
    )
    for path in bin_dir.iterdir():
        path.chmod(path.stat().st_mode | stat.S_IXUSR)


def write_stage_identity_fixture(root: Path, receipt: dict[str, str]) -> tuple[Path, Path]:
    """Create an already-staged identity and read-only runtime fixture."""
    root.mkdir(parents=True, exist_ok=True)
    identity_dir = root / "identity"
    identity_dir.mkdir()
    identity_dir.chmod(0o700)
    key_path = identity_dir / "node-keypair.toml"
    key_path.write_text("already-staged-validator-47-key\n", encoding="utf-8")
    key_path.chmod(0o600)
    receipt_path = identity_dir / "identity-receipt.json"
    receipt_path.write_text(json.dumps(receipt) + "\n", encoding="utf-8")
    receipt_path.chmod(0o600)

    runtime = root / "oasis7_chain_runtime"
    runtime.write_text(
        "#!/usr/bin/env bash\n"
        "set -eu\n"
        "[[ \"${1:-}\" == identity-receipt ]] || exit 64\n"
        "python3 - \"$3\" <<'PY'\n"
        "import hashlib, json, pathlib, sys\n"
        "config_dir = pathlib.Path(sys.argv[1])\n"
        "receipt = json.loads((config_dir / 'identity-receipt.json').read_text())\n"
        "key_path = config_dir / 'node-keypair.toml'\n"
        "print(json.dumps({'schema_version':'oasis7.identity_receipt.v1',"
        "'node_id':receipt['node_id'], 'peer_id':receipt['libp2p_peer_id'],"
        "'key_path':str(key_path),"
        "'key_sha256':hashlib.sha256(key_path.read_bytes()).hexdigest(),"
        "'key_size_bytes':key_path.stat().st_size, 'key_mode':384,"
        "'key_uid':key_path.stat().st_uid, 'key_gid':key_path.stat().st_gid}))\n"
        "PY\n",
        encoding="utf-8",
    )
    runtime.chmod(0o755)
    return identity_dir, runtime


def run_validator_47_stage(
    temp: Path,
    *,
    receipt: dict[str, str],
    bootstrap_peers: Path = BOOTSTRAP_PEERS_EVIDENCE,
) -> subprocess.CompletedProcess[str]:
    identity_dir, runtime = write_stage_identity_fixture(temp, receipt)
    output = temp / "stage"
    return subprocess.run(
        [
            str(STAGE),
            "--runtime-build-ref",
            str(runtime),
            "--bootstrap-peers-file",
            str(bootstrap_peers),
            "--sequencer-finality-public-key",
            "e01e5c34dee2da3087653bc4cec02be01632f56250a800994c96ea44ae6f3690",
            "--storage-finality-public-key",
            "1f530cae002d7adb9a6c3dd8f4bc861226f112f88fdd252b28b6494019e21c33",
            "--extra-validator",
            f"triad-testnet-validator-47:{VALIDATOR_47_FINALITY_PUBLIC_KEY}:100",
            "--validator-47-identity-dir",
            str(identity_dir),
            "--out-dir",
            str(output),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def parse_env_file(path: Path) -> dict[str, str]:
    """Parse the generated simple KEY=value node.env contract."""
    values: dict[str, str] = {}
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition("=")
        if not separator or not key or key in values:
            raise AssertionError(f"invalid or duplicate node.env line {line_number}: {raw_line!r}")
        values[key] = value
    return values


class Validator47HostStagingContractTests(unittest.TestCase):
    def test_stage_identity_binding_is_before_output_materialization(self) -> None:
        """A public identity mismatch must fail before the stage root is touched."""
        source = STAGE.read_text(encoding="utf-8")
        preflight = source.index("validator_47_preflight_identity_receipt")
        output_cleanup = source.index('rm -rf "$out_dir"')
        self.assertLess(preflight, output_cleanup)
        self.assertIn("staged public identity does not match supplied governed signer", source)

    def test_inventory_is_authority_and_digest_is_consumed_by_operator_paths(self) -> None:
        """Stage/bootstrap/health/readback must consume one immutable inventory."""
        # Keep the expected authority shape local and deterministic.  The
        # production inventory is deliberately checked separately so a test
        # fixture cannot silently become an authority substitute.
        fixture_inventory = {
            "schema_version": "oasis7.public_testnet_validator_triad_inventory.v1",
            "network_tier": "public_testnet",
            "topology": "three_equal_validator",
            "nodes": {
                "validator-47": {
                    "node_id": VALIDATOR_47_NODE_ID,
                    "roles": ["validator", "checkpoint_provider", "full_storage_provider"],
                    "service": VALIDATOR_47_SERVICE,
                    "ports": ["6634", "6834"],
                }
            },
        }
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-inventory-") as temp_dir:
            fixture_path = Path(temp_dir) / "triad-inventory.json"
            fixture_path.write_text(
                json.dumps(fixture_inventory, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            fixture_digest = hashlib.sha256(fixture_path.read_bytes()).hexdigest()
        self.assertEqual(len(fixture_digest), 64)

        self.assertTrue(
            INVENTORY.is_file(),
            f"contract: missing governed triad inventory authority: {INVENTORY}",
        )
        self.assertFalse(INVENTORY.is_symlink(), "triad inventory authority must not be a symlink")
        value = json.loads(INVENTORY.read_text(encoding="utf-8"))
        self.assertEqual(value.get("schema_version"), fixture_inventory["schema_version"])
        self.assertEqual(value.get("network_tier"), "public_testnet")
        self.assertEqual(value.get("topology"), "three_equal_validator")

        for path in (STAGE, BOOTSTRAP, FLEET_HEALTH, SERVICE_READBACK):
            source = path.read_text(encoding="utf-8")
            self.assertTrue(
                INVENTORY_RELATIVE in source,
                f"contract: {path.name} does not consume the triad inventory authority",
            )
            self.assertTrue(
                re.search(r"(?i)(sha256|digest)", source),
                f"contract: {path.name} does not retain an inventory digest binding",
            )

    def test_stage_contract_renders_validator47_node_env_bindings(self) -> None:
        """The deployment stage must emit a complete validator-47 env contract."""
        source = STAGE.read_text(encoding="utf-8")
        self.assertTrue("node.env" in source, "contract: stage does not render or validate node.env")
        self.assertTrue(
            VALIDATOR_47_SERVICE in source,
            "contract: stage does not bind the validator-47 systemd service",
        )

    def test_stage_emits_storage_runtime_and_full_storage_p2p_roles(self) -> None:
        """The materialized node.env must use executable role claims."""
        receipt = {
            "schema_version": "oasis7.identity_provision.v1",
            "node_id": VALIDATOR_47_NODE_ID,
            "root_public_key": VALIDATOR_47_ROOT_PUBLIC_KEY,
            "finality_public_key": VALIDATOR_47_FINALITY_PUBLIC_KEY,
            "libp2p_peer_id": VALIDATOR_47_PEER_ID,
        }
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-emitted-env-") as temp_dir:
            temp = Path(temp_dir)
            result = run_validator_47_stage(temp, receipt=receipt)
            self.assertEqual(result.returncode, 0, result.stderr)
            staged_env = (temp / "stage/config/node.env").read_text(encoding="utf-8")
            self.assertIn("NODE_ROLE=storage\n", staged_env)
            self.assertIn("P2P_NODE_ROLE=full_storage\n", staged_env)
            self.assertNotIn("NODE_ROLE=validator\n", staged_env)

    def test_generated_stage_env_survives_bootstrap_and_launcher_dry_run(self) -> None:
        """Generated env must dry-run and reach the isolated runtime command."""
        receipt = {
            "schema_version": "oasis7.identity_provision.v1",
            "node_id": VALIDATOR_47_NODE_ID,
            "root_public_key": VALIDATOR_47_ROOT_PUBLIC_KEY,
            "finality_public_key": VALIDATOR_47_FINALITY_PUBLIC_KEY,
            "libp2p_peer_id": VALIDATOR_47_PEER_ID,
        }
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-launcher-env-") as temp_dir:
            temp = Path(temp_dir)
            result = run_validator_47_stage(temp, receipt=receipt)
            self.assertEqual(result.returncode, 0, result.stderr)

            stage = temp / "stage"
            staged_config = stage / "config"
            staged_env_path = staged_config / "node.env"
            staged_values = parse_env_file(staged_env_path)
            for key in LAUNCHER_REQUIRED_ENV:
                self.assertIn(key, staged_values, f"generated node.env missing {key}")
                self.assertTrue(staged_values[key], f"generated node.env has empty {key}")
            self.assertEqual(
                staged_values.get("REPLICATION_NETWORK_LISTEN_ADDRS_CSV"),
                VALIDATOR_47_REPLICATION_LISTEN,
                "generated node.env must bind validator-47 replication on its gossip port",
            )
            self.assertEqual(staged_values["NODE_ID"], VALIDATOR_47_NODE_ID)
            self.assertEqual(staged_values["NODE_ROLE"], "storage")
            self.assertEqual(staged_values["P2P_NODE_ROLE"], "full_storage")
            self.assertEqual(staged_values["EXECUTION_WORLD_DIR"], "staged-world")

            # Mirror the bootstrap's config/world materialization into an
            # isolated stack.  No host or service-manager operation occurs.
            stack = temp / "stack"
            stack_config = stack / "config"
            shutil.copytree(staged_config, stack_config)
            shutil.copytree(stage / "generated-world", stack / "staged-world")
            for name in ("node-keypair.toml", "identity-receipt.json"):
                shutil.copy2(stage / "identity" / name, stack_config / name)
            self.assertEqual(
                staged_env_path.read_bytes(),
                (stack_config / "node.env").read_bytes(),
                "bootstrap-shaped config must preserve generated node.env bytes",
            )

            runtime = stack / "current/bin/oasis7_chain_runtime"
            runtime.parent.mkdir(parents=True)
            runtime_args = stack / "runtime-args.txt"
            runtime.write_text(
                "#!/usr/bin/env bash\n"
                "set -eu\n"
                "printf '%s\\n' \"$@\" > \"$RUNTIME_ARG_CAPTURE\"\n",
                encoding="utf-8",
            )
            runtime.chmod(0o755)
            launcher_env = os.environ.copy()
            for key in (
                set(LAUNCHER_REQUIRED_ENV)
                | {"REPLICATION_NETWORK_LISTEN_ADDRS_CSV", "APP_ROOT", "ENV_FILE", "RELEASE_LINK", "BIN"}
            ):
                launcher_env.pop(key, None)
            launcher_env.update(
                {
                    "APP_ROOT": str(stack),
                    "ENV_FILE": str(stack_config / "node.env"),
                    "OASIS7_NODE_START_DRY_RUN": "1",
                    "RUNTIME_ARG_CAPTURE": str(runtime_args),
                }
            )
            launcher = subprocess.run(
                [str(START_NODE)],
                cwd=stack,
                env=launcher_env,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(launcher.returncode, 0, launcher.stderr)
            self.assertIn("runtime command:", launcher.stdout)
            for flag in (
                "--node-id",
                "--world-id",
                "--storage-profile",
                "--status-bind",
                "--node-role",
                "--node-tick-ms",
                "--pos-slot-duration-ms",
                "--pos-ticks-per-slot",
                "--pos-proposal-tick-phase",
                "--pos-max-past-slot-lag",
                "--config",
                "--execution-world-dir",
                "--execution-records-dir",
                "--storage-root",
                "--reward-runtime-epoch-duration-secs",
                "--reward-points-per-credit",
                "--node-gossip-bind",
                "--replication-network-listen",
                "--network-tier-manifest",
                "--genesis-validator-registry",
                "--deployment-inventory",
            ):
                self.assertIn(flag, launcher.stdout, f"launcher dry-run omitted {flag}")

            launcher_env.pop("OASIS7_NODE_START_DRY_RUN")
            launcher = subprocess.run(
                [str(START_NODE)],
                cwd=stack,
                env=launcher_env,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(launcher.returncode, 0, launcher.stderr)
            self.assertTrue(runtime_args.is_file(), "launcher did not reach the fake runtime command")
            runtime_argv = runtime_args.read_text(encoding="utf-8").splitlines()
            self.assertIn("--replication-network-listen", runtime_argv)
            replication_index = runtime_argv.index("--replication-network-listen")
            self.assertEqual(
                runtime_argv[replication_index + 1],
                VALIDATOR_47_REPLICATION_LISTEN,
                "launcher passed an unexpected replication listen address",
            )

    def test_identity_import_is_explicit_byte_preserving_and_registry_bound(self) -> None:
        """The final stack must import, never regenerate, validator-47 identity."""
        stage_source = STAGE.read_text(encoding="utf-8")
        bootstrap_source = BOOTSTRAP.read_text(encoding="utf-8")
        readback_source = SERVICE_READBACK.read_text(encoding="utf-8")
        for marker in (
            "--validator-47-identity-dir",
            "identity-receipt.json",
            "identity-receipt",
            "regular file",
            "mode 0600",
            "ownership",
            "bytes changed",
            "does not match governed registry",
        ):
            self.assertTrue(
                marker in stage_source or marker in bootstrap_source or marker in readback_source,
                f"identity import contract missing marker {marker}",
            )
        self.assertIn("--identity-dir", bootstrap_source)
        self.assertIn("install -m 0600", bootstrap_source)
        self.assertIn("identity-receipt", bootstrap_source)
        self.assertIn("identity_descriptor", readback_source)
        self.assertIn("finality_public_key", readback_source)
        self.assertIn("validator-47 public identity", readback_source)

    def test_stage_rejects_self_consistent_but_wrong_validator_identity_fields(self) -> None:
        """A coherent receipt is insufficient when root/finality/peer drift from inventory."""
        base_receipt = {
            "schema_version": "oasis7.identity_provision.v1",
            "node_id": VALIDATOR_47_NODE_ID,
            "root_public_key": VALIDATOR_47_ROOT_PUBLIC_KEY,
            "finality_public_key": VALIDATOR_47_FINALITY_PUBLIC_KEY,
            "libp2p_peer_id": VALIDATOR_47_PEER_ID,
        }
        mutations = {
            "node": {"node_id": "attacker-validator"},
            "root": {"root_public_key": "00" * 32},
            "finality": {"finality_public_key": "11" * 32},
            "peer": {"libp2p_peer_id": "12D3KooWAttackerPeer"},
        }
        for label, mutation in mutations.items():
            with self.subTest(identity_field=label), tempfile.TemporaryDirectory(
                prefix=f"oasis7-pg02r-b-identity-{label}-"
            ) as temp_dir:
                receipt = dict(base_receipt)
                receipt.update(mutation)
                result = run_validator_47_stage(Path(temp_dir), receipt=receipt)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertFalse(
                    (Path(temp_dir) / "stage").exists(),
                    "identity drift must fail before stage materialization",
                )

    def test_stage_rejects_wrong_bootstrap_topology_or_digest(self) -> None:
        """A three-validator stage must bind the exact governed peer file."""
        receipt = {
            "schema_version": "oasis7.identity_provision.v1",
            "node_id": VALIDATOR_47_NODE_ID,
            "root_public_key": VALIDATOR_47_ROOT_PUBLIC_KEY,
            "finality_public_key": VALIDATOR_47_FINALITY_PUBLIC_KEY,
            "libp2p_peer_id": VALIDATOR_47_PEER_ID,
        }
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-bootstrap-") as temp_dir:
            temp = Path(temp_dir)
            wrong_topology = temp / "wrong-topology.txt"
            wrong_topology.write_text(
                "/ip4/39.104.204.172/tcp/6831/p2p/12D3KooWMyPapumCaTABq27umWdHqXDr8AoTse21eMVnXeJEsbNp\n"
                "/ip4/39.104.205.67/tcp/6832/p2p/12D3KooWAuNCCEDu7CdUUDwALuAhuLekZHgVWxAYp4Ag5ti79fJj\n",
                encoding="utf-8",
            )
            result = run_validator_47_stage(temp / "topology", receipt=receipt, bootstrap_peers=wrong_topology)
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertIn("bootstrap", result.stderr.lower())
            self.assertFalse((temp / "topology" / "stage").exists())

            wrong_digest = temp / "wrong-digest.txt"
            wrong_digest.write_bytes(BOOTSTRAP_PEERS_EVIDENCE.read_bytes() + b"\n")
            result = run_validator_47_stage(temp / "digest", receipt=receipt, bootstrap_peers=wrong_digest)
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertIn("bootstrap", result.stderr.lower())
            self.assertFalse((temp / "digest" / "stage").exists())

    def test_bootstrap_rejects_stale_pair_identity_and_target_drift(self) -> None:
        """A stale pair env must fail closed for node, role, port, and world drift."""
        # This fixture describes the exact kind of stale pair input that must
        # never be promoted as validator-47.  It is data-only and never sent
        # to a host or provider.
        stale_pair_env = dict(VALIDATOR_47_ENV)
        stale_pair_env.update(
            {
                "NODE_ID": "triad-testnet-storage",
                "NODE_ROLE": "storage",
                "STATUS_BIND": "0.0.0.0:6632",
                "NODE_GOSSIP_BIND": "0.0.0.0:6832",
                "WORLD_ID": "stale-pair-world",
            }
        )
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-stale-") as temp_dir:
            env_path = Path(temp_dir) / "node.env"
            env_path.write_text(
                "\n".join(f"{key}={value}" for key, value in stale_pair_env.items()) + "\n",
                encoding="utf-8",
            )
            self.assertTrue(env_path.is_file())
            self.assertIn("triad-testnet-storage", env_path.read_text(encoding="utf-8"))

        source = BOOTSTRAP.read_text(encoding="utf-8")
        self.assertTrue(VALIDATOR_47_NODE_ID in source, "contract: bootstrap has no validator-47 target")
        self.assertTrue(VALIDATOR_47_SERVICE in source, "contract: bootstrap has no validator-47 service target")
        self.assertNotIn("systemctl start", source, "validator-47 staging must remain no-start")
        self.assertNotIn("systemctl enable", source, "validator-47 staging must remain disabled")
        for marker in (
            "NODE_ID",
            "NODE_ROLE",
            "6634",
            "6834",
            "checkpoint_provider",
            "full_storage_provider",
            "WORLD_ID",
            "NETWORK_TIER_MANIFEST_PATH",
            "GENESIS_VALIDATOR_REGISTRY_PATH",
            "EXECUTION_WORLD_DIR",
            "mismatch",
        ):
            self.assertTrue(
                marker in source,
                f"contract: bootstrap does not reject stale validator-47 binding field {marker}",
            )

    def test_readback_proves_disabled_inactive_no_process_and_no_listener(self) -> None:
        """Readback must expose disabled state and reject an enabled unit."""
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-readback-") as temp_dir:
            root = Path(temp_dir) / "stack"
            root.mkdir()
            config = root / "config"
            config.mkdir()
            config.chmod(0o700)
            inventory_bytes = INVENTORY.read_bytes()
            (config / INVENTORY.name).write_bytes(inventory_bytes)
            bootstrap_peer_bytes = BOOTSTRAP_PEERS_EVIDENCE.read_bytes()
            (config / "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt").write_bytes(bootstrap_peer_bytes)
            source_registry_path = config / "doc/testing/evidence/public-testnet-governed-bootstrap-validator-triad-registry-2026-09-15.json"
            source_registry_path.parent.mkdir(parents=True)
            source_registry_path.write_bytes(SOURCE_REGISTRY_EVIDENCE.read_bytes())
            (config / "public-testnet-governed-bootstrap-manifest-2026-06-06.json").write_text(
                json.dumps(
                    {
                        "bootstrap_peer_authority": {
                            "ref": "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt",
                            "sha256": hashlib.sha256(bootstrap_peer_bytes).hexdigest(),
                        },
                        "deployment_validator_registry": {
                            "ref": "config/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json",
                            "sha256": GENERATED_REGISTRY_SHA256,
                            "semantic_sha256": GENERATED_REGISTRY_SEMANTIC_SHA256,
                        },
                    }
                ),
                encoding="utf-8",
            )
            (config / "node-keypair.toml").write_text("already-staged-key\n", encoding="utf-8")
            (config / "node-keypair.toml").chmod(0o600)
            public_receipt = {
                "schema_version": "oasis7.identity_provision.v1",
                "node_id": VALIDATOR_47_NODE_ID,
                "root_public_key": VALIDATOR_47_ROOT_PUBLIC_KEY,
                "finality_public_key": VALIDATOR_47_FINALITY_PUBLIC_KEY,
                "libp2p_peer_id": VALIDATOR_47_PEER_ID,
            }
            receipt_bytes = (json.dumps(public_receipt, sort_keys=True) + "\n").encode("utf-8")
            (config / "identity-receipt.json").write_bytes(receipt_bytes)
            (config / "identity-receipt.json").chmod(0o600)
            registry_path = config / "public-testnet-governed-bootstrap-validator-registry-2026-06-06.json"
            registry_path.write_bytes(generated_registry_bytes())
            (config / "node.env").write_text(
                "\n".join(
                    [
                        "NODE_ID=triad-testnet-validator-47",
                        "NODE_ROLE=storage",
                        "P2P_NODE_ROLE=full_storage",
                        "CHECKPOINT_PROVIDER=1",
                        "FULL_STORAGE_PROVIDER=1",
                        "IDENTITY_KEY_PATH=config/node-keypair.toml",
                        "IDENTITY_RECEIPT_PATH=config/identity-receipt.json",
                        f"GENESIS_VALIDATOR_REGISTRY_SHA256={GENERATED_REGISTRY_SHA256}",
                        f"GENESIS_VALIDATOR_REGISTRY_SEMANTIC_SHA256={GENERATED_REGISTRY_SEMANTIC_SHA256}",
                        f"DEPLOYMENT_INVENTORY_SHA256={hashlib.sha256(inventory_bytes).hexdigest()}",
                        "BOOTSTRAP_PEER_PATH=config/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt",
                        f"BOOTSTRAP_PEER_SHA256={hashlib.sha256(bootstrap_peer_bytes).hexdigest()}",
                        f"IDENTITY_KEY_SHA256={hashlib.sha256((config / 'node-keypair.toml').read_bytes()).hexdigest()}",
                        f"IDENTITY_RECEIPT_SHA256={hashlib.sha256(receipt_bytes).hexdigest()}",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            (config / "node.env").chmod(0o600)
            bin_dir = Path(temp_dir) / "bin"
            bin_dir.mkdir()
            write_fake_readback_tools(bin_dir, unit_file_state="disabled")
            module = load_module(SERVICE_READBACK, "service_readback_pg02r_b")
            module.CANONICAL_ROOT = str(root)
            args = [
                "--read-only",
                "--role",
                "validator-47",
                "--root",
                str(root),
                "--service",
                VALIDATOR_47_SERVICE,
            ]
            output = io.StringIO()
            with (
                mock.patch.dict(os.environ, {"PATH": f"{bin_dir}:{os.environ['PATH']}"}),
                contextlib.redirect_stdout(output),
                contextlib.redirect_stderr(io.StringIO()),
            ):
                try:
                    result = module.main(args)
                except SystemExit as exc:
                    raise AssertionError(
                        "contract: readback rejected the validator-47 role before proving "
                        f"the no-start contract (exit {exc.code})"
                    ) from None
            self.assertEqual(result, 0)
            payload = json.loads(output.getvalue())
            self.assertEqual(payload["unit_file_state"], "disabled")
            self.assertFalse(payload["active"])
            self.assertFalse(payload["running"])
            self.assertEqual(payload["service_state"], "stopped")
            self.assertEqual(payload["listeners"], [])
            self.assertTrue(payload["no_process"])
            self.assertTrue(payload["no_listener"])
            self.assertEqual(
                payload["inventory"]["source_registry_ref"],
                "doc/testing/evidence/public-testnet-governed-bootstrap-validator-triad-registry-2026-09-15.json",
            )
            self.assertEqual(
                payload["inventory"]["source_registry_sha256"],
                "a6bfa524e32f2f54c4665d58f18e87b5fa21845e17c14269be1cb1f978adb50f",
            )

            write_fake_readback_tools(bin_dir, unit_file_state="enabled")
            enabled_module = load_module(SERVICE_READBACK, "service_readback_pg02r_b_enabled")
            enabled_module.CANONICAL_ROOT = str(root)
            with mock.patch.dict(os.environ, {"PATH": f"{bin_dir}:{os.environ['PATH']}"}):
                with self.assertRaises(SystemExit):
                    enabled_module.main(args)

    def test_readback_rejects_parent_component_symlink_before_resolve(self) -> None:
        """A symlinked parent must not redirect the canonical root before validation."""
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-root-symlink-") as temp_dir:
            temp = Path(temp_dir)
            real_parent = temp / "real-parent"
            real_root = real_parent / "stack"
            real_root.mkdir(parents=True)
            alias_parent = temp / "alias-parent"
            alias_parent.symlink_to(real_parent, target_is_directory=True)
            aliased_root = alias_parent / real_root.name

            module = load_module(SERVICE_READBACK, "service_readback_pg02r_b_root_symlink")
            module.CANONICAL_ROOT = str(aliased_root)
            args = [
                "--read-only",
                "--role",
                "validator-47",
                "--root",
                str(aliased_root),
                "--service",
                VALIDATOR_47_SERVICE,
            ]
            error = io.StringIO()
            with contextlib.redirect_stderr(error), self.assertRaises(SystemExit):
                module.parse_args(args)
            self.assertIn("symlink path component", error.getvalue())

    def test_readback_rejects_orphan_validator_process_not_attached_to_unit(self) -> None:
        """A stray runtime process must block no-start even when MainPID is zero."""
        with tempfile.TemporaryDirectory(prefix="oasis7-pg02r-b-orphan-") as temp_dir:
            root = Path(temp_dir) / "stack"
            root.mkdir()
            config = root / "config"
            config.mkdir()
            config.chmod(0o700)
            inventory_bytes = INVENTORY.read_bytes()
            (config / INVENTORY.name).write_bytes(inventory_bytes)
            bootstrap_peer_bytes = BOOTSTRAP_PEERS_EVIDENCE.read_bytes()
            (config / "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt").write_bytes(bootstrap_peer_bytes)
            source_registry_path = config / "doc/testing/evidence/public-testnet-governed-bootstrap-validator-triad-registry-2026-09-15.json"
            source_registry_path.parent.mkdir(parents=True)
            source_registry_path.write_bytes(SOURCE_REGISTRY_EVIDENCE.read_bytes())
            (config / "public-testnet-governed-bootstrap-manifest-2026-06-06.json").write_text(
                json.dumps(
                    {
                        "bootstrap_peer_authority": {
                            "ref": "public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt",
                            "sha256": hashlib.sha256(bootstrap_peer_bytes).hexdigest(),
                        },
                        "deployment_validator_registry": {
                            "ref": "config/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json",
                            "sha256": GENERATED_REGISTRY_SHA256,
                            "semantic_sha256": GENERATED_REGISTRY_SEMANTIC_SHA256,
                        },
                    }
                ),
                encoding="utf-8",
            )
            key_path = config / "node-keypair.toml"
            key_path.write_text("already-staged-key\n", encoding="utf-8")
            key_path.chmod(0o600)
            public_receipt = {
                "schema_version": "oasis7.identity_provision.v1",
                "node_id": VALIDATOR_47_NODE_ID,
                "root_public_key": VALIDATOR_47_ROOT_PUBLIC_KEY,
                "finality_public_key": VALIDATOR_47_FINALITY_PUBLIC_KEY,
                "libp2p_peer_id": VALIDATOR_47_PEER_ID,
            }
            receipt_bytes = (json.dumps(public_receipt, sort_keys=True) + "\n").encode("utf-8")
            receipt_path = config / "identity-receipt.json"
            receipt_path.write_bytes(receipt_bytes)
            receipt_path.chmod(0o600)
            registry_path = config / "public-testnet-governed-bootstrap-validator-registry-2026-06-06.json"
            registry_path.write_bytes(generated_registry_bytes())
            (config / "node.env").write_text(
                "\n".join(
                    [
                        f"NODE_ID={VALIDATOR_47_NODE_ID}",
                        "NODE_ROLE=storage",
                        "P2P_NODE_ROLE=full_storage",
                        "CHECKPOINT_PROVIDER=1",
                        "FULL_STORAGE_PROVIDER=1",
                        "IDENTITY_KEY_PATH=config/node-keypair.toml",
                        "IDENTITY_RECEIPT_PATH=config/identity-receipt.json",
                        f"GENESIS_VALIDATOR_REGISTRY_SHA256={GENERATED_REGISTRY_SHA256}",
                        f"GENESIS_VALIDATOR_REGISTRY_SEMANTIC_SHA256={GENERATED_REGISTRY_SEMANTIC_SHA256}",
                        f"DEPLOYMENT_INVENTORY_SHA256={hashlib.sha256(inventory_bytes).hexdigest()}",
                        "BOOTSTRAP_PEER_PATH=config/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt",
                        f"BOOTSTRAP_PEER_SHA256={hashlib.sha256(bootstrap_peer_bytes).hexdigest()}",
                        f"IDENTITY_KEY_SHA256={hashlib.sha256(key_path.read_bytes()).hexdigest()}",
                        f"IDENTITY_RECEIPT_SHA256={hashlib.sha256(receipt_bytes).hexdigest()}",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            (config / "node.env").chmod(0o600)
            bin_dir = Path(temp_dir) / "bin"
            bin_dir.mkdir()
            write_fake_readback_tools(
                bin_dir,
                unit_file_state="disabled",
                main_pid="0",
                ps_output=(
                    "  PID COMMAND\n"
                    f"4242 {root.resolve()}/current/bin/oasis7_chain_runtime "
                    "--node-id triad-testnet-validator-47 --status-bind 0.0.0.0:6634\n"
                ),
            )
            module = load_module(SERVICE_READBACK, "service_readback_pg02r_b_orphan")
            module.CANONICAL_ROOT = str(root)
            args = [
                "--read-only",
                "--role",
                "validator-47",
                "--root",
                str(root),
                "--service",
                VALIDATOR_47_SERVICE,
            ]
            with mock.patch.dict(os.environ, {"PATH": f"{bin_dir}:{os.environ['PATH']}"}):
                with self.assertRaises(SystemExit):
                    module.main(args)

    def test_bootstrap_and_readback_receipt_carries_no_start_proof(self) -> None:
        """The receipt path must retain independent no-start observations."""
        bootstrap_source = BOOTSTRAP.read_text(encoding="utf-8")
        readback_source = SERVICE_READBACK.read_text(encoding="utf-8")
        for marker in (
            "UnitFileState",
            "disabled",
            "inactive",
            "no_process",
            "no_listener",
            "listeners",
            "service-readback",
        ):
            self.assertTrue(
                marker in bootstrap_source or marker in readback_source,
                f"contract: no-start receipt/readback does not prove {marker}",
            )

    def test_runbook_and_manual_define_executable_validator47_recovery_path(self) -> None:
        """Stable operator docs must cover no-start, cutover, and clean redeploy."""
        command_markers = (
            "p2p-public-testnet-bootstrap-fresh-validator-host.sh",
            "--node-id triad-testnet-validator-47",
            f"--service-name {VALIDATOR_47_SERVICE}",
        )
        contract_patterns = {
            "validator-47": r"validator-47",
            "no-start": r"no[- ]start",
            "existing pair": r"existing pair",
            "cold cutover": r"cold[- ]cutover",
            "staggered": r"staggered",
            "clean redeploy": r"clean[- ]redeploy",
            "pair preservation": r"pair[- ]preserv",
        }
        doc_sources = (
            (RUNBOOK.name, RUNBOOK.read_text(encoding="utf-8") + "\n" + RUNBOOK_PHASE_C.read_text(encoding="utf-8")),
            (MANUAL.name, MANUAL.read_text(encoding="utf-8")),
        )
        for doc_name, raw_source in doc_sources:
            source = raw_source.lower()
            for marker in command_markers:
                self.assertTrue(
                    marker.lower() in source,
                    f"contract: {doc_name} lacks executable validator-47 command {marker}",
                )
            for marker, pattern in contract_patterns.items():
                self.assertTrue(
                    re.search(pattern, source),
                    f"contract: {doc_name} lacks executable validator-47 contract marker {marker}",
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
