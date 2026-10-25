import { afterEach, describe, expect, it, vi } from "vitest";
import { createBrowserGameSession } from "./browser_game_session.js";
import { createBrowserParticipantGateway } from "./browser_participant_entry.js";

const sessions = [];
afterEach(() => { for (const session of sessions.splice(0)) session.dispose(); });
const clone = (value) => JSON.parse(JSON.stringify(value));
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const flush = async () => { for (let i = 0; i < 30; i += 1) await Promise.resolve(); };
const failure = (code) => Object.assign(new Error(code), { code });
const executor = "ee".repeat(32);
function setup({ bound = false, unknown = false, operations = [] } = {}) {
  const config = { world: { world_id: "world", genesis_digest: "genesis" } };
  const initial = { task_id: "task", world_id: "world", genesis_digest: "genesis", owner_public_key: "owner",
    executor_epoch: bound ? "1" : "0", executor_binding: bound ? { executor_public_key: executor } : null };
  let fence = clone(initial), effectCount = 0;
  const hooks = {};
  const modelListeners = new Set();
  const modelState = { revision: 1, hasKey: true, checked: true };
  const modelSession = {
    subscribe: (listener) => { modelListeners.add(listener); return () => modelListeners.delete(listener); }, snapshot: () => ({ ...modelState }), dispose: vi.fn(),
    stop: vi.fn(async () => { modelState.revision += 1; modelState.checked = false; }),
    forget: vi.fn(async () => {}), logout: vi.fn(async () => {}),
    beginExecution: vi.fn(async () => { await hooks.beginExecution?.(); modelState.checked = false; return async () => "local-model-secret"; }),
    reserveExecution: vi.fn(async () => ({ id: 1, revision: modelState.revision })),
    settleExecution: vi.fn(async () => {}),
  };
  const identityStore = { getOrCreate: vi.fn(async (scope) => { await hooks.identity?.(scope); return { publicKeyHex: executor, sign: async () => new Uint8Array(64) }; }) };
  const participantFactory = vi.fn((options) => ({ prepare: vi.fn(async () => { await hooks.participant?.(options); }),
    getParticipantProofs: async () => ({ owner: {}, admission: {} }), gatewayCall: async () => "{}" }));
  const recoveries = [];
  const recoveryFactory = vi.fn(({ scope }) => {
    const recovery = { scope: clone(scope), load: vi.fn(async () => ({ operations })), hasUnresolved: vi.fn(async () => unknown),
      initializeRun: vi.fn(async (_runId, { fence: expected }) => {
        if (JSON.stringify(scope.fence) !== JSON.stringify(expected)) throw failure("run_fence_conflict");
      }), reserveModelCall: vi.fn(async () => ({ id: 1, runId: "run" })), settleModelCall: vi.fn(async () => {}) };
    recoveries.push(recovery); return recovery;
  });
  const clients = [];
  const clientFactory = vi.fn((options) => {
    const client = { options, executorPublicKey: executor,
      readTask: vi.fn(async () => { await hooks.readTask?.(options); return { fence: clone(fence) }; }),
      recover: vi.fn(async () => { await hooks.recover?.(options); }), hasUnresolved: vi.fn(async () => unknown || operations.some((entry) => entry.original.kind === "prefix")),
      freshFence: vi.fn(async () => { await hooks.freshFence?.(options); return { fence: clone(fence) }; }),
      bindFence: vi.fn(async () => {}), snapshot: vi.fn(async () => ({ active_goal: { owner_proof: { request: { content: "Build safely" } } } })),
      registerExecutor: vi.fn(async () => { await options.beforeEffect(); await hooks.registration?.(options); effectCount += 1;
        fence = { ...fence, executor_epoch: "1", executor_binding: { executor_public_key: executor } }; }),
      effect: vi.fn(async () => { await options.beforeEffect(); effectCount += 1; }),
    };
    clients.push(client); return client;
  });
  const runners = [];
  const runnerFactory = vi.fn((options) => {
    let phase = "idle";
    const runner = { options, state: () => ({ phase }), stop: vi.fn(() => { phase = "stopped"; }), recover: vi.fn(async () => true),
      start: vi.fn(async (run) => {
        phase = "running"; options.onState({ phase });
        await options.budget.initialize({ runId: run.runId, fence: run.confirmedFence });
        await hooks.runnerStart?.(options, run);
        phase = "stopped"; options.onState({ phase, code: "done" }); return { phase };
      }) };
    runners.push(runner); return runner;
  });
  const legacyGuard = vi.fn(async (options) => { await hooks.legacy?.(options); });
  const documentRef = new EventTarget(); Object.defineProperty(documentRef, "visibilityState", { value: "visible", configurable: true });
  const create = (override = {}) => {
    const session = createBrowserGameSession({ config, accountId: "account", agentId: "agent", ownerSigner: { publicKeyHex: "owner" },
      identityStore, participantFactory, recoveryFactory, clientFactory, runnerFactory, legacyGuard, modelSession,
      protocolLoader: async () => ({ requestDigest: (domain) => `${domain}:digest` }),
      documentRef, windowRef: new EventTarget(), navigatorRef: { onLine: true }, now: () => 1000,
      locks: { request: async (_name, _options, action) => action({ name: "activity" }) }, ...override });
    sessions.push(session); return session;
  };
  return { create, hooks, modelState, modelSession, identityStore, participantFactory, recoveryFactory,
    recoveries, clients, runners, legacyGuard, initial, documentRef,
    effects: () => effectCount, setFence: (value) => { fence = clone(value); }, emitModel: () => { for (const listener of modelListeners) listener(); } };
}
async function ready(env) {
  const session = env.create(); const inspected = await session.prepare({ inspectOnly: true });
  await session.prepare({ confirmedFence: inspected.fence }); return session;
}

