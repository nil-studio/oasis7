// Narrow synchronous SQLite glue. Payload strings stay opaque; Rust owns policy.
export function initializeTextRecord(state, key, initial) {
  return state.storage.transactionSync(() => {
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
    state.storage.sql.exec("INSERT OR IGNORE INTO custody_records (id, payload) VALUES (?, ?)", key, initial);
    return state.storage.sql.exec("SELECT payload FROM custody_records WHERE id = ?", key).one().payload;
  });
}

// The synchronous Rust callback may throw; this must roll back all SQL writes.
// It returns the exact next JSON bytes, never a JS object or numeric DTO.
export function updateTextRecord(state, key, transition) {
  return state.storage.transactionSync(() => {
    const old = state.storage.sql.exec("SELECT payload FROM custody_records WHERE id = ?", key).one().payload;
    const next = transition(old);
    if (typeof next !== "string") throw new TypeError("custody transition must return raw text");
    state.storage.sql.exec("UPDATE custody_records SET payload = ? WHERE id = ?", next, key);
    return next;
  });
}

// Owner component only: opaque JSON and decimal audit keys; callback owns all policy.
export function readTextRecord(state, key) {
  return state.storage.sql.exec("SELECT payload FROM custody_records WHERE id = ?", key).one().payload;
}
export function updateAuditedTextRecord(state, key, transition) {
  return state.storage.transactionSync(() => {
    const old = readTextRecord(state, key);
    const result = transition(old);
    if (!Array.isArray(result) || result.length !== 3 || result.some(v => typeof v !== "string"))
      throw new TypeError("invalid opaque custody transition");
    const [next, sequence, audit] = result;
    if (next !== old) {
      state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(record_id, sequence))");
      state.storage.sql.exec("INSERT INTO custody_audit (record_id, sequence, payload) VALUES (?, ?, ?)", key, sequence, audit);
      state.storage.sql.exec("UPDATE custody_records SET payload = ? WHERE id = ?", next, key);
    }
    return next;
  });
}

// No DTO parsing: Rust receives opaque rows and table names inside the same transaction.
function accountTransitionInner(state, key, initial, transition, ownerTerm) {
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
    state.storage.sql.exec("INSERT OR IGNORE INTO custody_records (id, payload) VALUES (?, ?)", key, initial);
    const old = readTextRecord(state, key);
    const records = state.storage.sql.exec("SELECT id, substr(payload, 1, 32769) AS payload FROM custody_records LIMIT 4").toArray();
    const table_names = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 16").toArray().map(row => `${row.type}:${row.name}`);
    const result = transition(old, JSON.stringify({ records, table_names }));
    if (!Array.isArray(result) || result.length !== (ownerTerm ? 4 : 3) || result.some(v => typeof v !== "string"))
      throw new TypeError("invalid opaque account transition");
    const [next, sequence, audit] = result;
    if (next !== old) {
      state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(record_id, sequence))");
      state.storage.sql.exec("INSERT INTO custody_audit (record_id, sequence, payload) VALUES (?, ?, ?)", key, sequence, audit);
      state.storage.sql.exec("UPDATE custody_records SET payload = ? WHERE id = ?", next, key);
    }
    if (ownerTerm) {
      if (key !== "custody-role-key-slot-v1") throw new TypeError("invalid Owner Term slot");
      const metadata = result[3];
      const names = new Set(table_names);
      const meta = names.has("table:custody_term_guarded_signer_state_v2");
      const once = names.has("table:custody_term_guarded_signer_once_v2");
      if (metadata !== "") {
        if (old !== initial || next === old || meta || once || metadata.length > 32768)
          throw new Error("Owner Term profile cannot initialize an existing or partial slot");
        state.storage.sql.exec("CREATE TABLE custody_term_guarded_signer_state_v2 (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
        state.storage.sql.exec("CREATE TABLE custody_term_guarded_signer_once_v2 (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
        state.storage.sql.exec("INSERT INTO custody_term_guarded_signer_state_v2 (id,payload) VALUES ('singleton',?)", metadata);
      } else if (!meta || !once) {
        throw new Error("Owner Term profile unavailable");
      }
    }
    return next;
}
export function accountTransition(state,key,initial,transition) {
  return state.storage.transactionSync(()=>accountTransitionInner(state,key,initial,transition,false));
}
// Only the actual new Owner slot transaction supplies the fourth opaque value.
// Existing/partial profiles are never repaired by a later key/sign request.
export function ownerTermRoleSlotTransitionV2(state,key,initial,transition) {
  return state.storage.transactionSync(()=>accountTransitionInner(state,key,initial,transition,true));
}
export async function readBoundedText(request, limit) {
  if (!request.body) return "";
  const reader = request.body.getReader();
  const parts = [];
  let total = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > limit) { await reader.cancel(); throw new RangeError("account request too large"); }
      parts.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) { bytes.set(part, offset); offset += part.byteLength; }
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}
