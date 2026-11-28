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



# Original host publisher contract, with only the former install step removed
# during normalization. Compare the complete sequence rather than dropping an
# arbitrary prefix or checking presence of isolated strings.
ORIGINAL_PUBLISHER = r'''  publish-preview:
    needs: [package, native-trusted]
    if: >-
      always() && !cancelled() && inputs.publish_preview && !inputs.measure_builds &&
      github.repository == 'nil-studio/oasis7' && github.event_name == 'workflow_dispatch' &&
      github.ref == 'refs/heads/main' && github.workflow_sha == github.sha &&
      ((inputs.retained_release == '' && needs.package.result == 'success' && needs.native-trusted.result == 'success') ||
       (inputs.retained_release != '' && needs.package.result == 'skipped' && needs.native-trusted.result == 'skipped'))
    permissions:
      contents: read
      actions: read
      id-token: write
    concurrency:
      group: ecs-preview-channel-publisher
      cancel-in-progress: false
    environment: ecs-preview-publisher
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          ref: ${{ github.sha }}
          fetch-depth: 0
          persist-credentials: false
      - uses: actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333 # v8.0.2
        if: inputs.retained_release == ''
        with:
          name: oasis7-ecs-linux-x64
          path: output/qualified
      - name: Install pinned OSS transport
        run: python3 -m pip install 'oss2==2.19.1'
      - name: Obtain short-lived OSS publisher credentials
        uses: aliyun/configure-aliyun-credentials-action@1e5248c8d5d93a8781ac344a68e19a43341e79e6
        with:
          oidc-provider-arn: ${{ vars.ECS_PREVIEW_OIDC_PROVIDER_ARN }}
          role-to-assume: ${{ vars.ECS_PREVIEW_PUBLISH_ROLE_ARN }}
          audience: sts.aliyuncs.com
          role-session-expiration: 1800
      - name: Publish immutable objects and advance channel last
        env:
          GH_TOKEN: ${{ github.token }}
          OSS_ENDPOINT: ${{ vars.ECS_PREVIEW_OSS_ENDPOINT }}
          OSS_BUCKET: ${{ vars.ECS_PREVIEW_OSS_BUCKET }}
          OSS_PREFIX: ${{ vars.ECS_PREVIEW_OSS_PREFIX }}
          PROGRAM_CONTRACT: ${{ vars.ECS_PREVIEW_PROGRAM_CONTRACT }}
          PERSISTENCE_CONTRACT: ${{ vars.ECS_PREVIEW_PERSISTENCE_CONTRACT }}
          IDENTITY_CONTRACT: ${{ vars.ECS_PREVIEW_IDENTITY_CONTRACT }}
          COMPONENTS: ${{ vars.ECS_PREVIEW_COMPONENTS }}
          RETAINED_RELEASE: ${{ inputs.retained_release }}
          ROLLBACK: ${{ inputs.rollback_preview }}
          GITHUB_WORKFLOW_SHA: ${{ github.workflow_sha }}
        shell: bash
        run: |
          set -euo pipefail
          args=(--endpoint "$OSS_ENDPOINT" --bucket "$OSS_BUCKET" --prefix "$OSS_PREFIX")
          if [[ -n "$RETAINED_RELEASE" ]]; then
            args+=(--retained-release "$RETAINED_RELEASE")
          else
            archives=(output/qualified/*.tar.gz)
            [[ ${#archives[@]} == 1 && -f "${archives[0]}" ]]
            args+=(--archive "${archives[0]}" --program-contract "$PROGRAM_CONTRACT"
                   --persistence-contract "$PERSISTENCE_CONTRACT" --identity-contract "$IDENTITY_CONTRACT"
                   --components "${COMPONENTS:-player}")
          fi
          if [[ "$ROLLBACK" == true ]]; then args+=(--rollback); fi
          python3 scripts/ecs-preview-release.py "${args[@]}"
'''
PUBLISHER_CONTAINER = r'''    container:
      image: ghcr.io/nil-studio/oasis7/ci-ecs-oss-publisher@sha256:524d6cd5cbeb5190ade258a4f4e6ae0c63dc6839fa84b6700f58f99884ae99a4
      credentials:
        username: ${{ github.actor }}
        password: ${{ github.token }}
      options: --user 1001:1001 --init
    defaults:
      run:
        shell: bash
'''
PUBLISHER_PREFLIGHT = r'''      - name: Verify the preinstalled OSS publisher and trusted checkout
        run: |
          set -euo pipefail
          test "$GITHUB_REF" = refs/heads/main
          test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"
          test "$(git rev-parse HEAD)" = "$GITHUB_SHA"
          test "$(id -u):$(id -g)" = 1001:1001
          test "$(command -v python3)" = /opt/oasis7-oss/bin/python3
          test ! -w /opt/oasis7-oss
          /usr/local/bin/node -e 'require("node:assert/strict").equal(process.versions.node.split(".")[0], "24")'
          gh --version | grep -E '^gh version 2[.]105[.]0( |$)'
          python3 -I -B /opt/oasis7-oss-proof/smoke.py
          python3 -I -B -c 'import importlib; engine=importlib.import_module("crcmod.crcmod"); native=importlib.import_module("crcmod._crcfunext"); assert engine._usingExtension is True and engine._crcfun is native'
'''
OLD_OSS_INSTALL = r'''      - name: Install pinned OSS transport
        run: python3 -m pip install 'oss2==2.19.1'
'''


