#!/usr/bin/env python3
"""Standalone alias and plan guard tests extracted from clean-room tests."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch


def _load_test_helpers() -> types.ModuleType:
    module_name = "full_network_clean_room_test_helpers"
    existing = sys.modules.get(module_name)
    if existing is not None:
        return existing
    helpers_path = (
        Path(__file__).resolve().parent
        / "p2p-public-testnet-full-network-clean-room-test-helpers.py"
    )
    spec = importlib.util.spec_from_file_location(module_name, helpers_path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {helpers_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


def _resolve_plan_helper_source(helpers: types.ModuleType) -> type:
    source = getattr(helpers, "_PLAN_HELPER_SOURCE", None)
    if source is not None:
        return source
    plan_test_path = (
        Path(__file__).resolve().parent
        / "p2p-public-testnet-full-network-clean-room.test.py"
    )
    module_name = "p2p_public_testnet_clean_room_plan_tests_helpers_source"
    spec = importlib.util.spec_from_file_location(module_name, plan_test_path)
    if spec is None or spec.loader is None:
        raise AssertionError(f"cannot load module: {plan_test_path}")
    module = importlib.util.module_from_spec(spec)
    helpers._LOADING_ALIASES_AND_GUARDS = True
    try:
        spec.loader.exec_module(module)
    finally:
        helpers._LOADING_ALIASES_AND_GUARDS = False
    return module.FullNetworkCleanRoomPlanTests


def _bind_plan_helpers(target: type, source: type) -> None:
    for name, value in source.__dict__.items():
        if name.startswith("test_") or (name.startswith("__") and name.endswith("__")):
            continue
        if name not in target.__dict__:
            setattr(target, name, value)


_HELPERS = _load_test_helpers()
ROOT = _HELPERS.ROOT
SIDECAR_PATH = _HELPERS.SIDECAR_PATH
load_module = _HELPERS.load_module
load_sidecar_module = _HELPERS.load_sidecar_module
_fixture_digest = _HELPERS._fixture_digest
FullNetworkCleanRoomPlanTestBase = _HELPERS.FullNetworkCleanRoomPlanTestBase


class FullNetworkCleanRoomPlanGuardTests(FullNetworkCleanRoomPlanTestBase):

    def test_v2_sidecar_executes_signer_and_verifier_over_final_canonical_payload(self) -> None:
        """The final v2 payload must pass the existing signer and verifier seam."""
        sidecar = load_sidecar_module()
        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        context = {
            "task_uid": request["task_uid"],
            "frozen_head_oid": request["head_oid"],
            "plan_digest": "p" * 64,
            "node_id": identity["node_id"],
            "peer_id": identity["peer_id"],
        }
        signer_calls: list[tuple[bytes, dict[str, object]]] = []
        verifier_calls: list[tuple[bytes, dict[str, object], dict[str, object]]] = []

        def signer(payload: bytes, received_context: dict[str, object]) -> str:
            signer_calls.append((payload, dict(received_context)))
            return "d" * 128

        def verifier(
            payload: bytes,
            envelope: dict[str, object],
            received_context: dict[str, object],
        ) -> dict[str, object]:
            verifier_calls.append((payload, dict(envelope), dict(received_context)))
            return {"verified": True}

        template = dict(identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(identity)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            envelope = sidecar.create(
                raw_path,
                template_path,
                output_path,
                context=context,
                signer=signer,
                verifier=verifier,
            )

        self.assertEqual(len(signer_calls), 1)
        self.assertEqual(len(verifier_calls), 1)
        self.assertEqual(signer_calls[0][1], context)
        self.assertEqual(verifier_calls[0][2], context)
        self.assertEqual(verifier_calls[0][0], signer_calls[0][0])
        self.assertEqual(verifier_calls[0][1]["signature_hex"], "d" * 128)
        self.assertEqual(envelope["signature_hex"], "d" * 128)
        self.assertIn(request["task_uid"].encode(), signer_calls[0][0])
        self.assertIn(request["head_oid"].encode(), signer_calls[0][0])
        self.assertIn(context["plan_digest"].encode(), signer_calls[0][0])

    def test_executable_v2_sidecar_rejects_raw_template_identity_cross_pair(self) -> None:
        """A raw identity from one node cannot be wrapped with another node's template."""
        request = self._input()
        raw_identity = request["nodes"][0]["identity_receipt"]
        template_identity = request["nodes"][1]["identity_receipt"]
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(
            raw_identity, key_path="/opt/oasis7/p2p-testnet/config/node-keypair.toml"
        )
        template = dict(template_identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SIDECAR_PATH),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertRegex(result.stderr, r"(?i)node|peer|key|identity|match")
        self.assertFalse(output_path.exists())

    def test_executable_v2_sidecar_rejects_malformed_template_freshness(self) -> None:
        """Malformed freshness metadata must never be emitted as a v2 envelope."""
        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        template = dict(identity)
        template.update(
            {
                "issued_at": "not-an-rfc3339-timestamp",
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(identity)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SIDECAR_PATH),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertRegex(result.stderr, r"(?i)issued|expires|timestamp|fresh|time")
        self.assertFalse(output_path.exists())

    def test_sidecar_output_cannot_replace_retained_v2_evidence(self) -> None:
        """An unverified sidecar output cannot replace retained v2 evidence."""
        request = self._input()
        identity = request["nodes"][0]["identity_receipt"]
        raw_v1 = self._raw_runtime_identity_receipt_v1_bytes(identity)
        template = dict(identity)
        template.update(
            {
                "signed_payload_sha256": "a" * 64,
                "signature_hex": "d" * 128,
                "canonical_digest": "c" * 64,
            }
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            raw_path = root / "identity-receipt.v1.raw"
            template_path = root / "identity-receipt.v2.template.json"
            output_path = root / "identity-receipt.v2.json"
            raw_path.write_bytes(raw_v1)
            template_path.write_text(json.dumps(template), encoding="utf-8")
            result = subprocess.run(
                [
                    "python3",
                    str(SIDECAR_PATH),
                    "--raw-v1",
                    str(raw_path),
                    "--template",
                    str(template_path),
                    "--out",
                    str(output_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            request["nodes"][0]["identity_receipt"] = json.loads(
                output_path.read_text(encoding="utf-8")
            )

        plan = self.module.build_plan(request)
        self.assertEqual(
            plan["identity_v2_evidence"], self._baseline_identity_v2_evidence
        )
        self.assertTrue(
            all(node["identity_receipt"]["schema_version"] == self.module.IDENTITY_RECEIPT_SCHEMA
                for node in plan["nodes"])
        )

    def test_plan_projects_identity_commands_through_governed_v2_sidecar(self) -> None:
        """Every authoritative identity command must use the executable v2 sidecar."""
        runbook = (
            (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md").read_text(encoding="utf-8")
            + "\n"
            + (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap-phase-c-rebuild.runbook.md").read_text(encoding="utf-8")
        )
        self.assertIn("p2p-public-testnet-identity-receipt-v2.py", runbook)
        self.assertIn("--raw-v1", runbook)
        self.assertIn("--template", runbook)
        self.assertIn("--out", runbook)
        self.assertNotIn("oasis7_chain_runtime identity-receipt --config-dir", runbook)

    def test_plan_rejects_synthetic_identity_digest_and_signature_pair(self) -> None:
        """A synthetic retained envelope cannot become an apply-ready identity."""
        request = self._input()
        evidence = self._tampered_evidence_artifact(
            self._baseline_identity_v2_evidence,
            mutate=lambda envelope: envelope.update(
                signed_payload_sha256="a" * 64,
                signature_hex="b" * 128,
                canonical_digest="c" * 64,
            ),
        )
        with self.assertRaises(SystemExit) as raised:
            self._build_plan_with_evidence(request, evidence)
        self.assertRegex(str(raised.exception), r"(?i)identity|payload|digest|placeholder|evidence|binding")

    def test_governed_bootstrap_runbook_describes_v2_identity_envelope_boundary(self) -> None:
        """The authoritative runbook must retire raw v1 direct admission and specify v2."""
        runbook = (
            (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md").read_text(encoding="utf-8")
            + "\n"
            + (ROOT / "doc/p2p/blockchain/public-testnet-governed-bootstrap-phase-c-rebuild.runbook.md").read_text(encoding="utf-8")
        )
        self.assertRegex(
            runbook,
            r"(?is)oasis7\.identity_receipt\.v1.{0,800}(?:raw|原始).{0,800}(?:retir|退役).{0,800}(?:direct admission|直接.*(?:admission|准入))",
        )
        self.assertIn("oasis7.identity_receipt.v2", runbook)
        for field in (
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
            "task_uid",
            "head_oid",
            "plan_digest",
            "capture_window_id",
            "rotation_epoch",
            "issued_at",
            "expires_at",
        ):
            with self.subTest(field=field):
                self.assertIn(field, runbook)
        self.assertRegex(
            runbook,
            r"(?is)(?:independent|governed).{0,500}verifier.{0,500}(?:before|prior to).{0,500}(?:provider|mutation|admission)",
        )

    def test_plan_requires_explicit_peer_id_on_every_inventory_node(self) -> None:
        """Legacy omitted/partial peer identities must not enter authenticated plans."""
        for omission in ("all", "storage-205"):
            with self.subTest(omission=omission):
                request = self._input()
                inventory = self._explicit_inventory(request)
                if omission == "all":
                    for node in inventory["nodes"].values():
                        node.pop("peer_id")
                else:
                    inventory["nodes"][omission].pop("peer_id")
                request["deployment_inventory"] = inventory
                with self.assertRaises(SystemExit) as raised:
                    self.module.build_plan(request)
                self.assertRegex(str(raised.exception), r"(?i)peer|inventory|explicit|complete")

    def test_plan_validates_inventory_digest_before_normalization(self) -> None:
        """Inventory field mutations must fail against the incoming stale receipt."""
        mutations = ("node_root", "persistent_state_paths", "expected_key_uid", "expected_key_gid", "peer_id")
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                request = self._input()
                inventory = self._explicit_inventory(request)
                request["deployment_inventory"] = inventory
                name = "storage-205"
                inventory_node = inventory["nodes"][name]
                node = next(item for item in request["nodes"] if item["name"] == name)
                if mutation in {"node_root", "persistent_state_paths"}:
                    old_root = node["node_root"]
                    new_root = "/opt/oasis7/attacker-root"
                    node["node_root"] = new_root
                    node["persistent_state_paths"] = [
                        path.replace(old_root, new_root, 1)
                        for path in node["persistent_state_paths"]
                    ]
                    inventory_node["node_root"] = new_root
                    inventory_node["persistent_state_paths"] = list(node["persistent_state_paths"])
                elif mutation == "expected_key_uid":
                    inventory_node["expected_key_uid"] = 1001
                    node["identity_receipt"]["key_uid"] = 1001
                elif mutation == "expected_key_gid":
                    inventory_node["expected_key_gid"] = 1002
                    node["identity_receipt"]["key_gid"] = 1002
                else:
                    rotated_peer = "12D3KooWstale-inventory-peer"
                    inventory_node["peer_id"] = rotated_peer
                    node["identity_receipt"]["peer_id"] = rotated_peer
                with self.assertRaises(SystemExit) as raised:
                    self.module.build_plan(request)
                self.assertRegex(str(raised.exception), r"(?i)inventory|receipt|digest|binding|peer|canonical")

    def test_plan_accepts_fully_explicit_digest_bound_inventory(self) -> None:
        """The governed explicit inventory schema remains a valid admission path."""
        request = self._input()
        request["deployment_inventory"] = self._explicit_inventory(request)
        plan = self.module.build_plan(request)
        for name in self.module.NODE_ORDER:
            self.assertIn("peer_id", plan["deployment_inventory"]["nodes"][name])
        self.assertEqual(
            plan["deployment_inventory"]["receipt"]["signed_payload_sha256"],
            self.module._canonical_deployment_inventory_payload_digest(
                plan["deployment_inventory"]
            ),
        )


_bind_plan_helpers(
    FullNetworkCleanRoomPlanGuardTests,
    _resolve_plan_helper_source(_HELPERS),
)


class PlannerOutputAliasTests(unittest.TestCase):
    def test_cli_cannot_replace_held_fleet_lock(self):
        for alias in ("exact", "normalized", "hardlink"):
            with self.subTest(alias=alias), tempfile.TemporaryDirectory() as directory:
                module = load_module()
                root = Path(directory).resolve()
                paths, source, evidence = self._authority_fixture(module, root)
                spec = importlib.util.spec_from_file_location(
                    "planner_cli_lock_adapter", ROOT / "scripts/p2p-public-testnet-full-network-clean-room-adapter.py")
                adapter = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(adapter)
                lock_path = root / "full-network-clean-room.lock"
                adapter.CANONICAL_FLEET_LOCK_PATH = str(lock_path)
                with patch.object(module._peer_registry_authority(), "CANONICAL_FLEET_LOCK_PATH", str(lock_path)):
                    first = adapter._acquire_fleet_transaction_guard(root / "first.json")
                    try:
                        plan = {"plan_digest": "test", "execution": {"mode": "plan-only"}}
                        with patch.object(module, "build_plan", return_value=plan):
                            ordinary = root / "ordinary-plan.json"
                            self.assertEqual(module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(ordinary)]), 0)
                        output = lock_path
                        if alias == "normalized":
                            (root / "nested").mkdir()
                            output = root / "nested" / ".." / lock_path.name
                        elif alias == "hardlink":
                            output = root / "hardlink"
                            os.link(lock_path, output)
                        before = (lock_path.read_bytes(), lock_path.stat().st_ino,
                                  output.read_bytes(), output.stat().st_ino)
                        with patch.object(module, "build_plan", return_value=plan) as build:
                            try:
                                module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)])
                            except SystemExit:
                                rejected = True
                            else:
                                rejected = False
                        try:
                            second = adapter._acquire_fleet_transaction_guard(root / "second.json")
                        except adapter.AdapterError:
                            blocked = True
                        else:
                            blocked = False
                            second.close()
                        after = (lock_path.read_bytes(), lock_path.stat().st_ino,
                                 output.read_bytes(), output.stat().st_ino)
                        self.assertEqual((rejected, build.call_count, before == after, blocked),
                                         (True, 0, True, True),
                                         "CLI must reject lock aliases before plan building or publication")
                    finally:
                        first.close()

    def test_governance_root_is_protected_without_opening_it(self):
        module = load_module()
        protected = module._plan_output_inputs(Path("/fixture/input"), Path("/fixture/map"), {})
        for path in (Path("/operator/truth/governance-root.json"),
                     Path(module.__file__).with_name("p2p-public-testnet-full-network-clean-room-adapter.py"),
                     Path(module.__file__).with_name("fixtures") / "oasis7-governance-root.v1.json"):
            with self.subTest(path=path), self.assertRaises(SystemExit):
                module._reject_plan_output_aliases(path, protected)

    def _authority_fixture(self, module, root):
        paths = {name: root / name for name in
                 ("tool", "trust", "registry", "registry-verifier", "trust-key", "provider-key", "provider-adapter")}
        for path in paths.values():
            path.write_bytes(b"fixture authority bytes")
            path.chmod(0o700)
        paths["trust"].write_text(json.dumps({"allowlist": [{"public_key_ref": str(paths["trust-key"])}]}))
        paths["registry"].write_text(json.dumps({
            "trust_config_path": str(paths["trust"]),
            "providers": [{"public_key_ref": str(paths["provider-key"]), "adapter_path": str(paths["provider-adapter"])}],
            "verifier": {"executable_path": str(paths["registry-verifier"])},
        }))
        for prefix, name in (("VERIFY_TOOL", "tool"), ("TRUST_CONFIG", "trust"), ("PROVIDER_REGISTRY", "registry")):
            setattr(module, f"IDENTITY_V2_{prefix}_PATH", paths[name])
            setattr(module, f"IDENTITY_V2_{prefix}_SHA256", _fixture_digest(paths[name]))
        source, evidence = root / "input.json", root / "map.json"
        source.write_text("{}")
        evidence.write_text("{}")
        return paths, source, evidence

    def test_cli_protects_complete_pinned_authority_closure(self):
        for name in ("tool", "trust", "registry", "registry-verifier", "trust-key", "provider-key", "provider-adapter"):
            for alias in ("direct", "resolved", "hardlink"):
                with self.subTest(name=name, alias=alias), tempfile.TemporaryDirectory() as directory:
                    module = load_module()
                    root = Path(directory).resolve()
                    paths, source, evidence = self._authority_fixture(module, root)
                    output = paths[name]
                    if alias == "resolved":
                        parent = root / "parent-alias"
                        parent.symlink_to(root, target_is_directory=True)
                        output = parent / name
                    elif alias == "hardlink":
                        output = root / "hardlink"
                        os.link(paths[name], output)
                    before = {path: (path.read_bytes(), path.stat()) for path in paths.values()}
                    entries = set(root.iterdir())
                    with patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}) as build:
                        with self.assertRaises(SystemExit):
                            module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)])
                        build.assert_not_called()
                    self.assertEqual(set(root.iterdir()), entries)
                    for path, (raw, metadata) in before.items():
                        self.assertEqual(path.read_bytes(), raw)
                        self.assertEqual(module._identity_v2_anchor_identity(path.stat()), module._identity_v2_anchor_identity(metadata))
        with tempfile.TemporaryDirectory() as directory:
            module = load_module()
            root = Path(directory).resolve()
            paths, source, evidence = self._authority_fixture(module, root)
            output = root / "ordinary-plan.json"
            with patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}) as build:
                self.assertEqual(module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)]), 0)
                build.assert_called_once()
            self.assertEqual(json.loads(output.read_text())["plan_digest"], "test")

    def test_output_closure_rejects_unpinned_reference_bytes_before_build(self):
        with tempfile.TemporaryDirectory() as directory:
            module = load_module()
            root = Path(directory).resolve()
            paths, source, evidence = self._authority_fixture(module, root)
            paths["registry"].write_text(json.dumps({"providers": []}))
            output = root / "new-parent" / "plan.json"
            with patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}) as build:
                with self.assertRaisesRegex(SystemExit, "pin|digest"):
                    module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence), "--out", str(output)])
                build.assert_not_called()
            self.assertFalse(output.parent.exists())

    def test_atomic_output_preserves_prior_plan_on_replace_failure(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "plan.json"
            output.write_bytes(b"prior plan\n")
            with patch.object(module.os, "replace", side_effect=OSError("injected failure")):
                with self.assertRaises(OSError):
                    module._write_plan_atomic(output, {"plan": "new"}, [])
            self.assertEqual(output.read_bytes(), b"prior plan\n")
            self.assertEqual(list(output.parent.iterdir()), [output])
            module._write_plan_atomic(output, {"plan": "new"}, [])
            self.assertEqual(json.loads(output.read_text()), {"plan": "new"})

    def test_cli_rejects_output_aliases_without_modifying_evidence(self):
        module = load_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, evidence_path, artifact = (root / name for name in ("input.json", "map.json", "raw.bin"))
            source.write_text("{}")
            artifact.write_bytes(b"exact runtime bytes")
            descriptor = {"path": str(artifact), "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest(), "size_bytes": artifact.stat().st_size}
            evidence_path.write_text(json.dumps({"context": descriptor, "plan_intent": descriptor, "entries": [{"raw_v1": descriptor}]}))
            hardlink = root / "hardlink"
            os.link(artifact, hardlink)
            parent_alias = root / "parent-alias"
            parent_alias.symlink_to(root, target_is_directory=True)
            before = {path: path.read_bytes() for path in (source, evidence_path, artifact)}
            for output in (source, evidence_path, artifact, hardlink, parent_alias / artifact.name):
                with self.subTest(output=str(output)), patch.object(module, "build_plan", return_value={"plan_digest": "test", "execution": {"mode": "plan-only"}}):
                    with self.assertRaises(SystemExit):
                        module.main(["--input", str(source), "--identity-v2-evidence-map", str(evidence_path), "--out", str(output)])
                    self.assertEqual({path: path.read_bytes() for path in before}, before)


if __name__ == "__main__":
    unittest.main()
