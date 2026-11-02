// Mechanical SQLite only. Rust verifies configured pins, exact typed grants,
// recipient/name/scope and every public original before entering these methods.
const MODE = "custody_execution_context_mode_v2";
const MARKER = "execution-context-stage-mode-v2";
const ONCE = "custody_execution_context_once_v2";
const EDGES = "custody_execution_context_edges_v2";
const GATES = "custody_execution_dispatch_receipts_v2";
const GATE_MARKER = "execution-context-per-leaf-mode-v2";
const MAX_ROWS = 512; // Account's 256 sessions x the two explicit Prepare profiles.
const MAX_EDGES = 4096;
const MAX_PUBLIC = 65536;
const MAX_SEALED = 262144;
// Only the internal JSON wrapper may expand while quoting the already bounded
// original payload. This does not change either public request/storage bound.
const MAX_CLAIM = 7 * MAX_PUBLIC;
const LEGACY = new Set(["table:custody_records", "table:custody_audit", "table:sqlite_sequence", "table:_cf_METADATA"]);
const OWN = new Set([`table:${MODE}`, `table:${ONCE}`, `table:${EDGES}`, `table:${GATES}`]);
function denied() { throw new Error("execution stage storage rejected"); }
function text(value, max) { if (typeof value !== "string" || value.length === 0 || new TextEncoder().encode(value).length > max) denied(); return value; }
function decimal(value) { if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,19})$/.test(value) || BigInt(value) > 18446744073709551615n) denied(); return BigInt(value); }
function tables(state) {
  const all = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 33").toArray().map(r => `${r.type}:${r.name}`);
  if (all.length > 32 || all.some(v => !LEGACY.has(v) && !OWN.has(v))) denied();
  const own = all.filter(v => OWN.has(v));
  if (own.length !== 0 && own.length !== 3 && own.length !== 4) denied();
  if (own.length !== 0 && ![MODE,ONCE,EDGES].every(v=>own.includes(`table:${v}`))) denied();
  return own.length >= 3;
}
function columns(state, table, expected) {
  const actual = state.storage.sql.exec(`PRAGMA table_info(${table})`).toArray().map(r => r.name);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) denied();
}
function read(state) {
  const initialized = tables(state);
  const hasAudit = state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name='custody_audit'").toArray().length === 1;
  const markers = hasAudit ? state.storage.sql.exec("SELECT sequence,payload FROM custody_audit WHERE record_id=? LIMIT 2",MARKER).toArray() : [];
  if (!initialized) {
    if (markers.length !== 0) denied();
    return { mode: null, records: [], edges: [] };
  }
  if (markers.length !== 1 || markers[0].sequence !== "1" || markers[0].payload !== "oasis7.custody.execution-context.mode.v2") denied();
  const gateTables=state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",GATES).toArray();
  const gateMarkers=state.storage.sql.exec("SELECT sequence,payload FROM custody_audit WHERE record_id=? LIMIT 2",GATE_MARKER).toArray();
  if (gateTables.length!==gateMarkers.length || gateTables.length>1) denied();
  let gateRows=[];
  if (gateTables.length===1) {
    if(gateMarkers[0].sequence!=="1" || gateMarkers[0].payload!=="oasis7.custody.execution-context.per-leaf.v2") denied();
    columns(state,GATES,["operation_id","edge","receipt"]);
    gateRows=state.storage.sql.exec(`SELECT operation_id,edge,length(CAST(receipt AS BLOB)) AS bytes FROM ${GATES} LIMIT 4097`).toArray();
    if(gateRows.length>4096) denied();
  }
  columns(state, MODE, ["id", "payload"]);
  columns(state, ONCE, ["operation_id", "profile", "proof_digest", "scope_digest", "payload", "next_edge", "sealed", "completed"]);
  columns(state, EDGES, ["attempt_id", "operation_id", "edge"]);
  const modes = state.storage.sql.exec(`SELECT id, substr(payload,1,4097) AS payload FROM ${MODE} LIMIT 2`).toArray();
  if (modes.length !== 1 || modes[0].id !== 1 || new TextEncoder().encode(modes[0].payload).length > 4096) denied();
  const mode = JSON.parse(modes[0].payload);
  if (mode.api_version !== 2 || mode.actor_id !== state.id.toString()) denied();
  decimal(mode.issuer_epoch); decimal(mode.config_generation);
  const records = state.storage.sql.exec(`SELECT operation_id,profile,proof_digest,scope_digest,next_edge,length(CAST(payload AS BLOB)) AS payload_bytes,length(CAST(sealed AS BLOB)) AS sealed_bytes,length(CAST(completed AS BLOB)) AS completed_bytes FROM ${ONCE} ORDER BY operation_id LIMIT 513`).toArray();
  const edges = state.storage.sql.exec(`SELECT attempt_id,operation_id,edge FROM ${EDGES} ORDER BY attempt_id LIMIT 4097`).toArray();
  if (records.length > MAX_ROWS || edges.length > MAX_EDGES) denied();
  const seen = new Map();
  for (const r of records) {
    text(r.operation_id,256); text(r.proof_digest,64); text(r.scope_digest,64);
    if (!Number.isInteger(r.payload_bytes) || r.payload_bytes <= 0 || r.payload_bytes > MAX_PUBLIC) denied();
    if (r.sealed_bytes !== null && (!Number.isInteger(r.sealed_bytes) || r.sealed_bytes <= 0 || r.sealed_bytes > MAX_SEALED)) denied();
    if (r.completed_bytes !== null && (!Number.isInteger(r.completed_bytes) || r.completed_bytes <= 0 || r.completed_bytes > 131072)) denied();
    r.payload="";r.sealed=r.sealed_bytes===null?null:"";r.completed=r.completed_bytes===null?null:"";
    delete r.payload_bytes;delete r.sealed_bytes;delete r.completed_bytes;
    if (!/^[a-f0-9]{64}$/.test(r.proof_digest) || !/^[a-f0-9]{64}$/.test(r.scope_digest)) denied();
    if (r.profile !== "private_current_v2" && r.profile !== "prepare_context_v2" && r.profile !== "prepare_context_gated_v2") denied();
    if (!Number.isInteger(r.next_edge) || r.next_edge < 2 || r.next_edge > (r.profile === "private_current_v2" ? 2 : 10)) denied();
    if (r.profile === "private_current_v2" && r.sealed !== null) denied();
    seen.set(r.operation_id,r);
  }
  for(const g of gateRows) {
    if(!records.some(r=>r.operation_id===g.operation_id && r.profile==="prepare_context_gated_v2")
        || !Number.isInteger(g.edge) || g.edge<1 || g.edge>9 || !Number.isInteger(g.bytes) || g.bytes<=0 || g.bytes>32768) denied();
  }
  const byOperation = new Map();
  for (const e of edges) {
    text(e.attempt_id,256);
    if (!seen.has(e.operation_id) || !Number.isInteger(e.edge) || e.edge < 0 || e.edge > 9) denied();
    const list = byOperation.get(e.operation_id) ?? [];
    if (list.includes(e.edge)) denied(); list.push(e.edge); byOperation.set(e.operation_id,list);
  }
  for (const r of records) {
    const list = byOperation.get(r.operation_id) ?? [];
    if (!list.includes(0) || !list.includes(1)) denied();
    if (r.profile === "private_current_v2" && (list.length !== 2 || list.some(v => v > 1))) denied();
    if ((r.profile === "prepare_context_v2" || r.profile === "prepare_context_gated_v2")) {
      if(r.profile==="prepare_context_gated_v2") {
        if(gateTables.length!==1) denied();
        const receipts=state.storage.sql.exec(`SELECT edge,length(CAST(receipt AS BLOB)) AS bytes FROM ${GATES} WHERE operation_id=? LIMIT 11`,r.operation_id).toArray();
        const expectedReceipts=r.next_edge<=7 ? [...Array(r.next_edge-1).keys()].map(i=>i+1) : r.next_edge===9 ? [1,2,3,4,5,6,7,8] : [1,2,3,4,5,6,7,8,9];
        if(receipts.length!==expectedReceipts.length || receipts.some(v=>v.bytes<=0||v.bytes>32768) || expectedReceipts.some(edge=>!receipts.some(v=>v.edge===edge))) denied();
      }
      const expected = r.next_edge <= 7 ? [...Array(r.next_edge).keys()] : r.next_edge === 9 ? [0,1,2,3,4,5,6] : r.next_edge === 10 ? [0,1,2,3,4,5,6,9] : null;
      if (expected === null || list.length !== expected.length || expected.some(v => !list.includes(v))) denied();
      if ((r.next_edge === 10) !== (r.sealed !== null)) denied();
      if (r.completed !== null && r.next_edge !== 10) denied();
    }
  }
  return { mode, records, edges };
}
export function readExecutionContextStagesV2(state) { return state.storage.transactionSync(() => JSON.stringify(read(state))); }
// Witnesses are read one at a time. Even 512 maximum-sized pending records
// cannot force a hundreds-of-MiB JSON/Wasm allocation during strict restore.
export function readExecutionContextStageRowV2(state, operation) {
  text(operation,256);
  return state.storage.transactionSync(() => {
    if (!tables(state)) denied();
    const rows=state.storage.sql.exec(`SELECT operation_id,profile,proof_digest,scope_digest,substr(payload,1,65537) AS payload,next_edge,substr(sealed,1,262145) AS sealed,substr(completed,1,131073) AS completed FROM ${ONCE} WHERE operation_id=? LIMIT 2`,operation).toArray();
    if (rows.length!==1) denied();
    const row=rows[0];text(row.payload,MAX_PUBLIC);
    if(row.sealed!==null) text(row.sealed,MAX_SEALED);
    if(row.completed!==null) text(row.completed,131072);
    return JSON.stringify(row);
  });
}

