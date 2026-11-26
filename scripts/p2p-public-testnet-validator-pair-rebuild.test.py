#!/usr/bin/env python3
"""Contract tests for the governed validator pair rebuild executor.

These tests intentionally exercise only local temporary roots.  They are the
RED contract for task #3324 (predecessor task #3318); no live validator, SSH
target, or observer is used.
"""

from __future__ import annotations

import copy
import datetime as dt
import hashlib
import importlib.util
import json
import os
import signal
import subprocess
import sys
import tempfile
import unittest
import shutil
from pathlib import Path
from typing import Any


def _load_test_helpers():
    helpers_path = Path(__file__).resolve().parent / "p2p-public-testnet-validator-pair-rebuild-test-helpers.py"
    spec = importlib.util.spec_from_file_location("pair_rebuild_test_helpers", helpers_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to load test helpers module from {helpers_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_HELPERS = _load_test_helpers()
ROOT = _HELPERS.ROOT
EXECUTOR = _HELPERS.EXECUTOR
WRAPPER = _HELPERS.WRAPPER
PROVENANCE = _HELPERS.PROVENANCE
RELEASE_BUNDLE = _HELPERS.RELEASE_BUNDLE
sha256 = _HELPERS.sha256
canonical_digest = _HELPERS.canonical_digest
CANONICAL_RESET_SURFACES = _HELPERS.CANONICAL_RESET_SURFACES
FROZEN_HEAD_OID = _HELPERS.FROZEN_HEAD_OID
ValidatorPairRebuildContractBase = _HELPERS.ValidatorPairRebuildContractBase


class ValidatorPairRebuildContractTests(ValidatorPairRebuildContractBase):
    def _write_live_github_fixture(
        self,
        *,
        transaction_id: str = "human-direct-ssh-tx-3481",
        impact_path: Path | None = None,
    ) -> dict[str, Path | str]:
        """Add a local fake-GitHub authority response to the fake-SSH fixture.

        The executor must obtain the authority from the live comment surface,
        not from the request's persisted stop-authority JSON.  The fake
        ``gh`` command is deliberately installed ahead of the real PATH and
        has no network implementation; an implementation which silently
        reaches the real GitHub CLI would therefore fail this fixture.
        """
        fixture = self._write_human_direct_ssh_fixture(
            transaction_id=transaction_id,
            impact_path=impact_path,
        )
        request_path = Path(fixture["request"])
        request = json.loads(request_path.read_text(encoding="utf-8"))
        request["readback"]["quiet_window_seconds"] = 0.1
        request["inventory"] = {
            "schema_version": "oasis7.public_testnet_validator_inventory.v1",
            "nodes": [
                {
                    "node_id": "triad-testnet-sequencer",
                    "role": "sequencer-204",
                    "host": "root@39.104.204.172",
                    "root": "/opt/oasis7/p2p-testnet",
                    "service": "oasis7-triad-sequencer.service",
                    "ports": ["6631", "6831"],
                    "host_key_fingerprint": "SHA256:39-sequencer-pin",
                },
                {
                    "node_id": "triad-testnet-storage",
                    "role": "storage-205",
                    "host": "root@39.104.205.67",
                    "root": "/opt/oasis7/p2p-testnet",
                    "service": "oasis7-triad-storage.service",
                    "ports": ["6632", "6832"],
                    "host_key_fingerprint": "SHA256:39-storage-pin",
                },
            ],
        }
        inventory_sha256 = hashlib.sha256(
            json.dumps(request["inventory"], ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        body_value = {
            "schema_version": "oasis7.validator_pair_rebuild_live_authority.v1",
            "authorized": True,
            "task_uid": request["task_uid"],
            "action": "validator_pair_rebuild",
            "transaction_id": request["transaction_id"],
            "nonce": request["nonce"],
            "inventory_sha256": inventory_sha256,
            "head_oid": FROZEN_HEAD_OID,
            "issued_at": request["issued_at"],
            "expires_at": request["expires_at"],
        }
        body = json.dumps(body_value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
        response = {
            "id": 5452142815,
            "user": {"login": "release-authority-fixture"},
            "body": body,
            "created_at": request["issued_at"],
            "updated_at": request["issued_at"],
            "deleted": False,
            "revoked": False,
            "author_association": "MEMBER",
        }
        request["github_live"] = {
            "provider": "github",
            "repository": "eng-cc/oasis7",
            "issue_number": 3481,
            "comment_id": response["id"],
            "actor": response["user"]["login"],
            "body_sha256": hashlib.sha256(body.encode()).hexdigest(),
            "task_uid": body_value["task_uid"],
            "action": body_value["action"],
            "transaction_id": body_value["transaction_id"],
            "nonce": body_value["nonce"],
            "inventory_sha256": inventory_sha256,
            "head_oid": body_value["head_oid"],
            "issued_at": body_value["issued_at"],
            "expires_at": body_value["expires_at"],
        }
        request["request_digest"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in request.items() if key != "request_digest"},
                ensure_ascii=True,
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        request_path.write_text(json.dumps(request, indent=2, sort_keys=True) + "\n", encoding="utf-8")

        response_path = Path(fixture["fixture"]) / "github-live-comment.json"
        response_path.write_text(json.dumps(response, sort_keys=True) + "\n", encoding="utf-8")
        github_calls = Path(fixture["fixture"]) / "github-api-argv.jsonl"
        fake_gh = Path(fixture["bin"]) / "gh"
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
        fixture["github_response"] = response_path
        fixture["github_calls"] = github_calls
        return fixture

    def _live_github_env(
        self,
        fixture: dict[str, Path | str],
        *,
        unavailable: bool = False,
    ) -> dict[str, str]:
        env = self._human_direct_ssh_env(fixture)
        env["HUMAN_DIRECT_SSH_GITHUB_RESPONSE"] = str(fixture["github_response"])
        env["HUMAN_DIRECT_SSH_GITHUB_LOG"] = str(fixture["github_calls"])
        if unavailable:
            env["HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE"] = "1"
        else:
            env.pop("HUMAN_DIRECT_SSH_GITHUB_UNAVAILABLE", None)
        return env

    def _run_live_human_direct_ssh(
        self,
        fixture: dict[str, Path | str],
        *,
        unavailable: bool = False,
        extra: tuple[str, ...] = (),
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            self._human_direct_ssh_args(fixture, *extra),
            text=True,
            capture_output=True,
            env=self._live_github_env(fixture, unavailable=unavailable),
        )

    def _rewrite_request(self, fixture: dict[str, Path | str], mutate: object) -> dict[str, Any]:
        request_path = Path(fixture["request"])
        request = json.loads(request_path.read_text(encoding="utf-8"))
        mutate(request)
        request["request_digest"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in request.items() if key != "request_digest"},
                ensure_ascii=True,
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        request_path.write_text(json.dumps(request, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return request

    def _rewrite_github_response(self, fixture: dict[str, Path | str], mutate: object) -> dict[str, Any]:
        response_path = Path(fixture["github_response"])
        response = json.loads(response_path.read_text(encoding="utf-8"))
        mutate(response)
        response_path.write_text(json.dumps(response, sort_keys=True) + "\n", encoding="utf-8")
        return response

    def _configure_exact_operator_inventory(self, fixture: dict[str, Path | str]) -> None:
        known_hosts = Path(fixture["known_hosts"])
        known_hosts.write_text(
            "39.104.204.172 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILBk7BgiUTvJZD/LSJLRHUw4eGFFe3eyUs3eG2ZU0xxj\n"
            "39.104.205.67 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILU4G7oGjBJ+tzaZh4nQJjGkPKM1gXQZYT3GWJLMDhZC\n",
            encoding="utf-8",
        )

        def mutate(request: dict[str, Any]) -> None:
            request["hosts"]["sequencer"].update(
                {
                    "host": "root@39.104.204.172",
                    "root": "/opt/oasis7/p2p-testnet",
                    "host_key_fingerprint": "SHA256:7NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU",
                }
            )
            request["hosts"]["storage"].update(
                {
                    "host": "root@39.104.205.67",
                    "root": "/opt/oasis7/p2p-testnet",
                    "host_key_fingerprint": "SHA256:1SVgiaT5JLCw8PsPpVfLE9UyWNf82IJDZsiE7LAa1gI",
                }
            )

        request = self._rewrite_request(fixture, mutate)
        body = json.loads(json.loads(Path(fixture["github_response"]).read_text(encoding="utf-8"))["body"])
        body["inventory_sha256"] = hashlib.sha256(
            json.dumps(request["inventory"], ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        response = json.loads(Path(fixture["github_response"]).read_text(encoding="utf-8"))
        response["body"] = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
        Path(fixture["github_response"]).write_text(json.dumps(response, sort_keys=True) + "\n", encoding="utf-8")
        request["github_live"]["inventory_sha256"] = body["inventory_sha256"]
        request["github_live"]["body_sha256"] = hashlib.sha256(response["body"].encode()).hexdigest()
        request_path = Path(fixture["request"])
        request["request_digest"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in request.items() if key != "request_digest"},
                ensure_ascii=True,
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        request_path.write_text(json.dumps(request, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def test_human_direct_ssh_accepts_only_canonical_read_only_request(self) -> None:
        fixture = self._write_human_direct_ssh_fixture()
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rejects_arbitrary_adapter_input(self) -> None:
        fixture = self._write_human_direct_ssh_fixture()
        adapter = Path(fixture["fixture"]) / "caller-owned-adapter.py"
        adapter.write_text("#!/usr/bin/env python3\nraise SystemExit(0)\n", encoding="utf-8")
        adapter.chmod(0o755)
        result = self._run_human_direct_ssh(fixture, "--host-adapter", str(adapter))
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(adapter.*(forbidden|unsupported|not allowed)|arbitrary.*adapter)")
        self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_requires_pinned_host_key_and_strict_checking(self) -> None:
        fixture = self._write_human_direct_ssh_fixture()
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        rows = [json.loads(line) for line in Path(fixture["ssh_args"]).read_text(encoding="utf-8").splitlines()]
        self.assertGreaterEqual(len(rows), 2)
        for args in rows:
            self.assertIn("StrictHostKeyChecking=yes", args)
            self.assertIn(f"UserKnownHostsFile={Path(fixture['known_hosts']).resolve()}", args)
            self.assertIn("GlobalKnownHostsFile=/dev/null", args)
            self.assertNotIn("StrictHostKeyChecking=no", args)
            self.assertNotIn("UserKnownHostsFile=/dev/null", args)

    def test_human_direct_ssh_rejects_caller_selected_alternate_known_hosts_path(self) -> None:
        fixture = self._write_live_github_fixture()
        alternate = Path(fixture["fixture"]) / "caller-selected-known_hosts"
        shutil.copyfile(fixture["known_hosts"], alternate)
        alternate.chmod(0o600)
        self._rewrite_request(fixture, lambda request: request.update({"known_hosts_path": str(alternate)}))
        result = self._run_live_human_direct_ssh(fixture, extra=("--known-hosts", str(alternate)))
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(canonical|executor|known.?hosts|production|path|binding)")
        self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rejects_conflicting_duplicate_host_key(self) -> None:
        fixture = self._write_live_github_fixture()
        known_hosts = Path(fixture["known_hosts"])
        known_hosts.write_text(
            known_hosts.read_text(encoding="utf-8")
            + "39.104.204.172 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILU4G7oGjBJ+tzaZh4nQJjGkPKM1gXQZYT3GWJLMDhZC\n",
            encoding="utf-8",
        )
        known_hosts.chmod(0o600)
        result = self._run_live_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(duplicate|conflict|exactly one|known.?hosts|fingerprint|pin)")
        self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rejects_non_private_executor_known_hosts_mode(self) -> None:
        fixture = self._write_live_github_fixture()
        Path(fixture["known_hosts"]).chmod(0o644)
        result = self._run_live_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(known.?hosts|mode|permission|600|private|owner)")
        self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_binds_fixed_role_root_service_and_command_allowlist(self) -> None:
        fixture = self._write_human_direct_ssh_fixture()
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = Path(fixture["ssh_commands"]).read_text(encoding="utf-8")
        self.assertIn("ps -eo pid=,args=", commands)
        self.assertIn("ss -ltn", commands)
        self.assertIn("sequencer", commands)
        self.assertIn("storage", commands)
        self.assertNotRegex(commands, r"(?i)(systemctl|reset|stage|start|stop|rm\s+-rf|mkdir)")

    def test_human_direct_ssh_rejects_non_allowlisted_role_root_service_or_command(self) -> None:
        for field in ("role", "root", "service", "command"):
            with self.subTest(field=field):
                fixture = self._write_human_direct_ssh_fixture(mismatched=field)
                result = self._run_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, rf"(?i)({field}|allow.?list|fixed|canonical|binding)")

    def test_human_direct_ssh_rejects_observer_mutation_true(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(mismatched="observer")
        result = self._run_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(observer|mutation|read.?only|quiescen)")

    def test_human_direct_ssh_keeps_secret_out_of_argv_logs_and_receipt(self) -> None:
        fixture = self._write_human_direct_ssh_fixture()
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        secret = str(fixture["secret"])
        for path in (fixture["ssh_args"], fixture["ssh_commands"], fixture["sshpass"], fixture["receipt"]):
            if Path(path).exists():
                self.assertNotIn(secret, Path(path).read_text(encoding="utf-8"))
        self.assertNotIn(secret, result.stdout)
        self.assertNotIn(secret, result.stderr)
        seam_rows = [json.loads(line) for line in Path(fixture["ssh_seam"]).read_text(encoding="utf-8").splitlines()]
        self.assertTrue(any(row["environment_binding_present"] or row["temporary_fd_present"] for row in seam_rows))
        self.assertNotRegex(
            Path(fixture["ssh_args"]).read_text(encoding="utf-8") if Path(fixture["ssh_args"]).exists() else "",
            r"(?i)(--password|--secret|secret=|password=)",
        )

    def test_human_direct_ssh_password_mode_omits_batch_mode_and_key_mode_keeps_it(self) -> None:
        password_fixture = self._write_human_direct_ssh_fixture()
        password_result = self._run_human_direct_ssh(password_fixture)
        self.assertEqual(password_result.returncode, 0, password_result.stderr)
        password_rows = [
            json.loads(line)
            for line in Path(password_fixture["sshpass"]).read_text(encoding="utf-8").splitlines()
        ]
        self.assertTrue(password_rows)
        for row in password_rows:
            self.assertTrue(row["secret_env"])
            self.assertNotIn("BatchMode=yes", row["argv"])

        key_fixture = self._write_human_direct_ssh_fixture()
        key_args = self._human_direct_ssh_args(key_fixture)
        credential_index = key_args.index("--credential-env")
        del key_args[credential_index : credential_index + 2]
        key_env = self._human_direct_ssh_env(key_fixture)
        key_env.pop("HUMAN_DIRECT_SSH_SECRET", None)
        key_result = subprocess.run(key_args, text=True, capture_output=True, env=key_env)
        self.assertEqual(key_result.returncode, 0, key_result.stderr)
        key_rows = [
            json.loads(line)
            for line in Path(key_fixture["ssh_args"]).read_text(encoding="utf-8").splitlines()
        ]
        self.assertTrue(key_rows)
        for row in key_rows:
            self.assertIn("BatchMode=yes", row)
        self.assertFalse(Path(key_fixture["sshpass"]).exists())

    def test_human_direct_ssh_rejects_stale_or_replayed_transaction_request_fingerprint_nonce(self) -> None:
        cases = (
            ({"stale": True}, r"(?i)(stale|expired|timestamp|nonce)"),
            ({"expired": True}, r"(?i)(expired|expiry|stale|timestamp)"),
            ({"replayed": True}, r"(?i)(replay|nonce|transaction)"),
            ({"mismatched": "transaction"}, r"(?i)(transaction|binding)"),
            ({"mismatched": "request"}, r"(?i)(request.*digest|binding|digest)"),
            ({"mismatched": "fingerprint"}, r"(?i)(fingerprint|host.?key|binding)"),
        )
        for options, pattern in cases:
            with self.subTest(options=options):
                fixture = self._write_human_direct_ssh_fixture(**options)
                result = self._run_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, pattern)

    def test_human_direct_ssh_rejects_active_process_or_listener(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(state="active")
        result = self._run_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(active|listener|quiescen|running)")

    def test_human_direct_ssh_rejects_active_process_readback(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(state="active-process")
        result = self._run_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(active.*process|running|quiescen)")

    def test_human_direct_ssh_rejects_active_listener_readback(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(state="active-listener")
        result = self._run_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(active.*listener|listener|quiescen)")

    def test_human_direct_ssh_ignores_unrelated_process_noise_during_quiet_window(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(state="flapping")
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_human_direct_ssh_rejects_canonical_state_changes_during_quiet_window(self) -> None:
        for state in ("canonical-process-flapping", "canonical-listener-flapping"):
            with self.subTest(state=state):
                fixture = self._write_human_direct_ssh_fixture(state=state)
                result = self._run_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, r"(?i)(active|listener|process|quiescen|running)")

    def test_human_direct_ssh_quiesce_never_uses_systemctl_or_mutation_commands(self) -> None:
        fixture = self._write_human_direct_ssh_fixture()
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(Path(fixture["systemctl"]).read_text(encoding="utf-8"), "")
        commands = Path(fixture["ssh_commands"]).read_text(encoding="utf-8")
        self.assertNotRegex(commands, r"(?i)(systemctl|reset|stage|start|stop|kill|rm\s+-rf|mkdir|cp\s)")

    def test_human_direct_ssh_forbids_full_204_chain_status(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(full_status=True)
        result = self._run_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(204|full.*status|/v1/chain/status|status.*forbidden)")

    def test_human_direct_ssh_rejects_caller_or_persisted_authority_without_live_github_comment(self) -> None:
        for source in ("caller", "persisted"):
            with self.subTest(source=source):
                fixture = self._write_live_github_fixture()

                def mutate(request: dict[str, Any]) -> None:
                    request.pop("github_live", None)
                    authority = {
                        "authorized": True,
                        "schema_version": "oasis7.human_stop_authority.v1",
                        "task_uid": request["task_uid"],
                        "transaction_id": request["transaction_id"],
                        "nonce": request["nonce"],
                        "stopped_at": request["issued_at"],
                    }
                    if source == "caller":
                        request["caller_authority"] = authority
                    else:
                        persisted = Path(fixture["fixture"]) / "persisted-authority.json"
                        persisted.write_text(json.dumps(authority) + "\n", encoding="utf-8")
                        request["persisted_authority_path"] = str(persisted)

                self._rewrite_request(fixture, mutate)
                result = self._run_live_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, r"(?i)(live|github|authority|comment|evidence|capability)")
                self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_requires_live_github_binding_for_actor_comment_body_task_action_transaction_nonce_inventory_head_and_freshness(self) -> None:
        def body_mutator(field: str, response: dict[str, Any]) -> None:
            if field == "actor":
                response["user"]["login"] = "untrusted-actor"
            elif field == "comment_id":
                response["id"] = 5452142816
            else:
                body = json.loads(response["body"])
                if field == "body_sha256":
                    body["action"] = "caller-authored-action"
                elif field == "task_uid":
                    body[field] = "foreign-task-3481"
                elif field == "action":
                    body[field] = "reset_and_start"
                elif field == "transaction_id":
                    body[field] = "foreign-transaction-3481"
                elif field == "nonce":
                    body[field] = "foreign-nonce-3481"
                elif field == "inventory_sha256":
                    body[field] = "f" * 64
                elif field == "head_oid":
                    body[field] = "e" * 40
                elif field == "freshness":
                    body["issued_at"] = "2000-01-01T00:00:00Z"
                    body["expires_at"] = "2000-01-01T00:01:00Z"
                response["body"] = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":"))

        for field in (
            "actor",
            "comment_id",
            "body_sha256",
            "task_uid",
            "action",
            "transaction_id",
            "nonce",
            "inventory_sha256",
            "head_oid",
            "freshness",
        ):
            with self.subTest(binding=field):
                fixture = self._write_live_github_fixture()
                self._rewrite_github_response(fixture, lambda response, field=field: body_mutator(field, response))
                if field == "freshness":
                    response = json.loads(Path(fixture["github_response"]).read_text(encoding="utf-8"))
                    body_sha256 = hashlib.sha256(response["body"].encode()).hexdigest()

                    def bind_stale_body(request: dict[str, Any]) -> None:
                        request["github_live"]["body_sha256"] = body_sha256

                    self._rewrite_request(fixture, bind_stale_body)
                result = self._run_live_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(
                    result.stderr,
                    r"(?i)(live|github|authority|actor|comment|digest|task|action|transaction|nonce|inventory|head|fresh|expired|binding)",
                )
                self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rejects_authority_head_not_matching_executor_frozen_head(self) -> None:
        fixture = self._write_live_github_fixture()

        def mutate_response(response: dict[str, Any]) -> None:
            body = json.loads(response["body"])
            body["head_oid"] = "e" * 40
            response["body"] = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":"))

        response = self._rewrite_github_response(fixture, mutate_response)
        authority_head = json.loads(response["body"])["head_oid"]
        authority_body_sha256 = hashlib.sha256(response["body"].encode()).hexdigest()

        def bind_rewritten_authority(request: dict[str, Any]) -> None:
            request["github_live"]["head_oid"] = authority_head
            request["github_live"]["body_sha256"] = authority_body_sha256

        self._rewrite_request(fixture, bind_rewritten_authority)
        result = self._run_live_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(frozen|executor|head|commit|binding)")
        self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_fails_closed_for_edited_deleted_revoked_or_unavailable_github_authority(self) -> None:
        for state in ("edited", "deleted", "revoked", "api_unavailable"):
            with self.subTest(state=state):
                fixture = self._write_live_github_fixture()
                if state == "api_unavailable":
                    result = self._run_live_human_direct_ssh(fixture, unavailable=True)
                else:
                    def mutate(response: dict[str, Any], state=state) -> None:
                        response[state] = True
                        if state == "edited":
                            response["updated_at"] = "2000-01-01T00:00:01Z"

                    self._rewrite_github_response(fixture, mutate)
                    result = self._run_live_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, r"(?i)(live|github|comment|edited|deleted|revoked|unavailable|api|authority)")
                self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rejects_caller_host_known_hosts_root_and_service_overrides_with_live_authority(self) -> None:
        for field in ("host", "known_hosts", "root", "service"):
            with self.subTest(override=field):
                fixture = self._write_live_github_fixture()

                def mutate(request: dict[str, Any]) -> None:
                    if field == "host":
                        request["hosts"]["sequencer"].update(
                            {
                                "host": "root@storage.example",
                                "host_key_fingerprint": "SHA256:storage-fixture-pin",
                            }
                        )
                    elif field == "known_hosts":
                        alternate = Path(fixture["fixture"]) / "caller-known_hosts"
                        alternate.write_text(Path(fixture["known_hosts"]).read_text(encoding="utf-8"), encoding="utf-8")
                        request["known_hosts_path"] = str(alternate)
                    elif field == "root":
                        request["hosts"]["sequencer"]["root"] = "/opt/caller-owned-root"
                    else:
                        request["hosts"]["sequencer"]["service"] = "caller-owned.service"

                self._rewrite_request(fixture, mutate)
                result = self._run_live_human_direct_ssh(fixture)
                self.assertNotEqual(result.returncode, 0)
                self.assertRegex(result.stderr, rf"(?i)({field}|host|known.?hosts|root|service|allow.?list|binding|github|authority)")
                self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_accepts_only_exact_repo_inventory_for_204_and_205_opt_roots_and_pins(self) -> None:
        fixture = self._write_live_github_fixture()
        self._configure_exact_operator_inventory(fixture)
        result = self._run_live_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(Path(fixture["receipt"]).read_text(encoding="utf-8"))
        self.assertEqual(receipt["hosts"]["sequencer-204"]["host"], "root@39.104.204.172")
        self.assertEqual(receipt["hosts"]["storage-205"]["host"], "root@39.104.205.67")
        self.assertEqual(receipt["hosts"]["sequencer-204"]["root"], "/opt/oasis7/p2p-testnet")
        self.assertEqual(receipt["hosts"]["storage-205"]["root"], "/opt/oasis7/p2p-testnet")
        self.assertEqual(receipt["host_fingerprints"]["sequencer-204"], "SHA256:7NkC2GehDCcN+IWPbaxh+0JuIVGCEtKpdK69S6fHZPU")
        self.assertEqual(receipt["host_fingerprints"]["storage-205"], "SHA256:1SVgiaT5JLCw8PsPpVfLE9UyWNf82IJDZsiE7LAa1gI")

    def test_human_direct_ssh_rejects_live_inventory_digest_not_equal_to_repo_inventory(self) -> None:
        fixture = self._write_live_github_fixture()

        def mutate(response: dict[str, Any]) -> None:
            body = json.loads(response["body"])
            body["inventory_sha256"] = "a" * 64
            response["body"] = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":"))

        self._rewrite_github_response(fixture, mutate)
        response = json.loads(Path(fixture["github_response"]).read_text(encoding="utf-8"))
        body = json.loads(response["body"])
        self._rewrite_request(
            fixture,
            lambda request: request["github_live"].update(
                {
                    "inventory_sha256": body["inventory_sha256"],
                    "body_sha256": hashlib.sha256(response["body"].encode()).hexdigest(),
                }
            ),
        )
        result = self._run_live_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(inventory|repository|canonical|binding|github|authority)")
        self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rewrite_of_receipt_or_proof_cannot_authorize_plan_admission(self) -> None:
        fixture = self._write_live_github_fixture(
            transaction_id=self.quiescence_id,
            impact_path=self.impact,
        )
        direct = self._run_live_human_direct_ssh(fixture)
        self.assertEqual(direct.returncode, 0, direct.stderr)
        request = json.loads(Path(fixture["request"]).read_text(encoding="utf-8"))
        fake_authority = request["github_live"]
        for name in ("receipt.json", "quiescence-adapter-receipt.json"):
            path = Path(fixture["out"]) / name
            value = json.loads(path.read_text(encoding="utf-8"))
            value["github_live"] = fake_authority
            value["canonical_digest"] = canonical_digest(value)
            path.write_text(json.dumps(value, sort_keys=True) + "\n", encoding="utf-8")
        proof_path = Path(fixture["out"]) / "quiescence-proof.json"
        proof = json.loads(proof_path.read_text(encoding="utf-8"))
        source_path = Path(fixture["out"]) / "quiescence-adapter-receipt.json"
        proof["github_live"] = fake_authority
        proof["source_receipt_sha256"] = sha256(source_path)
        proof["canonical_digest"] = canonical_digest(proof)
        proof_path.write_text(json.dumps(proof, sort_keys=True) + "\n", encoding="utf-8")
        self._rewrite_github_response(fixture, lambda response: response.update({"revoked": True}))
        self.quiescence_proof = proof_path
        result = subprocess.run(
            self._base_args(),
            text=True,
            capture_output=True,
            env=self._live_github_env(fixture),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(live|github|revoked|authority|receipt|proof|admission|evidence)")
        self.assertFalse((self.out / "transaction.json").exists())

    def test_apply_repeats_live_github_authority_and_fake_ssh_before_any_mutation(self) -> None:
        fixture = self._write_live_github_fixture(
            transaction_id=self.quiescence_id,
            impact_path=self.impact,
        )
        audit = {
            "audit_only": True,
            "direct_request_path": str(fixture["request"]),
            "direct_request_sha256": sha256(Path(fixture["request"])),
            "known_hosts_path": str(fixture["known_hosts"]),
            "credential_binding": {
                "kind": "temporary-fd-or-environment",
                "environment_name": "HUMAN_DIRECT_SSH_SECRET",
            },
        }
        audit_path = self.root / "executor-routing-audit.json"
        audit["canonical_digest"] = canonical_digest(audit)
        audit_path.write_text(json.dumps(audit, sort_keys=True) + "\n", encoding="utf-8")
        self.quiescence_proof = audit_path
        planned = subprocess.run(
            self._base_args(),
            text=True,
            capture_output=True,
            env=self._live_github_env(fixture),
        )
        self.assertEqual(planned.returncode, 0, planned.stderr)
        transaction_path = self.out / "transaction.json"
        transaction = json.loads(transaction_path.read_text(encoding="utf-8"))
        request = json.loads(Path(fixture["request"]).read_text(encoding="utf-8"))
        transaction.update(
            {
                "github_live": request["github_live"],
                "live_ssh_request_path": str(fixture["request"]),
                "live_ssh_known_hosts_path": str(fixture["known_hosts"]),
            }
        )
        transaction["plan_digest"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in transaction.items() if key != "plan_digest"},
                ensure_ascii=True,
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        transaction_path.write_text(json.dumps(transaction, sort_keys=True) + "\n", encoding="utf-8")
        self._rewrite_github_response(fixture, lambda response: response.update({"revoked": True}))
        adapter = self._write_host_adapter()
        before = {
            role: (self.nodes[role] / "current" / "bin" / "oasis7_chain_runtime").read_bytes()
            for role in self.nodes
        }
        ssh_before = Path(fixture["ssh_commands"]).read_text(encoding="utf-8")
        result = subprocess.run(
            [
                str(WRAPPER),
                "apply",
                "--transaction",
                str(transaction_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(fixture["request"]),
                "--known-hosts",
                str(fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=self._live_github_env(fixture),
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(live|github|revoked|authority|ssh|before.*mutation|admission)")
        for role in self.nodes:
            self.assertEqual((self.nodes[role] / "current" / "bin" / "oasis7_chain_runtime").read_bytes(), before[role])
        self.assertEqual(Path(fixture["ssh_commands"]).read_text(encoding="utf-8"), ssh_before)

    def test_human_direct_ssh_rejects_pending_nonce_ledger_outside_output_and_worktree(self) -> None:
        fixture = self._write_live_github_fixture()
        with tempfile.TemporaryDirectory(prefix="oasis7-human-direct-nonce-") as ledger_dir:
            ledger = Path(ledger_dir) / "nonce-ledger.json"
            request = json.loads(Path(fixture["request"]).read_text(encoding="utf-8"))
            self._rewrite_request(fixture, lambda value: value.update({"nonce_ledger_path": str(ledger)}))
            request = json.loads(Path(fixture["request"]).read_text(encoding="utf-8"))
            ledger.write_text(
                json.dumps(
                    {
                        "schema_version": "oasis7.validator_pair_rebuild_nonce.v1",
                        "nonce": request["nonce"],
                        "task_uid": request["task_uid"],
                        "transaction_id": request["transaction_id"],
                        "request_digest": request["request_digest"],
                        "authority_sha256": request["github_live"]["body_sha256"],
                        "state": "pending",
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            ledger.chmod(0o600)
            result = self._run_live_human_direct_ssh(fixture)
            self.assertNotEqual(result.returncode, 0)
            self.assertRegex(result.stderr, r"(?i)(nonce|pending|ledger|replay|authority|github)")
            self.assertFalse(Path(fixture["out"]) / "nonce-ledger.json" == ledger)
            self.assertFalse(Path(fixture["receipt"]).exists())

    def test_human_direct_ssh_rejects_replayed_nonce_from_external_ledger(self) -> None:
        fixture = self._write_live_github_fixture()
        first = self._run_live_human_direct_ssh(fixture)
        self.assertEqual(first.returncode, 0, first.stderr)
        replay_out = Path(fixture["fixture"]) / "replay-out"
        ssh_after_first = Path(fixture["ssh_commands"]).read_text(encoding="utf-8")
        replay = self._run_live_human_direct_ssh(fixture, extra=("--out-dir", str(replay_out)))
        self.assertNotEqual(replay.returncode, 0)
        self.assertRegex(replay.stderr, r"(?i)(nonce|replay|consumed|ledger|authority|github)")
        self.assertFalse((replay_out / "receipt.json").exists())
        self.assertEqual(Path(fixture["ssh_commands"]).read_text(encoding="utf-8"), ssh_after_first)


    def test_plan_requires_verified_provenance_and_emits_both_orders(self) -> None:
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["schema_version"], "oasis7.validator_pair_rebuild_plan.v1")
        self.assertRegex(receipt["plan_digest"], r"^[0-9a-f]{64}$")
        self.assertEqual(receipt["mutation_order"], ["storage-205", "sequencer-204"])
        self.assertEqual(receipt["startup_order"], ["sequencer-204", "storage-205"])
        self.assertEqual(receipt["phase"], "planned")

    def test_plan_receipt_truthfully_reports_direct_remote_observation(self) -> None:
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["receipt_contract"]["remote_activity"], "executor-owned-direct-ssh-read-only")
        self.assertFalse(receipt["receipt_contract"]["systemd_activity"])
        self.assertFalse(receipt["receipt_contract"]["destructive_activity"])

    def test_active_consumer_impact_approval_is_stop_authority_without_stopped_claim(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_active_impact_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")

        normalized = module.validate_impact(self.impact)

        self.assertEqual(normalized["decision"], "proceed")
        self.assertEqual(normalized["impact"], "active")
        self.assertFalse(normalized["validators_already_stopped"])

    def test_active_pair_plan_rejects_self_asserted_stopped_boolean_without_independent_proof(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": True,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        self._rewrite_request(
            self.human_direct_fixture,
            lambda request: request.update({"impact_record_sha256": sha256(self.impact)}),
        )
        proof = json.loads(self.quiescence_proof.read_text(encoding="utf-8"))
        proof["direct_request_sha256"] = sha256(Path(self.human_direct_fixture["request"]))
        proof["canonical_digest"] = canonical_digest(proof)
        self.quiescence_proof.write_text(json.dumps(proof, sort_keys=True) + "\n", encoding="utf-8")
        Path(self.human_direct_fixture["nonce_ledger"]).write_text("", encoding="utf-8")
        Path(self.human_direct_fixture["nonce_ledger"]).chmod(0o600)
        previous_state = os.environ.get("HUMAN_DIRECT_SSH_FAKE_STATE")
        os.environ["HUMAN_DIRECT_SSH_FAKE_STATE"] = "active"
        try:
            result = subprocess.run(self._base_args(), text=True, capture_output=True)
        finally:
            if previous_state is None:
                os.environ.pop("HUMAN_DIRECT_SSH_FAKE_STATE", None)
            else:
                os.environ["HUMAN_DIRECT_SSH_FAKE_STATE"] = previous_state

        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(
            result.stderr.lower(),
            r"independently verified.*(?:stopped|quiescence).*proof|stopped-state receipt|active|running|quiescence",
        )
        self.assertFalse(self.out.exists())

    def test_quiesce_emits_bounded_two_role_stopped_proof_without_mutation(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        fixture = self._write_human_direct_ssh_fixture(
            transaction_id="active-quiescence-3481",
            impact_path=self.impact,
        )
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(Path(fixture["receipt"]).read_text(encoding="utf-8"))
        self.assertEqual(receipt["phase"], "quiesce")
        self.assertEqual(set(receipt["nodes"]), {"storage-205", "sequencer-204"})
        self.assertTrue(all(item["active"] is False and item["running"] is False for item in receipt["nodes"].values()))
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())
        self.assertFalse((self.nodes["sequencer-204"] / "backups").exists())

    def test_quiesce_rejects_caller_attestable_fabricated_adapter_provenance(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        adapter = self._write_quiescence_adapter()
        source = adapter.read_text(encoding="utf-8")
        source = source.replace(
            "    'observer_mutation': False,\n",
            "    'observer_mutation': False,\n"
            "    'repository_executable': {\n"
            "        'schema_version': 'oasis7.validator_pair_rebuild_repository_executable.v1',\n"
            "        'path': 'caller-authored-adapter.py',\n"
            "        'sha256': '0' * 64,\n"
            "    },\n",
        )
        adapter.write_text(source, encoding="utf-8")
        out_dir = self.root / "fabricated-quiescence"
        result = subprocess.run(
            [
                sys.executable,
                str(EXECUTOR),
                "quiesce",
                "--consumer-impact-record",
                str(self.impact),
                "--quiescence-id",
                "fabricated-quiescence-3481",
                "--node",
                f"storage-205=local:{self.nodes['storage-205']}",
                "--node",
                f"sequencer-204=local:{self.nodes['sequencer-204']}",
                "--host-adapter",
                str(adapter),
                "--out-dir",
                str(out_dir),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr.lower(), r"capability_blocked|trusted|attest|provenance|observe")
        self.assertFalse((out_dir / "quiescence-proof.json").exists())

    def test_quiesce_rejects_freshly_finalized_arbitrary_adapter_provenance(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        # This adapter is freshly written and chmod-finalized, so the current
        # mtime/ctime heuristic accepts it even though no governed producer
        # attestation or host observation exists.
        adapter = self._write_quiescence_adapter()
        out_dir = self.root / "fresh-arbitrary-quiescence"
        result = subprocess.run(
            [
                sys.executable,
                str(EXECUTOR),
                "quiesce",
                "--consumer-impact-record",
                str(self.impact),
                "--quiescence-id",
                "fresh-arbitrary-quiescence-3481",
                "--node",
                f"storage-205=local:{self.nodes['storage-205']}",
                "--node",
                f"sequencer-204=local:{self.nodes['sequencer-204']}",
                "--host-adapter",
                str(adapter),
                "--out-dir",
                str(out_dir),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr.lower(), r"trusted|attest|provenance|producer|observe")
        self.assertFalse((out_dir / "quiescence-proof.json").exists())

    def test_plan_rejects_hand_authored_stopped_quiescence_proof_without_producer_attestation(self) -> None:
        generated_proof = json.loads(self.quiescence_proof.read_text(encoding="utf-8"))
        generated_source_path = Path(generated_proof["source_receipt_path"])
        generated_source = json.loads(generated_source_path.read_text(encoding="utf-8"))

        # Re-home the producer output as a caller-authored pair.  Every
        # currently checked digest, order, role, and stopped-state field is
        # internally consistent, but no governed producer attestation binds
        # this hand-authored source/proof pair to an adapter invocation.
        hand_source_path = self.root / "hand-authored-quiescence-source.json"
        hand_source_path.write_text(json.dumps(generated_source, indent=2) + "\n", encoding="utf-8")
        hand_proof = copy.deepcopy(generated_proof)
        hand_proof["source_receipt_path"] = str(hand_source_path)
        hand_proof["source_receipt_sha256"] = sha256(hand_source_path)
        hand_proof_path = self.root / "hand-authored-quiescence-proof.json"
        hand_proof_path.write_text(json.dumps(hand_proof, indent=2) + "\n", encoding="utf-8")

        args = self._base_args()
        args[args.index("--stopped-quiescence-proof") + 1] = str(hand_proof_path)
        result = subprocess.run(args, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr.lower(), r"trusted|attest|provenance|producer")
        self.assertFalse(self.out.exists())

    def test_consumer_impact_requires_strict_boolean_and_rfc3339_timestamp(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_strict_impact_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        for invalid_value in ("false", 0, 1, None):
            with self.subTest(validators_already_stopped=invalid_value):
                impact = json.loads(self.impact.read_text(encoding="utf-8"))
                impact["validators_already_stopped"] = invalid_value
                self.impact.write_text(json.dumps(impact), encoding="utf-8")
                with self.assertRaisesRegex(SystemExit, "boolean"):
                    module.validate_impact(self.impact)

        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact["timestamp"] = "2026-08-28 00:00:00+00:00"
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        with self.assertRaisesRegex(SystemExit, "RFC3339"):
            module.validate_impact(self.impact)

    def test_quiesce_rejects_observer_mutation_receipt(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        adapter = self._write_quiescence_adapter()
        adapter_source = adapter.read_text(encoding="utf-8")
        adapter.write_text(
            adapter_source.replace(
                "    'observer_mutation': False,\n",
                "    'observer_mutation': True,\n",
            ),
            encoding="utf-8",
        )
        out_dir = self.root / "observer-mutation"
        result = subprocess.run(
            [
                sys.executable,
                str(EXECUTOR),
                "quiesce",
                "--consumer-impact-record",
                str(self.impact),
                "--quiescence-id",
                "observer-mutation-3481",
                "--node",
                f"storage-205=local:{self.nodes['storage-205']}",
                "--node",
                f"sequencer-204=local:{self.nodes['sequencer-204']}",
                "--host-adapter",
                str(adapter),
                "--out-dir",
                str(out_dir),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr.lower(), r"capability_blocked|observer")
        self.assertFalse((out_dir / "quiescence-proof.json").exists())

    def test_plan_rejects_symlink_stopped_quiescence_proof(self) -> None:
        proof_link = self.root / "quiescence-proof-link.json"
        proof_link.symlink_to(self.quiescence_proof)
        args = self._base_args()
        args[args.index("--stopped-quiescence-proof") + 1] = str(proof_link)
        result = subprocess.run(args, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr.lower(), r"regular file|symlink")
        self.assertFalse(self.out.exists())

    def test_plan_requires_explicit_transaction_id_in_stopped_quiescence_proof(self) -> None:
        proof = json.loads(self.quiescence_proof.read_text(encoding="utf-8"))
        proof.pop("transaction_id", None)
        self.quiescence_proof.write_text(json.dumps(proof), encoding="utf-8")
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(
            result.stderr.lower(),
            r"capability_blocked|transaction.*identity|transaction_id|persisted.*proof|audit.*record|producer.*attestation",
        )
        self.assertFalse(self.out.exists())

    def test_plan_requires_explicit_transaction_id_in_stopped_quiescence_source_receipt(self) -> None:
        proof = json.loads(self.quiescence_proof.read_text(encoding="utf-8"))
        source_path = Path(proof["source_receipt_path"])
        source = json.loads(source_path.read_text(encoding="utf-8"))
        source.pop("transaction_id", None)
        hand_source_path = self.root / "missing-source-transaction-id.json"
        hand_source_path.write_text(json.dumps(source, indent=2) + "\n", encoding="utf-8")
        proof["source_receipt_path"] = str(hand_source_path)
        proof["source_receipt_sha256"] = sha256(hand_source_path)
        hand_proof_path = self.root / "missing-source-transaction-id-proof.json"
        hand_proof_path.write_text(json.dumps(proof, indent=2) + "\n", encoding="utf-8")

        args = self._base_args()
        args[args.index("--stopped-quiescence-proof") + 1] = str(hand_proof_path)
        result = subprocess.run(args, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(
            result.stderr.lower(),
            r"capability_blocked|transaction.*identity|transaction_id|persisted.*proof|audit.*record|producer.*attestation",
        )
        self.assertFalse(self.out.exists())

    def test_quiesce_requires_explicit_transaction_id_in_adapter_receipt(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "active",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        adapter = self._write_quiescence_adapter()
        adapter_source = adapter.read_text(encoding="utf-8")
        adapter.write_text(
            adapter_source.replace("    'transaction_id': transaction['transaction_id'],\n", ""),
            encoding="utf-8",
        )
        out_dir = self.root / "missing-transaction-id"
        result = subprocess.run(
            [
                sys.executable,
                str(EXECUTOR),
                "quiesce",
                "--consumer-impact-record",
                str(self.impact),
                "--quiescence-id",
                "missing-transaction-id-3481",
                "--node",
                f"storage-205=local:{self.nodes['storage-205']}",
                "--node",
                f"sequencer-204=local:{self.nodes['sequencer-204']}",
                "--host-adapter",
                str(adapter),
                "--out-dir",
                str(out_dir),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr.lower(), r"capability_blocked|transaction.*identity|transaction_id")
        self.assertFalse((out_dir / "quiescence-proof.json").exists())

    def test_quiesce_supports_none_impact_with_previously_stopped_validators(self) -> None:
        fixture = self._write_human_direct_ssh_fixture(
            transaction_id="none-impact-quiescence-3481",
            impact_path=self.impact,
        )
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(Path(fixture["receipt"]).read_text(encoding="utf-8"))
        self.assertEqual(receipt["phase"], "quiesce")
        self.assertEqual(set(receipt["nodes"]), {"storage-205", "sequencer-204"})
        self.assertTrue(all(item["active"] is False and item["running"] is False for item in receipt["nodes"].values()))
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())
        self.assertFalse((self.nodes["sequencer-204"] / "backups").exists())

    def test_wrapper_quiesce_envelope_forbids_mutation_phases(self) -> None:
        impact = json.loads(self.impact.read_text(encoding="utf-8"))
        impact.update(
            {
                "impact": "unknown",
                "evidence_source": "live public-testnet consumer impact record",
                "timestamp": "2026-08-28T00:00:00Z",
                "validators_already_stopped": False,
                "outage_update_channel": "issue-3481 outage update thread",
                "recovery_update_checkpoint": "issue-3481 Phase G recovery checkpoint",
                "producer_wording_approval": "issue-3481 producer approval",
                "decision": "proceed",
            }
        )
        self.impact.write_text(json.dumps(impact), encoding="utf-8")
        fixture = self._write_human_direct_ssh_fixture(
            transaction_id="wrapper-quiescence-3481",
            impact_path=self.impact,
        )
        result = self._run_human_direct_ssh(fixture)
        self.assertEqual(result.returncode, 0, result.stderr)
        envelope = json.loads(result.stdout)
        contract = envelope["receipt_contract"]
        self.assertEqual(contract["mode"], "human_direct_ssh")
        self.assertTrue(contract["quiescence_only"])
        self.assertFalse(contract["destructive_activity"])
        self.assertEqual(contract["reset"], "forbidden")
        self.assertEqual(contract["stage"], "forbidden")
        self.assertEqual(contract["authority"], "human_direct_ssh")
        self.assertEqual(contract["credential_seam"], "temporary-fd-or-environment; secret-free argv/log/receipt")
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_remote_identity_receipt_binds_metadata_without_local_key_access(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_remote_identity_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        receipt = self.root / "remote-identity-receipt.json"
        receipt.write_text(
            json.dumps(
                {
                    "schema_version": "oasis7.identity_receipt.v1",
                    "node_id": "triad-testnet-storage",
                    "peer_id": "peer-storage-205",
                    "key_path": "/remote/validator/config/node-keypair.toml",
                    "key_sha256": "a" * 64,
                    "key_size_bytes": 321,
                    "key_mode": 0o600,
                    "key_uid": 1001,
                    "key_gid": 1001,
                }
            ),
            encoding="utf-8",
        )
        expected_metadata = module._expected_identity_metadata(
            {
                "expected_key_mode": 0o600,
                "expected_key_uid": 1001,
                "expected_key_gid": 1001,
            },
            "identity manifest entry",
        )
        self.assertEqual(
            module._expected_identity_binding(
                {
                    "expected_node_id": "triad-testnet-storage",
                    "expected_peer_id": "peer-storage-205",
                },
                "identity manifest entry",
            ),
            {"node_id": "triad-testnet-storage", "peer_id": "peer-storage-205"},
        )
        with self.assertRaisesRegex(SystemExit, "governed expected key metadata"):
            module.verify_signed_attestation(
                receipt,
                {"root_digest": "remote-capture"},
                "remote identity receipt",
                "storage-205",
            )
        summary = module.verify_signed_attestation(
            receipt,
            {"root_digest": "remote-capture"},
            "remote identity receipt",
            "storage-205",
            expected_identity_metadata=expected_metadata,
        )
        self.assertEqual(summary["role"], "storage-205")
        self.assertEqual(summary["node_id"], "triad-testnet-storage")
        self.assertEqual(summary["peer_id"], "peer-storage-205")
        self.assertEqual(summary["key_path"], "/remote/validator/config/node-keypair.toml")
        self.assertEqual(summary["key_sha256"], "a" * 64)
        self.assertEqual(summary["key_size_bytes"], 321)
        self.assertEqual(summary["key_mode"], 0o600)
        self.assertEqual(summary["key_uid"], 1001)
        self.assertEqual(summary["key_gid"], 1001)

    def test_remote_identity_receipt_rejects_weak_metadata_contract(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_remote_identity_metadata_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        receipt = self.root / "weak-remote-identity-receipt.json"
        receipt.write_text(
            json.dumps(
                {
                    "schema_version": "oasis7.identity_receipt.v1",
                    "node_id": "triad-testnet-storage",
                    "peer_id": "peer-storage-205",
                    "key_path": "/remote/validator/config/node-keypair.toml",
                    "key_sha256": "b" * 64,
                    "key_size_bytes": 321,
                    "key_mode": 0o644,
                    "key_uid": 9999,
                    "key_gid": 9999,
                }
            ),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(SystemExit, "metadata"):
            module.verify_signed_attestation(
                receipt,
                {"root_digest": "remote-capture"},
                "remote identity receipt",
                "storage-205",
                expected_identity_metadata={
                    "key_mode": 0o600,
                    "key_uid": 1001,
                    "key_gid": 1001,
                },
            )

    def test_apply_requires_phase_callbacks_and_signed_identity_proof(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["phase"], "applied")
        self.assertIn("direct_quiescence_observation", receipt)
        self.assertIn("backup_receipt", receipt)
        self.assertIn("sequencer_rebuild_proof", receipt["host_receipt"])

    def test_host_receipt_rejects_trusted_but_unplanned_proof(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_host_binding_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        plan = json.loads(subprocess.run(self._base_args(), text=True, capture_output=True, check=True).stdout)
        plan["schema_version"] = "oasis7.validator_pair_rebuild_transaction.v1"
        plan["transaction_id"] = "pair-rebuild-test-binding"
        plan["phase"] = "staged"
        plan_path = self.out / "binding-transaction.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        adapter = self._write_host_adapter()
        receipt = module.run_host_adapter(adapter, plan_path, plan, "apply")
        alternate_proof = self.root / "alternate-host-proof.json"
        alternate_signature = self.root / "alternate-host-proof.sig"
        self._write_attestation(
            alternate_proof,
            alternate_signature,
            "oasis7.validator_pair_rebuild_proof.v1",
            "sequencer-204",
            "peer-unplanned",
            "unplanned-sequencer",
        )
        receipt["sequencer_rebuild_proof"]["path"] = str(alternate_proof)
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, plan, "apply")

    def _bound_phase_receipt(self, phase: str):
        spec = importlib.util.spec_from_file_location("pair_rebuild_host_binding_fixture", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        plan = json.loads(subprocess.run(self._base_args(), text=True, capture_output=True, check=True).stdout)
        plan["schema_version"] = "oasis7.validator_pair_rebuild_transaction.v1"
        plan["transaction_id"] = f"pair-rebuild-test-{phase}"
        plan["phase"] = "staged" if phase == "apply" else "prepared"
        plan_path = self.out / f"{phase}-transaction.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        receipt = module.run_host_adapter(self._write_host_adapter(), plan_path, plan, phase)
        if phase == "apply":
            empty_surface = self.root / "empty-post-delete-surface"
            empty_surface.mkdir(exist_ok=True)
            empty_inventory = module.inventory_tree(empty_surface)
            post_delete_absence = {
                "schema_version": "oasis7.validator_pair_rebuild_post_delete_absence.v1",
                "absent": True,
                "target_set": list(module.RESET_SURFACES),
                "target_set_sha256": module.reset_surface_digest(),
                "target_inventory": {
                    surface: empty_inventory
                    for surface in module.RESET_SURFACES
                },
            }
            for role in ("storage-205", "sequencer-204"):
                receipt["nodes"][role]["post_delete_absence"] = copy.deepcopy(post_delete_absence)
        return module, plan, receipt

    def _bound_host_receipt(self):
        return self._bound_phase_receipt("apply")

    def test_apply_requires_explicit_observer_mutation_false(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        receipt.pop("observer_mutation", None)
        with self.assertRaisesRegex(SystemExit, "observer"):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_requires_per_node_independent_observation(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role].pop("independently_observed", None)
        with self.assertRaisesRegex(SystemExit, r"independent|observ"):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_requires_per_node_service_state(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role].pop("service_state", None)
        with self.assertRaisesRegex(SystemExit, r"service.?state|service"):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_rejects_non_exact_per_node_role_and_root_bindings(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        cases = {
            "role substitution": ("role", "sequencer-204"),
            "root substitution": ("root", str(self.nodes["sequencer-204"])),
            "missing role": ("role", None),
            "missing root": ("root", None),
        }
        for label, (field, value) in cases.items():
            with self.subTest(binding=label):
                candidate = copy.deepcopy(receipt)
                node = candidate["nodes"]["storage-205"]
                if value is None:
                    node.pop(field, None)
                else:
                    node[field] = value
                with self.assertRaisesRegex(SystemExit, r"role|root|binding"):
                    module.validate_host_receipt(candidate, plan, "apply")

    def test_apply_rejects_missing_per_node_post_delete_absence_receipts(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role].pop("post_delete_absence", None)
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_apply_rejects_target_surface_mismatch_including_advertised_bridge_root(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        advertised_targets = [*CANONICAL_RESET_SURFACES, "data/bridge-root"]
        target_digest = hashlib.sha256(
            json.dumps(advertised_targets, ensure_ascii=True, separators=(",", ":")).encode()
        ).hexdigest()
        for role in ("storage-205", "sequencer-204"):
            receipt["nodes"][role]["post_delete_absence"] = {
                "absent": True,
                "target_set": advertised_targets,
                "target_set_sha256": target_digest,
            }
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, plan, "apply")

    def test_backup_rejects_each_incomplete_hashed_manifest_binding(self) -> None:
        module, plan, receipt = self._bound_phase_receipt("backup")
        required_fields = (
            "backup_root",
            "backup_manifest_sha256",
            "backup_inventory",
            "backup_capacity",
            "backup_non_seed",
        )
        for missing_field in required_fields:
            incomplete = copy.deepcopy(receipt)
            for role in ("storage-205", "sequencer-204"):
                node = incomplete["nodes"][role]
                node.update(
                    {
                        "backup_root": str(Path(plan["nodes"][role]["root"]) / "backups" / "pair-rebuild-test-backup"),
                        "backup_manifest_sha256": "a" * 64,
                        "backup_inventory": plan["capacity"][role]["inventory"],
                        "backup_capacity": plan["capacity"][role],
                        "backup_non_seed": {
                            "forensic_only": True,
                            "seed_eligible": False,
                            "restore_deleted_chain_state": False,
                        },
                    }
                )
                node.pop(missing_field)
            with self.subTest(missing_field=missing_field):
                with self.assertRaises(SystemExit):
                    module.validate_host_receipt(incomplete, plan, "backup")

    def test_host_receipt_rejects_incomplete_swapped_or_stale_evidence(self) -> None:
        module, plan, receipt = self._bound_host_receipt()
        omitted = copy.deepcopy(receipt)
        omitted["identity_receipts"] = omitted["identity_receipts"][:1]
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(omitted, plan, "apply")
        swapped = copy.deepcopy(receipt)
        swapped["identity_receipts"][0]["role"], swapped["identity_receipts"][1]["role"] = (
            swapped["identity_receipts"][1]["role"],
            swapped["identity_receipts"][0]["role"],
        )
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(swapped, plan, "apply")
        stale = copy.deepcopy(receipt)
        stale["captured_at"] = "2000-01-01T00:00:00Z"
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(stale, plan, "apply")
        stale_window = copy.deepcopy(receipt)
        plan["adapter_binding"]["phase_window_started_at"] = "2000-01-01T00:00:00Z"
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(stale_window, plan, "apply")
        substituted = copy.deepcopy(receipt)
        substituted["evidence_bindings"]["identity_receipts"][0]["sha256"] = "0" * 64
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(substituted, plan, "apply")

    def test_adapter_binding_separates_raw_and_verification_proof_files(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_binding_shape_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        identities = []
        for role in ("storage-205", "sequencer-204"):
            path = self.root / f"binding-{role}.json"
            path.write_text(role, encoding="utf-8")
            identities.append({
                "path": str(path),
                "sha256": sha256(path),
                "role": role,
                "node_id": f"node-{role}",
                "peer_id": f"peer-{role}",
                "key_path": f"/remote/{role}/config/node-keypair.toml",
                "key_sha256": "c" * 64,
                "key_size_bytes": 321,
                "key_mode": 0o600,
                "key_uid": 1001,
                "key_gid": 1001,
            })
        raw_proof = self.root / "binding-raw-proof.json"
        verification = self.root / "binding-verification.json"
        raw_proof.write_text("raw-proof", encoding="utf-8")
        verification.write_text("verification-receipt", encoding="utf-8")
        plan = {
            "plan_digest": "a" * 64,
            "transaction_id": "binding-shape",
            "proof": {
                "identity_receipts": identities,
                "sequencer_rebuild_proof_path": str(raw_proof),
                "sequencer_rebuild_proof_verification_path": str(verification),
                "sequencer_rebuild_proof": {
                    "path": str(verification),
                    "sha256": sha256(verification),
                    "proof_sha256": sha256(raw_proof),
                    "role": "sequencer-204",
                    "node_id": "node-sequencer-204",
                    "peer_id": "peer-sequencer-204",
                },
            },
        }
        evidence = module._expected_adapter_evidence(plan)
        self.assertEqual(evidence["identity_receipts"][0]["key_mode"], 0o600)
        self.assertEqual(evidence["identity_receipts"][0]["key_uid"], 1001)
        self.assertEqual(evidence["identity_receipts"][0]["key_gid"], 1001)
        self.assertEqual(evidence["sequencer_rebuild_proof"]["path"], str(raw_proof.resolve()))
        self.assertEqual(evidence["sequencer_rebuild_proof"]["sha256"], sha256(raw_proof))
        self.assertEqual(evidence["sequencer_rebuild_proof"]["verification_path"], str(verification.resolve()))
        self.assertEqual(evidence["sequencer_rebuild_proof"]["verification_sha256"], sha256(verification))

    def test_plan_rejects_missing_signature_before_any_node_access(self) -> None:
        self._write_provenance(signature=False)
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("signature", result.stderr.lower())

    def test_provenance_rejects_tampered_signed_body(self) -> None:
        receipt = json.loads(self.provenance.read_text(encoding="utf-8"))
        receipt["package"]["run_id"] = "tampered"
        body = {key: value for key, value in receipt.items() if key != "binding_digest"}
        receipt["binding_digest"] = hashlib.sha256(
            json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        self.provenance.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        result = subprocess.run(
            [
                sys.executable,
                str(PROVENANCE),
                "validate",
                "--provenance",
                str(self.provenance),
                "--package-dir",
                str(self.package),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("detached signature verification failed", result.stderr.lower())

    def test_staged_receipt_falls_back_to_copied_detached_files(self) -> None:
        staged = self.root / "staged-evidence"
        staged.mkdir()
        staged_receipt = staged / self.provenance.name
        staged_receipt.write_bytes(self.provenance.read_bytes())
        (staged / self.signature.name).write_bytes(self.signature.read_bytes())
        (staged / self.public_key.name).write_bytes(self.public_key.read_bytes())
        original_signature = self.signature.read_bytes()
        original_public_key = self.public_key.read_bytes()
        self.signature.unlink()
        self.public_key.unlink()
        result = subprocess.run(
            [
                sys.executable,
                str(PROVENANCE),
                "validate",
                "--provenance",
                str(staged_receipt),
                "--package-dir",
                str(self.package),
            ],
            text=True,
            capture_output=True,
        )
        self.signature.write_bytes(original_signature)
        self.public_key.write_bytes(original_public_key)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_apply_writes_full_manifests_and_preserves_order(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["phase"], "applied")
        self.assertEqual(receipt["mutation_order"], ["storage-205", "sequencer-204"])
        self.assertTrue(receipt["nodes"]["storage-205"]["backup"]["verified"])
        self.assertTrue(receipt["nodes"]["sequencer-204"]["backup"]["verified"])
        self.assertTrue((self.nodes["storage-205"] / "backups").exists())
        self.assertTrue((self.nodes["sequencer-204"] / "backups").exists())
        self.assertTrue(receipt["host_receipt"]["nodes"]["sequencer-204"]["healthz_ok"])
        self.assertTrue(receipt["capacity_apply"]["storage-205"]["verified"])
        for role in ("storage-205", "sequencer-204"):
            self.assertEqual(
                receipt["capacity_apply"][role]["inventory"],
                receipt["capacity"][role]["inventory"],
            )
        self.assertEqual(
            set(receipt["staged"]["storage-205"]["governed_inventory"]),
            {"manifest", "genesis", "registry", "bootstrap", "world"},
        )
        self.assertEqual(receipt["host_receipt"]["plan_digest"], receipt["plan_digest"])
        self.assertEqual(receipt["host_receipt"]["evidence_bindings"], receipt["adapter_binding"]["evidence_bindings"])
        self.assertTrue(receipt["host_receipt"]["captured_at"])

    def test_apply_transaction_only_invocation_recovers_direct_observation_routing(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        result = subprocess.run(
            [
                str(self.executor_launcher),
                "apply",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(self._write_host_adapter()),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["phase"], "applied")

    def test_apply_requires_host_gate_receipt(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(EXECUTOR), "apply", "--transaction", str(plan_path)],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("host adapter", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_apply_rechecks_capacity_after_plan(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        transaction = json.loads(plan.stdout)
        transaction["capacity"]["storage-205"]["required_bytes"] = 10**30
        transaction["plan_digest"] = hashlib.sha256(
            json.dumps({key: value for key, value in transaction.items() if key != "plan_digest"}, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        plan_path = self.out / "transaction.json"
        plan_path.write_text(json.dumps(transaction), encoding="utf-8")
        result = subprocess.run(
            self._apply_args(plan_path, self._write_host_adapter()),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("apply-time capacity", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_apply_rejects_inventory_drift_after_plan(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        (self.nodes["storage-205"] / "data" / "unexpected-entry").write_text("drift\n", encoding="utf-8")
        result = subprocess.run(
            self._apply_args(plan_path, self._write_host_adapter()),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("inventory changed", result.stderr.lower())
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())

    def test_rollback_restores_the_stopped_snapshot(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        applied = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
            check=True,
        )
        applied_receipt = json.loads(applied.stdout)
        transaction_id = applied_receipt["transaction_id"]
        self.assertNotEqual(sha256(self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime"), sha256(self.nodes["storage-205"] / "backups" / transaction_id / "snapshot" / "current" / "bin" / "oasis7_chain_runtime"))
        result = subprocess.run(
            [str(self.executor_launcher), "rollback", "--transaction", str(plan_path), "--host-adapter", str(adapter)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["phase"], "rolled_back")
        self.assertEqual(sha256(self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime"), sha256(self.nodes["storage-205"] / "backups" / transaction_id / "snapshot" / "current" / "bin" / "oasis7_chain_runtime"))

    def test_transaction_journal_fsyncs_file_and_parent_directory(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        journal = self.root / "journal" / "transaction.json"
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            module.write_json(journal, {"phase": "prepared"})
        finally:
            module.os.fsync = original_fsync
        self.assertEqual(json.loads(journal.read_text(encoding="utf-8"))["phase"], "prepared")
        self.assertGreaterEqual(len(fsync_calls), 2)

    def test_backup_manifest_fsyncs_file_and_parent_directory(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_backup_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        node_root = self.root / "durable-node"
        node_root.mkdir()
        (node_root / "payload").write_text("before\\n", encoding="utf-8")
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            backup = module.snapshot_node({"role": "storage-205", "root": str(node_root)}, "durable-tx")
        finally:
            module.os.fsync = original_fsync
        self.assertTrue(Path(backup["manifest"]).is_file())
        self.assertGreaterEqual(len(fsync_calls), 2)

    def test_copy_entry_fsyncs_nested_files_and_directories(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_tree_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        source = self.root / "tree-source"
        (source / "nested").mkdir(parents=True)
        (source / "nested" / "payload").write_bytes(b"durable-payload\n")
        destination = self.root / "tree-destination"
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            module.copy_entry(source, destination)
        finally:
            module.os.fsync = original_fsync
        self.assertEqual((destination / "nested" / "payload").read_bytes(), b"durable-payload\n")
        self.assertGreaterEqual(len(fsync_calls), 4)

    def test_restore_node_fsyncs_restored_files_and_directories(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_restore_durable_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        node_root = self.root / "restore-node"
        (node_root / "nested").mkdir(parents=True)
        (node_root / "nested" / "payload").write_bytes(b"before\n")
        backup = module.snapshot_node({"role": "storage-205", "root": str(node_root)}, "restore-durable-tx")
        (node_root / "nested" / "payload").write_bytes(b"changed\n")
        fsync_calls: list[int] = []
        original_fsync = module.os.fsync
        module.os.fsync = lambda descriptor: fsync_calls.append(descriptor)
        try:
            module.restore_node(backup, "restore-durable-tx")
        finally:
            module.os.fsync = original_fsync
        self.assertEqual((node_root / "nested" / "payload").read_bytes(), b"before\n")
        self.assertGreaterEqual(len(fsync_calls), 4)

    def test_credential_fd_is_consumed_once_and_reused_from_memory(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_credential_cache_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        secret_path = self.root / "credential"
        secret_path.write_bytes(b"fd-secret-for-reobserve\n")
        credential_fd = os.open(secret_path, os.O_RDONLY)
        args = module.argparse.Namespace(credential_env=None, credential_fd=credential_fd)
        try:
            first_env, first_use = module._direct_ssh_environment(args, {})
            try:
                second_env, second_use = module._direct_ssh_environment(args, {})
            except SystemExit as error:
                self.fail(f"credential fd was read again instead of reusing the in-memory secret: {error}")
        finally:
            os.close(credential_fd)
        self.assertTrue(first_use and second_use)
        self.assertEqual(first_env["SSHPASS"], "fd-secret-for-reobserve")
        self.assertEqual(second_env["SSHPASS"], first_env["SSHPASS"])

    def test_human_direct_ssh_requires_authorized_live_action_and_actor_association(self) -> None:
        fixture = self._write_live_github_fixture()

        def unauthorize(response: dict[str, Any]) -> None:
            response["author_association"] = "CONTRIBUTOR"
            body = json.loads(response["body"])
            body["authorized"] = False
            response["body"] = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":"))

        response = self._rewrite_github_response(fixture, unauthorize)
        self._rewrite_request(
            fixture,
            lambda request: request["github_live"].update(
                {"body_sha256": hashlib.sha256(response["body"].encode()).hexdigest()}
            ),
        )
        result = self._run_live_human_direct_ssh(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertRegex(result.stderr, r"(?i)(authorized|association|actor|permission|authority)")

    def test_rollback_receipt_requires_stopped_state_after_callback(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_rollback_gate_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module._validate_adapter_binding = lambda receipt, plan, phase: None
        module.validate_repository_executable_identity = lambda value, label: value
        receipt = {
            "schema_version": "oasis7.validator_pair_rebuild_host_receipt.v2",
            "phase": "rollback",
            "transaction_id": "rollback-gate-tx",
            "mutation_order": ["storage-205", "sequencer-204"],
            "startup_order": ["sequencer-204", "storage-205"],
            "nodes": {
                role: {
                    "rollback_verified": True,
                    "active": True,
                    "running": True,
                    "service_state": "running",
                    "independently_observed": True,
                }
                for role in ("storage-205", "sequencer-204")
            },
            "repository_executable": {},
            "observer_mutation": False,
            "captured_at": "2026-08-29T00:00:00Z",
        }
        with self.assertRaises(SystemExit):
            module.validate_host_receipt(receipt, {"transaction_id": "rollback-gate-tx", "nodes": {}}, "rollback")

    def test_resume_is_idempotent_for_a_terminal_transaction(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_resume_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        transaction = {
            "schema_version": module.SCHEMA,
            "phase": "rolled_back",
            "transaction_id": "resume-terminal-tx",
            "rollback": {"strategy": "same-filesystem-full-snapshot", "status": "verified"},
        }
        transaction["canonical_digest"] = module.canonical_digest(transaction)
        path = self.root / "resume-terminal.json"
        path.write_text(json.dumps(transaction), encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(EXECUTOR), "resume", "--transaction", str(path)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["phase"], "rolled_back")

    def test_resume_adopts_complete_orphan_backups_after_transaction_promotion(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_resume_adopt_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        transaction_id = "resume-adopt-tx"
        nodes = {}
        for role in ("storage-205", "sequencer-204"):
            root = self.nodes[role]
            (root / "data" / "payload").write_bytes(role.encode())
            module.snapshot_node({"role": role, "root": str(root)}, transaction_id)
            nodes[role] = {"root": str(root)}
        transaction = {
            "schema_version": module.SCHEMA,
            "phase": "prepared",
            "transaction_id": transaction_id,
            "nodes": nodes,
        }
        transaction["canonical_digest"] = module.canonical_digest(transaction)
        path = self.root / "resume-adopt.json"
        path.write_text(json.dumps(transaction), encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(EXECUTOR), "resume", "--transaction", str(path)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        recovered = json.loads(result.stdout)
        self.assertEqual(recovered["recovery"]["status"], "adopted")
        self.assertEqual(set(recovered["backup"]), {"storage-205", "sequencer-204"})
        self.assertEqual(recovered["phase"], "backed_up")

    def test_resume_rejects_adapterless_rollback_required_without_rewriting_transaction(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_resume_rollback_required_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        transaction_id = "resume-rollback-required-tx"
        nodes = {}
        backup_roots = {}
        for role in ("storage-205", "sequencer-204"):
            root = self.nodes[role]
            (root / "data" / "payload").write_bytes(role.encode())
            backup = module.snapshot_node({"role": role, "root": str(root)}, transaction_id)
            nodes[role] = {"root": str(root)}
            backup_roots[role] = Path(backup["backup_root"])
        transaction = {
            "schema_version": module.SCHEMA,
            "phase": "rollback_required",
            "transaction_id": transaction_id,
            "nodes": nodes,
        }
        transaction["canonical_digest"] = module.canonical_digest(transaction)
        path = self.root / "resume-rollback-required.json"
        path.write_text(json.dumps(transaction), encoding="utf-8")
        before = path.read_bytes()
        result = subprocess.run(
            [sys.executable, str(EXECUTOR), "resume", "--transaction", str(path)],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rollback_required", result.stderr)
        self.assertEqual(path.read_bytes(), before)
        for backup_root in backup_roots.values():
            self.assertTrue(backup_root.is_dir())

    def test_resume_rollback_required_with_adapter_reauthorizes_and_restores(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "resume-rollback-required-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "resume-rollback-required-phases.log"
        adapter = self._write_failure_host_adapter(phases)
        original_runtime = (self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime").read_bytes()
        applied = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertEqual(applied.returncode, 0, applied.stderr)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight", "backup", "apply"])

        spec = importlib.util.spec_from_file_location("pair_rebuild_resume_governed_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        transaction["phase"] = "rollback_required"
        transaction["canonical_digest"] = module.canonical_digest(transaction)
        plan_path.write_text(json.dumps(transaction), encoding="utf-8")
        phases.write_text("", encoding="utf-8")
        resumed = subprocess.run(
            [
                str(self.executor_launcher),
                "resume",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=self._human_direct_ssh_env(self.human_direct_fixture),
        )
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        recovered = json.loads(resumed.stdout)
        self.assertEqual(recovered["phase"], "rolled_back")
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["rollback"])
        self.assertTrue(recovered["rollback_direct_quiescence_observation"]["reauthorized"])
        self.assertTrue(recovered["rollback_post_callback_direct_quiescence_observation"]["reauthorized"])
        self.assertEqual(
            (self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime").read_bytes(),
            original_runtime,
        )

    def test_resume_cleans_only_transaction_bound_partial_orphan_backup(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_resume_cleanup_test_executor", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        transaction_id = "resume-cleanup-tx"
        nodes = {}
        for role in ("storage-205", "sequencer-204"):
            root = self.nodes[role]
            partial = root / "backups" / transaction_id / "snapshot"
            partial.mkdir(parents=True)
            (partial / "partial").write_bytes(b"interrupted-copy")
            nodes[role] = {"root": str(root)}
        transaction = {
            "schema_version": module.SCHEMA,
            "phase": "prepared",
            "transaction_id": transaction_id,
            "nodes": nodes,
        }
        transaction["canonical_digest"] = module.canonical_digest(transaction)
        path = self.root / "resume-cleanup.json"
        path.write_text(json.dumps(transaction), encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(EXECUTOR), "resume", "--transaction", str(path)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        recovered = json.loads(result.stdout)
        self.assertEqual(recovered["recovery"]["status"], "cleaned_partial")
        for role in nodes:
            self.assertFalse((Path(nodes[role]["root"]) / "backups" / transaction_id).exists())

    def test_sigkill_during_host_callback_fails_closed_without_replay(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "sigkill-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "sigkill-phases.log"
        adapter = self._write_failure_host_adapter(phases)
        source = adapter.read_text(encoding="utf-8")
        source = source.replace(
            "import sys\n",
            "import sys\nimport os\nimport signal\n",
        ).replace(
            "phase = sys.argv[sys.argv.index('--phase') + 1]\n",
            "phase = sys.argv[sys.argv.index('--phase') + 1]\n"
            "if phase == 'backup':\n"
            "    os.kill(os.getppid(), signal.SIGKILL)\n",
        )
        adapter.write_text(source, encoding="utf-8")
        crashed = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(crashed.returncode, 0)
        interrupted = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(interrupted["phase"], "backup_in_progress")
        self.assertEqual(interrupted.get("adapter_callback", {}).get("status"), "in_flight")
        before_resume = plan_path.read_bytes()
        resumed = subprocess.run(
            [
                str(self.executor_launcher),
                "resume",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertNotEqual(resumed.returncode, 0)
        self.assertRegex(resumed.stderr, r"(?i)(in.?flight|ambiguous|reconcil)")
        self.assertEqual(plan_path.read_bytes(), before_resume)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight", "backup"])

    def test_successful_callback_completion_is_adopted_after_sigkill_without_replay(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "callback-completion-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "callback-completion-phases.log"
        adapter = self._write_failure_host_adapter(phases)
        crash_launcher = self.root / "crash-after-callback-launcher.py"
        crash_launcher.write_text(
            f"""#!{sys.executable}
import importlib.util
import os
import signal
import sys
from pathlib import Path

executor = Path({str(EXECUTOR)!r})
spec = importlib.util.spec_from_file_location("pair_rebuild_crash_after_callback", executor)
if spec is None or spec.loader is None:
    raise SystemExit("unable to load validator-pair executor")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.EXECUTOR_KNOWN_HOSTS_PATH = Path(os.environ["OASIS7_TEST_EXECUTOR_KNOWN_HOSTS_PATH"])
original_run_host_adapter = module.run_host_adapter

def crash_after_apply_callback(adapter, transaction_path, plan, phase):
    receipt = original_run_host_adapter(adapter, transaction_path, plan, phase)
    if phase == "apply":
        os.kill(os.getpid(), signal.SIGKILL)
    return receipt

module.run_host_adapter = crash_after_apply_callback
sys.argv = [str(executor), *sys.argv[1:]]
raise SystemExit(module.main())
""",
            encoding="utf-8",
        )
        crash_launcher.chmod(0o755)
        crashed = subprocess.run(
            [
                str(crash_launcher),
                "apply",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertNotEqual(crashed.returncode, 0)
        interrupted = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(interrupted["phase"], "staged")
        self.assertNotIn("host_receipt", interrupted)
        self.assertEqual(interrupted.get("adapter_callback", {}).get("status"), "completed")
        self.assertEqual(interrupted.get("adapter_callback", {}).get("phase"), "apply")
        before_resume_phases = phases.read_text(encoding="utf-8").splitlines()
        before_resume_github_calls = len(
            Path(self.human_direct_fixture["github_calls"]).read_text(encoding="utf-8").splitlines()
        )
        resumed = subprocess.run(
            [
                str(self.executor_launcher),
                "resume",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        recovered = json.loads(resumed.stdout)
        self.assertEqual(recovered["phase"], "applied")
        self.assertIn("host_receipt", recovered)
        self.assertNotIn("adapter_callback", recovered)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), before_resume_phases)
        after_resume_github_calls = len(
            Path(self.human_direct_fixture["github_calls"]).read_text(encoding="utf-8").splitlines()
        )
        self.assertGreater(after_resume_github_calls, before_resume_github_calls)

    def test_resume_accepts_fresh_replacement_authority_for_existing_transaction(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "replacement-authority-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "replacement-authority-phases.log"
        adapter = self._write_failure_host_adapter(phases)
        crash_launcher = self.root / "replacement-authority-crash-launcher.py"
        crash_launcher.write_text(
            f"""#!{sys.executable}
import importlib.util
import os
import signal
import sys
from pathlib import Path

executor = Path({str(EXECUTOR)!r})
spec = importlib.util.spec_from_file_location("pair_rebuild_replacement_authority", executor)
if spec is None or spec.loader is None:
    raise SystemExit("unable to load validator-pair executor")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.EXECUTOR_KNOWN_HOSTS_PATH = Path(os.environ["OASIS7_TEST_EXECUTOR_KNOWN_HOSTS_PATH"])
original_run_host_adapter = module.run_host_adapter

def crash_after_apply_callback(adapter, transaction_path, plan, phase):
    receipt = original_run_host_adapter(adapter, transaction_path, plan, phase)
    if phase == "apply":
        os.kill(os.getpid(), signal.SIGKILL)
    return receipt

module.run_host_adapter = crash_after_apply_callback
sys.argv = [str(executor), *sys.argv[1:]]
raise SystemExit(module.main())
""",
            encoding="utf-8",
        )
        crash_launcher.chmod(0o755)
        crashed = subprocess.run(
            [
                str(crash_launcher),
                "apply",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=self._human_direct_ssh_env(self.human_direct_fixture),
        )
        self.assertNotEqual(crashed.returncode, 0)
        interrupted = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(interrupted["phase"], "staged")
        self.assertEqual(interrupted.get("adapter_callback", {}).get("status"), "completed")
        self.assertEqual(interrupted.get("adapter_callback", {}).get("phase"), "apply")

        old_request = json.loads(Path(self.human_direct_fixture["request"]).read_text(encoding="utf-8"))
        replacement = self._write_live_github_fixture(
            transaction_id=interrupted["transaction_id"],
            impact_path=self.impact,
        )
        replacement_nonce = "replacement-direct-ssh-nonce-3492"
        replacement_issued = (dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=1)).replace(microsecond=0)
        replacement_issued_text = replacement_issued.isoformat().replace("+00:00", "Z")
        replacement_expires_text = (replacement_issued + dt.timedelta(minutes=15)).isoformat().replace("+00:00", "Z")

        def rewrite_replacement_response(response: dict[str, Any]) -> None:
            body = json.loads(response["body"])
            body.update(
                {
                    "nonce": replacement_nonce,
                    "issued_at": replacement_issued_text,
                    "expires_at": replacement_expires_text,
                }
            )
            response["body"] = json.dumps(body, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
            response["created_at"] = replacement_issued_text
            response["updated_at"] = replacement_issued_text

        replacement_response = self._rewrite_github_response(replacement, rewrite_replacement_response)
        replacement_body = json.loads(replacement_response["body"])

        def rewrite_replacement_request(request: dict[str, Any]) -> None:
            request.update(
                {
                    "nonce": replacement_nonce,
                    "issued_at": replacement_issued_text,
                    "expires_at": replacement_expires_text,
                }
            )
            request["github_live"].update(
                {
                    "nonce": replacement_nonce,
                    "issued_at": replacement_issued_text,
                    "expires_at": replacement_expires_text,
                    "body_sha256": hashlib.sha256(replacement_response["body"].encode()).hexdigest(),
                    "transaction_id": replacement_body["transaction_id"],
                }
            )

        replacement_request = self._rewrite_request(replacement, rewrite_replacement_request)
        for field in ("nonce", "issued_at", "expires_at", "request_digest", "quiescence_id"):
            self.assertNotEqual(replacement_request[field], old_request[field], field)
        self.assertEqual(replacement_request["task_uid"], old_request["task_uid"])
        self.assertEqual(replacement_request["transaction_id"], interrupted["transaction_id"])
        self.assertEqual(replacement_request["impact_record_sha256"], old_request["impact_record_sha256"])

        resumed = subprocess.run(
            [
                str(self.executor_launcher),
                "resume",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(replacement["request"]),
                "--known-hosts",
                str(replacement["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=self._live_github_env(replacement),
        )
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        recovered = json.loads(resumed.stdout)
        self.assertEqual(recovered["phase"], "applied")
        self.assertIn("host_receipt", recovered)
        self.assertNotIn("adapter_callback", recovered)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight", "backup", "apply"])
        self.assertTrue(Path(replacement["github_calls"]).read_text(encoding="utf-8").splitlines())

    def test_resume_fails_closed_on_durable_failed_callback_without_replay(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "failed-callback-resume-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "failed-callback-resume-phases.log"
        adapter = self._write_failure_host_adapter(phases, "preflight")
        failed = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertNotEqual(failed.returncode, 0)
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(transaction["phase"], "preflight_failed")
        self.assertEqual(transaction.get("adapter_callback", {}).get("status"), "failed")
        before_resume = plan_path.read_bytes()
        resumed = subprocess.run(
            [
                str(self.executor_launcher),
                "resume",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertNotEqual(resumed.returncode, 0)
        self.assertRegex(resumed.stderr, r"(?i)(failed|non.?retry|reconcil)")
        self.assertEqual(plan_path.read_bytes(), before_resume)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight"])

    def test_rollback_fails_closed_on_durable_failed_callback_without_replay(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "failed-callback-rollback-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        applied = subprocess.run(
            self._apply_args(plan_path, self._write_host_adapter()),
            text=True,
            capture_output=True,
            check=True,
            env=os.environ.copy(),
        )
        self.assertEqual(applied.returncode, 0)
        phases = self.root / "failed-callback-rollback-phases.log"
        adapter = self._write_failure_host_adapter(phases, "rollback")
        rollback_args = [
            str(self.executor_launcher),
            "rollback",
            "--transaction",
            str(plan_path),
            "--host-adapter",
            str(adapter),
            "--request",
            str(self.human_direct_fixture["request"]),
            "--known-hosts",
            str(self.human_direct_fixture["known_hosts"]),
        ]
        failed = subprocess.run(rollback_args, text=True, capture_output=True, env=os.environ.copy())
        self.assertNotEqual(failed.returncode, 0)
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(transaction["phase"], "rollback_failed")
        self.assertEqual(transaction.get("adapter_callback", {}).get("status"), "failed")
        before_retry = plan_path.read_bytes()
        retry = subprocess.run(rollback_args, text=True, capture_output=True, env=os.environ.copy())
        self.assertNotEqual(retry.returncode, 0)
        self.assertRegex(retry.stderr, r"(?i)(failed|non.?retry|reconcil)")
        self.assertEqual(plan_path.read_bytes(), before_retry)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["rollback"])

    def test_resume_rejects_disappeared_persisted_backups_without_rewrite(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "disappeared-backup-transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        applied = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
            check=True,
        )
        self.assertEqual(applied.returncode, 0)
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        backup_roots = [
            Path(transaction["backup"][role]["backup_root"])
            for role in ("storage-205", "sequencer-204")
        ]
        transaction["phase"] = "staged"
        transaction.pop("host_receipt", None)
        transaction["canonical_digest"] = canonical_digest(transaction)
        plan_path.write_text(json.dumps(transaction, sort_keys=True), encoding="utf-8")
        for backup_root in backup_roots:
            shutil.rmtree(backup_root)
        before_resume = plan_path.read_bytes()
        resumed = subprocess.run(
            [
                str(self.executor_launcher),
                "resume",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
                "--request",
                str(self.human_direct_fixture["request"]),
                "--known-hosts",
                str(self.human_direct_fixture["known_hosts"]),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertNotEqual(resumed.returncode, 0)
        self.assertRegex(resumed.stderr, r"(?i)(backup|snapshot|missing|incomplete)")
        self.assertEqual(plan_path.read_bytes(), before_resume)

    def test_apply_failure_reauthorizes_direct_ssh_before_automatic_rollback(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "automatic-rollback-direct-phases.log"
        adapter = self._write_failure_host_adapter(phases, "apply")
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight", "backup", "apply", "rollback"])
        github_calls = Path(self.human_direct_fixture["github_calls"]).read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(github_calls), 5)

    def test_manual_rollback_reauthorizes_direct_ssh_before_restore(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "manual-rollback-direct-phases.log"
        adapter = self._write_failure_host_adapter(phases)
        applied = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
            check=True,
        )
        self.assertEqual(applied.returncode, 0)
        phases.write_text("", encoding="utf-8")
        before_github_calls = len(Path(self.human_direct_fixture["github_calls"]).read_text(encoding="utf-8").splitlines())
        result = subprocess.run(
            [
                str(self.executor_launcher),
                "rollback",
                "--transaction",
                str(plan_path),
                "--host-adapter",
                str(adapter),
            ],
            text=True,
            capture_output=True,
            env=os.environ.copy(),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["rollback"])
        after_github_calls = len(Path(self.human_direct_fixture["github_calls"]).read_text(encoding="utf-8").splitlines())
        self.assertEqual(after_github_calls, before_github_calls + 2)

    def test_apply_requires_explicit_preflight_receipt_for_both_validators(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "preflight-phases.log"
        adapter = self._write_failure_host_adapter(phases, "preflight")
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight"])
        self.assertFalse((self.nodes["storage-205"] / "backups").exists())
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(transaction["phase"], "preflight_failed")


    def test_apply_failure_records_direct_reauthorization_before_automatic_rollback(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "automatic-rollback-phases.log"
        adapter = self._write_failure_host_adapter(phases, "apply")
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight", "backup", "apply", "rollback"])
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertTrue(transaction["rollback_direct_quiescence_observation"]["reauthorized"])
        self.assertEqual(transaction["rollback"]["status"], "verified")

    def test_apply_failure_does_not_restore_when_automatic_rollback_callback_fails(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        phases = self.root / "automatic-rollback-callback-failure-phases.log"
        adapter = self._write_failure_host_adapter(phases, "apply", "rollback")
        result = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(phases.read_text(encoding="utf-8").splitlines(), ["preflight", "backup", "apply", "rollback"])
        transaction = json.loads(plan_path.read_text(encoding="utf-8"))
        self.assertEqual(transaction["phase"], "rollback_failed")
        self.assertEqual(transaction["rollback"]["status"], "failed")
        current_runtime = self.nodes["storage-205"] / "current" / "bin" / "oasis7_chain_runtime"
        backup_runtime = Path(transaction["backup"]["storage-205"]["snapshot"]) / "current" / "bin" / "oasis7_chain_runtime"
        self.assertNotEqual(sha256(current_runtime), sha256(backup_runtime))

    def test_rollback_rejects_tampered_backup_manifest(self) -> None:
        plan = subprocess.run(self._base_args(), text=True, capture_output=True, check=True)
        plan_path = self.out / "transaction.json"
        plan_path.write_text(plan.stdout, encoding="utf-8")
        adapter = self._write_host_adapter()
        applied = subprocess.run(
            self._apply_args(plan_path, adapter),
            text=True,
            capture_output=True,
            check=True,
        )
        transaction = json.loads(applied.stdout)
        manifest = Path(transaction["backup"]["storage-205"]["manifest"])
        manifest.write_text(manifest.read_text(encoding="utf-8") + "\n", encoding="utf-8")
        result = subprocess.run(
            [str(self.executor_launcher), "rollback", "--transaction", str(plan_path), "--host-adapter", str(adapter)],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("manifest", result.stderr.lower())

    def test_plan_rejects_insufficient_capacity(self) -> None:
        self.capacity.write_text(
            json.dumps(
                {
                    role: {"free_bytes": 1, "free_inodes": 100_000, "same_filesystem": True}
                    for role in self.nodes
                }
            ),
            encoding="utf-8",
        )
        result = subprocess.run(self._base_args(), text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("capacity", result.stderr.lower())

    def test_provenance_create_and_validate_round_trip(self) -> None:
        generated = self.root / "generated-provenance.json"
        create = subprocess.run(
            [
                sys.executable,
                str(PROVENANCE),
                "create",
                "--package-dir",
                str(self.package),
                "--manifest",
                str(self.governed / "manifest.json"),
                "--genesis",
                str(self.governed / "genesis.json"),
                "--registry",
                str(self.governed / "registry.json"),
                "--bootstrap",
                str(self.governed / "bootstrap.txt"),
                "--world",
                str(self.governed / "world.json"),
                "--network-id",
                "oasis7-public-testnet-governed-20260606",
                "--chain-id",
                "oasis7-public-testnet-governed-20260606",
                "--output",
                str(generated),
                "--signer-id",
                "testnet-package-attestor",
                "--signature-ref",
                str(self.signature),
                "--public-key-ref",
                str(self.public_key),
            ],
            text=True,
            capture_output=True,
        )
        self.assertEqual(create.returncode, 0, create.stderr)
        generated_payload = json.loads(generated.read_text(encoding="utf-8"))
        generated_payload["signature"]["status"] = "verified"
        body = {key: value for key, value in generated_payload.items() if key != "binding_digest"}
        generated_payload["binding_digest"] = hashlib.sha256(
            json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
        ).hexdigest()
        generated.write_text(json.dumps(generated_payload, indent=2) + "\n", encoding="utf-8")
        payload_path = self.root / "generated-provenance-signing-payload.bin"
        payload_path.write_bytes(json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode())
        subprocess.run(
            [
                "openssl",
                "dgst",
                "-sha256",
                "-sign",
                str(self.signing_key),
                "-out",
                str(self.signature),
                str(payload_path),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        validate = subprocess.run(
            [sys.executable, str(PROVENANCE), "validate", "--provenance", str(generated), "--package-dir", str(self.package)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(validate.returncode, 0, validate.stderr)
        self.assertEqual(json.loads(validate.stdout)["signature"]["signer_id"], "testnet-package-attestor")

    def test_provenance_rejects_nested_directory_symlink_before_file_filtering(self) -> None:
        world = self.root / "world-tree"
        world.mkdir()
        (world / "state.json").write_text('{"height":1}\n', encoding="utf-8")
        target = self.root / "outside-world"
        target.mkdir()
        (target / "hidden.json").write_text('{"hidden":true}\n', encoding="utf-8")
        (world / "nested-link").symlink_to(target, target_is_directory=True)
        generated = self.root / "symlink-provenance.json"
        create = subprocess.run(
            [
                sys.executable,
                str(PROVENANCE),
                "create",
                "--package-dir",
                str(self.package),
                "--manifest",
                str(self.governed / "manifest.json"),
                "--genesis",
                str(self.governed / "genesis.json"),
                "--registry",
                str(self.governed / "registry.json"),
                "--bootstrap",
                str(self.governed / "bootstrap.txt"),
                "--world",
                str(world),
                "--network-id",
                "oasis7-public-testnet-governed-20260606",
                "--chain-id",
                "oasis7-public-testnet-governed-20260606",
                "--output",
                str(generated),
                "--signer-id",
                "testnet-package-attestor",
                "--signature-ref",
                str(self.signature),
                "--public-key-ref",
                str(self.public_key),
            ],
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(create.returncode, 0)
        self.assertIn("symlink", create.stderr.lower())

    def test_release_bundle_validates_pair_provenance_binding(self) -> None:
        bundle = self.root / "release-bundle.json"
        create = subprocess.run(
            [
                "bash",
                str(RELEASE_BUNDLE),
                "create",
                "--bundle",
                str(bundle),
                "--candidate-id",
                "pair-rebuild-fixture",
                "--runtime-build-ref",
                str(self.package / "oasis7_chain_runtime"),
                "--world-snapshot-ref",
                str(self.governed / "world.json"),
                "--governance-manifest-ref",
                str(self.governed / "manifest.json"),
                "--validator-pair-provenance-ref",
                str(self.provenance),
                "--allow-dirty-worktree",
            ],
            text=True,
            capture_output=True,
        )
        self.assertEqual(create.returncode, 0, create.stderr)
        validate = subprocess.run(
            ["bash", str(RELEASE_BUNDLE), "validate", "--bundle", str(bundle)],
            text=True,
            capture_output=True,
        )
        self.assertEqual(validate.returncode, 0, validate.stderr)

    def test_runtime_verification_receipt_binds_peer_and_exact_raw_proof(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_runtime_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        public_key_hex = "9ea2a4ca4893c1d2fed7482510333464c28a0ddb9db497a1238edb71d4ba286b"
        signature_hex = "22258b4742876b6a997c410b268547b8a40280acbc5ef2ad5a21a4ef034bc463fb728d31ab1ca549e9441c50cf4be587413ece9f9c68067770ce97712096b704"
        claims = {
            "schema_version": "oasis7.rebuild_status.v1",
            "observed_at_unix_ms": 1,
            "node_id": "sequencer-node",
            "world_id": "",
            "ok": True,
            "liveness": {"running": True, "last_error": None},
            "readiness": {"status": "ready", "failed_gates": []},
            "heights": {"committed_height": 1, "network_committed_height": 1, "last_execution_height": 1},
            "network_head": {
                "source": "self",
                "decision": "ready",
                "height": 1,
                "block_hash": "block",
                "execution_block_hash": "execution-block",
                "execution_state_root": "state-root",
                "observed_peer_count": 0,
                "fresh_peer_count": 0,
            },
            "checkpoint": None,
            "local_peer_id": "peer-sequencer",
            "connected_peers": [],
            "connected_peer_count": 0,
        }
        claims_bytes = json.dumps(claims, ensure_ascii=True, separators=(",", ":")).encode()
        self.assertEqual(hashlib.sha256(claims_bytes).hexdigest(), "5acd019055674e080903296b8af0583fec0c86d0243809af187bd2aa11155c76")
        raw_proof = self.root / "raw-rebuild-proof.json"
        raw_value = {
            "schema_version": "oasis7.rebuild_status.v1",
            "observed_at_unix_ms": 1,
            "ok": True,
            "liveness": {"running": True, "last_error": None},
            "readiness": {"status": "ready", "failed_gates": []},
            "heights": {"committed_height": 1, "network_committed_height": 1, "last_execution_height": 1},
            "network_head": {
                "source": "self",
                "decision": "ready",
                "height": 1,
                "block_hash": "block",
                "execution_block_hash": "execution-block",
                "execution_state_root": "state-root",
                "observed_peer_count": 0,
                "fresh_peer_count": 0,
            },
            "checkpoint": None,
            "local_peer_id": "peer-sequencer",
            "connected_peers": [],
            "connected_peer_count": 0,
            "proof": {
                "schema_version": "oasis7.rebuild_proof.v1",
                "signer_id": "sequencer-node",
                "signer_public_key_hex": public_key_hex,
                "signed_payload_sha256": hashlib.sha256(claims_bytes).hexdigest(),
                "signature_hex": signature_hex,
            },
        }
        raw_proof.write_text(json.dumps(raw_value, ensure_ascii=True, separators=(",", ":")), encoding="utf-8")
        receipt = self.root / "runtime-verification-receipt.json"
        receipt_value = {
            "schema_version": "oasis7.rebuild_proof_verification.v1",
            "proof_schema_version": "oasis7.rebuild_proof.v1",
            "signer_id": "sequencer-node",
            "signer_public_key_hex": public_key_hex,
            "signed_payload_sha256": hashlib.sha256(claims_bytes).hexdigest(),
            "local_peer_id": "peer-sequencer",
            "proof_sha256": sha256(raw_proof),
            "verified": True,
        }
        receipt.write_text(json.dumps(receipt_value), encoding="utf-8")
        verifier = self.root / "governed-runtime-verifier.py"
        verifier.write_text(
            "#!/usr/bin/env python3\nimport json\nprint(" + repr(json.dumps(receipt_value)) + ")\n",
            encoding="utf-8",
        )
        verifier.chmod(0o755)
        trusted_root = {"allowlist": [{"signer_id": "sequencer-node", "public_key_hex": public_key_hex}]}
        summary = module.verify_signed_attestation(
            receipt,
            trusted_root,
            "runtime receipt",
            "sequencer-204",
            raw_proof,
            verifier,
        )
        self.assertEqual(summary["peer_id"], "peer-sequencer")
        self.assertEqual(summary["proof_sha256"], sha256(raw_proof))
        forged_verifier = self.root / "forged-runtime-verifier.py"
        forged_receipt = dict(receipt_value)
        forged_receipt["proof_sha256"] = "0" * 64
        forged_verifier.write_text(
            "#!/usr/bin/env python3\nimport json\nprint(" + repr(json.dumps(forged_receipt)) + ")\n",
            encoding="utf-8",
        )
        forged_verifier.chmod(0o755)
        with self.assertRaises(SystemExit):
            module.verify_signed_attestation(
                receipt,
                trusted_root,
                "runtime receipt",
                "sequencer-204",
                raw_proof,
                forged_verifier,
            )
        other_raw_proof = self.root / "other-raw-rebuild-proof.json"
        other_raw_proof.write_bytes(b"another canonical raw signed proof\n")
        with self.assertRaises(SystemExit):
            module.verify_signed_attestation(receipt, trusted_root, "runtime receipt", "sequencer-204", other_raw_proof)
        raw_proof.write_bytes(b"different raw signed proof\n")
        with self.assertRaises(SystemExit):
            module.verify_signed_attestation(receipt, trusted_root, "runtime receipt", "sequencer-204", raw_proof)

    def test_runtime_verification_receipt_rejects_unvalidated_raw_envelope(self) -> None:
        spec = importlib.util.spec_from_file_location("pair_rebuild_runtime_negative_test", EXECUTOR)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        raw_proof = self.root / "malformed-raw-rebuild-proof.json"
        raw_proof.write_text(
            json.dumps({
                "schema_version": "oasis7.rebuild_status.v1",
                "local_peer_id": "peer-sequencer",
                "proof": {
                    "schema_version": "oasis7.rebuild_proof.v1",
                    "signer_id": "sequencer-node",
                    "signer_public_key_hex": "a" * 64,
                    "signed_payload_sha256": "b" * 64,
                    "signature_hex": "c" * 128,
                },
            }),
            encoding="utf-8",
        )
        receipt = self.root / "malformed-runtime-verification-receipt.json"
        receipt.write_text(
            json.dumps({
                "schema_version": "oasis7.rebuild_proof_verification.v1",
                "proof_schema_version": "oasis7.rebuild_proof.v1",
                "signer_id": "sequencer-node",
                "signer_public_key_hex": "a" * 64,
                "signed_payload_sha256": "b" * 64,
                "local_peer_id": "peer-sequencer",
                "proof_sha256": sha256(raw_proof),
                "verified": True,
            }),
            encoding="utf-8",
        )
        trust_root = {"allowlist": [{"signer_id": "sequencer-node", "public_key_hex": "a" * 64}]}
        with self.assertRaises(SystemExit):
            module.verify_signed_attestation(receipt, trust_root, "runtime receipt", "sequencer-204", raw_proof)
        with self.assertRaises(SystemExit):
            module.verify_signed_attestation(receipt, trust_root, "runtime receipt", "sequencer-204")



def _load_plan_contract_tests():
    plan_path = Path(__file__).resolve().parent / "p2p-public-testnet-validator-pair-rebuild-plan.test.py"
    spec = importlib.util.spec_from_file_location("pair_rebuild_plan_contract_tests", plan_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to load plan contract tests from {plan_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.ValidatorPairRebuildPlanContractTests


ValidatorPairRebuildPlanContractTests = _load_plan_contract_tests()


if __name__ == "__main__":
    unittest.main()
