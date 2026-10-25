import { browserIdentityScope, createBrowserGameDatabase, gameStorageFail } from "./browser_game_identity.js";

// Sorting is for local immutable-record comparison only, never wire signatures.
function stable(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isSafeInteger(value)) return value;
  if (Array.isArray(value)) return value.map(stable);
  if (value && Object.getPrototypeOf(value) === Object.prototype) {
    return Object.fromEntries(Object.keys(value).sort().map((key) => {
      if (/^(api_?key|model_?key|authorization|private_?key|credential)$/i.test(key)) gameStorageFail("recovery_secret_forbidden");
      return [key, stable(value[key])];
    }));
  }
  gameStorageFail("recovery_record_invalid");
}
const encode = (value) => JSON.stringify(stable(value));
const copy = (value) => JSON.parse(JSON.stringify(value));
const states = new Set(["received", "pending", "unknown", "history_unavailable", "committed", "rejected", "expired"]);
const terminal = (state) => ["committed", "rejected", "expired"].includes(state);
const integer = (value) => Number.isSafeInteger(value) && value >= 0;
export const BROWSER_GAME_RUN_LIMITS = Object.freeze({ maxCalls: 8, maxTokens: 131072,
  maxOutputTokens: 512, maxDurationMs: 120000 });
function operationId(value) {
  if (typeof value !== "string" || !value.trim() || value.length > 256) gameStorageFail("recovery_operation_invalid");
  return value;
}