export function claimExecutionContextStageV2(state, raw) {
  text(raw,MAX_CLAIM); const q = JSON.parse(raw);
  return state.storage.transactionSync(() => {
    const before = read(state);
    if (before.records.length >= MAX_ROWS || before.edges.length + 2 > MAX_EDGES) denied();
    if (q.profile !== "private_current_v2" && q.profile !== "prepare_context_v2" && q.profile !== "prepare_context_gated_v2") denied();
    text(q.operation_id,256);text(q.proof_digest,64);text(q.scope_digest,64);text(q.payload,MAX_PUBLIC);text(q.actor_tag,256);
    if (!Array.isArray(q.first_ports) || q.first_ports.length !== 2 || q.first_ports[0] === q.first_ports[1]) denied();
    q.first_ports.forEach(v => text(v,256));
    const epoch = decimal(q.issuer_epoch), generation = decimal(q.config_generation);
    if (epoch === 0n || generation === 0n) denied();
    if (before.mode === null) {
      state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(record_id, sequence))");
      state.storage.sql.exec("INSERT INTO custody_audit(record_id,sequence,payload) VALUES(?, '1', 'oasis7.custody.execution-context.mode.v2')",MARKER);
      state.storage.sql.exec(`CREATE TABLE ${MODE} (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB))<=4096))`);
      state.storage.sql.exec(`CREATE TABLE ${ONCE} (operation_id TEXT PRIMARY KEY, profile TEXT NOT NULL, proof_digest TEXT NOT NULL, scope_digest TEXT NOT NULL, payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB))<=65536), next_edge INTEGER NOT NULL, sealed TEXT CHECK(sealed IS NULL OR length(CAST(sealed AS BLOB))<=262144), completed TEXT CHECK(completed IS NULL OR length(CAST(completed AS BLOB))<=131072))`);
      state.storage.sql.exec(`CREATE TABLE ${EDGES} (attempt_id TEXT PRIMARY KEY, operation_id TEXT NOT NULL, edge INTEGER NOT NULL, UNIQUE(operation_id,edge))`);
    } else if (before.mode.actor_tag !== q.actor_tag || before.mode.issuer_namespace !== q.issuer_namespace || before.mode.issuer_service !== q.issuer_service || before.mode.environment !== q.environment || epoch < decimal(before.mode.issuer_epoch) || generation < decimal(before.mode.config_generation)) denied();
    if (before.records.some(r => r.operation_id === q.operation_id) || before.edges.some(e => q.first_ports.includes(e.attempt_id))) denied();
    const mode = JSON.stringify({api_version:2,actor_id:state.id.toString(),actor_tag:q.actor_tag,issuer_namespace:q.issuer_namespace,issuer_service:q.issuer_service,environment:q.environment,issuer_epoch:q.issuer_epoch,config_generation:q.config_generation});
    state.storage.sql.exec(`INSERT OR REPLACE INTO ${MODE} (id,payload) VALUES (1,?)`,mode);
    state.storage.sql.exec(`INSERT INTO ${ONCE} (operation_id,profile,proof_digest,scope_digest,payload,next_edge,sealed,completed) VALUES (?,?,?,?,?,2,NULL,NULL)`,q.operation_id,q.profile,q.proof_digest,q.scope_digest,q.payload);
    for (let i=0;i<2;i++) state.storage.sql.exec(`INSERT INTO ${EDGES} (attempt_id,operation_id,edge) VALUES (?,?,?)`,q.first_ports[i],q.operation_id,i);
    if(q.profile==="prepare_context_gated_v2") {
      text(q.dispatch_receipt,32768);
      if(!before.records.some(r=>r.profile==="prepare_context_gated_v2") && state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",GATES).toArray().length===0) {
        state.storage.sql.exec(`CREATE TABLE ${GATES} (operation_id TEXT NOT NULL, edge INTEGER NOT NULL, receipt TEXT NOT NULL CHECK(length(CAST(receipt AS BLOB))<=32768), PRIMARY KEY(operation_id,edge))`);
        state.storage.sql.exec("INSERT INTO custody_audit(record_id,sequence,payload) VALUES(?, '1', 'oasis7.custody.execution-context.per-leaf.v2')",GATE_MARKER);
      }
      state.storage.sql.exec(`INSERT INTO ${GATES}(operation_id,edge,receipt) VALUES(?,1,?)`,q.operation_id,q.dispatch_receipt);
    }
    read(state); return q.operation_id;
  });
}
export function completeExecutionPrivateCurrentV2(state, operation, proof, result) {
  text(result,131072);
  return state.storage.transactionSync(() => {
    const snapshot=read(state);const row=snapshot.records.find(r=>r.operation_id===operation);
    if (!row || row.profile!=="private_current_v2" || row.proof_digest!==proof || row.completed!==null) denied();
    state.storage.sql.exec(`UPDATE ${ONCE} SET completed=? WHERE operation_id=?`,result,operation);
    read(state); return result;
  });
}
export function claimExecutionContextEdgeV2(state, operation, proof, edge, attempt, receipt) {
  text(attempt,256);
  return state.storage.transactionSync(() => {
    const snapshot=read(state); const row=snapshot.records.find(r=>r.operation_id===operation);
    if (!row || (row.profile!=="prepare_context_v2" && row.profile!=="prepare_context_gated_v2") || row.proof_digest!==proof || row.next_edge!==edge || edge<2 || edge>6 || snapshot.edges.some(e=>e.attempt_id===attempt)) denied();
    state.storage.sql.exec(`INSERT INTO ${EDGES} (attempt_id,operation_id,edge) VALUES (?,?,?)`,attempt,operation,edge);
    if(row.profile==="prepare_context_gated_v2") {
      text(receipt,32768);
      state.storage.sql.exec(`INSERT INTO ${GATES}(operation_id,edge,receipt) VALUES(?,?,?)`,operation,edge,receipt);
    }
    state.storage.sql.exec(`UPDATE ${ONCE} SET next_edge=? WHERE operation_id=?`,edge+1,operation);
    read(state);
  });
}
export function recordExecutionNativeCompletionV2(state, operation, proof, worldReceipts) {
  return state.storage.transactionSync(() => {
    const row=read(state).records.find(r=>r.operation_id===operation);
    if (!row || (row.profile!=="prepare_context_v2" && row.profile!=="prepare_context_gated_v2") || row.proof_digest!==proof || row.next_edge!==7) denied();
    if(row.profile==="prepare_context_gated_v2") {
      if(!Array.isArray(worldReceipts)||worldReceipts.length!==2) denied();
      for(let i=0;i<2;i++) { text(worldReceipts[i],32768);state.storage.sql.exec(`INSERT INTO ${GATES}(operation_id,edge,receipt) VALUES(?,?,?)`,operation,7+i,worldReceipts[i]); }
    }
    state.storage.sql.exec(`UPDATE ${ONCE} SET next_edge=9 WHERE operation_id=?`,operation); read(state);
  });
}
export function persistExecutionContextPutV2(state, operation, proof, attempt, sealed, receipt) {
  text(attempt,256);text(sealed,MAX_SEALED);
  return state.storage.transactionSync(() => {
    const snapshot=read(state);const row=snapshot.records.find(r=>r.operation_id===operation);
    if (!row || (row.profile!=="prepare_context_v2" && row.profile!=="prepare_context_gated_v2") || row.proof_digest!==proof || row.next_edge!==9 || row.sealed!==null || snapshot.edges.some(e=>e.attempt_id===attempt)) denied();
    state.storage.sql.exec(`INSERT INTO ${EDGES} (attempt_id,operation_id,edge) VALUES (?,?,9)`,attempt,operation);
    if(row.profile==="prepare_context_gated_v2") {text(receipt,32768);state.storage.sql.exec(`INSERT INTO ${GATES}(operation_id,edge,receipt) VALUES(?,9,?)`,operation,receipt);}
    state.storage.sql.exec(`UPDATE ${ONCE} SET sealed=?, next_edge=10 WHERE operation_id=?`,sealed,operation); read(state);
  });
}
export function completeExecutionContextPutV2(state, operation, proof, result) {
  text(result,131072);
  return state.storage.transactionSync(() => {
    const row=read(state).records.find(r=>r.operation_id===operation);
    if (!row || (row.profile!=="prepare_context_v2" && row.profile!=="prepare_context_gated_v2") || row.proof_digest!==proof || row.next_edge!==10 || row.completed!==null) denied();
    state.storage.sql.exec(`UPDATE ${ONCE} SET completed=? WHERE operation_id=?`,result,operation); read(state); return result;
  });
}

export function executionContextActorMatchesV2(state, env, name) {
  text(name,2048);
  return state.id.toString() === env.NATIVE_SESSIONS.idFromName(name).toString();
}

export function readExecutionDispatchReceiptsV2(state,operation) {
  text(operation,256);
  return state.storage.transactionSync(()=>{
    read(state);
    return JSON.stringify(state.storage.sql.exec(`SELECT edge,receipt FROM ${GATES} WHERE operation_id=? ORDER BY edge LIMIT 11`,operation).toArray());
  });
}
