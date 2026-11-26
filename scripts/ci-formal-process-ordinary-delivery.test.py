#!/usr/bin/env python3
"""OrdinaryArtifactDelivery and CasePartitionArtifactDelivery regressions split from ci-formal-process.test.py."""
import copy
from contextlib import ExitStack
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def module(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), HERE / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


artifacts = module('ci-test-artifacts')


def complete_partition_selection():
    packages = ('oasis7_viewer_codec',)
    return dict(
        fast_plan_version=1,
        execution_profile='pr-fast',
        scope='full',
        groups=['baseline', 'oasis7_required'],
        oasis7_required_packages=list(packages),
        oasis7_required_wasm_packages=[p for p in packages if p in artifacts.PORTABLE_REQUIRED_PACKAGES],
        formal_suites=[],
        test_policy_changed=False,
        architecture=dict(source='source', tested='tested'),
        ordinary_artifact_partition_version=1,
        ordinary_partitions=['controls', 'runtime', 'component-libs', 'integrations-0', 'integrations-1'],
    )


def complete_partition_manifest(selected):
    binaries = {}
    for name, package, kind, variant in [
        ('runtime-tests', 'oasis7_chain_runtime', 'lib', 'required'),
        ('oasis7-lib-tests', 'oasis7', 'lib', 'required'),
        ('component', 'oasis7_runtime', 'lib', 'required'),
        ('new-integration', 'oasis7_runtime', 'test', 'required'),
        ('supplement', 'oasis7', 'lib', 'required-full-fast-0'),
        ('empty', 'oasis7', 'test', 'required')]:
        binaries[name] = dict(package=package, target=package, target_kind=[kind],
                              variant=variant, ordinary=True, tests=[] if name == 'empty' else ['actual_behavior'],
                              ignored_tests=[], path=name, cwd='.')
    for owner, pattern in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS:
        binaries[owner] = dict(package='oasis7', target='oasis7', target_kind=['lib'],
                               variant=owner, ordinary=False, tests=[pattern + 'regression'],
                               ignored_tests=[], path=owner, cwd='.')
    return dict(scope='ordinary', execution_plan=selected, binaries=binaries)


def entry_coverage(manifest, entries):
    selected = []
    for entry in entries:
        binary = manifest['binaries'][entry['binary']]
        if entry.get('batch'):
            names = set(binary['tests']) - set(binary['ignored_tests'])
            names -= set(entry.get('skip', ())) | set(entry.get('skip_exact', ()))
        elif 'filter' in entry:
            names = {name for name in binary['tests'] if entry['filter'] in name
                     and name not in binary['ignored_tests']}
        else:
            names = {entry['name']}
        selected.extend((entry['binary'], name) for name in names)
    return selected


