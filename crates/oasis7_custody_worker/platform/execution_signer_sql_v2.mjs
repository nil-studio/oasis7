// Separate bounded V2 signer inventory. The legacy slot row stays immutable.
const META = "custody_execution_signer_state_v2";
const ONCE = "custody_execution_signer_once_v2";
const ORIGINALS = "custody_execution_signer_originals_v2";
const INITIALIZATIONS = "custody_execution_signer_initializations_v2";
const MAX_ROWS = 1024;
const MAX_BYTES = 4096;
function inventory(state) {
  const objects = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table','trigger','view') LIMIT 16").toArray();
  const allowed = new Set(["table:custody_records", "table:custody_audit", "table:sqlite_sequence", "table:_cf_METADATA", `table:${META}`, `table:${ONCE}`, `table:${ORIGINALS}`, `table:${INITIALIZATIONS}`]);
  if (objects.length > 8 || objects.some(o => !allowed.has(`${o.type}:${o.name}`))) throw new Error("execution signer inventory rejected");
}
function slot(state, key, expected) {
  inventory(state);
  if (state.storage.sql.exec("SELECT COUNT(*) AS count FROM custody_records").one().count !== 1) throw new Error("execution signer slot inventory");
  const actual = state.storage.sql.exec("SELECT payload FROM custody_records WHERE id = ?", key).one().payload;
  if (actual !== expected) throw new Error("execution signer slot changed");
}
function tables(state) {
  return state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN (?, ?, ?, ?)", META, ONCE, ORIGINALS, INITIALIZATIONS).toArray();
}
function rows(state) {
  const header = state.storage.sql.exec("SELECT id FROM custody_execution_signer_state_v2 LIMIT 2").toArray();
  if (header.length !== 1 || header[0].id !== 1) throw new Error("execution signer header inventory");
  for (const [table, first] of [[META, "id"], [ONCE, "operation_id"], [ORIGINALS, "operation_id"], [INITIALIZATIONS, "operation_id"]]) {
    const columns = state.storage.sql.exec(`PRAGMA table_info(${table})`).toArray();
    if (columns.length !== 2 || columns[0].name !== first || columns[0].pk !== 1 || columns[0].type !== (table === META ? "INTEGER" : "TEXT")
        || columns[1].name !== "payload" || columns[1].type !== "TEXT" || columns[1].notnull !== 1)
      throw new Error("execution signer columns rejected");
  }
  const publications = state.storage.sql.exec("SELECT COUNT(*) AS count FROM custody_execution_signer_originals_v2").one().count;
  const claims = state.storage.sql.exec("SELECT COUNT(*) AS count FROM custody_execution_signer_once_v2").one().count;
  const orphan = state.storage.sql.exec("SELECT operation_id FROM custody_execution_signer_originals_v2 WHERE operation_id NOT IN (SELECT operation_id FROM custody_execution_signer_once_v2) LIMIT 1").toArray();
  if (claims > MAX_ROWS || publications > claims || orphan.length !== 0) throw new Error("execution signer reverse inventory rejected");
  const initializations = state.storage.sql.exec("SELECT operation_id, substr(payload, 1, 4097) AS payload FROM custody_execution_signer_initializations_v2 ORDER BY operation_id LIMIT 1025").toArray();
  if (initializations.length > MAX_ROWS) throw new RangeError("execution initialization inventory full");
  return {sign: state.storage.sql.exec("SELECT operation_id, substr(payload, 1, 4097) AS payload FROM custody_execution_signer_once_v2 ORDER BY operation_id LIMIT 1025").toArray(), initializations};
}
function text(value) {
  if (typeof value !== "string" || new TextEncoder().encode(value).byteLength > MAX_BYTES)
    throw new TypeError("invalid execution signer record");
}
// Explicit bootstrap only. The signing/claim path never calls this function.
// Every initialization consumes its own permanent signed operation. Exact replay rejects.
export function initializeExecutionSignerV2(state, slotKey, expectedSlot, initial, transition) {
  text(initial);
  return state.storage.transactionSync(() => {
    slot(state, slotKey, expectedSlot);
    const present = tables(state).length;
    if (present !== 0 && present !== 4) throw new Error("partial execution signer state");
    if (present === 0) {
      state.storage.sql.exec("CREATE TABLE custody_execution_signer_state_v2 (id INTEGER PRIMARY KEY CHECK (id = 1), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 4096))");
      state.storage.sql.exec("CREATE TABLE custody_execution_signer_once_v2 (operation_id TEXT PRIMARY KEY, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 4096))");
      state.storage.sql.exec("CREATE TABLE custody_execution_signer_originals_v2 (operation_id TEXT PRIMARY KEY, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536))");
      state.storage.sql.exec("CREATE TABLE custody_execution_signer_initializations_v2 (operation_id TEXT PRIMARY KEY, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 4096))");
      state.storage.sql.exec("INSERT INTO custody_execution_signer_state_v2 (id, payload) VALUES (1, ?)", initial);
    }
    const old = state.storage.sql.exec("SELECT payload FROM custody_execution_signer_state_v2 WHERE id = 1").one().payload;
    const result = transition(old, JSON.stringify(rows(state)));
    if (!Array.isArray(result) || result.length !== 3 || result.some(v => typeof v !== "string")) throw new TypeError("invalid initialization transition");
    const [next, operation, claim] = result;
    text(next); text(claim);
    if (!/^[0-9a-f]{64}$/.test(operation)) throw new TypeError("invalid initialization operation");
    state.storage.sql.exec("INSERT INTO custody_execution_signer_initializations_v2 (operation_id, payload) VALUES (?, ?)", operation, claim);
    state.storage.sql.exec("UPDATE custody_execution_signer_state_v2 SET payload = ? WHERE id = 1", next);
    return next;
  });
}
// The callback checks exact scope, chain, index count, all IDs and cold records.
// Its output is the new metadata + one immutable claim. Failure rolls back both.
// No CREATE/INSERT OR IGNORE: missing new-mode state is a hard failure.
export function claimExecutionSignerV2(state, slotKey, expectedSlot, transition) {
  return state.storage.transactionSync(() => {
    slot(state, slotKey, expectedSlot);
    if (tables(state).length !== 4) throw new Error("execution signer state unavailable");
    const old = state.storage.sql.exec("SELECT payload FROM custody_execution_signer_state_v2 WHERE id = 1").one().payload;
    const existing = rows(state);
    if (existing.sign.length >= MAX_ROWS) throw new RangeError("execution signer inventory full");
    const result = transition(old, JSON.stringify(existing));
    if (!Array.isArray(result) || result.length !== 3 || result.some(v => typeof v !== "string"))
      throw new TypeError("invalid execution signer transition");
    const [next, operation, claim] = result;
    text(next); text(claim);
    if (!/^[0-9a-f]{64}$/.test(operation)) throw new TypeError("invalid execution signer operation");
    state.storage.sql.exec("INSERT INTO custody_execution_signer_once_v2 (operation_id, payload) VALUES (?, ?)", operation, claim);
    state.storage.sql.exec("UPDATE custody_execution_signer_state_v2 SET payload = ? WHERE id = 1", next);
    return next;
  });
}

