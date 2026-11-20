"""Actual report entrypoint regressions for the formal status adapter."""
import json
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OBSERVER = "formal_public_observer_v1"
FULL = "formal_full_state_sync_read_only_v1"


def fixture(profile=OBSERVER):
    now = int(time.time() * 1000)
    identity = dict(incarnation="instance-1", generation=1, config_digest="a" * 64)
    world = dict(world_id="world-1", genesis_digest="b" * 64)
    cut = dict(world=world, chain_id="chain-1", position=1, record_id="c"*64,
               after_state_root="d"*64, active_authority_id="e"*64,
               finalizing_authority_id="f"*64, authority_epoch=1)
    status = dict(api_version="oasis7.local-node-status/v1", node_id="node-1", runtime_profile=profile,
        chain=dict(runtime_profile=profile, world_id="world-1", chain_id="chain-1"),
        config_digest=identity["config_digest"], local_generation=1,
        instance=dict(api_version="oasis7.local-node-instance/v1",state="running",**identity),
        blocker=None,configuration_changed=False,formal_readiness=True,can_propose=False,
        configured_role="observer" if profile == OBSERVER else "full_state_sync")
    status["chain"]["formal_observer" if profile == OBSERVER else "formal_full_sync"] = dict(
        api_version="oasis7.formal-public-observer/v1" if profile == OBSERVER else "oasis7.formal-full-state-sync/v1",
        world_id="world-1",chain_id="chain-1",genesis_digest=world["genesis_digest"])
    if profile == OBSERVER:
        status["formal_public_observer"] = dict(api_version="oasis7.formal-public-observer-status/v1",
            runtime_profile=profile,**identity,read_state="qualified_public_read",read_ready=True,
            can_propose=False,state_sync_scope="public_projection_only",full_state_sync_readiness="unknown",
            source_freshness="unknown",last_verified_commit=dict(world=world,position=1,
            execution_block_hash="h",state_root_ref="r",binding={}),logical_tick=1,
            last_read_observed_unix_ms=now,error=None,public_projection={"secret":"DO_NOT_COPY"})
    else:
        status["formal_full_state_sync"] = dict(api_version="oasis7.formal-full-sync-status/v1",
            runtime_profile=profile,**identity,state="qualified_full_state_read_only",ready=True,can_propose=False,
            state_root_verified=True,world_loaded=True,sync_scope="issuer_granted_full_state_read_only",
            world_id="world-1",chain_id="chain-1",grant_epoch=1,last_verified_cut=cut,
            loaded_cut=cut,lag_positions=0,catch_up_pending=False,last_checked_unix_ms=now,error=None)
    return status


def bft_fixture():
    return dict(api_version="oasis7.bft-node-observation/v1", runtime_profile="bft/version=1",
        incarnation="a"*64, running=True, blocker=None, qualification="unknown",
        protected_services_readiness="unknown", can_propose=False,
        observation=dict(observed_at_unix_ms=time.time_ns()//1_000_000,
            world_id="w",chain_id="c",operator_instance_id="operator",authority_epoch=2,
            height=10,round=3,active_set_hash="b"*64,pending_set_hash=None,binding_digest="c"*64,
            checkpoint_revision=12,observed_lease_generation=1,window_start_height=9,
            committed_window_heights=1,pending_driver_apply=False,
            last_applied_height=9,last_applied_header_id="d"*64))


