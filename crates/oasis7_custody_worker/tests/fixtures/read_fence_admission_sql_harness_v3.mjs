// SQL callback component only. This independent test domain is NOT a Core Read
// grant, payment, protected issuer or portable Current. Core real Read signature
// and historical pin validation have separate Rust tests.
import {readFenceAdmissionInventoryV3,readFenceOrdinaryInventoryV3,consumeReadFenceAdmissionV3,consumeExecutionFenceAdmissionV3,transitionFenceOrdinaryV3} from '../../platform/read_fence_admission_sql_v3.mjs';
const profiles=[{profile:'execution_v2',prefix:'execution_fence_admission_v2',birth:'execution-fence-read-birth-v2'},{profile:'reconcile_read_v3',prefix:'read_fence_admission_v3',birth:'read-fence-read-birth-v3'}];
const ordinary=[{id:'recovery-fence-v1',payload:'{"component":"opaque-fence-business-storage-only"}'},{id:'recovery-fence-ledger-v1',payload:'{"component":"opaque-ledger-business-storage-only"}'}];
const encode=new TextEncoder(),bytes=hex=>Uint8Array.from(hex.match(/../g)??[],v=>parseInt(v,16)),message=proof=>encode.encode(JSON.stringify(['oasis7.test.read-fence-sql-storage.v3',proof.claim]));
const hash=async value=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',encode.encode(value))),v=>v.toString(16).padStart(2,'0')).join('');
export class ReadFenceSqlHarnessV3 {
 constructor(state,env){this.state=state;this.env=env;}
 async fetch(request){const q=await request.json(),sql=this.state.storage.sql,capacities=[this.env.FIXTURE_EXEC_CAPACITY,this.env.FIXTURE_READ_CAPACITY],rowLimit=65792;
 const exists=n=>sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",n).toArray().length===1;
 const summary=()=>({records:exists('custody_records')?sql.exec('SELECT id,payload FROM custody_records ORDER BY id').toArray():[],profiles:profiles.map(p=>({profile:p.profile,header:exists(p.prefix+'_header'),floor:exists(p.prefix+'_floor'),once:exists(p.prefix+'_once')?sql.exec(`SELECT id,payload FROM ${p.prefix}_once ORDER BY id`).toArray():null}))});
 if(q.op==='identity')return Response.json({object_id:this.state.id.toString()});
 if(q.op==='prepare'){sql.exec('CREATE TABLE custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)');for(const r of ordinary)sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)',r.id,r.payload);return Response.json({ready:true});}
 if(q.op==='summary')return Response.json(summary());
 if(q.op==='drop-floor'){sql.exec('DROP TABLE read_fence_admission_v3_floor');return Response.json({fault:true});}
 if(q.op==='drop-header'){sql.exec('DROP TABLE read_fence_admission_v3_header');return Response.json({fault:true});}
 if(q.op==='drop-birth'){sql.exec('DELETE FROM custody_records WHERE id=?',profiles[1].birth);return Response.json({fault:true});}
 if(q.op==='drop-fence'){sql.exec("DELETE FROM custody_records WHERE id='recovery-fence-v1'");return Response.json({fault:true});}
 if(q.op==='corrupt-birth'){const row=sql.exec('SELECT payload FROM custody_records WHERE id=?',profiles[1].birth).one();const v=JSON.parse(row.payload);v.proof.signature_hex='00'.repeat(64);sql.exec('UPDATE custody_records SET payload=? WHERE id=?',JSON.stringify(v),profiles[1].birth);return Response.json({fault:true});}
 if(q.op==='unknown'){sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)','unknown-fifth-record','{}');return Response.json({fault:true});}
 if(q.op==='unknown-table'){sql.exec('CREATE TABLE unknown_profile(payload TEXT)');return Response.json({fault:true});}
 try {
  const publicKey=await crypto.subtle.importKey('raw',bytes(this.env.FIXTURE_STORAGE_PUBLIC_KEY),{name:'Ed25519'},false,['verify']);
  const verify=async proof=>{if(!proof||proof.claim.object_id!==this.state.id.toString()||typeof proof.signature_hex!=='string'||!/^[0-9a-f]{128}$/.test(proof.signature_hex)||!await crypto.subtle.verify({name:'Ed25519'},publicKey,bytes(proof.signature_hex),message(proof)))throw Error('storage fixture signature');};
  // Async fixture signature checks are rebound by exact full payload cache in
  // the actual synchronous SQL callback; no stale Boolean authorizes new rows.
  const raw=readFenceAdmissionInventoryV3(this.state,capacities,rowLimit),inventory=JSON.parse(raw),verified=new Set();
  for(const p of inventory.admissions){if(p.once)for(const row of p.once){await verify(JSON.parse(row.payload).proof);verified.add(row.payload);}}
  const verifiedBirth=new Set();for(const p of profiles){const row=inventory.records.find(r=>r.id===p.birth);if(row){await verify(JSON.parse(row.payload).proof);verifiedBirth.add(row.payload);}}
  const project=(raw,businessPayloads=true)=>{
   const v=JSON.parse(raw);if(v.records.length>=5||new Set(v.records.map(r=>r.id)).size!==v.records.length)throw Error('record bounds');
   const knownTables=new Set(['custody_records','custody_audit','sqlite_sequence','_cf_METADATA']);
   for(let i=0;i<2;i++){
    const p=profiles[i],a=v.admissions[i],birth=v.records.find(r=>r.id===p.birth);
    if(a.header===null&&a.floor===null&&a.once===null&&a.count===null){if(birth)throw Error('orphan birth');continue;}
    if(!birth||!verifiedBirth.has(birth.payload)||a.header?.length!==1||a.floor?.length!==1||!Array.isArray(a.once)||a.header[0].id!=='singleton'||a.floor[0].id!=='singleton'||a.count!==String(a.once.length))throw Error('partial');
    const h=JSON.parse(a.header[0].payload),f=JSON.parse(a.floor[0].payload),born=JSON.parse(birth.payload);
    if(h.count!==a.count||JSON.stringify(h)!==JSON.stringify(f)||born.capacity!==capacities[i]||h.capacity!==capacities[i]||born.proof.claim.profile!==p.profile||h.first_attempt!==born.proof.claim.attempt)throw Error('header/floor/cap');
    if(!a.once.some(r=>JSON.stringify(JSON.parse(r.payload).proof)===JSON.stringify(born.proof)))throw Error('birth reverse');
    for(const row of a.once)if(!verified.has(row.payload))throw Error('unmatched verified cache');
    for(const kind of ['header','floor','once'])knownTables.add(`${p.prefix}_${kind}`);
   }
   for(const r of v.catalogue){if(r.type==='table'&&knownTables.has(r.name))continue;if(r.type==='index'&&r.sql===null&&[...knownTables].some(n=>r.name===`sqlite_autoindex_${n}_1`))continue;throw Error('unknown schema');}
   for(const r of v.records)if(!ordinary.some(b=>b.id===r.id)&&!profiles.some(p=>p.birth===r.id))throw Error('unknown record');
   const business=v.records.filter(r=>ordinary.some(b=>b.id===r.id));
   if(!business.some(r=>r.id==='recovery-fence-v1'))throw Error('business partial');
   for(const r of business)if(businessPayloads?ordinary.find(b=>b.id===r.id)?.payload!==r.payload:r.payload!==null)throw Error('business bytes or pre-once payload read');
   return JSON.stringify({records:business,table_names:v.catalogue.filter(r=>r.type==='table'&&['custody_records','custody_audit','sqlite_sequence','_cf_METADATA'].includes(r.name)).map(r=>`table:${r.name}`),audit_count:v.audit_count,audit_rows:v.audit_rows});
  };
  if(q.op==='read'){return Response.json(JSON.parse(readFenceOrdinaryInventoryV3(this.state,capacities,rowLimit,ordinary[0].payload,project)));}
  if(q.op==='no-op-transition'){transitionFenceOrdinaryV3(this.state,capacities,rowLimit,project,()=>['','','','']);return Response.json({unchanged:true});}
  if(q.op!=='seed-execution'&&this.env.FIXTURE_READ_ENABLED!=='true')throw Error('new admission disabled');
  await verify(q.proof);if(q.proof.claim.profile!=='reconcile_read_v3'&&q.op!=='seed-execution')throw Error('closed fixture purpose');
  const id=await hash(JSON.stringify(['oasis7.test.read-fence-sql-once.v3',q.proof.claim.profile,q.proof.claim.attempt]));
  const index=q.op==='seed-execution'?0:1,p=profiles[index];
  const next=raw=>{
   project(raw,false);const v=JSON.parse(raw),a=v.admissions[index];let count=0,birth;
   if(a.header===null){birth=JSON.stringify({schema:3,capacity:capacities[index],proof:q.proof});}
   else {const old=JSON.parse(a.header[0].payload);count=Number(old.count);if(a.once.some(row=>row.id===id))throw Error('once');birth=v.records.find(r=>r.id===p.birth).payload;}
   if(count>=Number(capacities[index]))throw Error('capacity');
   const header=JSON.stringify({count:String(count+1),capacity:capacities[index],first_attempt:JSON.parse(birth).proof.claim.attempt});
   return [birth,header,header,id,JSON.stringify({schema:3,proof:q.proof}),String(Date.now()),String(q.proof.claim.expires_at)];
  };
  if(q.op==='seed-execution'){
   if(q.proof.claim.profile!=='execution_v2')throw Error('fixture profile');
   consumeExecutionFenceAdmissionV3(this.state,capacities,rowLimit,next);return Response.json({fixture_storage_only:true});
  }
  if(q.op==='reject-before-write'){consumeReadFenceAdmissionV3(this.state,capacities,rowLimit,()=>{throw Error('causal gate denied');});}
  if(q.op==='claim'||q.op==='late-denial'){consumeReadFenceAdmissionV3(this.state,capacities,rowLimit,next);return Response.json({storage_only:true},{status:q.op==='late-denial'?409:200});}
  throw Error('unknown fixture operation');
 }catch{return Response.json({rejected:true},{status:409});}
 }
}
export default {fetch(request,env){return env.HARNESS.get(env.HARNESS.idFromName(new URL(request.url).pathname)).fetch(request);}};
