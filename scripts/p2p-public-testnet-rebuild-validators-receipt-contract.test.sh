#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Reuse the signed local fixtures owned by the canonical executor contract
# test.  This test is deliberately a shell-entrypoint test: it proves that
# the governed shell dispatch is deterministic and cannot accidentally reach
# SSH/systemd or mutate either stopped node during plan mode.
python3 - "$ROOT_DIR" <<'PY'
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


root = Path(sys.argv[1])
test_path = root / "scripts" / "p2p-public-testnet-validator-pair-rebuild.test.py"
wrapper_path = root / "scripts" / "p2p-public-testnet-rebuild-validators.sh"
runbook_path = root / "doc" / "p2p" / "blockchain" / "public-testnet-governed-bootstrap.runbook.md"
phase_c_runbook_path = root / "doc" / "p2p" / "blockchain" / "public-testnet-governed-bootstrap-phase-c-rebuild.runbook.md"
wrapper_source = wrapper_path.read_text(encoding="utf-8")
runbook_source = runbook_path.read_text(encoding="utf-8") + "\n" + phase_c_runbook_path.read_text(encoding="utf-8")
help_output = subprocess.run(
    [str(wrapper_path), "--help"], text=True, capture_output=True, check=True
).stdout
if (
    "Plan is local-only" in help_output
    or "Plan is non-mutating but performs bounded live GitHub/SSH read-only" not in help_output
    or "re-observation" not in help_output
    or "resume" not in help_output
):
    raise SystemExit("wrapper help must disclose bounded live observation and resume recovery")
if "resume)" not in wrapper_source or "OASIS7_VALIDATOR_PAIR_NONCE_LEDGER" not in wrapper_source:
    raise SystemExit("wrapper must expose explicit fail-closed resume routing and nonce binding")
for required in ("tempfile.mkstemp", "os.fsync", "os.replace"):
    if required not in wrapper_source:
        raise SystemExit(f"shell envelope is missing durable publication primitive: {required}")
if "destination.write_text(output + \"\\n\"" in wrapper_source:
    raise SystemExit("shell envelope must not publish receipts with plain write_text")
for required in (
    "/var/lib/oasis7/p2p-public-testnet/validator-pair-nonces.jsonl",
    "OASIS7_VALIDATOR_PAIR_NONCE_LEDGER",
    "mode `0600`",
    "nonce_ledger_path",
):
    if required not in runbook_source:
        raise SystemExit(f"runbook is missing nonce-ledger prerequisite: {required}")

# Exercise only the shell boundary with a bounded fake executor first.  This
# proves resume is forwarded verbatim (including every authority/credential
# seam) and cannot silently fall through to the historical SSH parser.
with tempfile.TemporaryDirectory(prefix="oasis7-resume-forwarding-") as forwarding_dir:
    forwarding_root = Path(forwarding_dir)
    forwarded_args_path = forwarding_root / "forwarded-args.json"
    forwarded_nonce_path = forwarding_root / "forwarded-nonce-ledger.txt"
    fake_python = forwarding_root / "python3"
    fake_python.write_text(
        f"""#!{sys.executable}
import json
import os
import sys

executor = {str(root / 'scripts' / 'p2p-public-testnet-validator-pair-rebuild.py')!r}
if len(sys.argv) > 1 and sys.argv[1] == executor:
    with open({str(forwarded_args_path)!r}, 'w', encoding='utf-8') as handle:
        json.dump(sys.argv[1:], handle)
    with open({str(forwarded_nonce_path)!r}, 'w', encoding='utf-8') as handle:
        handle.write(os.environ.get('OASIS7_VALIDATOR_PAIR_NONCE_LEDGER', ''))
    print(json.dumps({{'schema_version': 'oasis7.validator_pair_rebuild_transaction.v1', 'phase': 'rolled_back', 'mutation_order': [], 'startup_order': []}}))
else:
    os.execv({sys.executable!r}, [{sys.executable!r}, *sys.argv[1:]])
""",
        encoding="utf-8",
    )
    fake_python.chmod(0o755)
    forwarding_transaction = forwarding_root / "transaction.json"
    forwarding_args = [
        "resume",
        "--transaction",
        str(forwarding_transaction),
        "--host-adapter",
        str(forwarding_root / "host-adapter.py"),
        "--request",
        str(forwarding_root / "live-request.json"),
        "--known-hosts",
        str(forwarding_root / "known-hosts"),
        "--credential-env",
        "OASIS7_RESUME_TEST_SECRET",
    ]
    forwarding_env = dict(os.environ)
    forwarding_env["PATH"] = f"{forwarding_root}:{forwarding_env['PATH']}"
    forwarding_env["OASIS7_VALIDATOR_PAIR_NONCE_LEDGER"] = str(forwarding_root / "nonce-ledger.jsonl")
    forwarded = subprocess.run(
        [str(wrapper_path), *forwarding_args],
        text=True,
        capture_output=True,
        env=forwarding_env,
        check=False,
    )
    if forwarded.returncode != 0:
        raise SystemExit(f"shell resume forwarding failed: {forwarded.stderr}")
    expected_forwarded = [str(root / "scripts" / "p2p-public-testnet-validator-pair-rebuild.py"), *forwarding_args]
    if json.loads(forwarded_args_path.read_text(encoding="utf-8")) != expected_forwarded:
        raise SystemExit("resume did not forward the explicit executor argument vector")
    if forwarded_nonce_path.read_text(encoding="utf-8") != forwarding_env["OASIS7_VALIDATOR_PAIR_NONCE_LEDGER"]:
        raise SystemExit("resume did not forward the explicit external nonce-ledger binding")
    forwarding_receipt = json.loads(forwarded.stdout)
    if forwarding_receipt.get("receipt_contract", {}).get("mode") != "resume":
        raise SystemExit("resume forwarding did not publish a durable resume envelope")
