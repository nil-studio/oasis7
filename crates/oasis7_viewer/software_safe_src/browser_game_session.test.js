import { afterEach, describe, expect, it, vi } from "vitest";
import { createBrowserGameSession } from "./browser_game_session.js";
import { createBrowserParticipantGateway } from "./browser_participant_entry.js";
import { directActionSelection } from "./browser_game_permit.js";

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
  const actionPermits = [];
  const recoveryFactory = vi.fn(({ scope }) => {
    const recovery = { scope: clone(scope), load: vi.fn(async () => ({ operations, actionPermits })), hasUnresolved: vi.fn(async () => unknown),
      initializeActionPermit: vi.fn(async (permitId, value) => {
        const previous = actionPermits.find((item) => item.permitId === permitId); if (previous) return previous;
        const permit = { permitId, ...value, originalOperationId: null }; actionPermits.push(permit); return permit;
      }), getActionPermit: vi.fn(async (id) => actionPermits.find((item) => item.permitId === id)),
      initializeRun: vi.fn(async (_runId, { fence: expected }) => {
        if (JSON.stringify(scope.fence) !== JSON.stringify(expected)) throw failure("run_fence_conflict");
      }), reserveModelCall: vi.fn(async () => ({ id: 1, runId: "run" })), settleModelCall: vi.fn(async () => {}) };
    recoveries.push(recovery); return recovery;
  });
  const clients = [];
  const clientFactory = vi.fn((options) => {
    const client = { options, executorPublicKey: executor, agentId: "agent",
      readTask: vi.fn(async () => { await hooks.readTask?.(options); return { fence: clone(fence) }; }),
      recover: vi.fn(async () => { await hooks.recover?.(options); }), hasUnresolved: vi.fn(async () => unknown || operations.some((entry) => entry.original.kind === "prefix"
        && !operations.some((other) => ["action", "outcome"].includes(other.original.kind) && ["committed", "rejected", "expired"].includes(other.state)
          && other.original.operationId.replace(/:(action|outcome)$/, "") === entry.original.operationId.replace(/:prefix$/, "")))),
      freshFence: vi.fn(async () => { await hooks.freshFence?.(options); return { fence: clone(fence) }; }),
      bindFence: vi.fn(async () => {}), snapshot: vi.fn(async () => ({ active_goal: { owner_proof: { request: { content: "Build safely" } } } })),
      registerExecutor: vi.fn(async () => { await options.beforeEffect(); await hooks.registration?.(options); effectCount += 1;
        fence = { ...fence, executor_epoch: "1", executor_binding: { executor_public_key: executor } }; }),
      effect: vi.fn(async () => { await options.beforeEffect(); effectCount += 1; }),
      context: vi.fn(async (operationId, runBudget) => {
        await hooks.context?.(options);
        if (!operations.some((entry) => entry.original.operationId === `${operationId}:prefix`)) {
          await options.beforeEffect({ operationId: `${operationId}:prefix`, kind: "prefix", runBudget });
          effectCount += 1;
          operations.push({ original: { operationId: `${operationId}:prepare`, kind: "prepare", runBudget }, state: "committed" });
          operations.push({ original: { operationId: `${operationId}:prefix`, kind: "prefix", runBudget, context: { fence: clone(fence), valid_until_tick: "10" } }, state: "committed" });
        }
        return { prepared: { identity: { fence: clone(fence) }, valid_until_tick: "10" }, current_public_context: {} };
      }),
      quote: vi.fn(async (operationId, captured, kind, selection) => {
        await hooks.quote?.(options);
        return { captured, quoted: { quote: { rejection: null, action: { type: "ScheduleRecipe", data: {
          requester_agent_id: "agent", factory_id: selection.factory, recipe_id: selection.recipe,
          plan: { accepted_batches: 1, consume: [{ kind: "iron_ore", amount: 4 }, { kind: "carbon_fuel", amount: 1 }], power_required: 8 },
          logistics_route_ids: [], logistics_path_ids: [] } }, preview: { power_required: 8,
          consume: [{ kind: "iron_ore", amount: 4 }, { kind: "carbon_fuel", amount: 1 }, { kind: "maintenance_part", amount: 1 }] } } } };
      }),
      action: vi.fn(async (operationId, captured, action, runBudget) => {
        await options.beforeEffect({ operationId: `${operationId}:action`, kind: "action", runBudget }); effectCount += 1;
        actionPermits.find((item) => item.permitId === runBudget.permitId).originalOperationId = `${operationId}:action`;
        operations.push({ original: { operationId: `${operationId}:action`, kind: "action", runBudget }, state: "committed" });
        return { outcome: { status: "committed" } };
      }),
      outcome: vi.fn(async (operationId, captured, reason, runBudget) => {
        await options.beforeEffect({ operationId: `${operationId}:outcome`, kind: "outcome", runBudget }); effectCount += 1;
        operations.push({ original: { operationId: `${operationId}:outcome`, kind: "outcome", runBudget }, state: "committed" });
        return { outcome: { status: "committed" } };
      }),
      retireExpiredContexts: vi.fn(async ({ operationId }) => {
        const original = operations.find((entry) => entry.original.operationId === `${operationId}:prefix`).original;
        await options.beforeEffect({ operationId: `${operationId}:outcome`, kind: "outcome", runBudget: original.runBudget }); effectCount += 1;
        operations.push({ original: { operationId: `${operationId}:outcome`, kind: "outcome", runBudget: original.runBudget }, state: "committed" });
        return { retired: 1 };
      }),
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
  const claimSelection = () => directActionSelection({ type: "WorldRule", data: { actor_id: "agent", request_id: "claim-request",
    command: { type: "ClaimOrdinaryIndustrialStarter", data: { offer_id: "fixture-offer" } } } }, "agent");
  it("executes one explicit player action without a model Key and labels it as player work", async () => {
    const env = setup({ bound: true }); env.modelState.hasKey = false; env.modelState.checked = false;
    const session = await ready(env), confirmedFence = session.snapshot().fence;
    const result = await session.directAction({ confirmedFence, permitId: "claim", selection: claimSelection() });
    expect(result).toMatchObject({ phase: "settled", submissionState: "committed", agentDelegated: false });
    expect(env.clients.at(-1).action).toHaveBeenCalledTimes(1);
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled(); expect(env.modelSession.reserveExecution).not.toHaveBeenCalled();
    expect(env.runners.at(-1).start).not.toHaveBeenCalled();
    await session.directAction({ confirmedFence, permitId: "claim", selection: claimSelection() });
    expect(env.clients.at(-1).action).toHaveBeenCalledTimes(1);
    await expect(env.clients.at(-1).effect()).rejects.toMatchObject({ code: "run_confirmation_required" });
  });
  it("makes player direct work mutually exclusive with Start and never widens the runtime permit", async () => {
    const env = setup({ bound: true }); const session = await ready(env), pending = deferred();
    env.hooks.context = async (options) => {
      await expect(options.beforeEffect({ operationId: "unrelated:action", kind: "action", runBudget: { permitId: "claim" } }))
        .rejects.toMatchObject({ code: "action_permit_scope_mismatch" });
      await pending.promise;
    };
    const direct = session.directAction({ confirmedFence: session.snapshot().fence, permitId: "claim", selection: claimSelection() });
    await flush();
    await expect(session.start()).rejects.toMatchObject({ code: "already_running" });
    await expect(session.prepare({ inspectOnly: true })).rejects.toMatchObject({ code: "already_running" });
    pending.resolve(); await direct;
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled();
  });
  it("previews a real native quote under an explicit prefix-only player permit without submitting an action", async () => {
    const env = setup({ bound: true }); env.modelState.hasKey = false;
    const session = await ready(env), confirmedFence = session.snapshot().fence;
    env.hooks.quote = async (options) => {
      await expect(options.beforeEffect({ operationId: "preview:first:action", kind: "action", runBudget: { previewPermitId: "first" } }))
        .rejects.toMatchObject({ code: "action_permit_scope_mismatch" });
    };
    const preview = await session.preview({ confirmedFence, permitId: "first", kind: "schedule",
      selection: { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 } });
    expect(preview).toMatchObject({ contextOperationId: "preview:first", selection: { kind: "first_production", maxElectricity: 8 } });
    expect(preview.selection.maxConsume).toContainEqual({ kind: "maintenance_part", amount: 1 });
    expect(env.clients.at(-1).action).not.toHaveBeenCalled(); expect(env.modelSession.beginExecution).not.toHaveBeenCalled();
    expect(session.snapshot()).toMatchObject({ phase: "preview_ready", agentDelegated: false });
  });
  it("discards a late player quote after Stop before any action can be submitted", async () => {
    const env = setup({ bound: true }); const session = await ready(env), pending = deferred();
    env.hooks.quote = () => pending.promise;
    const preview = expect(session.preview({ confirmedFence: session.snapshot().fence, permitId: "first", kind: "schedule",
      selection: { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 } })).rejects.toMatchObject({ code: "stopped" });
    await flush(); session.stop(); pending.resolve(); await preview;
    expect(env.clients.at(-1).action).not.toHaveBeenCalled(); expect(env.modelSession.beginExecution).not.toHaveBeenCalled();
    expect(session.snapshot().phase).toBe("stopped");
  });
  it("explicitly ends a rejected preview without a Key, then permits a new player operation without resetting a model run", async () => {
    const env = setup({ bound: true }); env.modelState.hasKey = false; env.modelState.checked = false;
    const session = await ready(env), confirmedFence = session.snapshot().fence;
    env.hooks.quote = async () => { throw failure("quote_rejected"); };
    await expect(session.preview({ confirmedFence, permitId: "one", kind: "schedule", selection: { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 } }))
      .rejects.toMatchObject({ code: "quote_rejected" });
    expect(env.clients.at(-1).outcome).not.toHaveBeenCalled();
    await session.endPreview({ confirmedFence, permitId: "one", contextOperationId: "preview:one" });
    expect(env.clients.at(-1).outcome).toHaveBeenCalledWith("preview:one", expect.any(Object), "executor_aborted", expect.objectContaining({ previewPermitId: "one" }));
    await session.directAction({ confirmedFence, permitId: "new-player-choice", selection: claimSelection() });
    expect(env.clients.at(-1).action).toHaveBeenCalledTimes(1);
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled(); expect(env.recoveries.at(-1).initializeRun).not.toHaveBeenCalled();
  });
  it("explicitly retires only an expired preview without a Key, and a new click uses a later journal identity", async () => {
    const env = setup({ bound: true }); env.modelState.hasKey = false;
    const seen = [];
    const session = env.create({ protocolLoader: async () => ({ requestDigest: (domain, value) => { seen.push(JSON.parse(value)); return `${domain}:${JSON.parse(value).sequence ?? 0}`; } }) });
    const inspected = await session.prepare({ inspectOnly: true }); await session.prepare({ confirmedFence: inspected.fence });
    const confirmedFence = session.snapshot().fence;
    const firstId = await session.nextPreviewPermitId({ factory: "factory" });
    await session.preview({ confirmedFence, permitId: firstId, kind: "schedule", selection: { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 } });
    const client = env.clients.at(-1); client.freshFence.mockResolvedValue({ fence: confirmedFence, logicalTick: "11" });
    await session.endPreview({ confirmedFence, permitId: firstId, contextOperationId: `preview:${firstId}` });
    expect(client.retireExpiredContexts).toHaveBeenCalledWith({ operationId: `preview:${firstId}` });
    expect(client.outcome).not.toHaveBeenCalled();
    const nextId = await session.nextPreviewPermitId({ factory: "factory" }); expect(nextId).not.toBe(firstId);
    expect(seen.filter((item) => Object.hasOwn(item, "sequence")).map((item) => item.sequence)).toEqual([0, 1]);
    expect(env.modelSession.beginExecution).not.toHaveBeenCalled(); expect(env.recoveries.at(-1).initializeRun).not.toHaveBeenCalled();
  });
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

it("keeps safe diagnostics in a blocked executor session snapshot", async () => {
 const env = setup(); env.hooks.readTask = async () => { throw { code: "participant_request_failed", diagnostic: { stage: "participant_owner_view", httpStatus: 403, serverCode: "csrf_failed", message: "PRIVATE_KEY" } }; };
 const session = env.create(); await expect(session.prepare({ inspectOnly: true })).rejects.toMatchObject({ code: "participant_request_failed" });
 expect(session.snapshot()).toMatchObject({ phase: "blocked", diagnostic: { stage: "participant_owner_view", httpStatus: 403, serverCode: "csrf_failed" } });
 expect(JSON.stringify(session.snapshot())).not.toContain("PRIVATE_KEY");
});
