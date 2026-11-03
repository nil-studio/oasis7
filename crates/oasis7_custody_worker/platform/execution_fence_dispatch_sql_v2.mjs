// Dedicated one-attempt object. Strong private transition precedes every DDL.
const once='custody_execution_fence_dispatch_once_v2';
const header='custody_execution_fence_dispatch_header_v2';
function inventory(state){
  const tables=state.storage.sql.exec("SELECT type,name FROM sqlite_master WHERE type IN ('table','view','trigger') ORDER BY type,name LIMIT 5").toArray().map(r=>`${r.type}:${r.name}`);
  const rows=tables.includes(`table:${once}`)?state.storage.sql.exec(`SELECT id,substr(payload,1,65537) AS payload FROM ${once} ORDER BY id LIMIT 2`).toArray():null;
  const metadata=tables.includes(`table:${header}`)?state.storage.sql.exec(`SELECT id,substr(payload,1,65537) AS payload FROM ${header} ORDER BY id LIMIT 2`).toArray():null;
  return {tables,rows,header:metadata};
}
export function readExecutionFenceDispatch(state){return JSON.stringify(inventory(state));}
export function claimExecutionFenceDispatch(state,transition){
  return state.storage.transactionSync(()=>{
    const before=inventory(state);
    const next=transition(JSON.stringify(before));
    if(typeof next!=='string'||new TextEncoder().encode(next).byteLength>65536)throw new Error('execution fence claim rejected');
    if(before.rows!==null||before.header!==null||before.tables.some(t=>t!=='table:_cf_METADATA'))throw new Error('execution fence claim exists');
    state.storage.sql.exec(`CREATE TABLE ${header}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`);
    state.storage.sql.exec(`CREATE TABLE ${once}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`);
    state.storage.sql.exec(`INSERT INTO ${header}(id,payload) VALUES('singleton',?)`,next);
    state.storage.sql.exec(`INSERT INTO ${once}(id,payload) VALUES('singleton',?)`,next);
    return next;
  });
}