describe("browser game session authorization", () => {
  it("inspects without registration, then requires the exact displayed Task fence", async () => {
    const env = setup(); const session = env.create();
    await expect(session.prepare()).rejects.toMatchObject({ code: "run_confirmation_required" });
    const inspected = await session.prepare({ inspectOnly: true });
    expect(inspected).toMatchObject({ phase: "inspected", ready: false, fence: env.initial });
    expect(env.effects()).toBe(0); expect(env.legacyGuard).not.toHaveBeenCalled();
    expect(env.identityStore.getOrCreate).toHaveBeenCalledWith({ worldId: "world", genesisDigest: "genesis", accountId: "account", agentId: "agent" });
    await expect(session.start()).rejects.toMatchObject({ code: "run_confirmation_required" });
    env.setFence({ ...env.initial, task_id: "changed-task" });
    await expect(session.prepare({ confirmedFence: inspected.fence })).rejects.toMatchObject({ code: "task_fence_changed" });
    expect(env.effects()).toBe(0);
  });
  it("rebinds durable recovery and run budgets to the post-registration fence", async () => {
    const env = setup(); const session = await ready(env);
    expect(env.effects()).toBe(1);
    const actual = session.snapshot().fence;
    expect(actual.executor_binding.executor_public_key).toBe(executor);
    expect(env.recoveries.at(-1).scope.fence).toEqual(actual);
    await session.start();
    expect(env.recoveries.at(-1).initializeRun).toHaveBeenCalledWith("browser-agent-run:digest", { fence: actual });
    expect(env.legacyGuard).toHaveBeenCalledTimes(2);
  });
  it("blocks unknown originals, but lets a committed prefix reach explicit Start retirement", async () => {
    const blocked = setup({ bound: true, unknown: true }); const bad = blocked.create();
    const inspection = await bad.prepare({ inspectOnly: true });
    expect(inspection.code).toBe("recovery_required");
    await expect(bad.prepare({ confirmedFence: inspection.fence })).rejects.toMatchObject({ code: "recovery_required" });
    const env = setup({ bound: true, operations: [{ original: { kind: "prefix", operationId: "old:prefix" }, state: "committed" }] });
    const session = await ready(env);
    env.hooks.runnerStart = async (options) => { await options.gameClient.effect(); };
    await session.start();
    expect(env.effects()).toBe(1); // explicit running permit, never inspection
  });
  it("blocks legacy runs before registration and before execution", async () => {
    const env = setup(); const session = env.create(); const inspected = await session.prepare({ inspectOnly: true });
    env.hooks.legacy = async () => { throw failure("legacy_run_active"); };
    await expect(session.prepare({ confirmedFence: inspected.fence })).rejects.toMatchObject({ code: "legacy_run_active" });
    expect(env.effects()).toBe(0);
    env.hooks.legacy = undefined;
    await session.prepare({ confirmedFence: inspected.fence });
    env.hooks.legacy = async () => { throw failure("legacy_state_unknown"); };
    await expect(session.start()).rejects.toMatchObject({ code: "legacy_state_unknown" });
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled();
  });
  it("aborts a stopped preparation and keeps its late result from overriding a newer inspection", async () => {
    const env = setup(); const session = env.create(); const pending = deferred();
    env.hooks.participant = () => pending.promise;
    const old = expect(session.prepare({ inspectOnly: true })).rejects.toMatchObject({ code: "stopped" });
    await flush();
    const firstSignal = env.participantFactory.mock.calls[0][0].signal;
    session.stop(); expect(firstSignal.aborted).toBe(true);
    env.hooks.participant = undefined;
    const fresh = await session.prepare({ inspectOnly: true });
    pending.resolve(); await old;
    expect(session.snapshot()).toEqual(fresh);
    expect(env.effects()).toBe(0);
  });
  it("stops an old registration permit even while a new prepare is active", async () => {
    const env = setup(); const session = await ready(env);
    const oldRegistrationClient = env.clients.find((client) => client.registerExecutor.mock.calls.length);
    session.stop();
    const pending = deferred(); env.hooks.participant = () => pending.promise;
    const next = session.prepare({ inspectOnly: true }); await flush();
    await expect(oldRegistrationClient.options.beforeEffect()).rejects.toMatchObject({ code: "stopped" });
    pending.resolve(); await next;
    expect(env.effects()).toBe(1);
  });
  it("does not rearm model permission when stopped during the fresh-fence await", async () => {
    const env = setup({ bound: true }); const session = await ready(env); const pending = deferred();
    env.hooks.freshFence = () => pending.promise;
    const start = expect(session.start()).rejects.toMatchObject({ code: "stopped" });
    await flush(); session.stop(); pending.resolve(); await start;
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled();
    expect(env.runners.at(-1).start).not.toHaveBeenCalled();
  });
  it("does not start a late rearm result or overwrite newer generation credentials", async () => {
    const env = setup({ bound: true }); const session = await ready(env); const pending = deferred();
    env.hooks.beginExecution = () => pending.promise;
    const start = expect(session.start()).rejects.toMatchObject({ code: "stopped" });
    await flush(); session.stop();
    const stopCount = env.modelSession.stop.mock.calls.length;
    pending.resolve(); await start;
    expect(env.runners.at(-1).start).not.toHaveBeenCalled();
    expect(env.modelSession.stop).toHaveBeenCalledTimes(stopCount);
    expect(session.snapshot()).toMatchObject({ phase: "stopped", ready: false });
  });
  it("requires a checked current model revision before Start and after awaited reads", async () => {
    const env = setup({ bound: true }); const session = await ready(env);
    env.modelState.checked = false;
    await expect(session.start()).rejects.toMatchObject({ code: "run_confirmation_required" });
    env.modelState.checked = true;
    env.hooks.freshFence = async () => { env.modelState.checked = false; };
    await expect(session.start()).rejects.toMatchObject({ code: "run_confirmation_required" });
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled();
  });
  it("pre-reserves both durable run and shared account budgets and settles measured usage", async () => {
    const env = setup({ bound: true }); const session = await ready(env);
    let reservation;
    const usage = { prompt_tokens: 100, completion_tokens: 20 };
    env.hooks.runnerStart = async (options) => {
      reservation = await options.budget.reserve({ inputBytes: 1000, maxOutputTokens: 512 });
      await options.budget.settle({ reservation, usage });
    };
    await session.start();
    expect(env.recoveries.at(-1).reserveModelCall).toHaveBeenCalledWith("browser-agent-run:digest", 1000, 512);
    expect(env.modelSession.reserveExecution).toHaveBeenCalledWith({ inputBytes: 1000, maxOutputTokens: 512 });
    expect(env.recoveries.at(-1).settleModelCall).toHaveBeenCalledWith("browser-agent-run:digest", reservation.durable, { inputTokens: 100, outputTokens: 20 });
    expect(env.modelSession.settleExecution).toHaveBeenCalledWith({ reservation: reservation.shared, usage });
  });
  it("keeps a first ledger reservation when the second ledger blocks the send", async () => {
    const env = setup({ bound: true }); const session = await ready(env);
    env.modelSession.reserveExecution.mockRejectedValue(failure("budget_exhausted"));
    let sent = false;
    env.hooks.runnerStart = async (options) => { await options.budget.reserve({ inputBytes: 1000, maxOutputTokens: 512 }); sent = true; };
    await expect(session.start()).rejects.toMatchObject({ code: "budget_exhausted" });
    expect(sent).toBe(false); expect(env.recoveries.at(-1).reserveModelCall).toHaveBeenCalledTimes(1);
    expect(env.recoveries.at(-1).settleModelCall).not.toHaveBeenCalled();
  });
  it("cancels session-CSRF retrieval after an admitted registration and never posts after Stop", async () => {
    const env = setup(); const session = env.create(); const inspected = await session.prepare({ inspectOnly: true });
    const pending = deferred();
    const fetchImpl = vi.fn(() => pending.promise);
    env.hooks.registration = async (options) => {
      const gateway = createBrowserParticipantGateway({ config: { endpoint: "/api/public/hosted-account/v2" },
        accountId: "account", fetchImpl, signal: options.signal });
      await gateway.gatewayCall("/entry-owner/control", { operation: "register" },
        { beforeSend: options.beforeEffect, signal: options.signal });
    };
    const preparing = expect(session.prepare({ confirmedFence: inspected.fence })).rejects.toMatchObject({ code: "participant_stopped" });
    await flush(); expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][0]).toMatch(/\/session$/);
    session.stop();
    pending.resolve(new Response(JSON.stringify({ api_version: 2, ok: true, account: { account_id: "account" },
      authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) })));
    await preparing; await flush();
    expect(fetchImpl).toHaveBeenCalledTimes(1); expect(env.effects()).toBe(0);
    expect(env.participantFactory.mock.calls.at(-1)[0].signal.aborted).toBe(true);
  });
  it("clears the previous goal on a fresh inspection and permits read-only reconciliation before ready", async () => {
    const env = setup({ bound: true }); const session = await ready(env);
    expect(session.snapshot().goal).toBe("Build safely");
    await session.prepare({ inspectOnly: true });
    expect(session.snapshot()).toMatchObject({ phase: "inspected", ready: false, goal: null });
    expect(await session.recover()).toBe(true);
    expect(session.snapshot()).toMatchObject({ phase: "inspected", ready: false });
    expect(env.effects()).toBe(0); expect(env.runners.at(-1).start).not.toHaveBeenCalled();
    const unknown = setup({ bound: true, unknown: true }); const blocked = unknown.create();
    await blocked.prepare({ inspectOnly: true });
    expect(await blocked.recover()).toBe(false);
    expect(blocked.snapshot()).toMatchObject({ phase: "blocked", code: "recovery_required", ready: false });
  });
  it("does not publish a stopped read-only recovery over a new inspection", async () => {
    const env = setup({ bound: true }); const session = env.create(); await session.prepare({ inspectOnly: true });
    const pending = deferred(); env.hooks.recover = () => pending.promise;
    const old = expect(session.recover()).rejects.toMatchObject({ code: "stopped" });
    await flush(); session.stop(); env.hooks.recover = undefined;
    const inspected = await session.prepare({ inspectOnly: true });
    pending.resolve(); await old;
    expect(session.snapshot()).toEqual(inspected);
  });
  it("immediately aborts the run generation when another tab changes the credential revision", async () => {
    const env = setup({ bound: true }); const session = await ready(env); const pending = deferred();
    env.hooks.runnerStart = async (options) => { await pending.promise; await options.gameClient.effect(); };
    const running = expect(session.start()).rejects.toMatchObject({ code: "stopped" });
    await flush();
    const runtimeClient = env.clients.at(-1);
    env.modelState.revision += 1; env.emitModel();
    expect(runtimeClient.options.signal.aborted).toBe(true);
    expect(session.snapshot()).toMatchObject({ phase: "stopped", code: "model_connection_changed", ready: false });
    pending.resolve(); await running;
    expect(env.effects()).toBe(0);
  });
});
