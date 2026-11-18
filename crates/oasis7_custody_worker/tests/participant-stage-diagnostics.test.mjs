import assert from 'node:assert/strict';
import test from 'node:test';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,rmSync,readFileSync,symlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {assertParticipantBound,projectParticipantState,readParticipantState} from './fixtures/participant_stage_diagnostics.mjs';

const secret='CANARY_PRIVATE_COOKIE_SIGNATURE_ORIGINAL';
function snapshot(){return {state:{draft:{entry:{custody_account_id:'account',provision_id:'provision'},plan:{environment:'test'}},participant_provisions:{[secret]:{
  oracle_reads:8,claims:{policy:'1',submit:'2',[secret]:'3'},callbacks:{policy:'4',admission:'5'},policy:{secret},admission:{secret},submit:{secret},lookup:{secret},owner_view:{secret},result:null,submit_bytes:secret}},cookie:secret}};}
function fixture(t,values=[snapshot()]){
  const root=mkdtempSync(join(tmpdir(),'participant-diagnostic-'));
  t.after(()=>rmSync(root,{recursive:true,force:true}));
  for(const [i,value]of values.entries())execFileSync('python3',['-c',
    'import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); db.execute("CREATE TABLE custody_records (id TEXT,payload TEXT)"); db.execute("INSERT INTO custody_records VALUES (?,?)",("custody-provision-approval-v1",sys.stdin.read())); db.commit(); db.close()',join(root,`${i}.sqlite`)],{input:JSON.stringify(value)});
  return root;
}
test('real SQLite snapshot is scoped, projected and not mutated',t=>{
  const root=fixture(t);const before=readFileSync(join(root,'0.sqlite'));
  const projected=projectParticipantState(readParticipantState(root,'account','provision'));
  assert.equal(projected.count,1);assert.equal(projected.items[0].reads,8);
  assert.equal(projected.items[0].claims.submit,true);assert.equal(projected.items[0].callbacks.final,false);
  assert.equal(projected.items[0].result,false);assert.ok(!JSON.stringify(projected).includes(secret));
  assert.deepEqual(readFileSync(join(root,'0.sqlite')),before);
});
for(const [name,account,provision,change]of [
  ['wrong account','other','provision',()=>{}],['wrong provision','account','other',()=>{}],
  ['wrong environment','account','provision',v=>{v.state.draft.plan.environment='production';}],
])test(`real SQLite rejects ${name}`,t=>{const value=snapshot();change(value);const root=fixture(t,[value]);assert.throws(()=>readParticipantState(root,account,provision));});
test('duplicate scoped snapshots are unknown',t=>{const root=fixture(t,[snapshot(),snapshot()]);assert.throws(()=>readParticipantState(root,'account','provision'));});
test('UTF-8 byte oversized original is rejected before parsing',t=>{const value=snapshot();value.state.cookie='密'.repeat(710000);const root=fixture(t,[value]);assert.throws(()=>readParticipantState(root,'account','provision'));});
test('symlink SQLite is rejected',t=>{const root=fixture(t);symlinkSync(join(root,'0.sqlite'),join(root,'linked.sqlite'));assert.throws(()=>readParticipantState(root,'account','provision'));});
test('projection is bounded and unknown fields never become output',()=>{
  const state=snapshot().state;state.participant_provisions=Object.fromEntries(Array.from({length:19},(_,i)=>[i,{oracle_reads:secret,claims:{[secret]:secret},callbacks:null,result:secret}]));
  const result=projectParticipantState(state);assert.equal(result.items.length,16);assert.equal(result.omitted,3);
  assert.equal(result.items[0].reads,null);assert.equal(result.items[0].result,null);assert.ok(!JSON.stringify(result).includes(secret));
});
test('success performs no reads, suspension or emission',async()=>{
  const fail=()=>{assert.fail('success must not invoke diagnostic I/O');};
  await assertParticipantBound({suspend:fail,nativeRequests:fail,worldResults:fail},{stage:'participant_bound'},{read:fail,emit:fail});
});
test('failure freezes before real SQLite observation and preserves strict assertion',async t=>{
  const root=fixture(t);let stopped=false;let output;
  const runtime={persist:root,account:'account',suspend:async()=>{stopped=true;},nativeRequests:()=>({'/participant/lookup':1,[secret]:999}),
    worldResults:()=>[{path:'/participant/lookup',http:200,outcome:{status:'pending',secret}},{path:secret,http:200,outcome:{status:secret}}]};
  await assert.rejects(assertParticipantBound(runtime,{stage:'native_session_ready',provision_id:'provision',pending_operations:[secret]},
    {read:(...args)=>{assert.equal(stopped,true);return readParticipantState(...args);},emit:v=>{output=v;}}),e=>e.code==='ERR_ASSERTION'&&e.expected==='participant_bound'&&e.actual==='native_session_ready');
  assert.ok(!output.includes(secret));const report=JSON.parse(output);assert.equal(report.snapshot_status,'available');assert.equal(report.responses[0].outcome,'pending');
  assert.equal(report.participants.items[0].reads,8);assert.equal(report.requests['/participant/lookup'],1);
});
for(const phase of ['suspend','read','emit'])test(`${phase} failure cannot mask the original assertion`,async()=>{
  let output;const fail=()=>{throw new Error(secret);};
  await assert.rejects(assertParticipantBound({suspend:phase==='suspend'?fail:async()=>{},persist:'unused',account:'account'},
    {stage:'native_session_ready',provision_id:'provision'}, {read:phase==='read'?fail:()=>snapshot().state,emit:phase==='emit'?fail:v=>{output=v;}}),
    e=>e.code==='ERR_ASSERTION'&&e.actual==='native_session_ready');
  if(output){assert.ok(!output.includes(secret));if(phase!=='emit')assert.equal(JSON.parse(output).snapshot_status,'unavailable');}
});
