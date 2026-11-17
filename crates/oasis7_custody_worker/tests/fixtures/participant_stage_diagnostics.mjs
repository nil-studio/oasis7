// Failure-only fixture observations, never authorization or a workflow retry.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';

const routes=['/participant/policy-original','/participant/authorize','/participant/submit','/participant/lookup','/participant/owner-view'];
const steps=['policy','sign_bundle','authorize','submit','owner_view'];
const callbacks=['policy','admission','final'];
const outcomes=['received','pending','committed','rejected','expired','unknown','history_unavailable'];
const object=v=>v!==null && typeof v==='object' && !Array.isArray(v);
const count=v=>Number.isSafeInteger(v)&&v>=0?v:null;
const presence=v=>v===null?false:object(v)?true:null;
const flags=(v,names)=>object(v)?Object.fromEntries(names.map(n=>[n,Object.hasOwn(v,n)])):null;

export function projectParticipantState(state) {
  const originals=object(state?.participant_provisions)?Object.values(state.participant_provisions):null;
  return {count:originals?.length??null,omitted:originals?Math.max(0,originals.length-16):null,
    items:originals?.slice(0,16).map(v=>({reads:count(v?.oracle_reads),claims:flags(v?.claims,steps),callbacks:flags(v?.callbacks,callbacks),
      policy:presence(v?.policy),admission:presence(v?.admission),submit:presence(v?.submit),lookup:presence(v?.lookup),
      owner_view:presence(v?.owner_view),result:presence(v?.result)}))??[]};
}

export function readParticipantState(persist,account,provision) {
  // Bound raw SQLite bytes before parsing. Nothing read is printed by Python.
  const raw=execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
root=Path(sys.argv[1]).resolve(strict=True)
found=[]
for i,p in enumerate(root.rglob('*.sqlite')):
 if i>=256: raise SystemExit(2)
 if p.is_symlink() or not p.resolve().is_relative_to(root): raise SystemExit(2)
 db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True,timeout=0)
 try:
  db.execute('PRAGMA query_only=ON')
  try: rows=db.execute('SELECT length(CAST(payload AS BLOB)),substr(CAST(payload AS BLOB),1,2097153) FROM custody_records WHERE id=? LIMIT 2',('custody-provision-approval-v1',)).fetchall()
  except sqlite3.OperationalError: continue
  if len(rows)>1: raise SystemExit(2)
  for size,raw in rows:
   if not isinstance(size,int) or size>2097152: raise SystemExit(2)
   data=json.loads(raw)
   state=data['state'];draft=state['draft'];entry=draft['entry']
   if entry['custody_account_id']==sys.argv[2] and entry['provision_id']==sys.argv[3] and draft['plan']['environment']=='test':
    found.append(state)
    if len(found)>1: raise SystemExit(2)
 finally: db.close()
if len(found)!=1: raise SystemExit(2)
sys.stdout.write(json.dumps(found[0],ensure_ascii=False))
`,resolve(persist),account,provision],{encoding:'utf8',timeout:5000,maxBuffer:2*1024*1024+1,stdio:['ignore','pipe','ignore']});
  if(Buffer.byteLength(raw)>2*1024*1024)throw new Error('participant snapshot unavailable');
  return JSON.parse(raw);
}

export async function assertParticipantBound(runtime,progress,{emit=console.error,read=readParticipantState}={}) {
  let failure;
  try{assert.equal(progress.stage,'participant_bound');}catch(error){failure=error;}
  if(!failure)return;
  const report={schema:'oasis7.custody-participant-stage-diagnostic/v1',stage:['native_session_ready','participant_bound'].includes(progress?.stage)?progress.stage:'unknown',
    pending_count:Array.isArray(progress?.pending_operations)?progress.pending_operations.length:null,requests:null,responses:null,snapshot_status:'unavailable',participants:null};
  try{const requests=runtime.nativeRequests();report.requests=Object.fromEntries(routes.map(p=>[p,object(requests)?count(requests[p]??0):null]));}catch{}
  try{const responses=runtime.worldResults();if(Array.isArray(responses))report.responses=responses.filter(v=>routes.includes(v?.path)).slice(-16).map(v=>({path:v.path,
    http:Number.isInteger(v.http)&&v.http>=100&&v.http<=599?v.http:null,outcome:outcomes.includes(v?.outcome?.status)?v.outcome.status:'unknown'}));}catch{}
  try{await runtime.suspend();report.participants=projectParticipantState(read(runtime.persist,runtime.account,progress.provision_id));report.snapshot_status='available';}catch{}
  try{emit(JSON.stringify(report));}catch{}
  throw failure;
}
