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

    def test_object_prefix_recovers_header_and_preserves_tail_offsets(self):
        with tempfile.TemporaryDirectory() as temp:
            raw=Path(temp)/"raw";dest=Path(temp)/"events"
            data=b'{"header":1}[progress text]\ntext {"not_event":true}\n{"unfinished":'
            raw.write_bytes(data)
            result=m.frame_complete_events(raw,dest)
            self.assertEqual(dest.read_bytes(),b'{"header":1}\n')
            record=result["prefix_records"][0]
            self.assertEqual(data[record["byte_offset"]:record["byte_offset"]+record["object_bytes"]],b'{"header":1}')
            self.assertGreater(record["trailing_bytes"],1)
            self.assertEqual(len(result["partial_records"]),1)
            self.assertEqual(raw.read_bytes(),data)

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

    def test_artifact_only_postprocess_binds_inputs_and_never_runs_queries(self):
        for summary_failure in (False,True):
            with self.subTest(summary_failure=summary_failure),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);output=root/"out";output.mkdir();work=root/"work";work.mkdir()
                codeql=root/"tools/codeql";codeql.parent.mkdir()
                library=codeql.parent/"qlpacks/codeql/rust-all/0.2.23";library.mkdir(parents=True)
                (library/"qlpack.yml").write_text("name: codeql/rust-all\nversion: 0.2.23\n")
                source=library/"codeql/rust/internal/CachedStages.qll";source.parent.mkdir(parents=True);source.write_bytes(b"source\n")
                blob=m.hashlib.sha1(b"blob 7\0source\n").hexdigest()
                raw=b'{"header":1}progress\n{"event":2}\n'
                buffer=io.BytesIO()
                with zipfile.ZipFile(buffer,"w") as archive:
                    for name in m.COST_QUERIES:archive.writestr("cost-"+name+".log",raw)
                data=buffer.getvalue()
                packet=dict(artifact_id=1,run=2,source="source",name="frozen",zip_bytes=len(data),zip_sha256=m.hashlib.sha256(data).hexdigest(),
                    members={"cost-"+n+".log":{"bytes":len(raw),"sha256":m.hashlib.sha256(raw).hexdigest()} for n in m.COST_QUERIES},
                    tag_git_blob_sha1={"internal/CachedStages.qll":blob})
                (root/"codeql-rust-evaluator-input.json").write_text(json.dumps(packet))
                metadata=dict(id=1,name="frozen",size_in_bytes=len(data),digest="sha256:"+packet["zip_sha256"],expired=False,workflow_run=dict(id=2,head_sha="source"))
                class Opener:
                    def open(self,request,timeout):return io.BytesIO(data if request.full_url.endswith("/zip") else json.dumps(metadata).encode())
                calls=[]
                def run(argv,cwd,log,end,timeout,artifact,env):
                    calls.append(argv);self.assertNotIn("GH_TOKEN",env)
                    log.write_text("--format=text" if "--help" in argv else "summary")
                    if summary_failure and "--format=text" in argv:raise RuntimeError("controlled partial summary")
                args=SimpleNamespace(codeql=codeql,work=work,output=output,deadline=time.monotonic()+300)
                with patch.object(m,"__file__",str(root/"helper.py")),patch.object(m.urllib.request,"build_opener",return_value=Opener()),patch.dict(os.environ,{"GH_TOKEN":"controlled-unit-token"}):
                    if summary_failure:
                        with self.assertRaises(RuntimeError):m.inspect_saved_evaluator_evidence(run,args,{})
                    else:m.inspect_saved_evaluator_evidence(run,args,{})
                self.assertTrue(all(c[1:3]==["generate","log-summary"] for c in calls))
                self.assertEqual(sum("--format=text" in c for c in calls),1 if summary_failure else 4)
                self.assertEqual(len(list(output.glob("recovered-*.jsonl"))),4)
                result=json.loads((output/"inspection-result.json").read_text())
                self.assertEqual(result["query_evaluations"],0);self.assertEqual(result["database_downloads"],0)
                self.assertTrue(result["bundle_sources_match_tag"])

    def test_artifact_identity_failure_or_expiry_prevents_download(self):
        packet=dict(artifact_id=1,run=2,source="source",name="frozen",zip_bytes=3,zip_sha256="a"*64)
        correct=dict(id=1,name="frozen",size_in_bytes=3,digest="sha256:"+"a"*64,expired=False,workflow_run=dict(id=2,head_sha="source"))
        for bad in (dict(correct,expired=True),dict(correct,digest="changed"),dict(correct,workflow_run=dict(id=2,head_sha="other"))):
            class Opener:
                def open(self,request,timeout):
                    self.assertion=None
                    if request.full_url.endswith("/zip"):raise AssertionError("must not download")
                    return io.BytesIO(json.dumps(bad).encode())
            with self.assertRaises(RuntimeError):m.artifact_identity(Opener(),"token",packet,time.monotonic()+2)
        class Never:
            def open(self,*args,**kwargs):raise AssertionError("expired must not request")
        with self.assertRaises(TimeoutError):m.artifact_request(Never(),"token",packet,time.monotonic()-1)

    def test_batch_fails_before_evaluation_on_unknown_help_or_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            args = SimpleNamespace(codeql=Path("/cli"), work=root, output=root, deadline=time.monotonic()+569)
            calls = []
            def invoke(argv, cwd, log, *rest, **kw):
                calls.append(argv)
                log.write_text("--additional-packs= --compilation-cache= --max-disk-cache= --evaluator-log= --evaluator-log-minify --[no-]tuple-counting --common-caches= --no-default-compilation-cache")
            with self.assertRaisesRegex(RuntimeError, "requires480"):
                m.batch_queries(invoke, args, {}, root, root, root, root)
            self.assertEqual(len(calls), 1)
            self.assertEqual(calls[0][1:], ["database", "run-queries", "--help", "-v"])
            def unknown(argv, cwd, log, *rest, **kw): log.write_text("unknown")
            with self.assertRaisesRegex(RuntimeError, "option unavailable"):
                m.batch_queries(unknown, args, {}, root, root, root, root)

    def test_batch_timeout_survives_summary_error_without_decode(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); work = root / "work"; out = root / "out"
            work.mkdir(); out.mkdir(); query = out / "queries"; query.mkdir()
            database = work / "db"; database.mkdir(); cache = work / "cache"; cache.mkdir()
            for name, text in m.QUERIES.items(): (query / (name+".ql")).write_text(text)
            args = SimpleNamespace(codeql=Path("/cli"), work=work, output=out, deadline=time.monotonic()+900)
            calls = []
            def invoke(argv, cwd, log, *rest, **kw):
                calls.append(argv)
                if "--help" in argv:
                    log.write_text("--additional-packs= --compilation-cache= --max-disk-cache= --evaluator-log= --evaluator-log-minify --[no-]tuple-counting --common-caches= --[no-]default-compilation-cache")
                elif "run-queries" in argv:
                    log.write_text('{"kind":"header"}\n')
                    log.with_suffix(".log.result.json").write_text(json.dumps({"cleanup_errors": [], "error": "TimeoutError('diagnostic phase deadline exhausted')"}))
                    self.assertEqual(kw["file_limit"], 1024**3)
                    self.assertEqual(argv[-4:], [str(query/(name+".ql")) for name in m.QUERIES])
                    raise RuntimeError("original batch timeout")
                else: raise RuntimeError("summary failure")
            with patch.object(m.shutil, "disk_usage", return_value=SimpleNamespace(free=8*1024**3)):
                with self.assertRaisesRegex(RuntimeError, "original batch timeout"):
                    m.batch_queries(invoke, args, {}, query, work, cache, database)
            self.assertFalse(any("decode" in call for call in calls))
            self.assertEqual(len(calls), 3)
            self.assertTrue((out/"batch-framing.json").is_file())

    def test_anchored_recursion_preserves_star_keys_zero_and_generated_nodes(self):
        # Concrete finite-graph LFP proof control, distinct from QL optimizer evidence.
        parents = {"generated": {"root"}, "leaf": {"generated"}, "self": {"self"},
                   "orphanA": {"orphanB"}, "orphanB": {"orphanA"}}
        roots = {"positive": {"root", "self"}, "negative": {"absent"}, "multi": {"leaf", "root"}}
        nodes = set(parents) | {p for ps in parents.values() for p in ps} | {"absent"}
        for key, bases in roots.items():
            lfp = set(bases)
            while True:
                nxt = lfp | {n for n in nodes if parents.get(n, set()) & lfp}
                if nxt == lfp: break
                lfp = nxt
            reference = set()
            for node in nodes:
                seen = set(); pending = [node]
                while pending:
                    current = pending.pop()
                    if current in seen: continue
                    seen.add(current); pending.extend(parents.get(current, set()))
                if seen & bases: reference.add(node)
            self.assertEqual(lfp & nodes, reference, key)
        self.assertEqual(m.ANCHOR_COMPILE_QUERIES["MacroFlowOriginal"], m.QUERIES["MacroFlow"])
        candidate = m.ANCHOR_COMPILE_QUERIES["MacroFlowAnchored"]
        self.assertEqual(candidate.count("targetDescendant(file, line, path, n.asExpr())"), 2)
        self.assertIn("or targetDescendant(file, line, path, node.getParentNode())", candidate)
        self.assertNotIn("node.getLocation()", candidate)

    def test_compile_mode_returns_before_any_database_or_fixture(self):
        driver = Path(__file__).with_name("codeql-rust-model-diagnostic.py").read_text()
        branch = driver.split("    if args.compile_anchored_parent:\n",1)[1].split("    if peer_mode is not None:",1)[0]
        self.assertIn('"--dump-ra"', branch)
        self.assertIn('"query_evaluations": 0', branch)
        self.assertIn("        return", branch)
        self.assertNotIn('"--dump-dil"', branch)

    def test_actual_driver_and_workflow_token_boundary(self):
        driver = Path(__file__).with_name("codeql-rust-model-diagnostic.py").read_text()
        self.assertLess(driver.index('env.pop("GH_TOKEN", None)'), driver.index('run([cli, "version"'))
        self.assertIn('env.pop("GITHUB_TOKEN", None)', driver)
        self.assertIn('mode.add_argument("--production-macro-database"', driver)
        workflow = Path(".github/workflows/rust.yml").read_text()
        job = workflow.split("  codeql-rust-model-diagnostic:",1)[1].split("\n  select:",1)[0]
        self.assertNotIn("security-events:", job)
        self.assertNotIn("actions: read", job)
        self.assertIn("--compile-anchored-parent", job)
        self.assertNotIn("GH_TOKEN:", job)

if __name__ == "__main__": unittest.main()
