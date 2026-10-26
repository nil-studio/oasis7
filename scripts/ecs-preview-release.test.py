#!/usr/bin/env python3
"""Offline publication ordering, schema and failure-boundary regressions."""
import copy
import hashlib
import json
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace
from ecs_preview_release import canonical, publish, validate_channel, validate_manifest, object_key

PREFIX = "preview"
A, B = "a" * 40, "b" * 40


def manifest(source=A, run=1):
    rid = f"{source}/{run}-1"
    return dict(schema_version=1, release_id=rid, source_sha=source, archive_key=f"{PREFIX}/releases/{rid}/bundle.tar.gz", archive_sha256=hashlib.sha256(b"qualified bundle").hexdigest(), platform="linux-x64", components=["player"], compatibility=dict(program_contract="v1", persistence_contract="v1", identity_contract="v1"))


class Missing(Exception):
    status = 404


class Store:
    prefix = PREFIX

    def __init__(self):
        self.objects = {}
        self.events = []
        self.fail = None

    def get(self, key):
        if key not in self.objects:
            raise Missing()
        return self.objects[key]

    def get_with_etag(self, key):
        return self.get(key), str(hash(self.get(key)))

    def put_immutable(self, key, data):
        if self.fail and self.fail in key:
            raise OSError("injected upload failure")
        if key in self.objects and self.objects[key] != data:
            raise ValueError("immutable collision")
        self.objects[key] = data
        self.events.append(key)

    def verify_archive(self, key, expected):
        if hashlib.sha256(self.get(key)).hexdigest() != expected:
            raise ValueError("bad archive")

    def put_channel(self, key, data, etag=None):
        if key in self.objects and etag != str(hash(self.objects[key])):
            raise ValueError("channel compare-and-swap failed")
        self.objects[key] = data
        self.events.append(key)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.store = Store()

    def send(self, m=None, seq=1, archive=b"qualified bundle", rollback=False):
        return publish(self.store, m or manifest(), archive, str(seq), seq, lambda a, b: a == b or (a, b) == (A, B), rollback)

    def test_channel_last_and_exact_bytes(self):
        result = self.send()
        self.assertEqual(self.store.events[-1], "preview/channels/preview.json")
        self.assertEqual(result["manifest_key"], self.store.events[-2])
        self.assertEqual(self.store.get(result["manifest_key"]), canonical(manifest()))

    def test_incomplete_upload_never_advances(self):
        for fail in ("bundle.tar.gz", "manifest.json"):
            with self.subTest(fail=fail):
                self.store = Store()
                self.store.fail = fail
                with self.assertRaises(OSError):
                    self.send()
                self.assertNotIn("preview/channels/preview.json", self.store.objects)

    def test_bad_archive_never_uploads(self):
        with self.assertRaises(ValueError):
            self.send(archive=b"corrupt")
        self.assertEqual(self.store.events, [])

    def test_late_run_and_rerun_rejected(self):
        self.send(seq=2)
        before = len(self.store.events)
        self.send(seq=2)
        self.assertEqual(before, len(self.store.events))
        with self.assertRaisesRegex(ValueError, "stale"):
            self.send(seq=1)

    def test_ancestry_beats_monotonic_counter(self):
        self.send(manifest(B), seq=2)
        with self.assertRaisesRegex(ValueError, "ancestry"):
            self.send(manifest(A), seq=3)

    def test_new_rollback_intent_reuses_retained_bytes(self):
        old = manifest(A)
        self.send(old)
        self.send(manifest(B, 2), seq=2)
        count = len(self.store.events)
        result = self.send(old, seq=3, archive=None, rollback=True)
        self.assertTrue(result["rollback"])
        self.assertEqual(len(self.store.events), count + 1)
        self.assertEqual(result["source_sha"], A)

    def test_retained_corruption_does_not_update_channel(self):
        old = manifest()
        self.send(old)
        before = self.store.get("preview/channels/preview.json")
        self.store.objects[old["archive_key"]] = b"bad"
        with self.assertRaises(ValueError):
            self.send(old, seq=2, archive=None)
        self.assertEqual(before, self.store.get("preview/channels/preview.json"))

    def test_strict_schema_and_no_self_authorized_safety(self):
        for mutate in (lambda m: m.update(safe=True), lambda m: m.update(platform="darwin"), lambda m: m["compatibility"].update(safe=True), lambda m: m.update(archive_key="preview/../world"), lambda m: m.update(components=["arbitrary-command"])):
            m = copy.deepcopy(manifest())
            mutate(m)
            with self.assertRaises(ValueError):
                validate_manifest(m, PREFIX)
        channel = self.send()
        channel["sequence"] = True
        with self.assertRaises(ValueError):
            validate_channel(channel, PREFIX)
        with self.assertRaises(ValueError):
            object_key("other/releases/x", PREFIX)

    def test_streaming_transport_and_short_lived_auth(self):
        from ecs_preview_release import OssStore
        class Result:
            etag = "etag"
            def __init__(self, data):
                import io
                self.stream = io.BytesIO(data)
            def read(self, size=-1):
                self.assert_bounded(size)
                return self.stream.read(size)
            def assert_bounded(self, size):
                if size < 0:
                    raise AssertionError("unbounded object read")
            def close(self):
                self.stream.close()
        class Bucket:
            def __init__(self, *args):
                self.data = {}
                self.headers = []
                self.version = None
            def get_object(self, key):
                if key not in self.data:
                    raise Missing()
                return Result(self.data[key])
            def put_object(self, key, data, headers=None):
                self.headers.append(headers)
                if headers and key in self.data:
                    error = RuntimeError("exists")
                    error.status = 409
                    raise error
                self.data[key] = data.read() if hasattr(data, "read") else data
            def get_bucket_versioning(self):
                return SimpleNamespace(status=self.version)
        sdk = SimpleNamespace(StsAuth=lambda *args: args, Bucket=Bucket)
        env = dict(ALIBABA_CLOUD_ACCESS_KEY_ID="id", ALIBABA_CLOUD_ACCESS_KEY_SECRET="secret", ALIBABA_CLOUD_SECURITY_TOKEN="token")
        with patch.dict("sys.modules", {"oss2": sdk}), patch.dict("os.environ", env, clear=True):
            store = OssStore("https://oss-cn-example.aliyuncs.com", "preview-test", PREFIX)
            store.assert_immutable_bucket()
            store.bucket.version = "Suspended"
            with self.assertRaises(ValueError):
                store.assert_immutable_bucket()
            with tempfile.TemporaryDirectory() as directory:
                source = Path(directory) / "bundle"
                source.write_bytes(b"x" * (1024 * 1024 + 7))
                digest = hashlib.sha256(source.read_bytes()).hexdigest()
                key = "preview/releases/archive"
                store.put_immutable_file(key, source, digest)
                store.put_immutable_file(key, source, digest)  # Same bytes may resume.
                target = Path(directory) / "download"
                self.assertEqual(store.download_to(key, target, 2 * 1024 * 1024), source.stat().st_size)
                self.assertEqual(target.read_bytes(), source.read_bytes())
                with self.assertRaises(ValueError):
                    store.download_to(key, Path(directory) / "oversize", 100)
                source.write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    store.put_immutable_file(key, source, hashlib.sha256(b"changed").hexdigest())
            for endpoint in ("https://user:password@oss-cn-example.aliyuncs.com", "https://oss-cn-example.aliyuncs.com:443", "https://oss-cn-example.aliyuncs.com/#fragment"):
                with self.assertRaises(ValueError):
                    OssStore(endpoint, "preview-test", PREFIX)
        with patch.dict("sys.modules", {"oss2": sdk}), patch.dict("os.environ", {"ALIBABA_CLOUD_ACCESS_KEY_ID": "long-lived"}, clear=True):
            with self.assertRaises(KeyError):
                OssStore("https://oss-cn-example.aliyuncs.com", "preview-test", PREFIX)

    def test_workflow_authority_is_job_scoped(self):
        from pathlib import Path
        text = (Path(__file__).resolve().parent.parent / ".github/workflows/ecs-packages.yml").read_text()
        publisher = text.split("  publish-preview:\n")[1]
        for gate in ("inputs.publish_preview", "!inputs.measure_builds", "github.workflow_sha == github.sha", "needs.package.result == 'success'", "needs.native-trusted.result == 'success'", "id-token: write", "fetch-depth: 0"):
            self.assertIn(gate, publisher)
        self.assertNotIn("id-token: write", text.split("  publish-preview:\n")[0])


if __name__ == "__main__":
    unittest.main()