spec = importlib.util.spec_from_file_location("pair_rebuild_contract_fixture", test_path)
if spec is None or spec.loader is None:
    raise SystemExit("cannot load canonical local fixture")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

fixture = module.ValidatorPairRebuildContractTests(methodName="runTest")
fixture.setUp()
try:
    base = fixture._base_args()
    # _base_args is [wrapper, plan, ...].  Replace only the owned dispatcher
    # while retaining the complete current plan argument set.
    shell = root / "scripts" / "p2p-public-testnet-rebuild-validators.sh"
    command = [str(shell), "plan", *base[2:]]
    env = dict(os.environ)
    with tempfile.TemporaryDirectory(prefix="oasis7-rebuild-receipt-contract-bin-") as fake_bin:
        fake = Path(fake_bin)
        # Plan now performs bounded live SSH/GitHub reads. Keep the fixture's
        # loopback-only fake SSH/GitHub tools and forbid only systemd activity.
        for command_name in ("systemctl",):
            path = fake / command_name
            path.write_text("#!/usr/bin/env bash\nprintf '%s\n' \"$0\" >>\"${O7_RECEIPT_FORBIDDEN_LOG:?}\"\nexit 91\n", encoding="utf-8")
            path.chmod(0o755)
        forbidden = fake / "forbidden.log"
        env["O7_RECEIPT_FORBIDDEN_LOG"] = str(forbidden)
        env["PATH"] = f"{fake}:{env['PATH']}"

        first = subprocess.run(command, text=True, capture_output=True, env=env, check=False)
        second = subprocess.run(command, text=True, capture_output=True, env=env, check=False)
    if first.returncode != 0:
        raise SystemExit(f"shell plan failed: {first.stderr}")
    if second.returncode != 0:
        raise SystemExit(f"second shell plan failed: {second.stderr}")
    if first.stdout != second.stdout:
        raise SystemExit("plan output is not byte-stable")
    if forbidden.exists() and forbidden.read_text(encoding="utf-8").strip():
        raise SystemExit("plan mode reached systemd")

    receipt = json.loads(first.stdout)
    contract = receipt.get("receipt_contract")
    if receipt.get("schema_version") != "oasis7.validator_pair_rebuild_plan.v1":
        raise SystemExit("plan schema is not validator_pair_rebuild_plan.v1")
    if not isinstance(contract, dict):
        raise SystemExit("receipt contract envelope is missing")
    if contract.get("remote_activity") != "executor-owned-direct-ssh-read-only" or contract.get("systemd_activity") is not False or contract.get("destructive_activity") is not False:
        raise SystemExit("plan activity gates do not describe bounded read-only observation")
    if contract.get("mutation_order") != ["storage-205", "sequencer-204"]:
        raise SystemExit("mutation order is not explicit")
    if contract.get("startup_order") != ["sequencer-204", "storage-205"]:
        raise SystemExit("startup order is not explicit")
    if contract.get("rollback_boundary", {}).get("restore_deleted_chain_state") is not False:
        raise SystemExit("rollback boundary permits deleted chain-state restore")
    for role in ("storage-205", "sequencer-204"):
        node = contract["nodes"][role]
        if not node["inventory"]["entry_class_equation"]:
            raise SystemExit(f"inventory class equation failed for {role}")
        if not node["forensic_backup"]["non_seed_proof"]["machine_checkable"]:
            raise SystemExit(f"backup non-seed proof missing for {role}")
        if not node["post_delete_absence_proof"]["required"]:
            raise SystemExit(f"post-delete proof missing for {role}")
        targets = node["destructive_target_resolution"]
        if not targets or not all(item["resolution_proof"]["target_under_root"] for item in targets):
            raise SystemExit(f"destructive path proof failed for {role}")

    expected_digest = hashlib.sha256(
        json.dumps(
            {key: item for key, item in receipt.items() if key != "plan_digest"},
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    if receipt.get("plan_digest") != expected_digest:
        raise SystemExit("plan digest does not bind the receipt envelope")

    adapter = fixture._write_host_adapter()
    transaction = fixture.out / "transaction.json"
    applied = subprocess.run(
        [str(shell), "apply", "--transaction", str(transaction), "--host-adapter", str(adapter)],
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )
    if applied.returncode != 0:
        raise SystemExit(f"shell apply failed: {applied.stderr}")
    applied_receipt = json.loads(applied.stdout)
    applied_contract = applied_receipt.get("receipt_contract", {})
    if applied_contract.get("mode") != "apply":
        raise SystemExit("apply receipt envelope mode is missing")
    if applied_contract.get("same_window_fleet_health", {}).get("status") != "verified":
        raise SystemExit("apply receipt is missing same-window fleet health")
    if not applied_contract.get("host_receipts"):
        raise SystemExit("apply receipt is missing phase receipts")

    rolled_back = subprocess.run(
        [str(shell), "rollback", "--transaction", str(transaction), "--host-adapter", str(adapter)],
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )
    if rolled_back.returncode != 0:
        raise SystemExit(f"shell rollback failed: {rolled_back.stderr}")
    rollback_receipt = json.loads(rolled_back.stdout)
    if rollback_receipt.get("receipt_contract", {}).get("rollback_boundary", {}).get("restore_deleted_chain_state") is not False:
        raise SystemExit("rollback receipt weakens deleted-chain-state boundary")

    direct_fixture = fixture.human_direct_fixture
    resume_common = [
        str(shell),
        "resume",
        "--transaction",
        str(transaction),
        "--host-adapter",
        str(adapter),
        "--request",
        str(direct_fixture["request"]),
        "--known-hosts",
        str(direct_fixture["known_hosts"]),
        "--credential-env",
        "HUMAN_DIRECT_SSH_SECRET",
    ]
    missing_adapter = subprocess.run(
        [
            str(shell),
            "resume",
            "--transaction",
            str(transaction),
            "--request",
            str(direct_fixture["request"]),
            "--known-hosts",
            str(direct_fixture["known_hosts"]),
            "--credential-env",
            "HUMAN_DIRECT_SSH_SECRET",
        ],
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )
    if missing_adapter.returncode == 0 or "resume requires --host-adapter" not in missing_adapter.stderr:
        raise SystemExit("resume must reject missing governed host adapter")

    missing_authority = subprocess.run(
        [
            str(shell),
            "resume",
            "--transaction",
            str(transaction),
            "--host-adapter",
            str(adapter),
            "--known-hosts",
            str(direct_fixture["known_hosts"]),
            "--credential-env",
            "HUMAN_DIRECT_SSH_SECRET",
        ],
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )
    if missing_authority.returncode == 0 or "resume requires --request" not in missing_authority.stderr:
        raise SystemExit("resume must reject missing live GitHub authority request")

    missing_nonce_env = dict(env)
    missing_nonce_env.pop("OASIS7_VALIDATOR_PAIR_NONCE_LEDGER", None)
    missing_nonce = subprocess.run(
        resume_common,
        text=True,
        capture_output=True,
        env=missing_nonce_env,
        check=False,
    )
    if missing_nonce.returncode == 0 or "resume requires OASIS7_VALIDATOR_PAIR_NONCE_LEDGER" not in missing_nonce.stderr:
        raise SystemExit("resume must reject an unbound external nonce ledger")

    resumed = subprocess.run(resume_common, text=True, capture_output=True, env=env, check=False)
    if resumed.returncode != 0:
        raise SystemExit(f"shell resume failed: {resumed.stderr}")
    resumed_receipt = json.loads(resumed.stdout)
    if resumed_receipt.get("phase") != "rolled_back":
        raise SystemExit("resume fixture did not preserve terminal transaction phase")
    if resumed_receipt.get("receipt_contract", {}).get("mode") != "resume":
        raise SystemExit("resume receipt envelope mode is missing")
finally:
    fixture.tearDown()

print("ok: governed shell plan receipt is deterministic and resume routing is explicit and bounded")
PY
