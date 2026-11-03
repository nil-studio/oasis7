// New dedicated class only. All auth/config/full-signed-history decisions are
// made by its private Rust transition before any DDL; no network in this file.
const header='custody_source_inspect_admission_v2';
const once='custody_source_inspect_once_v2';
const birth='execution-source-inspect-birth-v2';
const rowMax=4*32768;
function text(v, limit=rowMax) {
  if(typeof v!=='string'||new TextEncoder().encode(v).byteLength>limit)throw new Error('source inspect storage rejected');
  return v;
}
function inventory(state,capacity) {
  if(!Number.isSafeInteger(capacity)||capacity<1||capacity>0xffffffff)throw new Error('source inspect capacity rejected');
  const tables=state.storage.sql.exec("SELECT type,name FROM sqlite_master WHERE type IN ('table','view','trigger') ORDER BY type,name LIMIT 6").toArray().map(r=>`${r.type}:${r.name}`);
  const has=t=>tables.includes(`table:${t}`);
  const records=has('custody_records')?state.storage.sql.exec('SELECT id,substr(payload,1,131073) AS payload FROM custody_records ORDER BY id LIMIT 2').toArray():null;
  const meta=has(header)?state.storage.sql.exec(`SELECT id,substr(payload,1,16385) AS payload FROM ${header} ORDER BY id LIMIT 2`).toArray():null;
  const count=has(once)?state.storage.sql.exec(`SELECT CAST(COUNT(*) AS TEXT) AS n FROM ${once}`).one().n:null;
  // Capacity is supplied only by the private adapter's parsed server config,
  // never an inbound field. Keep count visible on overflow, but do not allocate it.
  const boundedCount=count!==null&&/^(0|[1-9][0-9]*)$/.test(count)&&BigInt(count)<=BigInt(capacity);
  const claims=boundedCount?state.storage.sql.exec(`SELECT id,substr(payload,1,131073) AS payload FROM ${once} ORDER BY id`).toArray():null;
  return {tables,records,meta,count,claims};
}
export function readPendingSourceInspect(state,capacity){return JSON.stringify(inventory(state,capacity));}
export function claimPendingSourceInspect(state,capacity,transition){
  return state.storage.transactionSync(()=>{
    const before=inventory(state,capacity);
    const next=transition(JSON.stringify(before));
    if(!Array.isArray(next)||next.length!==4)throw new Error('source inspect transition rejected');
    const [meta,id,claim,firstBirth]=next.map(v=>text(v));
    text(meta,16384);text(id,256);
    const fresh=before.records===null&&before.meta===null&&before.claims===null;
    if(fresh){
      if(before.tables.some(t=>t!=='table:_cf_METADATA'))throw new Error('source inspect partial storage');
      state.storage.sql.exec('CREATE TABLE custody_records(id TEXT PRIMARY KEY,payload TEXT NOT NULL)');
      state.storage.sql.exec(`CREATE TABLE ${header}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`);
      state.storage.sql.exec(`CREATE TABLE ${once}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`);
      state.storage.sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)',birth,firstBirth);
      state.storage.sql.exec(`INSERT INTO ${header}(id,payload) VALUES('singleton',?)`,meta);
    }else{
      // Neither partial nor unknown state is repaired. The callback already
      // strongly checked the immutable birth/header/full signed claim rows.
      if(before.records===null||before.meta===null||before.claims===null||before.records.length!==1||before.records[0].id!==birth||before.records[0].payload!==firstBirth)throw new Error('source inspect partial storage');
      const old=before.meta;
      if(old.length!==1||old[0].id!=='singleton')throw new Error('source inspect header');
      const result=state.storage.sql.exec(`UPDATE ${header} SET payload=? WHERE id='singleton' AND payload=?`,meta,old[0].payload);
      if(result.rowsWritten!==1)throw new Error('source inspect header conflict');
    }
    state.storage.sql.exec(`INSERT INTO ${once}(id,payload) VALUES(?,?)`,id,claim);
    return claim;
  });
}
export function authorizePendingSourceInspectNative(state,capacity,transition){
  return state.storage.transactionSync(()=>{
    const next=transition(JSON.stringify(inventory(state,capacity)));
    if(!Array.isArray(next)||next.length!==3)throw new Error('source inspect authorization rejected');
    const [id,original,claimed]=next.map(v=>text(v));text(id,256);
    const result=state.storage.sql.exec(`UPDATE ${once} SET payload=? WHERE id=? AND payload=?`,claimed,id,original);
    if(result.rowsWritten!==1)throw new Error('source inspect authorization conflict');
    return claimed;
  });
}
