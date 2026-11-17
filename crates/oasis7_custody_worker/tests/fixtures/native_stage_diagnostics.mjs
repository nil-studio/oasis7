import {projectNativeResponses} from './native_response_diagnostics.mjs';
// Failure-only observations of the local fixture, never an authorization check.
import assert from 'node:assert/strict';

const stages=['draft','approved','role_keys_ready','native_registered','native_session_ready','participant_bound','ready_for_setup','prepared','explicit_login_required','frozen'];
const resultKinds=['registration_challenge','binding_receipt','session_status','explicit_login_required'];
const paths=['/capabilities','/custody/management-session-status','/custody/approvals/verify','/machine-registration/options','/machine-registration/complete',
  '/custody/managed-bindings/options','/custody/managed-bindings/complete','/custody/managed-bindings/lookup',
  '/native-key/options','/native-key/complete','/custody/managed-bindings/session-status'];
const timingKinds=['restart','provision_continue','managed_approve','managed_continue'];
const maxItems=16;
const object=value=>value!==null && typeof value==='object' && !Array.isArray(value);
const choice=(value,values)=>values.includes(value)?value:'unknown';
const count=value=>Number.isSafeInteger(value) && value>=0?value:null;
const decimal=value=>typeof value==='string' && /^(0|[1-9][0-9]{0,19})$/.test(value) && BigInt(value)<=18446744073709551615n?value:null;
const time=value=>typeof value==='number' && count(value)!==null?String(value):decimal(value);
const present=value=>value===null?false:value===undefined?null:object(value)?true:null;
const timestampPresent=value=>value===null?false:decimal(value)!==null?true:null;
const field=(map,key)=>object(map) && typeof key==='string'?Object.hasOwn(map,key)?map[key]:null:undefined;
const entries=map=>object(map)?Object.entries(map):null;

function collection(values,project) {
  return values===null?{count:null,omitted:null,items:[]}:
    {count:values.length,omitted:Math.max(0,values.length-maxItems),items:values.slice(0,maxItems).map(project)};
}

function verification(signed) {
  const fact=signed?.verification;
  return {present:present(signed),observed_at:decimal(fact?.observed_at),proof_expires_at:decimal(fact?.proof_expires_at),
    source_session_expires_at:decimal(fact?.source_session_expires_at),source_session_idle_expires_at:decimal(fact?.source_session_idle_expires_at),
    purpose:choice(fact?.purpose,['consume_approval','inspect_origin']),proof_state:choice(fact?.proof_state,['active','expired']),
    origin_state:choice(fact?.origin_state,['active','revoked','unavailable'])};
}

function source(inspection) {
  return {present:present(inspection),created_at:decimal(inspection?.created_at),expires_at:decimal(inspection?.expires_at),
    claimed:timestampPresent(inspection?.claimed_at),claimed_at:decimal(inspection?.claimed_at),
    verified:timestampPresent(inspection?.verified_at),verified_at:decimal(inspection?.verified_at),verification:verification(inspection?.verification)};
}

function result(value) {
  return {present:present(value),kind:value===null?null:choice(value?.kind,resultKinds)};
}

function budget(ledger) {
  const limit=decimal(ledger?.limit),spent=decimal(ledger?.spent),reservations=entries(ledger?.reservations);
  let reserved=null,remaining=null;
  if(limit!==null && spent!==null && reservations!==null && reservations.every(([,v])=>decimal(v)!==null)) {
    const total=reservations.reduce((sum,[,v])=>sum+BigInt(v),0n);
    if(total<=BigInt(limit) && BigInt(spent)<=BigInt(limit)-total) {
      reserved=String(total);remaining=String(BigInt(limit)-BigInt(spent)-total);
    }
  }
  return {limit,spent,reserved,remaining};
}

function history(value) {
  return {present:present(value),phase:choice(value?.phase,['register','binding','initial_login','managed_login']),
    result:result(value?.result),session_attestation:present(value?.session_attestation)};
}

