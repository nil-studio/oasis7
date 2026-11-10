import assert from 'node:assert/strict';
import test from 'node:test';
import {startRejectionTrace, setRejectionFact, failRejectionTrace} from '../platform/rejection_diagnostic.mjs';

function capture(run) {
  const messages=[], original=console.error, clock=Date.now;
  let reads=0, now=1000;
  console.error=value=>messages.push(JSON.parse(value));
  Date.now=()=>{reads++;return now;};
  try {run({messages,setNow:value=>{now=value;},reads:()=>reads});}
  finally {console.error=original;Date.now=clock;}
}
test('default diagnostics do not read clock or emit, including failures',()=>capture(({messages,reads})=>{
  for(const enabled of [undefined,false,'false','true']) {
    const trace=startRejectionTrace(enabled,'activation',null,null);
    setRejectionFact(trace,'source_expires_at',1200);
    failRejectionTrace(trace,'core.checks.before','binding_mismatch');
  }
  assert.equal(reads(),0);assert.deepEqual(messages,[]);
}));
test('failure captures rejection time once with relative safe facts and unknown missing data',()=>capture(({messages,setNow})=>{
  const trace=startRejectionTrace(true,'activation',1000,900);
  setRejectionFact(trace,'source_expires_at',6100);
  setRejectionFact(trace,'execution_root_deadline',8000);
  setNow(6010);failRejectionTrace(trace,'core.checks.before','binding_mismatch');
  setNow(7000);failRejectionTrace(trace,'activate.store','unknown');
  assert.equal(messages.length,1);
  assert.deepEqual(messages[0],{custody_rejection:{selection:'first_observed_failure',time_basis:'failure_observed',kind:'activation',stage:'core.checks.before',category:'binding_mismatch',elapsed_ms:5010,source_age_ms:5110,source_freshness_remaining_ms:-110,source_remaining_ms:90,execution_root_remaining_ms:1990,lease_remaining_ms:'unknown',lifetime_remaining_ms:'unknown',lifetime_status_remaining_ms:'unknown',proof_remaining_ms:'unknown',observation_remaining_ms:'unknown'}});
}));
test('untrusted names, values and exception details cannot enter diagnostic output',()=>capture(({messages})=>{
  const secret='token SID proof signature secret';
  const trace=startRejectionTrace(true,secret,null,null);
  setRejectionFact(trace,secret,secret);
  setRejectionFact(trace,'lease_until',secret);
  failRejectionTrace(trace,secret,secret);
  assert.equal(messages.length,1);
  assert.equal(messages[0].custody_rejection.stage,'unknown');
  assert.equal(messages[0].custody_rejection.category,'unknown');
  assert.equal(JSON.stringify(messages).includes(secret),false);
}));
test('invalid clock is unknown, and diagnostics cannot throw into business error paths',()=>capture(({messages})=>{
  const trace=startRejectionTrace(true,'client',null,null);
  Date.now=()=>NaN;
  failRejectionTrace(trace,'client.admit','unknown');
  assert.equal(messages[0].custody_rejection.elapsed_ms,'unknown');
  const other=startRejectionTrace(true,'client',null,null);
  console.error=()=>{throw new Error('logging unavailable');};
  assert.doesNotThrow(()=>failRejectionTrace(other,'client.admit','unknown'));
}));

test('check time uses the original business timestamp without another clock read',()=>capture(({messages,setNow,reads})=>{
  const trace=startRejectionTrace(true,'activation',1000,900);
  setNow(8000);const before=reads();
  failRejectionTrace(trace,'core.checks.final','binding_mismatch',5900);
  assert.equal(reads(),before);
  assert.equal(messages[0].custody_rejection.time_basis,'check_time');
  assert.equal(messages[0].custody_rejection.source_age_ms,5000);
  assert.equal(messages[0].custody_rejection.source_freshness_remaining_ms,0);
}));

