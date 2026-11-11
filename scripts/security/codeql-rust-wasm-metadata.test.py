#!/usr/bin/env python3
"""Fixed WASM metadata admission/graph/process tests; no Cargo, CodeQL or network."""
import base64
import copy
from contextlib import redirect_stdout
import datetime
import email.utils
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import tarfile
import textwrap
import threading
import time
import types
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / '.github/workflows/codeql-rust-wasm-metadata.yml'
TEXT = WORKFLOW.read_text()
BOOTSTRAP = textwrap.dedent(TEXT.split('          # BEGIN_WASM_METADATA_BOOTSTRAP\n', 1)[1]
                           .split('          # END_WASM_METADATA_BOOTSTRAP', 1)[0])
b = types.ModuleType('wasm_metadata_bootstrap_test')
exec(compile(BOOTSTRAP, str(WORKFLOW), 'exec'), b.__dict__)


def load_driver():
    path = Path(__file__).with_name('codeql-rust-wasm-metadata.py')
    spec = importlib.util.spec_from_file_location('wasm_metadata_driver_test', path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module


class Response:
    status = 200

    def __init__(self, value, date=True):
        self.body = io.BytesIO(json.dumps(value).encode())
        self.headers = {'Date': email.utils.formatdate(usegmt=True)} if date else {}
        self.closed = False

    def read(self, size):
        return self.body.read(size)

    def close(self):
        self.closed = True


class RepositoryFixture:
    """Independent small API fixture, retaining the production endpoint restrictions."""
    def __init__(self, tool, missing_date=False, corrupt=False, failure=None):
        self.tool = tool; self.requests = []; self.responses = []
        self.missing_date, self.corrupt, self.failure = missing_date, corrupt, failure
        self.files = {f'scripts/security/{name}': (name + '\n').encode() for name in b.HELPERS}
        self.files[b.WORKFLOW] = b'name: fixture\n'
        self.blobs = {b.blob_id(data): data for data in self.files.values()}
        self.tree = [{'path': name, 'type': 'blob', 'mode': '100644', 'sha': b.blob_id(data)}
                     for name, data in self.files.items()]

    def open(self, req, timeout):
        self.requests.append((req.full_url, timeout))
        if self.failure:
            raise self.failure
        url = req.full_url
        if '/jobs?' in url:
            now = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(seconds=3)
            value = {'jobs': [{'name': b.JOB_NAME, 'status': 'in_progress', 'started_at': now.isoformat(), 'id': 17,
                               'head_sha': self.tool, 'run_id': 42, 'run_attempt': 1}]}
        elif '/git/commits/' in url:
            value = {'sha': self.tool, 'tree': {'sha': 'a' * 40}}
        elif '/git/trees/' in url:
            value = {'sha': 'a' * 40, 'truncated': False, 'tree': self.tree}
        elif '/git/blobs/' in url:
            oid = url.rsplit('/', 1)[1]; data = self.blobs[oid]
            value = {'sha': oid, 'encoding': 'base64', 'size': len(data),
                     'content': base64.b64encode(data if not self.corrupt else b'wrong').decode()}
        else:
            raise AssertionError('unexpected endpoint')
        response = Response(value, date=not self.missing_date)
        self.responses.append(response)
        return response


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = {'GITHUB_REPOSITORY': 'nil-studio/oasis7', 'GITHUB_SHA': '1' * 40,
                    'GITHUB_RUN_ID': '42', 'GITHUB_RUN_ATTEMPT': '1',
                    'GITHUB_EVENT_NAME': 'workflow_dispatch', 'GITHUB_REF': 'refs/heads/main',
                    'WASM_TRUSTED_REF': 'refs/heads/main', 'WASM_EXECUTE': 'true',
                    'GH_TOKEN': 'fixture-private-token', 'RUNNER_TEMP': str(self.root)}
        self.console = io.StringIO(); self.handoffs = []

    def handoff(self, session, clock, identity, api, env):
        self.handoffs.append((session, clock, identity))
        self.assertEqual(set(identity['files']), {*b.HELPERS, 'workflow.yml'})
        for name, item in identity['files'].items():
            self.assertEqual(hashlib.sha256((session / 'tools' / name).read_bytes()).hexdigest(), item['sha256'])
        self.assertLess(clock['prepare_end_monotonic'], clock['deadline_monotonic'])
        self.assertEqual(clock['deadline_monotonic'] - clock['prepare_end_monotonic'], 420)
        return 0

    def invoke(self, fixture, handoff=None):
        with redirect_stdout(self.console):
            return b.bootstrap(self.env, opener=fixture, handoff=handoff or self.handoff)

    def public_bytes(self):
        out = self.root / 'rust-wasm-metadata/evidence'
        return self.console.getvalue().encode() + b''.join(p.read_bytes() for p in out.glob('*') if p.is_file())

    def test_verified_fixed_blobs_reach_handoff_with_original_clock(self):
        fixture = RepositoryFixture(self.env['GITHUB_SHA'])
        self.assertEqual(self.invoke(fixture), 0)
        self.assertEqual(len(self.handoffs), 1)
        self.assertEqual(len(fixture.requests), 9)
        self.assertTrue(all(x.closed for x in fixture.responses))
        self.assertNotIn(b'fixture-private-token', self.public_bytes())
        self.assertNotIn(b'Authorization', self.public_bytes())
        self.assertTrue(all(url.startswith('https://api.github.com/repos/nil-studio/oasis7/') for url, _ in fixture.requests))
        terminal = json.loads((self.root / 'rust-wasm-metadata/evidence/bootstrap-terminal.json').read_text())
        self.assertEqual(terminal['status'], 'BOOTSTRAP_VERIFIED')

    def test_corrupt_blob_never_reaches_handoff_or_file_persistence(self):
        fixture = RepositoryFixture(self.env['GITHUB_SHA'], corrupt=True)
        self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(self.handoffs)
        self.assertFalse(list((self.root / 'rust-wasm-metadata/tools').iterdir()))

    def test_missing_real_clock_never_falls_back(self):
        fixture = RepositoryFixture(self.env['GITHUB_SHA'], missing_date=True)
        self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(self.handoffs)
        self.assertEqual(len(fixture.requests), 1)
        self.assertFalse((self.root / 'rust-wasm-metadata/evidence/job-clock.json').exists())

    def test_network_failure_raw_url_token_and_exception_text_never_written(self):
        marker = 'private-fixture-argument-and-token'
        fixture = RepositoryFixture(self.env['GITHUB_SHA'], failure=OSError('https://bad/?token=' + marker))
        writes = []
        original = b.save_bootstrap
        def inspected(out, name, value):
            writes.append(json.dumps(value))
            self.assertNotIn(marker, writes[-1])
            return original(out, name, value)
        with patch.object(b, 'save_bootstrap', inspected):
            self.assertEqual(self.invoke(fixture), 1)
        self.assertNotIn(marker.encode(), self.public_bytes())
        self.assertTrue(writes)
        self.assertFalse(self.handoffs)

    def test_handoff_exception_is_projected_before_first_persistence(self):
        marker = 'private-handoff-argv'
        def fail(*args):
            raise RuntimeError(marker)
        self.assertEqual(self.invoke(RepositoryFixture(self.env['GITHUB_SHA']), handoff=fail), 1)
        self.assertNotIn(marker.encode(), self.public_bytes())
        terminal = json.loads((self.root / 'rust-wasm-metadata/evidence/bootstrap-terminal.json').read_text())
        self.assertEqual(terminal['error_class'], 'RuntimeError')

    def test_expired_original_prepare_clock_rejects_before_helper_read(self):
        fixture = RepositoryFixture(self.env['GITHUB_SHA'])
        original = b.bind_clock
        def expired(api):
            result = original(api); api.end = time.monotonic() - 1
            return result
        with patch.object(b, 'bind_clock', expired):
            self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(self.handoffs)
        self.assertEqual(len(fixture.requests), 1)

    def test_false_and_untrusted_ref_cannot_fetch(self):
        fixture = RepositoryFixture(self.env['GITHUB_SHA'])
        self.env['WASM_EXECUTE'] = 'false'
        self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(fixture.requests)
        self.env['WASM_EXECUTE'] = 'true'; self.env['GITHUB_REF'] = 'refs/heads/untrusted'
        self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(fixture.requests)

    def test_cancelled_network_stops_without_raw_error(self):
        fixture = RepositoryFixture(self.env['GITHUB_SHA'], failure=KeyboardInterrupt('private-cancel-token'))
        self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(self.handoffs)
        self.assertNotIn(b'private-cancel-token', self.public_bytes())

    def test_unknown_endpoint_and_redirect_cannot_become_fetch_permission(self):
        api = b.API(self.env, RepositoryFixture(self.env['GITHUB_SHA']))
        with self.assertRaises(b.Rejected):
            api.get('unknown', '2' * 40)
        with self.assertRaises(b.Rejected):
            api.get('tool-blob', '2' * 40)
        self.assertIsNone(b.NoRedirect().redirect_request(None, None, 302, '', {}, 'https://evil.example'))


    def test_clock_for_different_head_is_rejected_without_fallback(self):
        fixture = RepositoryFixture('2' * 40)
        self.assertEqual(self.invoke(fixture), 1)
        self.assertFalse(self.handoffs); self.assertEqual(len(fixture.requests), 1)

    def test_fixed_archive_redirect_closes_response_and_never_forwards_token(self):
        marker = 'PRIVATE-SIGNED-URL'
        location = f'https://codeload.github.com/{b.REPOSITORY}/legacy.tar.gz/{b.SOURCE}?token={marker}'
        raw = io.BytesIO(b'PRIVATE-HTTP-ERROR-BODY'); requests = []
        class Opener:
            def open(self, req, timeout):
                requests.append(req)
                if len(requests) == 1:
                    raise b.urllib.error.HTTPError(req.full_url, 302, marker, {'Location': location}, raw)
                response = Response({}); response.body = io.BytesIO(b'archive-fixture')
                return response
        api = b.API(self.env, Opener())
        self.assertEqual(api.archive(), b'archive-fixture')
        self.assertTrue(raw.closed)
        self.assertTrue(requests[0].get_header('Authorization'))
        self.assertIsNone(requests[1].get_header('Authorization'))
        self.assertNotIn(marker, json.dumps(api.receipts)); self.assertNotIn('PRIVATE-HTTP-ERROR-BODY', json.dumps(api.receipts))


d = load_driver()


def clock_receipt(seconds=1100):
    mono, wall = time.monotonic(), time.time()
    return {'deadline_monotonic': mono + seconds, 'deadline_wall': wall + seconds,
            'prepare_end_monotonic': mono + seconds - 420, 'prepare_end_wall': wall + seconds - 420,
            'maximum_job_seconds': 1200, 'tool_sha': '1' * 40}


class FixtureGraph:
    """Synthetic public graph; no real Cargo/registry is consulted."""
    def __init__(self, root, create=True):
        self.source, self.cache, self.target = root/'source', root/'cargo-home', root/'target'
        self.names = [*d.LOCAL_PACKAGES, *('member_%02d' % i for i in range(47))]
        self.members = ['crates/' + name for name in self.names]
        self.files = {'Cargo.toml': ('[workspace]\nresolver="2"\nmembers=' + json.dumps(self.members) + '\n').encode(),
                      'Cargo.lock': b'version=4\n'}
        packages = []; nodes = []
        self.ids = {}
        for name in self.names:
            manifest = self.source/'crates'/name/'Cargo.toml'
            pid = 'path+' + manifest.parent.as_uri() + '#0.1.0'; self.ids[name] = pid
            self.files['crates/' + name + '/Cargo.toml'] = ('[package]\nname="' + name + '"\nversion="0.1.0"\n').encode()
            packages.append({'id': pid, 'name': name, 'version': '0.1.0', 'source': None,
                             'features': {k: [] for k in ('default', 'recovery-fence', 'owner', 'identity', 'executor', 'approval-harness', 'native-runtime-harness')} if name == d.LOCAL_PACKAGES[0] else {},
                             'dependencies': [], 'targets': [], 'manifest_path': str(manifest),
                             'description': 'PRIVATE-FREE-TEXT-FIXTURE', 'metadata': {'value': 'PRIVATE-FREE-TEXT-FIXTURE'}})
            packages[-1]['hints'] = {'value': 'PRIVATE-FREE-TEXT-FIXTURE'}
            nodes.append({'id': pid, 'features': ['recovery-fence'] if name == d.LOCAL_PACKAGES[0] else [],
                          'dependencies': [], 'deps': []})
        self.document = {'version': 1, 'packages': packages, 'workspace_members': list(self.ids.values()),
                         'workspace_default_members': list(self.ids.values()), 'workspace_root': str(self.source),
                         'target_directory': str(self.target), 'build_directory': str(self.target),
                         'metadata': {'private': 'PRIVATE-FREE-TEXT-FIXTURE'}, 'resolve': {'root': None, 'nodes': nodes}}
        for index, name in enumerate(d.LOCAL_PACKAGES[1:]):
            self.edge(d.LOCAL_PACKAGES[0], name, kind=(None, 'dev', 'build')[index % 3], target='cfg(target_arch = "wasm32")')
        if create:
            self.cache.mkdir(parents=True); self.target.mkdir(parents=True)
            for relative, data in self.files.items():
                path = self.source/relative; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)

    def package(self, name):
        return next(p for p in self.document['packages'] if p['name'] == name)

    def node(self, name):
        return next(n for n in self.document['resolve']['nodes'] if n['id'] == self.ids[name])

    def edge(self, parent, child, kind=None, target=None, default=False):
        self.package(parent)['dependencies'].append({'name': child, 'source': None, 'req': '*', 'kind': kind,
            'rename': None, 'optional': False, 'uses_default_features': default, 'features': [], 'target': target,
            'registry': None, 'path': str(self.source/'crates'/child)})
        self.node(parent)['dependencies'].append(self.ids[child])
        self.node(parent)['deps'].append({'name': child, 'pkg': self.ids[child], 'dep_kinds': [{'kind': kind, 'target': target}]})

    def evaluate(self):
        return d.metadata_graph(json.dumps(self.document).encode(), self.source, self.cache, self.target, self.members, set())


class MetadataTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.graph = FixtureGraph(self.root)

    def test_complete_53_member_graph_and_six_actual_feature_contexts(self):
        graph, observation = self.graph.evaluate()
        self.assertEqual(len(graph['workspace_members']), 53)
        self.assertEqual(len(graph['resolve']['nodes']), 53)
        self.assertIsNone(graph['resolve']['root'])
        self.assertEqual(set(observation['local_packages']), set(d.LOCAL_PACKAGES))
        self.assertEqual(len(observation['custody_reachable_nodes']), 6)
        kinds = {k['kind'] for e in self.graph.node(d.LOCAL_PACKAGES[0])['deps'] for k in e['dep_kinds']}
        self.assertEqual(kinds, {None, 'build', 'dev'})
        self.assertEqual(observation['codeql'], 'NOT_RUN')
        self.assertFalse(observation['single_crate_isolation'])

    def test_free_text_is_removed_before_any_evidence_write(self):
        graph, observation = self.graph.evaluate()
        out = self.root/'evidence'; out.mkdir(); evidence = d.Evidence(out)
        evidence.save('metadata-graph.json', graph); evidence.save('role-observation.json', observation)
        for path in out.iterdir(): self.assertNotIn(b'PRIVATE-FREE-TEXT-FIXTURE', path.read_bytes())

    def test_workspace_member_missing_duplicate_and_extra_are_rejected(self):
        baseline = copy.deepcopy(self.graph.document)
        for change in ('missing', 'duplicate', 'external'):
            with self.subTest(change=change):
                self.graph.document = copy.deepcopy(baseline)
                values = self.graph.document['workspace_members']
                if change == 'missing': values.pop()
                elif change == 'duplicate': values.append(values[0])
                else: values.append('registry+https://example.invalid#x@1')
                with self.assertRaises(d.Rejected): self.graph.evaluate()

    def test_extra_role_harness_and_missing_role_rejected(self):
        for values in ([], ['owner'], ['recovery-fence', 'identity'], ['recovery-fence', 'approval-harness'], ['recovery-fence', 'native-runtime-harness']):
            with self.subTest(features=values):
                self.graph.node(d.LOCAL_PACKAGES[0])['features'] = values
                _, observation = self.graph.evaluate()
                self.assertEqual(observation['status'], 'ROLE_CONTEXT_MISMATCH')
                self.assertEqual(observation['actual_custody_features'], sorted(values))

    def test_merged_declaration_duplicate_features_are_preserved_but_resolved_duplicates_rejected(self):
        dep = self.graph.package(d.LOCAL_PACKAGES[0])['dependencies'][0]
        dep['features'] = ['derive', 'derive', 'rc']
        graph, _ = self.graph.evaluate()
        self.assertEqual(graph['packages'][0]['dependencies'][0]['features'], ['derive', 'derive', 'rc'])
        self.graph.node(d.LOCAL_PACKAGES[0])['features'] = ['recovery-fence', 'recovery-fence']
        with self.assertRaises(d.Rejected): self.graph.evaluate()

    def test_default_requires_matching_actual_resolved_edge(self):
        self.graph.node(d.LOCAL_PACKAGES[0])['features'].append('default')
        _, observation = self.graph.evaluate()
        self.assertFalse(observation['checks']['default_marker_explained'])
        self.graph.edge('member_00', d.LOCAL_PACKAGES[0], kind='dev', default=True)
        _, observation = self.graph.evaluate()
        self.assertEqual(len(observation['default_marker_sources']), 1)
        self.graph.node('member_00')['deps'][0]['dep_kinds'][0]['kind'] = 'build'
        _, observation = self.graph.evaluate()
        self.assertEqual(observation['status'], 'ROLE_CONTEXT_MISMATCH')

    def test_target_escape_credentials_forged_identity_and_nonvirtual_root_rejected(self):
        baseline = copy.deepcopy(self.graph.document)
        for change in ('target', 'credentials', 'id', 'root', 'bool-version'):
            with self.subTest(change=change):
                self.graph.document = copy.deepcopy(baseline)
                if change == 'target': self.graph.document['target_directory'] = '/outside'
                elif change == 'credentials': self.graph.document['packages'][0]['source'] = 'git+https://token@example.invalid/repo'
                elif change == 'id': self.graph.document['packages'][0]['id'] += '?token=private'
                elif change == 'root': self.graph.document['resolve']['root'] = self.graph.ids[d.LOCAL_PACKAGES[0]]
                else: self.graph.document['version'] = True
                with self.assertRaises(d.Rejected): self.graph.evaluate()

    def test_invalid_json_duplicate_keys_and_missing_local_reachability_rejected(self):
        for data in (b'{', b'{"version":1,"version":1}', b'{"v":NaN}'):
            with self.subTest(data=data), self.assertRaises((d.Rejected, json.JSONDecodeError)):
                d.metadata_graph(data, self.graph.source, self.graph.cache, self.graph.target, self.graph.members, set())
        self.graph.node(d.LOCAL_PACKAGES[0])['dependencies'].pop()
        self.graph.node(d.LOCAL_PACKAGES[0])['deps'].pop()
        _, observation = self.graph.evaluate()
        self.assertFalse(observation['checks']['six_local_packages_reachable'])


class ClockEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.evidence = d.Evidence(self.root)

    def test_absolute_budget_has_no_phase_reset_or_late_admission(self):
        receipt = clock_receipt(500); clock = d.Clock(receipt)
        start = time.monotonic(); metadata_end = clock.admit_metadata()
        self.assertLessEqual(metadata_end, start + 300.01)
        self.assertLessEqual(metadata_end, clock.end - 120)
        with patch.object(d.time, 'monotonic', return_value=receipt['deadline_monotonic'] - 419):
            with self.assertRaises(d.Rejected): clock.admit_metadata()
        invalid = dict(receipt, prepare_end_monotonic=receipt['prepare_end_monotonic'] + 1)
        with self.assertRaises(d.Rejected): d.Clock(invalid)

    def test_forward_wall_clock_and_missing_real_deadline_fail_closed(self):
        clock = d.Clock(clock_receipt())
        with patch.object(d.time, 'time', return_value=clock.wall + 1):
            with self.assertRaises(d.Rejected): clock.check()
        with self.assertRaises((d.Rejected, KeyError)): d.Clock({})

    def test_ordinary_cap_and_audit_reservation_are_enforced_before_write(self):
        with patch.multiple(d, ORDINARY=2000, PROCESS_RESERVE=1000, AUDIT=100, TERMINAL=200, CAP=2300):
            with self.assertRaises(d.Rejected): self.evidence.save('ordinary.json', {'value': 'x' * 1000})
            self.assertFalse((self.root/'ordinary.json').exists())
            self.evidence.save('ordinary.json', {'value': 'x' * 800})
            self.evidence.save('metadata.result.json', {'phase': 'metadata'})
            (self.root/'cargo-guard-calls').mkdir(); (self.root/'cargo-guard-calls/a.json').write_bytes(b'x' * 101)
            with self.assertRaises(d.Rejected): self.evidence.save('new.json', {})
            self.assertFalse((self.root/'new.json').exists())

    def test_final_footprint_checked_and_closed_manifest_is_immutable(self):
        with patch.object(d, 'TERMINAL', 1024):
            with self.assertRaises(d.Rejected): self.evidence.admit_final({'files': {}}, {'files': {}})
        self.evidence.save('test.json', {'okay': True})
        clock = d.Clock(clock_receipt()); self.evidence.seal(clock, clock.prepare)
        manifest = json.loads((self.root/'sha256.json').read_bytes())['files']
        self.assertEqual(set(manifest), {'test.json'})
        self.assertEqual(manifest['test.json']['sha256'], hashlib.sha256((self.root/'test.json').read_bytes()).hexdigest())
        with self.assertRaises(d.Rejected): self.evidence.save('test.json', {})

    def test_original_helpers_and_shared_runtime_remain_read_only(self):
        script = Path(d.__file__).read_text()
        self.assertNotIn('runtime.run(', script)
        self.assertNotIn("os.environ['HOME']", script)
        self.assertNotIn("os.environ['CODEX_HOME']", script)
        self.assertEqual(set(d.PHASES), {'toolchain-install', 'cargo-version', 'rustc-version', 'dependency-fetch', 'cargo-version-after', 'rustc-version-after', 'discover', 'metadata'})
        self.assertEqual(d.AUDIT, d.guard.MAX_AUDIT)

    def test_real_rustc_verbose_shape_accepts_only_fixed_binary_and_known_fields(self):
        # Shape is from saved real rustc receipts; the commit is intentionally synthetic.
        raw = b'rustc 1.99.0 (aaaaaaaaa 2026-10-01)\nbinary: rustc\ncommit-hash: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\ncommit-date: 2026-10-01\nhost: x86_64-unknown-linux-gnu\nrelease: 1.99.0\nLLVM version: 22.1.8\n'
        self.assertEqual(d.parse_version(raw, 'rustc')['binary'], 'rustc')
        for bad in (raw.replace(b'binary: rustc', b'binary: cargo'), raw + b'unknown: PRIVATE-VERSION\n', raw.replace(b'binary: rustc\n', b''), raw.replace(b'(aaaaaaaaa ', b'(bbbbbbbbb '), raw.replace(b'(aaaaaaaaa 2026-10-01)', b'(aaaaaaaaa 2026-10-02)')):
            with self.subTest(raw_sha=hashlib.sha256(bad).hexdigest()), self.assertRaises(d.Rejected):
                d.parse_version(bad, 'rustc')

    def test_exact_cargo_and_first_observed_rustc_are_distinct(self):
        def version(kind, commit):
            return (f'{kind} 1.99.0 ({commit[:9]} 2026-10-01)\n' + ('binary: rustc\n' if kind == 'rustc' else '') + f'release: 1.99.0\ncommit-hash: {commit}\ncommit-date: 2026-10-01\nhost: {d.HOST}\n').encode()
        self.assertEqual(d.parse_version(version('cargo', d.CARGO_COMMIT), 'cargo')['commit-hash'], d.CARGO_COMMIT)
        self.assertEqual(d.parse_version(version('rustc', 'a' * 40), 'rustc')['history_scope'], 'EXACT_RUSTC_COMMIT_FIRST_OBSERVED_THIS_RUN')
        with self.assertRaises(d.Rejected): d.parse_version(version('cargo', 'b' * 40), 'cargo')