// Stores only a fixed, public signed original, separately from the once claim.
// It never changes or removes the claim and cannot open a key on a later read.
export function publishExecutionOriginalV2(state, slotKey, expectedSlot, operation, record, validate) {
  if (typeof record !== "string" || new TextEncoder().encode(record).byteLength > 65536)
    throw new RangeError("execution original too large");
  return state.storage.transactionSync(() => {
    slot(state, slotKey, expectedSlot);
    if (tables(state).length !== 4) throw new Error("execution signer state unavailable");
    const old = state.storage.sql.exec("SELECT payload FROM custody_execution_signer_state_v2 WHERE id = 1").one().payload;
    validate(old, JSON.stringify(rows(state)));
    if (state.storage.sql.exec("SELECT COUNT(*) AS count FROM custody_execution_signer_once_v2 WHERE operation_id = ?", operation).one().count !== 1)
      throw new Error("execution claim unavailable");
    state.storage.sql.exec("INSERT INTO custody_execution_signer_originals_v2 (operation_id, payload) VALUES (?, ?)", operation, record);
    return record;
  });
}
export function readExecutionOriginalV2(state, slotKey, expectedSlot, operation, validate) {
  return state.storage.transactionSync(() => {
    slot(state, slotKey, expectedSlot);
    if (tables(state).length !== 4) throw new Error("execution signer state unavailable");
    const old = state.storage.sql.exec("SELECT payload FROM custody_execution_signer_state_v2 WHERE id = 1").one().payload;
    validate(old, JSON.stringify(rows(state)));
    const found = state.storage.sql.exec("SELECT substr(payload, 1, 65537) AS payload FROM custody_execution_signer_originals_v2 WHERE operation_id = ?", operation).toArray();
    if (found.length === 0) return null;
    return found[0].payload;
  });
}