// IDs only join exact records in memory. None leave this projection.
export function projectNativeState(state,{managedOperationId}={}) {
  const native=entries(state?.native_stages),managed=field(state?.managed_logins,managedOperationId),task=managed?.task;
  const approval=field(managed?.book?.records,managedOperationId);
  const recoveries=entries(state?.session_status_recoveries);
  const selectedRecoveries=recoveries===null?null:recoveries.filter(([,v])=>managedOperationId===undefined?
    v?.funding?.kind==='provision':v?.funding?.kind==='managed_login' && v.funding.operation_id===managedOperationId);
  const histories=entries(state?.native_history);
  const selectedHistory=histories===null || typeof task?.login_operation_id!=='string'?null:
    histories.filter(([,v])=>v?.phase==='managed_login' && v.native_operation_id===task.login_operation_id);
  return {
    provision:{stage:choice(state?.provision?.stage,stages),deadline:decimal(state?.draft?.plan?.expires_at),transport:budget(state?.provision?.transport)},
    participant_originals:object(state?.participant_provisions)?Object.keys(state.participant_provisions).length:null,
    native_stages:collection(native,([,v])=>({stage:choice(v?.request?.stage,['registration','binding','login']),
      reserved_at:decimal(v?.reserved_at),claimed:timestampPresent(v?.claimed_at),claimed_at:decimal(v?.claimed_at),reads:count(v?.reads),
      result:result(v?.result),source:source(field(state?.origin_inspections,v?.inspection_id))})),
    managed_login:managedOperationId===undefined?null:{
      present:object(state?.managed_logins)?Object.hasOwn(state.managed_logins,managedOperationId):null,
      task:{present:present(task),claimed:timestampPresent(task?.claimed_at),claimed_at:decimal(task?.claimed_at),reads:count(task?.reads),
        deadline:decimal(task?.deadline),transport:budget(task?.transport),result:result(task?.result),private_sid_generation:timestampPresent(task?.private_sid_generation)},
      approval:{present:present(approval),admitted_at:decimal(approval?.admitted_at),handoff_until:decimal(approval?.handoff_until),
        consumed:timestampPresent(approval?.consumed_at),consumed_at:decimal(approval?.consumed_at),verification:verification(approval?.verification)},
      history:collection(selectedHistory,([,v])=>history(v)),
    },
    recoveries:collection(selectedRecoveries,([,v])=>({funding:choice(v?.funding?.kind,['provision','managed_login']),
      created_at:decimal(v?.created_at),expires_at:decimal(v?.expires_at),claimed:timestampPresent(v?.claimed_at),claimed_at:decimal(v?.claimed_at),
      reads:count(v?.reads),has_result:present(v?.result),completed_at:decimal(v?.completed_at),
      result_private_sid_generation:timestampPresent(v?.result_private_sid_generation),source:source(v?.source_inspection),
      history:history(field(state?.native_history,v?.request?.history_digest))})),
  };
}

export function createNativeTimings() {
  const facts={events:[],omitted_events:0};
  return {facts,async run(kind,operation) {
    const event={kind,started_at:Date.now(),finished_at:null};
    if(facts.events.length===maxItems){facts.events.shift();facts.omitted_events++;}
    facts.events.push(event);
    try{return await operation();}finally{event.finished_at=Date.now();}
  }};
}

function timings(facts) {
  return {plan_created_at:time(facts?.plan_created_at),plan_expires_at:time(facts?.plan_expires_at),identity_ready_at:time(facts?.identity_ready_at),
    provision_approved_at:time(facts?.provision_approved_at),omitted_events:count(facts?.omitted_events),
    events:Array.isArray(facts?.events)?facts.events.slice(-maxItems).map(v=>({kind:choice(v?.kind,timingKinds),started_at:time(v?.started_at),finished_at:time(v?.finished_at)})):null};
}

export async function assertNativeStage(runtime,progress,expected,{managedOperationId,emit=console.error}={}) {
  let failure;
  try{assert.equal(progress.stage,expected);}catch(error){failure=error;}
  if(!failure)return;
  const report={schema:'oasis7.custody-native-stage-diagnostic/v1',failure_observed_at:String(Date.now()),
    expected_stage:choice(expected,stages),stage:choice(progress?.stage,stages),
    pending_count:Array.isArray(progress?.pending_operations)?progress.pending_operations.length:null,
    transport_remaining:decimal(progress?.transport_remaining),timings:null,requests:null,responses:null,snapshot_status:'unavailable',state:null};
  try{report.responses=projectNativeResponses(runtime.nativeResponses());}catch{/* Never replace the original failure. */}
  try{report.timings=timings(runtime.nativeTimings());}catch{/* Keep unknown; never print exception details. */}
  try{const requests=runtime.nativeRequests();report.requests=Object.fromEntries(paths.map(path=>[path,
    object(requests)?Object.hasOwn(requests,path)?count(requests[path]):0:null]));}catch{/* A missing counter is not zero. */}
  try{
    // Freeze before the read. This timestamp is not the time of an authorization check.
    await runtime.suspend();
    const state=await runtime.readNativeDiagnosticState();
    if(!object(state))throw new Error('diagnostic state unavailable');
    report.state=projectNativeState(state,{managedOperationId});
    report.snapshot_status='available';
  }catch{/* Diagnostic failure must preserve the original strict assertion. */}
  try{emit(JSON.stringify(report));}catch{/* The original failure still propagates. */}
  throw failure;
}
