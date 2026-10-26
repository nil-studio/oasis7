// Static capability only. Rust owns all policy, parsing and authority checks.
export async function ownerNativeCall(env, raw) {
  return env.OWNER_NATIVE_PERMITS.call(raw);
}

// Opaque, fixed-key historical SQL read. Never creates tables, records or audit rows.
export function readNativeManifest(state) {
  return state.storage.transactionSync(() => {
    const table_names = state.storage.sql.exec("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'trigger', 'view') LIMIT 16").toArray().map(row => `${row.type}:${row.name}`);
    const records = table_names.includes("table:custody_records")
      ? state.storage.sql.exec("SELECT id, substr(payload, 1, 32769) AS payload FROM custody_records LIMIT 4").toArray()
      : [];
    return JSON.stringify({ records, table_names });
  });
}

// Cipher record actors are opaque read/CAS stores, never authorization issuers.
export const readNativeCipher = readNativeManifest;

export async function ownerNativeRecover(env, raw) {
  return env.OWNER_NATIVE_PERMITS.recover(raw);
}
