// Storage component only. Independent fixture signatures are not Core paid
// proofs or Directory/Native/Current authority. Actual SQLite adapter is used.
import {claimReadDirectoryV3, ordinaryDirectoryAdmissionV3} from '../../platform/read_directory_admission_sql_v3.mjs';
import {claimExecutionPendingActorV2} from '../../platform/execution_pending_actor_sql_v2.mjs';
const defs={execution:{birth:'manager-directory-pending-birth-v2',header:'custody_pending_current_floor_v2',once:'custody_pending_current_once_v2'},read:{birth:'manager-directory-read-birth-v3',header:'custody_read_directory_header_v3',once:'custody_read_directory_once_v3'}};
const ordinary='{"storage_component":"original-business-bytes"}';
const encoder=new TextEncoder(),fromHex=h=>Uint8Array.from(h.match(/../g)??[],x=>parseInt(x,16));
const message=p=>encoder.encode(JSON.stringify(['oasis7.test.read-directory-storage-v3',p.claim]));
const ddl=t=>`CREATE TABLE ${t}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`;
export class ReadDirectorySqlV3Harness {
 constructor(state,env){this.state=state;this.env=env;}
 async fetch(request){const q=await request.json(),sql=this.state.storage.sql;
  if(q.op==='prepare'){sql.exec(ddl('custody_records'));sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)','manager-directory-v1',ordinary);return Response.json({ready:true});}
  const exists=t=>sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",t).toArray().length===1;
  if(q.op==='fault'){const p=defs[q.profile];if(q.kind==='birth')sql.exec('DELETE FROM custody_records WHERE id=?',p.birth);else if(q.kind==='header')sql.exec(`DROP TABLE ${p.header}`);else if(q.kind==='once')sql.exec(`DROP TABLE ${p.once}`);else if(q.kind==='unknown')sql.exec("INSERT INTO custody_records(id,payload) VALUES('unknown','{}')");else if(q.kind==='tamper'){const row=sql.exec(`SELECT id,payload FROM ${p.once} LIMIT 1`).one();const value=JSON.parse(row.payload);value.proof.signature_hex='00'.repeat(64);sql.exec(`UPDATE ${p.once} SET payload=? WHERE id=?`,JSON.stringify(value),row.id);}return Response.json({fault:true});}
  if(q.op==='inventory'){return Response.json({ordinary:sql.exec('SELECT payload FROM custody_records WHERE id=?','manager-directory-v1').one().payload,records:sql.exec('SELECT id,payload FROM custody_records ORDER BY id').toArray(),profiles:Object.fromEntries(Object.entries(defs).map(([name,p])=>[name,{header:exists(p.header)?sql.exec(`SELECT id,payload FROM ${p.header}`).toArray():null,once:exists(p.once)?sql.exec(`SELECT id,payload FROM ${p.once} ORDER BY id`).toArray():null}]))});}
  try {
   const cap=Number(this.env.CAPACITY), verified=new Set();
   const publicKey=await crypto.subtle.importKey('raw',fromHex(this.env.PUBLIC_KEY),{name:'Ed25519'},false,['verify']);
   const verify=async proof=>{if(!proof||!/^[0-9a-f]{128}$/.test(proof.signature_hex)||!await crypto.subtle.verify({name:'Ed25519'},publicKey,fromHex(proof.signature_hex),message(proof)))throw Error('fixture signature');};
   for(const p of Object.values(defs)){if(exists(p.once))for(const row of sql.exec(`SELECT payload FROM ${p.once} LIMIT ?`,cap+1).toArray()){await verify(JSON.parse(row.payload).proof);verified.add(row.payload);}const born=sql.exec('SELECT payload FROM custody_records WHERE id=?',p.birth).toArray();for(const row of born){await verify(JSON.parse(row.payload).proof);verified.add(row.payload);}}
   const check=v=>{for(const [name,p]of Object.entries(defs)){const old=v[name];if(old===null)continue;if(old.birth.length!==1||old.header.length!==1||old.header[0].id!=='singleton'||old.once.length===0||old.once.length>cap)throw Error('partial');const birth=JSON.parse(old.birth[0].payload),header=JSON.parse(old.header[0].payload);if(!verified.has(old.birth[0].payload)||birth.capacity!==cap||header.capacity!==cap||header.count!==old.once.length||header.first!==birth.proof.claim.attempt||header.public_key!==this.env.PUBLIC_KEY)throw Error('header');let found=false;for(const row of old.once){if(!verified.has(row.payload)||row.id!==JSON.parse(row.payload).proof.claim.attempt)throw Error('exact cache');if(JSON.stringify(JSON.parse(row.payload).proof)===JSON.stringify(birth.proof))found=true;}if(!found)throw Error('reverse');}return v;};
   if(q.op==='ordinary'){const result=ordinaryDirectoryAdmissionV3(this.state,cap,cap,65536,raw=>check(JSON.parse(raw)),()=>sql.exec('SELECT payload FROM custody_records WHERE id=?','manager-directory-v1').one().payload);if(result!==ordinary)throw Error('business');return Response.json({ordinary:true});}
   await verify(q.proof);if(q.proof.claim.profile!==q.profile)throw Error('profile');
   if(q.op==='exec-paid'){
    if(q.profile!=='execution')throw Error('profile');
    claimExecutionPendingActorV2(this.state,cap,raw=>{
     const old=JSON.parse(raw);
     const read=defs.read;
     const birthRows=sql.exec('SELECT id,payload FROM custody_records WHERE id=?',read.birth).toArray();
     const readRows=birthRows.length||exists(read.header)||exists(read.once)?{birth:birthRows,header:exists(read.header)?sql.exec(`SELECT id,payload FROM ${read.header}`).toArray():[],once:exists(read.once)?sql.exec(`SELECT id,payload FROM ${read.once} ORDER BY id`).toArray():[]}:null;
     check({execution:old.metadata===null?null:{birth:old.birth,header:old.metadata,once:old.claims},read:readRows});
     if(q.disabled)throw Error('admission disabled; historical verification still ran');
     if(old.claims?.some(r=>r.id===q.proof.claim.attempt))throw Error('duplicate');
     const count=old.claims?.length??0;if(count>=cap)throw Error('capacity');
     const birth=old.birth?.[0]?.payload??JSON.stringify({capacity:cap,proof:q.proof});
     return[JSON.stringify({capacity:cap,count:count+1,first:JSON.parse(birth).proof.claim.attempt,public_key:this.env.PUBLIC_KEY}),q.proof.claim.attempt,JSON.stringify({proof:q.proof}),birth];
    },cap,65536,raw=>check(JSON.parse(raw)));return Response.json({claimed:true});
   }
   if(q.profile==='execution'){
    // Preexisting independently signed Exec metadata is fixture setup only.
    // New production Read code must inspect/authenticate it without rewriting it.
    const p=defs.execution;this.state.storage.transactionSync(()=>{if(exists(p.header))throw Error('duplicate');sql.exec(ddl(p.header));sql.exec(ddl(p.once));const birth=JSON.stringify({capacity:cap,proof:q.proof});sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)',p.birth,birth);sql.exec(`INSERT INTO ${p.header}(id,payload) VALUES('singleton',?)`,JSON.stringify({capacity:cap,count:1,first:q.proof.claim.attempt,public_key:this.env.PUBLIC_KEY}));sql.exec(`INSERT INTO ${p.once}(id,payload) VALUES(?,?)`,q.proof.claim.attempt,JSON.stringify({proof:q.proof}));});return Response.json({seeded:true});
   }
   claimReadDirectoryV3(this.state,cap,cap,65536,raw=>{const v=check(JSON.parse(raw)),old=v.read;if(old?.once.some(r=>r.id===q.proof.claim.attempt))throw Error('duplicate');const count=old?.once.length??0;if(count>=cap)throw Error('capacity');const birth=old?.birth[0].payload??JSON.stringify({capacity:cap,proof:q.proof});return[birth,JSON.stringify({capacity:cap,count:count+1,first:JSON.parse(birth).proof.claim.attempt,public_key:this.env.PUBLIC_KEY}),q.proof.claim.attempt,JSON.stringify({proof:q.proof}),q.proof.claim.until];});return Response.json({claimed:true});
  }catch{return Response.json({rejected:true},{status:409});}
 }
}
export default {fetch(request,env){return env.HARNESS.get(env.HARNESS.idFromName(new URL(request.url).pathname)).fetch(request);}};
