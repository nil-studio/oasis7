#!/usr/bin/env python3
"""Real, offline Cargo fixtures for RG-A01 through RG-A04 (no Cargo stubs)."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('architecture', HERE / 'cargo-dev-architecture.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class CargoBoundaries(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='oasis7-architecture-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["root","middle","forbidden","other"]\n'
                   '[workspace.dependencies]\nrenamed={package="forbidden",path="forbidden"}\n')
        self.write('owner.md', '<a id="boundary"></a>\n')
        for name in ('root', 'middle', 'forbidden', 'other'):
            self.package(name)
        self.command('git', 'init', '-q')
        self.command('git', '-c', 'user.email=fixture@example.invalid', '-c', 'user.name=Fixture',
                     'commit', '--allow-empty', '-qm', 'fixture')
        self.rule = dict(key='fixture', upstream='owner.md#boundary',
                         roots=[dict(name='root', source=None)], forbidden=[dict(name='forbidden')],
                         kinds=['normal', 'build'], declaration_kinds=['normal', 'build'],
                         profiles=[dict(id='production', target='host', default_features=False,
                                        features=[], mode='production')], allows=[], exceptions=[])
        self.rule['required_profiles'] = ['production']

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def package(self, name, extra=''):
        self.write(f'{name}/Cargo.toml', f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2024"\n' + extra)
        self.write(f'{name}/src/lib.rs', '')

    def command(self, *args):
        return checker.run(self.root, list(args))

    def inspect(self, builds=True):
        self.command('cargo', 'generate-lockfile', '--offline')
        return checker.inspect(self.root, dict(revision=1, rules=[self.rule]), builds)

    def violations(self, report):
        self.assertFalse([r for r in report['results'] if r['status'] == 'inspection_error'], report)
        return [r for r in report['results'] if r['status'] == 'violation']

    def test_legal_graph_real_build(self):
        self.assertTrue(all(r['status'] == 'pass' for r in self.inspect()['results']))

    def test_direct_alias_workspace_optional_and_target_declarations(self):
        for table, value in [('[dependencies]', 'renamed={workspace=true,optional=true}'),
                             ("[target.'cfg(windows)'.dependencies]", 'renamed={workspace=true}'),
                             ("[target.'cfg(target_arch = \"wasm32\")'.dependencies]", 'renamed={workspace=true}'),
                             ('[build-dependencies]', 'renamed={workspace=true}')]:
            with self.subTest(table=table):
                self.package('root', table + '\n' + value + '\n')
                failures = self.violations(self.inspect())
                self.assertTrue(any(r['view'] == 'declaration' and r['edge']['alias'] == 'renamed'
                                    for r in failures), failures)

    def test_indirect_normal_and_host_build_paths(self):
        for kind in ('dependencies', 'build-dependencies'):
            with self.subTest(kind=kind):
                self.package('root', f'[{kind}]\nmiddle={{path="../middle"}}\n')
                if kind == 'build-dependencies':
                    self.write('root/build.rs', 'fn main() {}')
                self.package('middle', '[dependencies]\nforbidden={path="../forbidden"}\n')
                failures = self.violations(self.inspect())
                closures = [r for r in failures if r['view'] == 'closure']
                closures = [r for r in closures if 'path' in r]
                self.assertEqual(len(closures), 1, failures)
                self.assertEqual(len(closures[0]['path']), 3)
                self.assertEqual(closures[0]['edges'][0]['declarations'][0]['kind'],
                                 'build' if kind == 'build-dependencies' else 'normal')

    def test_optional_activation(self):
        self.package('root', '[dependencies]\nmiddle={path="../middle",optional=true}\n'
                     '[features]\nenable=["dep:middle"]\n')
        self.package('middle', '[dependencies]\nforbidden={path="../forbidden"}\n')
        self.assertFalse(self.violations(self.inspect()))
        self.rule['profiles'][0]['features'] = ['enable']
        self.assertTrue(self.violations(self.inspect()))

    def test_platform_activation_and_host_build(self):
        self.package('root', "[target.'cfg(windows)'.dependencies]\nmiddle={path=\"../middle\"}\n")
        self.package('middle', '[dependencies]\nforbidden={path="../forbidden"}\n')
        host = self.command('rustc', '-vV')
        native = self.violations(self.inspect())
        self.assertEqual(bool(native), 'windows' in host)
        # Cargo resolves target cfg without installing a foreign std. Builds
        # remain explicitly uncovered rather than forging cross-build success.
        for target in ('x86_64-pc-windows-msvc', 'wasm32-unknown-unknown'):
            self.rule['profiles'][0]['target'] = target
            report = self.inspect(builds=False)
            failures = self.violations(report)
            self.assertEqual(bool(failures), target.startswith('x86_64'))
            self.assertTrue(any(r['status'] == 'coverage_gap' for r in report['results']))

    def test_root_dev_isolation_and_exact_interoperability_edge(self):
        self.package('root', '[dev-dependencies]\nmiddle={path="../middle",default-features=false,features=["interop"]}\n')
        self.package('middle', '[features]\ninterop=[]\n[dependencies]\nforbidden={path="../forbidden"}\n')
        self.package('other', '[dev-dependencies]\nforbidden={path="../forbidden"}\n')
        self.assertFalse(self.violations(self.inspect()))
        self.rule['profiles'][0]['mode'] = 'test'
        self.assertTrue(self.violations(self.inspect()))
        self.rule['test_edges'] = [dict(root=dict(name='root',source=None),
                                       dependency=dict(name='middle',source=None), cfg=None,
                                       default_features=False, features=['interop'], kind='dev', target='host', profiles=['production'],
                                       purpose='real interoperability', upstream='owner.md#boundary')]
        self.assertFalse(self.violations(self.inspect()))
        self.package('root', '[dependencies]\nmiddle={path="../middle",default-features=false,features=["interop"]}\n')
        self.assertTrue(self.violations(self.inspect()))

    def test_dev_allowance_does_not_exempt_activated_normal_edge(self):
        self.package('root', '[dependencies]\nmiddle={path="../middle",optional=true}\n'
                     '[features]\nenable=["dep:middle"]\n'
                     '[dev-dependencies]\nmiddle={path="../middle",default-features=false,features=["interop"]}\n')
        self.package('middle', '[features]\ninterop=[]\n[dependencies]\nforbidden={path="../forbidden"}\n')
        self.rule['test_edges'] = [dict(root=dict(name='root',source=None),
                                       dependency=dict(name='middle',source=None), cfg=None,
                                       default_features=False, features=['interop'], kind='dev', target='host', profiles=['production'],
                                       purpose='real interoperability', upstream='owner.md#boundary')]
        self.assertFalse(self.violations(self.inspect()))
        self.rule['profiles'][0].update(mode='test', features=['enable'])
        self.assertTrue(self.violations(self.inspect()))

    def test_other_root_dev_features_do_not_activate_production(self):
        self.package('root', '[dependencies]\nmiddle={path="../middle"}\n')
        self.package('middle', '[dependencies]\nforbidden={path="../forbidden",optional=true}\n[features]\nactivate=["dep:forbidden"]\n')
        self.package('other', '[dev-dependencies]\nmiddle={path="../middle",features=["activate"]}\n')
        self.assertFalse(self.violations(self.inspect()))
        self.rule["profiles"][0]["mode"] = "test"
        self.assertFalse(self.violations(self.inspect()))

    def test_deduplicated_node_does_not_hide_unallowed_second_path(self):
        self.package('root', '[dev-dependencies]\nmiddle={path="../middle",default-features=false,features=["interop"]}\n'
                     'other={path="../other"}\n')
        self.package('middle', '[features]\ninterop=[]\n[dependencies]\nforbidden={path="../forbidden"}\n')
        self.package('other', '[dependencies]\nmiddle={path="../middle",default-features=false,features=["interop"]}\n')
        self.rule['test_edges'] = [dict(root=dict(name='root',source=None),
                                       dependency=dict(name='middle',source=None), cfg=None,
                                       default_features=False, features=['interop'], kind='dev', target='host', profiles=['production'],
                                       purpose='interoperability', upstream='owner.md#boundary')]
        self.rule['profiles'][0]['mode'] = 'test'
        failures = self.violations(self.inspect())
        self.assertTrue(failures)
        self.assertIn('other', failures[0]['path'][1])

    def test_dependency_dev_does_not_enter_selected_root_tests(self):
        self.package('root', '[dependencies]\nmiddle={path="../middle"}\n')
        self.package('middle', '[dev-dependencies]\nforbidden={path="../forbidden"}\n')
        self.rule['profiles'][0]['mode'] = 'test'
        self.assertFalse(self.violations(self.inspect()))

    def test_errors_and_coverage(self):
        config = dict(revision=1, rules=[self.rule])
        with self.assertRaisesRegex(checker.InspectionError, 'missing Cargo.lock'):
            checker.inspect(self.root, config)
        self.command('cargo', 'generate-lockfile', '--offline')
        self.rule['roots'][0]['name'] = 'absent'
        with self.assertRaisesRegex(checker.InspectionError, 'root identity'):
            checker.inspect(self.root, config)
        self.rule['roots'][0]['name'] = 'root'
        report = checker.inspect(self.root, config, selected=['absent'])
        self.assertEqual(report['results'][0]['status'], 'coverage_gap')
        self.rule['profiles'][0]['features'] = ['unregistered']
        self.assertEqual(self.inspect()['results'][0]['status'], 'coverage_gap')
        self.rule['profiles'] = []
        self.assertEqual(self.inspect()['results'][0]['status'], 'coverage_gap')
        self.rule['exceptions'] = ['allow_all_dev']
        with self.assertRaises(checker.InspectionError):
            self.inspect()

    def test_version_specific_forbidden_does_not_expand(self):
        self.package('root', '[dependencies]\nforbidden={path="../forbidden"}\n')
        self.rule['forbidden'][0]['version'] = '0.0.1'
        self.assertFalse(self.violations(self.inspect()))
        self.rule['forbidden'][0]['version'] = '0.1.0'
        self.assertTrue(self.violations(self.inspect()))

    def test_invalid_tree_is_error(self):
        with self.assertRaises(checker.InspectionError):
            checker.tree_nodes('not a graph', [])

    def test_legacy_host_scope_does_not_ban_foreign_target_declaration(self):
        self.package('root', "[target.'cfg(target_arch = \"wasm32\")'.dependencies]\nforbidden={path=\"../forbidden\"}\n")
        self.rule['forbidden'][0]['version'] = '0.1.0'
        self.rule['declaration_kinds'] = []  # Explicit legacy activated-graph scope.
        self.assertFalse(self.violations(self.inspect()))
        self.package('root', '[dependencies]\nforbidden={path="../forbidden"}\n')
        self.assertTrue(self.violations(self.inspect()))

    def test_provider_feature_names_are_exact(self):
        self.rule['forbidden'] = [dict(name='never_present')]
        self.rule['forbidden_feature_tokens'] = ['viewer', 'wasmtime', 'libp2p']
        self.package('middle', '[features]\nviewer_protocol=[]\npreviewer=[]\nviewer=[]\nwasmtime=[]\nlibp2p=[]\n')
        prefix = '[dependencies]\nmiddle={path="../middle",optional=true}\nviewer={package="forbidden",path="../forbidden",optional=true}\n'
        self.package('root', prefix + '[features]\npreview=["middle/viewer_protocol","middle/previewer"]\n')
        self.assertFalse(self.violations(self.inspect()))
        for reference in ('middle/viewer', 'middle?/viewer', 'dep:viewer', 'middle/wasmtime', 'middle/libp2p'):
            self.package('root', prefix + f'[features]\nforward=["{reference}"]\n')
            self.assertTrue(any(r['view'] == 'features' for r in self.violations(self.inspect())))

    def test_every_registered_rule_with_real_positive_and_alias_counterexample(self):
        rules = json.loads((HERE / 'cargo-dev-architecture.json').read_text(encoding='utf-8'))['rules']
        state = next(r for r in rules if r['key'] == 'world-state')
        self.assertIn(dict(name='wasmtime'), state['forbidden'])
        scenarios = [(rule, rule['forbidden'][0]) for rule in rules]
        scenarios.append((state, dict(name='wasmtime')))
        for rule, forbidden in scenarios:
            with self.subTest(rule=rule['key'], forbidden=forbidden['name']):
                rule = json.loads(json.dumps(rule))
                rule['upstream'] = 'owner.md#boundary'
                rule['profiles'] = [dict(id='production', target='host', default_features=True,
                                         features=[], mode='production')]
                rule['required_profiles'] = ['production']
                rule.pop('test_edges', None)
                self.rule = rule
                name = rule['roots'][0]['name']
                root_manifest = f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2024"\n[features]\ndefault=[]\n'
                self.write('root/Cargo.toml', root_manifest)
                self.write('forbidden/Cargo.toml', f'[package]\nname="{forbidden["name"]}"\nversion="{forbidden.get("version", "0.1.0")}"\nedition="2024"\n')
                self.assertFalse(self.violations(self.inspect()))
                self.write('root/Cargo.toml', root_manifest + f'[dependencies]\nbanned={{package="{forbidden["name"]}",path="../forbidden"}}\n')
                self.assertTrue(self.violations(self.inspect()))

    def test_registered_game_client_profiles_reject_transitive_dev_backend(self):
        rules = json.loads((HERE / 'cargo-dev-architecture.json').read_text(encoding='utf-8'))['rules']
        self.rule = next(r for r in rules if r['key'] == 'game-client')
        self.rule['upstream'] = 'owner.md#boundary'
        root_manifest = '[package]\nname="oasis7_game_client"\nversion="0.1.0"\nedition="2024"\n'
        self.write('root/Cargo.toml', root_manifest)
        self.write('forbidden/Cargo.toml', '[package]\nname="oasis7_runtime"\nversion="0.1.0"\nedition="2024"\n')
        self.package('middle', '[dependencies]\nruntime={package="oasis7_runtime",path="../forbidden"}\n')
        self.assertFalse(self.violations(self.inspect()))
        self.write('root/Cargo.toml', root_manifest + '[dev-dependencies]\nmiddle={path="../middle"}\n')
        failures = self.violations(self.inspect())
        self.assertTrue(any(r['view'] == 'closure' and r['profile']['mode'] == 'test'
                            for r in failures), failures)

    def test_violation_never_reports_closure_and_build_pass(self):
        self.package('root', '[dependencies]\nforbidden={path="../forbidden"}\n')
        report = self.inspect()
        self.assertTrue(self.violations(report))
        self.assertTrue(any(r['view'] == 'build' and r['status'] == 'pass' for r in report['results']))
        self.assertFalse(any(r.get('view') in ('closure', 'closure_and_build') and r['status'] == 'pass'
                             for r in report['results']))

    def test_production_only_test_boundary_still_builds_dev(self):
        self.package('root', '[dev-dependencies]\nforbidden={path="../forbidden"}\n')
        self.rule['profiles'][0]['mode'] = 'test'
        self.rule['test_kinds'] = ['normal', 'build']
        self.assertFalse(self.violations(self.inspect()))
        self.write('forbidden/src/lib.rs', 'compile_error!("dev must compile");')
        self.assertTrue(any(r['status'] == 'inspection_error' for r in self.inspect()['results']))
        self.write('forbidden/src/lib.rs', '')
        self.package('root', '[dependencies]\nforbidden={path="../forbidden"}\n')
        self.assertTrue(self.violations(self.inspect()))

    def test_trusted_graph_selection_tracks_features_dev_and_host_target(self):
        self.package('root', '[dependencies]\nmiddle={path="../middle",optional=true}\n'
                     '[features]\nenable=["dep:middle"]\n'
                     '[dev-dependencies]\nother={path="../other"}\n')
        self.rule['forbidden'] = [dict(name='absent')]
        self.rule['profiles'] += [dict(id='enabled', target='host', default_features=False,
                                      features=['enable'], mode='production'),
                                 dict(id='tests', target='host', default_features=False, features=[], mode='test')]
        self.rule['required_profiles'] = [p['id'] for p in self.rule['profiles']]
        self.command('cargo', 'generate-lockfile', '--offline')
        self.command('git', 'add', '.')
        self.command('git', '-c', 'user.email=fixture@example.invalid', '-c', 'user.name=Fixture', 'commit', '-qm', 'inputs')
        config = dict(revision=1, rules=[self.rule])
        def inspect(packages):
            plan = {'architecture': {'version': 1, 'mode': 'targeted', 'packages': packages, 'tested': 'HEAD'}}
            return checker.inspect(self.root, config, plan=plan)
        for package, expected in [('middle', {'enabled'}), ('other', {'tests'}), ('root', {'production','enabled','tests'}), ('forbidden', set())]:
            report = inspect([package])
            self.assertFalse(self.violations(report))
            built = {r['profile']['id'] for r in report['results'] if r.get('view') == 'build' and r['status'] == 'pass'}
            self.assertEqual(built, expected, report)
            self.assertEqual(report['timings']['graph_profiles'], 3)
        # Even a build exempted by trusted scope retains complete forbidden graph inspection.
        self.rule['forbidden'] = [dict(name='middle')]
        self.assertTrue(self.violations(inspect(['forbidden'])))
        diagnostic = checker.inspect(self.root, config, selected=['production'], plan={'architecture': {
            'version': 1, 'mode': 'targeted', 'packages': [], 'tested': 'HEAD'}})
        self.assertTrue(any(r['status'] == 'coverage_gap' for r in diagnostic['results']))
        self.assertEqual(checker.inspect(self.root, config, plan={})['timings']['build_profiles'], 3)

    def test_unregistered_required_profile_is_gap(self):
        self.rule['required_profiles'].append('required-wasm')
        self.assertTrue(any(r['status'] == 'coverage_gap' for r in self.inspect()['results']))

    def test_host_cfg_build_tools_survive_foreign_target(self):
        self.package('root', '[build-dependencies]\nmiddle={path="../middle"}\n')
        self.write('root/build.rs', 'fn main() {}')
        host_unix = 'windows' not in self.command('rustc', '-vV')
        self.package('middle', "[target.'cfg(unix)'.dependencies]\nforbidden={path=\"../forbidden\"}\n")
        self.rule['profiles'][0]['target'] = 'wasm32-unknown-unknown'
        failures = self.violations(self.inspect(builds=False))
        self.assertEqual(bool(failures), host_unix)

    def test_cli_parse_lock_offline_and_tool_failures_are_nonzero(self):
        config = dict(revision=1, rules=[self.rule])
        self.write('rules.json', json.dumps(config))
        command = [sys.executable, str(HERE / 'cargo-dev-architecture.py'),
                   '--repo-root', str(self.root), '--rules', str(self.root / 'rules.json')]
        def check_error(env=None):
            result = subprocess.run(command, env=env, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(result.stdout)['results'][0]['status'], 'inspection_error')
        check_error()  # missing lock
        self.command('cargo', 'generate-lockfile', '--offline')
        self.write('rules.json', '{broken')
        check_error()
        self.write('rules.json', json.dumps(config))
        check_error(dict(os.environ, PATH=''))  # missing actual tool
        self.package('root', '[dependencies]\nmissing={path="../missing"}\n')
        check_error()  # real Cargo resolver error
        self.package('root', '[dependencies]\nnot_cached_5023="=999.0.0"\n')
        check_error()  # real offline resolution failure


if __name__ == '__main__':
    unittest.main()
