// One account control record is published atomically under a short Web Lock.
// Never pass this record to rendering, logging, or cross-tab notifications.
export class BrowserModelStoreError extends Error {
  constructor(code) { super(code); this.name = "BrowserModelStoreError"; this.code = code; }
}
const fail = (code) => { throw new BrowserModelStoreError(code); };
const integer = (value) => Number.isSafeInteger(value) && value >= 0;
const empty = () => ({ schema: 1, revision: 0, key: null, provider: "deepseek", stopped: true,
  callsReserved: 0, tokensReserved: 0, reservations: [] });

export function createBrowserModelStore({ accountId, storage,
  locks = globalThis.navigator?.locks } = {}) {
  if (typeof accountId !== "string" || !accountId.trim()) fail("account_required");
  if (storage === undefined) {
    try { storage = globalThis.localStorage; } catch { fail("storage_unavailable"); }
  }
  const storageKey = `oasis7.browser-model.v1:${encodeURIComponent(accountId)}:deepseek`;
  const lockName = `${storageKey}:management`;
  const listeners = new Set();
  let blocked = false;
  function read() {
    let raw;
    try { raw = storage?.getItem(storageKey); } catch { fail("storage_unavailable"); }
    if (!storage || typeof storage.setItem !== "function") fail("storage_unavailable");
    if (raw == null) return empty();
    let record;
    try { record = JSON.parse(raw); } catch { fail("storage_corrupt"); }
    if (record?.schema !== 1) fail("storage_schema_unsupported");
    if (!integer(record.revision) || record.provider !== "deepseek"
      || !(record.key === null || (typeof record.key === "string" && record.key.length > 0))
      || typeof record.stopped !== "boolean" || !integer(record.callsReserved)
      || record.callsReserved > 4 || !integer(record.tokensReserved) || record.tokensReserved > 2048
      || !Array.isArray(record.reservations) || record.reservations.length > 4
      || record.reservations.some((r) => !integer(r.id) || !integer(r.revision)
        || !integer(r.tokens) || typeof r.settled !== "boolean")
      || new Set(record.reservations.map((r) => r.id)).size !== record.reservations.length
      || record.reservations.length !== record.callsReserved
      || record.reservations.reduce((sum, r) => sum + r.tokens, 0) !== record.tokensReserved
      || (!record.key && !record.stopped)) fail("storage_corrupt");
    return record;
  }
  const safe = (r) => ({ revision: r.revision, hasKey: Boolean(r.key), provider: r.provider,
    status: !r.key ? "unconfigured" : (r.stopped || blocked ? "stopped" : "saved"),
    callsReserved: r.callsReserved, tokensReserved: r.tokensReserved, stopped: r.stopped || blocked });
  const emit = () => { for (const listener of listeners) { try { listener(); } catch { /* observer only */ } } };
  function check(r, revision) {
    if (!integer(revision) || r.revision !== revision) fail("revision_conflict");
  }
  async function locked(action) {
    if (typeof locks?.request !== "function") fail("locks_unavailable");
    try { return await locks.request(lockName, { mode: "exclusive" }, () => action(read())); }
    catch (error) { if (error instanceof BrowserModelStoreError) throw error; fail("locks_unavailable"); }
  }
  function publish(r) {
    if (r.revision >= Number.MAX_SAFE_INTEGER) fail("revision_exhausted");
    try { storage.setItem(storageKey, JSON.stringify(r)); }
    catch { blocked = true; emit(); fail("storage_write_failed"); }
    blocked = false;
    emit();
    return safe(r);
  }
  async function invalidate(expectedRevision, forget) {
    // Stop this page even when persistence or lock access fails.
    blocked = true;
    emit();
    return locked((r) => {
      if (expectedRevision !== undefined) check(r, expectedRevision);
      r.revision += 1; r.stopped = true;
      if (forget) r.key = null;
      return publish(r);
    });
  }
  return {
    storageKey,
    snapshot: () => safe(read()),
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
    refresh() { const result = safe(read()); emit(); return result; },
    save(key, expectedRevision) {
      if (typeof key !== "string" || !key.trim() || key.length > 8192 || /[\r\n]/.test(key)) {
        return Promise.reject(new BrowserModelStoreError("invalid_key"));
      }
      return locked((r) => {
        check(r, expectedRevision);
        r.key = key.trim(); r.stopped = false; r.revision += 1;
        return publish(r);
      });
    },
    stop: (revision) => invalidate(revision, false),
    forget: (revision) => invalidate(revision, true),
    clear: () => invalidate(undefined, true),
    beginProbe(expectedRevision) {
      return locked((r) => {
        check(r, expectedRevision);
        if (!r.key) fail("credential_missing");
        r.revision += 1; r.stopped = false;
        return publish(r);
      });
    },
    reserve({ revision, maxInputTokens, maxOutputTokens }) {
      return locked((r) => {
        check(r, revision);
        if (blocked || r.stopped) fail("stopped");
        if (!r.key) fail("credential_missing");
        const tokens = maxInputTokens + maxOutputTokens;
        if (!integer(maxInputTokens) || !integer(maxOutputTokens) || !integer(tokens) || tokens === 0) fail("invalid_budget");
        if (r.callsReserved >= 4 || r.tokensReserved + tokens > 2048) fail("budget_exhausted");
        const reservation = { id: r.callsReserved + 1, revision, tokens, settled: false };
        r.callsReserved += 1; r.tokensReserved += tokens; r.reservations.push(reservation);
        publish(r);
        return { id: reservation.id, revision };
      });
    },
    settle(reservation, usage) {
      return locked((r) => {
        const found = r.reservations.find((item) => item.id === reservation?.id && item.revision === reservation?.revision);
        if (!found || found.settled) fail("reservation_invalid");
        if (!usage || !integer(usage.inputTokens) || !integer(usage.outputTokens)
          || !integer(usage.inputTokens + usage.outputTokens)
          || usage.inputTokens + usage.outputTokens > found.tokens) return safe(r);
        const used = usage.inputTokens + usage.outputTokens;
        r.tokensReserved -= found.tokens - used; found.tokens = used; found.settled = true;
        return publish(r);
      });
    },
    credential(revision) {
      return locked((r) => {
        check(r, revision);
        if (blocked || r.stopped) fail("stopped");
        if (!r.key) fail("credential_missing");
        return r.key;
      });
    },
  };
}
