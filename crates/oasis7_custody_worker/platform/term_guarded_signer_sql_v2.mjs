// Fixed private Owner Actor storage only. No network, authority or key bytes.
const slotKey='custody-role-key-slot-v1';
const stateTable='custody_term_guarded_signer_state_v2';
const onceTable='custody_term_guarded_signer_once_v2';
function rows(state){
  const role=state.storage.sql.exec("SELECT id,substr(payload,1,32769) AS payload FROM custody_records LIMIT 4").toArray();
  const tables=state.storage.sql.exec("SELECT type,name FROM sqlite_master WHERE type IN ('table','trigger','view') LIMIT 16").toArray().map(v=>`${v.type}:${v.name}`);
  const present=n=>tables.includes(`table:${n}`);
  return {role,tables,metadata:present(stateTable)?state.storage.sql.exec(`SELECT id,substr(payload,1,32769) AS payload FROM ${stateTable} LIMIT 2`).toArray():null,
    claims:present(onceTable)?state.storage.sql.exec(`SELECT id,substr(payload,1,32769) AS payload FROM ${onceTable} LIMIT 257`).toArray():null};
}
export function readTermGuardedSignerState(state){return JSON.stringify(rows(state));}
export function claimTermGuardedSigner(state,transition){
  return state.storage.transactionSync(()=>{
    const inventory=rows(state);
    // Missing/partial tables must fail before callback/INSERT, never initialize.
    if(inventory.metadata===null||inventory.claims===null)throw new Error('Term signer state unavailable');
    const next=transition(JSON.stringify(inventory));
    if(typeof next!=='string'||next.length>32768)throw new TypeError('invalid Term claim');
    const row=JSON.parse(next);
    if(typeof row.id!=='string'||!/^[0-9a-f]{64}$/.test(row.id))throw new TypeError('invalid Term claim id');
    state.storage.sql.exec(`INSERT INTO ${onceTable} (id,payload) VALUES (?,?)`,row.id,next);
    const slot=inventory.role.find(v=>v.id===slotKey);
    if(!slot)throw new Error('Term signer slot unavailable');
    return slot.payload;
  });
}
export function recordTermGuardedSignerOriginal(state,id,transition){
  return state.storage.transactionSync(()=>{
    const inventory=rows(state);
    if(inventory.metadata===null||inventory.claims===null)throw new Error('Term signer state unavailable');
    const next=transition(JSON.stringify(inventory));
    if(typeof next!=='string'||next.length>32768)throw new TypeError('invalid Term original');
    const row=JSON.parse(next);
    if(row.id!==id||!/^[0-9a-f]{64}$/.test(id))throw new TypeError('invalid Term original id');
    const old=state.storage.sql.exec(`SELECT payload FROM ${onceTable} WHERE id=?`,id).toArray();
    if(old.length!==1)throw new Error('Term original claim unavailable');
    state.storage.sql.exec(`UPDATE ${onceTable} SET payload=? WHERE id=?`,next,id);
  });
}
