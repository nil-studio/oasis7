// Opaque enrollment persistence only. Rust owns identity, schema, bounds and policy.
// Absence and an existing empty/corrupt record remain distinct.
export function readEnrollment(state, key) {
  const tables = state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'custody_records'").toArray();
  if (tables.length === 0) return null;
  const rows = state.storage.sql.exec("SELECT substr(payload, 1, 524289) AS payload FROM custody_records WHERE id = ?", key).toArray();
  return rows.length === 0 ? null : rows[0].payload;
}

export function enrollmentTransition(state, key, transition) {
  return state.storage.transactionSync(() => {
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
    const old = readEnrollment(state, key);
    const records = state.storage.sql.exec("SELECT id, substr(payload, 1, 524289) AS payload FROM custody_records LIMIT 5").toArray();
    const table_names = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 16").toArray().map(row => `${row.type}:${row.name}`);
    const result = transition(old, JSON.stringify({ records, table_names }));
    if (!Array.isArray(result) || result.length !== 3 || result.some(value => typeof value !== 'string'))
      throw new TypeError('invalid opaque enrollment transition');
    const [next, sequence, audit] = result;
    if (next !== old) {
      state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(record_id, sequence))");
      state.storage.sql.exec("INSERT INTO custody_audit (record_id, sequence, payload) VALUES (?, ?, ?)", key, sequence, audit);
      if (old === null)
        state.storage.sql.exec("INSERT INTO custody_records (id, payload) VALUES (?, ?)", key, next);
      else
        state.storage.sql.exec("UPDATE custody_records SET payload = ? WHERE id = ?", next, key);
    }
    return next;
  });
}

// Dedicated abuse actor: one current-window row, no per-attempt audit growth.
// Rust still owns the fixed key, current window, limits and rollback rejection.
export function enrollmentAbuseTransition(state, key, transition) {
  return state.storage.transactionSync(() => {
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
    const old = readEnrollment(state, key);
    const records = state.storage.sql.exec("SELECT id, substr(payload, 1, 524289) AS payload FROM custody_records LIMIT 5").toArray();
    const table_names = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 16").toArray().map(row => `${row.type}:${row.name}`);
    const next = transition(old, JSON.stringify({records, table_names}));
    if (typeof next !== 'string') throw new TypeError('invalid opaque enrollment abuse transition');
    if (next !== old) {
      if (old === null)
        state.storage.sql.exec("INSERT INTO custody_records (id, payload) VALUES (?, ?)", key, next);
      else
        state.storage.sql.exec("UPDATE custody_records SET payload = ? WHERE id = ?", next, key);
    }
    return next;
  });
}
