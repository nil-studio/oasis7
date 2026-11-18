import assert from 'node:assert/strict';
import test from 'node:test';
import {assertNativeStage,createNativeTimings,projectNativeState,readIdentityNativeDiagnostic} from './fixtures/native_stage_diagnostics.mjs';

const secret='NEVER_PRINT_COOKIE_NONCE_SIGNATURE_OR_ORIGINAL';
const selected='selected-private-operation';
const signed=observed=>({signature:secret,verification:{observed_at:observed,proof_expires_at:'5000',
  source_session_expires_at:'90000',source_session_idle_expires_at:'80000',purpose:'inspect_origin',proof_state:'expired',origin_state:'active',verify_nonce:secret}});
const inspection=observed=>({created_at:'100',expires_at:'30100',claimed_at:'101',verified_at:'102',verification:signed(observed),request_bytes:secret,verify_nonce:secret});
const ledger=(limit,spent)=>({limit,spent,reservations:{}});
function stateFixture() {
  return {draft:{plan:{expires_at:'6100'}},provision:{stage:'native_registered',transport:ledger('400','304')},participant_provisions:{},
    origin_inspections:{registrationSource:inspection('110'),loginSource:inspection('220')},
    native_stages:{registration:{request:{stage:'registration'},inspection_id:'registrationSource',reserved_at:'100',claimed_at:'101',reads:2,result:{kind:'registration_challenge',result:secret}},
      login:{request:{stage:'login'},inspection_id:'loginSource',reserved_at:'200',claimed_at:null,reads:0,result:null}},
    managed_logins:{other:{task:{transport:ledger('400','0')},book:{records:{other:{verification:signed('999')}}}},
      [selected]:{task:{login_operation_id:'selected-native-operation',claimed_at:'310',reads:8,deadline:'60000',transport:ledger('128','128'),result:null,private_sid_generation:'2',status_nonce:secret},
        book:{records:{[selected]:{admitted_at:'300',handoff_until:'5000',consumed_at:'301',verification:signed('302'),input:{approval:secret}}}}}},
    native_history:{right:{phase:'managed_login',native_operation_id:'selected-native-operation',result:{kind:'session_status',result:secret},session_attestation:{session_ref:secret}},
      wrong:{phase:'managed_login',native_operation_id:'other-native-operation',result:{kind:'explicit_login_required'},session_attestation:null}},
    session_status_recoveries:{other:{funding:{kind:'managed_login',operation_id:'other'},result:{cookie:secret}},
      provision:{funding:{kind:'provision'},result:{cookie:secret}},
      selected:{funding:{kind:'managed_login',operation_id:selected},request:{history_digest:'right'},created_at:'6000',expires_at:'36000',claimed_at:'6001',reads:3,
        completed_at:'6003',result:{status:{active:true},signature:secret},result_private_sid_generation:'2',source_inspection:inspection('6002')}},
    cookie:secret,raw_request:secret};
}

test('Native stages retain their own source inspection and distinguish an unclaimed zero-read original',()=>{
  const actual=projectNativeState(stateFixture()).native_stages;
  assert.equal(actual.count,2);
  assert.equal(actual.items[0].source.verification.observed_at,'110');
  assert.equal(actual.items[1].source.verification.observed_at,'220');
  assert.equal(actual.items[0].claimed,true);
  assert.equal(actual.items[1].claimed,false);
  assert.equal(actual.items[1].reads,0);
  assert.deepEqual(actual.items[1].result,{present:false,kind:null});
});

test('ManagedLogin joins its own approval, budget, history and recovery without borrowing Provision or another login',()=>{
  const actual=projectNativeState(stateFixture(),{managedOperationId:selected});
  assert.deepEqual(actual.managed_login.task.transport,{limit:'128',spent:'128',reserved:'0',remaining:'0'});
  assert.equal(actual.provision.transport.remaining,'96');
  assert.equal(actual.managed_login.approval.verification.observed_at,'302');
  assert.equal(actual.managed_login.task.deadline,'60000');
  assert.equal(actual.managed_login.history.count,1);
  assert.equal(actual.managed_login.history.items[0].result.kind,'session_status');
  assert.equal(actual.recoveries.count,1);
  assert.equal(actual.recoveries.items[0].source.verification.observed_at,'6002');
  assert.equal(actual.recoveries.items[0].history.phase,'managed_login');
  assert.equal(actual.recoveries.items[0].history.session_attestation,true);
  assert.equal(actual.managed_login.task.result.present,false);
  assert.equal(actual.recoveries.items[0].has_result,true,'recovery need not fill the original login result');
  assert.equal(Object.hasOwn(actual.recoveries.items[0],'kind'),false,'a signed recovery status has no NativeStageResult kind');
});