def tree_fixture(files):
    directories = {''}
    for path in files:
        folder = Path(path).parent
        while str(folder) != '.':
            directories.add(folder.as_posix()); folder = folder.parent
    records = {path: {'path': path, 'type': 'blob', 'mode': '100644', 'sha': b.blob_id(data), 'size': len(data)}
               for path, data in files.items()}
    root_oid = None
    for folder in sorted(directories, key=lambda s: (-s.count('/'), -len(s))):
        children = [r for p, r in records.items() if (str(Path(p).parent) if str(Path(p).parent) != '.' else '') == folder]
        payload = b''.join(r['mode'].lstrip('0').encode() + b' ' + Path(r['path']).name.encode() + b'\0' + bytes.fromhex(r['sha'])
                           for r in sorted(children, key=lambda r: (Path(r['path']).name + ('/' if r['type'] == 'tree' else '')).encode()))
        oid = hashlib.sha1(b'tree ' + str(len(payload)).encode() + b'\0' + payload).hexdigest()
        if folder: records[folder] = {'path': folder, 'type': 'tree', 'mode': '040000', 'sha': oid}
        else: root_oid = oid
    return {'sha': root_oid, 'truncated': False, 'tree': list(records.values())}


def archive_fixture(files, bad_path=None):
    target = io.BytesIO()
    with tarfile.open(fileobj=target, mode='w:gz') as archive:
        directory = tarfile.TarInfo('fixture'); directory.type = tarfile.DIRTYPE; archive.addfile(directory)
        for path, data in files.items():
            info = tarfile.TarInfo('fixture/' + (bad_path or path)); info.mode = 0o644; info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return target.getvalue()


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.clock = d.Clock(clock_receipt())
        self.files = {'Cargo.toml': b'[workspace]\n', 'nested/hello.txt': b'hello\n'}
        self.tree = tree_fixture(self.files)

    def test_recomputed_tree_blobs_and_full_file_membership(self):
        self.assertEqual(d.git_blob(b'hello\n'), 'ce013625030ba8dba906f756967f9e9ca394464a')
        rows = d.verify_git_tree(self.tree, self.tree['sha'])
        before = d.unpack_source(archive_fixture(self.files), rows, self.root/'source', self.clock, self.clock.prepare)
        self.assertEqual(set(before['files']), set(self.files))
        self.assertEqual(d.snapshot_source(self.root/'source', rows, self.clock, self.clock.prepare), before)
        (self.root/'source/nested/hello.txt').write_bytes(b'changed')
        with self.assertRaises(d.Rejected): d.snapshot_source(self.root/'source', rows, self.clock, self.clock.prepare)

    def test_changed_tree_and_unverified_blob_cannot_reach_source_files(self):
        changed = copy.deepcopy(self.tree); changed['tree'][0]['sha'] = 'a' * 40
        with self.assertRaises(d.Rejected): d.verify_git_tree(changed, changed['sha'])
        rows = d.verify_git_tree(self.tree, self.tree['sha'])
        archive = archive_fixture({'Cargo.toml': b'wrong'})
        with self.assertRaises(d.Rejected): d.unpack_source(archive, rows, self.root/'source', self.clock, self.clock.prepare)
        self.assertFalse((self.root/'source/Cargo.toml').exists())

    def test_archive_traversal_duplicate_and_incomplete_sets_rejected(self):
        rows = d.verify_git_tree(self.tree, self.tree['sha'])
        with self.assertRaises(d.Rejected):
            d.unpack_source(archive_fixture(self.files, '../../outside'), rows, self.root/'source', self.clock, self.clock.prepare)
        self.assertFalse((self.root/'outside').exists())
        with self.assertRaises(d.Rejected):
            d.unpack_source(archive_fixture({'Cargo.toml': self.files['Cargo.toml']}), rows, self.root/'incomplete', self.clock, self.clock.prepare)


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); (self.root/'evidence').mkdir()
        self.evidence = d.Evidence(self.root/'evidence'); self.clock = d.Clock(clock_receipt())
        self.env = {'PATH': os.environ.get('PATH', ''), 'HOME': os.environ.get('HOME', '/tmp'), 'PYTHONDONTWRITEBYTECODE': '1'}
        d.set_subreaper()

    def run_python(self, source, *, capture=False, timeout=10):
        return d.run_owned('dependency-fetch', [sys.executable, '-c', source], self.root, self.env,
                           self.clock, self.clock.prepare, self.evidence, {'cwd': str(self.root)}, capture=capture, timeout=timeout)

    def receipt(self):
        return json.loads((self.root/'evidence/dependency-fetch.result.json').read_bytes())

    def assert_group_gone(self, pgid):
        with self.assertRaises(ProcessLookupError): os.killpg(pgid, 0)

    def test_raw_streams_and_argv_never_enter_first_persistence(self):
        marker = 'PRIVATE-ARGV-STDOUT-STDERR'
        writes = []; original = self.evidence.save
        def inspect(name, value, **kwargs):
            writes.append(json.dumps(value)); self.assertNotIn(marker, writes[-1])
            return original(name, value, **kwargs)
        with patch.object(self.evidence, 'save', inspect):
            raw, result = self.run_python(f'import sys; print({marker!r}); print({marker!r}, file=sys.stderr)', capture=True)
        self.assertIn(marker.encode(), raw)  # in-memory only; not a validated JSON artifact
        self.assertNotIn(marker.encode(), b''.join(p.read_bytes() for p in (self.root/'evidence').iterdir()))
        self.assertEqual(result['exit'], 0)
        self.assertEqual(result['streams']['stdout']['observation'], 'EOF_COMPLETE')
        self.assert_group_gone(result['pgid'])

    def test_overflow_is_fail_closed_and_preserves_only_prefix_digests(self):
        with patch.object(d, 'MAX_STDOUT', 16):
            with self.assertRaises(d.CommandFailed): self.run_python('print("PRIVATE" * 10000)', capture=True)
        receipt = self.receipt(); self.assertEqual(receipt['failure_kind'], 'output_limit')
        self.assertNotIn('PRIVATE', json.dumps(receipt)); self.assert_group_gone(receipt['pgid'])

    def test_nonzero_and_launch_error_preserve_no_exception_text(self):
        with self.assertRaises(d.CommandFailed): self.run_python('import sys; sys.stderr.write("PRIVATE-ERROR"); sys.exit(7)')
        self.assertEqual(self.receipt()['exit'], 7)
        self.assertNotIn('PRIVATE-ERROR', json.dumps(self.receipt()))
        with self.assertRaises(d.CommandFailed):
            d.run_owned('discover', ['/private-missing-command'], self.root, self.env, self.clock,
                        self.clock.prepare, self.evidence, {'cwd': str(self.root)}, timeout=10)
        value = (self.root/'evidence/discover.result.json').read_text()
        self.assertIn('FileNotFoundError', value); self.assertNotIn('/private-missing-command', value)

    def test_timeout_terminates_and_reaps_ignoring_grandchild(self):
        source = ('import os,signal,time\n'
                  'child=os.fork()\n'
                  'signal.signal(signal.SIGTERM,signal.SIG_IGN)\n'
                  'time.sleep(60)\n')
        started = time.monotonic()
        with self.assertRaises(d.CommandFailed): self.run_python(source, timeout=4.3)
        receipt = self.receipt(); self.assertEqual(receipt['failure_kind'], 'timeout')
        self.assertLess(time.monotonic() - started, 4.3)
        self.assertEqual(receipt['cleanup_errors'], []); self.assert_group_gone(receipt['pgid'])

    def test_monitor_cancellation_cleans_before_projected_receipt(self):
        with patch.object(d.runtime.GroupSamples, 'sample', side_effect=d.Stopped()):
            with self.assertRaises(d.CommandFailed): self.run_python('import time; time.sleep(60)')
        receipt = self.receipt(); self.assertEqual(receipt['failure_kind'], 'cancelled')
        self.assert_group_gone(receipt['pgid'])

    def test_receipt_write_failure_still_cleans_owned_process(self):
        procs = []; original = subprocess.Popen
        def spawn(*args, **kwargs):
            proc = original(*args, **kwargs); procs.append(proc); return proc
        with patch.object(d.subprocess, 'Popen', spawn), patch.object(self.evidence, 'save', side_effect=OSError('PRIVATE-WRITE-ERROR')):
            with self.assertRaises(OSError): self.run_python('print("not persisted")')
        self.assertEqual(len(procs), 1); self.assert_group_gone(procs[0].pid)
        self.assertEqual(list((self.root/'evidence').iterdir()), [])

    def test_real_term_during_spawn_is_deferred_until_owned_identity_exists(self):
        old = signal.getsignal(signal.SIGTERM); signal.signal(signal.SIGTERM, d.interruption)
        original = subprocess.Popen
        def spawn(*args, **kwargs):
            proc = original(*args, **kwargs)
            os.kill(os.getpid(), signal.SIGTERM)
            return proc
        try:
            with patch.object(d.subprocess, 'Popen', spawn), self.assertRaises(d.CommandFailed):
                self.run_python('import time; time.sleep(60)')
            receipt = self.receipt(); self.assertEqual(receipt['failure_kind'], 'cancelled')
            self.assert_group_gone(receipt['pgid'])
        finally:
            signal.signal(signal.SIGTERM, old)

    def test_second_real_term_cannot_interrupt_cleanup_or_first_receipt_write(self):
        old = signal.getsignal(signal.SIGTERM); signal.signal(signal.SIGTERM, d.interruption)
        timers = [threading.Timer(delay, lambda: os.kill(os.getpid(), signal.SIGTERM)) for delay in (.15, .3)]
        try:
            for timer in timers: timer.start()
            with self.assertRaises((d.CommandFailed, d.Stopped)):
                self.run_python('import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(60)')
            receipt = self.receipt(); self.assertEqual(receipt['failure_kind'], 'cancelled')
            self.assertEqual(receipt['cleanup_errors'], []); self.assert_group_gone(receipt['pgid'])
        finally:
            for timer in timers: timer.cancel(); timer.join()
            signal.signal(signal.SIGTERM, old)



