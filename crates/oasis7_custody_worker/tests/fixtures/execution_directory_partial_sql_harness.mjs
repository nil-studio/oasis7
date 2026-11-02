// Private SQL callback seam only. This fixture's independent test domain/key
// is not a Core paid grant, Native proof or portable Current authority.
import {claimExecutionPendingActorV2} from '../../platform/execution_pending_actor_sql_v2.mjs';
const floor='custody_pending_current_floor_v2',once='custody_pending_current_once_v2',birthKey='manager-directory-pending-birth-v2';
const ordinary='{"component":"ordinary-sql-prerequisite"}';
const encode=new TextEncoder(),bytes=hex=>Uint8Array.from(hex.match(/../g)??[],v=>parseInt(v,16));
const message=proof=>encode.encode(JSON.stringify(['oasis7.test.directory-sql-storage',proof.claim]));
export class DirectoryPartialSqlHarness {
 constructor(state,env){this.state=state;this.env=env;}
 async fetch(request){const q=await request.json(),sql=this.state.storage.sql;
  if(q.op==='prepare'){sql.exec('CREATE TABLE custody_records(id TEXT PRIMARY KEY,payload TEXT NOT NULL)');sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)','manager-directory-v1',ordinary);return Response.json({ordinary_ready:true});}
  if(q.op==='drop-floor'){sql.exec(`DROP TABLE ${floor}`);return Response.json({fault_installed:true});}
  if(q.op==='drop-both'){sql.exec(`DROP TABLE ${floor}`);sql.exec(`DROP TABLE ${once}`);return Response.json({fault_installed:true});}
  if(q.op==='corrupt-birth'){const value=JSON.parse(sql.exec('SELECT payload FROM custody_records WHERE id=?',birthKey).one().payload);value.proof.signature_hex='00'.repeat(64);sql.exec('UPDATE custody_records SET payload=? WHERE id=?',JSON.stringify(value),birthKey);return Response.json({fault_installed:true});}
  if(q.op==='drop-birth'){sql.exec('DELETE FROM custody_records WHERE id=?',birthKey);return Response.json({fault_installed:true});}
  if(q.op==='inventory'){const exists=n=>sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",n).toArray().length===1;const births=sql.exec('SELECT payload FROM custody_records WHERE id=?',birthKey).toArray();return Response.json({floor_exists:exists(floor),claims:exists(once)?sql.exec(`SELECT COUNT(*) AS n FROM ${once}`).one().n:null,birth_exists:births.length===1,ordinary_unchanged:sql.exec('SELECT payload FROM custody_records WHERE id=?','manager-directory-v1').one().payload===ordinary});}
  try{
   const capacity=Number(this.env.FIXTURE_DIRECTORY_CAPACITY);if(!Number.isInteger(capacity)||capacity<=0)throw Error('capacity');
   const publicKey=await crypto.subtle.importKey('raw',bytes(this.env.FIXTURE_STORAGE_PUBLIC_KEY),{name:'Ed25519'},false,['verify']);
   const verify=async proof=>{if(!proof||typeof proof.signature_hex!=='string'||!/^[0-9a-f]{128}$/.test(proof.signature_hex)||!await crypto.subtle.verify({name:'Ed25519'},publicKey,bytes(proof.signature_hex),message(proof)))throw Error('signed storage evidence');};
   await verify(q.proof);
   const has=sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",once).toArray().length===1;
   const originals=has?sql.exec(`SELECT payload FROM ${once} ORDER BY id LIMIT ?`,capacity+1).toArray():[];
   const verified=new Set();for(const row of originals){const c=JSON.parse(row.payload);await verify(c.proof);verified.add(row.payload);}
   const birthRows=sql.exec('SELECT payload FROM custody_records WHERE id=?',birthKey).toArray();let verifiedBirth=null;if(birthRows.length===1){await verify(JSON.parse(birthRows[0].payload).proof);verifiedBirth=birthRows[0].payload;}else if(birthRows.length>1)throw Error('birth');
   const digest=await crypto.subtle.digest('SHA-256',encode.encode(JSON.stringify(['oasis7.test.directory-sql-once',q.proof.claim.physical_attempt_id])));
   const id=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');
   claimExecutionPendingActorV2(this.state,capacity,raw=>{
    const v=JSON.parse(raw);let count=0;let birth;
    if(v.metadata===null&&v.claims===null&&v.birth?.length===0){birth=JSON.stringify({schema:2,capacity,public_key:this.env.FIXTURE_STORAGE_PUBLIC_KEY,proof:q.proof});}
    else{
     if(!Array.isArray(v.metadata)||v.metadata.length!==1||v.metadata[0].id!=='singleton'||!Array.isArray(v.claims)||v.count!==String(v.claims.length)||v.birth?.length!==1||v.birth[0].payload!==verifiedBirth)throw Error('partial');
     const old=JSON.parse(v.metadata[0].payload),born=JSON.parse(verifiedBirth);
     if(old.schema!==2||old.capacity!==capacity||old.public_key!==this.env.FIXTURE_STORAGE_PUBLIC_KEY||old.count!==v.claims.length||born.capacity!==capacity||born.public_key!==this.env.FIXTURE_STORAGE_PUBLIC_KEY||old.first_attempt!==born.proof.claim.physical_attempt_id)throw Error('metadata');
     let found=false;for(const row of v.claims){if(row.id===id||!verified.has(row.payload))throw Error('replay or unmatched exact verified cache');if(JSON.stringify(JSON.parse(row.payload).proof)===JSON.stringify(born.proof))found=true;}
     if(!found)throw Error('birth reverse');count=old.count;birth=verifiedBirth;
    }
    if(count>=capacity)throw Error('full');
    return [JSON.stringify({schema:2,capacity,public_key:this.env.FIXTURE_STORAGE_PUBLIC_KEY,first_attempt:JSON.parse(birth).proof.claim.physical_attempt_id,count:count+1}),id,JSON.stringify({api_version:2,proof:q.proof,claimed_at:String(Date.now())}),birth];
   });return Response.json({claimed:true});
  }catch{return Response.json({rejected:true},{status:409});}
 }
}
export default {fetch(request,env){return env.HARNESS.get(env.HARNESS.idFromName(new URL(request.url).pathname)).fetch(request);}};
