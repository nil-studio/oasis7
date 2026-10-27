// Independent authority rows only. Rust owns all decoded state and policy.
function inventory(state) {
  const records=state.storage.sql.exec('SELECT id, substr(payload,1,524289) AS payload FROM custody_records ORDER BY id LIMIT 3').toArray();
  const table_names=state.storage.sql.exec("SELECT type,name FROM sqlite_master WHERE type IN ('table','trigger','view') ORDER BY type,name LIMIT 16").toArray().map(r=>`${r.type}:${r.name}`);
  const hasAudit=table_names.includes('table:custody_audit');
  const audit=hasAudit?state.storage.sql.exec('SELECT CAST(COUNT(*) AS TEXT) AS count FROM custody_audit').one().count:'0';
  const audit_rows=hasAudit?state.storage.sql.exec('SELECT record_id,sequence,substr(payload,1,2049) AS payload FROM custody_audit ORDER BY record_id,sequence LIMIT 130').toArray():[];
  return JSON.stringify({records,table_names,audit_count:audit,audit_rows});
}
export function readFenceInventory(state) { return inventory(state); }
export function fenceTransition(state, transition) {
  return state.storage.transactionSync(()=>{
    const result=transition(inventory(state));
    if(!Array.isArray(result)||result.length!==4||result.some(v=>typeof v!=='string')) throw new TypeError('invalid opaque fence transition');
    const [ledger,fence,sequence,audit]=result;
    if(ledger!=='') {
      state.storage.sql.exec('CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL,sequence TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(record_id,sequence))');
      state.storage.sql.exec("INSERT INTO custody_audit(record_id,sequence,payload) VALUES('recovery-fence-ledger-v1',?,?)",sequence,audit);
      state.storage.sql.exec("INSERT INTO custody_records(id,payload) VALUES('recovery-fence-ledger-v1',?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",ledger);
      state.storage.sql.exec("UPDATE custody_records SET payload=? WHERE id='recovery-fence-v1'",fence);
    }
    return inventory(state);
  });
}