// Static adapter contracts supplement the executable sink tests. They do not
// replace compiling Wasm or running the real role/Account integration suite.
import {readFileSync} from 'node:fs';
const source=name=>readFileSync(new URL(`../src/${name}`,import.meta.url),'utf8');
test('Rust adapter defaults off before any JS call and never serializes errors',()=>{
  const trace=source('rejection_diagnostic.rs');
  assert.ok(trace.indexOf('return Self(None)')<trace.indexOf('Self(start('));
  assert.match(trace,/!= Some\("true"\)/);
  assert.match(trace,/if self\.0\.is_none\(\)\s*\{\s*return;/);
  assert.equal((trace.match(/wasm_bindgen\(catch,/g)||[]).length,3);
  assert.doesNotMatch(trace,/console_|format!|to_string\(error|serde_json/);
  assert.match(trace,/ProvisionApprovalError::Admission\(error\) => match error/);
  assert.match(trace,/ApprovalAdmissionError::Expired => "admission_expired"/);
  assert.match(trace,/ProvisionError::Expired => "provision_expired"/);
});
test('activation captures before join completes and core checks use their original timestamp',()=>{
  const entry=source('owner_test_session_dispatch.rs');
  const join=entry.slice(entry.indexOf('let (attested, enrolled, observed) = activation_inputs('),entry.indexOf('let attestation: TestSessionPrivateAttestation',entry.indexOf('let (attested, enrolled, observed)')));
  assert.match(join,/attest\(env, &attestation_request\)\.await\.map_err\(\|e\|\s*\{\s*diagnostic\.fail\("join.attestation"\);\s*e/);
  assert.match(join,/enrollment\(env, enrollment\)[\s\S]*?\.await\s*\.map_err\(\|e\| report\("join.enrollment", e\)\)/);
  assert.match(join,/runtime_observe\([\s\S]*?\.await\s*\.map_err\(\|e\| report\("join.observation", e\)\)/);
  const store=source('test_session_store_preflight.rs');
  assert.match(store,/diagnostic\.core_at\(check_stage, &e, t\);\s*rejected\(\)/);
  assert.match(store,/diagnostic\.core_at\("core.transition.final", &e, t\);\s*rejected\(\)/);
  assert.match(store,/"core.checks.before"[\s\S]*?"core.checks.final"/);
  assert.match(source('provision_native_store.rs'),/diagnostic: Option<&activation_diagnostic::Trace>/);
});
test('client phases preserve returned errors and classify before core errors are erased',()=>{
  for(const name of ['identity_test_session_client_runtime.rs','owner_test_session_client_dispatch.rs','owner_test_session_client.rs']){
    const code=source(name);
    assert.match(code,/result\.map_err\(\|error\|\s*\{\s*diagnostic\.fail\(stage.get\(\)\);\s*error\s*\}\)/);
  }
  const runtime=source('identity_test_session_client_runtime.rs');
  for(const stage of ['client.admit','client.claim_source','client.source_verify','client.record_source','client.pre_decrypt','client.private_read','client.post_decrypt','client.claim_lifetime','client.lifetime','client.lifetime_pin','client.current','client.complete'])assert.ok(runtime.includes(`stage.set("${stage}")`),stage);
  assert.match(source('test_session_client_authority.rs'),/core_at\("proof.prepare", &error, time.get\(\)\);\s*rejected\(\)/);
  assert.match(source('test_session_client_store.rs'),/core_at\("account.challenge", &e, time.get\(\)\);\s*rejected\(\)/);
});
test('original integration roles explicitly enable the sink without selecting another artifact',()=>{
  const fixture=readFileSync(new URL('./fixtures/test-session-support.mjs',import.meta.url),'utf8');
  for(const [suffix,group] of [['','lifecycle'],['-activation','activation'],['-alarm','alarm'],['-paid','paid'],['-results','results'],['-tools','tools']]) {
    const entry=readFileSync(new URL(`./test-session${suffix}.integration.test.mjs`,import.meta.url),'utf8');
    assert.match(entry,/import \{createTestSessionSuite\} from '\.\/fixtures\/test-session-support\.mjs';/);
    assert.ok(entry.includes(`createTestSessionSuite().${group}();`),`${group} uses the shared diagnostic fixture`);
  }
  assert.match(fixture,/worker.bindings=\{\.\.\.worker.bindings,CUSTODY_REJECTION_DIAGNOSTIC:'true',CUSTODY_TEST_SESSION_ENABLED:'true'/);
  // The existing optional harness conditions remain optional, not forced by diagnostics.
  assert.match(fixture,/worker.name==='owner' && ownerDiagnosticActive && \(process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true'\|\|setupDiagnostic\)/);
  assert.match(fixture,/if\(kind==='finish'\)for\(const control of \['initialize','tools_list'\]\)/);
});
