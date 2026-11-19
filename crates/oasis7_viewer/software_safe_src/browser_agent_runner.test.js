import { describe, expect, it, vi } from "vitest";
import { BROWSER_AGENT_TOOLS, createBrowserAgentRunner } from "./browser_agent_runner.js";
const fence = { task_id: "task", agent_id: "agent", task_status: "active", executor_binding: { executor_public_key: "executor" } };
const response = (name, args = {}) => ({ choices: [{ message: { role: "assistant", content: null, tool_calls: [{ id: "tool-1", type: "function", function: { name, arguments: JSON.stringify(args) } }] } }] });
function fixture(turns = [response("abort")], recovery) {
 const order = [], prepared = { identity: { fence }, snapshot: { active_goal: { owner_proof: { request: { content: "goal" } } } } };
 const captured = { prepared, current_public_context: { actor_observation: {}, native_authority: {} } };
 let unresolved = false, visible = true;
 const gameClient = {
  agentId: "agent", recover: vi.fn(async () => order.push("recover")), hasUnresolved: vi.fn(async () => unresolved),
  freshFence: vi.fn(async () => ({ fence })), bindFence: vi.fn(async () => {}),
  prepareContext: vi.fn(async () => { order.push("prepare"); return prepared; }), captureContext: vi.fn(async () => { order.push("capture"); return captured; }),
  readContext: vi.fn(async () => captured), publicView: vi.fn(async () => ({ view: { state: {} } })),
  quote: vi.fn(async () => ({ captured, quoted: { quote: { action: { type: "ScheduleRecipe" }, rejection: null } } })),
  action: vi.fn(async () => { order.push("action"); return { outcome: { status: "committed" } }; }),
  outcome: vi.fn(async () => { order.push("outcome"); return { outcome: { status: "committed" } }; }),
 };
 const budget = { initialize: vi.fn(async () => order.push("initialize_budget")), reserve: vi.fn(async () => { order.push("reserve"); return "reservation"; }), settle: vi.fn(async () => order.push("settle")) };
 const getCredential = vi.fn(async () => { order.push("credential"); return "fixture-key"; });
 const modelCall = vi.fn(async (options) => {
  const reservation = await options.beforeSend({ inputBytes: 100, maxOutputTokens: 512 }); await options.getCredential(reservation);
  order.push("model"); await options.afterResponse({ reservation, usage: null, status: "completed" });
  return turns.shift() ?? response("abort");
 });
 const runner = createBrowserAgentRunner({ gameClient, budget, recovery, getCredential, modelCall, visibility: () => visible });
 return { runner, gameClient, budget, modelCall, order, getCredential, setUnresolved: (value) => { unresolved = value; }, hide: () => { visible = false; } };
}
const selectedRecipe = { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 };
const oneBatch = { ...selectedRecipe, maxElectricity: 8,
 maxConsume: [{ kind: "iron_ore", amount: 4 }, { kind: "carbon_fuel", amount: 1 }, { kind: "maintenance_part", amount: 1 }] };