test('missing fresh recovery verification stays missing even when the old approval has verification',()=>{
  const state=stateFixture();state.session_status_recoveries.selected.source_inspection.verification=null;
  state.session_status_recoveries.selected.source_inspection.verified_at=null;
  state.session_status_recoveries.selected.result=null;
  const actual=projectNativeState(state,{managedOperationId:selected});
  assert.equal(actual.managed_login.approval.verification.present,true);
  assert.equal(actual.recoveries.items[0].source.verified,false);
  assert.equal(actual.recoveries.items[0].source.verification.present,false);
  assert.equal(actual.recoveries.items[0].source.verification.observed_at,null);
  assert.equal(actual.recoveries.items[0].has_result,false);
});

test('unknown maps, absent records, missing fields and explicit empty data remain distinguishable',()=>{
  const unknown=projectNativeState({},{managedOperationId:selected});
  assert.equal(unknown.native_stages.count,null);
  assert.equal(unknown.managed_login.present,null);
  assert.equal(unknown.recoveries.count,null);
  assert.equal(unknown.provision.transport.remaining,null);
  const state=stateFixture();delete state.origin_inspections.loginSource;delete state.native_stages.login.claimed_at;
  state.managed_logins={};state.session_status_recoveries={};
  const empty=projectNativeState(state,{managedOperationId:selected});
  assert.equal(empty.native_stages.items[1].source.present,false);
  assert.equal(empty.native_stages.items[1].claimed,null);
  assert.equal(empty.managed_login.present,false);
  assert.equal(empty.recoveries.count,0);
  delete state.origin_inspections;
  assert.equal(projectNativeState(state).native_stages.items[1].source.present,null);
});

test('ledger arithmetic accepts complete decimal u64 data and rejects incomplete, boolean, overflow and negative balances',()=>{
  const state=stateFixture();state.provision.transport={limit:'18446744073709551615',spent:'18446744073709551600',reservations:{a:'10'}};
  assert.equal(projectNativeState(state).provision.transport.remaining,'5');
  for(const invalid of [ledger('128','129'),{limit:'128',spent:'0'},ledger(true,'0'),ledger('18446744073709551616','0'),
    {limit:'128',spent:'0',reservations:{a:'129'}},{limit:'128',spent:'0',reservations:{a:false}},ledger('01','0'),ledger(128,0)]) {
    state.provision.transport=invalid;assert.equal(projectNativeState(state).provision.transport.remaining,null);
  }
});

test('projection emits only fixed enums, finite counts, timestamps and presence flags',()=>{
  const state=stateFixture();state.native_stages.login.result={kind:secret,request_bytes:secret};
  state.native_stages.login.reads=true;state.native_stages.login.reserved_at=secret;
  state.origin_inspections.loginSource.verification.verification.origin_state=secret;
  const actual=projectNativeState(state,{managedOperationId:selected}),text=JSON.stringify(actual);
  for(const forbidden of [secret,selected,'selected-native-operation','registrationSource','loginSource','history_digest','inspection_id','request_bytes','verify_nonce','signature','cookie']) {
    assert.equal(text.includes(forbidden),false,forbidden);
  }
  assert.equal(actual.native_stages.items[1].result.kind,'unknown');
  assert.equal(actual.native_stages.items[1].reads,null);
  assert.equal(actual.native_stages.items[1].reserved_at,null);
  assert.equal(actual.native_stages.items[1].source.verification.origin_state,'unknown');
  assert.equal(Object.hasOwn(actual.native_stages.items[0],'completed_at'),false);
});

test('large collections are explicitly truncated and malformed maps are not reported empty',()=>{
  const state=stateFixture();state.native_stages=Object.fromEntries(Array.from({length:19},(_,i)=>[String(i),state.native_stages.login]));
  const actual=projectNativeState(state).native_stages;
  assert.equal(actual.count,19);assert.equal(actual.omitted,3);assert.equal(actual.items.length,16);
  for(const invalid of [null,[],secret]) {state.native_stages=invalid;assert.equal(projectNativeState(state).native_stages.count,null);}
});

