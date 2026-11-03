import {directoryAdmissionInventoryV3} from './read_directory_admission_sql_v3.mjs';
// New paid Directory profile only. Existing ordinary record bytes stay untouched.
const header='custody_pending_current_floor_v2',once='custody_pending_current_once_v2';
const birthKey='manager-directory-pending-birth-v2',ordinaryKey='manager-directory-v1';
function inventory(state,capacity){
 if(!Number.isInteger(capacity)||capacity<=0||capacity>4294967295)throw Error('directory capacity unavailable');
 const sql=state.storage.sql,tables=sql.exec("SELECT type,name FROM sqlite_master WHERE type IN ('table','view','trigger') LIMIT 16").toArray().map(r=>`${r.type}:${r.name}`),has=n=>tables.includes(`table:${n}`);
 const records=has('custody_records')?sql.exec('SELECT id FROM custody_records LIMIT 3').toArray():null;
 const birth=has('custody_records')?sql.exec('SELECT id,substr(payload,1,32769) AS payload FROM custody_records WHERE id=? LIMIT 2',birthKey).toArray():null;
 const count=has(once)?sql.exec(`SELECT CAST(COUNT(*) AS TEXT) AS count FROM ${once}`).one().count:null;
 if(count!==null&&(!/^(0|[1-9][0-9]*)$/.test(count)||BigInt(count)>BigInt(capacity)))throw Error('directory inventory unavailable');
 return {tables,records,birth,metadata:has(header)?sql.exec(`SELECT id,substr(payload,1,32769) AS payload FROM ${header} LIMIT 2`).toArray():null,count,claims:has(once)?sql.exec(`SELECT id,substr(payload,1,32769) AS payload FROM ${once} ORDER BY id LIMIT ?`,capacity+1).toArray():null};
}
export function claimExecutionPendingActorV2(state,capacity,transition,readCapacity,readRowBound,validateAll){
 return state.storage.transactionSync(()=>{
  if(typeof validateAll!=='function')throw Error('full Directory verifier unavailable');
  const complete=directoryAdmissionInventoryV3(state,capacity,readCapacity,readRowBound);
  validateAll(JSON.stringify(complete));
  const before=inventory(state,capacity);
  // Only after full same-TX authentication of BOTH closed families, provide
  // the unchanged legacy Exec transition its own typed family projection.
  // Unknown records/tables have already failed the complete scan/verifier.
  before.records=before.records.filter(r=>r.id!=='manager-directory-read-birth-v3');
  before.tables=before.tables.filter(t=>t!=='table:custody_read_directory_header_v3'&&t!=='table:custody_read_directory_once_v3');
  if(!Array.isArray(before.records)||!before.records.some(v=>v.id===ordinaryKey)||before.records.some(v=>v.id!==ordinaryKey&&v.id!==birthKey)||!Array.isArray(before.birth)||before.birth.length>1)throw Error('directory ordinary or birth inventory unavailable');
  const installing=before.birth.length===0&&before.metadata===null&&before.claims===null;
  if(!installing&&(before.birth.length!==1||before.metadata===null||before.claims===null))throw Error('directory partial unavailable');
  const next=transition(JSON.stringify(before));
  if(!Array.isArray(next)||next.length!==4||next.some(v=>typeof v!=='string'))throw Error('directory transition unavailable');
  const [metadata,id,claim,birth]=next;
  if(!/^[0-9a-f]{64}$/.test(id)||[metadata,claim,birth].some(v=>new TextEncoder().encode(v).byteLength>32768))throw Error('directory row bound');
  const sql=state.storage.sql;
  if(installing){
   sql.exec(`CREATE TABLE ${header}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`);
   sql.exec(`CREATE TABLE ${once}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`);
   sql.exec(`INSERT INTO ${header}(id,payload) VALUES('singleton',?)`,metadata);
   sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)',birthKey,birth);
  }else{
   if(before.birth[0].payload!==birth)throw Error('directory birth immutable');
   sql.exec(`UPDATE ${header} SET payload=? WHERE id='singleton'`,metadata);
  }
  sql.exec(`INSERT INTO ${once}(id,payload) VALUES(?,?)`,id,claim);
  return claim;
 });
}