export function createBrowserGameRecovery({ scope, indexedDB, storage,
  locks = globalThis.navigator?.locks, now = Date.now } = {}) {
  const identity = browserIdentityScope(scope);
  if (typeof scope.taskId !== "string" || !scope.taskId.trim() || scope.taskId.length > 512
    || !scope.fence || typeof scope.fence !== "object" || Array.isArray(scope.fence)) {
    gameStorageFail("recovery_scope_invalid");
  }
  const scopeKey = encode({ identity, taskId: scope.taskId });
  const capturedScope = copy(stable(scope));
  const fenceKey = encode(capturedScope);
  const key = `recovery:${scopeKey}`;
  const database = storage ?? createBrowserGameDatabase(indexedDB);
  const initial = () => ({ schema: 1, scopeKey, operations: [], runBudgets: [] });
  function validate(record) {
    if (record === undefined) return initial();
    if (record?.schema !== 1 || record.scopeKey !== scopeKey || !Array.isArray(record.operations)
      || record.operations.length > 256) gameStorageFail("recovery_corrupt");
    const ids = new Set();
    for (const entry of record.operations) {
      validateOriginal(entry.original);
      if (browserIdentityScope(entry.scope) !== identity || entry.scope.taskId !== scope.taskId
        || !entry.scope.fence) gameStorageFail("recovery_corrupt");
      if (!states.has(entry.state) || ids.has(entry.original.operationId)) gameStorageFail("recovery_corrupt");
      ids.add(entry.original.operationId);
      if (entry.state === "committed" && !entry.receipt) gameStorageFail("recovery_corrupt");
    }
    // The additive budget field preserves existing originals. An old envelope
    // with no budgets has never authorized a model call through this path.
    if (record.runBudgets === undefined) record.runBudgets = [];
    if (!Array.isArray(record.runBudgets) || record.runBudgets.length > 256) gameStorageFail("run_budget_corrupt");
    const runs = new Set(); const fences = new Set();
    for (const budget of record.runBudgets) {
      operationId(budget.runId);
      if (runs.has(budget.runId) || typeof budget.fenceKey !== "string" || fences.has(budget.fenceKey)
        || !integer(budget.startedAt) || !integer(budget.deadline)
        || budget.deadline - budget.startedAt !== BROWSER_GAME_RUN_LIMITS.maxDurationMs
        || !integer(budget.callsReserved) || budget.callsReserved > BROWSER_GAME_RUN_LIMITS.maxCalls
        || !integer(budget.tokensReserved) || budget.tokensReserved > BROWSER_GAME_RUN_LIMITS.maxTokens
        || !Array.isArray(budget.reservations) || budget.reservations.length !== budget.callsReserved
        || budget.reservations.some((r, index) => r.id !== index + 1 || !integer(r.inputBytes)
          || !integer(r.maxOutputTokens) || r.maxOutputTokens > BROWSER_GAME_RUN_LIMITS.maxOutputTokens
          || !integer(r.tokens) || r.tokens > r.inputBytes + r.maxOutputTokens
          || typeof r.settled !== "boolean")
        || budget.reservations.reduce((sum, r) => sum + r.tokens, 0) !== budget.tokensReserved) {
        gameStorageFail("run_budget_corrupt");
      }
      runs.add(budget.runId); fences.add(budget.fenceKey);
    }
    // Detect non-JSON values and forbidden credential fields on reads too.
    encode(record);
    return record;
  }
  function validateOriginal(original) {
    operationId(original?.operationId);
    if (typeof original.kind !== "string" || !original.kind.trim() || original.kind.length > 128
      || !original.signedRequest || typeof original.signedRequest !== "object"
      || !original.correlation || typeof original.correlation !== "object"
      || !Object.hasOwn(original, "context") || !original.runBudget || typeof original.runBudget !== "object") {
      gameStorageFail("recovery_record_invalid");
    }
    const serialized = encode(original);
    if (serialized.length > 1048576) gameStorageFail("recovery_record_too_large");
  }
  async function mutate(action) {
    if (typeof locks?.request !== "function") gameStorageFail("recovery_locks_unavailable");
    try {
      return await locks.request(`oasis7:${key}`, { mode: "exclusive" },
        () => database.update(key, (stored) => action(validate(stored))));
    } catch (error) {
      if (typeof error?.code === "string") throw error;
      gameStorageFail("recovery_storage_failed");
    }
  }
  function run(record, runId, requireFence = true) {
    operationId(runId);
    const budget = record.runBudgets.find((item) => item.runId === runId);
    if (!budget) gameStorageFail("run_budget_missing");
    if (requireFence && budget.fenceKey !== fenceKey) gameStorageFail("run_fence_conflict");
    return budget;
  }
  const api = {
    async load() { return copy(validate(await database.get(key))); },
    async initializeRun(runId, { fence } = {}) {
      operationId(runId);
      if (encode(fence) !== encode(capturedScope.fence)) gameStorageFail("run_fence_conflict");
      const record = await mutate((stored) => {
        const existing = stored.runBudgets.find((item) => item.runId === runId);
        if (existing) { run(stored, runId); return stored; }
        if (stored.runBudgets.some((item) => item.fenceKey === fenceKey)) gameStorageFail("run_identity_conflict");
        if (stored.runBudgets.length >= 256) gameStorageFail("run_budget_capacity_exhausted");
        const startedAt = now();
        if (!integer(startedAt) || !integer(startedAt + BROWSER_GAME_RUN_LIMITS.maxDurationMs)) gameStorageFail("run_clock_invalid");
        stored.runBudgets.push({ runId, fenceKey, startedAt,
          deadline: startedAt + BROWSER_GAME_RUN_LIMITS.maxDurationMs,
          callsReserved: 0, tokensReserved: 0, reservations: [] });
        return stored;
      });
      return copy(run(record, runId));
    },
    async getRunBudget(runId) { return copy(run(validate(await database.get(key)), runId)); },
    async reserveModelCall(runId, inputBytes, maxOutputTokens = 512) {
      if (!integer(inputBytes) || inputBytes === 0 || !integer(maxOutputTokens) || maxOutputTokens === 0
        || maxOutputTokens > BROWSER_GAME_RUN_LIMITS.maxOutputTokens
        || !integer(inputBytes + maxOutputTokens)) gameStorageFail("run_reservation_invalid");
      let reservation;
      await mutate((stored) => {
        const budget = run(stored, runId);
        const time = now();
        if (!integer(time) || time < budget.startedAt || time >= budget.deadline) gameStorageFail("run_deadline_exceeded");
        const tokens = inputBytes + maxOutputTokens;
        if (budget.callsReserved >= BROWSER_GAME_RUN_LIMITS.maxCalls
          || budget.tokensReserved + tokens > BROWSER_GAME_RUN_LIMITS.maxTokens) gameStorageFail("run_budget_exhausted");
        reservation = { id: budget.callsReserved + 1, inputBytes, maxOutputTokens, tokens, settled: false };
        budget.reservations.push(reservation); budget.callsReserved += 1; budget.tokensReserved += tokens;
        return stored;
      });
      return { ...copy(reservation), runId };
    },
    async settleModelCall(runId, reservation, usage) {
      const record = await mutate((stored) => {
        // Late usage may settle an old fence, but cannot authorize another send.
        const budget = run(stored, runId, false);
        const found = budget.reservations.find((item) => item.id === reservation?.id);
        if (!found || reservation.runId !== runId || found.settled) gameStorageFail("run_reservation_invalid");
        if (!usage || !integer(usage.inputTokens) || !integer(usage.outputTokens)
          || !integer(usage.inputTokens + usage.outputTokens)
          || usage.inputTokens + usage.outputTokens > found.tokens) return stored;
        const used = usage.inputTokens + usage.outputTokens;
        budget.tokensReserved -= found.tokens - used; found.tokens = used; found.settled = true;
        return stored;
      });
      return copy(run(record, runId, false));
    },
    async remember(original) {
      validateOriginal(original);
      // Snapshot caller-owned objects before awaiting; later edits cannot mutate
      // the persisted nonce, signed payload, correlation, Context or budget.
      const captured = copy(original);
      const record = await mutate((stored) => {
        const previous = stored.operations.find((entry) => entry.original.operationId === captured.operationId);
        if (previous) {
          if (encode(previous.original) !== encode(captured) || encode(previous.scope) !== fenceKey) gameStorageFail("recovery_original_conflict");
          return stored;
        }
        if (stored.operations.length >= 256) gameStorageFail("recovery_capacity_exhausted");
        stored.operations.push({ original: captured, scope: capturedScope, state: "unknown", receipt: null });
        return stored;
      });
      return copy(record.operations.find((entry) => entry.original.operationId === captured.operationId).original);
    },
    async settle(id, result) {
      operationId(id);
      if (!states.has(result?.state) || (result.state === "committed" && !result.receipt)) gameStorageFail("recovery_result_invalid");
      const captured = copy(stable({ state: result.state, receipt: result.receipt ?? null }));
      const record = await mutate((stored) => {
        const entry = stored.operations.find((item) => item.original.operationId === id);
        if (!entry) gameStorageFail("recovery_original_missing");
        if (terminal(entry.state)) {
          if (entry.state !== captured.state || encode(entry.receipt) !== encode(captured.receipt)) gameStorageFail("recovery_settlement_conflict");
          return stored;
        }
        entry.state = captured.state; entry.receipt = captured.receipt;
        return stored;
      });
      return copy(record.operations.find((entry) => entry.original.operationId === id));
    },
    async reconcile(lookup) {
      if (typeof lookup !== "function") gameStorageFail("recovery_lookup_required");
      const loaded = await api.load();
      for (const entry of loaded.operations) {
        if (terminal(entry.state)) continue;
        // Lookup verifies service signature, correlation and canonical receipts
        // in the Game Client. This module never fabricates a new signed request.
        let result;
        try { result = await lookup({ ...copy(entry.original), scope: copy(entry.scope) }); }
        catch { continue; } // Preserve Unknown/Pending, never assume no effect.
        if (states.has(result?.state)) await api.settle(entry.original.operationId, result);
      }
      return api.load();
    },
    async hasUnresolved() { return (await api.load()).operations.some((entry) => !terminal(entry.state)); },
  };
  return api;
}
