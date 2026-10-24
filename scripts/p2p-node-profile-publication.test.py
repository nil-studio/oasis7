"""The actual common publisher binds local profile collection and freshness."""
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch

SCRIPTS=Path(__file__).resolve().parent

def load(name,filename):
    spec=importlib.util.spec_from_file_location(name,SCRIPTS/filename)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module

publisher=load("profile_publisher","p2p-real-env-observability-attempt.py")
report=load("local_profile_report","p2p-node-profile-report.py")
fixtures=load("profile_fixtures","p2p-profile-readiness.test.py")

class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name).resolve()/"out"
        self.arguments=["--binary-sha256","a"*64,"--data-dir-sha256","b"*64]

    def begin(self,arguments=None):
        return publisher.begin(self.root,self.arguments if arguments is None else arguments,"node-profile")

    def source(self,attempt,result="success",node_id=None):
        directory=Path(attempt["run_dir"])
        config={"binary_sha256":"a"*64,"data_dir_sha256":"b"*64,"timeout_ms":10000}
        publisher.atomic_write(directory/"config.json",publisher.encoded(config))
        payload=fixtures.fixture()
        now=time.time_ns()//1_000_000
        if node_id: payload["node_id"]=node_id
        profile=report.adapter.project(payload,"local_cli",now) if result=="success" else report.unknown("collection_failed")
        collection=dict(config,mode="local_cli",result=result,expected_sample_count=1,attempted_count=1,
                        failed_sample_count=0 if result=="success" else 1,complete=result=="success",
                        started_at_unix_ms=now,finished_at_unix_ms=now)
        source=report.build(profile,collection,now)
        source.update(run_id=str(attempt["sequence"])+"-"+attempt["attempt_id"],run_dir=str(directory))
        (directory/"report").mkdir(exist_ok=True)
        publisher.atomic_write(directory/"report/latest_summary.json",publisher.encoded(source))
        publisher.atomic_write(directory/"report/latest_summary.md",report.markdown(source).encode())
        return source

    def finish(self,attempt,errors=None):
        return publisher.finish(self.root,attempt["sequence"],attempt["attempt_id"],errors or [],"node-profile")

    def test_pending_and_success(self):
        attempt=self.begin();pending,_,code=publisher.read(self.root,"node-profile")
        self.assertEqual(code,1);self.assertEqual(pending["profile_readiness"]["status"],"unknown")
        self.assertFalse(pending["current_measurement_available"])
        self.source(attempt);self.assertEqual(self.finish(attempt),0)
        current,md,code=publisher.read(self.root,"node-profile")
        self.assertEqual(code,0);self.assertEqual(current["profile_readiness"]["status"],"ready")
        self.assertTrue(current["current_measurement_available"])
        self.assertEqual(current["last_success"]["source_identity"]["node_id"],"node-1")
        self.assertIn("Profile Readiness",md)

    def test_read_recomputes_age_without_mutation(self):
        a=self.begin();source=self.source(a);self.finish(a)
        before={name:(self.root/name).read_bytes() for name in ("attempt_state.json","latest_summary.json","latest_summary.md")}
        now=source["collection"]["finished_at_unix_ms"]+60001
        with patch.object(publisher.time,"time_ns",return_value=now*1_000_000):
            current,md,code=publisher.read(self.root,"node-profile")
        self.assertEqual(code,0);self.assertEqual(current["profile_readiness"]["status"],"stale")
        self.assertFalse(current["current_measurement_available"])
        self.assertEqual(current["collection"]["finished_at_unix_ms"],source["collection"]["finished_at_unix_ms"])
        self.assertEqual(current["profile_readiness"]["observed_at_unix_ms"],source["profile_readiness"]["observed_at_unix_ms"])
        self.assertIn('"stale"',md)
        for name,data in before.items(): self.assertEqual((self.root/name).read_bytes(),data)

    def test_failed_latest_preserves_explicit_history(self):
        a=self.begin();self.source(a);self.finish(a)
        b=self.begin();self.source(b,"command_failed")
        self.assertEqual(self.finish(b,[{"stage":"node-profile","exit_code":1,"error":"collector_failed"}]),1)
        current,_,code=publisher.read(self.root,"node-profile")
        self.assertEqual(code,1);self.assertEqual(current["attempt"]["sequence"],2)
        self.assertEqual(current["profile_readiness"]["status"],"unknown")
        self.assertFalse(current["current_measurement_available"])
        self.assertEqual(current["last_success"]["sequence"],1)
        self.assertEqual(current["last_success"]["source_identity"]["node_id"],"node-1")

    def test_changed_scope_does_not_reuse_history(self):
        a=self.begin();self.source(a);self.finish(a)
        self.begin(["--binary-sha256","c"*64])
        current,_,_=publisher.read(self.root,"node-profile")
        self.assertIsNone(current["last_success"])

    def test_changed_identity_replaces_current_history(self):
        a=self.begin();self.source(a);self.finish(a)
        b=self.begin();self.source(b,node_id="node-2");self.finish(b)
        current,_,code=publisher.read(self.root,"node-profile")
        self.assertEqual(code,0)
        self.assertEqual(current["last_success"]["sequence"],2)
        self.assertEqual(current["last_success"]["source_identity"]["node_id"],"node-2")
        self.assertEqual(current["attempt"]["source_identity"]["node_id"],"node-2")

    def test_type_and_artifact_corruption_fail_closed(self):
        a=self.begin();self.source(a);self.finish(a)
        with self.assertRaises(publisher.AttemptError): publisher.read(self.root,"host")
        (self.root/"latest_summary.json").write_text("{}")
        with self.assertRaises(publisher.AttemptError): publisher.read(self.root,"node-profile")

    def test_future_collection_is_unknown(self):
        a=self.begin();source=self.source(a);self.finish(a)
        now=source["collection"]["started_at_unix_ms"]-1
        with patch.object(publisher.time,"time_ns",return_value=now*1_000_000):
            current,_,_=publisher.read(self.root,"node-profile")
        self.assertNotEqual(current["profile_readiness"]["status"],"ready")
        self.assertEqual(current["collection_freshness"]["state"],"unknown")
        self.assertFalse(current["current_measurement_available"])

if __name__=="__main__": unittest.main()
