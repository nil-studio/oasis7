// Closed Directory inventory: one original business row and two independently
// authenticated admission families. Only Rust callbacks project their full
// signed historical rows. No business payload is read during paid admission.
const BUSINESS = "manager-directory-v1";
const PROFILES = Object.freeze({
  execution: Object.freeze({birth: "manager-directory-pending-birth-v2", header: "custody_pending_current_floor_v2", once: "custody_pending_current_once_v2"}),
  read: Object.freeze({birth: "manager-directory-read-birth-v3", header: "custody_read_directory_header_v3", once: "custody_read_directory_once_v3"}),
});
const baseTables = ["custody_records", "custody_audit", "custody_management_metadata", "_cf_METADATA", "sqlite_sequence"];
const rowDDL = name => `CREATE TABLE ${name}(id TEXT PRIMARY KEY,payload TEXT NOT NULL)`;
const normalize = value => value.replace(/\s+/g, "").toLowerCase();
const deny = () => { throw new Error("Directory closed inventory rejected"); };
const bytes = value => new TextEncoder().encode(value).byteLength;
function capacity(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 4294967295) deny();
  return value;
}
function limit(value) {
  if (!Number.isSafeInteger(value) || value < 32768 || value >= Number.MAX_SAFE_INTEGER) deny();
  return value;
}
function rows(sql, table, cap, bound) {
  return sql.exec(`SELECT id,substr(payload,1,?) AS payload,length(CAST(payload AS BLOB)) AS bytes FROM ${table} ORDER BY id LIMIT ?`, bound + 1, cap + 1).toArray();
}
function scan(state, executionCapacity, readCapacity, readRowBound) {
  capacity(executionCapacity); capacity(readCapacity); limit(readRowBound);
  const sql = state.storage.sql;
  const schema = sql.exec("SELECT type,name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name LIMIT 16").toArray();
  const profileTables = Object.values(PROFILES).flatMap(p => [p.header, p.once]);
  if (schema.length >= 16 || schema.some(r => r.type !== "table" || ![...baseTables, ...profileTables].includes(r.name))) deny();
  const has = table => schema.some(r => r.name === table);
  for (const table of profileTables) {
    const existing = schema.find(r => r.name === table);
    if (existing && normalize(existing.sql) !== normalize(rowDDL(table))) deny();
  }
  // Exactly one business row + two compiled known births + overflow probe.
  // Sidecars are separately complete to their own capacity+1, never LIMIT 4.
  const records = has("custody_records") ? sql.exec("SELECT id FROM custody_records ORDER BY id LIMIT 4").toArray() : [];
  const known = [BUSINESS, ...Object.values(PROFILES).map(p => p.birth)];
  if (records.length > 3 || records.some(r => !known.includes(r.id))) deny();
  const result = {records, execution: null, read: null};
  for (const [name, p] of Object.entries(PROFILES)) {
    const cap = name === "execution" ? executionCapacity : readCapacity;
    const bound = name === "execution" ? 32768 : readRowBound;
    const birth = has("custody_records") ? sql.exec("SELECT id,substr(payload,1,?) AS payload,length(CAST(payload AS BLOB)) AS bytes FROM custody_records WHERE id=? LIMIT 2", bound + 1, p.birth).toArray() : [];
    // Zero is an internal "no configuration" sentinel, never an admissible
    // capacity. It is allowed only for a completely absent family so unchanged
    // ordinary deployments do not need new knobs merely to read old business.
    if (cap === 0 && (birth.length !== 0 || has(p.header) || has(p.once))) deny();
    const header = has(p.header) ? rows(sql, p.header, 1, bound) : null;
    const once = has(p.once) ? rows(sql, p.once, cap, bound) : null;
    const absent = birth.length === 0 && header === null && once === null;
    if (!absent && (birth.length !== 1 || header === null || header.length !== 1 || once === null || once.length === 0 || once.length > cap)) deny();
    if ([...birth, ...(header ?? []), ...(once ?? [])].some(r => typeof r.payload !== "string" || r.bytes > bound)) deny();
    const payloads = values => values.map(({id, payload}) => ({id, payload}));
    result[name] = absent ? null : {birth: payloads(birth), header: payloads(header), once: payloads(once)};
  }
  if ((result.execution !== null || result.read !== null) && !records.some(r => r.id === BUSINESS)) deny();
  return result;
}
// validate is a synchronous, private Rust closure. It must strongly validate
// BOTH families against external historical pins, scopes, capacities and every
// reverse/ordinal/count edge. Disabled still validates history. Its return value
// is discarded; it is not a JSON Current predicate.
export function inspectDirectoryAdmissionV3(state, executionCapacity, readCapacity, readRowBound, validate) {
  return state.storage.transactionSync(() => {
    const inventory = scan(state, executionCapacity, readCapacity, readRowBound);
    validate(JSON.stringify(inventory));
    return JSON.stringify(inventory);
  });
}
// This wrapper must surround every original ordinary read/write/initialization.
// The original operation runs in the same transaction after complete validation;
// it receives no filtered/repaired replacement business bytes.
export function ordinaryDirectoryAdmissionV3(state, executionCapacity, readCapacity, readRowBound, validate, operation) {
  return state.storage.transactionSync(() => {
    validate(JSON.stringify(scan(state, executionCapacity, readCapacity, readRowBound)));
    return operation();
  });
}
export function claimReadDirectoryV3(state, executionCapacity, readCapacity, readRowBound, transition) {
  return state.storage.transactionSync(() => {
    if (readCapacity === 0) deny();
    const inventory = scan(state, executionCapacity, readCapacity, readRowBound);
    if (!inventory.records.some(r => r.id === BUSINESS)) deny();
    // Complete strong validation of both families and fresh Read proof happens
    // before any mutation. This callback never receives ordinary row payloads.
    const next = transition(JSON.stringify(inventory));
    if (!Array.isArray(next) || next.length !== 5 || next.some(v => typeof v !== "string")) deny();
    const [birth, header, id, once, originalUntil] = next;
    if (!/^[0-9a-f]{64}$/.test(id) || [birth, header, once].some(v => bytes(v) > readRowBound) || !/^[1-9][0-9]*$/.test(originalUntil)) deny();
    const until = Number(originalUntil);
    const live = () => { if (!Number.isSafeInteger(until) || Date.now() >= until) deny(); };
    live();
    const sql = state.storage.sql, p = PROFILES.read;
    if (inventory.read === null) {
      sql.exec(rowDDL(p.header)); sql.exec(rowDDL(p.once));
      sql.exec("INSERT INTO custody_records(id,payload) VALUES(?,?)", p.birth, birth);
      sql.exec(`INSERT INTO ${p.header}(id,payload) VALUES('singleton',?)`, header);
    } else {
      if (inventory.read.birth[0].payload !== birth || inventory.read.header[0].id !== "singleton") deny();
      sql.exec(`UPDATE ${p.header} SET payload=? WHERE id='singleton'`, header);
    }
    sql.exec(`INSERT INTO ${p.once}(id,payload) VALUES(?,?)`, id, once);
    live();
    return once;
  });
}
