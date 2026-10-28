// Bounded release-Wasm restore measurement. Snapshot bytes never leave memory
// except as input to the isolated, compile-time-only approval harness.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,rmSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
export function readSavedAccountSnapshot(persist,account){
  let raw;
  try { raw=execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
found=[]
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  try: row=db.execute("SELECT payload FROM custody_records WHERE id=?",('custody-provision-approval-v1',)).fetchone()
  finally: db.close()
  if row and json.loads(row[0])['state']['draft']['entry']['custody_account_id']==sys.argv[2]: found.append(row[0])
 except sqlite3.Error: pass
if len(found)!=1: raise SystemExit(2)
sys.stdout.write(found[0])
`,resolve(persist),account],{encoding:'utf8',maxBuffer:2*1024*1024+1,stdio:['ignore','pipe','ignore']}); }
  catch {throw new Error('exact Account snapshot unavailable for readonly measurement');}
  assert.ok(Buffer.byteLength(raw)<=2*1024*1024,'snapshot exceeds measurement bound');
  return raw;
}
// Isolated restore samples intentionally contain no Account callback attribution.
// Collect actual callbacks only through the separate fixed diagnostic Account port.
export async function benchmarkAccountRestoreWithStub(stub,snapshot,{wasmPath=resolve(root,'artifacts/approval-harness/index_bg.wasm')}={}){
  const bytes=Buffer.byteLength(snapshot);assert.ok(bytes<=2*1024*1024);
  const snapshot_digest=digest(snapshot);const samples=[];
  for(const mode of ['restore','parse_json_only','legacy_double_decode','transition_clone']) for(let i=0;i<3;i++){
    const started=process.hrtime.bigint();
    const reply=await stub.fetch('https://fixture.test/performance/account-restore',{method:'POST',body:JSON.stringify({api_version:1,snapshot,repeats:1,mode})});
    const text=await reply.text();
    const rpc_wall_ms=Number(process.hrtime.bigint()-started)/1e6;
    assert.equal(reply.status,200,'restore measurement failed');
    let metrics;try{metrics=JSON.parse(text);}catch{throw new Error('invalid measurement metrics');}
    assert.equal(metrics.snapshot_digest,snapshot_digest);
    assert.equal(metrics.bytes,bytes);assert.equal(metrics.samples.length,1);assert.equal(metrics.samples[0].decode_count,mode==='parse_json_only'?0:mode==='legacy_double_decode'?2:1);
    assert.equal(metrics.mode,mode);assert.equal(metrics.validation_scope,mode==='restore'?'full_persistence_restore':mode==='parse_json_only'?'json_syntax_only':'full_persistence_restore_and_state_eq');
    if(mode==='parse_json_only') assert.equal(metrics.stage,null,'JSON syntax success must not report a validated phase');
    const signatures=metrics.samples[0].strict_signature_metrics;
    assert.equal(signatures.scope,'owned_sdk_private_goal_strict','isolated sample must own its synchronous SDK scope');
    assert.ok(signatures.summary,'SDK metrics unavailable');
    for(const name of ['calls','strict_attempts','successes','failures','canonical_bytes','max_canonical_bytes','distinct_tuples','duplicate_tuples','overflow_tuples','tracked_tuples'])assert.ok(Number.isSafeInteger(signatures.summary[name])&&signatures.summary[name]>=0,'signature counter must be a bounded safe count');
    assert.ok(signatures.summary.tracked_tuples<=1024);
    assert.equal(signatures.summary.distinct_tuples+signatures.summary.duplicate_tuples+signatures.summary.overflow_tuples,signatures.summary.strict_attempts);
    if(mode==='parse_json_only')assert.equal(signatures.summary.calls,0,'JSON parsing must not execute strict signature verification');
    if(mode==='legacy_double_decode'||mode==='transition_clone') assert.equal(metrics.samples[0].equal,true,'validated baseline state must remain equal');
    samples.push({mode,temperature:i===0?'cold_mode':'warm_mode',rpc_wall_ms,...metrics});
  }
  const rejection_checks=[];
  for(const mode of ['legacy_double_decode','transition_clone']) for(const corruption of ['schema','state']) {
    const invalid=JSON.parse(snapshot);
    if(corruption==='schema') invalid.schema_version=0;
    else { assert.ok(invalid.state?.draft?.entry);invalid.state.draft.entry.plan_digest='00'.repeat(32); }
    const reply=await stub.fetch('https://fixture.test/performance/account-restore',{method:'POST',body:JSON.stringify({api_version:1,snapshot:JSON.stringify(invalid),repeats:1,mode})});
    await reply.text();
    assert.equal(reply.status,400,'invalid persisted input must be rejected by validation');
    rejection_checks.push({mode,corruption,rejected:true});
  }
  return {profile:'release-wasm-account-restore-signature-metrics-v2',platform:process.platform,arch:process.arch,node:process.version,wasm_digest:digest(readFileSync(wasmPath)),snapshot_digest,bytes,samples,rejection_checks,
    timing_scope:'Node RPC wall time includes transport/input parsing/digest work; workerd event clock may be frozen during CPU; parse-only vs restore wall differences are diagnostic, not exact validation CPU; SDK diagnostic counters cover private_goal strict verification only, not Worker NativeRoot/WebCrypto or pure CPU; duplicate tuple fingerprints are never output; overflow uniqueness is unknown'};
}
export async function benchmarkAccountRestore(snapshot){
  return withRestoreHarness(snapshot,stub=>benchmarkAccountRestoreWithStub(stub,snapshot));
}
export async function verifySnapshotReuse(snapshot){
  return withRestoreHarness(snapshot,async stub=>{
    const reply=await stub.fetch('https://fixture.test/performance/account-restore',{method:'POST',body:JSON.stringify({api_version:1,snapshot,repeats:1,mode:'snapshot_reuse'})});
    assert.equal(reply.status,200,'strict snapshot reuse probe must pass');
    assert.deepEqual(await reply.json(),{identical_bytes_decoded_once:true,changed_bytes_restored:true,invalid_schema_rejected:true,same_sequence_corruption_rejected:true,failed_restore_not_cached:true});
  });
}
async function withRestoreHarness(snapshot,run){
  const directory=mkdtempSync(resolve(tmpdir(),'oasis7-restore-bench-'));
  const mf=new Miniflare({...convertV4MiniflareOptions({name:'restore-measurement',modulesRoot:root,modules:[
    {type:'ESModule',path:resolve(root,'tests/fixtures/approval_harness.mjs')},
    {type:'ESModule',path:resolve(root,'artifacts/approval-harness/index.js')},
    {type:'CompiledWasm',path:resolve(root,'artifacts/approval-harness/index_bg.wasm')}],compatibilityDate:'2026-10-25',cf:false,durableObjects:{STORE:{className:'TestApprovalAuthority',useSQLite:true}}}),resourcePersistencePath:directory,unsafeDevRegistryPath:resolve(directory,'registry')});
  try{const {STORE}=await mf.getBindings();return await run(STORE.get(STORE.idFromName('same-snapshot-three-restores')));}
  finally{await mf.dispose();rmSync(directory,{recursive:true,force:true});}
}
