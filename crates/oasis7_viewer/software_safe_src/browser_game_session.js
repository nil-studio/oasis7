import { browserGameDiagnostic } from "./browser_game_diagnostics.js";
import { loadBrowserGameProtocol } from "./browser_game_protocol.js";
import { createBrowserExecutorIdentity } from "./browser_game_identity.js";
import { createBrowserGameRecovery } from "./browser_game_recovery.js";
import { createBrowserParticipantEntry } from "./browser_participant_entry.js";
import { createBrowserGameClient, BrowserGameError } from "./browser_game_client.js";
import { createBrowserAgentRunner } from "./browser_agent_runner.js";
import { createBrowserModelSession } from "./browser_model_session.js";
import { assertNoLegacyRun } from "./browser_legacy_run_guard.js";
import { sameBrowserValue } from "./browser_game_permit.js";
import { executeBrowserDirectAction } from "./browser_direct_executor.js";
import { readBrowserActionPreview, endBrowserActionPreview, openBrowserPreviews } from "./browser_action_preview.js";

const same = sameBrowserValue;
const fail = (code) => { throw new BrowserGameError(code); };
// Pins and ownership come from the installed, authenticated Viewer session.
export function createBrowserGameSession({ config, accountId, agentId, ownerSigner,
  isCurrent = () => true, locks = globalThis.navigator?.locks,
  documentRef = globalThis.document, windowRef = globalThis.window,
  navigatorRef = globalThis.navigator, now = Date.now,
  protocolLoader = loadBrowserGameProtocol, identityStore = createBrowserExecutorIdentity(),
  participantFactory = createBrowserParticipantEntry, clientFactory = createBrowserGameClient,
  recoveryFactory = createBrowserGameRecovery, runnerFactory = createBrowserAgentRunner,
  legacyGuard = assertNoLegacyRun, modelSession = createBrowserModelSession({ accountId }) } = {}) {
  if (!config || !accountId || !agentId || !ownerSigner) fail("game_configuration_invalid");
  const listeners = new Set();
  let status = { phase: "unprepared", ready: false, code: null, fence: null };
  let codec, client, recovery, runner, credential, credentialRevision, runGeneration, operationController;
  let preparing = false, starting = null, disposed = false, generation = 0;
  let playerPermit = null;
  let lastWall = now();
  const notify = () => { for (const listener of listeners) listener(); };
  const publish = (value) => { status = { ...status, ...(Object.hasOwn(value, "code") ? { diagnostic: undefined } : {}), ...value }; notify(); };
  const unsubscribe = modelSession.subscribe(() => {
    if (runGeneration === generation && credential && credentialRevision !== undefined) {
      let current;
      try { current = modelSession.snapshot(); } catch { stop("storage_unavailable"); return; }
      if (current.revision !== credentialRevision || current.stopped || !current.hasKey) { stop("model_connection_changed"); return; }
    }
    notify();
  });
  function assertCurrent() {
    if (disposed || !isCurrent()) fail("game_session_changed");
    if (documentRef?.visibilityState !== "visible" || navigatorRef?.onLine === false) fail("page_paused");
    const current = now();
    if (current < lastWall || current - lastWall > 10000) { stop("page_paused"); fail("page_paused"); }
    lastWall = current;
  }
  function check(expected, controller) {
    assertCurrent();
    if (expected !== generation || controller?.signal.aborted) fail("stopped");
  }
  function stop(code = "stopped") {
    const stoppedGeneration = ++generation;
    operationController?.abort(); preparing = false; starting = null; runGeneration = null; playerPermit = null;
    runner?.stop(code); credential = null; credentialRevision = undefined;
    void modelSession.stop().catch(() => {
      if (generation === stoppedGeneration && !disposed) publish({ code: "storage_write_failed" });
    });
    publish({ phase: "stopped", ready: false, code });
  }
  const pause = () => { if (documentRef?.visibilityState !== "visible" || navigatorRef?.onLine === false) stop("page_paused"); };
  const wake = () => stop("page_paused");
  documentRef?.addEventListener("visibilitychange", pause);
  documentRef?.addEventListener("freeze", wake); documentRef?.addEventListener("resume", wake);
  windowRef?.addEventListener("pagehide", wake); windowRef?.addEventListener("offline", wake);
  const clock = setInterval(() => {
    const current = now();
    if ((current < lastWall || current - lastWall > 10000) && runner?.state().phase === "running") stop("page_paused");
    lastWall = current;
  }, 1000);
  async function prepare({ takeover = false, inspectOnly = false, confirmedFence } = {}) {
    if (preparing || starting === generation || runner?.state().phase === "running") fail("already_running");
    if (!inspectOnly && !confirmedFence) fail("run_confirmation_required");
    lastWall = now(); assertCurrent();
    operationController?.abort();
    const controller = new AbortController(); operationController = controller;
    const expected = ++generation; preparing = true; credential = null; runGeneration = null;
    const checked = () => check(expected, controller);
    publish({ phase: "preparing", ready: false, code: null });
    try {
      codec ??= await protocolLoader(); checked();
      const scope = { worldId: config.world.world_id, genesisDigest: config.world.genesis_digest, accountId, agentId };
      const executorSigner = await identityStore.getOrCreate(scope); checked();
      const participant = participantFactory({ config, codec, ownerSigner, accountId, agentId, signal: controller.signal });
      await participant.prepare(); checked();
      let registrationAllowed = false;
      const dependencies = { config, codec, ownerSigner, executorSigner, participantProofs: participant.getParticipantProofs,
        agentId, gatewayCall: participant.gatewayCall, signal: controller.signal,
        beforeEffect: async () => { checked(); if (!registrationAllowed) fail("run_confirmation_required"); } };
      const readOnly = { load: async () => ({ operations: [] }), remember: async () => fail("task_scope_required") };
      const observed = await clientFactory({ ...dependencies, recovery: readOnly }).readTask(); checked();
      let localRecovery = recoveryFactory({ scope: { ...scope, taskId: observed.fence.task_id, fence: observed.fence } });
      let localClient = clientFactory({ ...dependencies, taskId: observed.fence.task_id, recovery: localRecovery });
      await localClient.recover(); checked();
      // A committed prefix is not an unknown request. Explicit Start may retire
      // its original Context after expiry; read-only inspection never does so.
      const unknown = await localRecovery.hasUnresolved(); checked();
      if (inspectOnly) {
        client = localClient; recovery = localRecovery; runner = null;
        publish({ phase: "inspected", ready: false, code: unknown ? "recovery_required" : null, fence: observed.fence, goal: null });
        return { ...status };
      }
      if (!same(confirmedFence, observed.fence)) fail("task_fence_changed");
      if (unknown) fail("recovery_required");
      await legacyGuard({ signal: controller.signal }); checked();
      registrationAllowed = true;
      const existing = observed.fence.executor_binding;
      if (existing?.executor_public_key !== executorSigner.publicKeyHex) {
        if (existing && !takeover) fail("executor_takeover_confirmation_required");
        const operationId = codec.requestDigest("browser-executor-registration", JSON.stringify({ fence: observed.fence, executor: executorSigner.publicKeyHex }));
        const recorded = (await localRecovery.load()).operations; checked();
        let delegationGeneration = 1;
        for (const entry of recorded) {
          let wire; try { wire = JSON.parse(entry.original.signedRequest.rawJson); } catch { continue; }
          const request = wire?.signed_payload?.kind === "delegation" ? wire.signed_payload.payload?.request : null;
          if (request?.delegate_public_key === executorSigner.publicKeyHex && Number.isSafeInteger(request.generation)) delegationGeneration = Math.max(delegationGeneration, request.generation + 1);
        }
        checked();
        await localClient.registerExecutor({ operationId, confirmedFence: observed.fence, executorId: `browser:${executorSigner.publicKeyHex}`, generation: delegationGeneration, nonce: now() }); checked();
      }
      const current = await localClient.freshFence(); checked();
      registrationAllowed = false;
      // Registration changes the fence but keeps the durable recovery bucket.
      localRecovery = recoveryFactory({ scope: { ...scope, taskId: current.fence.task_id, fence: current.fence } });
      let localRunner;
      const runtimePermit = async (effect) => {
        checked();
        // A player click has its own exact operation permission. It never
        // grants access to the model run's credential/runtime permission.
        if (playerPermit?.generation === expected) {
          if (playerPermit.mode === "end_preview") {
            if (!effect || effect.kind !== "outcome" || effect.operationId !== `${playerPermit.operationId}:outcome`
              || effect.runBudget?.previewPermitId !== playerPermit.permitId) fail("action_permit_scope_mismatch");
            return;
          }
          if (playerPermit.mode === "grant") {
            if (!effect || effect.kind !== "native_grant" || effect.operationId !== `${playerPermit.operationId}:native_grant`
              || effect.runBudget?.ownerPermitId !== playerPermit.permitId) fail("action_permit_scope_mismatch");
            return;
          }
          if (!effect || effect.operationId !== `${playerPermit.operationId}:${effect.kind}`
            || !["prefix", ...(playerPermit.mode === "direct" ? ["action"] : [])].includes(effect.kind)
            || (playerPermit.mode === "direct" ? effect.runBudget?.permitId !== playerPermit.permitId
              : effect.runBudget?.previewPermitId !== playerPermit.permitId)) fail("action_permit_scope_mismatch");
          return;
        }
        if (runGeneration !== expected || !credential || localRunner?.state().phase !== "running") fail("run_confirmation_required");
        await credential(); checked();
        if (runGeneration !== expected) fail("stopped");
      };
      localClient = clientFactory({ ...dependencies, recovery: localRecovery, taskId: current.fence.task_id, beforeEffect: runtimePermit });
      await localClient.bindFence(current.fence); checked();
      const taskSnapshot = await localClient.snapshot(); checked();
      const goal = taskSnapshot.active_goal?.owner_proof?.request?.content;
      let budgetRunId;
      const budget = {
        async initialize({ runId, fence }) { checked(); await localRecovery.initializeRun(runId, { fence }); checked(); budgetRunId = runId; },
        async reserve(candidate) {
          checked(); if (!budgetRunId) fail("run_budget_missing");
          const durable = await localRecovery.reserveModelCall(budgetRunId, candidate.inputBytes, candidate.maxOutputTokens); checked();
          const shared = await modelSession.reserveExecution(candidate); checked();
          return { durable, shared };
        },
        async settle({ reservation, usage }) {
          let error;
          try { await localRecovery.settleModelCall(budgetRunId, reservation.durable,
            usage ? { inputTokens: usage.prompt_tokens, outputTokens: usage.completion_tokens } : null); } catch (failure) { error = failure; }
          try { await modelSession.settleExecution({ reservation: reservation.shared, usage }); } catch (failure) { error ??= failure; }
          if (error) throw error;
        },
      };
      localRunner = runnerFactory({ gameClient: localClient, recovery: localRecovery, budget,
        getCredential: async () => { checked(); if (runGeneration !== expected || !credential) fail("stopped"); const key = await credential(); checked(); return key; },
        visibility: () => documentRef?.visibilityState === "visible" && isCurrent(), online: () => navigatorRef?.onLine !== false,
        onState: (next) => { if (generation === expected && !controller.signal.aborted) publish(next); } });
      checked(); client = localClient; recovery = localRecovery; runner = localRunner;
      publish({ phase: "ready", ready: true, code: null, fence: current.fence,
        goal: typeof goal === "string" ? goal : typeof goal?.goal_text === "string" ? goal.goal_text : JSON.stringify(goal ?? null) });
      return { ...status };
    } catch (error) {
      controller.abort();
      if (generation === expected) { const diagnostic = browserGameDiagnostic(error, "executor_prepare"); publish({ phase: "blocked", ready: false, code: diagnostic.code, diagnostic }); }
      throw error;
    } finally { if (expected === generation) preparing = false; }
  }
  async function start({ delegation } = {}) {
    lastWall = now(); assertCurrent();
    const modelState = modelSession.snapshot();
    if (!status.ready || !runner || !modelState.hasKey || modelState.checked !== true || operationController?.signal.aborted) fail("run_confirmation_required");
    if (starting === generation || runner.state().phase === "running") fail("already_running");
    if (typeof locks?.request !== "function") fail("locks_unavailable");
    const expected = generation, confirmedFence = status.fence, controller = operationController;
    const currentClient = client, currentRunner = runner;
    starting = expected;
    try {
      return await locks.request(`oasis7:browser-agent:${JSON.stringify([config.world.world_id, config.world.genesis_digest, accountId, agentId])}`,
        { mode: "exclusive", ifAvailable: true }, async (lock) => {
          if (!lock) fail("agent_busy");
          check(expected, controller);
          await legacyGuard({ signal: controller.signal }); check(expected, controller);
          const current = await currentClient.freshFence(); check(expected, controller);
          if (!same(current.fence, confirmedFence)) fail("task_fence_changed");
          const currentModel = modelSession.snapshot();
          if (!currentModel.hasKey || currentModel.checked !== true) fail("run_confirmation_required");
          const permission = await modelSession.beginExecution(currentModel.revision); check(expected, controller);
          credential = permission; credentialRevision = modelSession.snapshot().revision; runGeneration = expected;
          const runId = codec.requestDigest("browser-agent-run", JSON.stringify({ fence: confirmedFence, executor: currentClient.executorPublicKey }));
          return currentRunner.start({ confirmedFence, runId, ...(delegation ? { delegation } : {}) });
        });
    } finally {
      if (starting === expected) starting = null;
      if (generation === expected) { credential = null; credentialRevision = undefined; runGeneration = null; await modelSession.stop(); }
    }
  }
  async function playerOperation(mode, options = {}) {
    lastWall = now(); assertCurrent();
    if (!status.ready || !client || !recovery || operationController?.signal.aborted) fail("run_confirmation_required");
    if (starting === generation || runner?.state().phase === "running" || playerPermit) fail("already_running");
    if (!options.confirmedFence || !same(options.confirmedFence, status.fence)) fail("task_fence_changed");
    if (typeof locks?.request !== "function") fail("locks_unavailable");
    const expected = generation, controller = operationController;
    starting = expected;
    try {
      return await locks.request(`oasis7:browser-agent:${JSON.stringify([config.world.world_id, config.world.genesis_digest, accountId, agentId])}`,
        { mode: "exclusive", ifAvailable: true }, async (lock) => {
          if (!lock) fail("agent_busy");
          check(expected, controller); await legacyGuard({ signal: controller.signal }); check(expected, controller);
          const operationId = mode === "preview" ? `preview:${options.permitId}` : mode === "grant" ? `grant:${options.permitId}` : options.contextOperationId ?? `direct:${options.permitId}`;
          playerPermit = { generation: expected, mode, permitId: options.permitId, operationId };
          const dependencies = { ...options, gameClient: client, recovery, check: () => check(expected, controller),
            onContextOperation: (nextOperationId) => { check(expected, controller); if (mode !== "direct" || playerPermit?.generation !== expected) fail("action_permit_scope_mismatch"); playerPermit.operationId = nextOperationId; },
            onState: (next) => { check(expected, controller); publish(next); } };
          if (mode === "grant") {
            const result = await client.nativeGrant(options); check(expected, controller);
            publish({ phase: "ready", code: null, nativeGrant: result.grant, agentDelegated: false }); return result;
          }
          if (mode === "end_preview") {
            const result = await endBrowserActionPreview(dependencies); check(expected, controller);
            publish({ phase: "ready", code: null, preview: null }); return result;
          }
          if (mode === "preview") {
            const preview = await readBrowserActionPreview(dependencies); check(expected, controller);
            publish({ phase: "preview_ready", code: null, preview, agentDelegated: false }); return preview;
          }
          return await executeBrowserDirectAction(dependencies);
        });
    } catch (error) {
      if (generation === expected) publish({ phase: "paused", code: typeof error?.code === "string" ? error.code : "game_operation_failed" });
      throw error;
    } finally {
      if (starting === expected) starting = null;
      if (playerPermit?.generation === expected) playerPermit = null;
    }
  }
  return {
    modelSession,
    snapshot: () => ({ ...status }), subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
    prepare, start, stop,
    preview: (options) => playerOperation("preview", options),
    directAction: (options) => playerOperation("direct", options),
    nativeGrant: (options) => playerOperation("grant", options),
    endPreview: (options) => playerOperation("end_preview", options),
    async firstSessionView() {
      assertCurrent(); if (!client || operationController?.signal.aborted) fail("task_scope_required");
      const expected = generation, controller = operationController;
      const owner = await client.ownerView(); check(expected, controller);
      const world = status.ready ? await client.publicView() : null; check(expected, controller);
      const journal = await recovery.load(); check(expected, controller);
      return { owner, world, openPreviews: openBrowserPreviews(journal),
        actionOriginals: (journal.actionPermits ?? []).filter((permit) => permit.originalOperationId).map((permit) => {
          const original = journal.operations.find((entry) => entry.original.operationId === permit.originalOperationId);
          return { selection: permit.selection, mode: permit.mode, operationId: permit.originalOperationId,
            state: original?.state, modelInvoked: original?.original.runBudget.modelInvoked === true };
        }) };
    },
    async starterClaim(options) {
      assertCurrent(); if (!client || operationController?.signal.aborted || !status.ready) fail("task_scope_required");
      const expected = generation, controller = operationController;
      const action = await client.starterClaim(options); check(expected, controller); return action;
    },
    operationId(domain, value) { if (!codec) fail("game_protocol_unavailable"); return codec.requestDigest(domain, JSON.stringify(value)); },
    async nextPreviewPermitId(value) {
      assertCurrent(); if (!recovery || !codec) fail("task_scope_required");
      const expected = generation, controller = operationController;
      const journal = await recovery.load(); check(expected, controller);
      return codec.requestDigest("browser-player-preview", JSON.stringify({ fence: status.fence, selection: value,
        sequence: journal.operations.filter((entry) => entry.original.kind === "prepare" && entry.original.runBudget.previewPermitId).length }));
    },
    async recover() {
      assertCurrent(); if (!client || operationController?.signal.aborted) fail("task_scope_required");
      if (starting === generation || runner?.state().phase === "running") fail("already_running");
      const expected = generation, controller = operationController, originalClient = client, originalRecovery = recovery;
      await originalClient.recover(); check(expected, controller);
      const unknown = await originalRecovery.hasUnresolved(); check(expected, controller);
      publish({ phase: unknown ? "blocked" : "inspected", ready: false, code: unknown ? "recovery_required" : null, goal: null });
      return !unknown;
    },
    async forget() { stop(); return modelSession.forget(); },
    async logout() { stop(); return modelSession.logout(); },
    dispose() {
      disposed = true; stop(); unsubscribe(); modelSession.dispose(); clearInterval(clock); listeners.clear();
      documentRef?.removeEventListener("visibilitychange", pause); documentRef?.removeEventListener("freeze", wake); documentRef?.removeEventListener("resume", wake);
      windowRef?.removeEventListener("pagehide", wake); windowRef?.removeEventListener("offline", wake);
    },
  };
}