test('successful stage assertions perform no diagnostics, suspension, reads or output',async()=>{
  const runtime=new Proxy({},{get(){throw new Error('success path performed diagnostic I/O');}});
  await assertNativeStage(runtime,{stage:'native_registered'},'native_registered',{emit(){assert.fail('unexpected output');}});
});

function failingRuntime(overrides={}) {
  const calls=[];
  return {calls,runtime:{nativeTimings(){calls.push('timings');return {plan_created_at:100,plan_expires_at:'6100',identity_ready_at:102,provision_approved_at:105,events:[],omitted_events:0,raw:secret};},
    nativeRequests(){calls.push('requests');return {'/native-key/complete':2,[secret]:99};},
    async suspend(){calls.push('suspend');},readNativeDiagnosticState(){calls.push('snapshot');return stateFixture();},...overrides}};
}
const originalAssertion=error=>error.code==='ERR_ASSERTION' && error.actual==='approved' && error.expected==='native_session_ready' && error.operator==='strictEqual';

test('failure captures counters before suspension, reads after it and preserves the strict assertion',async()=>{
  const {runtime,calls}=failingRuntime();let report;
  const progress={stage:'approved',pending_operations:[secret],transport_remaining:'0',cookie:secret};
  await assert.rejects(assertNativeStage(runtime,progress,'native_session_ready',{managedOperationId:selected,emit:text=>{calls.push('emit');report=JSON.parse(text);}}),originalAssertion);
  assert.deepEqual(calls,['timings','requests','suspend','snapshot','emit']);
  assert.equal(report.snapshot_status,'available');assert.equal(report.pending_count,1);
  assert.equal(report.requests['/native-key/complete'],2);assert.equal(report.requests['/native-key/options'],0);
  assert.equal(report.timings.plan_created_at,'100');assert.equal(report.timings.plan_expires_at,'6100');
  assert.equal(report.state.recoveries.items[0].has_result,true);
  assert.equal(JSON.stringify(report).includes(secret),false);
  assert.match(report.failure_observed_at,/^[0-9]+$/);
  assert.equal(Object.hasOwn(report,'expired_at_send'),false);
});

for(const fault of ['timings','requests','suspend','snapshot','emit']) {
  test(`a diagnostic ${fault} failure never replaces the original assertion or prints exception details`,async()=>{
    const methods={timings:'nativeTimings',requests:'nativeRequests',suspend:'suspend',snapshot:'readNativeDiagnosticState'};
    const {runtime,calls}=failingRuntime(fault==='emit'?{}:{[methods[fault]](){throw new Error(secret);}});
    let output;
    await assert.rejects(assertNativeStage(runtime,{stage:'approved'},'native_session_ready',{emit:text=>{output=text;if(fault==='emit')throw new Error(secret);}}),originalAssertion);
    assert.equal(output.includes(secret),false);
    const report=JSON.parse(output);
    if(fault==='suspend'||fault==='snapshot')assert.equal(report.snapshot_status,'unavailable');
    if(fault==='suspend')assert.equal(calls.includes('snapshot'),false);
    if(fault==='requests')assert.equal(report.requests,null);
  });
}

test('missing snapshot is unavailable and unknown returned stages cannot enter diagnostic output',async()=>{
  const {runtime}=failingRuntime({readNativeDiagnosticState:()=>undefined});let output;
  await assert.rejects(assertNativeStage(runtime,{stage:secret},'participant_bound',{emit:text=>{output=text;}}),error=>error.actual===secret && error.expected==='participant_bound');
  assert.equal(output.includes(secret),false);assert.equal(JSON.parse(output).snapshot_status,'unavailable');
});

test('bounded timing retains completed and failed calls without changing their values or exceptions',async()=>{
  const recorder=createNativeTimings(),failure=new Error('original call failed');
  for(let i=0;i<18;i++)assert.equal(await recorder.run('provision_continue',async()=>i),i);
  await assert.rejects(recorder.run('managed_continue',async()=>{throw failure;}),error=>error===failure);
  assert.equal(recorder.facts.events.length,16);assert.equal(recorder.facts.omitted_events,3);
  assert.equal(recorder.facts.events.at(-1).kind,'managed_continue');
  assert.ok(recorder.facts.events.every(v=>Number.isSafeInteger(v.started_at) && v.finished_at>=v.started_at));
});


