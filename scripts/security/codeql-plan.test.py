#!/usr/bin/env python3
"""Local Git fixtures: these do not prove hosted extraction or scanning."""
import importlib.util
import json
import pathlib
import subprocess
import tempfile
import unittest
from types import SimpleNamespace

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('planner', HERE / 'codeql-plan.py')
planner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(planner)


class PlannerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.git('init', '-b', 'main')
        self.git('config', 'user.email', 'fixture@example.test')
        self.git('config', 'user.name', 'Fixture')
        self.write('Cargo.toml', '[workspace]\nmembers=[]\n')
        self.write('src/lib.rs', 'const S: &str = include_str!("../data/help.md");\n')
        self.write('data/help.md', 'embedded')
        self.write('docs/guide.md', 'ordinary')
        self.base = self.commit()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], stderr=subprocess.DEVNULL).decode().strip()

    def write(self, path, value='fixture'):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(value)

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-m', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def run_plan(self, **kw):
        args = dict(repo_root=str(self.root), policy=str(HERE / 'codeql-policy.json'), base=self.base,
                    head=self.git('rev-parse', 'HEAD'), checkout=None, mode='observe', event='pull_request',
                    default_branch='main', profile='default', full=False, pr_number=3, max_slots=2,
                    batch_modes=json.dumps({u + '/' + p: 'observe' if p == 'default' else 'baseline' for u, _ in planner.UNITS for p in ('default', 'extended')}))
        args.update(kw)
        return planner.plan(SimpleNamespace(**args))

    def test_CQ_T01_non_consumed_docs(self):
        self.write('docs/guide.md', 'changed'); self.commit()
        self.assertEqual(self.run_plan()['status'], 'not_applicable')

    def test_CQ_T02_T03_T04_T05_language_selection(self):
        for path, unit in [('tools/a.py', 'python-repo'), ('web/a.html', 'javascript-repo'),
                           ('src/new.rs', 'rust-repo'), ('.github/workflows/test.yml', 'actions-repo')]:
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path); self.commit()
                self.assertEqual(self.run_plan()['selected_units'], [unit])

    def test_CQ_T06_T12_policy_change_full_and_tamper_rejected(self):
        self.write('scripts/security/codeql-policy.json', '{}'); self.commit()
        self.assertEqual(len(self.run_plan()['selected_units']), 4)
        policy = json.loads((HERE / 'codeql-policy.json').read_text())
        policy['excluded_prefixes'].append('src/')
        self.write('candidate.json', json.dumps(policy))
        with self.assertRaisesRegex(ValueError, 'exclusion'):
            self.run_plan(policy=str(self.root / 'candidate.json'))

    def test_ci_required_scope_selects_its_python_and_actions_consumers(self):
        path = 'scripts/ci-required-scope.json'
        self.write(path, '{"groups":["baseline"]}\n')
        self.commit()
        plan = self.run_plan()
        self.assertEqual(plan['selected_units'], ['actions-repo', 'python-repo'])
        for unit in ('actions-repo', 'python-repo'):
            self.assertIn(path, plan['reasons'][unit])

    def test_ci_required_scope_does_not_register_neighboring_json_inputs(self):
        for path in ('scripts/ci-required-scope-v2.json', 'scripts/ci-unregistered.json',
                     'scripts/nested/ci-required-scope.json', 'scripts/ci-required-scope.json.extra'):
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path, '{}\n')
                self.commit()
                with self.assertRaisesRegex(ValueError, 'unregistered input'):
                    self.run_plan()

    def test_ci_required_scope_keeps_existing_rust_embedded_consumers(self):
        path = 'scripts/ci-required-scope.json'
        self.write(path, '{}\n')
        self.write('src/lib.rs', 'const S: &str = include_str!("../scripts/ci-required-scope.json");\n')
        self.base = self.commit()
        self.write(path, '{"groups":["baseline"]}\n')
        self.commit()
        plan = self.run_plan()
        self.assertEqual(plan['selected_units'], ['actions-repo', 'python-repo', 'rust-repo'])
        self.assertIn('embedded or possible build input: ' + path, plan['reasons']['rust-repo'])

    def test_document_corpus_inputs_select_python(self):
        key = '41' + '9' * 62
        for path in (
            'doc/.governance/document-corpus-inventory.json',
            'doc/testing/evidence/inventory.json',
            'doc/.governance/top-level-directory-registry.json',
            f'doc/.governance/document-corpus/objects/41/{key}.json',
            f'doc/.governance/document-corpus/semantic/entries/41/{key}.json',
            'doc/.governance/document-corpus/semantic/bundles/current-design.json',
            f'doc/.governance/document-corpus/evidence/entries/41/{key}.json',
            'doc/.governance/document-corpus/evidence/groups/current-evidence.json',
        ):
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path, '{}'); self.commit()
                result = self.run_plan()
                self.assertEqual(result['selected_units'], ['python-repo'])
                self.assertIn(path, result['reasons']['python-repo'])

    def test_document_corpus_mixed_pr_preserves_rust_hold(self):
        path = ('doc/.governance/document-corpus/objects/41/'
                '4194b076a574aeb5fa9511d1bbda2855a131189cb3500fb5c3d2a28a7740f87d.json')
        self.write(path, '{}')
        self.write('web/viewer.js', 'export const view = 1;')
        self.write('build.rs', 'fn main() {}')
        self.commit()
        batches = json.dumps({u + '/default': 'observe' for u in
                              ('actions-repo', 'python-repo', 'javascript-repo')})
        result = self.run_plan(batch_modes=batches)
        self.assertEqual(result['selected_units'], ['python-repo', 'javascript-repo'])
        self.assertEqual(result['not_selected_reasons']['rust-repo'], 'batch_disabled')
        self.assertIn('embedded or possible build input: ' + path, result['reasons']['rust-repo'])

    def test_document_corpus_rename_delete_and_embedded_consumers(self):
        path = 'doc/.governance/document-corpus-inventory.json'
        self.write(path, '{}')
        self.write('src/lib.rs', 'const S: &str = include_str!("../' + path + '");')
        self.base = self.commit()
        (self.root / path).rename(self.root / 'moved.js'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'],
                         ['python-repo', 'javascript-repo', 'rust-repo'])
        self.write(path, '{}'); self.base = self.commit()
        (self.root / path).unlink(); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], ['python-repo', 'rust-repo'])

    def test_document_corpus_does_not_register_unknown_json(self):
        for path in (
            'doc/.governance/unknown.json',
            'doc/.governance/document-corpus/unknown.json',
            'doc/.governance/document-corpus/objects/41/not-a-shard.json',
            'doc/.governance/document-corpus/semantic/bundles/nested/record.json',
            'doc/testing/evidence/unregistered.json',
            'web/data.json',
        ):
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path, '{}'); self.commit()
                with self.assertRaisesRegex(ValueError, 'unregistered input'):
                    self.run_plan()

    def test_CQ_T07_T08_all_manifest_inventory_and_delete(self):
        self.write('standalone/Cargo.toml', '[package]\nname="standalone"\nversion="0.1.0"\n')
        self.write('excluded/Cargo.toml', '[package]\nname="excluded"\nversion="0.1.0"\n')
        self.write('vendor-patch/Cargo.toml', '[package]\nname="patch"\nversion="0.1.0"\n')
        self.base = self.commit()
        (self.root / 'standalone/Cargo.toml').unlink(); self.commit()
        result = self.run_plan()
        self.assertIn('standalone/Cargo.toml', result['completeness']['manifest_inventory']['base'])
        self.assertNotIn('standalone/Cargo.toml', result['completeness']['manifest_inventory']['head'])
        self.assertIn('excluded/Cargo.toml', result['completeness']['manifest_inventory']['head'])
        self.assertIn('vendor-patch/Cargo.toml', result['completeness']['manifest_inventory']['head'])
        self.assertEqual(result['selected_units'], ['rust-repo'])

    def test_wasm_candidate_bundle_selects_rust_and_python_consumers(self):
        root = 'crates/oasis7_runtime/src/world/artifacts/'
        names = ['builtin_wasm_candidate_baseline.v1.json']
        names += [f'{group}_builtin_modules.source-rule2-candidate.{suffix}'
                  for group in ('m1', 'm4', 'm5') for suffix in ('identity.json', 'sha256')]
        for name in names:
            with self.subTest(name=name):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(root + name); self.commit()
                result = self.run_plan()
                self.assertEqual(result['selected_units'], ['python-repo', 'rust-repo'])
                self.assertIn(root + name, result['reasons']['python-repo'])
                self.assertIn(root + name, result['reasons']['rust-repo'])

    def test_wasm_candidate_registration_preserves_non_rust_batches(self):
        self.write('crates/oasis7_runtime/src/world/artifacts/builtin_wasm_candidate_baseline.v1.json')
        self.commit()
        batches = json.dumps({unit + '/default': 'observe' for unit in
                              ('actions-repo', 'python-repo', 'javascript-repo')})
        result = self.run_plan(batch_modes=batches)
        self.assertEqual(result['candidate_units'], ['python-repo', 'rust-repo'])
        self.assertEqual(result['status'], 'selected')
        self.assertEqual(result['selected_units'], ['python-repo'])
        self.assertEqual([row['unit'] for row in result['matrix']['include']], ['python-repo'])
        self.assertEqual(result['not_selected_reasons']['rust-repo'], 'batch_disabled')
        self.write('scripts/probe.py')
        self.write('web/app.js')
        self.write('.github/workflows/test.yml')
        self.commit()
        result = self.run_plan(batch_modes=batches)
        self.assertEqual(result['selected_units'], ['actions-repo', 'python-repo', 'javascript-repo'])
        self.assertEqual(result['not_selected_reasons']['rust-repo'], 'batch_disabled')

    def test_wasm_candidate_rename_and_delete_keep_old_consumers(self):
        path = 'crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.source-rule2-candidate.identity.json'
        self.write(path, 'unique candidate fixture'); self.base = self.commit()
        (self.root / path).rename(self.root / 'moved.js'); self.commit()
        result = self.run_plan()
        self.assertEqual(result['selected_units'], ['python-repo', 'javascript-repo', 'rust-repo'])
        self.assertIn(path, result['completeness']['diff_paths'])
        self.assertIn('moved.js', result['completeness']['diff_paths'])
        self.write(path); self.base = self.commit()
        (self.root / path).unlink(); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], ['python-repo', 'rust-repo'])

    def test_wasm_candidate_registration_does_not_accept_other_artifacts(self):
        root = 'crates/oasis7_runtime/src/world/artifacts/'
        for path in (
            root + 'builtin_wasm_candidate_baseline.v2.json',
            root + 'm2_builtin_modules.source-rule2-candidate.identity.json',
            root + 'm1_builtin_modules.source-rule2-candidate.identity.json.extra',
            root + 'm4_builtin_modules.source-rule2-candidate.sha256.tmp',
            root + 'm1_builtin_modules.identity.json',
            root + 'nested/builtin_wasm_candidate_baseline.v1.json',
            'web/builtin_wasm_candidate_baseline.v1.json',
        ):
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path); self.commit()
                with self.assertRaisesRegex(ValueError, 'unregistered input'):
                    self.run_plan()

    def test_wasm_candidate_symlink_still_requires_review(self):
        path = self.root / 'crates/oasis7_runtime/src/world/artifacts/builtin_wasm_candidate_baseline.v1.json'
        path.parent.mkdir(parents=True)
        path.symlink_to(self.root / 'src/lib.rs')
        self.commit()
        with self.assertRaisesRegex(ValueError, 'changed symbolic link'):
            self.run_plan()

    def test_CQ_T09_rename_both_sides_special_paths(self):
        self.write('old file\n$.py'); self.base = self.commit()
        (self.root / 'old file\n$.py').rename(self.root / 'new file\n$.js'); self.commit()
        result = self.run_plan()
        self.assertEqual(result['selected_units'], ['python-repo', 'javascript-repo'])
        self.assertEqual(len(result['completeness']['diff_paths']), 2)

    def test_CQ_T10_missing_identity(self):
        with self.assertRaisesRegex(ValueError, 'unreadable'):
            self.run_plan(base='0' * 40)

    def test_CQ_T11_complete_over_300_diff(self):
        for i in range(350): self.write(f'docs/{i}.md')
        self.write('last.py'); self.commit()
        result = self.run_plan()
        self.assertEqual(len(result['completeness']['diff_paths']), 351)
        self.assertEqual(result['selected_units'], ['python-repo'])

    def test_CQ_T13_embedded_document_and_build_data(self):
        self.write('data/help.md', 'changed'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], ['rust-repo'])
        self.base = self.git('rev-parse', 'HEAD')
        self.write('build.rs', 'fn main() {}'); self.base = self.commit()
        self.write('web/data.json', '{}'); self.commit()
        with self.assertRaisesRegex(ValueError, 'unregistered input'): self.run_plan()

    def test_possible_rust_consumption_does_not_register_unknown_paths(self):
        self.write('build.rs', 'fn main() {}')
        self.write('web/app.js', 'import data from "./data.json";')
        self.base = self.commit()
        for path in ('new.config', 'web/data.json'):
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path, '{}'); self.commit()
                with self.assertRaisesRegex(ValueError, 'unregistered input'): self.run_plan()

    def test_CQ_T13_build_script_text_and_unknown_inputs(self):
        self.write('build.rs', 'fn main() { std::fs::read_to_string("docs/generated.txt").unwrap(); }')
        self.write('docs/generated.txt', 'before'); self.base = self.commit()
        for path in ('docs/generated.txt', 'docs/guide.md', 'docs/input.rst', 'docs/input.adoc', 'tools/input.py', 'tools/input.sh'):
            with self.subTest(path=path):
                self.base = self.git('rev-parse', 'HEAD')
                self.write(path, 'changed'); self.commit()
                self.assertIn('rust-repo', self.run_plan()['selected_units'])

    def test_CQ_T13_deleted_build_script_old_side_consumer(self):
        self.write('build.rs', 'fn main() { std::fs::read_to_string("docs/guide.md").unwrap(); }')
        self.base = self.commit()
        (self.root / 'build.rs').unlink()
        self.write('docs/guide.md', 'changed'); self.commit()
        self.assertIn('embedded or possible build input: docs/guide.md', self.run_plan()['reasons']['rust-repo'])

    def test_CQ_T05_workflow_shell_boundary_and_independent_shell(self):
        self.write('scripts/independent.sh', 'echo standalone'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], [])
        self.write('.github/workflows/test.yml', 'jobs:\n  check:\n    steps:\n      - run: ./scripts/check.sh\n')
        self.write('scripts/check.sh', 'echo before'); self.base = self.commit()
        self.write('scripts/check.sh', 'echo after'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], ['actions-repo'])

    def test_CQ_T05_unknown_action_shell_linkage_old_side(self):
        self.write('local/action.yml', 'runs:\n  using: composite\n  steps:\n    - shell: bash\n      run: $SCRIPT_PATH\n')
        self.write('scripts/check.sh', 'echo before'); self.base = self.commit()
        (self.root / 'local/action.yml').unlink()
        self.write('scripts/check.sh', 'echo after'); self.commit()
        self.assertIn('workflow or possible action input: scripts/check.sh', self.run_plan()['reasons']['actions-repo'])

    def test_CQ_T14_vendor_and_exclusions(self):
        self.write('third_party/a.rs'); self.write('target/out.rs'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], [])
        self.base = self.git('rev-parse', 'HEAD')
        self.write('vendor-patched/a.rs'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], ['rust-repo'])

    def test_CQ_T15_T37_modes_profiles_and_slots(self):
        self.write('a.py'); self.commit()
        self.assertEqual(self.run_plan(mode='off')['status'], 'disabled')
        self.assertEqual(self.run_plan(mode='baseline')['status'], 'disabled')
        result = self.run_plan(profile='extended', max_slots=1, event='schedule')
        row = result['matrix']['include'][0]
        self.assertEqual(row['category'], 'oasis7/python-repo/extended')
        self.assertEqual(row['queries'], 'security-extended')
        self.assertEqual(row['slot'], 0)
        self.assertEqual(len(self.run_plan(event='schedule', mode='baseline', full=True)['selected_units']), 4)

    def test_unknown_and_symlink_fail_closed(self):
        self.write('new.config'); self.commit()
        with self.assertRaisesRegex(ValueError, 'unregistered input'): self.run_plan()
        self.base = self.git('rev-parse', 'HEAD')
        (self.root / 'link.py').symlink_to('/outside'); self.commit()
        with self.assertRaisesRegex(ValueError, 'symbolic link'): self.run_plan()

    def test_maintenance_scope_keeps_full_candidates_and_batch_gates(self):
        units = [u for u, _ in planner.UNITS]
        for scope, expected in [('all', units), ('non-rust', units[:-1]), ('rust', units[-1:])]:
            with self.subTest(scope=scope):
                result = self.run_plan(event='schedule', full=True, maintenance_scope=scope)
                self.assertEqual(result['candidate_units'], units)
                self.assertEqual(result['selected_units'], expected)
                self.assertEqual(result['maintenance_scope'], scope)
                for unit in set(units) - set(expected):
                    self.assertEqual(result['not_selected_reasons'][unit], 'maintenance_scope')
        result = self.run_plan(event='schedule', full=True, maintenance_scope='rust',
                               batch_modes='{"python-repo/default":"observe"}')
        self.assertEqual(result['status'], 'disabled')
        self.assertEqual(result['not_selected_reasons']['rust-repo'], 'batch_disabled')

    def test_rust_budget_is_only_default_maintenance(self):
        for event, profile, expected in [('pull_request', 'default', 45),
                ('schedule', 'default', 90), ('workflow_dispatch', 'default', 90),
                ('schedule', 'extended', 45)]:
            with self.subTest(event=event, profile=profile):
                result = self.run_plan(event=event, profile=profile, full=True)
                rows = {row['unit']: row for row in result['matrix']['include']}
                self.assertEqual(rows['rust-repo']['timeout_minutes'], expected)
                self.assertEqual(rows['python-repo']['timeout_minutes'], 45 if profile == 'extended' else 20)
        result = self.run_plan(full=True, batch_modes='{"rust-repo/default":"baseline"}')
        self.assertEqual(result['selected_units'], [])

    def test_maintenance_scope_cannot_change_pr_coverage(self):
        for scope in ('rust', 'non-rust', 'invalid'):
            with self.subTest(scope=scope), self.assertRaisesRegex(ValueError, 'maintenance scope'):
                self.run_plan(full=True, maintenance_scope=scope)

    def test_batches_default_disabled_and_single_trial(self):
        self.write("tools/a.py"); self.commit()
        result = self.run_plan(batch_modes="{}")
        self.assertEqual(result["status"], "disabled")
        self.assertEqual(result["candidate_units"], ["python-repo"])
        self.assertEqual(result["not_selected_reasons"]["python-repo"], "batch_disabled")
        config = json.dumps({"python-repo/default": "baseline"})
        self.assertEqual(self.run_plan(batch_modes=config)["status"], "disabled")
        result = self.run_plan(batch_modes=config, mode="baseline", event="workflow_dispatch", full=True)
        self.assertEqual(result["selected_units"], ["python-repo"])
        self.assertEqual(result["batch_modes"], {"python-repo/default": "baseline"})
        self.assertNotIn("accepted", result)
        self.assertEqual(result["matrix"]["include"][0]["category"], "oasis7/python-repo/default")

    def test_cli_missing_batch_option_defaults_to_no_scans(self):
        self.write("tools/a.py"); head = self.commit()
        output = self.root / "plan-output.json"
        result = subprocess.run(['python3', str(HERE / 'codeql-plan.py'), '--repo-root', str(self.root),
            '--policy', str(HERE / 'codeql-policy.json'), '--base', self.base, '--head', head,
            '--output', str(output), '--mode', 'baseline', '--event', 'workflow_dispatch', '--full'],
            capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(output.read_text())
        self.assertEqual(value['batch_modes'], {})
        self.assertEqual(value['status'], 'disabled')
        self.assertEqual(value['matrix']['include'], [])

    def test_batches_global_ceiling_profiles_and_config_routing(self):
        self.write("scripts/security/config.py"); self.commit()
        config = json.dumps({"python-repo/default": "observe", "rust-repo/default": "baseline", "python-repo/extended": "baseline"})
        result = self.run_plan(batch_modes=config)
        self.assertEqual(len(result["candidate_units"]), 4)
        self.assertEqual(result["selected_units"], ["python-repo"])
        self.assertEqual(result["not_selected_reasons"]["rust-repo"], "pr_baseline_only")
        self.assertEqual(self.run_plan(batch_modes=config, mode="off")["selected_units"], [])
        self.assertEqual(self.run_plan(batch_modes=config, mode="baseline")["selected_units"], [])
        result = self.run_plan(batch_modes=config, event="schedule", profile="extended", full=True)
        self.assertEqual(result["selected_units"], ["python-repo"])
        self.assertEqual(result["matrix"]["include"][0]["queries"], "security-extended")

    def test_batch_configuration_rejects_untrusted_shapes(self):
        for config in ("[]", "null", "{", "{\"python-repo/default\":\"observe\",\"python-repo/default\":\"baseline\"}",
                       json.dumps({"unknown/default": "baseline"}), json.dumps({"python-repo/default": True}),
                       json.dumps({"python-repo/extended": "observe"}), json.dumps({"python-repo/default": "off"})):
            with self.subTest(config=config), self.assertRaises(ValueError): self.run_plan(batch_modes=config)

    def test_dynamic_include_and_invalid_cli_configuration(self):
        self.write('src/lib.rs', 'include_bytes!(concat!(env!("OUT_DIR"), "/data"));')
        self.base = self.commit()
        self.write('docs/guide.md', 'possible input'); self.commit()
        self.assertEqual(self.run_plan()['selected_units'], ['rust-repo'])
        result = subprocess.run(['python3', str(HERE / 'codeql-plan.py'), '--mode', 'invalid'], capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'invalid choice', result.stderr)


if __name__ == '__main__': unittest.main()
