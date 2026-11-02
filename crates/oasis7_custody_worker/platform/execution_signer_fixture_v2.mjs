// Test-only fixed fault enum. Never imported by an ordinary artifact.
const KEY = "custody-role-key-slot-v1";
const faults = new WeakMap();
export function executionFixtureStateV2(original) {
  const count = {update_faults: 0, publication_faults: 0};
  const sql = new Proxy(original.storage.sql, {get(target, name) {
    const value = Reflect.get(target, name, target);
    if (name !== "exec") return typeof value === "function" ? value.bind(target) : value;
    return (...args) => {
      let updateCandidate = false, publicationCandidate = false;
      if (args[0] === "UPDATE custody_execution_signer_state_v2 SET payload = ? WHERE id = 1") {
        const candidate = JSON.parse(args[1]);
        const inserted = target.exec("SELECT COUNT(*) AS count FROM custody_execution_signer_once_v2").one().count;
        updateCandidate = candidate.api_version === 2 && candidate.count === "1" && inserted === 1;
      }
      if (args[0] === "INSERT INTO custody_execution_signer_originals_v2 (operation_id, payload) VALUES (?, ?)") {
        const candidate = JSON.parse(args[2]);
        const committed = target.exec("SELECT COUNT(*) AS count FROM custody_execution_signer_once_v2 WHERE operation_id = ?", args[1]).one().count;
        publicationCandidate = candidate.api_version === 2 && candidate.operation_digest === args[1]
          && candidate.original?.request?.original?.signature_hex?.length === 128 && committed === 1;
      }
      try { return value.apply(target, args); } catch (error) {
        if (updateCandidate) count.update_faults++;
        if (publicationCandidate) count.publication_faults++;
        throw error;
      }
    };
  }});
  const storage = new Proxy(original.storage, {get(target, name) {
    if (name === "sql") return sql;
    const value = Reflect.get(target, name, target);
    return typeof value === "function" ? value.bind(target) : value;
  }});
  const proxy = new Proxy(original, {get(target, name) {
    if (name === "storage") return storage;
    const value = Reflect.get(target, name, target);
    return typeof value === "function" ? value.bind(target) : value;
  }});
  faults.set(proxy, count);
  return proxy;
}
export function executionFixtureSeedV2(state, payload) {
  return state.storage.transactionSync(() => {
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)");
    const count = state.storage.sql.exec("SELECT COUNT(*) AS count FROM custody_records").one().count;
    if (count === 0) state.storage.sql.exec("INSERT INTO custody_records (id, payload) VALUES (?, ?)", KEY, payload);
    if (state.storage.sql.exec("SELECT COUNT(*) AS count FROM custody_records").one().count !== 1) throw new Error("fixture slot inventory");
  });
}
export function executionFixtureCountsV2(state) {
  const tables = new Set(state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table'").toArray().map(r => r.name));
  const count = table => tables.has(table) ? state.storage.sql.exec(`SELECT COUNT(*) AS count FROM ${table}`).one().count : 0;
  return JSON.stringify({initializations:count("custody_execution_signer_initializations_v2"),once:count("custody_execution_signer_once_v2"), originals:count("custody_execution_signer_originals_v2"), update_faults:faults.get(state).update_faults,publication_faults:faults.get(state).publication_faults});
}
export function executionFixtureFaultV2(state, kind) {
  return state.storage.transactionSync(() => {
    switch(kind) {
      case "claim_update_check":
        state.storage.sql.exec("ALTER TABLE custody_execution_signer_state_v2 RENAME TO fixture_old_meta");
        state.storage.sql.exec("CREATE TABLE custody_execution_signer_state_v2 (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL CHECK(json_extract(payload, '$.count') = '0'))");
        state.storage.sql.exec("INSERT INTO custody_execution_signer_state_v2 SELECT * FROM fixture_old_meta");
        state.storage.sql.exec("DROP TABLE fixture_old_meta");
        break;
      case "publication_check":
        state.storage.sql.exec("DROP TABLE custody_execution_signer_originals_v2");
        state.storage.sql.exec("CREATE TABLE custody_execution_signer_originals_v2 (operation_id TEXT PRIMARY KEY, payload TEXT NOT NULL CHECK(0))");
        break;
      case "remove_state": state.storage.sql.exec("DROP TABLE custody_execution_signer_state_v2"); break;
      case "corrupt_head": state.storage.sql.exec("UPDATE custody_execution_signer_state_v2 SET payload=json_set(payload, '$.head', ?)", "ff".repeat(32)); break;
      default: throw new Error("unknown fixed fixture fault");
    }
  });
}
