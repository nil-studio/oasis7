import importlib.util
import io
import json
from pathlib import Path
import stat
import time
import os
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
import unittest
import urllib.request
import zipfile

spec = importlib.util.spec_from_file_location("production_macros", Path(__file__).with_name("codeql-rust-production-macros.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class Guards(unittest.TestCase):
    def archive(self, entries):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as archive:
            for name, mode in entries:
                info = zipfile.ZipInfo(name)
                info.external_attr = mode << 16
                archive.writestr(info, b"source")
        return zipfile.ZipFile(stream)

    def test_valid_outer_and_absolute_source_key(self):
        with self.archive([("db/src.zip", stat.S_IFREG | 0o600)]) as archive:
            self.assertEqual(len(m.zip_inputs(archive)), 1)
        with self.archive([("/checkout/src/lib.rs", stat.S_IFREG | 0o600)]) as archive:
            self.assertEqual(len(m.zip_inputs(archive, nested=True)), 1)
            with self.assertRaises(RuntimeError): m.zip_inputs(archive)

    def test_unsafe_and_special_members(self):
        for name, mode in [("../escape", stat.S_IFREG), ("a/../b", stat.S_IFREG), ("a\\b", stat.S_IFREG), ("link", stat.S_IFLNK), ("fifo", stat.S_IFIFO)]:
            with self.subTest(name=name), self.archive([(name, mode)]) as archive:
                with self.assertRaises(RuntimeError): m.zip_inputs(archive, nested=True)

    def test_duplicate_and_nested_size_rejected(self):
        with self.archive([("a", stat.S_IFREG), ("a", stat.S_IFREG)]) as archive:
            with self.assertRaises(RuntimeError): m.zip_inputs(archive)
        class Huge:
            def infolist(self):
                info = zipfile.ZipInfo("source")
                info.file_size = 4 * 1024**3 + 1
                return [info]
        with self.assertRaises(RuntimeError): m.zip_inputs(Huge(), nested=True)

    def test_redirect_strips_credentials_and_rejects_http(self):
        original = urllib.request.Request(m.API, headers={"Authorization": "Bearer secret"})
        redirected = m.NoCredentialRedirect().redirect_request(original, None, 302, "", {}, "https://objects.example/db")
        self.assertIsNone(redirected.get_header("Authorization"))
        with self.assertRaises(RuntimeError):
            m.NoCredentialRedirect().redirect_request(original, None, 302, "", {}, "http://objects.example/db")

    def test_expired_zero_requests(self):
        class Opener:
            def open(self, *args, **kwargs): raise AssertionError("must not request")
        with self.assertRaises(TimeoutError): m.request(Opener(), "token", "application/zip", time.monotonic() - 1)

    def test_metadata_identity_drift_rejected(self):
        class Response(io.BytesIO):
            pass
        class Opener:
            def __init__(self, value): self.value = value
            def open(self, *args, **kwargs): return Response(json.dumps(self.value).encode())
        correct = dict(id=m.DB_ID, commit_oid=m.DB_COMMIT, size=m.DB_BYTES, language="rust")
        self.assertEqual(m.identity(Opener(correct), "token", time.monotonic()+1), correct)
        for key, value in [("id", 0), ("commit_oid", "changed"), ("size", 0), ("language", "javascript")]:
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                m.identity(Opener(dict(correct, **{key:value})), "token", time.monotonic()+1)

    def test_queries_bind_full_source_not_basename(self):
        for name, query in m.QUERIES.items():
            if name == "SourceAst":
                self.assertIn(m.TARGET, query)
                self.assertIn("select mc.getLocation().getFile().getRelativePath()", query)
                continue
            self.assertIn('file.getRelativePath() = "' + m.TARGET + '"', query)
            self.assertIn('file.getRelativePath().regexpMatch(".*/' + m.TARGET.replace('.', '[.]') + '")', query)
            self.assertIn('select file.getRelativePath()', query)
            self.assertNotIn('matches("%/executable_image_tests.rs")', query)
        def eligible(path): return path == m.TARGET or path.endswith("/" + m.TARGET)
        self.assertTrue(eligible("checkout/" + m.TARGET))
        self.assertFalse(eligible("foreign/executable_image_tests.rs"))
        self.assertFalse(eligible(m.TARGET.replace("_", "X")))

    def test_presence_preserves_fixed_positive_criterion_and_rows(self):
        query = m.QUERIES["MacroLayers"]
        self.assertNotIn("count(", query)
        for flag in ("entity", "expanded"):
            self.assertIn(flag + " = 1 and exists", query)
            self.assertIn(flag + " = 0 and not exists", query)
        for layer in ("unconditional", "mac-outer", "mac-panic", "linux-outer", "linux-format", "linux-panic"):
            self.assertIn('layer = "' + layer + '"', query)
        for entity_count, expanded_count in ((0,0),(1,0),(1,1),(4,2)):
            self.assertEqual((int(entity_count > 0), int(expanded_count > 0)),
                             (int(bool(entity_count)), int(bool(expanded_count))))
        source = Path(__file__).with_name("codeql-rust-production-macros.py").read_text()
        self.assertIn('for name in ("SourceAst", "ClosureFlow", "MacroFlow", "MacroLayers")', source)
        self.assertNotIn("DataFlow", m.QUERIES["SourceAst"])

    def test_cost_probe_controls_and_original_source_query(self):
        self.assertEqual(m.COST_QUERIES["Engine"], "select 1\n")
        self.assertNotIn("MacroCall", m.COST_QUERIES["ExactFile"])
        self.assertNotIn("getLocation", m.COST_QUERIES["MacroPopulation"])
        self.assertEqual(m.COST_QUERIES["SourceAst"], m.QUERIES["SourceAst"])

    def test_framing_keeps_raw_partial_without_event_schema_gate(self):
        with tempfile.TemporaryDirectory() as temp:
            raw=Path(temp)/"raw"; dest=Path(temp)/"events"
            content=b'progress\n{"unknown_schema":true}\n{"partial":'
            raw.write_bytes(content)
            result=m.frame_complete_events(raw,dest)
            self.assertEqual(raw.read_bytes(),content)
            self.assertEqual(dest.read_bytes(),b'{"unknown_schema":true}\n')
            self.assertEqual(result["partial_or_non_object_lines"],1)
            self.assertFalse(result["event_schema_interpreted"])

    def test_dev_stderr_is_captured_and_timeout_retains_partial_stage(self):
        spec=importlib.util.spec_from_file_location("owned_cost_runner",Path(__file__).with_name("codeql-rust-model-diagnostic.py"))
        runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            command=[sys.executable,"-c","import time; f=open('/dev/stderr','w'); f.write('{\"stage\":\"started\"}\\n'); f.flush(); time.sleep(2)"]
            with self.assertRaises(RuntimeError):
                runner.run(command,root,root/"raw.log",time.monotonic()+5,.1,root,os.environ)
            self.assertIn(b'"stage":"started"',(root/"raw.log").read_bytes())
            result=json.loads((root/"raw.log.result.json").read_text())
            self.assertEqual(result["cleanup_errors"],[])
            with patch.object(runner,"CAP",1024*1024+1024):
                with self.assertRaises(RuntimeError):
                    runner.run([sys.executable,"-c","open('/dev/stderr','w').write('x'*4096)"],root,root/"cap.log",time.monotonic()+5,2,root,os.environ)
            self.assertEqual((root/"cap.log").stat().st_size,0)

    def cost_fake(self, failure, remaining=1000):
        temporary=tempfile.TemporaryDirectory();root=Path(temporary.name)
        output=root/"out";output.mkdir();work=root/"work";work.mkdir()
        (output/"query-run-help.log").write_text("--evaluator-log= --evaluator-log-minify --[no-]tuple-counting --verbosity=")
        args=SimpleNamespace(codeql=Path("/codeql"),work=work,output=output,deadline=time.monotonic()+remaining)
        calls=[]
        def run(argv,cwd,log,end,timeout,artifact,env):
            calls.append(argv)
            if "log-summary" in argv and "--help" in argv:
                log.write_text("--format=text");return
            log.write_text('{"unknown_event":"started"}\n')
            error=None;cleanup=[]
            if "query" in argv and "run" in argv and "Engine.ql" in str(argv):
                error,cleanup=failure
            result=dict(error=error,cleanup_errors=cleanup,exit=143 if error else 0)
            log.with_suffix(log.suffix+".result.json").write_text(json.dumps(result))
            if error:raise RuntimeError("controlled")
        return temporary,args,calls,run

    def test_only_clean_owned_timeout_continues_distinct_probes(self):
        timeout="TimeoutError('diagnostic phase deadline exhausted')"
        for failure,expected in [((timeout,[]),4),(("RuntimeError('cap')",[]),1),((timeout,["unknown"]),1)]:
            temporary,args,calls,run=self.cost_fake(failure)
            try:
                with self.assertRaises(RuntimeError):m.cost_probes(run,args,{},args.work,"packs",args.work,args.work)
                probes=[c for c in calls if "query" in c and "run" in c]
                self.assertEqual(len(probes),expected)
                if expected == 1:
                    self.assertFalse(any("--format=text" in c for c in calls))
                    result=json.loads((args.output/"cost-probe-results.json").read_text())
                    self.assertTrue(result["unsafe_stop"])
                    self.assertNotIn("framing",result["probes"][0])
                self.assertTrue((args.output/"cost-probe-results.json").exists())
            finally:temporary.cleanup()

    def test_summary_failure_starts_no_more_summaries(self):
        temporary,args,calls,run=self.cost_fake((None,[]))
        def failing_summary(argv,cwd,log,end,timeout,artifact,env):
            run(argv,cwd,log,end,timeout,artifact,env)
            if "--format=text" in argv:
                raise RuntimeError("controlled summary cap or IO failure")
        try:
            with self.assertRaises(RuntimeError):m.cost_probes(failing_summary,args,{},args.work,"packs",args.work,args.work)
            self.assertEqual(sum("--format=text" in c for c in calls),1)
            result=json.loads((args.output/"cost-probe-results.json").read_text())
            self.assertTrue(result["summary_errors"])
            self.assertEqual(len(result["probes"]),4)
        finally:temporary.cleanup()

    def test_insufficient_remaining_starts_no_probe(self):
        temporary,args,calls,run=self.cost_fake((None,[]),remaining=209)
        try:
            with self.assertRaises(RuntimeError):m.cost_probes(run,args,{},args.work,"packs",args.work,args.work)
            self.assertFalse(any("query" in c and "run" in c for c in calls))
        finally:temporary.cleanup()

    def test_actual_driver_and_workflow_token_boundary(self):
        driver = Path(__file__).with_name("codeql-rust-model-diagnostic.py").read_text()
        self.assertLess(driver.index('env.pop("GH_TOKEN", None)'), driver.index('run([cli, "version"'))
        self.assertIn('env.pop("GITHUB_TOKEN", None)', driver)
        self.assertIn('mode.add_argument("--production-macro-database"', driver)
        workflow = Path(".github/workflows/rust.yml").read_text()
        job = workflow.split("  codeql-rust-model-diagnostic:",1)[1].split("\n  select:",1)[0]
        self.assertIn("security-events: read", job)
        self.assertIn("--production-macro-database", job)
        self.assertIn("GH_TOKEN: ${{ github.token }}", job)

if __name__ == "__main__": unittest.main()
