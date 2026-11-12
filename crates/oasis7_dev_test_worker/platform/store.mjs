// One actor owns all grants/callers. Rust validates records and never treats a missing
// initialized ledger as zero. State and initialization marker share one SQL transaction.
function table(state) {
  state.storage.sql.exec("CREATE TABLE IF NOT EXISTS dev_test_records (id TEXT PRIMARY KEY, body TEXT NOT NULL)");
}
export function loadRecord(state, key) {
  table(state);
  const rows = state.storage.sql.exec("SELECT body FROM dev_test_records WHERE id = ?", key).toArray();
  if (rows.length > 1) throw new Error("record_conflict");
  if (key === "policy-meta" && rows.length === 0) {
    const anchor=state.storage.sql.exec("SELECT body FROM dev_test_records WHERE id = '__policy_initialized'").toArray();
    const any=state.storage.sql.exec("SELECT id FROM dev_test_records LIMIT 1").toArray();
    if (anchor.length || any.length) throw new Error("policy_record_missing_unknown");
  }
  return rows.length === 0 ? "" : rows[0].body;
}
export function commitRecord(state, key, previous, next) {
  if (next.length > (key.startsWith("native:") ? 6*1024*1024 : 4194304)) throw new Error("record_capacity");
  table(state);
  state.storage.transactionSync(() => {
    const rows = state.storage.sql.exec("SELECT body FROM dev_test_records WHERE id = ?", key).toArray();
    const current = rows.length === 0 ? "" : rows[0].body;
    if (current !== previous) throw new Error("state_changed_unknown");
    if (key === "policy-meta") {
      if (!current) {
        const any=state.storage.sql.exec("SELECT id FROM dev_test_records LIMIT 1").toArray();
        if (any.length) throw new Error("policy_record_missing_unknown");
      }
      state.storage.sql.exec("INSERT INTO dev_test_records(id,body) VALUES ('__policy_initialized','1') ON CONFLICT(id) DO NOTHING");
    }
    state.storage.sql.exec("INSERT INTO dev_test_records(id,body) VALUES (?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body", key, next);
  });
}

export function commitLedger(state, metaPrevious, metaNext, key, previous, next) {
  if (next.length > 4194304 || metaNext.length > 65536) throw new Error("record_capacity");
  table(state);
  state.storage.transactionSync(() => {
    const read = name => state.storage.sql.exec("SELECT body FROM dev_test_records WHERE id = ?", name).toArray();
    const meta = read("policy-meta"), row = read(key);
    if (meta.length !== 1 || read("__policy_initialized").length !== 1) throw new Error("policy_record_missing_unknown");
    if ((meta[0]?.body ?? "") !== metaPrevious || (row[0]?.body ?? "") !== previous) throw new Error("state_changed_unknown");
    for (const [id, body] of [["policy-meta", metaNext], [key, next]]) {
      state.storage.sql.exec("INSERT INTO dev_test_records(id,body) VALUES (?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body", id, body);
    }
  });
}
export async function readBoundedText(request, limit, deadline) {
  if (!request.body) return "";
  const reader = request.body.getReader(); const decoder = new TextDecoder("utf-8", {fatal:true});
  let total=0, result="", timer;
  try {
    const expired = new Promise((_,reject) => { timer=setTimeout(() => { reader.cancel().catch(()=>{}); reject(new Error("deadline_unknown")); },Math.max(0,deadline-Date.now())); });
    while (true) {
      const item = await Promise.race([reader.read(), expired]);
      if (Date.now() >= deadline) throw new Error("deadline_unknown");
      if (item.done) break;
      total += item.value.byteLength;
      if (total > limit || Date.now() >= deadline) throw new Error("body_bound_or_deadline");
      result += decoder.decode(item.value,{stream:true});
    }
    return result+decoder.decode();
  } finally { clearTimeout(timer); await reader.cancel().catch(()=>{}); }
}
export function randomHex() { return Array.from(crypto.getRandomValues(new Uint8Array(32)),x=>x.toString(16).padStart(2,"0")).join(""); }