class OrdinaryArtifactDelivery(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.bundle, self.archives = self.root / 'producer', self.root / 'archives'
        self.bundle.mkdir()
        self.config = self.root / 'suites.json'
        self.config.write_text('{}')
        self.plan = complete_partition_selection() | dict(
            ordinary_artifact_delivery_version=1,
            oasis7_required_packages=['oasis7', 'oasis7_runtime', 'oasis7_chain_runtime', 'oasis7_viewer_runtime'])
        self.manifest = complete_partition_manifest(self.plan)
        self.manifest['binaries']['viewer-component'] = dict(
            package='oasis7_viewer_runtime', target='oasis7_viewer_runtime', target_kind=['lib'],
            variant='required', ordinary=True, tests=['viewer_behavior'], ignored_tests=[], cwd='.')
        self.manifest['binaries']['viewer'] = dict(test=False, ordinary=False, tests=[], ignored_tests=[])
        self.manifest['binaries']['required-executable-helper'] = dict(test=False, ordinary=False, tests=[], ignored_tests=[])
        for name, entry in self.manifest['binaries'].items():
            entry.update(test=entry.get('test', True), path='bin/' + name, libraries=[])
            entry['original_executable'] = str(self.root / 'target/debug' / name)
            text = ('#!' + sys.executable + '\nimport sys\n'
                    'names = ' + repr(entry['tests']) + '\n'
                    'if "--ignored" not in sys.argv:\n'
                    '    for name in names: print(name + ": test")\n')
            self.file(entry['path'], text.encode(), executable=True)
            entry['sha256'] = artifacts.digest(self.bundle / entry['path'])
        # The two helpers intentionally have identical bytes, exercising safe
        # archive-local hardlinks without conflating their logical identities.
        self.file('lib/runtime.so', b'fixture dynamic library')
        self.manifest['binaries']['runtime-tests']['libraries'] = ['lib/runtime.so']
        self.manifest.update(schema_version=2, suites={},
            source_sha='source', tested_sha='tested', producer_run_id='local', producer_attempt='1',
            checkout=str(self.root), config_sha256=artifacts.digest(self.config), formal_authority_sha256=None,
            rust_toolchain='local', runner_image=sys.platform,
            libraries={'lib/runtime.so': artifacts.digest(self.bundle / 'lib/runtime.so')})
        stack = ExitStack()
        self.addCleanup(stack.close)
        stack.enter_context(patch.object(artifacts, 'ROOT', self.root))
        stack.enter_context(patch.object(artifacts, 'CONFIG', self.config))
        # Reduce only the package universe to this fixture; retain real parent,
        # closure, partition, archive, file and actual executable inventory checks.
        stack.enter_context(patch.object(artifacts, 'required_packages',
                                         return_value=self.plan['oasis7_required_packages']))
        stack.enter_context(patch.object(artifacts, 'output', return_value='tested'))
        stack.enter_context(patch.dict(os.environ, SOURCE_SHA='source', GITHUB_RUN_ID='local',
                                       RUST_TOOLCHAIN='local', ImageOS=sys.platform, GITHUB_RUN_ATTEMPT='2'))
        self.write_parent()

    def file(self, name, data, executable=False):
        path = self.bundle / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(0o755 if executable else 0o644)
        return dict(path=name, sha256=artifacts.digest(path))

    def write_parent(self):
        (self.bundle / 'manifest.json').write_text(json.dumps(self.manifest))

    def pack(self, diagnostic=False):
        self.write_parent()
        return artifacts.pack_ordinary_delivery(self.bundle, self.archives, self.plan, diagnostic=diagnostic)

    def unpack(self, owner, common=None, payload=None, diagnostic=False):
        destination = self.root / ('consume-' + owner)
        return destination, artifacts.unpack_ordinary_delivery(
            common or self.archives / 'common.tar.gz', payload or self.archives / (owner + '.tar.gz'),
            destination, self.plan, owner, diagnostic=diagnostic)

    def mutate_archive(self, source, mutate):
        members = []
        with tarfile.open(source) as archive:
            for member in archive:
                members.append((member, archive.extractfile(member).read() if member.isfile() else None))
        mutate(members)
        destination = self.root / 'damaged.tar.gz'
        with tarfile.open(destination, 'w:gz') as archive:
            for member, raw in members:
                if raw is not None:
                    member.size = len(raw)
                archive.addfile(member, io.BytesIO(raw) if raw is not None else None)
        return destination

    def test_transport_preserves_parent_union_snapshots_helpers_libraries_and_prior_attempt(self):
        report = self.pack()
        ordinary = self.root / 'ordinary.tar.gz'
        artifacts.pack(self.bundle, ordinary, suite='ordinary', plan=self.plan)
        with tarfile.open(ordinary) as archive:
            parent = archive.extractfile('manifest.json').read()
        self.assertEqual(report['parent_manifest_sha256'], hashlib.sha256(parent).hexdigest())
        total = []
        for owner in artifacts.ORDINARY_ARTIFACT_PARTITIONS[1:]:
            destination, receipt = self.unpack(owner)
            manifest, env = artifacts.load(destination, 'ordinary', plan=self.plan, ordinary_artifact_partition=owner)
            self.assertEqual((destination / 'manifest.json').read_bytes(), parent)
            self.assertEqual(manifest['producer_attempt'], '1')
            self.assertTrue((destination / 'lib/runtime.so').is_file())
            self.assertIn('PRE2_VIEWER_BINARY', env)
            self.assertTrue((self.root / 'target/debug/required-executable-helper').is_file())
            selected = artifacts.ordinary_artifact_entries(manifest, self.plan, owner)
            total.extend(selected)
            expected = {manifest['binaries'][name]['path'] for name in receipt['included_binaries']}
            self.assertEqual({p.relative_to(destination).as_posix() for p in (destination / 'bin').iterdir()}, expected)
        self.assertCountEqual(total, artifacts.ordinary_entries(json.loads(parent)))
        with tarfile.open(self.archives / 'common.tar.gz') as archive:
            self.assertTrue(any(member.islnk() for member in archive))
        self.assertNotIn('controls', report['archives'])

    def test_wrong_capability_cannot_publish_or_consume_delivery(self):
        for value in (True, False, 0, 2, '1', None):
            with self.subTest(value=value), self.assertRaises(ValueError):
                artifacts.fast_plan(self.plan | {'ordinary_artifact_delivery_version': value})
        for change in ({'ordinary_artifact_partition_version': 2}, {'ordinary_execution': 'inline'},
                       {'ordinary_partitions': ['all']}, {'scope': 'targeted'}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.fast_plan(self.plan | change)
        for field in ('ordinary_artifact_partition_version', 'fast_plan_version'):
            changed = dict(self.plan)
            changed.pop(field)
            with self.subTest(field=field), self.assertRaises(ValueError):
                artifacts.fast_plan(changed)
        self.plan.pop('ordinary_artifact_delivery_version')
        self.write_parent()
        with self.assertRaises(ValueError):
            self.pack()
        # A legacy complete package still works; diagnostics cannot authorize run.
        legacy = self.root / 'legacy.tar.gz'
        artifacts.pack(self.bundle, legacy, 'ordinary', plan=self.plan)
        self.pack(diagnostic=True)
        destination, _ = self.unpack('runtime', diagnostic=True)
        with self.assertRaises(ValueError):
            artifacts.load(destination, 'ordinary', plan=self.plan)
        with self.assertRaises(ValueError):
            artifacts.load(destination, 'ordinary', plan=self.plan, ordinary_artifact_partition='runtime')

    def test_receipts_reject_missing_duplicate_wrong_owner_and_mixed_parent(self):
        self.pack()
        for change in ({'schema_version': True}, {'schema_version': 2}, {'diagnostic_only': True},
                       {'partition': 'integrations-0'}, {'parent_manifest_sha256': '0' * 64},
                       {'included_binaries': []}, {'included_binaries': ['runtime-tests'] * 2}):
            def mutate(members):
                member, raw = members[0]
                members[0] = member, json.dumps(json.loads(raw) | change).encode()
            damaged = self.mutate_archive(self.archives / 'runtime.tar.gz', mutate)
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.unpack('runtime', payload=damaged)
        payload = self.root / 'earlier-runtime.tar.gz'
        shutil.copyfile(self.archives / 'runtime.tar.gz', payload)
        self.manifest['producer_attempt'] = '2'
        self.pack()
        with self.assertRaises(ValueError):
            self.unpack('runtime', payload=payload)

    def test_archive_refuses_overwrites_missing_files_and_external_links(self):
        self.pack()
        source = self.archives / 'runtime.tar.gz'
        for name in ('manifest.json', artifacts.ORDINARY_DELIVERY_RECEIPTS[0],
                     'bin/viewer', 'lib/runtime.so', '../outside', '/absolute'):
            def append(members):
                member = tarfile.TarInfo(name)
                member.mode = 0o644
                members.append((member, b'foreign payload'))
            damaged = self.mutate_archive(source, append)
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.unpack('runtime', payload=damaged)
        for damage in ('duplicate', 'missing', 'symlink', 'cross-hardlink', 'corrupt'):
            def mutate(members):
                if damage == 'duplicate':
                    members.append(copy.deepcopy(members[-1]))
                elif damage == 'missing':
                    members.pop()
                elif damage == 'corrupt':
                    members[-1] = members[-1][0], b'corrupt bytes'
                else:
                    member, _ = members[-1]
                    member.type = tarfile.SYMTYPE if damage == 'symlink' else tarfile.LNKTYPE
                    member.linkname, member.size = 'bin/viewer', 0
                    members[-1] = member, None
            damaged = self.mutate_archive(source, mutate)
            with self.subTest(damage=damage), self.assertRaises(ValueError):
                self.unpack('runtime', payload=damaged)
        self.assertFalse((self.root / 'outside').exists())

    def test_pack_ordinary_delivery_compresses_archives_concurrently_and_preserves_order(self):
        entered = threading.Barrier(2)
        lock = threading.Lock()
        active = [0, 0]
        original_write = artifacts.write_artifact_archive

        def tracked_write(bundle, archive_path, metadata_files, files):
            with lock:
                active[0] += 1
                active[1] = max(active)
            try:
                if active[0] <= 2:
                    try:
                        entered.wait(timeout=1.5)
                    except threading.BrokenBarrierError:
                        pass
                return original_write(bundle, archive_path, metadata_files, files)
            finally:
                with lock:
                    active[0] -= 1

        with patch.object(artifacts, 'native_cpu_budget', return_value=4), \
                patch.object(artifacts, 'write_artifact_archive', side_effect=tracked_write):
            report = self.pack()
        self.assertGreaterEqual(active[1], 2)
        expected_owners = ['common', *artifacts.ordinary_partitions(self.plan)[1:]]
        self.assertEqual(list(report['archives'].keys()), expected_owners)
        for owner in expected_owners[1:]:
            self.unpack(owner)

    def test_load_keeps_real_inventory_executable_source_and_complete_parent_checks(self):
        self.pack()
        for damage in ('mode', 'missing', 'inventory', 'parent', 'source', 'tested', 'plan'):
            destination, _ = self.unpack('runtime')
            manifest = json.loads((destination / 'manifest.json').read_bytes())
            executable = destination / manifest['binaries']['runtime-tests']['path']
            if damage == 'mode':
                executable.chmod(0o644)
            elif damage == 'missing':
                executable.unlink()
            else:
                if damage == 'inventory':
                    manifest['binaries']['runtime-tests']['tests'].append('invented_case')
                elif damage == 'parent':
                    del manifest['binaries']['component']
                elif damage == 'plan':
                    manifest['execution_plan']['policy_sha'] = 'changed'
                else:
                    manifest[damage + '_sha'] = 'different'
                raw = json.dumps(manifest).encode()
                (destination / 'manifest.json').write_bytes(raw)
                for receipt_name in artifacts.ORDINARY_DELIVERY_RECEIPTS:
                    path = destination / receipt_name
                    receipt = json.loads(path.read_bytes())
                    receipt['parent_manifest_sha256'] = hashlib.sha256(raw).hexdigest()
                    path.write_text(json.dumps(receipt))
            with self.subTest(damage=damage), self.assertRaises(ValueError):
                artifacts.load(destination, 'ordinary', plan=self.plan, ordinary_artifact_partition='runtime')
            shutil.rmtree(destination)

    def test_shared_provider_and_builtin_resources_keep_full_parent_requirements(self):
        self.manifest['binaries']['runtime-tests']['features'] = ['test_tier_full']
        self.manifest['binaries']['component']['features'] = ['wasmtime', 'test_tier_full']
        identity = dict(source_sha='source', tested_sha='tested', checkout_tree_sha256='tree')
        provider = artifacts.ProviderArtifact
        provider_files = {kind: self.file(provider.DIRECTORY + '/' + provider.MODULE + suffix,
                                          b'explicit synthetic verifier fixture')
                          for kind, suffix in (('wasm', '.wasm'), ('metadata', '.metadata.json'),
                                                ('receipt', '.build-receipt.json'))}
        provider_files['checker'] = self.file(provider.DIRECTORY + '/sync_builtin_wasm_identity', b'checker', True)
        self.manifest['preview_provider'] = dict(identity, files=provider_files, libraries=[])
        builtin = artifacts.BuiltinTestArtifact
        modules = {module: {kind: self.file(builtin.DIRECTORY + '/' + module + suffix, b'builtin fixture')
                            for kind, suffix in (('wasm', '.wasm'), ('metadata', '.metadata.json'),
                                                  ('receipt', '.build-receipt.json'))}
                   for module in builtin.MODULES}
        checker = self.file(builtin.DIRECTORY + '/sync_builtin_wasm_identity', b'checker', True)
        self.manifest['builtin_test_resources'] = dict(identity, modules=modules,
                                                       checker=dict(files={'checker': checker}, libraries=[]))
        original_run = subprocess.run
        verified = []
        def official_verifier_fixture(command, *args, **kwargs):
            # Only the canonical Rust verifier is mocked. File identities,
            # requirements, environments and actual native --list stay real.
            if any(flag in command for flag in ('--verify-provider-artifact', '--verify-builtin-test-artifact')):
                verified.append(command)
                return subprocess.CompletedProcess(command, 0)
            return original_run(command, *args, **kwargs)
        with patch.object(provider, 'tree_digest', return_value='tree'), \
             patch.object(subprocess, 'run', side_effect=official_verifier_fixture):
            self.pack()
            for owner in self.plan['ordinary_partitions'][1:]:
                destination, _ = self.unpack(owner)
                parent, env = artifacts.load(destination, 'ordinary', plan=self.plan,
                                              ordinary_artifact_partition=owner)
                self.assertEqual(parent['selection'], {'suite': 'ordinary'})
                self.assertTrue(builtin.required(parent))
                self.assertTrue(provider.VARIABLES[0] in env and 'OASIS7_BUILTIN_WASM_FETCHER' in env)
            self.assertEqual(len(verified), 25)  # Producer and all four consumers, all five resources.
            destination = self.root / ('consume-' + self.plan['ordinary_partitions'][1])
            for entry in (provider_files['wasm'], modules['m1.memory.core']['metadata']):
                path = destination / entry['path']
                data = path.read_bytes()
                path.write_bytes(b'corrupt shared resource')
                with self.subTest(path=entry['path']), self.assertRaises(ValueError):
                    artifacts.load(destination, 'ordinary', plan=self.plan,
                                   ordinary_artifact_partition=self.plan['ordinary_partitions'][1])
                path.write_bytes(data)


class CasePartitionArtifactDelivery(unittest.TestCase):
    file = OrdinaryArtifactDelivery.file
    write_parent = OrdinaryArtifactDelivery.write_parent
    pack = OrdinaryArtifactDelivery.pack
    unpack = OrdinaryArtifactDelivery.unpack
    mutate_archive = OrdinaryArtifactDelivery.mutate_archive

    def setUp(self):
        OrdinaryArtifactDelivery.setUp(self)
        self.plan.update(ordinary_artifact_partition_version=2, ordinary_artifact_delivery_version=2,
                         ordinary_partitions=list(artifacts.TARGETED_ORDINARY_ARTIFACT_PARTITIONS))
        runtime = self.manifest['binaries']['runtime-tests']
        runtime['tests'] = sorted([f'case_{index}' for index in range(12)] + list(artifacts.ORDINARY_EXCLUSIVE_RUNTIME_CASES))
        text = ('#!' + sys.executable + '\nimport sys\n'
                'names = ' + repr(runtime['tests']) + '\n'
                'if "--ignored" not in sys.argv:\n'
                '    for name in names: print(name + ": test")\n')
        self.file(runtime['path'], text.encode(), executable=True)
        runtime['sha256'] = artifacts.digest(self.bundle / runtime['path'])
        self.write_parent()

    def test_shared_provider_and_builtin_resources_keep_full_parent_requirements(self):
        OrdinaryArtifactDelivery.test_shared_provider_and_builtin_resources_keep_full_parent_requirements(self)

    def test_common_shares_only_runtime_and_payloads_cover_every_other_complete_harness(self):
        report = self.pack()
        whole = self.root / 'ordinary.tar.gz'
        artifacts.pack(self.bundle, whole, suite='ordinary', plan=self.plan)
        with tarfile.open(whole) as archive:
            parent = archive.extractfile('manifest.json').read()
        common = set(report['archives']['common']['included_binaries'])
        self.assertEqual({name for name in common if self.manifest['binaries'][name]['test']}, {'runtime-tests'})
        self.assertEqual(set(report['archives']), {'common', *self.plan['ordinary_partitions'][1:]})
        cases, payloads = [], []
        for owner in self.plan['ordinary_partitions'][1:]:
            destination, receipt = self.unpack(owner)
            manifest, env = artifacts.load(destination, 'ordinary', plan=self.plan, ordinary_artifact_partition=owner)
            self.assertEqual((destination / 'manifest.json').read_bytes(), parent)
            self.assertEqual(manifest['producer_attempt'], '1')
            self.assertTrue((destination / 'lib/runtime.so').is_file())
            self.assertIn('PRE2_VIEWER_BINARY', env)
            self.assertTrue((self.root / 'target/debug/required-executable-helper').is_file())
            selected = artifacts.ordinary_artifact_entries(manifest, self.plan, owner)
            actual = entry_coverage(manifest, selected)
            self.assertTrue(actual)
            self.assertTrue(any(name == 'runtime-tests' for name, _ in actual))
            cases.extend(actual)
            payload = set(report['archives'][owner]['included_binaries'])
            self.assertFalse(payload & common)
            payloads.extend(payload)
            self.assertEqual(set(receipt['included_binaries']), common | payload)
            self.assertEqual({p.relative_to(destination).as_posix() for p in (destination / 'bin').iterdir()},
                             {manifest['binaries'][name]['path'] for name in common | payload})
        self.assertEqual(len(payloads), len(set(payloads)))
        self.assertEqual(set(payloads) | common, set(json.loads(parent)['included_binaries']))
        self.assertCountEqual(cases, entry_coverage(self.manifest,
                              artifacts.ordinary_entries(self.manifest)))
        self.assertEqual(len(cases), len(set(cases)))
        self.assertTrue(all(any(binary == owner for binary, _ in cases)
                            for owner, _ in artifacts.DEFAULT_SNAPSHOT_REGRESSIONS))
        self.assertEqual(report['consumer_compressed_bytes'], sum(
            report['archives']['common']['compressed_bytes'] + report['archives'][owner]['compressed_bytes']
            for owner in self.plan['ordinary_partitions'][1:]))

    def test_compact_three_partition_delivery_covers_every_case_and_binary(self):
        self.plan.update(
            scope='targeted',
            oasis7_required_packages=['oasis7_chain_runtime'],
            ordinary_dynamic_partition_version=1,
            ordinary_partitions=list(artifacts.COMPACT_TARGETED_ORDINARY_ARTIFACT_PARTITIONS),
            ordinary_harness_costs=json.loads((HERE / 'ci-required-scope.json').read_text())['ordinary_harness_costs'],
        )
        self.write_parent()
        report = self.pack()
        self.assertEqual(set(report['archives']), {'common', 'native-0', 'native-1'})
        cases = []
        for owner in ('native-0', 'native-1'):
            destination, _ = self.unpack(owner)
            manifest, _ = artifacts.load(destination, 'ordinary', plan=self.plan, ordinary_artifact_partition=owner)
            selected = artifacts.ordinary_artifact_entries(manifest, self.plan, owner)
            cases.extend(entry_coverage(manifest, selected))
        self.assertCountEqual(cases, entry_coverage(
            self.manifest, artifacts.ordinary_entries(self.manifest)))
        self.assertEqual(len(cases), len(set(cases)))
        with self.assertRaises(ValueError):
            artifacts.fast_plan(self.plan | {'ordinary_partitions': ['controls', 'native-0', 'native-1', 'native-2', 'native-3']})

        # Multi-package targeted workloads exceeding 60s of non-runtime harnesses expand to 4 shards
        # so the ~358s/shard 2-way runtime-tests baseline does not push shards past 8m.
        bft_like = dict(
            self.plan,
            oasis7_required_packages=['oasis7_chain_runtime', 'oasis7_formal_protocol', 'oasis7_viewer_runtime'],
        )
        self.assertEqual(
            list(artifacts.targeted_ordinary_artifact_partitions(bft_like)),
            list(artifacts.TARGETED_ORDINARY_ARTIFACT_PARTITIONS),
        )
        with self.assertRaises(ValueError):
            artifacts.fast_plan(bft_like)

        # Verify inventory_pairs preserves exact keys/results under bounded concurrency and OASIS7_LOCAL_CHECK=1.
        with patch.object(artifacts, 'inventory', side_effect=lambda p, ignored=False, env=None: [f'{p.name}:{int(ignored)}']):
            for local_flag in ('0', '1'):
                with patch.dict(os.environ, {'OASIS7_LOCAL_CHECK': local_flag}):
                    pairs = artifacts.inventory_pairs([('a', Path('/tmp/bin_a')), ('b', Path('/tmp/bin_b'))])
                self.assertEqual(pairs, {'a': (['bin_a:0'], ['bin_a:1']), 'b': (['bin_b:0'], ['bin_b:1'])})

    def test_full_case_partition_delivery_requires_new_authority_and_preserves_profile_gates(self):
        for scope in ('targeted', 'full'):
            for profile in ('pr-fast', 'main-fast'):
                selected = self.plan | dict(scope=scope, execution_profile=profile)
                self.assertEqual(artifacts.fast_plan(selected), selected)
        for change in ({'ordinary_artifact_delivery_version': 1}, {'ordinary_artifact_delivery_version': True},
                       {'ordinary_artifact_partition_version': 1}, {'scope': 'explicit-full'},
                       {'execution_profile': 'periodic-full'}, {'execution_profile': 'candidate-full'},
                       {'test_policy_changed': True}, {'ordinary_fast_targets': []},
                       {'ordinary_full_fast_targets': []}, {'ordinary_execution': 'inline'}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                artifacts.fast_plan(self.plan | change)
        legacy = dict(self.plan)
        legacy.pop('ordinary_artifact_delivery_version')
        with self.assertRaises(ValueError):
            artifacts.fast_plan(legacy)
        legacy['scope'] = 'targeted'
        artifacts.fast_plan(legacy)
        with self.assertRaises(ValueError):
            self.pack(diagnostic=True)

    def test_changed_partition_inventory_cannot_drop_repeat_or_share_another_harness(self):
        original = artifacts.ordinary_artifact_entries
        groups = {owner: original(self.manifest, self.plan, owner) for owner in self.plan['ordinary_partitions'][1:]}
        for damage in ('missing-case', 'duplicate-case', 'empty-case', 'wrong-count',
                       'missing-owner', 'missing-harness', 'shared-harness'):
            changed = copy.deepcopy(groups)
            runtime = changed['native-0'][0]
            case = next(name for binary, name in entry_coverage(self.manifest, [runtime]))
            if damage == 'missing-case':
                runtime['skip_exact'].append(case)
                runtime['count'] -= 1
            elif damage == 'duplicate-case':
                changed['native-0'][0] = copy.deepcopy(changed['native-1'][0])
            elif damage == 'empty-case':
                runtime.update(skip_exact=self.manifest['binaries']['runtime-tests']['tests'], count=0)
            elif damage == 'wrong-count':
                runtime['count'] += 1
            elif damage == 'missing-owner':
                changed['native-0'] = []
            else:
                owner, index = next((owner, index) for owner, entries in changed.items()
                                    for index, entry in enumerate(entries) if entry['binary'] != 'runtime-tests')
                entry = changed[owner][index]
                if damage == 'missing-harness':
                    changed[owner].pop(index)
                else:
                    other = next(name for name in changed if name != owner)
                    changed[other].append(entry)
            with self.subTest(damage=damage), patch.object(artifacts, 'ordinary_artifact_entries',
                    side_effect=lambda manifest, plan, owner: changed[owner]), self.assertRaises(ValueError):
                self.pack()

    def test_v2_receipts_and_archives_reject_cross_parent_wrong_owner_and_common_overwrites(self):
        report = self.pack()
        owner = next(owner for owner in self.plan['ordinary_partitions'][1:]
                     if report['archives'][owner]['included_binaries'])
        payload = self.archives / (owner + '.tar.gz')
        for change in ({'partition': 'native-other'}, {'parent_manifest_sha256': '0' * 64},
                       {'included_binaries': ['runtime-tests']}, {'diagnostic_only': True}):
            def mutate(members):
                member, raw = members[0]
                members[0] = member, json.dumps(json.loads(raw) | change).encode()
            damaged = self.mutate_archive(payload, mutate)
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.unpack(owner, payload=damaged)
        for source, field in ((payload, 'payload'), (self.archives / 'common.tar.gz', 'common')):
            for damage in ('missing', 'corrupt', 'duplicate', 'common-overwrite', 'external-link'):
                def mutate(members):
                    if damage == 'missing':
                        members.pop()
                    elif damage == 'corrupt':
                        member, _ = next((member, raw) for member, raw in reversed(members) if member.isfile())
                        index = next(index for index, (item, _) in enumerate(members) if item is member)
                        members[index] = member, b'corrupt bytes'
                    elif damage == 'duplicate':
                        members.append(copy.deepcopy(members[-1]))
                    else:
                        member = tarfile.TarInfo('bin/runtime-tests')
                        member.mode = 0o755
                        if damage == 'external-link':
                            member.type, member.linkname = tarfile.SYMTYPE, '../outside'
                            members.append((member, None))
                        else:
                            members.append((member, b'overwrite common runtime'))
                damaged = self.mutate_archive(source, mutate)
                with self.subTest(field=field, damage=damage), self.assertRaises(ValueError):
                    self.unpack(owner, **{field: damaged})
        earlier = self.root / 'earlier-payload.tar.gz'
        shutil.copyfile(payload, earlier)
        self.manifest['producer_attempt'] = '2'
        self.pack()
        with self.assertRaises(ValueError):
            self.unpack(owner, payload=earlier)


if __name__ == '__main__':
    unittest.main()
