#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
python3 - "$ROOT_DIR" <<'PY'
import json,os,pathlib,shutil,subprocess,sys,tempfile,unittest
ROOT=pathlib.Path(sys.argv[1])
MODEL=r'''#!/usr/bin/env python3
import json,os,pathlib,sys
s=pathlib.Path(os.environ['MODEL']); a=sys.argv[1:]; role=pathlib.Path(sys.argv[0]).name
with (s/'calls').open('a') as f:f.write(json.dumps([role,a])+'\n')
image='sha256:'+'a'*64
recipe={'producer_recipe_hash':('b' if (s/'changed').exists() else 'a')*64}
if role=='cargo':
 a=a[a.index('--')+1:]
 if a[0]=='producer-recipe':pathlib.Path(a[2]).write_text(json.dumps(recipe))
 elif a[0]=='candidate-builder-evidence':
  r=json.loads(pathlib.Path(a[1]).read_text()); v=json.loads(pathlib.Path(a[2]).read_text())[0]
  if v['Os']!='linux' or v['Architecture']!='amd64' or v['Config']['Labels']['io.oasis7.candidate.producer-recipe']!=r['producer_recipe_hash']:sys.exit(31)
  pathlib.Path(a[3]).write_text(json.dumps({'builder_recipe':r,'builder_execution':{'image_id':v['Id']}}))
 else:sys.exit(32)
 sys.exit(0)
if a[0]=='info':sys.exit(0)
if a[:2]==['buildx','version']:sys.exit(1)
if a[:2]==['image','inspect']:
 if (s/'missing').exists():sys.exit(1)
 env={'PATH':'/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin','DEBIAN_FRONTEND':'noninteractive','CARGO_HOME':'/cargo','RUSTUP_HOME':'/rustup','OASIS7_WASM_TOOLCHAIN':'nightly-2025-12-11','OASIS7_WASM_TARGET':'wasm32-unknown-unknown','OASIS7_WASM_BUILD_STD':'1','OASIS7_WASM_BUILD_STD_COMPONENTS':'std,panic_abort','OASIS7_WASM_BUILD_STD_FEATURES':'','OASIS7_WASM_BUILD_IN_CONTAINER':'1','OASIS7_WASM_BUILD_SUITE_BIN':'/cargo/bin/wasm_build_suite','TZ':'UTC','LANG':'C','LC_ALL':'C','SOURCE_DATE_EPOCH':'0'}
 if (s/'wrong-suite').exists():env['OASIS7_WASM_BUILD_SUITE_BIN']='/bin/true'
 if (s/'missing-suite').exists():del env['OASIS7_WASM_BUILD_SUITE_BIN']
 if (s/'unknown-wasm-env').exists():env['OASIS7_WASM_BUILDER_IMAGE']='other'
 if (s/'shell-env').exists():env['BASH_ENV']='/tmp/other'
 if (s/'wrong-path').exists():env['PATH']='/tmp/other'
 env=[k+'='+v for k,v in env.items()]
 if (s/'duplicate-suite').exists():env.append('OASIS7_WASM_BUILD_SUITE_BIN=/cargo/bin/wasm_build_suite')
 entry=['/bin/true'] if (s/'wrong-entrypoint').exists() else ['/opt/oasis7/scripts/build-wasm-module.sh']
 if (s/'missing-entrypoint').exists():entry=None
 print(json.dumps([{'Id':image,'Os':'linux','Architecture':'arm64' if (s/'platform').exists() else 'amd64','Config':{'Labels':{'io.oasis7.candidate.producer-recipe':('c' if (s/'label').exists() else 'a')*64},'Env':env,'Entrypoint':entry}}]))
elif a[0]=='build':
 pathlib.Path(a[a.index('--iidfile')+1]).write_text(image)
 (s/'missing').unlink(missing_ok=True)
elif a[0]=='create':
 assert a[-1]==image
 print('c'*64)
elif a[0]=='rm':assert a==['rm','c'*64]
elif a[0]=='run':
 assert image in a and not any(x.startswith('oasis7/wasm-builder:') for x in a)
 mounts=[a[i+1] for i,x in enumerate(a) if x=='--mount']
 parsed=[dict(part.split('=',1) for part in x.split(',')) for x in mounts]
 root=next(pathlib.Path(x['src']) for x in parsed if x['dst']=='/workspace')
 for m in parsed:
  if m['dst'] in ['/tmp/oasis7-cargo-home','/tmp/oasis7-compiler-tmp']:
   p=pathlib.Path(m['src']);assert p.is_dir() and not list(p.iterdir());(p/'written').touch()
 env=[a[i+1] for i,x in enumerate(a) if x=='--env']
 evidence=next(x.split('=',1)[1] for x in env if x.startswith('OASIS7_WASM_CANDIDATE_BUILDER_EVIDENCE='))
 e=json.loads((root/evidence.removeprefix('/workspace/')).read_text())
 assert e['builder_execution']['image_id']==image
 # Model the existing guest's independent producer-content check, not just labels.
 if (s/'producer-mismatch').exists():sys.exit(34)
 if (s/'mutate').exists():(s/'changed').touch()
 if (s/'guest-failure').exists():sys.exit(35)
else:sys.exit(33)
'''
class Batch(unittest.TestCase):
 def setUp(self):
  self.t=tempfile.TemporaryDirectory();self.addCleanup(self.t.cleanup);self.base=pathlib.Path(self.t.name);self.root=self.base/'root';self.root.mkdir();self.state=self.base/'state';self.state.mkdir();self.bin=self.base/'bin';self.bin.mkdir()
  for p in ['scripts/build-builtin-wasm-modules.sh','scripts/lib/builtin-wasm-candidate-batch.sh']:
   dst=self.root/p;dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/p,dst)
  d=self.root/'crates/oasis7_runtime/src/world/artifacts';d.mkdir(parents=True)
  (d/'m1_builtin_module_ids.txt').write_text('first\nsecond\n')
  (d/'builtin_module_manifest_map.txt').write_text('first crates/first/Cargo.toml\nsecond crates/second/Cargo.toml\n')
  for n in ['first','second']:
   p=self.root/'crates'/n/'Cargo.toml';p.parent.mkdir(parents=True);p.write_text('[package]\n')
  for n in ['cargo','docker']:
   p=self.bin/n;p.write_text(MODEL);p.chmod(0o755)
  self.env=dict(os.environ,PATH=str(self.bin)+os.pathsep+os.environ['PATH'],MODEL=str(self.state))
  for k in list(self.env):
   if k.startswith('OASIS7_WASM_'):del self.env[k]
 def execute(self,ok=True):
  r=subprocess.run(['bash',str(self.root/'scripts/build-builtin-wasm-modules.sh'),'--candidate-build'],cwd=self.root,env=self.env,capture_output=True,text=True)
  if ok:self.assertEqual(r.returncode,0,r.stderr)
  else:self.assertNotEqual(r.returncode,0,r.stderr)
  calls=[json.loads(x) for x in (self.state/'calls').read_text().splitlines()] if (self.state/'calls').exists() else []
  out=self.root/'.tmp/builtin-wasm'
  self.assertFalse(list(out.glob('.candidate-batch.*')))
  if any(a[0]=='create' for role,a in calls if role=='docker'):
   self.assertEqual(sum(a[0]=='rm' for role,a in calls if role=='docker'),1)
  return calls
 def test_existing_image_is_pinned_and_used_for_both_fresh_modules(self):
  c=self.execute();d=[a for role,a in c if role=='docker'];self.assertEqual(sum(a[0]=='build' for a in d),0);runs=[a for a in d if a[0]=='run'];self.assertEqual(len(runs),2)
  homes=[]
  for a in runs:
   mount=next(a[i+1] for i,x in enumerate(a) if x=='--mount' and 'dst=/tmp/oasis7-cargo-home' in a[i+1]);homes.append(mount)
  self.assertNotEqual(*homes)
 def test_missing_image_is_built_exactly_once(self):
  (self.state/'missing').touch();c=self.execute();self.assertEqual(sum(a[0]=='build' for role,a in c if role=='docker'),1)
 def test_wrong_platform_or_label_fails_before_pin_and_run(self):
  for marker in ['platform','label']:
   with self.subTest(marker=marker):
    (self.state/marker).touch();c=self.execute(False);self.assertFalse(any(a[0] in ['run','create'] for role,a in c if role=='docker'));(self.state/marker).unlink()
 def test_wrong_or_missing_entrypoint_is_rejected_before_pin_and_run(self):
  for marker in ['wrong-entrypoint','missing-entrypoint']:
   with self.subTest(marker=marker):
    (self.state/marker).touch();c=self.execute(False);self.assertFalse(any(a[0] in ['run','create'] for role,a in c if role=='docker'));(self.state/marker).unlink()
 def test_critical_environment_cannot_change_the_actual_producer(self):
  for marker in ['wrong-suite','missing-suite','duplicate-suite','unknown-wasm-env','shell-env','wrong-path']:
   with self.subTest(marker=marker):
    (self.state/marker).touch();c=self.execute(False);self.assertFalse(any(a[0] in ['run','create'] for role,a in c if role=='docker'));(self.state/marker).unlink()
 def test_actual_guest_producer_mismatch_is_not_a_label_success(self):
  (self.state/'producer-mismatch').touch();c=self.execute(False);self.assertEqual(sum(a[0]=='run' for role,a in c if role=='docker'),1)
 def test_recipe_mutation_stops_batch_after_first_module(self):
  (self.state/'mutate').touch();c=self.execute(False);self.assertEqual(sum(a[0]=='run' for role,a in c if role=='docker'),1)
 def test_guest_failure_cleans_only_own_pin_and_directories(self):
  (self.state/'guest-failure').touch();self.execute(False)
 def test_builder_override_is_not_accepted(self):
  self.env['OASIS7_WASM_BUILDER_IMAGE']='arbitrary:latest';c=self.execute(False);self.assertEqual(c,[])
 def test_output_symlink_is_rejected_before_creating_evidence(self):
  (self.root/'.tmp').mkdir();(self.root/'.tmp/builtin-wasm').symlink_to(self.state,target_is_directory=True);c=self.execute(False);self.assertEqual(c,[])
 def test_manifest_symlink_is_rejected(self):
  p=self.root/'crates/first/Cargo.toml';p.unlink();p.symlink_to(self.root/'crates/second/Cargo.toml');c=self.execute(False);self.assertFalse(any(a[0]=='run' for role,a in c if role=='docker'))
unittest.main(argv=[sys.argv[0]],verbosity=2)
PY
