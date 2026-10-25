#!/usr/bin/env python3
"""Cargo input immutability and process-boundary regressions; no CodeQL run."""
import hashlib
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import codeql_rust_cargo_guard as m


class Guard(unittest.TestCase):
    def test_argument_boundary_and_toolchain_are_preserved(self):
        argv=['+1.99.0','-vv','rustc','--manifest-path','/frozen/Cargo.toml','--','--emit=metadata']
        actual,verb,protected=m.locked_argv(argv)
        self.assertEqual(actual,argv[:5]+['--locked']+argv[5:])
        self.assertEqual((verb,protected),('rustc',True))
        for verb in m.PROTECTED:
            self.assertEqual(m.locked_argv([verb,'--locked'])[0],[verb,'--locked'])
        self.assertEqual(m.locked_argv(['+1.99','metadata','--no-deps'])[0],['+1.99','metadata','--no-deps','--locked'])
        self.assertEqual(m.locked_argv(['-Z','unstable-options','config','get'])[0],['-Z','unstable-options','config','get'])
        self.assertEqual(m.locked_argv(['+1.99','-Zunstable-options','metadata'])[0],['+1.99','-Zunstable-options','metadata','--locked'])
        self.assertEqual(m.locked_argv(['-vV']),(['-vV'],'version',False))
        self.assertEqual(m.locked_argv(['config','get','build.target']),(['config','get','build.target'],'config',False))

    def test_unknown_global_options_and_mutating_commands_reject(self):
        for argv in ([],['+bad tool','metadata'],['--config','metadata','metadata'],['config','set','x','y'],
                     ['install','thing'],['generate-lockfile'],['update'],['foo'],['+stable']):
            with self.subTest(argv=argv),self.assertRaises(RuntimeError):m.locked_argv(argv)

    def test_independent_lock_needs_versioned_hash_not_only_existing_file(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'vendor').mkdir()
            (root/'Cargo.toml').write_text('[workspace]\n')
            (root/'vendor/Cargo.toml').write_text('[package]\nname="vendor"\nversion="1.0.0"\n[workspace]\n')
            (root/'member').mkdir();(root/'member/Cargo.toml').write_text('[package]\nname="member"\nversion="1.0.0"\n')
            manifests=['Cargo.toml','vendor/Cargo.toml','member/Cargo.toml']
            initial={'locks':[{'path':'Cargo.lock','status':'CAPTURED','sha256':'a'*64},
                              {'path':'vendor/Cargo.lock','status':'ABSENT'}]}
            frozen={'Cargo.lock':{'sha256':'a'*64}}
            r=m.standalone_locks(root,manifests,initial,frozen)
            self.assertEqual(r['status'],'FAIL');self.assertEqual(r['rejected_locks'],['vendor/Cargo.lock'])
            initial['locks'][1].update(status='CAPTURED',sha256='b'*64)
            self.assertEqual(m.standalone_locks(root,manifests,initial,frozen)['status'],'FAIL')
            frozen['vendor/Cargo.lock']={'sha256':'b'*64}
            self.assertEqual(m.standalone_locks(root,manifests,initial,frozen)['status'],'PASS')
            initial['locks'][1]['status']='UNKNOWN'
            self.assertEqual(m.standalone_locks(root,manifests,initial,frozen)['status'],'FAIL')

    def setup_guard(self,root,real=False):
        work=root/'work';out=root/'out';work.mkdir();out.mkdir()
        if real:
            home=m.install(work,out,time.time()+60,'source-sha')
        else:
            cargo=root/'real-cargo'
            cargo.write_text('#!'+sys.executable+'\nimport sys,os,json\nfrom pathlib import Path\nif "-Vv" in sys.argv: print("cargo 1.99.0 fixture");sys.exit(0)\nprint(json.dumps({"args":sys.argv[1:],"home":os.environ.get("CARGO_HOME"),"toolchain":os.environ.get("RUSTUP_TOOLCHAIN")}))\nsys.exit(7 if "--fixture-fail" in sys.argv else 0)\n')
            cargo.chmod(0o700)
            with patch.object(m.shutil,'which',return_value=str(cargo)):
                home=m.install(work,out,time.time()+60,'source-sha')
        config=json.loads((out/'cargo-guard.json').read_text())
        return home,out,config

    def run_guard(self,home,args,**env):
        current=dict(os.environ,CARGO_HOME=str(home),RUSTUP_TOOLCHAIN='1.99.0',**env)
        return subprocess.run([str(home/'bin/cargo'),*args],env=current,capture_output=True,timeout=20)

    def audit(self,out,config):
        return m.inspect(out,config['deadline'],'source-sha')

    def test_real_child_gets_locked_restored_home_and_analysis_toolchain(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            with patch.dict(os.environ,{'CARGO_HOME':str(root/'original-home')}):home,out,c=self.setup_guard(root)
            r=self.run_guard(home,['metadata','--manifest-path','/frozen/Cargo.toml'])
            self.assertEqual(r.returncode,0,r.stderr)
            value=json.loads(r.stdout)
            self.assertIn('--locked',value['args']);self.assertEqual(value['home'],str(root/'original-home'))
            self.assertEqual(value['toolchain'],'1.99.0')
            result=self.audit(out,c);self.assertEqual(result['status'],'PASS');self.assertEqual(result['full_metadata_calls'],1)
            receipt=json.loads(next((out/'cargo-guard-calls').glob('*.json')).read_text())
            self.assertIn('1.99.0',receipt['actual_cargo_version']);self.assertNotIn('args',receipt)

    def test_missing_required_metadata_context_cannot_pass(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d))
            c['expected_metadata_manifests']=[hashlib.sha256(b'/frozen/vendor/Cargo.toml').hexdigest()]
            (out/'cargo-guard.json').write_text(json.dumps(c))
            self.assertEqual(self.run_guard(home,['metadata','--manifest-path','/frozen/Cargo.toml']).returncode,0)
            self.assertEqual(self.audit(out,c)['missing_metadata_contexts'],1)
            self.assertEqual(self.audit(out,c)['status'],'FAIL')
            self.assertEqual(self.run_guard(home,['metadata','--manifest-path','/frozen/vendor/Cargo.toml']).returncode,0)
            self.assertEqual(self.audit(out,c)['status'],'PASS')

    def test_nonzero_then_successful_no_deps_fallback_cannot_pass(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d))
            self.assertEqual(self.run_guard(home,['metadata','--fixture-fail']).returncode,7)
            self.assertEqual(self.run_guard(home,['metadata','--no-deps']).returncode,0)
            self.assertEqual(self.audit(out,c)['status'],'FAIL')

    def test_no_calls_or_only_no_deps_cannot_pass(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d))
            with self.assertRaises(RuntimeError):self.audit(out,c)
            self.assertEqual(self.run_guard(home,['metadata','--no-deps']).returncode,0)
            self.assertEqual(self.audit(out,c)['status'],'FAIL')

    def test_unknown_command_is_a_persistent_failure(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d))
            self.assertEqual(self.run_guard(home,['metadata']).returncode,0)
            self.assertEqual(self.run_guard(home,['update']).returncode,101)
            self.assertEqual(self.audit(out,c)['status'],'FAIL')

    def test_pending_missing_duplicate_and_foreign_records_reject(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d));audit=out/'cargo-guard-calls'
            m.reserve(c,{'status':'PENDING'})
            self.assertEqual(self.audit(out,c)['status'],'FAIL')
            (audit/'.counter').write_text('0000000000000002')
            with self.assertRaises(RuntimeError):self.audit(out,c)

    def test_identity_symlink_and_deadline_changes_reject(self):
        for kind in ('wrapper','real','helper','deadline','source'):
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as d:
                home,out,c=self.setup_guard(Path(d));self.assertEqual(self.run_guard(home,['metadata']).returncode,0)
                if kind=='wrapper':(home/'bin/cargo').write_text('changed')
                elif kind=='real':Path(c['real_cargo']).write_text('changed')
                elif kind=='helper':c['helper_sha256']='0'*64;(out/'cargo-guard.json').write_text(json.dumps(c))
                elif kind=='deadline':c['deadline']=time.time()-1;(out/'cargo-guard.json').write_text(json.dumps(c))
                else:c['source_sha']='foreign';(out/'cargo-guard.json').write_text(json.dumps(c))
                with self.assertRaises(RuntimeError):self.audit(out,c)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);target=root/'target';target.write_text('data');(root/'link').symlink_to(target)
            with self.assertRaises(RuntimeError):m.read(root/'link',100)

    def test_count_and_record_limits_fail_closed(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d));audit=out/'cargo-guard-calls'
            (audit/'.counter').write_text(f'{m.MAX_CALLS:016d}')
            with self.assertRaises(RuntimeError):m.reserve(c,{'status':'PENDING'})
            with self.assertRaises(RuntimeError):self.audit(out,c)
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d))
            with self.assertRaises(RuntimeError):m.reserve(c,{'data':'x'*m.MAX_RECORD})
            with self.assertRaises(RuntimeError):self.audit(out,c)

    def test_capture_deadline_cannot_be_extended_to_the_job_deadline(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d))
            self.assertEqual(self.run_guard(home,['metadata']).returncode,0)
            self.assertEqual(self.audit(out,c)['status'],'PASS')
            with self.assertRaisesRegex(RuntimeError,'deadline expired'):
                m.inspect(out,c['deadline'],'source-sha',end=time.monotonic()-1)

    def test_wall_clock_rollback_cannot_extend_the_original_monotonic_deadline(self):
        config={'deadline':time.time()+60,'monotonic_deadline':time.monotonic()-1}
        self.assertLess(m.remaining(config),0)
        with self.assertRaisesRegex(RuntimeError,'deadline expired'):m.verify_config(config)

    def test_mutex_wait_is_bounded_and_sticky_even_without_a_record(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d));audit=out/'cargo-guard-calls'
            c['deadline']=time.time()+0.1
            with (audit/'.lock').open('a+b') as held:
                fcntl.flock(held.fileno(),fcntl.LOCK_EX)
                start=time.monotonic()
                with self.assertRaisesRegex(RuntimeError,'mutex deadline'):m.reserve(c,{'status':'PENDING'})
                self.assertLess(time.monotonic()-start,1)
            self.assertEqual((audit/'.failed').read_bytes(),b'1')

    def test_version_output_is_bounded_before_process_completion(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);script=root/'oversize'
            script.write_text('#!'+sys.executable+'\nimport os,time\nos.write(1,b"x"*4096)\ntime.sleep(20)\n');script.chmod(0o700)
            start=time.monotonic()
            with self.assertRaisesRegex(RuntimeError,'output bound'):m.cargo_version([str(script)],os.environ.copy(),time.time()+10)
            self.assertLess(time.monotonic()-start,3)

    def test_parallel_children_get_distinct_complete_records(self):
        with tempfile.TemporaryDirectory() as d:
            home,out,c=self.setup_guard(Path(d));env=dict(os.environ,CARGO_HOME=str(home))
            children=[subprocess.Popen([str(home/'bin/cargo'),'metadata'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE) for _ in range(6)]
            for child in children:
                stdout,stderr=child.communicate(timeout=20);self.assertEqual(child.returncode,0,stderr)
            r=self.audit(out,c);self.assertEqual(r['status'],'PASS');self.assertEqual(r['calls'],6)

    def test_real_cargo_missing_and_redirected_stale_locks_cannot_update(self):
        if not shutil.which('cargo') or not shutil.which('rustup'):self.skipTest('Cargo/rustup unavailable')
        tools=subprocess.run(['rustup','toolchain','list'],capture_output=True,text=True,timeout=10)
        available=[line.split()[0] for line in tools.stdout.splitlines() if line.strip()]
        if not available:self.skipTest('No installed Cargo toolchain')
        toolchain=next((x for x in available if x.startswith('1.96.0')),available[0])
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);home,out,c=self.setup_guard(root,real=True)
            package=root/'package';(package/'src').mkdir(parents=True);(package/'src/lib.rs').write_text('')
            manifest=package/'Cargo.toml';manifest.write_text('[package]\nname="guard_fixture"\nversion="0.1.0"\nedition="2021"\n[workspace]\n')
            args=['metadata','--offline','--format-version=1','--manifest-path',str(manifest)]
            # Baseline failure: ordinary Cargo creates this missing lock. The guarded call must not.
            env=dict(os.environ,CARGO_HOME=str(home),RUSTUP_TOOLCHAIN=toolchain,CARGO_NET_OFFLINE='true',RUSTUP_AUTO_INSTALL='0')
            r=subprocess.run([str(home/'bin/cargo'),*args],env=env,capture_output=True,timeout=20)
            self.assertNotEqual(r.returncode,0);self.assertIn(b'--locked',r.stderr);self.assertFalse((package/'Cargo.lock').exists())
            actual_env=dict(os.environ,RUSTUP_TOOLCHAIN=toolchain,CARGO_NET_OFFLINE='true',RUSTUP_AUTO_INSTALL='0')
            subprocess.run([c['real_cargo'],*args],env=actual_env,check=True,capture_output=True,timeout=20)
            source_lock=package/'Cargo.lock';original=source_lock.read_bytes()
            r=subprocess.run([str(home/'bin/cargo'),*args],env=env,capture_output=True,timeout=20)
            self.assertEqual(r.returncode,0,r.stderr)
            healthy=root/'healthy';healthy.mkdir();home2,out2,c2=self.setup_guard(healthy,real=True)
            c2['expected_metadata_manifests']=[hashlib.sha256(os.fsencode(manifest.resolve())).hexdigest()]
            (out2/'cargo-guard.json').write_text(json.dumps(c2))
            good_env=dict(env,CARGO_HOME=str(home2))
            r=subprocess.run([str(home2/'bin/cargo'),*args],env=good_env,capture_output=True,timeout=20)
            self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(self.audit(out2,c2)['status'],'PASS')
            # Reproduce RA's redirected-copy branch with a deliberately stale lock.
            temporary=root/'temporary/Cargo.lock';temporary.parent.mkdir();temporary.write_bytes(original)
            manifest.write_text(manifest.read_text().replace('0.1.0','0.2.0'))
            env.update(CARGO_RESOLVER_LOCKFILE_PATH=str(temporary),__CARGO_TEST_CHANNEL_OVERRIDE_DO_NOT_USE_THIS='nightly')
            version=subprocess.run([c['real_cargo'],'--version'],env=actual_env,capture_output=True,text=True,timeout=10).stdout
            minor=int(version.split()[1].split('.')[1])
            if minor<82:self.skipTest('Cargo too old for redirected-lock fixture')
            flags=['-Zlockfile-path'] if minor>=95 else ['--lockfile-path',str(temporary)]
            r=subprocess.run([str(home/'bin/cargo'),*args,*flags,'-Zunstable-options'],env=env,capture_output=True,timeout=20)
            self.assertNotEqual(r.returncode,0);self.assertIn(b'--locked',r.stderr)
            self.assertEqual(temporary.read_bytes(),original);self.assertEqual(source_lock.read_bytes(),original)
            self.assertEqual(self.audit(out,c)['status'],'FAIL')


if __name__=='__main__':unittest.main()
