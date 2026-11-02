#!/usr/bin/env python3
import contextlib
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
