// Opaque same-actor metadata transaction. Rust owns projection, identity and quota.
export function managementParentsTransition(state, directoryKey, transition) {
  return state.storage.transactionSync(() => {
    // Missing business inventory is not initialized or treated as empty.
    const directory = state.storage.sql.exec("SELECT substr(payload, 1, 1048577) AS payload FROM custody_records WHERE id = ?", directoryKey).one().payload;
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_management_metadata (id INTEGER PRIMARY KEY CHECK(id = 1), payload TEXT NOT NULL)");
    const rows = state.storage.sql.exec("SELECT substr(payload, 1, 1025) AS payload FROM custody_management_metadata LIMIT 2").toArray();
    if (rows.length > 1) throw new TypeError('invalid metadata inventory');
    const tableNames = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 16").toArray().map(row => `${row.type}:${row.name}`);
    const result = transition(directory, rows.length ? rows[0].payload : null, JSON.stringify(tableNames));
    if (!Array.isArray(result) || result.length !== 2 || result.some(value => typeof value !== 'string'))
      throw new TypeError('invalid opaque metadata transition');
    const [quota, response] = result;
    state.storage.sql.exec("INSERT INTO custody_management_metadata (id, payload) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload", quota);
    return response;
  });
}