def validate_publisher_image(text):
    import re
    match = re.search(r'^  publish-preview:\n.*?(?=^  [a-z][a-z0-9_-]*:\n|\Z)', text, re.M | re.S)
    assert match
    job = match.group(0)
    permission = '      packages: read\n'
    assert job.count(permission) == job.count(PUBLISHER_CONTAINER) == job.count(PUBLISHER_PREFLIGHT) == 1
    assert '    permissions:\n      contents: read\n      actions: read\n      id-token: write\n      packages: read\n' in job
    checkout = ORIGINAL_PUBLISHER.split('    steps:\n', 1)[1].split('      - uses: actions/download-artifact@', 1)[0]
    download = '      - uses: actions/download-artifact@'
    assert checkout + PUBLISHER_PREFLIGHT + download in job
    normalized = job.replace(permission, '', 1).replace(PUBLISHER_CONTAINER, '', 1).replace(PUBLISHER_PREFLIGHT, '', 1)
    assert normalized.rstrip() == ORIGINAL_PUBLISHER.replace(OLD_OSS_INSTALL, '', 1).rstrip()


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


    def test_publisher_image_preserves_original_authority_and_every_business_step(self):
        text = (Path(__file__).resolve().parent.parent / '.github/workflows/ecs-packages.yml').read_text()
        validate_publisher_image(text)
        publisher = text.split('  publish-preview:\n', 1)[1]
        for old, new in (
            ('524d6cd5cbeb5190ade258a4f4e6ae0c63dc6839fa84b6700f58f99884ae99a4', '0' * 64),
            ('--user 1001:1001 --init', '--privileged'),
            ('packages: read', 'packages: write'),
            ('timeout-minutes: 10', 'timeout-minutes: 11'),
            ('inputs.publish_preview && !inputs.measure_builds', 'inputs.publish_preview'),
            ('test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"', 'true'),
            ('role-session-expiration: 1800', 'role-session-expiration: 3600'),
            ('[[ ${#archives[@]} == 1 && -f "${archives[0]}" ]]', 'true'),
            ('--components "${COMPONENTS:-player}"', '--components player'),
        ):
            with self.subTest(old=old), self.assertRaises(AssertionError):
                validate_publisher_image('  publish-preview:\n' + publisher.replace(old, new, 1))
        misplaced_permission = text.replace('      id-token: write\n      packages: read\n', '      id-token: write\n').replace('    concurrency:\n', '    concurrency:\n      packages: read\n')
        with self.assertRaises(AssertionError):
            validate_publisher_image(misplaced_permission)
        with self.assertRaises(AssertionError):
            validate_publisher_image(text.replace(PUBLISHER_PREFLIGHT, '') + PUBLISHER_PREFLIGHT)
        with self.assertRaises(AssertionError):
            validate_publisher_image(text.replace(PUBLISHER_PREFLIGHT, PUBLISHER_PREFLIGHT + OLD_OSS_INSTALL))


if __name__ == "__main__":
    unittest.main()
