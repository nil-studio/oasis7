// Local execution limits supplement the signed native grant. These comparisons
// are never used to encode signatures or claim server-side recipe/count limits.
const fail = (code = "action_permit_invalid") => { throw Object.assign(new Error(code), { code }); };
const object = (value) => value && Object.getPrototypeOf(value) === Object.prototype;
const id = (value) => typeof value === "string" && value.trim().length > 0 && value.length <= 256;
const positive = (value) => Number.isSafeInteger(value) && value > 0;
const canonical = (value) => Array.isArray(value) ? value.map(canonical)
 : object(value) ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])])) : value;
export const sameBrowserValue = (left, right) => JSON.stringify(canonical(left)) === JSON.stringify(canonical(right));
const clone = (value) => JSON.parse(JSON.stringify(value));
function materials(value) {
 if (!Array.isArray(value) || !value.length || value.length > 32) fail();
 const seen = new Set();
 return value.map((entry) => {
  if (!object(entry) || !id(entry.kind) || !positive(entry.amount) || seen.has(entry.kind)
   || Object.keys(entry).some((key) => !["kind", "amount"].includes(key))) fail();
  seen.add(entry.kind); return { kind: entry.kind, amount: entry.amount };
 }).sort((a, b) => a.kind.localeCompare(b.kind));
}
export function boundedFirstProductionSelection(value, agentId) {
 if (!object(value) || !id(agentId) || !id(value.factory) || value.recipe !== "recipe.smelter.iron_ingot"
  || value.batches !== 1 || value.maxElectricity !== 8
  || Object.keys(value).some((key) => !["factory", "recipe", "batches", "maxElectricity", "maxConsume"].includes(key))) fail();
 const maxConsume = materials(value.maxConsume);
 return { kind: "first_production", agentId, factory: value.factory, recipe: value.recipe,
  batches: 1, maxElectricity: 8, maxConsume };
}
export function directActionSelection(action, agentId) {
 if (!object(action) || !object(action.data) || !id(agentId)) fail();
 const data = action.data;
 if (action.type === "WorldRule") {
  if (data.actor_id !== agentId || !id(data.request_id) || !object(data.command)) fail();
 } else if (action.type === "TransferMaterial") {
  // The ordinary path can move the player's Agent ledger to a real site. It
  // must not pretend that the inverse site -> Agent capability exists.
  if (data.requester_agent_id !== agentId || data.from_ledger !== `agent:${agentId}`
   || typeof data.to_ledger !== "string" || !data.to_ledger.startsWith("site:")
   || !id(data.to_ledger.slice(5)) || !id(data.kind) || !positive(data.amount)
   || !Number.isSafeInteger(data.distance_km) || data.distance_km < 0
   || data.auto_reroute !== false || data.route_id != null
   || !Array.isArray(data.route_ids) || data.route_ids.length) fail();
 } else fail();
 return { kind: "direct_action", agentId, action: clone(action) };
}
export function directBuildSelection(value, agentId) {
 if (!object(value) || !id(agentId) || !id(value.site) || !id(value.factory)
  || value.factoryKind !== "factory.smelter.mk1" || !Number.isSafeInteger(value.maxElectricity)
  || value.maxElectricity < 0 || Object.keys(value).some((key) =>
   !["site", "factory", "factoryKind", "maxElectricity", "maxConsume"].includes(key))) fail();
 return { kind: "direct_build", agentId, site: value.site, factory: value.factory,
  factoryKind: value.factoryKind, maxElectricity: value.maxElectricity, maxConsume: materials(value.maxConsume) };
}
export function assertDirectTransferLocation(action, state) {
 if (action?.type !== "TransferMaterial") return;
 const data = action.data, siteId = data.to_ledger.slice(5);
 const agent = state?.agent_location_authorities?.[data.requester_agent_id];
 const site = state?.factory_site_authorities?.[siteId];
 const location = state?.location_anchors?.[site?.location_id];
 if (!agent?.active || agent.agent_id !== data.requester_agent_id || !site?.active || !site.chunk_ready
  || site.site_id !== siteId || agent.location_id !== site.location_id || !location?.active
  || location.location_id !== site.location_id || data.distance_km !== 0
  || !(site.owner_agent_id === data.requester_agent_id || site.authorized_agent_ids?.includes(data.requester_agent_id))) fail("action_permit_scope_mismatch");
}
export function validateBrowserActionSelection(selection) {
 if (!object(selection)) fail();
 let expected;
 if (selection.kind === "first_production") {
  const { kind: _kind, agentId, ...input } = selection;
  expected = boundedFirstProductionSelection(input, agentId);
 } else if (selection.kind === "direct_action") expected = directActionSelection(selection.action, selection.agentId);
 else if (selection.kind === "direct_build") {
  const { kind: _kind, agentId, ...input } = selection;
  expected = directBuildSelection(input, agentId);
 } else fail();
 if (!sameBrowserValue(expected, selection)) fail();
 return clone(expected);
}
export function assertFirstProductionTool(selection, name, args) {
 if (["observe", "industry", "capabilities", "abort"].includes(name)) {
  if (!object(args) || Object.keys(args).length) fail("tool_invalid");
  return;
 }
 if (!["quote_schedule", "schedule"].includes(name)) fail("action_permit_scope_mismatch");
 if (!sameBrowserValue(args, { factory: selection.factory, recipe: selection.recipe, batches: 1 })) fail("action_permit_scope_mismatch");
}
function withinMaterials(actual, limits) {
 let consumed;
 try { consumed = materials(actual); } catch { fail("action_permit_scope_mismatch"); }
 if (consumed.some((item) => item.amount > (limits.find((limit) => limit.kind === item.kind)?.amount ?? 0))) fail("action_permit_scope_mismatch");
}
export function assertBrowserPermitAction(selection, action, preview) {
 validateBrowserActionSelection(selection);
 if (selection.kind === "direct_action") {
  if (!sameBrowserValue(selection.action, action)) fail("action_permit_scope_mismatch");
  return;
 }
 const data = action?.data;
 if (selection.kind === "first_production") {
  if (action.type !== "ScheduleRecipe" || !object(data)
   || data.requester_agent_id !== selection.agentId || data.factory_id !== selection.factory
   || data.recipe_id !== selection.recipe || data.plan?.accepted_batches !== 1
   || data.plan.power_required !== 8
   || !Array.isArray(data.logistics_route_ids) || data.logistics_route_ids.length
   || !Array.isArray(data.logistics_path_ids) || data.logistics_path_ids.length) fail("action_permit_scope_mismatch");
  withinMaterials(data.plan.consume, [{ kind: "iron_ore", amount: 4 }, { kind: "carbon_fuel", amount: 1 }]);
 } else if (action.type !== "BuildFactory" || !object(data)
  || data.builder_agent_id !== selection.agentId || data.site_id !== selection.site
  || data.spec?.factory_id !== selection.factory) fail("action_permit_scope_mismatch");
 const power = selection.kind === "direct_build" ? preview?.construction_power_obligation?.electricity_amount : preview?.power_required;
 if (!object(preview) || !Number.isSafeInteger(power)
  || power < 0 || power > selection.maxElectricity) fail("action_permit_scope_mismatch");
 withinMaterials(preview.consume, selection.maxConsume);
}
