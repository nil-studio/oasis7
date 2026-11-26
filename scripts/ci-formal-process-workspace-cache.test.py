#!/usr/bin/env python3
"""Workspace cache regressions split from ci-formal-process.test.py."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
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


class WorkspaceCacheSelection(unittest.TestCase):
    """Cargo JSON fixtures cover unit selection without requiring a toolchain."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'checkout'
        self.root.mkdir()
        self.target = self.root / 'target'
        self.cache = self.base / 'cache'
        self.manifest = self.root / 'Cargo.toml'
        self.manifest.write_text('[package]\nname="cache_local"\nversion="0.1.0"\n')
        (self.root / 'lib.rs').write_text('pub fn value() {}')
        (self.root / 'build.rs').write_text('fn main() {}')
        self.messages, self.library_files, self.other_files = [], set(), set()
        for name, kinds, unit_hash, test, suffixes in [
                ('cache_lib', ['lib'], 'a' * 16, False, ['rlib', 'rmeta']),
                ('cache_macro', ['proc-macro'], 'b' * 16, False, ['so']),
                ('cache_lib', ['lib'], 'c' * 16, True, ['']),
                ('cache_tool', ['bin'], 'd' * 16, False, [''])]:
            selected = self.library_files if not test and 'bin' not in kinds else self.other_files
            filenames = []
            for suffix in suffixes:
                path = f'debug/deps/{"lib" if suffix else ""}{name}-{unit_hash}' + ('.' + suffix if suffix else '')
                self.write(path, selected)
                filenames.append(str(self.target / path))
            self.write(f'debug/deps/{name}-{unit_hash}.d', selected)
            self.write(f'debug/.fingerprint/cache_local-{unit_hash}/invoked.timestamp', selected)
            self.write(f'debug/.fingerprint/cache_local-{unit_hash}/unit.json', selected)
            self.messages.append(dict(reason='compiler-artifact', manifest_path=str(self.manifest),
                package_id='cache_local', filenames=filenames, target=dict(name=name, kind=kinds, src_path=str(self.root / 'lib.rs')),
                profile=dict(test=test)))
        compile_dir = 'debug/build/cache_local-' + 'e' * 16
        run_dir = 'debug/build/cache_local-' + 'f' * 16
        for path in (compile_dir + '/build-script-build', compile_dir + '/build_script_build.d',
                     'debug/.fingerprint/cache_local-' + 'e' * 16 + '/build-script-build-script-build',
                     run_dir + '/out/generated.rs', run_dir + '/output', run_dir + '/root-output',
                     run_dir + '/invoked.timestamp', run_dir + '/stderr',
                     'debug/.fingerprint/cache_local-' + 'f' * 16 + '/run-build-script-build-script-build'):
            self.write(path, self.library_files)
        self.messages += [
            dict(reason='compiler-artifact', manifest_path=str(self.manifest), package_id='cache_local',
                 filenames=[str(self.target / compile_dir / 'build-script-build')],
                 target=dict(name='build-script-build', kind=['custom-build'], src_path=str(self.root / 'build.rs')), profile=dict(test=False)),
            dict(reason='build-script-executed', package_id='cache_local', out_dir=str(self.target / run_dir / 'out')),
            dict(reason='compiler-artifact', manifest_path=str(self.base / 'external/Cargo.toml')),
            dict(reason='build-script-executed', package_id='external', out_dir=str(self.base / 'external/out')),
        ]

    def write(self, name, selection):
        path = self.target / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name)
        selection.add(name)

    def record(self, state):
        for message in self.messages:
            artifacts.cache_record(state, message)
        artifacts.cache_finish(state)

    def test_libraries_keep_complete_build_scripts_and_omit_binaries_and_harnesses(self):
        state = artifacts.cache_begin(self.root, selection='libraries')
        self.record(state)
        report = artifacts.cache_stage(state, self.cache)
        self.assertTrue(report['save'], report)
        self.assertEqual(report['selection'], 'libraries')
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        self.assertEqual(set(manifest['files']), self.library_files)
        self.assertTrue(all((self.cache / 'target' / name).is_file() for name in self.library_files))
        self.assertFalse(any((self.cache / 'target' / name).exists() for name in self.other_files))
        self.assertEqual(report['file_count'], len(self.library_files))
        self.assertLess(report['raw_bytes'], report['target_raw_bytes'])

    def test_default_and_legacy_state_keep_all_complete_units(self):
        for legacy in (False, True):
            with self.subTest(legacy=legacy):
                state = artifacts.cache_begin(self.root)
                self.assertEqual(state['selection'], 'all')
                if legacy:
                    del state['selection']
                self.record(state)
                report = artifacts.cache_stage(state, self.cache)
                self.assertTrue(report['save'], report)
                self.assertEqual(report['selection'], 'all')
                manifest = json.loads((self.cache / 'manifest.json').read_text())
                self.assertEqual(set(manifest['files']), self.library_files | self.other_files)

    def test_unknown_selection_and_oversize_library_units_do_not_stage(self):
        with self.assertRaisesRegex(ValueError, 'selection'):
            artifacts.cache_begin(self.root, selection='binaries')
        state = artifacts.cache_begin(self.root, selection='libraries')
        self.record(state)
        report = artifacts.cache_stage(state, self.cache, maximum=1)
        self.assertFalse(report['save'], report)
        self.assertFalse(report['eligible'], report)
        self.assertGreater(report['raw_bytes'], 1)
        self.assertFalse(self.cache.exists())


