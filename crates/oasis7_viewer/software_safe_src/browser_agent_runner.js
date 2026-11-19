import { callBrowserModel } from "./browser_model_provider.js";
import { BrowserGameError } from "./browser_game_client.js";
import { boundedFirstProductionSelection, validateBrowserActionSelection, assertFirstProductionTool, assertBrowserPermitAction, sameBrowserValue } from "./browser_game_permit.js";
import { openBrowserContexts } from "./browser_context_recovery.js";
const fail = (code) => { throw new BrowserGameError(code); };
const text = { type: "string", minLength: 1, maxLength: 256 };
const definition = (name, properties = {}, description = name) => ({ type: "function", function: { name, description, parameters: { type: "object", properties, required: Object.keys(properties), additionalProperties: false } } });
const buildSelection = { site: text, factory: text, kind: { type: "string", enum: ["factory.smelter.mk1", "factory.assembler.mk1"] } };
const scheduleSelection = { factory: text, recipe: text, batches: { type: "integer", minimum: 1, maximum: 4294967295 } };
export const BROWSER_AGENT_TOOLS = Object.freeze([
 definition("observe", {}, "Read the current authenticated Agent observation."),
 definition("industry", {}, "Read current authenticated industry state."),
 definition("capabilities", {}, "Read current native action grants; grants do not guarantee admission."),
 definition("quote_build", buildSelection), definition("build", buildSelection),
 definition("quote_schedule", scheduleSelection), definition("schedule", scheduleSelection),
 definition("move", { to: { type: "array", items: { type: "integer", minimum: -Number.MAX_SAFE_INTEGER, maximum: Number.MAX_SAFE_INTEGER }, minItems: 3, maxItems: 3 } }),
 definition("abort", {}, "Terminate the current Context with executor_aborted; no goal completion claim."),
]);
const names = new Set(BROWSER_AGENT_TOOLS.map((tool) => tool.function.name));
const effects = new Set(["build", "schedule", "move", "abort"]);
const same = sameBrowserValue;
const boundedNames = new Set(["observe", "industry", "capabilities", "quote_schedule", "schedule", "abort"]);
const boundedTools = (selection) => BROWSER_AGENT_TOOLS.filter((tool) => boundedNames.has(tool.function.name)).map((tool) =>
 ["schedule", "quote_schedule"].includes(tool.function.name)
  ? definition(tool.function.name, { factory: { type: "string", enum: [selection.factory] },
   recipe: { type: "string", enum: [selection.recipe] }, batches: { type: "integer", enum: [1] } }) : tool);
