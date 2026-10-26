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