class GuardVersionFirstWriteTests(unittest.TestCase):
    """Observe original guard reserve/finish arguments BEFORE their file writes."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.work = self.root/'work'; self.out = self.root/'evidence'
        self.work.mkdir(); self.out.mkdir(); self.bin = self.root/'bin'; self.bin.mkdir()
        self.cache = self.root/'cache'; self.cache.mkdir(); self.source = self.work/'source'; self.source.mkdir()
        (self.source/'Cargo.toml').write_text('[workspace]\n')
        self.clock = d.Clock(clock_receipt()); self.end = time.monotonic() + 30; self.wall = time.time() + 30
        self.evidence = d.Evidence(self.out); self.home = None
        self.env = dict(os.environ, PATH=str(self.bin)+os.pathsep+os.environ.get('PATH',''), CARGO_HOME=str(self.cache), RUSTUP_TOOLCHAIN='1.99.0')
        self.valid = f'cargo 1.99.0 ({d.CARGO_COMMIT[:9]} 2026-10-01)\nrelease: 1.99.0\ncommit-hash: {d.CARGO_COMMIT}\ncommit-date: 2026-10-01\nhost: {d.HOST}\n'.encode()
        self.marker = b'PRIVATE-GUARD-VERSION-CANARY'
        self.set_proxy(self.valid)
        self.args = ['metadata','--locked','--offline','--format-version=1','--manifest-path',str(self.source/'Cargo.toml')]
        data = Path(d.__file__).read_bytes()
        self.identity = {'files':{'codeql-rust-wasm-metadata.py':{'bytes':len(data),'git_blob':d.git_blob(data),'sha256':d.sha256(data)}}}
        saved = {sig:signal.getsignal(sig) for sig in (signal.SIGTERM,signal.SIGINT)}
        self.addCleanup(lambda:[signal.signal(sig,handler) for sig,handler in saved.items()])

    def set_proxy(self, output, code=0):
        script = '#!'+sys.executable+'\nimport os,sys\nif sys.argv[1:]==["-Vv"]:\n os.write(1,'+repr(output)+')\n sys.exit('+str(code)+')\nsys.exit(0)\n'
        (self.bin/'cargo').write_text(script); (self.bin/'cargo').chmod(0o700)

    def install(self):
        with patch.dict(os.environ,self.env,clear=True):
            self.home=d.guard.install(self.work,self.out,self.wall,d.SOURCE,[self.source/'Cargo.toml'],monotonic_deadline=self.end)

    def bind(self):
        if self.home is None:self.install()
        d.install_guard_adapter(self.home,self.evidence,self.identity,self.clock,self.end)

    def invoke_spied(self):
        self.bind(); writes=[]; reserve=d.guard.reserve; finish=d.guard.finish_record
        def before_reserve(config,record):
            self.assertNotIn(self.marker.decode(),json.dumps(record));writes.append(dict(record))
            return reserve(config,record)
        def before_finish(path,record):
            self.assertNotIn(self.marker.decode(),json.dumps(record));writes.append(dict(record))
            return finish(path,record)
        with patch.dict(os.environ,self.env,clear=True), patch.object(d.guard,'reserve',before_reserve), patch.object(d.guard,'finish_record',before_finish), redirect_stdout(io.StringIO()):
            code=d.invoke_guard_adapter(self.args,self.out/'cargo-guard.json')
        self.assertEqual(len(writes),2);self.assertEqual(writes[0]['status'],'PENDING')
        final=writes[1];self.assertEqual(final['argv_sha256'],hashlib.sha256(json.dumps(self.args).encode()).hexdigest())
        for path in self.out.rglob('*'):
            if path.is_file():self.assertNotIn(self.marker,path.read_bytes())
        return code,final

    def test_known_auxiliary_text_is_projected_before_first_guard_write(self):
        output=self.valid+b'os: '+self.marker+b'\n';self.set_proxy(output)
        code,record=self.invoke_spied();self.assertEqual(code,0);self.assertEqual(record['status'],'PASS')
        self.assertEqual(record['wasm_version_projection'],{'status':'VALIDATED_PROJECTED','bytes':len(output),'sha256':hashlib.sha256(output).hexdigest()})
        self.assertNotIn('os:',record['actual_cargo_version'])
        self.assertEqual(d.guard.inspect(self.out,self.wall,d.SOURCE,end=self.end)['status'],'PASS')

    def test_unknown_field_is_rejected_before_version_assignment_and_first_write(self):
        output=self.valid+b'unknown: '+self.marker+b'\n';self.set_proxy(output)
        code,record=self.invoke_spied();self.assertNotEqual(code,0);self.assertEqual(record['status'],'FAIL')
        self.assertNotIn('actual_cargo_version',record);self.assertEqual(record['error'],'Rejected')
        self.assertEqual(record['wasm_version_projection'],{'status':'REJECTED_BEFORE_WRITE','bytes':len(output),'sha256':hashlib.sha256(output).hexdigest()})

    def test_wrong_exact_commit_is_rejected_without_a_cached_version(self):
        self.set_proxy(self.valid.replace(d.CARGO_COMMIT.encode(),b'b'*40).replace(d.CARGO_COMMIT[:9].encode(),b'b'*9))
        code,record=self.invoke_spied();self.assertNotEqual(code,0);self.assertNotIn('actual_cargo_version',record)
        self.assertEqual(record['wasm_version_projection']['status'],'REJECTED_BEFORE_WRITE')

    def test_original_overflow_does_not_invent_complete_bytes_or_digest(self):
        self.set_proxy(self.marker*200)
        code,record=self.invoke_spied();self.assertNotEqual(code,0);self.assertEqual(record['error'],'RuntimeError')
        self.assertEqual(record['wasm_version_projection'],{'status':'NOT_RETURNED','bytes':None,'sha256':None})

    def test_original_nonzero_does_not_invent_complete_bytes_or_digest(self):
        self.set_proxy(self.valid+self.marker,7)
        code,record=self.invoke_spied();self.assertNotEqual(code,0);self.assertEqual(record['error'],'RuntimeError')
        self.assertEqual(record['wasm_version_projection'],{'status':'NOT_RETURNED','bytes':None,'sha256':None})

    def test_binding_is_once_and_only_before_any_call(self):
        self.bind();before=(self.out/'cargo-guard.json').read_bytes()
        with self.assertRaises(d.Rejected):self.bind()
        self.assertEqual((self.out/'cargo-guard.json').read_bytes(),before)

    def test_nonpristine_counter_prevents_any_rebinding(self):
        self.install();before=(self.home/'bin/cargo').read_bytes()
        (self.out/'cargo-guard-calls/.counter').write_bytes(b'0000000000000001')
        with self.assertRaises(d.Rejected):self.bind()
        self.assertEqual((self.home/'bin/cargo').read_bytes(),before)

    def test_wrapper_rejects_changed_adapter_bytes_before_import_or_cargo(self):
        adapter=self.root/'adapter.py';adapter.write_bytes(Path(d.__file__).read_bytes())
        self.install()
        with patch.object(d,'__file__',str(adapter)):self.bind()
        adapter.write_bytes(adapter.read_bytes()+b'\nraise RuntimeError("PRIVATE-GUARD-VERSION-CANARY")\n')
        result=subprocess.run([str(self.home/'bin/cargo'),*self.args],env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=5)
        self.assertEqual(result.returncode,101);self.assertNotIn(self.marker,result.stdout+result.stderr)
        self.assertEqual((self.out/'cargo-guard-calls/.counter').read_bytes(),b'0000000000000000')


class CompleteFixtureTests(unittest.TestCase):
    """Real Python processes + unchanged Cargo guard, never the real Cargo program."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.session = self.root/'session'; self.session.mkdir()
        (self.session/'evidence').mkdir(); tools = self.session/'tools'; tools.mkdir()
        self.identity = {'tool_sha': '1' * 40, 'files': {}}
        for filename in d.HELPERS:
            origin = WORKFLOW if filename == 'workflow.yml' else Path(d.__file__).with_name(filename)
            data = origin.read_bytes(); (tools/filename).write_bytes(data)
            self.identity['files'][filename] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'git_blob': b.blob_id(data)}
        self.fixture = FixtureGraph(self.session/'work', create=False)
        self.fixture.files['Cargo.lock'] = b'version=4\npackage=[]\n'
        self.fixture.files['rust-toolchain.toml'] = b'[toolchain]\nchannel="1.96.0"\n'
        self.tree = tree_fixture(self.fixture.files); self.archive = archive_fixture(self.fixture.files)
        self.bin = self.root/'bin'; self.bin.mkdir(); self.settings = self.root/'settings.json'; self.events = self.root/'events.jsonl'
        self.settings.write_text(json.dumps({'graph': self.fixture.document, 'fail_fetch': False, 'bad_discovery': False, 'mutate_lock': False}))
        source = '''#!PYTHON
import json,os,pathlib,shutil,sys
settings=json.loads(pathlib.Path(SETTINGS).read_text())
name=pathlib.Path(sys.argv[0]).name
args=sys.argv[1:]
with pathlib.Path(EVENTS).open('a') as out: out.write(json.dumps({'program':name,'operation':args[0] if args else None,'argument_count':len(args)})+'\\n')
if name=='rustup' and args==['toolchain','install','1.99.0','--profile','minimal']:
 root=pathlib.Path(os.environ['RUSTUP_HOME'])/'toolchains/1.99.0-x86_64-unknown-linux-gnu'
 (root/'bin').mkdir(parents=True)
 for kind in ('cargo','rustc'): shutil.copyfile(sys.argv[0],root/'bin'/kind); (root/'bin'/kind).chmod(0o700)
 (root/'lib/rustlib').mkdir(parents=True)
 (root/'lib/rustlib/multirust-channel-manifest.toml').write_text('fixture = true\\n')
 sys.exit(0)
if args==['-Vv']:
 kind='rustc' if name=='rustc' else 'cargo'; commit='a'*40 if kind=='rustc' else '5f94df4789f005f9a352888e8355ffc645b7ed0e'
 print(f'{kind} 1.99.0 ({commit[:9]} 2026-10-01)\\n' + ('binary: rustc\\n' if kind=='rustc' else '') + f'release: 1.99.0\\ncommit-hash: {commit}\\ncommit-date: 2026-10-01\\nhost: x86_64-unknown-linux-gnu')
 sys.exit(0)
if args[0]=='fetch':
 print('PRIVATE-NETWORK-PREP-OUTPUT'); print('PRIVATE-NETWORK-PREP-ERROR',file=sys.stderr)
 sys.exit(9 if settings['fail_fetch'] else 0)
if args[0]=='locate-project':
 print(json.dumps({'root':'/wrong' if settings['bad_discovery'] else settings['graph']['workspace_root']+'/Cargo.toml'})); sys.exit(0)
if args[0]=='metadata':
 print(json.dumps(settings['graph']))
 if settings['mutate_lock']: pathlib.Path(settings['graph']['workspace_root'],'Cargo.lock').write_text('changed')
 sys.exit(0)
sys.exit(13)
'''.replace('PYTHON', sys.executable).replace('SETTINGS', repr(str(self.settings))).replace('EVENTS', repr(str(self.events)))
        for name in ('cargo', 'rustup'):
            path = self.bin/name; path.write_text(source); path.chmod(0o700)
        self.env = {'PATH': str(self.bin) + os.pathsep + os.environ.get('PATH', ''), 'HOME': os.environ.get('HOME', '/tmp'),
                    'CODEX_HOME': '/unchanged-fixture-codex-home', 'GITHUB_SHA': '1' * 40}
        self.clock = clock_receipt()
        self.console = io.StringIO()
        saved = {sig: signal.getsignal(sig) for sig in (signal.SIGALRM, signal.SIGINT, signal.SIGTERM)}
        self.addCleanup(lambda: [signal.signal(sig, handler) for sig, handler in saved.items()])
        self.addCleanup(lambda: signal.setitimer(signal.ITIMER_REAL, 0))

    def execute(self):
        tree, archive = self.tree, self.archive
        class FixedAPI:
            receipts = []
            def get(self, kind, oid):
                if kind != 'source-tree' or oid != tree['sha']: raise AssertionError('fixed source request')
                return tree, None
            def archive(self): return archive
        with patch.object(d, 'SOURCE_TREE', tree['sha']), redirect_stdout(self.console):
            return d.diagnose(self.session, self.clock, self.identity, FixedAPI(), self.env)

    def change(self, **values):
        document = json.loads(self.settings.read_bytes()); document.update(values); self.settings.write_text(json.dumps(document))

    def observed(self):
        return [json.loads(line) for line in self.events.read_text().splitlines()]

    def test_closed_fixture_path_uses_original_guard_twice_and_seals_after_identity(self):
        self.assertEqual(self.execute(), 0, self.console.getvalue())
        out = self.session/'evidence'; terminal = json.loads((out/'terminal.json').read_bytes())
        self.assertEqual(terminal['status'], 'ROOT_WORKSPACE_ROLE_METADATA_OBSERVED')
        self.assertEqual(terminal['platform_completion'], 'PENDING_EXTERNAL_READBACK')
        verdict = json.loads((out/'guard-verdict.json').read_bytes())
        self.assertEqual((verdict['calls'], verdict['full_metadata_calls'], verdict['failed_or_pending']), (2, 1, 0))
        self.assertGreaterEqual(terminal['remaining_job_seconds_before_upload'], 120)
        events = self.observed(); self.assertEqual(sum(r['operation']=='metadata' for r in events), 1)
        self.assertEqual(sum(r['operation']=='locate-project' for r in events), 1)
        self.assertEqual(sum(r['operation']=='-Vv' for r in events), 5)
        manifest = json.loads((out/'sha256.json').read_bytes())['files']
        self.assertEqual(set(manifest), {p.relative_to(out).as_posix() for p in out.rglob('*') if p.is_file()}-{'sha256.json'})
        for relative, identity in manifest.items():
            data = (out/relative).read_bytes(); self.assertEqual(hashlib.sha256(data).hexdigest(), identity['sha256'])
            self.assertNotIn(b'PRIVATE-NETWORK', data); self.assertNotIn(b'PRIVATE-FREE-TEXT', data)

    def test_preparation_failure_never_attempts_d1_or_d2(self):
        self.change(fail_fetch=True)
        self.assertEqual(self.execute(), 1)
        self.assertFalse(any(r['operation'] in ('metadata', 'locate-project') for r in self.observed()))
        terminal = json.loads((self.session/'evidence/terminal.json').read_bytes())
        self.assertEqual(terminal['phase'], 'dependency-fetch')
        self.assertEqual(terminal['metadata_observation'], 'NOT_RUN')
        self.assertTrue((self.session/'evidence/sha256.json').is_file())

    def test_failed_discovery_never_attempts_d2(self):
        self.change(bad_discovery=True)
        self.assertEqual(self.execute(), 1)
        self.assertEqual(sum(r['operation']=='locate-project' for r in self.observed()), 1)
        self.assertFalse(any(r['operation']=='metadata' for r in self.observed()))
        self.assertTrue((self.session/'evidence/sha256.json').is_file())

    def test_valid_graph_role_mismatch_is_preserved_without_false_success(self):
        value = json.loads(self.settings.read_bytes())
        value['graph']['resolve']['nodes'][0]['features'].append('approval-harness')
        self.settings.write_text(json.dumps(value))
        self.assertEqual(self.execute(), 1)
        out = self.session/'evidence'
        terminal = json.loads((out/'terminal.json').read_bytes())
        observation = json.loads((out/'role-observation.json').read_bytes())
        self.assertEqual(terminal['metadata_observation'], 'ROLE_CONTEXT_MISMATCH')
        self.assertIn('approval-harness', observation['actual_custody_features'])
        self.assertEqual(terminal['status'], 'METADATA_INCOMPLETE')
        self.assertTrue((out/'metadata-graph.json').is_file()); self.assertTrue((out/'sha256.json').is_file())

    def test_after_source_mutation_cannot_be_reported_as_observed_success(self):
        self.change(mutate_lock=True)
        self.assertEqual(self.execute(), 1)
        terminal = json.loads((self.session/'evidence/terminal.json').read_bytes())
        self.assertEqual(terminal['status'], 'METADATA_INCOMPLETE')
        self.assertEqual(terminal['evidence'], 'INCOMPLETE')


class WorkflowTests(unittest.TestCase):
    def test_default_off_fixed_dispatch_and_least_privilege(self):
        self.assertIn('default: false', TEXT)
        self.assertIn("inputs.execute == true", TEXT)
        self.assertIn('timeout-minutes: 20', TEXT)
        self.assertIn('queue: max', TEXT)
        self.assertIn('cancel-in-progress: false', TEXT)
        self.assertIn('oasis7-codeql-scan-slot-1', TEXT)
        self.assertNotIn('actions/checkout', TEXT)
        self.assertNotIn('security-events:', TEXT)
        self.assertNotIn('schedule:', TEXT)
        self.assertIn('actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9', TEXT)


if __name__ == '__main__':
    unittest.main()