class WorkspaceCache(unittest.TestCase):
    """Real Cargo units, including both build-script units and a dependency cache."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / 'checkout'
        self.cache = self.base / 'workspace-cache'
        self.dependencies = self.base / 'dependency-cache'
        dependency = self.base / 'dependency'
        (dependency / 'src').mkdir(parents=True)
        (dependency / 'Cargo.toml').write_text('[package]\nname="cache_external"\nversion="0.1.0"\nedition="2021"\n')
        (dependency / 'src/lib.rs').write_text('pub fn value() -> &\'static str { "outside" }')
        self.sources = {
            'Cargo.toml': '[workspace]\nmembers=["support", "app"]\nresolver="2"\n[profile.dev]\ndebug=0\nincremental=false\n',
            '.gitignore': 'app/input.txt\n',
            'support/Cargo.toml': '[package]\nname="cache_support"\nversion="0.1.0"\nedition="2021"\n',
            'support/src/lib.rs': 'pub fn value() -> &\'static str { "alpha" }',
            'app/Cargo.toml': '[package]\nname="cache_app"\nversion="0.1.0"\nedition="2021"\n[dependencies]\ncache_support={path="../support"}\ncache_external={path="../../dependency"}\n',
            'app/src/main.rs': 'include!(concat!(env!("OUT_DIR"), "/generated.rs"));\nfn main() { println!("{}:{}:{}:{}", cache_support::value(), cache_external::value(), GENERATED, include_str!("../input.txt").trim()); }',
            # This file is deliberately not tracked, and .gitignore excludes it.
            'app/input.txt': 'one\n',
            'app/build.rs': 'use std::os::unix::fs::PermissionsExt; fn main() { let v=std::fs::read_to_string("input.txt").unwrap(); let mode=std::fs::metadata("input.txt").unwrap().permissions().mode(); let v=format!("{}{}", v.trim(), if mode & 0o111 != 0 { ":executable" } else { "" }); let out=std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()); std::fs::write(out.join("generated.rs"), format!("const GENERATED: &str = {:?};", v)).unwrap(); println!("cargo:rerun-if-changed=input.txt"); println!("cargo:rerun-if-env-changed=CACHE_FIXTURE_ENV"); }',
        }
        self.checkout()
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        subprocess.run(['git', 'add', '.'], cwd=self.root, check=True)
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.root,
                       check=True, capture_output=True)
        self.sources['Cargo.lock'] = (self.root / 'Cargo.lock').read_text()

    def checkout(self):
        if self.root.exists():
            shutil.rmtree(self.root)
        for name, content in self.sources.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        if self.dependencies.exists():
            shutil.copytree(self.dependencies, self.root / 'target', copy_function=shutil.copy2)

    def cargo(self, state=None, success=True, extra_env=None, extra_args=()):
        env = dict(os.environ, CARGO_INCREMENTAL='0', CACHE_FIXTURE_ENV='original')
        env.pop('RUSTC_WRAPPER', None)
        env.update(extra_env or {})
        messages = []
        with tempfile.TemporaryFile(mode='w+') as errors:
            with subprocess.Popen(['cargo', 'build', '--workspace', '--offline', '--locked',
                                   '--message-format=json', *extra_args], cwd=self.root, env=env,
                                  stdout=subprocess.PIPE, stderr=errors, text=True) as process:
                for line in process.stdout:
                    if line.startswith('{'):
                        message = json.loads(line)
                        messages.append(message)
                        if state is not None:
                            artifacts.cache_record(state, message)
                process.wait()
            errors.seek(0)
            self.assertEqual(process.returncode == 0, success, errors.read())
        if state is not None:
            artifacts.cache_finish(state)
        units = [m for m in messages if m.get('reason') == 'compiler-artifact']
        if success:
            value = subprocess.check_output([str(self.root / 'target/debug/cache_app')], text=True).strip()
        else:
            value = None
        return units, value

    def seed(self, selection='all'):
        state = artifacts.cache_begin(self.root, selection=selection)
        units, value = self.cargo(state)
        self.assertEqual(value, 'alpha:outside:one:one')
        self.assertTrue(all(not u['fresh'] for u in units))
        report = artifacts.cache_stage(state, self.cache)
        self.assertTrue(report['save'], report)
        self.assertGreater(report['target_raw_bytes'], report['raw_bytes'])
        # Restore only the external dependency, like the existing dependency
        # cache. No local-package outputs survive without the workspace overlay.
        for subdirectory in ('deps', '.fingerprint'):
            source = self.root / 'target/debug' / subdirectory
            for path in source.iterdir():
                if not path.name.startswith(('cache_external-', 'libcache_external-')):
                    continue
                destination = self.dependencies / 'debug' / subdirectory / path.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                if path.is_dir():
                    shutil.copytree(path, destination, copy_function=shutil.copy2)
                else:
                    shutil.copy2(path, destination)
        # An ordinary archive can round file mtimes to seconds. The manifest
        # must restore exact Cargo timestamps instead of trusting tar headers.
        archive_path = self.base / 'cache.tar'
        with tarfile.open(archive_path, 'w', format=tarfile.USTAR_FORMAT) as archive:
            archive.add(self.cache, arcname='cache')
        shutil.rmtree(self.cache)
        with tarfile.open(archive_path) as archive:
            archive.extractall(self.base / 'restored', filter='data')
        shutil.move(self.base / 'restored/cache', self.cache)
        return state

    def test_library_overlay_reuses_libraries_and_build_scripts_but_rebuilds_binary(self):
        self.seed(selection='libraries')
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        self.assertNotIn('debug/cache_app', manifest['files'])
        self.assertFalse(any(name.startswith('debug/deps/cache_app-') for name in manifest['files']))
        self.checkout()
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        self.assertFalse((self.root / 'target/debug/cache_app').exists())
        units, value = self.cargo()
        self.assertEqual(value, 'alpha:outside:one:one')
        for unit in units:
            self.assertEqual(unit['fresh'], 'bin' not in unit['target']['kind'], unit)
        self.checkout()
        changed = self.root / 'support/src/lib.rs'
        changed.write_text(self.sources['support/src/lib.rs'].replace('alpha', 'bravo'))
        original_mtime = manifest['inputs']['support/src/lib.rs']['mtime_ns']
        os.utime(changed, ns=(original_mtime, original_mtime))
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, value = self.cargo()
        self.assertEqual(value, 'bravo:outside:one:one')
        self.assertTrue(any(not unit['fresh'] and unit['target']['name'] == 'cache_support' for unit in units))

    def test_real_overlay_is_fresh_and_changed_inputs_rebuild(self):
        self.seed()
        self.checkout()
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        units, value = self.cargo()
        self.assertTrue(all(u['fresh'] for u in units), units)
        self.assertEqual(value, 'alpha:outside:one:one')
        manifest = json.loads((self.cache / 'manifest.json').read_text())
        cases = [
            ('support/src/lib.rs', self.sources['support/src/lib.rs'].replace('alpha', 'bravo'), None,
             'bravo:outside:one:one'),
            ('app/input.txt', 'two\n', None, 'alpha:outside:two:two'),
            ('app/input.txt', 'one\n', 0o755, 'alpha:outside:one:executable:one'),
        ]
        for name, text, mode, expected in cases:
            with self.subTest(input=name, mode=mode):
                self.checkout()
                path = self.root / name
                path.write_text(text)
                if mode is not None:
                    path.chmod(mode)
                old_mtime = manifest['inputs'][name]['mtime_ns']
                os.utime(path, ns=(old_mtime, old_mtime))
                restored = artifacts.cache_restore(self.root, self.cache)
                self.assertTrue(restored['restored'], restored)
                self.assertEqual(restored['changed_inputs'], 1)
                units, value = self.cargo()
                self.assertTrue(any(not u['fresh'] and u['target']['name'] == 'cache_app' for u in units), units)
                self.assertEqual(value, expected)
        self.checkout()
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, _ = self.cargo(extra_env={'CACHE_FIXTURE_ENV': 'changed'})
        self.assertTrue(any(not u['fresh'] and u['target']['name'] == 'cache_app' for u in units), units)

    def test_path_changes_rebuild_and_unsafe_cache_inputs_are_misses(self):
        # Also retain Cargo's default package-directory observation when a
        # build script does not narrow its inputs with rerun-if directives.
        self.sources['app/build.rs'] = self.sources['app/build.rs'].replace(
            'println!("cargo:rerun-if-changed=input.txt"); println!("cargo:rerun-if-env-changed=CACHE_FIXTURE_ENV");', '')
        (self.root / 'app/build.rs').write_text(self.sources['app/build.rs'])
        self.seed()
        for change in ('new', 'deleted', 'renamed', 'source-symlink'):
            with self.subTest(change=change):
                self.checkout()
                path = self.root / 'app/input.txt'
                if change == 'new':
                    (self.root / 'app/new.txt').write_text('new ignored input')
                elif change == 'renamed':
                    path.rename(self.root / 'app/renamed-input.txt')
                else:
                    path.unlink()
                    if change == 'source-symlink':
                        path.symlink_to(self.base / 'dependency/src/lib.rs')
                report = artifacts.cache_restore(self.root, self.cache)
                if change == 'source-symlink':
                    self.assertFalse(report['restored'], report)
                    self.assertFalse((self.root / 'target/debug/cache_app').exists())
                else:
                    self.assertTrue(report['restored'], report)
                    units, _ = self.cargo(success=change not in ('deleted', 'renamed'))
                    self.assertTrue(any(not u['fresh'] for u in units))
        original = json.loads((self.cache / 'manifest.json').read_text())
        for damage in ('digest', 'missing', 'payload-symlink', 'traversal', 'future', 'checkout'):
            with self.subTest(damage=damage):
                self.checkout()
                broken = self.base / ('broken-' + damage)
                shutil.copytree(self.cache, broken, copy_function=shutil.copy2)
                manifest = copy.deepcopy(original)
                name = next(iter(manifest['files']))
                path = broken / 'target' / name
                if damage == 'digest':
                    path.write_bytes(b'broken')
                elif damage == 'missing':
                    path.unlink()
                elif damage == 'payload-symlink':
                    path.unlink()
                    path.symlink_to(self.base / 'dependency/src/lib.rs')
                elif damage == 'traversal':
                    manifest['files']['../escape'] = manifest['files'].pop(name)
                elif damage == 'future':
                    manifest['files'][name]['mtime_ns'] = 2 ** 63 - 1
                else:
                    manifest['checkout'] = str(self.base / 'different-checkout')
                (broken / 'manifest.json').write_text(json.dumps(manifest))
                report = artifacts.cache_restore(self.root, broken)
                self.assertFalse(report['restored'], report)
                self.assertFalse((self.root / 'target/debug/cache_app').exists())

    def test_document_addition_reuses_unaffected_library(self):
        self.sources['doc/index.md'] = 'existing documentation'
        self.checkout()
        self.seed()
        self.checkout()
        (self.root / 'doc/added.md').write_text('new documentation')
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        self.assertEqual(restored['input_path_changes']['added']['paths'], ['doc/added.md'])
        units, value = self.cargo()
        support = [u for u in units if u['target']['name'] == 'cache_support']
        self.assertTrue(support)
        self.assertTrue(all(u['fresh'] for u in support), support)
        self.assertTrue(any(not u['fresh'] and 'custom-build' in u['target']['kind'] for u in units))
        self.assertEqual(value, 'alpha:outside:one:one')

    def test_added_ambiguous_module_cannot_reuse_old_successful_binary(self):
        self.sources['support/src/lib.rs'] = 'mod piece; pub fn value() -> &\'static str { piece::value() }'
        self.sources['support/src/piece.rs'] = 'pub fn value() -> &\'static str { "alpha" }'
        self.checkout()
        self.seed()
        self.checkout()
        added = self.root / 'support/src/piece/mod.rs'
        added.parent.mkdir()
        added.write_text('pub fn value() -> &\'static str { "bravo" }')
        os.utime(added, ns=(1, 1))
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        # Rust must report the now-ambiguous module, not return a stale success.
        self.cargo(success=False)

    def test_outside_package_include_is_not_ignored(self):
        self.sources['assets/value.txt'] = 'alpha'
        self.sources['support/src/lib.rs'] = 'pub fn value() -> &\'static str { include_str!("../../assets/value.txt") }'
        self.checkout()
        self.seed()
        self.checkout()
        (self.root / 'assets/added.txt').write_text('new sibling')
        restored = artifacts.cache_restore(self.root, self.cache)
        self.assertTrue(restored['restored'], restored)
        units, value = self.cargo()
        support = [u for u in units if u['target']['name'] == 'cache_support']
        self.assertTrue(support)
        self.assertTrue(any(not u['fresh'] for u in support), support)
        self.assertEqual(value, 'alpha:outside:one:one')

    def test_directory_observer_rebuilds_on_add_remove_and_rename(self):
        self.sources['assets/one.txt'] = 'one'
        self.sources['app/build.rs'] = '''fn main() {
            let count = std::fs::read_dir("../assets").unwrap().count();
            let value = match count { 1 => "one", 2 => "two", _ => "zero" };
            let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
            std::fs::write(out.join("generated.rs"), format!("const GENERATED: &str = {:?};", value)).unwrap();
            println!("cargo:rerun-if-changed=../assets");
        }'''
        self.checkout()
        self.seed()
        for change, expected in [('add', 'two'), ('remove', 'zero'), ('rename', 'one')]:
            with self.subTest(change=change):
                self.checkout()
                path = self.root / 'assets/one.txt'
                if change == 'add':
                    (self.root / 'assets/two.txt').write_text('two')
                elif change == 'remove':
                    path.unlink()
                else:
                    path.rename(path.with_name('renamed.txt'))
                restored = artifacts.cache_restore(self.root, self.cache)
                self.assertTrue(restored['restored'], restored)
                units, value = self.cargo()
                self.assertEqual(value, f'alpha:outside:{expected}:one')
                self.assertTrue(any(not u['fresh'] and 'custom-build' in u['target']['kind'] for u in units))

    def test_budget_and_source_changes_do_not_publish_partial_units(self):
        state = artifacts.cache_begin(self.root)
        self.cargo(state)
        before = artifacts._cache_inputs(self.root)
        with patch.object(artifacts, 'digest', side_effect=AssertionError('measurement must not hash files')):
            measurement = artifacts.cache_stage(state, self.cache, measure_only=True)
        self.assertTrue(measurement['eligible'], measurement)
        self.assertFalse(measurement['save'])
        self.assertGreater(measurement['file_count'], 0)
        self.assertGreater(measurement['target_raw_bytes'], measurement['raw_bytes'])
        self.assertGreater(measurement['unique_bytes'], 0)
        self.assertTrue(measurement['bytes_by_category'])
        self.assertFalse(self.cache.exists())
        self.assertEqual(before, artifacts._cache_inputs(self.root))
        report = artifacts.cache_stage(state, self.cache, maximum=1)
        self.assertFalse(report['save'], report)
        self.assertGreater(report['raw_bytes'], 1)
        self.assertFalse((self.cache / 'manifest.json').exists())
        (self.root / 'app/input.txt').write_text('changed during build\n')
        with self.assertRaisesRegex(ValueError, 'source inputs changed'):
            artifacts.cache_finish(state)
        report = artifacts.cache_stage(state, self.cache)
        self.assertFalse(report['save'], report)
        self.assertFalse((self.cache / 'manifest.json').exists())


class MixedLibraryWorkspaceCache(WorkspaceCache):
    """Run the existing real cold/restore/change/corruption cases with cdylib+lib."""

    def setUp(self):
        super().setUp()
        self.sources['support/Cargo.toml'] += '\n[lib]\ncrate-type=["cdylib", "lib"]\n[features]\nalternate=[]\n'
        self.checkout()

    def test_dependency_only_mixed_library_is_fresh_after_restore(self):
        self.sources['Cargo.toml'] = self.sources['Cargo.toml'].replace(
            'members=["support", "app"]', 'members=["app"]\nexclude=["support"]')
        self.checkout()
        state = self.seed()
        self.assertTrue(state['unhashed_libraries'])
        self.checkout()
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, value = self.cargo()
        self.assertTrue(all(unit['fresh'] for unit in units), units)
        self.assertEqual(value, 'alpha:outside:one:one')

    def test_repeated_mixed_library_variants_restore_the_final_features(self):
        self.seed()
        state = artifacts.cache_begin(self.root)
        self.cargo(state, extra_args=['--features', 'cache_support/alternate'])
        self.cargo(state)
        self.assertTrue(artifacts.cache_stage(state, self.cache)['save'])
        self.checkout()
        self.assertTrue(artifacts.cache_restore(self.root, self.cache)['restored'])
        units, value = self.cargo()
        local = [unit for unit in units if Path(unit['manifest_path']).is_relative_to(self.root)]
        self.assertTrue(all(unit['fresh'] for unit in local), local)
        self.assertEqual(value, 'alpha:outside:one:one')


if __name__ == '__main__':
    unittest.main()
