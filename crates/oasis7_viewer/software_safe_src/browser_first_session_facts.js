import { assertDirectTransferLocation } from "./browser_game_permit.js";
const fail = () => { throw Object.assign(new Error("game_response_invalid"), { code: "game_response_invalid" }); };
export function firstSessionFacts(observation, agentId) {
 const owner = observation?.owner, view = owner?.view;
 if (!view || view.canonical_agent_owner?.agent_id !== agentId) fail();
 const starter = view.ordinary_starter;
 if (!starter || !Number.isSafeInteger(owner.logical_tick)) fail();
 const state = observation.world?.view?.state ?? observation.world?.view?.observation?.state ?? null;
 const claim = starter.claim;
 if (claim && (claim.agent_id !== agentId || claim.owner_binding !== starter.owner_binding)) fail();
 const factory = claim && state?.factories?.[claim.factory_id];
 if (factory && (factory.builder_agent_id !== agentId || factory.site_id !== claim.site_id)) fail();
 const first = claim?.first_production;
 if (first && (first.agent_id !== agentId || first.owner_binding !== claim.owner_binding
  || first.factory_id !== claim.factory_id || first.site_id !== claim.site_id
  || first.recipe_id !== claim.recipe_id || first.profile_id !== claim.profile_id
  || first.profile_revision !== claim.profile_revision || !Number.isSafeInteger(first.job_id))) fail();
 const amounts = state?.agents?.[agentId]?.state?.resources?.amounts;
 const agentMaterials = state?.material_ledgers?.[`agent:${agentId}`] ?? {};
 const inputMaterials = factory ? state?.material_ledgers?.[factory.input_ledger] ?? {} : {};
 let canTransfer = false;
 if (factory && factory.input_ledger === `site:${claim.site_id}` && factory.site_location_id === claim.location_id) {
  try { assertDirectTransferLocation({ type: "TransferMaterial", data: { requester_agent_id: agentId, to_ledger: factory.input_ledger, distance_km: 0 } }, state); canTransfer = true; } catch { /* Missing or changed current authority stays blocked. */ }
 }
 const allGrants = (view.native_delegation_authorizations ?? []).filter((authorization) => {
  const grant = authorization.grant;
  return grant?.agent_id === agentId && grant.source_id === view.native_delegation_source_id
   && [grant.revision, grant.valid_from_tick, grant.valid_until_tick, grant.limit_units, authorization.spent_units,
    authorization.remaining_units].every(Number.isSafeInteger)
   && authorization.spent_units >= 0 && authorization.remaining_units >= 0
   && authorization.spent_units + authorization.remaining_units === grant.limit_units;
 });
 const grants = allGrants.filter(({ grant }) => !grant.revoked && grant.valid_from_tick <= owner.logical_tick && owner.logical_tick <= grant.valid_until_tick);
 return { starter, claim, factory: factory || null, first: first || null, agentMaterials, inputMaterials,
  electricity: Number.isSafeInteger(amounts?.electricity) ? amounts.electricity
   : amounts && typeof amounts === "object" && !Array.isArray(amounts) && !Object.hasOwn(amounts, "electricity") ? 0 : null,
  tick: owner.logical_tick, grants, allGrants, state, canTransfer, openPreviews: observation.openPreviews ?? [], actionOriginals: observation.actionOriginals ?? [],
  hasGrant: (object, action, minimum = 0) => grants.some((authorization) => authorization.grant.object_id === object
   && authorization.grant.action_kinds.includes(action) && authorization.remaining_units >= minimum),
  pendingBuild: claim && Object.values(state?.pending_factory_builds ?? {}).find((job) => job.factory_id === claim.factory_id || job.spec?.factory_id === claim.factory_id),
 };
}

export function firstSessionGrantDraft({ digest, fence, tick, objectId, action, limit, priorAuthorities = [] }) {
 if (!Number.isSafeInteger(tick) || tick < 0 || !Number.isSafeInteger(tick + 12000)
  || !Number.isSafeInteger(limit) || limit < 1) fail();
 const grantId = digest("first-session-native-grant", { fence, objectId, action, tick, priorAuthorities });
 return { permitId: grantId, confirmedFence: fence, operation: { operation: "install", grant_id: grantId,
  object_id: objectId, action_kinds: [action], period_id: `first-session:${grantId}`,
  valid_from_tick: String(tick), valid_until_tick: String(tick + 12000), limit_units: String(limit) } };
}
