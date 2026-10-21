import importlib.util
import io
import json
from pathlib import Path
import stat
import time
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
        for query in m.QUERIES.values():
            self.assertIn('file.getRelativePath() = "' + m.TARGET + '"', query)
            self.assertIn('file.getRelativePath().regexpMatch(".*/' + m.TARGET.replace('.', '[.]') + '")', query)
            self.assertIn('select file.getRelativePath()', query)
            self.assertNotIn('matches("%/executable_image_tests.rs")', query)
        def eligible(path): return path == m.TARGET or path.endswith("/" + m.TARGET)
        self.assertTrue(eligible("checkout/" + m.TARGET))
        self.assertFalse(eligible("foreign/executable_image_tests.rs"))
        self.assertFalse(eligible(m.TARGET.replace("_", "X")))

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
