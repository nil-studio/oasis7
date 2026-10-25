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
describe("browser Agent native-task runner", () => {
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