function boundedFixture(turns, actionState = "committed") {
 const journal = { actionPermits: [], operations: [] };
 const recovery = {
  load: vi.fn(async () => structuredClone(journal)),
  initializeActionPermit: vi.fn(async (permitId, value) => {
   const prior = journal.actionPermits.find((item) => item.permitId === permitId);
   if (prior) return structuredClone(prior);
   const permit = { permitId, ...value, originalOperationId: null }; journal.actionPermits.push(permit); return structuredClone(permit);
  }),
  getActionPermit: vi.fn(async (id) => structuredClone(journal.actionPermits.find((item) => item.permitId === id))),
 };
 const f = fixture(turns, recovery);
 const captured = { prepared: { identity: { fence }, snapshot: { active_goal: { owner_proof: { request: { content: "goal" } } } } }, current_public_context: {} };
 const action = { type: "ScheduleRecipe", data: { requester_agent_id: "agent", factory_id: "factory", recipe_id: oneBatch.recipe,
  plan: { accepted_batches: 1, power_required: 8, consume: [{ kind: "iron_ore", amount: 4 }, { kind: "carbon_fuel", amount: 1 }] }, logistics_route_ids: [], logistics_path_ids: [] } };
 f.gameClient.quote.mockResolvedValue({ captured, quoted: { quote: { action, rejection: null,
  preview: { power_required: 8, consume: oneBatch.maxConsume } } } });
 f.gameClient.action.mockImplementation(async (operation, _context, _action, runBudget) => {
  const permit = journal.actionPermits.find((item) => item.permitId === runBudget.permitId);
  permit.originalOperationId = `${operation}:action`;
  journal.operations.push({ original: { operationId: permit.originalOperationId, kind: "action", runBudget }, state: actionState });
  return { outcome: { status: actionState } };
 });
 f.gameClient.context = vi.fn(async () => captured);
 return { ...f, recovery, journal, action, captured, run: { runId: "run", confirmedFence: fence,
  delegation: { permitId: "first-production", selection: oneBatch } } };
}
describe("browser Agent native-task runner", () => {
 it.each(["committed", "rejected", "pending", "unknown"])("stops the bounded fixture after one %s original and only recovers it on restart", async (state) => {
  const f = boundedFixture([response("schedule", selectedRecipe), response("schedule", selectedRecipe)], state);
  const first = await f.runner.start(f.run);
  expect(first).toMatchObject({ submissionState: state, agentDelegated: true });
  expect(first.phase).toBe(["committed", "rejected"].includes(state) ? "settled" : "paused");
  expect(f.gameClient.prepareContext).toHaveBeenCalledTimes(1); expect(f.modelCall).toHaveBeenCalledTimes(1);
  expect(f.modelCall.mock.calls[0][0].tools.map((tool) => tool.function.name))
   .toEqual(["observe", "industry", "capabilities", "quote_schedule", "schedule", "abort"]);
  await f.runner.start(f.run);
  expect(f.gameClient.action).toHaveBeenCalledTimes(1); expect(f.modelCall).toHaveBeenCalledTimes(1);
  expect(f.budget.initialize).toHaveBeenCalledTimes(1);
  expect(f.gameClient.action.mock.calls[0][3]).toMatchObject({ runId: "run", permitId: "first-production", modelInvoked: true });
 });
 it.each([
  ["build", { factory: "factory", site: "site", kind: "factory.smelter.mk1" }],
  ["move", { to: [1, 2, 3] }],
  ["schedule", { ...selectedRecipe, recipe: "recipe.smelter.copper_wire" }],
  ["schedule", { ...selectedRecipe, factory: "different-factory" }],
  ["schedule", { ...selectedRecipe, batches: 2 }],
  ["schedule", { ...selectedRecipe, unexpected: true }],
 ])("rejects out-of-scope model tool %s before quoting or signing an action", async (tool, args) => {
  const f = boundedFixture([response(tool, args)]);
  expect(await f.runner.start(f.run)).toMatchObject({ phase: "paused", code: "action_permit_scope_mismatch" });
  expect(f.gameClient.action).not.toHaveBeenCalled(); expect(f.gameClient.quote).not.toHaveBeenCalled();
 });
 it("rejects a quote whose action escapes the exact recipe or full confirmed consumption", async () => {
  for (const mutate of [
   (value) => { value.quote.action.data.recipe_id = "recipe.smelter.copper_wire"; },
   (value) => { value.quote.action.data.plan.accepted_batches = 2; },
   (value) => { value.quote.preview.consume.push({ kind: "unconfirmed_sink", amount: 1 }); },
  ]) {
   const f = boundedFixture([response("schedule", selectedRecipe)]);
   const value = structuredClone(await f.gameClient.quote()); mutate(value.quoted); f.gameClient.quote.mockResolvedValue(value);
   expect(await f.runner.start(f.run)).toMatchObject({ code: "action_permit_scope_mismatch" });
   expect(f.gameClient.action).not.toHaveBeenCalled();
  }
 });
 it("reuses the player-confirmed preview Context without retiring it or silently capturing another", async () => {
  const f = boundedFixture([response("schedule", selectedRecipe)]);
  f.run.delegation.contextOperationId = "preview:single";
  f.journal.operations.push({ original: { operationId: "preview:single:prefix", kind: "prefix",
   runBudget: { previewRequest: { kind: "schedule", selection: selectedRecipe } } }, state: "committed" });
  f.setUnresolved(true); f.gameClient.retireExpiredContexts = vi.fn();
  expect(await f.runner.start(f.run)).toMatchObject({ phase: "settled", submissionState: "committed" });
  expect(f.gameClient.context).toHaveBeenCalledWith("preview:single", { runId: "run" });
  expect(f.gameClient.prepareContext).not.toHaveBeenCalled(); expect(f.gameClient.captureContext).not.toHaveBeenCalled();
  expect(f.gameClient.retireExpiredContexts).not.toHaveBeenCalled();
 });
 it("requires a new explicit confirmation after preview Context expiry", async () => {
  const f = boundedFixture([response("schedule", selectedRecipe)]);
  f.run.delegation.contextOperationId = "preview:single";
  f.journal.operations.push({ original: { operationId: "preview:single:prefix", kind: "prefix", runBudget: { previewRequest: { kind: "schedule", selection: selectedRecipe } } }, state: "committed" });
  f.captured.prepared.valid_until_tick = "10"; f.gameClient.freshFence.mockResolvedValue({ fence, logicalTick: "11" });
  expect(await f.runner.start(f.run)).toMatchObject({ code: "context_expired" });
  expect(f.gameClient.action).not.toHaveBeenCalled(); expect(f.modelCall).not.toHaveBeenCalled();
  expect(f.gameClient.prepareContext).not.toHaveBeenCalled();
 });
 it("recovers first, captures one legal Context and records a typed abort outcome", async () => {
  const f = fixture();
  expect(await f.runner.start({ runId: "run", confirmedFence: fence })).toMatchObject({ phase: "stopped", code: "executor_aborted" });
  expect(f.order).toEqual(["recover", "initialize_budget", "prepare", "capture", "reserve", "credential", "model", "settle", "outcome"]);
  expect(f.gameClient.outcome).toHaveBeenCalledWith("run:context:0", expect.any(Object), "executor_aborted", { runId: "run" });
  expect(f.gameClient.action).not.toHaveBeenCalled();
 });
 it("does not call a model or make a Context while original results are unknown", async () => {
  const f = fixture(); f.setUnresolved(true);
  expect(await f.runner.start({ runId: "run", confirmedFence: fence })).toMatchObject({ phase: "paused", code: "recovery_required" });
  expect(f.modelCall).not.toHaveBeenCalled(); expect(f.gameClient.prepareContext).not.toHaveBeenCalled(); expect(f.budget.initialize).not.toHaveBeenCalled();
 });
 it("only retires expired originals after explicit fence confirmation and before a new Context", async () => {
  const f = fixture(); f.setUnresolved(true);
  f.gameClient.retireExpiredContexts = vi.fn(async () => { f.order.push("retire_expired"); f.setUnresolved(false); });
  await f.runner.start({ runId: "run", confirmedFence: fence });
  expect(f.order.indexOf("recover")).toBeLessThan(f.order.indexOf("retire_expired"));
  expect(f.order.indexOf("retire_expired")).toBeLessThan(f.order.indexOf("initialize_budget"));
  expect(f.order.indexOf("retire_expired")).toBeLessThan(f.order.indexOf("prepare"));
  expect(f.gameClient.bindFence).toHaveBeenCalledWith(fence);
 });
 it("does not retire Contexts during read-only recovery", async () => {
  const f = fixture(); f.setUnresolved(true); f.gameClient.retireExpiredContexts = vi.fn();
  expect(await f.runner.recover()).toBe(false); expect(f.gameClient.retireExpiredContexts).not.toHaveBeenCalled();
  expect(f.gameClient.prepareContext).not.toHaveBeenCalled(); expect(f.modelCall).not.toHaveBeenCalled();
 });
 it("uses signed recipe quote action, then only advances after a committed exact result", async () => {
  const f = fixture([response("schedule", { factory: "factory", recipe: "recipe", batches: 1 }), response("abort")]);
  await f.runner.start({ runId: "run", confirmedFence: fence });
  expect(f.gameClient.quote).toHaveBeenCalledWith("run:context:0", expect.any(Object), "schedule", { factory: "factory", recipe: "recipe", batches: 1 });
  expect(f.gameClient.action.mock.calls[0][2]).toEqual({ type: "ScheduleRecipe" });
  expect(f.gameClient.prepareContext).toHaveBeenCalledTimes(2);
 });
 it("pauses pending effects without submitting another Context or model call", async () => {
  const f = fixture([response("move", { to: [1, 2, 3] })]);
  f.gameClient.action.mockResolvedValue({ outcome: { status: "pending" } });
  expect(await f.runner.start({ runId: "run", confirmedFence: fence })).toMatchObject({ phase: "paused", code: "original_result_requires_recovery" });
  expect(f.gameClient.action.mock.calls[0][2]).toEqual({ type: "MoveAgent", data: { agent_id: "agent", to: { x_cm: 1, y_cm: 2, z_cm: 3 } } });
  expect(f.modelCall).toHaveBeenCalledTimes(1); expect(f.gameClient.prepareContext).toHaveBeenCalledTimes(1);
 });
 it("discards late model results after Stop", async () => {
  const f = fixture(); let resolve;
  f.modelCall.mockImplementation(() => new Promise((done) => { resolve = done; }));
  const started = f.runner.start({ runId: "run", confirmedFence: fence });
  await vi.waitFor(() => expect(resolve).toBeTypeOf("function"));
  f.runner.stop(); resolve(response("move", { to: [0, 0, 0] })); await started;
  expect(f.gameClient.action).not.toHaveBeenCalled(); expect(f.gameClient.outcome).not.toHaveBeenCalled();
  expect(f.runner.state()).toMatchObject({ phase: "stopped" });
 });
 it.each(["recover", "freshFence", "bindFence"])("does not reactivate when Stop occurs during start %s", async (stage) => {
  const f = fixture(); let release;
  f.gameClient[stage].mockImplementationOnce(() => new Promise((resolve) => { release = resolve; }));
  const starting = f.runner.start({ runId: "run", confirmedFence: fence });
  await vi.waitFor(() => expect(release).toBeTypeOf("function"));
  f.runner.stop(); release(stage === "freshFence" ? { fence } : undefined); await starting;
  expect(f.runner.state()).toEqual({ phase: "stopped", code: "stopped" });
  expect(f.budget.initialize).not.toHaveBeenCalled(); expect(f.modelCall).not.toHaveBeenCalled();
  expect(f.gameClient.prepareContext).not.toHaveBeenCalled(); expect(f.gameClient.action).not.toHaveBeenCalled();
 });
 it("cleans up a run when recovery.load fails after activation", async () => {
  const f = fixture(undefined, { load: async () => { throw new Error("disk unavailable"); } });
  expect(await f.runner.start({ runId: "run", confirmedFence: fence })).toMatchObject({ phase: "paused", code: "run_failed" });
  expect(f.gameClient.prepareContext).not.toHaveBeenCalled(); expect(f.modelCall).not.toHaveBeenCalled();
  await expect(f.runner.recover()).resolves.toBe(true);
 });
 it("pauses when page visibility changes before the model result", async () => {
  const f = fixture(); f.modelCall.mockImplementation(async () => { f.hide(); return response("move", { to: [1, 2, 3] }); });
  await f.runner.start({ runId: "run", confirmedFence: fence });
  expect(f.gameClient.action).not.toHaveBeenCalled(); expect(f.runner.state().code).toBe("page_paused");
 });
 it("does not advertise unsupported or privileged tools", () => {
  expect(BROWSER_AGENT_TOOLS.map((item) => item.function.name)).toEqual(["observe", "industry", "capabilities", "quote_build", "build", "quote_schedule", "schedule", "move", "abort"]);
 });
 it("rejects multiple effects in a model batch before any world effect", async () => {
  const turn = response("move", { to: [1, 2, 3] }); turn.choices[0].message.tool_calls.push(response("abort").choices[0].message.tool_calls[0]);
  const f = fixture([turn]);
  expect(await f.runner.start({ runId: "run", confirmedFence: fence })).toMatchObject({ phase: "paused", code: "effect_batch_invalid" });
  expect(f.gameClient.action).not.toHaveBeenCalled(); expect(f.gameClient.outcome).not.toHaveBeenCalled();
 });
});
