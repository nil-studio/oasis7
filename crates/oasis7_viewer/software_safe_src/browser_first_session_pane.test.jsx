import { createSignal } from "solid-js";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BrowserFirstSessionPane } from "./browser_first_session_pane.jsx";
import { boundedFirstProductionSelection } from "./browser_game_permit.js";
import { firstSessionFacts } from "./browser_first_session_facts.js";
afterEach(cleanup);
const fence = { agent_id: "agent", task_id: "task", task_revision: "1", active_goal_reference: { goal_ref: "goal" } };
const selection = boundedFirstProductionSelection({ factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1, maxElectricity: 8,
 maxConsume: [{ kind: "iron_ore", amount: 4 }, { kind: "carbon_fuel", amount: 1 }, { kind: "maintenance_part", amount: 1 }] }, "agent");
function fixture({ claimed = true, built = true, settled = false, locale = "en" } = {}) {
 const claim = { agent_id: "agent", owner_binding: "owner", offer_id: "offer", slot_id: "slot", site_id: "site", factory_id: "factory",
  location_id: "location", profile_id: "ordinary-industrial-starter-v1", profile_revision: 1, recipe_id: "recipe.smelter.iron_ingot", materials: { iron_ore: 8, carbon_fuel: 2, maintenance_part: 2 } };
 const first = { ...claim, job_id: 42, consume: selection.maxConsume, produce: [{ kind: "iron_ingot", amount: 3 }], byproducts: [{ kind: "slag", amount: 1 }], power_required: 8 };
 const observation = { owner: { logical_tick: 5, view: { canonical_agent_owner: { agent_id: "agent" }, native_delegation_source_id: "source",
  native_delegation_authorizations: ["ScheduleRecipe", "BuildFactory"].map((action) => ({ grant: { grant_id: action, source_id: "source", agent_id: "agent", object_id: "factory", action_kinds: [action], revision: 1,
   valid_from_tick: 0, valid_until_tick: 100, limit_units: 8, revoked: false }, spent_units: 0, remaining_units: 8 })),
  ordinary_starter: { eligible: true, owner_binding: "owner", claim: claimed ? { ...claim, ...(settled ? { first_production: first } : {}) } : null,
   offers: { offer: { offer: { offer_id: "offer", materials: claim.materials, slots: { slot: { factory_id: "factory", site_id: "site", construction_electricity: 8 } } }, available_slots: ["slot"], blocker: null } } } } },
  world: { view: { state: { agents: { agent: { state: { resources: { amounts: { electricity: 24 } } } } }, material_ledgers: { "agent:agent": { iron_ore: 4 }, "site:site": { iron_ore: 4, carbon_fuel: 1, maintenance_part: 1 } },
   factories: built ? { factory: { factory_id: "factory", site_id: "site", builder_agent_id: "agent", site_location_id: "location", input_ledger: "site:site" } } : {},
   agent_location_authorities: { agent: { active: true, agent_id: "agent", location_id: "location" } },
   factory_site_authorities: { site: { active: true, chunk_ready: true, site_id: "site", location_id: "location", owner_agent_id: "agent" } },
   location_anchors: { location: { active: true, location_id: "location" } },
  } } } };
 const [state, setState] = createSignal({ ready: true, phase: "ready", fence });
 const preview = { permitId: "preview-id", confirmedFence: fence, contextOperationId: "preview:fixture", selection };
 const driver = { firstSessionView: vi.fn(async () => observation), operationId: vi.fn((domain) => domain),
  nextPreviewPermitId: vi.fn(async () => "preview-id"), endPreview: vi.fn(async () => {}),
  preview: vi.fn(async () => preview), nativeGrant: vi.fn(async () => {}), directAction: vi.fn(async () => {}),
  starterClaim: vi.fn(async () => ({ type: "WorldRule", data: { actor_id: "agent", request_id: "claim", command: { kind: "ordinary_starter", data: {} } } })),
  modelSession: { snapshot: () => ({ checked: true, hasKey: true }) },
  start: vi.fn(async () => { setState((old) => ({ ...old, phase: "settled", submissionState: "committed", agentDelegated: true, originalOperationId: "original-action" })); }),
 };
 render(() => <BrowserFirstSessionPane locale={locale} driver={() => driver} state={state} />);
 return { driver, observation, first, setState, preview };
}
describe("ordinary first-session native consumer", () => {
 it.each(["zh-CN", "en"])("explains finite supply exhaustion and distinguishes an unpublished offer in %s", async (locale) => {
  const f = fixture({ claimed: false, built: false, locale }), zh = locale === "zh-CN";
  await screen.findByRole("button", { name: zh ? "准备领取权限" : "Review claim authority" });
  const status = f.observation.owner.view.ordinary_starter;
  status.eligible = false; status.blocker = "ordinary_starter_supply_unavailable";
  status.offers.offer.blocker = "ordinary_starter_supply_unavailable"; status.offers.offer.available_slots = [];
  fireEvent.click(screen.getByRole("button", { name: zh ? "读取当前资源与作业" : "Read current resources and jobs" }));
  expect(await screen.findByText(zh ? /有限材料、电力或可用场地不足/ : /Finite materials, electricity or available sites are insufficient/)).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent(zh ? "不保证后续补充供给" : "later replenishment is not guaranteed");
  expect(screen.getByRole("status")).not.toHaveTextContent("ordinary_starter_supply_unavailable");
  expect(screen.getByText(/"starter_blocker": "ordinary_starter_supply_unavailable"/).closest("details")).not.toBeNull();
  expect(screen.getByRole("button", { name: zh ? "准备领取权限" : "Review claim authority" })).toBeDisabled();
  status.offers = {};
  fireEvent.click(screen.getByRole("button", { name: zh ? "读取当前资源与作业" : "Read current resources and jobs" }));
  expect(await screen.findByText(zh ? /尚未公布可领取的公共起步方案/ : /has not published a public starter offer/)).toBeVisible();
  expect(screen.queryByText(zh ? /有限材料、电力或可用场地不足/ : /Finite materials, electricity or available sites are insufficient/)).toBeNull();
  expect(f.driver.start).not.toHaveBeenCalled(); expect(f.driver.directAction).not.toHaveBeenCalled(); expect(f.driver.nativeGrant).not.toHaveBeenCalled();
 });
 it.each(["zh-CN", "en"])("explains unknown originals with read-only recovery guidance in %s", async (locale) => {
  const f = fixture({ locale }), zh = locale === "zh-CN";
  f.driver.preview.mockRejectedValue(Object.assign(new Error("operation_unknown"), { code: "operation_unknown" }));
  fireEvent.click(await screen.findByRole("button", { name: zh ? "核对单批权限与完整报价" : "Review one-batch authority and full quote" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent(zh ? "请保留原件" : "Retain the original");
  expect(alert).toHaveTextContent(zh ? "只读核对原件" : "Look up original operations");
  expect(alert).toHaveTextContent(zh ? "结果未知时不要再次提交" : "do not submit again while the result is unknown");
  expect(alert).not.toHaveTextContent("operation_unknown");
  expect(screen.getByText(/"error": "operation_unknown"/).closest("details")).not.toBeNull();
  expect(f.driver.preview).toHaveBeenCalledTimes(1);
  expect(f.driver.start).not.toHaveBeenCalled(); expect(f.driver.directAction).not.toHaveBeenCalled(); expect(f.driver.nativeGrant).not.toHaveBeenCalled();
 });
 it("waits for explicit exact grant confirmation and never starts a model for the claim", async () => {
  const f = fixture({ claimed: false, built: false });
  const button = await screen.findByRole("button", { name: "Review claim authority" });
  fireEvent.click(button);
  expect(await screen.findByText(/Claim the selected starter/)).toBeVisible();
  expect(f.driver.nativeGrant).not.toHaveBeenCalled(); expect(f.driver.start).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("checkbox", { name: "I confirm this exact object, scope and action" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Submit this player operation" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Submit this player operation" }));
  await waitFor(() => expect(f.driver.nativeGrant).toHaveBeenCalledTimes(1));
  expect(f.driver.nativeGrant.mock.calls[0][0]).toMatchObject({ confirmedFence: fence, operation: { action_kinds: ["WorldRule"], object_id: "offer", limit_units: "1" } });
  expect(f.driver.start).not.toHaveBeenCalled(); expect(f.driver.directAction).not.toHaveBeenCalled();
  f.observation.owner.view.native_delegation_authorizations.push({ grant: { grant_id: "claim", source_id: "source", agent_id: "agent", object_id: "offer", action_kinds: ["WorldRule"], revision: 1, valid_from_tick: 0, valid_until_tick: 100, limit_units: 1, revoked: false }, spent_units: 0, remaining_units: 1 });
  fireEvent.click(screen.getByRole("button", { name: "Read current resources and jobs" }));
  fireEvent.click(await screen.findByRole("button", { name: "Review this claim" }));
  await waitFor(() => expect(f.driver.operationId).toHaveBeenCalledWith("first-session-starter-claim", { fence, owner: "owner", offer: "offer", slot: "slot" }));
  expect(f.driver.start).not.toHaveBeenCalled(); expect(f.driver.directAction).not.toHaveBeenCalled();
 });
 it("confirms full signed inputs and delegates one exact quoted Context, then distinguishes request confirmation from product settlement", async () => {
  const f = fixture();
  fireEvent.click(await screen.findByRole("button", { name: "Review one-batch authority and full quote" }));
  expect(await screen.findByText(/Full input limit:.*Maintenance part × 1/)).toBeVisible();
  expect(f.driver.start).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("checkbox", { name: "I confirm this quote and current task; model delegation charges my provider account" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Let the Agent handle only this batch" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Let the Agent handle only this batch" }));
  await waitFor(() => expect(f.driver.start).toHaveBeenCalledWith({ delegation: { permitId: "first-session-agent-one-batch", selection,
   contextOperationId: "preview:fixture" } }));
  expect(f.driver.directAction).not.toHaveBeenCalled();
  expect(await screen.findByText(/Request result: Confirmed/)).toBeVisible();
  expect(screen.queryByText(/First product settled by the world/)).toBeNull();
  f.observation.owner.view.ordinary_starter.claim.first_production = f.first;
  fireEvent.click(screen.getByRole("button", { name: "Read current resources and jobs" }));
  expect(await screen.findByText(/First product settled by the world/)).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Next choice" }).value).toBe("");
 });
 it("requires the player's independent next choice, and submits that chosen batch without calling the model", async () => {
  const f = fixture({ settled: true });
  const choice = await screen.findByRole("combobox", { name: "Next choice" });
  expect(screen.queryByRole("button", { name: "Review one-batch authority and full quote" })).toBeNull();
  fireEvent.change(choice, { target: { value: "produce" } });
  fireEvent.click(screen.getByRole("button", { name: "Review one-batch authority and full quote" }));
  await screen.findByRole("button", { name: "Submit this action myself" });
  fireEvent.click(screen.getByRole("checkbox", { name: "I confirm this quote and this player action" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Submit this action myself" })).toBeEnabled());
  fireEvent.click(screen.getByRole("button", { name: "Submit this action myself" }));
  await waitFor(() => expect(f.driver.directAction).toHaveBeenCalledWith({ permitId: "first-session-player-action", confirmedFence: fence,
   selection, contextOperationId: "preview:fixture" }));
  expect(f.driver.start).not.toHaveBeenCalled();
 });
 it("fails closed for another owner's factory or first-production receipt", async () => {
  const f = fixture({ settled: true });
  await screen.findByRole("combobox", { name: "Next choice" });
  const altered = structuredClone(f.observation); altered.owner.view.ordinary_starter.claim.first_production.factory_id = "other-owner-factory";
  expect(() => firstSessionFacts(altered, "agent")).toThrow("game_response_invalid");
  altered.owner.view.ordinary_starter.claim.first_production.factory_id = "factory";
  altered.world.view.state.factories.factory.builder_agent_id = "another-agent";
  expect(() => firstSessionFacts(altered, "agent")).toThrow("game_response_invalid");
 });
 it("distinguishes a verified empty electricity stock from a missing Agent observation", async () => {
  const f = fixture(); await screen.findByRole("button", { name: "Review one-batch authority and full quote" });
  const observed = structuredClone(f.observation); observed.world.view.state.agents.agent.state.resources.amounts = {};
  expect(firstSessionFacts(observed, "agent").electricity).toBe(0);
  delete observed.world.view.state.agents.agent;
  expect(firstSessionFacts(observed, "agent").electricity).toBeNull();
 });
});
