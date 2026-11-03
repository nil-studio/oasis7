// Separate bounded inventory for context ciphertext; never widen the SID adapter.
// Rust owns scope, immutable-envelope, physical actor and authority checks.
const recordLimit = 256 * 1024 + 1;

function inventory(state) {
  const table_names = state.storage.sql.exec(
    "SELECT type, substr(name, 1, 257) AS name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 8",
  ).toArray().map(row => `${row.type}:${row.name}`);
  const records = table_names.includes('table:custody_records')
    ? state.storage.sql.exec(
      'SELECT substr(id, 1, 257) AS id, substr(payload, 1, ?) AS payload FROM custody_records LIMIT 2',
      recordLimit,
    ).toArray()
    : [];
  return JSON.stringify({ records, table_names });
}

// A historical read never initializes a missing actor or changes audit/state.
export function readContextBodyInventory(state) {
  return state.storage.transactionSync(() => inventory(state));
}

// The synchronous Rust callback may reject; all initialization and audit then roll back.
export function contextBodyTransition(state, key, initial, transition) {
  return state.storage.transactionSync(() => {
    state.storage.sql.exec('CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)');
    state.storage.sql.exec('INSERT OR IGNORE INTO custody_records (id, payload) VALUES (?, ?)', key, initial);
    const old = state.storage.sql.exec('SELECT substr(payload, 1, ?) AS payload FROM custody_records WHERE id = ?', recordLimit, key).one().payload;
    const result = transition(old, inventory(state));
    if (!Array.isArray(result) || result.length !== 3 || result.some(value => typeof value !== 'string')) {
      throw new TypeError('invalid opaque context transition');
    }
    const [next, sequence, audit] = result;
    if (next !== old) {
      state.storage.sql.exec('CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(record_id, sequence))');
      state.storage.sql.exec('INSERT INTO custody_audit (record_id, sequence, payload) VALUES (?, ?, ?)', key, sequence, audit);
      state.storage.sql.exec('UPDATE custody_records SET payload = ? WHERE id = ?', next, key);
    }
    return next;
  });
}