export function createBrowserAgentRunner({ gameClient, modelCall = callBrowserModel, getCredential, budget, recovery, visibility = () => !globalThis.document?.hidden, online = () => globalThis.navigator?.onLine !== false, now = Date.now, onState = () => {} } = {}) {
 if (!gameClient || !getCredential || (!budget && !recovery)) fail("runner_configuration_invalid");
 let status = { phase: "idle", code: null }, controller = null, generation = 0, active = false, starting = false;
 function publish(phase, code = null, extra = {}) { status = { phase, code, ...extra }; onState({ ...status }); }
 function assertActive(expected) {
  if (!active || expected !== generation || controller?.signal.aborted) fail("stopped");
  if (!visibility() || !online()) { stop("page_paused"); fail("page_paused"); }
 }
 function stop(code = "stopped") { active = false; generation += 1; controller?.abort(); publish("stopped", code); }
 async function recoverAt(expected) {
  if (expected !== generation) return null;
  publish("recovering"); await gameClient.recover();
  if (expected !== generation) return null;
  const unresolved = await gameClient.hasUnresolved();
  if (expected !== generation) return null;
  publish(unresolved ? "paused" : "idle", unresolved ? "recovery_required" : null); return !unresolved;
 }
 async function recover() { if (active || starting) fail("already_running"); starting = true; const expected = ++generation; try { return await recoverAt(expected); } finally { starting = false; } }
 const budgetFor = (runId) => budget ?? {
  initialize: ({ fence }) => recovery.initializeRun(runId, { fence }),
  reserve: ({ inputBytes, maxOutputTokens }) => recovery.reserveModelCall(runId, inputBytes, maxOutputTokens),
  settle: ({ reservation, usage }) => recovery.settleModelCall(runId, reservation, usage ? { inputTokens: usage.prompt_tokens, outputTokens: usage.completion_tokens } : null),
 };
 async function start({ confirmedFence, runId, operationId, delegation } = {}) {
  if (active || starting) fail("already_running");
  if (!confirmedFence || !runId || !visibility() || !online()) fail("run_confirmation_required");
  starting = true; const expected = ++generation, started = now(); controller = new AbortController();
  try {
  const selection = delegation ? (delegation.selection?.kind ? validateBrowserActionSelection(delegation.selection)
   : boundedFirstProductionSelection(delegation.selection, gameClient.agentId)) : null;
  if (selection && (selection.kind !== "first_production" || selection.agentId !== gameClient.agentId)) fail("action_permit_scope_mismatch");
  const previewOperation = delegation?.contextOperationId;
  if (previewOperation && (typeof previewOperation !== "string" || !previewOperation.startsWith("preview:") || previewOperation.length > 200)) fail("action_permit_invalid");
  if (selection && (!recovery?.initializeActionPermit || typeof delegation.permitId !== "string" || !delegation.permitId)) fail("action_permit_invalid");
  const permittedTools = selection ? boundedTools(selection) : BROWSER_AGENT_TOOLS;
  const permittedNames = selection ? boundedNames : names;
  let modelNote = null;
  const publishOriginal = async (permit) => {
   const entry = (await recovery.load()).operations.find((item) => item.original.operationId === permit.originalOperationId);
   if (expected !== generation) return { ...status };
   if (!entry) fail("action_permit_corrupt");
   const state = entry.state, settled = ["committed", "rejected", "expired"].includes(state);
   active = false;
   publish(settled ? "settled" : "paused", settled ? `original_action_${state}` : "original_result_requires_recovery",
    { originalOperationId: permit.originalOperationId, submissionState: state, agentDelegated: entry.original.runBudget.modelInvoked === true, modelNote });
   return { ...status };
  };
  // Recovery is read-only and strictly precedes new contexts, model calls and effects.
  const recovered = await recoverAt(expected);
  if (expected !== generation) return { ...status };
  const current = await gameClient.freshFence();
  if (expected !== generation) return { ...status };
  if (!same(current.fence, confirmedFence)) fail("task_fence_changed");
  await gameClient.bindFence(confirmedFence);
  if (expected !== generation) return { ...status };
  let reusablePreview = false;
  if (selection) {
   const journal = await recovery.load();
   const prior = journal.actionPermits?.find((item) => item.permitId === delegation.permitId);
   if (expected !== generation) return { ...status };
   if (prior?.originalOperationId) {
    const permit = await recovery.initializeActionPermit(delegation.permitId, { fence: confirmedFence, selection, mode: "agent", runId });
    if (expected !== generation) return { ...status };
    return await publishOriginal(permit);
   }
   if (previewOperation) {
    const expectedRequest = { kind: "schedule", selection: { factory: selection.factory, recipe: selection.recipe, batches: 1 } };
    const own = journal.operations.filter((entry) => entry.original.operationId.startsWith(`${previewOperation}:`));
    if (!own.length || own.some((entry) => !["prepare", "prefix"].includes(entry.original.kind)
     || !same(entry.original.runBudget?.previewRequest, expectedRequest))) fail("action_permit_scope_mismatch");
    const open = openBrowserContexts(journal.operations);
    reusablePreview = open.length === 1 && open[0].original.operationId === `${previewOperation}:prefix`
     && !journal.operations.some((entry) => !["committed", "rejected", "expired"].includes(entry.state));
    if (!reusablePreview) fail("recovery_required");
   }
  }
  active = true;
  // Explicit Start also authorizes the bounded original-Context retirement
  // lane. Read-only recover() never enters this phase or acquires this permit.
  publish("running");
  if (!recovered && !reusablePreview) {
   if (!gameClient.retireExpiredContexts) { active = false; publish("paused", "recovery_required"); return { ...status }; }
   try { await gameClient.retireExpiredContexts(); }
   catch (error) { if (expected === generation) { active = false; controller.abort(); publish("paused", typeof error?.code === "string" ? error.code : "recovery_required"); } return { ...status }; }
   if (!active || expected !== generation) return { ...status };
   const unresolved = await gameClient.hasUnresolved(); assertActive(expected);
   if (unresolved) { active = false; controller.abort(); publish("paused", "recovery_required"); return { ...status }; }
  }
  const limits = budgetFor(runId);
  try { assertActive(expected); await limits.initialize({ runId, fence: confirmedFence }); assertActive(expected); }
  catch (error) { if (expected === generation) { active = false; controller.abort(); publish("paused", typeof error?.code === "string" ? error.code : "run_failed"); } return { ...status }; }
  let captured = null, currentOperation = null, step = 0;
  try {
  if (selection) {
   const permit = await recovery.initializeActionPermit(delegation.permitId, { fence: confirmedFence, selection, mode: "agent", runId });
   assertActive(expected);
   if (permit.originalOperationId) return await publishOriginal(permit);
  }
  if (recovery?.load) {
   const prefix = operationId ? `${operationId}:` : `${runId}:context:`;
   for (const entry of (await recovery.load()).operations) {
    const id = entry.original.operationId;
    if (id.startsWith(prefix)) { const prior = Number(id.slice(prefix.length).split(":")[0]); if (Number.isSafeInteger(prior) && prior >= step) step = prior + 1; }
   }
  }
  assertActive(expected);
   while (active) {
    assertActive(expected);
    if (now() - started >= 120000) fail("run_time_limit");
    await gameClient.freshFence(); assertActive(expected);
    currentOperation = previewOperation ?? (operationId ? `${operationId}:${step}` : `${runId}:context:${step}`); step += 1;
    let prepared;
    if (previewOperation) {
     captured = await gameClient.context(currentOperation, { runId }); assertActive(expected); prepared = captured.prepared;
    } else {
     prepared = await gameClient.prepareContext(currentOperation, { runId }); assertActive(expected);
     captured = await gameClient.captureContext(currentOperation, prepared, { runId }); assertActive(expected);
    }
    const checkContext = async () => {
     const control = await gameClient.freshFence(); assertActive(expected);
     const until = prepared.valid_until_tick ?? prepared.identity.valid_until_tick;
     if (until !== undefined && control.logicalTick !== undefined && BigInt(control.logicalTick) > BigInt(until)) fail("context_expired");
    };
    const context = captured.current_public_context;
    const messages = [{ role: "system", content: "You control only the current authorized task. Use the supplied native task tools. Model text is not a world effect or proof of goal completion. Choose one effect per Context. Never invent grants or receipts. Unsupported actions require abort." + (selection ? ` The player permits exactly one ScheduleRecipe original for ${JSON.stringify(selection)}. Explain the current limiting resource and the signed quote before choosing schedule. You cannot build, move, change recipe, batches, factory or goal. A committed request is not a settled production job.` : "") },
     { role: "user", content: gameClient.modelContext ? gameClient.modelContext(captured) : JSON.stringify({ goal: prepared.snapshot.active_goal.owner_proof.request.content, context_identity: prepared.identity, task_fence: confirmedFence, actor_observation: context.actor_observation, native_authority: context.native_authority, capability_catalog: context.provider_authority?.capability_catalog }) }];
    let completed = false;
    while (!completed) {
     assertActive(expected); await checkContext();
     const response = await modelCall({ messages, tools: permittedTools, maxOutputTokens: 512, signal: controller.signal,
      beforeSend: async (candidate) => { assertActive(expected); await checkContext(); return limits.reserve(candidate); },
      getCredential: async (reservation) => { assertActive(expected); const key = await getCredential(reservation); assertActive(expected); return key; },
      afterResponse: (result) => limits.settle(result),
     });
     assertActive(expected); await checkContext();
     const message = response.choices?.[0]?.message, calls = message?.tool_calls;
     if (!Array.isArray(calls) || !calls.length) fail("model_tool_required");
     if (selection && typeof message.content === "string" && message.content.trim()) {
      modelNote = message.content.slice(0, 2048); publish("running", null, { modelNote, agentDelegated: true });
     }
     let effectCount = 0;
     for (let i = 0; i < calls.length; i += 1) {
      const name = calls[i].function?.name;
      if (!permittedNames.has(name) || calls[i].type !== "function") fail(selection ? "action_permit_scope_mismatch" : "unsupported_tool");
      if (effects.has(name) && (++effectCount > 1 || i !== calls.length - 1)) fail("effect_batch_invalid");
     }
     messages.push(message);
     for (const call of calls) {
      assertActive(expected); await checkContext();
      let args; try { args = JSON.parse(call.function.arguments); } catch { fail("tool_invalid"); }
      const name = call.function.name; let value;
      if (selection) assertFirstProductionTool(selection, name, args);
      if (name === "observe" || name === "industry" || name === "capabilities") {
       const view = await gameClient.publicView();
       if (selection) {
        const projection = view.view?.observation ?? view.view, world = projection?.state;
        const factory = world?.factories?.[selection.factory];
        const info = { read_version: view.version, logical_tick: view.logical_tick };
        value = name === "observe" ? { ...info, agent: world?.agents?.[gameClient.agentId] }
         : name === "industry" ? { ...info, factory, agent_materials: world?.material_ledgers?.[`agent:${gameClient.agentId}`],
          input_materials: factory ? world?.material_ledgers?.[factory.input_ledger] : null }
         : { ...info, native_delegation_authorizations: projection?.native_delegation_authorizations?.filter((item) =>
          item.grant?.agent_id === gameClient.agentId && item.grant.object_id === selection.factory && item.grant.action_kinds?.includes("ScheduleRecipe")), task_control: projection?.game_task_control };
       } else value = name === "observe" ? view : name === "industry" ? { read_version: view.version, logical_tick: view.logical_tick, industry: view.view?.state ?? view.view?.observation?.state } : { read_version: view.version, logical_tick: view.logical_tick, native_delegation_authorizations: view.view?.native_delegation_authorizations ?? view.view?.observation?.native_delegation_authorizations, task_control: view.view?.game_task_control ?? view.view?.observation?.game_task_control };
      } else if (["quote_build", "quote_schedule", "build", "schedule"].includes(name)) {
       const kind = name.includes("build") ? "build" : "schedule";
       const result = await gameClient.quote(currentOperation, captured, kind, args); assertActive(expected);
       value = result.quoted;
       if (name === "build" || name === "schedule") {
        if (!result.quoted.quote || result.quoted.quote.rejection) fail("quote_rejected");
        if (selection) assertBrowserPermitAction(selection, result.quoted.quote.action, result.quoted.quote.preview);
        assertActive(expected); value = await gameClient.action(currentOperation, result.captured, result.quoted.quote.action,
         selection ? { runId, permitId: delegation.permitId, modelInvoked: true, quotePreview: result.quoted.quote.preview } : { runId }); completed = true;
       }
      } else if (name === "move") {
       if (!Array.isArray(args.to) || args.to.length !== 3 || !args.to.every(Number.isSafeInteger)) fail("tool_invalid");
       captured = await gameClient.readContext(currentOperation, captured.prepared.identity); assertActive(expected);
       value = await gameClient.action(currentOperation, captured, { type: "MoveAgent", data: { agent_id: gameClient.agentId, to: { x_cm: args.to[0], y_cm: args.to[1], z_cm: args.to[2] } } }, { runId }); completed = true;
      } else if (name === "abort") { value = await gameClient.outcome(currentOperation, captured, "executor_aborted", { runId }); completed = true; }
      assertActive(expected);
      messages.push({ role: "tool", tool_call_id: call.id, content: JSON.stringify(value) });
      if (effects.has(name)) {
       if (selection && name === "schedule") {
        const permit = await recovery.getActionPermit(delegation.permitId); assertActive(expected);
        return await publishOriginal(permit);
       }
       if (value?.outcome?.status !== "committed") { publish("paused", "original_result_requires_recovery"); active = false; return { ...status }; }
       if (name === "abort") { publish("stopped", "executor_aborted"); active = false; return { ...status }; }
      }
     }
    }
   }
  } catch (error) {
   active = false;
   if (expected === generation) { generation += 1; controller.abort(); publish("paused", error instanceof BrowserGameError || typeof error?.code === "string" ? error.code : "run_failed"); }
   return { ...status };
  }
  return { ...status };
  } catch (error) {
   if (expected === generation) { active = false; controller.abort(); publish("paused", typeof error?.code === "string" ? error.code : "run_failed"); }
   return { ...status };
  } finally { starting = false; }
 }
 return { start, stop, recover, state: () => ({ ...status }) };
}