test('Identity diagnostic is failure-only, follows suspension, whitelists output and preserves errors',async()=>{
  const {runtime,calls}=failingRuntime({readIdentityNativeDiagnostic(){calls.push('identity');return {status:'available',cookie:secret,phase:{state:secret,result_kind:secret,result_present:true,private_sid_generation_present:false,stored_slots:{session_cookie:1,[secret]:8}}};}});
  let report;
  await assertNativeStage(runtime,{stage:'approved'},'approved',{managedOperationId:selected,emit:()=>assert.fail()});
  assert.deepEqual(calls,[]);
  await assert.rejects(assertNativeStage(runtime,{stage:'approved'},'native_session_ready',{managedOperationId:selected,emit:v=>{report=JSON.parse(v);}}),originalAssertion);
  assert.deepEqual(calls,['timings','requests','suspend','identity','snapshot']);
  assert.equal(report.identity.phase.state,'unknown');assert.equal(report.identity.phase.stored_slots.session_cookie,1);
  assert.equal(JSON.stringify(report).includes(secret),false);
  runtime.readIdentityNativeDiagnostic=()=>{throw Error(secret);};
  await assert.rejects(assertNativeStage(runtime,{stage:'approved'},'native_session_ready',{managedOperationId:selected,emit:v=>{report=JSON.parse(v);}}),originalAssertion);
  assert.deepEqual(report.identity,{status:'unavailable'});
});

test('real SQLite Identity projection isolates permit and scope, bounds bytes, and never emits private records',async()=>{
  const {mkdtempSync,rmSync,readFileSync}=await import('node:fs');
  const {tmpdir}=await import('node:os');const {join}=await import('node:path');
  const {execFileSync}=await import('node:child_process');
  const directory=mkdtempSync(join(tmpdir(),'identity-stage-'));
  const path=join(directory,'manifest.sqlite');
  const manifest={schema_version:1,scope:{role_scope:{context:{identity:{account_id:'account',environment:'test'}},provision_id:'provision'}},
    phases:[{phase:'managed_login',permit_id:selected,operation_id:'login',state:'unknown',result:null,result_private_sid_generation:null}],
    records:[{operation_id:'login',kind:'login_dispatch',stored:true,record_id:secret,plaintext_digest:secret},{operation_id:'other',kind:'session_cookie',stored:true}],cookie:secret};
  const save=value=>execFileSync('python3',['-c',`import sqlite3,sys
with sqlite3.connect(sys.argv[1]) as db:
 db.execute('CREATE TABLE IF NOT EXISTS custody_records(id TEXT PRIMARY KEY,payload TEXT)')
 db.execute('INSERT OR REPLACE INTO custody_records VALUES(?,?)',('custody-native-manifest-v1',sys.argv[2]))`,path,JSON.stringify(value)],{stdio:'ignore'});
  try {
    save(manifest);const before=readFileSync(path);
    const actual=readIdentityNativeDiagnostic(directory,'account','provision',selected);
    assert.equal(actual.status,'available');assert.equal(actual.phase.state,'unknown');
    assert.deepEqual(actual.phase.stored_slots,{login_dispatch:1,session_cookie:0,session_csrf:0});
    assert.equal(JSON.stringify(actual).includes(secret),false);assert.deepEqual(readFileSync(path),before);
    for(const args of [['wrong','provision',selected],['account','wrong',selected],['account','provision','other']])
      assert.equal(readIdentityNativeDiagnostic(directory,...args).status,'absent');
    manifest.phases.push({...manifest.phases[0]});save(manifest);
    assert.equal(readIdentityNativeDiagnostic(directory,'account','provision',selected).status,'unavailable');
    manifest.phases.pop();manifest.cookie='é'.repeat(32768);save(manifest);
    assert.equal(readIdentityNativeDiagnostic(directory,'account','provision',selected).status,'unavailable');
    assert.equal(readIdentityNativeDiagnostic(join(directory,'missing'),'account','provision',selected).status,'unavailable');
  } finally {rmSync(directory,{recursive:true,force:true});}
});
