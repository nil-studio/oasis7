import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {core, utils, tar} from '../../../vendor/actions-cache-explicit-context/index.mjs';
const CODE='babd8e58e161d3f06b2a3493f28ddd0b6d5eaf6f';
const REPO='/home/runner/work/oasis7/oasis7';
const BASE='codex/engineering-portable-cache-container-cutover';
const HEAD='codex/engineering-portable-cache-job-transport-proof';
const FROM='Lock portable cache transport proof scope';
const TO='Verify portable cache transport in both images and host';
const IMAGES={browser:'ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9',worker:'ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:0b13d0b5ba9051617eb6ab77a5af324f066af79bb2753d664b780535c2fe13a3'};
const components=['worker-build','fonts-noto-cjk','fonts-freefont-ttf','chrome'];
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function bounded(file,maximum=65536) {
  const fd=fs.openSync(file,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW|fs.constants.O_NONBLOCK);
  try {
    const info=fs.fstatSync(fd);if(!info.isFile()||info.size<=0||info.size>maximum)throw Error('invalid proof file');
    const buffer=Buffer.alloc(maximum+1),count=fs.readSync(fd,buffer,0,buffer.length,0);
    if(count!==info.size||count>maximum)throw Error('proof file changed or exceeds bound');
    return buffer.subarray(0,count);
  } finally {fs.closeSync(fd);}
}
function childEnv() {
  return Object.fromEntries(['PATH','HOME','LANG','LC_ALL','TMPDIR','RUNNER_TEMP','RUNNER_OS','RUNNER_ARCH','GITHUB_WORKSPACE','RUST_TOOLCHAIN','RUSTUP_HOME','CARGO_HOME','GITHUB_ENV','GITHUB_OUTPUT','GITHUB_STEP_SUMMARY']
    .filter(key=>process.env[key]!==undefined).map(key=>[key,process.env[key]]));
}
function python(args) {return execFileSync('python3',args,{cwd:REPO,env:childEnv(),encoding:'utf8',timeout:90000,maxBuffer:65536});}
function same(first,second) {const a=fs.statSync(first),b=fs.statSync(second);if(a.dev!==b.dev||a.ino!==b.ino)throw Error('proof workspace bind mismatch');}
function admission() {
  const e=process.env,pull=JSON.parse(bounded(e.GITHUB_EVENT_PATH,1024*1024));
  if(process.platform!=='linux'||process.getuid()!==1001||process.getgid()!==1001||e.GITHUB_ACTIONS!=='true'||e.GITHUB_REPOSITORY!=='nil-studio/oasis7'||e.RUNNER_OS!=='Linux'||e.RUNNER_ARCH!=='X64'||e.RUNNER_ENVIRONMENT!=='github-hosted'||e.GITHUB_EVENT_NAME!=='pull_request'||e.GITHUB_HEAD_REF!==HEAD||e.GITHUB_BASE_REF!==BASE||pull.action!=='edited'||pull.changes?.title?.from!==FROM||pull.pull_request?.title!==TO||pull.pull_request?.draft!==true||pull.pull_request?.head?.repo?.full_name!=='nil-studio/oasis7'||pull.pull_request?.base?.repo?.full_name!=='nil-studio/oasis7')throw Error('proof is not the exact owned one-shot title transition');
  if(e.CODE_SHA!==CODE||e.CONTROLLER_SHA!==pull.pull_request.head.sha||!/^[a-f0-9]{40}$/.test(e.CONTROLLER_SHA)||!/^[1-9][0-9]*$/.test(e.GITHUB_RUN_ID)||e.GITHUB_RUN_ATTEMPT!=='1')throw Error('proof source/run identity mismatch');
  same(e.GITHUB_WORKSPACE,REPO);
  if(execFileSync('git',['-C',REPO,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==CODE)throw Error('proof checkout mismatch');
  const phase=e.PROBE_PHASE,role=e.PROBE_KIND;
  const jobs={seed:'host-seed',image:role==='browser'?'browser-roundtrip':'worker-roundtrip',read:'host-reader'};
  if(!['seed','image','read'].includes(phase)||!['host','browser','worker'].includes(role)||e.GITHUB_JOB!==jobs[phase]||(phase==='image')!==(role!=='host'))throw Error('proof phase/job mismatch');
  if(phase!=='image'&&e.GITHUB_WORKSPACE!==REPO)throw Error('host reader must use original unmodified workspace');
  return {phase,role};
}
function layouts() {
  return components.map(component=>{
    const contract=component==='worker-build'?'worker-build':'browser-archive';
    const row=JSON.parse(python(['scripts/ci-portable-cache-archive.py','--contract',contract,'--component',component]));
    return {...row,version:utils.getCacheVersion([row.root],'zstd-without-long',false)};
  });
}
function payload(role,row) {return Buffer.from(`portable-proof|${CODE}|${role}|${row.component}`);}
function absent(root) {if(fs.existsSync(root))throw Error('fixture destination is not initially empty');}
function removeOwn(row,role) {
  if(!fs.existsSync(row.root))return;
  if(JSON.stringify(fs.readdirSync(row.root).sort())!==JSON.stringify(['payload'])||!bounded(path.join(row.root,'payload')).equals(payload(role,row)))throw Error('refuse cleanup of unknown fixture content');
  fs.rmSync(row.root,{recursive:true});
}
function put(row,role) {absent(row.root);fs.mkdirSync(row.root,{recursive:true});fs.writeFileSync(path.join(row.root,'payload'),payload(role,row),{flag:'wx',mode:0o600});}
async function extract(row,file,role,explicit) {
  absent(row.root);
  python(['scripts/ci-portable-cache-archive.py','--supervise','--contract',row.contract,'--component',row.component,'--archive',file,'--compression','zstd-without-long']);
  await tar.extractTar(file,'zstd-without-long',explicit?REPO:undefined);
  if(!bounded(path.join(row.root,'payload')).equals(payload(role,row)))throw Error('host/image payload bytes differ');
}
async function archive(row,stage,explicit) {
  const temporary=fs.mkdtempSync(path.join(process.env.RUNNER_TEMP,'portable-archive-'));
  try {
    const members=await utils.resolvePaths([row.root],explicit?REPO:undefined);
    if(JSON.stringify(members)!==JSON.stringify([row.member]))throw Error('archive member differs from original host');
    await tar.createTar(temporary,members,'zstd-without-long',explicit?REPO:undefined);
    const bytes=bounded(path.join(temporary,utils.getCacheFileName('zstd-without-long')));
    const filename=row.component+'.tzst';fs.writeFileSync(path.join(stage,filename),bytes,{flag:'wx',mode:0o600});
    return {component:row.component,key:row.key,root:row.root,member:row.member,version:row.version,file:filename,archive_sha256:sha(bytes)};
  } finally {fs.rmSync(temporary,{recursive:true,force:true});}
}
function incoming(directory,role,rows) {
  if(!fs.lstatSync(directory).isDirectory())throw Error('proof input is not a regular directory');
  const names=fs.readdirSync(directory).sort(),expected=['manifest.json',...components.map(c=>c+'.tzst')].sort();
  if(JSON.stringify(names)!==JSON.stringify(expected))throw Error('proof archive inventory differs');
  const manifest=JSON.parse(bounded(path.join(directory,'manifest.json')));
  if(JSON.stringify(Object.keys(manifest).sort())!==JSON.stringify(['controller','image','role','rows','run','schema','source'].sort())||manifest.schema!==1||manifest.source!==CODE||manifest.controller!==process.env.CONTROLLER_SHA||manifest.run!==process.env.GITHUB_RUN_ID||manifest.role!==role||manifest.image!==(IMAGES[role]||'host')||!Array.isArray(manifest.rows)||manifest.rows.length!==4)throw Error('proof manifest identity mismatch');
  for(const [index,row]of rows.entries()) {
    const found=manifest.rows[index],file=row.component+'.tzst';
    const required={component:row.component,key:row.key,root:row.root,member:row.member,version:row.version,file};
    if(JSON.stringify(Object.keys(found).sort())!==JSON.stringify([...Object.keys(required),'archive_sha256'].sort()))throw Error('proof row schema mismatch');
    for(const [key,value]of Object.entries(required))if(found[key]!==value)throw Error('proof row identity mismatch');
    if(found.archive_sha256!==sha(bounded(path.join(directory,file))))throw Error('proof archive hash mismatch');
  }
  return manifest;
}
async function main() {
  const {phase,role}=admission();
  // No cache-service operation is used. Only exact fixture tar primitives run.
  for(const key of Object.keys(process.env))if(/^(?:INPUT_|ACTIONS_)|^(?:GH_TOKEN|GITHUB_TOKEN|NODE_OPTIONS|HTTP_PROXY|HTTPS_PROXY|ALL_PROXY|NO_PROXY|http_proxy|https_proxy|all_proxy|no_proxy)$/.test(key))delete process.env[key];
  if(phase==='image') {
    const code="import importlib.util,os; s=importlib.util.spec_from_file_location('binding','scripts/ci-portable-cache-job.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m);m.check_bindings(os.environ['PROBE_KIND'],os.environ,os.environ['CODE_SHA'])";
    execFileSync('python3',['-c',code],{cwd:process.env.GITHUB_WORKSPACE,env:{...childEnv(),PROBE_KIND:role,CODE_SHA:CODE},stdio:'inherit',timeout:90000});
    if(role==='worker')console.log(python(['.github/actions/setup-linux-mold/setup.py','--install-dir','/opt/oasis7-mold/3.0.0-v1']));
  }
  const rows=layouts(),root=process.env.GITHUB_WORKSPACE;
  if(phase==='read') {
    for(const writer of ['browser','worker']) {
      const directory=path.join(root,'portable-proof-input-'+writer);incoming(directory,writer,rows);
      for(const row of rows){await extract(row,path.join(directory,row.component+'.tzst'),writer,false);removeOwn(row,writer);}
    }
  } else {
    const stage=path.join(root,'portable-proof-stage');fs.mkdirSync(stage);
    if(phase==='image')incoming(path.join(root,'portable-proof-input-host'),'host',rows);
    const records=[];
    for(const row of rows) {
      if(phase==='image'){await extract(row,path.join(root,'portable-proof-input-host',row.component+'.tzst'),'host',true);removeOwn(row,'host');}
      put(row,role);records.push(await archive(row,stage,phase==='image'));removeOwn(row,role);
      await extract(row,path.join(stage,row.component+'.tzst'),role,phase==='image');removeOwn(row,role);
    }
    fs.writeFileSync(path.join(stage,'manifest.json'),JSON.stringify({schema:1,source:CODE,controller:process.env.CONTROLLER_SHA,run:process.env.GITHUB_RUN_ID,role,image:IMAGES[role]||'host',rows:records}),{flag:'wx',mode:0o600});
  }
  console.log(JSON.stringify({probe_only:true,phase,role,source:CODE,controller:process.env.CONTROLLER_SHA,image:IMAGES[role]||'host',components:rows.map(r=>({component:r.component,key:r.key,raw_paths:[r.root],version:r.version})),physical_host_default_reader:phase==='read',main_admission_verified:false,cache_api:false,shared_cache_write:false,cargo_compile:false}));
}
try {await main();} catch(error) {core.setFailed(error.stack||error.message);}