class BftDiagnosticsTests(unittest.TestCase):
    def project(self, value, source="http"):
        import importlib.util
        spec=importlib.util.spec_from_file_location("bft_adapter", ROOT/"scripts/p2p-profile-readiness.py")
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        return module.project(value,source,time.time_ns()//1_000_000)

    def test_live_facts_never_grant_qualification(self):
        value=bft_fixture()
        value["observation"]["private_action"]="DO_NOT_COPY"
        result=self.project(value)
        self.assertEqual(result["height"],10)
        self.assertEqual(result["last_applied_height"],9)
        self.assertEqual(result["status"],"unknown")
        self.assertFalse(result["can_propose"])
        self.assertEqual(result["protected_services_readiness"],"unknown")
        self.assertNotIn("DO_NOT_COPY",json.dumps(result))
        self.assertTrue(self.project(value,"file")["replay"])

    def test_applied_height_cannot_exceed_observed_head(self):
        # No worker may have applied a block beyond its own reported head.
        # Reject internally impossible diagnostics before projecting receipt facts.
        for source in ("http", "file"):
            status = bft_fixture()
            status["observation"]["last_applied_height"] = 11
            result = self.project(status, source)
            self.assertEqual(result["status"], "unknown")
            self.assertEqual(result["reason"], "invalid_bft_window_or_applied")
            self.assertFalse(result["can_propose"])
            self.assertNotIn("last_applied_height", result)

            status["observation"]["last_applied_height"] = 10
            recovered = self.project(status, source)
            self.assertEqual(recovered["last_applied_height"], 10)
            self.assertEqual(recovered["reason"], "bft_qualification_not_observed")
            self.assertFalse(recovered["can_propose"])

    def test_committed_window_count_cannot_exceed_height_span(self):
        # Worker history counts committed heights beginning at the pinned
        # initial schedule; it cannot include commits beyond the next height.
        for source in ("http", "file"):
            status = bft_fixture()
            status["observation"]["committed_window_heights"] = 2
            result = self.project(status, source)
            self.assertEqual(result["status"], "unknown")
            self.assertEqual(result["reason"], "invalid_bft_window_or_applied")
            self.assertFalse(result["can_propose"])

            status["observation"]["committed_window_heights"] = 1
            recovered = self.project(status, source)
            self.assertEqual(recovered["reason"], "bft_qualification_not_observed")
            self.assertEqual(recovered["committed_window_heights"], 1)
            self.assertFalse(recovered["can_propose"])

    def test_missing_blocker_is_unknown_not_an_unblocked_observation(self):
        for source in ("http", "file"):
            status = bft_fixture()
            del status["blocker"]
            result = self.project(status, source)
            self.assertEqual(result["status"], "unknown")
            self.assertEqual(result["reason"], "invalid_bft_observation")
            self.assertFalse(result["can_propose"])
            self.assertNotIn("producer_blocker", result)

    def test_invalid_claims_missing_facts_and_stale_observation_fail_closed(self):
        for field,value in (("can_propose",True),("qualification","ready"),("running",1),
                            ("protected_services_readiness","ready"),("incarnation","")):
            status=bft_fixture();status[field]=value
            result=self.project(status)
            self.assertEqual(result["status"],"unknown");self.assertFalse(result["can_propose"])
        status=bft_fixture();status["observation"]["observed_at_unix_ms"]-=61000
        self.assertEqual(self.project(status)["status"],"stale")
        status=bft_fixture();status["observation"]["observed_at_unix_ms"]+=61000
        self.assertEqual(self.project(status)["reason"],"bft_observation_clock_uncertain")
        status=bft_fixture();status["observation"]["committed_window_heights"]=65
        self.assertEqual(self.project(status)["reason"],"invalid_bft_window_or_applied")
        status=bft_fixture();status["observation"]=None
        self.assertEqual(self.project(status)["reason"],"bft_observation_unavailable")
        status=bft_fixture();status.update(running=False,blocker="runtime_not_running")
        self.assertEqual(self.project(status)["status"],"blocked")


class ReportTests(unittest.TestCase):
    def report(self, status):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/"input.json";source.write_text(json.dumps(status))
            subprocess.run(["bash",str(ROOT/"scripts/oasis7-node-observability-report.sh"),
                "--status-json-path",str(source),"--out-dir",tmp],check=True,capture_output=True,cwd=tmp)
            summary=json.loads((Path(tmp)/"latest_summary.json").read_text())
            md=(Path(tmp)/"latest_summary.md").read_text()
            self.assertIn("Profile Readiness",md)
            self.assertNotIn("DO_NOT_COPY",json.dumps(summary)+md)
            return summary["profile_readiness"]

    def test_actual_entrypoint_observer(self):
        result=self.report(fixture());self.assertEqual(result["status"],"ready")
        self.assertEqual(result["node_id"],"node-1");self.assertEqual(result["configured_role"],"observer")
        self.assertEqual(result["runtime_profile"],OBSERVER);self.assertIs(result["can_propose"],False)

    def test_full_sync(self):
        self.assertEqual(self.report(fixture(FULL))["status"],"ready")

    def test_no_false_ready(self):
        cases=[("formal_readiness",False),("configuration_changed",True),("can_propose",True),
               ("blocker","DO_NOT_COPY")]
        for key,value in cases:
            status=fixture();status[key]=value
            self.assertNotEqual(self.report(status)["status"],"ready",key)
        for field,value in [("generation",2),("read_ready","true"),("error","DO_NOT_COPY"),
                            ("last_read_observed_unix_ms",0),
                            ("last_read_observed_unix_ms",int(time.time()*1000)+120000),
                            ("api_version","future")]:
            status=fixture();status["formal_public_observer"][field]=value
            self.assertNotEqual(self.report(status)["status"],"ready",field)
        status=fixture();status["instance"]["state"]="stopped"
        self.assertEqual(self.report(status)["status"],"blocked")

    def test_full_sync_invalid_cut_and_identity(self):
        for field,value in [("loaded_cut",None),("world_loaded",False),("state_root_verified",False),
                            ("world_id","wrong"),("grant_epoch",True),("catch_up_pending","false"),("lag_positions",True)]:
            status=fixture(FULL);status["formal_full_state_sync"][field]=value
            self.assertNotEqual(self.report(status)["status"],"ready",field)

    def test_replay_does_not_refresh_and_recovery(self):
        status=fixture();original=status["formal_public_observer"]["last_read_observed_unix_ms"]
        first=self.report(status);second=self.report(status)
        self.assertEqual(first["observed_at_unix_ms"],original)
        self.assertEqual(second["observed_at_unix_ms"],original)
        self.assertGreaterEqual(second["age_ms"],first["age_ms"])
        status["formal_readiness"]=False
        self.assertEqual(self.report(status)["status"],"blocked")
        status["formal_readiness"]=True
        self.assertEqual(self.report(status)["status"],"ready")

    def test_same_world_different_genesis_is_unknown(self):
        for profile in (OBSERVER,FULL):
            status=fixture(profile)
            config=status["chain"]["formal_observer" if profile == OBSERVER else "formal_full_sync"]
            config["genesis_digest"]="9"*64
            self.assertEqual(self.report(status)["status"],"unknown")

    def test_blocked_retains_historical_witness(self):
        status=fixture(FULL);child=status["formal_full_state_sync"]
        child.update(state="sync_unavailable",ready=False,world_loaded=False,loaded_cut=None,error="DO_NOT_COPY")
        status["formal_readiness"]=False
        result=self.report(status)
        self.assertEqual(result["status"],"blocked")
        self.assertEqual(result["last_verified_cut"]["position"],1)
        self.assertEqual(result["grant_epoch"],1)
        self.assertEqual(result["producer_state"],"sync_unavailable")
        self.assertIs(result["producer_ready"],False)
        self.assertIs(result["formal_readiness"],False)
        self.assertEqual(result["instance_state"],"running")
        self.assertEqual(result["observed_at_unix_ms"],child["last_checked_unix_ms"])
        self.assertNotIn("loaded_cut",result)
        status=fixture();status["instance"]["state"]="stopped"
        result=self.report(status)
        self.assertEqual(result["status"],"blocked")
        self.assertEqual(result["last_verified_commit"]["position"],1)

    def test_malformed_objects_and_booleans(self):
        for status in ([], None, True, "status"):
            self.assertEqual(self.report(status)["status"],"unknown")
        for field in ("instance","chain","formal_public_observer"):
            status=fixture();status[field]=[]
            self.assertEqual(self.report(status)["status"],"unknown")
        status=fixture();status["formal_readiness"]=1
        self.assertEqual(self.report(status)["status"],"unknown")
        status=fixture();status["blocker"]={"raw":"DO_NOT_COPY"}
        self.assertEqual(self.report(status)["status"],"unknown")

    def test_opaque_unicode_world_and_chain_identifiers(self):
        for profile in (OBSERVER,FULL):
            status=fixture(profile)
            world="世界@`world";chain="链@``chain"
            status["chain"].update(world_id=world,chain_id=chain)
            config=status["chain"]["formal_observer" if profile == OBSERVER else "formal_full_sync"]
            config.update(world_id=world,chain_id=chain)
            if profile == OBSERVER:
                status["formal_public_observer"]["last_verified_commit"]["world"]["world_id"]=world
            else:
                child=status["formal_full_state_sync"]
                child.update(world_id=world,chain_id=chain)
                for name in ("loaded_cut","last_verified_cut"):
                    child[name]["world"]["world_id"]=world
                    child[name]["chain_id"]=chain
            result=self.report(status)
            self.assertEqual(result["status"],"ready")
            self.assertEqual(result["world_id"],world)
            self.assertEqual(result["chain_id"],chain)

    def test_producer_node_labels(self):
        for label in ("观察者@north`lane", "a"*256, "界"*85+"a"):
            status=fixture();status["node_id"]=label
            self.assertEqual(self.report(status)["node_id"],label)
            self.assertEqual(self.report(status)["status"],"ready")
        for label in ("", " ", "triad-testnet-node", "界"*85+"ab", "a"*257):
            status=fixture();status["node_id"]=label
            self.assertEqual(self.report(status)["status"],"unknown")

    def test_markdown_node_label_is_contained(self):
        label="观察者@`lane`` [link](https://evil.invalid)\n## Inject"
        status=fixture();status["node_id"]=label
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/"input.json";source.write_text(json.dumps(status))
            subprocess.run(["bash",str(ROOT/"scripts/oasis7-node-observability-report.sh"),
                "--status-json-path",str(source),"--out-dir",tmp],check=True,capture_output=True,cwd=tmp)
            md=(Path(tmp)/"latest_summary.md").read_text()
            line=next(line for line in md.splitlines() if line.startswith("- node_id:") and "lane" in line)
            self.assertTrue(line.startswith('- node_id: ```"'))
            self.assertTrue(line.endswith('"```'))
            self.assertIn('\\n## Inject',line)
            self.assertNotIn("\n## Inject",md)

    def test_producer_state_is_allowlisted(self):
        status=fixture();status["formal_public_observer"].update(read_state="DO_NOT_COPY",read_ready=False)
        result=self.report(status)
        self.assertEqual(result["producer_state"],"unknown")
        self.assertEqual(result["status"],"blocked")
        status=fixture();status["formal_public_observer"]["read_ready"]="false"
        self.assertIsNone(self.report(status)["producer_ready"])

    def test_unsupported_and_unknown(self):
        self.assertEqual(self.report({"running":True})["status"],"unsupported")
        status=fixture();status["api_version"]="future"
        self.assertEqual(self.report(status)["status"],"unknown")
        status=fixture();status["runtime_profile"]="future"
        self.assertEqual(self.report(status)["status"],"unknown")

if __name__ == "__main__":
    unittest.main()
