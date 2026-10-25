import { callBrowserModel } from "./browser_model_provider.js";
import { BrowserGameError } from "./browser_game_client.js";
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
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
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
 async function start({ confirmedFence, runId, operationId } = {}) {
  if (active || starting) fail("already_running");
  if (!confirmedFence || !runId || !visibility() || !online()) fail("run_confirmation_required");
  starting = true; const expected = ++generation, started = now(); controller = new AbortController();
  try {
  // Recovery is read-only and strictly precedes new contexts, model calls and effects.
  const recovered = await recoverAt(expected);
  if (expected !== generation) return { ...status };
  const current = await gameClient.freshFence();
  if (expected !== generation) return { ...status };
  if (!same(current.fence, confirmedFence)) fail("task_fence_changed");
  await gameClient.bindFence(confirmedFence);
  if (expected !== generation) return { ...status };
  active = true;
  // Explicit Start also authorizes the bounded original-Context retirement
  // lane. Read-only recover() never enters this phase or acquires this permit.
  publish("running");
  if (!recovered) {
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
    currentOperation = operationId ? `${operationId}:${step}` : `${runId}:context:${step}`; step += 1;
    const prepared = await gameClient.prepareContext(currentOperation, { runId }); assertActive(expected);
    captured = await gameClient.captureContext(currentOperation, prepared, { runId }); assertActive(expected);
    const checkContext = async () => {
     const control = await gameClient.freshFence(); assertActive(expected);
     if (prepared.valid_until_tick !== undefined && control.logicalTick !== undefined && BigInt(control.logicalTick) > BigInt(prepared.valid_until_tick)) fail("context_expired");
    };
    const context = captured.current_public_context;
    const messages = [{ role: "system", content: "You control only the current authorized task. Use the supplied native task tools. Model text is not a world effect or proof of goal completion. Choose one effect per Context. Never invent grants or receipts. Unsupported actions require abort." },
     { role: "user", content: gameClient.modelContext ? gameClient.modelContext(captured) : JSON.stringify({ goal: prepared.snapshot.active_goal.owner_proof.request.content, context_identity: prepared.identity, task_fence: confirmedFence, actor_observation: context.actor_observation, native_authority: context.native_authority, capability_catalog: context.provider_authority?.capability_catalog }) }];
    let completed = false;
    while (!completed) {
     assertActive(expected); await checkContext();
     const response = await modelCall({ messages, tools: BROWSER_AGENT_TOOLS, maxOutputTokens: 512, signal: controller.signal,
      beforeSend: async (candidate) => { assertActive(expected); await checkContext(); return limits.reserve(candidate); },
      getCredential: async (reservation) => { assertActive(expected); const key = await getCredential(reservation); assertActive(expected); return key; },
      afterResponse: (result) => limits.settle(result),
     });
     assertActive(expected); await checkContext();
     const message = response.choices?.[0]?.message, calls = message?.tool_calls;
     if (!Array.isArray(calls) || !calls.length) fail("model_tool_required");
     let effectCount = 0;
     for (let i = 0; i < calls.length; i += 1) {
      const name = calls[i].function?.name;
      if (!names.has(name) || calls[i].type !== "function") fail("unsupported_tool");
      if (effects.has(name) && (++effectCount > 1 || i !== calls.length - 1)) fail("effect_batch_invalid");
     }
     messages.push(message);
     for (const call of calls) {
      assertActive(expected); await checkContext();
      let args; try { args = JSON.parse(call.function.arguments); } catch { fail("tool_invalid"); }
      const name = call.function.name; let value;
      if (name === "observe" || name === "industry" || name === "capabilities") {
       const view = await gameClient.publicView();
       value = name === "observe" ? view : name === "industry" ? { read_version: view.version, logical_tick: view.logical_tick, industry: view.view?.state ?? view.view?.observation?.state } : { read_version: view.version, logical_tick: view.logical_tick, native_delegation_authorizations: view.view?.native_delegation_authorizations ?? view.view?.observation?.native_delegation_authorizations, task_control: view.view?.game_task_control ?? view.view?.observation?.game_task_control };
      } else if (["quote_build", "quote_schedule", "build", "schedule"].includes(name)) {
       const kind = name.includes("build") ? "build" : "schedule";
       const result = await gameClient.quote(currentOperation, captured, kind, args); assertActive(expected);
       value = result.quoted;
       if (name === "build" || name === "schedule") {
        if (!result.quoted.quote || result.quoted.quote.rejection) fail("quote_rejected");
        assertActive(expected); value = await gameClient.action(currentOperation, result.captured, result.quoted.quote.action, { runId }); completed = true;
       }
      } else if (name === "move") {
       if (!Array.isArray(args.to) || args.to.length !== 3 || !args.to.every(Number.isSafeInteger)) fail("tool_invalid");
       captured = await gameClient.readContext(currentOperation, captured.prepared.identity); assertActive(expected);
       value = await gameClient.action(currentOperation, captured, { type: "MoveAgent", data: { agent_id: gameClient.agentId, to: { x_cm: args.to[0], y_cm: args.to[1], z_cm: args.to[2] } } }, { runId }); completed = true;
      } else if (name === "abort") { value = await gameClient.outcome(currentOperation, captured, "executor_aborted", { runId }); completed = true; }
      assertActive(expected);
      messages.push({ role: "tool", tool_call_id: call.id, content: JSON.stringify(value) });
      if (effects.has(name)) {
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
