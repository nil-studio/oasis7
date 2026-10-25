import { describe, it, expect, vi } from "vitest";
import { createBrowserGameRecovery } from "./browser_game_recovery.js";

const scope = { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent",
  taskId: "task", fence: { owner_generation: "1", executor_epoch: "1" } };
const original = (operationId = "action:1") => ({ operationId, kind: "action", signedRequest: {
  request: { nonce: "original-nonce", task_fence: scope.fence }, signature: "signed-original" },
  correlation: { operation_id: operationId, payload_digest: "digest" }, context: { context_id: "original-context" },
  runBudget: { callsReserved: 1, tokensReserved: 2048 } });
function setup() {
  const records = new Map();
  const storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => {
    const updated = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(updated));
    return structuredClone(updated);
  } };
  const locks = { request: async (_name, _options, action) => action() };
  const create = (override = {}) => createBrowserGameRecovery({ scope, storage, locks, ...override });
  return { storage, records, create };
}
describe("browser original-operation recovery", () => {
  it("commits original materials before allowing submit and preserves immutable nonce on reload", async () => {
    const { create } = setup(); const first = create(); const request = original();
    const saved = await first.remember(request);
    request.signedRequest.request.nonce = "changed";
    const reloaded = await create().load();
    expect(reloaded.operations[0].original).toEqual(saved);
    expect(saved.signedRequest.request.nonce).toBe("original-nonce");
    await expect(first.remember(request)).rejects.toMatchObject({ code: "recovery_original_conflict" });
    expect((await first.remember(saved)).operationId).toBe("action:1");
  });
  it("never reaches submit when durable storage fails", async () => {
    const { create, storage } = setup(); const submit = vi.fn();
    storage.update = async () => { throw new Error("failed write"); };
    await expect(create().remember(original()).then(submit)).rejects.toMatchObject({ code: "recovery_storage_failed" });
    expect(submit).not.toHaveBeenCalled();
  });
  it("reconciles exact originals read-only and keeps Pending/HistoryUnavailable/Unknown unresolved", async () => {
    const { create } = setup(); const recovery = create();
    for (const name of ["pending", "history", "unknown", "committed"]) await recovery.remember(original(name));
    const lookup = vi.fn(async (stored) => ({ state: { pending: "pending", history: "history_unavailable",
      unknown: "unknown", committed: "committed" }[stored.operationId], receipt: stored.operationId === "committed" ? { result: "done" } : null }));
    await recovery.reconcile(lookup);
    expect(lookup).toHaveBeenCalledTimes(4);
    for (const [stored] of lookup.mock.calls) {
      expect(stored.signedRequest.request.nonce).toBe("original-nonce");
      expect(stored.context.context_id).toBe("original-context");
    }
    expect(await recovery.hasUnresolved()).toBe(true);
    await recovery.reconcile(lookup);
    expect(lookup).toHaveBeenCalledTimes(7); // settled original is never retried
    await expect(recovery.settle("committed", { state: "unknown" })).rejects.toMatchObject({ code: "recovery_settlement_conflict" });
  });
  it("retains historical fence originals after Stop, but isolates world/owner/agent/task", async () => {
    const { create } = setup(); await create().remember(original());
    const changed = create({ scope: { ...scope, fence: { ...scope.fence, executor_epoch: "2" } } });
    expect(await changed.hasUnresolved()).toBe(true);
    await expect(changed.remember(original())).rejects.toMatchObject({ code: "recovery_original_conflict" });
    const lookup = vi.fn(async () => { throw new Error("network unknown"); });
    await changed.reconcile(lookup);
    expect(lookup.mock.calls[0][0].scope.fence.executor_epoch).toBe("1");
    for (const field of ["worldId", "genesisDigest", "accountId", "agentId", "taskId"]) {
      expect(await create({ scope: { ...scope, [field]: "other" } }).hasUnresolved()).toBe(false);
    }
  });
  it("rejects secrets, malformed state, corrupted journals and absent locks", async () => {
    const { create, records } = setup(); const recovery = create();
    await expect(recovery.remember({ ...original(), credential: "model-secret" }))
      .rejects.toMatchObject({ code: "recovery_secret_forbidden" });
    await expect(create({ locks: {} }).remember(original())).rejects.toMatchObject({ code: "recovery_locks_unavailable" });
    await recovery.remember(original());
    await expect(recovery.settle("action:1", { state: "committed" })).rejects.toMatchObject({ code: "recovery_result_invalid" });
    [...records.values()][0].schema = 2;
    await expect(recovery.load()).rejects.toMatchObject({ code: "recovery_corrupt" });
  });
  it("reserves physical run calls durably across tabs and never resets the same fence", async () => {
    const { create } = setup(); let time = 1000;
    const recovery = create({ now: () => time });
    await recovery.initializeRun("stable-run", { fence: scope.fence });
    const results = await Promise.all(Array.from({ length: 9 }, () => recovery.reserveModelCall("stable-run", 100, 512)
      .then(() => "reserved", (error) => error.code)));
    expect(results.filter((value) => value === "reserved")).toHaveLength(8);
    expect(results.filter((value) => value === "run_budget_exhausted")).toHaveLength(1);
    const reloaded = create({ now: () => time });
    expect((await reloaded.initializeRun("stable-run", { fence: scope.fence })).callsReserved).toBe(8);
    await expect(reloaded.initializeRun("other-run", { fence: scope.fence })).rejects.toMatchObject({ code: "run_identity_conflict" });
    await expect(reloaded.reserveModelCall("stable-run", 1, 1)).rejects.toMatchObject({ code: "run_budget_exhausted" });
    time = 121000;
    await expect(reloaded.reserveModelCall("stable-run", 1, 1)).rejects.toMatchObject({ code: "run_deadline_exceeded" });
    expect((await reloaded.initializeRun("stable-run", { fence: scope.fence })).deadline).toBe(121000);
  });
  it("keeps unknown model usage reserved and refunds validated usage once", async () => {
    const { create } = setup(); const recovery = create({ now: () => 1000 });
    await recovery.initializeRun("stable-run", { fence: scope.fence });
    const reservation = await recovery.reserveModelCall("stable-run", 1000, 512);
    for (const usage of [null, {}, { inputTokens: -1, outputTokens: 1 },
      { inputTokens: 1.5, outputTokens: 1 }, { inputTokens: 2000, outputTokens: 0 },
      { inputTokens: Number.MAX_SAFE_INTEGER, outputTokens: 1 }]) {
      expect((await recovery.settleModelCall("stable-run", reservation, usage)).tokensReserved).toBe(1512);
    }
    expect((await recovery.settleModelCall("stable-run", reservation, { inputTokens: 100, outputTokens: 30 })).tokensReserved).toBe(130);
    expect((await recovery.getRunBudget("stable-run")).callsReserved).toBe(1);
    await expect(recovery.settleModelCall("stable-run", reservation, { inputTokens: 0, outputTokens: 0 }))
      .rejects.toMatchObject({ code: "run_reservation_invalid" });
  });
  it("enforces aggregate conservative input budget, output ceiling and original fence", async () => {
    const { create } = setup(); const recovery = create({ now: () => 1000 });
    await recovery.initializeRun("stable-run", { fence: scope.fence, limits: { maxCalls: 1000 } });
    await recovery.reserveModelCall("stable-run", 130000, 512);
    await expect(recovery.reserveModelCall("stable-run", 1000, 512)).rejects.toMatchObject({ code: "run_budget_exhausted" });
    await expect(recovery.reserveModelCall("stable-run", 1, 513)).rejects.toMatchObject({ code: "run_reservation_invalid" });
    const nextFence = { ...scope.fence, executor_epoch: "2" };
    const changed = create({ now: () => 1000, scope: { ...scope, fence: nextFence } });
    await expect(changed.reserveModelCall("stable-run", 1, 1)).rejects.toMatchObject({ code: "run_fence_conflict" });
    await expect(changed.initializeRun("stable-run", { fence: nextFence })).rejects.toMatchObject({ code: "run_fence_conflict" });
    expect((await changed.initializeRun("authorized-next-run", { fence: nextFence })).callsReserved).toBe(0);
  });
  it("fails closed on budget write errors, clock rollback and corrupt totals", async () => {
    const { create, storage, records } = setup(); let time = 1000;
    const recovery = create({ now: () => time });
    await recovery.initializeRun("stable-run", { fence: scope.fence });
    time = 999;
    await expect(recovery.reserveModelCall("stable-run", 1, 1)).rejects.toMatchObject({ code: "run_deadline_exceeded" });
    time = 1000;
    const update = storage.update;
    storage.update = async () => { throw new Error("quota"); };
    await expect(recovery.reserveModelCall("stable-run", 1, 1)).rejects.toMatchObject({ code: "recovery_storage_failed" });
    storage.update = update;
    expect((await recovery.getRunBudget("stable-run")).callsReserved).toBe(0);
    [...records.values()][0].runBudgets[0].tokensReserved = 1;
    await expect(recovery.load()).rejects.toMatchObject({ code: "run_budget_corrupt" });
  });
});
