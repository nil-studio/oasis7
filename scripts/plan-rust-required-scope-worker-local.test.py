#!/usr/bin/env python3
import contextlib, importlib.util, inspect, io, json, os, subprocess, sys, tempfile, unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('selector', HERE / 'plan-rust-required-scope.py')
selector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selector)


class WorkerCompileProfiles(unittest.TestCase):
    package='oasis7_custody_worker'
    root='crates/oasis7_custody_worker/src/lib.rs'
    def policy(self,body,*,profile=True,package=None,parent=None):
        package=package or self.package
        root=self.root if package==self.package else 'crates/other/src/lib.rs'
        before='pub fn old() {}\n'
        sources={root:body.encode()}
        path=root
        if parent is not None:
            sources[root]=parent.encode();path=root.replace('lib.rs','leaf.rs');sources[path]=body.encode()
        def read(*args):
            ref,name=args[1].split(':',1)
            return (before if ref=='base' else sources[name].decode()).encode()
        profile_args=({'worker_wasm_compile':profile}
                      if 'worker_wasm_compile' in inspect.signature(selector.changed_test_policy).parameters else {})
        with patch.object(selector,'git',side_effect=read):
            return selector.changed_test_policy('base','head',[path],{path:{package}}, {package},
                {package:{'identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','optional_future'}},
                ordinary_roots={root},source_inputs={'sources':sources,'directories':{package:str(Path(root).parent.parent)},
                    'manifests':{package:{'package':{'name':package},'features':{name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','optional_future')}}},'default_features':{package:set()}},
                **profile_args)
    def test_six_separate_commands_have_no_default_features(self):
        commands=[command for command,error in selector.worker_wasm_compile_checks({})];self.assertEqual(len(commands),6)
        for command in commands:
            self.assertIn('--no-default-features',command)
            enabled=command[command.index('--features')+1].split(',')
            self.assertEqual(len(set(enabled)&{'owner','identity','executor','recovery-fence'}),1)
            self.assertIn('wasm32-unknown-unknown',command)
    def test_known_owner_wasm_production_cfg_has_actual_compile_profile(self):
        result=self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { pub fn product() {} }')
        self.assertIn(self.root,result[2])
    def test_existing_native_harness_profile_is_separate(self):
        self.policy('#[cfg(all(target_arch="wasm32",feature="identity",feature="native-runtime-harness"))] mod test_session { pub fn product() {} }')
    def test_no_profile_keeps_previous_rejection(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { pub fn product() {} }',profile=False)
    def test_mutually_exclusive_roles_are_not_unioned(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner",feature="identity"))] mod test_session { pub fn product() {} }')
    def test_unknown_feature_does_not_gain_driver(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="optional_future"))] mod test_session { pub fn product() {} }')
    def test_compile_profile_does_not_authorize_wasm_test(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] #[test] fn hidden() {}')
    def test_compile_profile_does_not_authorize_ignored_test(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[test] #[ignore] fn hidden() {}')
    def test_other_package_cannot_borrow_worker_profile(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { pub fn product() {} }',package='other')
    def test_nested_identity_parent_cannot_borrow_owner_leaf(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { pub fn product() {} }',parent='#[cfg(all(target_arch="wasm32",feature="identity"))] mod leaf;')


class WorkerFutureSignerCompileContract(unittest.TestCase):
    def manifest(self):
        return {'features':{'default':[],**{name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','execution-signer-harness')}}}
    def test_absent_future_feature_has_only_six_existing_profiles(self):
        self.assertEqual(len(selector.worker_wasm_compile_checks({})),6)
    def test_declared_empty_default_off_has_seven_success_and_three_negative_checks(self):
        checks=selector.worker_wasm_compile_checks(self.manifest())
        self.assertEqual(len(checks),10)
        success=[command for command,error in checks if error is None]
        self.assertEqual(len(success),7)
        self.assertEqual(success[-1][success[-1].index('--features')+1],'executor,execution-signer-harness')
        self.assertEqual({command[command.index('--features')+1] for command,error in checks if error},
                         {'identity,execution-signer-harness','owner,execution-signer-harness','recovery-fence,execution-signer-harness'})
        self.assertTrue(all(error==selector.WORKER_SIGNER_GUARD_MESSAGE for command,error in checks if error))
    def test_default_enabled_or_dependency_adding_feature_is_rejected(self):
        for features in ({'default':['execution-signer-harness'],'execution-signer-harness':[]},
                         {'default':['wrapper'],'wrapper':['execution-signer-harness'],'execution-signer-harness':[]},
                         {'execution-signer-harness':['owner']}):
            with self.assertRaises(selector.TestPolicyError):
                selector.worker_wasm_compile_checks({'features':features})
    def guard(self,tail=None):
        return '#[cfg(all(feature="execution-signer-harness",not(feature="executor")))] '+(tail or 'compile_error!("'+selector.WORKER_SIGNER_GUARD_MESSAGE+'");')
    def attribute(self):
        tokens=selector.rust_source_inputs(self.guard(),return_tokens=True)[0]
        values=[token[1] for token in tokens]
        return tuple(values[2:values.index(']')])
    def test_only_literal_single_compile_error_guard_is_recognized(self):
        self.assertTrue(selector.worker_exact_signer_guard(self.guard(),self.attribute(),('unresolved','',0)))
        for source in (self.guard('fn arbitrary_code() {}'),self.guard()+' '+self.guard(),
                       self.guard('compile_error!("wrong reason");')):
            self.assertFalse(selector.worker_exact_signer_guard(source,self.attribute(),('unresolved','',0)))
        self.assertFalse(selector.worker_exact_signer_guard(self.guard(),self.attribute(),('fn','arbitrary_code',0)))


class WorkerCompileDriverContract(unittest.TestCase):
    def test_missing_feature_runs_six_and_declared_runs_all_ten_checks(self):
        from types import SimpleNamespace
        base={name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness')}
        for manifest,count in (({'features':base},6),({'features':{**base,'execution-signer-harness':[]}},10)):
            calls=[]
            def runner(command,**kwargs):
                calls.append((command,kwargs))
                return SimpleNamespace(returncode=1,stdout=selector.WORKER_SIGNER_GUARD_MESSAGE)
            with contextlib.redirect_stdout(io.StringIO()):
                selector.run_worker_wasm_compile_checks(manifest,runner)
            self.assertEqual(len(calls),count)
    def test_negative_unexpected_success_or_wrong_failure_is_rejected(self):
        from types import SimpleNamespace
        for code,text in ((0,selector.WORKER_SIGNER_GUARD_MESSAGE),(1,'unrelated build failure')):
            def runner(command,**kwargs):
                return SimpleNamespace(returncode=code,stdout=text)
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(ValueError):
                selector.run_worker_wasm_compile_checks({'features':{name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','execution-signer-harness')}},runner)
    def test_positive_build_failure_is_propagated(self):
        def runner(command,**kwargs):
            raise subprocess.CalledProcessError(101,command)
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(subprocess.CalledProcessError):
            selector.run_worker_wasm_compile_checks({'features':{name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness')}},runner)


class WorkerPolicyBootstrap(unittest.TestCase):
    def test_ci_only_upgrade_does_not_rescan_unmodified_worker_rust_cfg(self):
        paths=['scripts/plan-rust-required-scope.py','scripts/ci-tests.sh',
               'scripts/plan-rust-required-scope.test.py',
               'doc/testing/ci/ci-tiered-execution.design.md']
        with patch.object(selector,'git',side_effect=AssertionError('no Rust source lookup')):
            self.assertEqual(selector.changed_test_policy('base','source',paths,{},set(),{}),([],False,[]))


class WorkerSignerGuardCurrentAttributeBinding(unittest.TestCase):
    ROOT='crates/oasis7_custody_worker/src/lib.rs'
    def guard(self):
        return '#[cfg(all(feature="execution-signer-harness",not(feature="executor")))] compile_error!("'+selector.WORKER_SIGNER_GUARD_MESSAGE+'");'
    def policy(self,extra):
        source=self.guard()+'\n'+extra
        package='oasis7_custody_worker'
        manifest={'package':{'name':package},'features':{name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','execution-signer-harness')}}
        def read(*args):
            return ('' if args[1].startswith('base:') else source).encode()
        with patch.object(selector,'git',side_effect=read):
            return selector.changed_test_policy('base','head',[self.ROOT],{self.ROOT:{package}}, {package},
                {package:{'identity','owner','executor','recovery-fence','execution-signer-harness'}},
                ordinary_roots={self.ROOT},source_inputs={'sources':{self.ROOT:source.encode()},
                    'directories':{package:'crates/oasis7_custody_worker'},'manifests':{package:manifest},
                    'default_features':{package:set()}},worker_wasm_compile=True)
    def test_legitimate_guard_elsewhere_cannot_authorize_hidden_wasm_test_macro(self):
        extra='macro_rules! hidden { () => { #[test] fn hidden_test() {} }; }\n#[cfg(all(test,target_arch="wasm32",feature="owner",feature="execution-signer-harness"))]\nhidden!();'
        with self.assertRaises(selector.TestPolicyError):self.policy(extra)
    def test_other_macro_and_cfg_attr_cannot_borrow_legitimate_guard(self):
        for extra in (
            '#[cfg(all(test,target_arch="wasm32",feature="owner",feature="execution-signer-harness"))] other_hidden!();',
            '#[cfg(all(test,target_arch="wasm32",feature="executor",feature="execution-signer-harness"))] hidden!();',
            '#[cfg(all(test,target_arch="wasm32",feature="owner",feature="execution-signer-harness"))] #[test] fn hidden_function() {}'):
            with self.subTest(extra=extra),self.assertRaises(selector.TestPolicyError):self.policy(extra)
    def test_exact_guard_still_recognized_only_at_current_attribute(self):
        guard=self.guard()
        values=[token[1] for token in selector.rust_source_inputs(guard,return_tokens=True)[0]]
        canonical=tuple(values[2:values.index(']')])
        self.assertTrue(selector.worker_exact_signer_guard(guard,canonical,('unresolved','',0)))
        unrelated=('cfg','(','test',')')
        self.assertFalse(selector.worker_exact_signer_guard(guard+' #[cfg(test)] hidden!();',unrelated,('unresolved','',0)))
        self.assertFalse(selector.worker_exact_signer_guard(guard+' '+guard,canonical,('unresolved','',0)))


class WorkerActualLocalFeatureClosure(unittest.TestCase):
    ROOT='crates/oasis7_custody_worker/src/lib.rs'
    def manifest(self):
        return {'features':{'default':[],**{name:[] for name in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','hidden')}}}
    def policy(self,local,*,parent=None):
        package='oasis7_custody_worker';manifest=self.manifest();manifest['features']['owner']=['hidden']
        root=self.ROOT;path=root;sources={root:local.encode()}
        if parent is not None:
            path=root.replace('lib.rs','leaf.rs');sources={root:parent.encode(),path:local.encode()}
        def read(*args):return ('' if args[1].startswith('base:') else sources[path].decode()).encode()
        with patch.object(selector,'git',side_effect=read):
            return selector.changed_test_policy('base','head',[path],{path:{package}},{package},{package:set(manifest['features'])},
                ordinary_roots={root},source_inputs={'sources':sources,'directories':{package:'crates/oasis7_custody_worker'},
                    'manifests':{package:manifest},'default_features':{package:set()}},worker_wasm_compile=True)
    def test_owner_alias_excludes_module_under_every_real_successful_tuple(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner",not(feature="hidden")))] mod test_session_hidden { fn broken() { missing_function(); } }')
    def test_same_actual_closure_checks_parent_and_child(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",not(feature="hidden")))] mod test_session_leaf { fn product() {} }',
                        parent='#[cfg(all(target_arch="wasm32",feature="owner"))] mod leaf;')
        self.policy('#[cfg(all(target_arch="wasm32",feature="hidden"))] mod test_session_leaf { fn product() {} }',
                    parent='#[cfg(all(target_arch="wasm32",feature="owner"))] mod leaf;')
    def test_explicit_initial_closure_never_injects_default_and_handles_local_cycle(self):
        manifest=self.manifest();manifest['features'].update({'default':['hidden'],'owner':['a'],'a':['b'],'b':['a']})
        self.assertEqual(selector.worker_local_feature_closure(manifest,('owner',)),{'owner','a','b'})
        self.assertEqual(selector.worker_local_feature_closure(manifest,('default',)),{'default','hidden'})
    def test_dependency_edges_never_invent_local_feature_activation(self):
        manifest=self.manifest();manifest['dependencies']={'core':{'path':'../core'},'codec':{'version':'1','optional':True}}
        manifest['features']['owner']=['core/x','dep:codec']
        self.assertEqual(selector.worker_local_feature_closure(manifest,('owner',)),{'owner'})
        for edge in ('codec/x','codec?/x','missing/x','missing','dep:core'):
            manifest['features']['owner']=[edge]
            with self.subTest(edge=edge),self.assertRaises(selector.TestPolicyError):
                selector.worker_local_feature_closure(manifest,('owner',))
    def test_future_signer_cannot_be_enabled_indirectly_by_any_role_or_harness(self):
        for role in ('identity','owner','executor','recovery-fence','approval-harness','native-runtime-harness','default'):
            manifest=self.manifest();manifest['features'].update({'execution-signer-harness':[],'bridge':['execution-signer-harness'],role:['bridge']})
            with self.subTest(role=role),self.assertRaises(selector.TestPolicyError):
                selector.worker_wasm_compile_features(manifest)
    def test_foreign_feature_with_same_name_is_not_local_signer_opt_in(self):
        manifest=self.manifest();manifest['features'].update({'execution-signer-harness':[],'owner':['core/execution-signer-harness']})
        manifest['dependencies']={'core':{'path':'../core'}}
        self.assertEqual(len(selector.worker_wasm_compile_features(manifest)),7)
        self.assertNotIn('execution-signer-harness',selector.worker_local_feature_closure(manifest,('owner',)))


class WorkerApplicableCfgConjunction(unittest.TestCase):
    package = 'oasis7_custody_worker'
    root = 'crates/oasis7_custody_worker/src/lib.rs'

    def policy(self, body, before='pub fn old() {}', parent=None, leaf='leaf.rs'):
        path = self.root if parent is None else self.root.replace('lib.rs', leaf)
        sources = {self.root: (body if parent is None else parent).encode()}
        sources[path] = body.encode()
        features = {name: [] for name in ('identity', 'owner', 'executor', 'recovery-fence',
                                         'approval-harness', 'native-runtime-harness')}
        def read(*args):
            ref, name = args[1].split(':', 1)
            return before.encode() if ref == 'base' else sources[name]
        with patch.object(selector, 'git', side_effect=read):
            return selector.changed_test_policy(
                'base', 'head', [path], {path: {self.package}}, {self.package},
                {self.package: set(features)}, ordinary_roots={self.root},
                source_inputs={'sources': sources,
                    'directories': {self.package: 'crates/oasis7_custody_worker'},
                    'manifests': {self.package: {'features': features}},
                    'default_features': {self.package: set()}}, worker_wasm_compile=True)

    def test_stacked_attributes_need_one_actual_tuple_in_both_orders(self):
        gates = ['#[cfg(all(target_arch="wasm32",feature="identity"))]',
                 '#[cfg(feature="owner")]']
        for attrs in (gates, gates[::-1]):
            with self.subTest(attrs=attrs), self.assertRaises(selector.TestPolicyError):
                self.policy(' '.join(attrs) + ' mod test_session { fn broken() { missing_function(); } }')

    def test_inline_and_file_inner_ancestors_apply_to_current_owner(self):
        for body in (
            '#[cfg(feature="identity")] mod outer { #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } } }',
            '#![cfg(feature="identity")] #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } }',
        ):
            with self.subTest(body=body), self.assertRaises(selector.TestPolicyError):
                self.policy(body)

    def test_unchanged_inline_and_file_gates_still_constrain_new_child(self):
        for before, body in (
            ('#[cfg(feature="identity")] mod outer {}', '#[cfg(feature="identity")] mod outer { #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } } }'),
            ('#![cfg(feature="identity")] pub fn old() {}', '#![cfg(feature="identity")] #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } }'),
        ):
            with self.subTest(before=before), self.assertRaises(selector.TestPolicyError):
                self.policy(body, before)

    def test_mount_and_local_gates_cannot_use_different_tuples(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } }',
                        parent='#[cfg(feature="identity")] mod leaf;')

    def test_complete_alternative_route_and_same_tuple_controls(self):
        body = '#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn ok() {} }'
        self.policy(body, parent='#[cfg(feature="identity")] #[path="leaf.rs"] mod identity_leaf; #[cfg(feature="owner")] #[path="leaf.rs"] mod owner_leaf;')
        self.policy('#[cfg(target_arch="wasm32")] #[cfg(feature="owner")] mod test_session { fn ok() {} }')
        self.policy('#![cfg(feature="owner")] #[cfg(target_arch="wasm32")] mod test_session { fn ok() {} }', before='#![cfg(feature="owner")] pub fn old() {}')

    def test_same_name_inline_instances_do_not_share_ancestor_gates(self):
        owner = '#[cfg(feature="owner")] mod outer { #[cfg(target_arch="wasm32")] mod test_session { fn ok() {} } }'
        identity = '#[cfg(feature="identity")] mod outer { fn ordinary() {} }'
        for body in (owner + identity, identity + owner):
            with self.subTest(body=body):
                self.policy(body)

    def test_unchanged_same_name_instances_keep_exact_lexical_scope(self):
        identity = '#[cfg(feature="identity")] mod outer { fn ordinary() {} }'
        old_owner = '#[cfg(feature="owner")] mod outer {}'
        new_owner = '#[cfg(feature="owner")] mod outer { #[cfg(target_arch="wasm32")] mod test_session { fn ok() {} } }'
        for before, body in ((old_owner + identity, new_owner + identity),
                             (identity + old_owner, identity + new_owner)):
            with self.subTest(before=before):
                self.policy(body, before)

    def test_repeated_inline_mounts_keep_their_own_ancestor_gates(self):
        parent = '#[cfg(feature="identity")] mod outer { mod leaf; } #[cfg(feature="owner")] mod outer { mod leaf; }'
        self.policy('#[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn ok() {} }',
                    parent=parent, leaf='outer/leaf.rs')

    def test_nested_nonmodule_scopes_cannot_hide_an_unproved_ancestor(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg(feature="identity")] fn outer() { #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } } }')

    def test_cfg_attr_selection_is_not_proved_but_lint_is_not_a_barrier(self):
        with self.assertRaises(selector.TestPolicyError):
            self.policy('#[cfg_attr(feature="owner",cfg(feature="identity"))] #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn broken() { missing_function(); } }')
        self.policy('#[cfg_attr(feature="identity",allow(dead_code))] #[cfg(all(target_arch="wasm32",feature="owner"))] mod test_session { fn ok() {} }')


class LocalInputPlans(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'ci@example.invalid')
        self.git('config', 'user.name', 'CI fixture')
        self.write('Cargo.toml', '[workspace]\nmembers=["crates/*"]\n[workspace.dependencies]\ncore={path="crates/core"}\n')
        for pkg in ('core', 'consumer', 'other'):
            dev = '[target.\'cfg(unix)\'.dev-dependencies]\ncore={workspace=true,optional=true}\n' if pkg == 'consumer' else ''
            self.write(f'crates/{pkg}/Cargo.toml', f'[package]\nname="{pkg}"\nversion="0.1.0"\n{dev}[features]\ntest_tier_required=[]\ntest_tier_full=["test_tier_required"]\n')
            self.write(f'crates/{pkg}/src/lib.rs', '')
        self.write('doc/readme.md', 'ordinary doc')
        self.write('scripts/ci-required-scope.json', (HERE / 'ci-required-scope.json').read_text())
        self.base = self.commit()

    def tearDown(self):
        os.chdir(self.previous)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL).decode().strip()

    def write(self, path, content):
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(content)

    def commit(self):
        self.git('add', '-A')
        self.git('commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def fast_config(self):
        groups = ['baseline', 'rust_baseline', 'core', 'consumer', 'other', *selector.PLATFORM_GROUPS,
                  'oasis7_required', 'formal_process', 'command_center']
        return {
            'groups': groups, 'resources': {g: [] for g in groups},
            'package_groups': {n: ['oasis7_required', 'formal_process'] for n in ('core', 'consumer', 'other')},
            'rules': [{'match': ['doc/**'], 'groups': [], 'reason': 'doc'},
                      {'match': ['tools/oasis7-command-center/**'], 'groups': ['command_center'], 'reason': 'desktop'}],
            'fast_scope_version': 1, 'oasis7_required_packages': ['core', 'consumer', 'other'],
            'oasis7_required_wasm_packages': ['core', 'other'],
            'formal_risk_rules': [
                {'match': ['crates/core/src/**', 'crates/core/tests/**'], 'suites': [], 'reason': 'core behavior has ordinary contracts'},
                {'match': ['crates/core/src/history.rs'], 'suites': ['world-history'], 'reason': 'retained history'}],
            'command_center_fast_paths': ['tools/oasis7-command-center/ui/src/**'],
        }

    def object_store(self):
        if hasattr(self, 'objects'):
            return
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.objects = Path(temporary.name) / 'objects'
        self.objects.mkdir()
        self.original_objects = Path(self.git('rev-parse', '--git-path', 'objects')).resolve()
        environment = patch.dict(os.environ, {
            'GIT_OBJECT_DIRECTORY': str(self.objects),
            'GIT_ALTERNATE_OBJECT_DIRECTORIES': str(self.original_objects),
            'GIT_OPTIONAL_LOCKS': '0',
        })
        environment.start()
        self.addCleanup(environment.stop)

    def tree(self, overrides, parent='HEAD'):
        """Build immutable fixtures without commits or even a temporary index."""
        self.object_store()
        entries = {}
        for record in selector.git('ls-tree', '-r', '-z', parent).split(b'\0'):
            if record:
                metadata, path = record.split(b'\t', 1)
                mode, kind, oid = metadata.split()
                entries[path.decode('utf-8', 'surrogateescape')] = (mode, kind, oid)
        for path, contents in overrides.items():
            if contents is None:
                entries.pop(path, None)
            else:
                blob = contents.encode() if isinstance(contents, str) else contents
                oid = subprocess.check_output(['git', 'hash-object', '-w', '--stdin'], input=blob).strip()
                entries[path] = (b'100644', b'blob', oid)
        root = {}
        for path, entry in entries.items():
            directory = root
            parts = path.split('/')
            for part in parts[:-1]:
                directory = directory.setdefault(part, {})
            directory[parts[-1]] = entry

        def write_tree(directory):
            records = []
            for name, value in directory.items():
                if isinstance(value, dict):
                    value = (b'040000', b'tree', write_tree(value))
                records.append(b' '.join(value) + b'\t' + name.encode('utf-8', 'surrogateescape') + b'\0')
            return subprocess.check_output(['git', 'mktree', '-z'], input=b''.join(records)).strip()

        return write_tree(root).decode()

    def inputs(self, index_tree, worktree_tree, changes=()):
        return dict(schema_version=1, source_sha=self.git('rev-parse', 'HEAD'),
                    index_tree=index_tree, worktree_tree=worktree_tree, changes=list(changes))

    def select(self, local_inputs, config=None):
        return selector.select(config or self.fast_config(), self.base, self.git('rev-parse', 'HEAD'),
                               profile='pr-fast', policy_ref=self.base, local_inputs=local_inputs)

    def test_clean_api_and_snapshot_retain_the_same_selection_and_real_source(self):
        self.write('doc/readme.md', 'committed documentation')
        head = self.commit()
        config = self.fast_config()
        expected = selector.select(config, self.base, head, profile='pr-fast', policy_ref=self.base)
        tree = self.tree({})
        inputs = self.inputs(tree, tree, [dict(origin='committed', status='M', path='doc/readme.md')])
        actual = self.select(inputs, config)
        evidence = actual.pop('local_inputs')
        actual['reasons'].pop()
        self.assertEqual(actual, expected)
        self.assertEqual(evidence['source_sha'], head)
        self.assertEqual(evidence['worktree_tree'], tree)
        self.assertEqual(evidence['compared_paths'], ['doc/readme.md'])
        self.assertEqual(evidence['comparisons'], [(self.base, head), (head, tree), (tree, tree)])

    def test_cancelled_staged_change_and_all_origins_remain_selected(self):
        self.write('doc/readme.md', 'committed')
        head = self.commit()
        before_index = Path('.git/index').read_bytes()
        before_refs = self.git('show-ref')
        before_objects = sorted(str(path) for path in Path('.git/objects').rglob('*'))
        source = 'crates/core/src/lib.rs'
        special = 'doc/new space\tline\n雪.md'
        index_tree = self.tree({source: 'pub fn staged() {}'})
        worktree_tree = self.tree({special: 'untracked documentation'})
        changes = [dict(origin='committed', status='M', path='doc/readme.md'),
                   dict(origin='staged', status='M', path=source),
                   dict(origin='unstaged', status='M', path=source),
                   dict(origin='untracked', status='??', path=special)]
        result = self.select(self.inputs(index_tree, worktree_tree, changes))
        self.assertEqual(result['affected_packages'], ['consumer', 'core'])
        self.assertIn('oasis7_required', result['groups'])
        self.assertEqual(result['source_sha'], head)
        self.assertEqual(result['tested_sha'], head)
        self.assertEqual(result['architecture']['source'], head)
        self.assertEqual(result['local_inputs']['changes'], changes)
        self.assertIn(special, result['local_inputs']['compared_paths'])
        self.assertEqual(Path('.git/index').read_bytes(), before_index)
        self.assertEqual(self.git('show-ref'), before_refs)
        self.assertEqual(sorted(str(path) for path in Path('.git/objects').rglob('*')), before_objects)
        self.assertEqual(Path(source).read_text(), '')
        self.assertFalse(Path(special).exists())

    def test_manifest_graphs_include_index_dependencies_even_after_worktree_reverts(self):
        manifest = 'crates/other/Cargo.toml'
        index_tree = self.tree({manifest: Path(manifest).read_text() + '\n[dependencies]\ncore={path="../core"}\n'})
        source = 'crates/core/src/lib.rs'
        worktree_tree = self.tree({source: 'pub fn changed() {}'})
        result = self.select(self.inputs(index_tree, worktree_tree, [
            dict(origin='staged', status='M', path=manifest),
            dict(origin='unstaged', status='M', path=manifest),
            dict(origin='unstaged', status='M', path=source),
        ]))
        self.assertEqual(result['scope'], 'full')
        self.assertEqual(result['affected_packages'], ['consumer', 'core', 'other'])
        self.assertIn(manifest, result['local_inputs']['compared_paths'])

    def test_deleted_and_renamed_paths_keep_both_endpoints_and_unknowns_block(self):
        self.write('unowned.file', 'unique file that must retain its old owner requirement')
        self.base = self.commit()
        index_tree = self.tree({})
        worktree_tree = self.tree({'unowned.file': None,
                                  'doc/renamed.md': Path('unowned.file').read_text(),
                                  'crates/core/src/lib.rs': None})
        changes = [dict(origin='unstaged', status='R100', path='doc/renamed.md', old_path='unowned.file'),
                   dict(origin='unstaged', status='D', path='crates/core/src/lib.rs')]
        with self.assertRaisesRegex(selector.UnknownInputsError, 'unowned.file'):
            self.select(self.inputs(index_tree, worktree_tree, changes))

    def test_snapshot_provenance_cannot_omit_diff_or_hide_an_unknown_path_behind_full(self):
        index_tree = self.tree({})
        worktree_tree = self.tree({'Cargo.toml': Path('Cargo.toml').read_text() + '\n# changed\n',
                                  'unknown.file': 'unknown'})
        changes = [dict(origin='unstaged', status='M', path='Cargo.toml')]
        with self.assertRaisesRegex(ValueError, 'provenance omits.*unknown.file'):
            self.select(self.inputs(index_tree, worktree_tree, changes))
        changes.append(dict(origin='untracked', status='??', path='unknown.file'))
        with self.assertRaisesRegex(selector.UnknownInputsError, 'unknown.file'):
            self.select(self.inputs(index_tree, worktree_tree, changes))

    def test_invalid_snapshots_are_rejected_before_conservative_selection(self):
        tree = self.tree({})
        valid = self.inputs(tree, tree)
        invalid = [None, [], dict(valid, schema_version=True), dict(valid, schema_version=2),
                   dict(valid, source_sha='0' * 40), dict(valid, source_sha='HEAD'),
                   dict(valid, index_tree=valid['source_sha']), dict(valid, worktree_tree='0' * 40),
                   dict(valid, extra=True), dict(valid, changes={}),
                   dict(valid, changes=[dict(origin='unknown', status='M', path='doc/readme.md')]),
                   dict(valid, changes=[dict(origin='staged', status='M', path='../outside')]),
                   dict(valid, changes=[dict(origin='staged', status='M', path='doc/\0bad')]),
                   dict(valid, changes=[dict(origin='staged', status='R100', path='doc/new')])]
        for value in invalid:
            with self.subTest(value=value), patch.object(selector, '_select') as inner:
                with self.assertRaises(ValueError):
                    # None means the unchanged API, so validate its malformed
                    # payload directly rather than invoking the absent flag.
                    if value is None:
                        selector.validate_local_inputs(self.base, 'HEAD', None, value)
                    else:
                        self.select(value)
                inner.assert_not_called()

    def test_provenance_must_preserve_the_correct_stage(self):
        path = 'doc/readme.md'
        index_tree = self.tree({path: 'staged'})
        worktree_tree = self.tree({})
        inputs = self.inputs(index_tree, worktree_tree, [dict(origin='unstaged', status='M', path=path)])
        with self.assertRaisesRegex(ValueError, 'provenance omits'):
            self.select(inputs)

    def test_declared_tree_resolution_is_scoped_and_policy_stays_a_commit(self):
        tree = self.tree({'doc/readme.md': 'dirty'})
        inputs = self.inputs(self.tree({}), tree, [dict(origin='unstaged', status='M', path='doc/readme.md')])
        with self.assertRaises(subprocess.CalledProcessError):
            selector.resolve_commit(tree)
        self.select(inputs)
        with self.assertRaises(subprocess.CalledProcessError):
            selector.resolve_commit(tree)
        with self.assertRaisesRegex(ValueError, 'policy must be a real commit'):
            selector.select(self.fast_config(), self.base, 'HEAD', policy_ref=tree, local_inputs=inputs)
        inputs['changes'].append(dict(origin='referenced', status='A', path='unowned-extra.file'))
        with self.assertRaises(selector.UnknownInputsError):
            self.select(inputs)
        with self.assertRaises(subprocess.CalledProcessError):
            selector.resolve_commit(tree)

    def test_captured_ignored_reference_uses_the_actual_dependency_graph(self):
        source = 'crates/core/src/lib.rs'
        fixture = 'crates/core/src/ignored-fixture.txt'
        self.write('.gitignore', '/' + fixture + '\n')
        self.base = self.commit()
        self.assertEqual(self.git('check-ignore', fixture), fixture)
        index_tree = self.tree({})
        worktree_tree = self.tree({source: 'pub const DATA: &str = include_str!("ignored-fixture.txt");',
                                  fixture: 'captured dependency'})
        changes = [dict(origin='unstaged', status='M', path=source),
                   dict(origin='referenced', status='A', path=fixture)]
        plan = self.select(self.inputs(index_tree, worktree_tree, changes))
        self.assertEqual(plan['affected_packages'], ['consumer', 'core'])
        self.assertIn(fixture, plan['local_inputs']['compared_paths'])

    def test_external_cargo_dependency_retains_conservative_coverage(self):
        manifest = 'crates/core/Cargo.toml'
        index_tree = self.tree({})
        worktree_tree = self.tree({manifest: Path(manifest).read_text()
                                  + '\n[dependencies]\nexternal={path="../../../external"}\n'})
        plan = self.select(self.inputs(index_tree, worktree_tree, [
            dict(origin='unstaged', status='M', path=manifest)]))
        self.assertEqual(plan['scope'], 'full')
        self.assertTrue(any('path dependency outside resolved workspace' in reason for reason in plan['reasons']))

    def test_local_entry_registration_is_exact_and_cannot_exempt_neighbor_paths(self):
        index_tree = self.tree({})
        worktree_tree = self.tree({'scripts/check-local.py': '# entry'})
        changes = [dict(origin='untracked', status='??', path='scripts/check-local.py')]
        plan = self.select(self.inputs(index_tree, worktree_tree, changes))
        self.assertEqual(plan['scope'], 'full')
        worktree_tree = self.tree({'scripts/check-local.py': '# entry', 'scripts/check-local-other.py': '# unknown'})
        changes.append(dict(origin='untracked', status='??', path='scripts/check-local-other.py'))
        with self.assertRaisesRegex(selector.UnknownInputsError, 'check-local-other'):
            self.select(self.inputs(index_tree, worktree_tree, changes))

    def test_cancelled_staged_test_policy_change_is_not_exempted(self):
        source = 'crates/core/src/lib.rs'
        index_tree = self.tree({source: '#[test]\n#[ignore]\nfn hidden() {}'})
        worktree_tree = self.tree({})
        changes = [dict(origin='staged', status='M', path=source),
                   dict(origin='unstaged', status='M', path=source)]
        with self.assertRaisesRegex(selector.TestPolicyError, 'ignored test'):
            self.select(self.inputs(index_tree, worktree_tree, changes))

    def test_cli_reads_snapshot_and_never_emits_success_for_invalid_input(self):
        tree = self.tree({'doc/readme.md': 'working documentation'})
        inputs = self.inputs(self.tree({}), tree, [dict(origin='unstaged', status='M', path='doc/readme.md')])
        directory = self.objects.parent
        config_path, input_path = directory / 'config.json', directory / 'inputs.json'
        config_path.write_text(json.dumps(self.fast_config()))
        input_path.write_text(json.dumps(inputs))
        command = [sys.executable, str(HERE / 'plan-rust-required-scope.py'),
                   '--config', str(config_path), '--base-ref', self.base, '--head-ref', 'HEAD',
                   '--profile', 'pr-fast', '--local-inputs', str(input_path)]
        process = subprocess.run(command, text=True, capture_output=True)
        self.assertEqual(process.returncode, 0, process.stderr)
        plan = json.loads(process.stdout)
        self.assertEqual(plan['groups'], ['baseline'])
        self.assertEqual(plan['local_inputs']['worktree_tree'], tree)
        output = directory / 'plan.json'
        for contents in ('{invalid', 'null', '[]'):
            with self.subTest(contents=contents):
                input_path.write_text(contents)
                process = subprocess.run(command + ['--output', str(output)], text=True, capture_output=True)
                self.assertEqual(process.returncode, 2, process.stderr)
                self.assertEqual(process.stdout, '')
                self.assertFalse(output.exists())
        input_path.write_text(json.dumps(inputs))
        full_command = list(command)
        full_command[full_command.index('--profile') + 1] = 'candidate-full'
        process = subprocess.run(full_command, text=True, capture_output=True)
        self.assertEqual(process.returncode, 2, process.stderr)
        self.assertIn('full inventory does not accept local input snapshots', process.stderr)
        self.assertEqual(process.stdout, '')


if __name__ == '__main__':
    unittest.main()
