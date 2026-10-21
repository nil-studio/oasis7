// Only inputs used by these read-only preflights belong in the fingerprint.
// A new tick, feed receipt, movement or another Agent does not change either quote.
export function quoteDependencyFingerprint(state, kind, inputs = {}, snapshot = state.snapshot) {
  const auth = state.auth || {}, model = snapshot?.model || {};
  const runtime = snapshot?.runtime_snapshot?.state || {};
  const id = auth.boundAgentId || "";
  const owner = state.canonicalAgentOwnerView || snapshot?.player_gameplay?.canonical_agent_chat;
  const authority = owner?.canonical_authority;
  const common = [auth.available, auth.playerId, auth.publicKey, id, auth.registrationStatus,
    auth.sessionEpoch, auth.bindingEpoch, auth.authorityEpoch,
    model.agents?.[id]?.id ?? (model.agents?.[id] ? id : null),
    model.agent_player_bindings?.[id], model.agent_player_public_key_bindings?.[id],
    state.worldFeed?.worldId, state.worldFeed?.reorgEpoch, state.worldFeed?.stale,
    owner?.world_id, owner?.player_id, owner?.public_key, owner?.agent_id,
    authority?.branch_id, authority?.agent_identity_generation];
  if (kind === "refine") {
    const amounts = model.agents?.[id]?.resources?.amounts || {};
    const economy = snapshot?.config?.economy || {};
    return JSON.stringify([common, amounts.data ?? 0, amounts.electricity ?? 0,
      economy.refine_electricity_cost_per_kg, economy.refine_hardware_yield_ppm,
      economy.factory_build_hardware_cost]);
  }
  const profile = runtime.product_profiles?.[inputs.product_id];
  return JSON.stringify([common, inputs.product_id, inputs.amount,
    runtime.industry_progress?.stage, profile?.product_id, profile?.role_tag,
    profile?.tradable, profile?.unlock_stage]);
}

export function quoteResponseMatches(state, request, response, kind) {
  return request?.status === "pending" && request.requestAuthNonce != null
    && response?.request_auth_nonce === request.requestAuthNonce
    && request.dependencyFingerprint === quoteDependencyFingerprint(state, kind, request.dependencyInputs);
}

export function invalidateDependentQuote(state, kind, snapshot) {
  const requestKey = kind === "refine" ? "refineQuoteRequest" : "productValidationQuoteRequest";
  const quoteKey = kind === "refine" ? "refineQuotePreflight" : "productValidationQuote";
  const request = state[requestKey];
  if (!request || !["pending", "received"].includes(request.status)) return;
  const previous = request.dependencyFingerprint ?? quoteDependencyFingerprint(state, kind, request.dependencyInputs);
  if (previous === quoteDependencyFingerprint(state, kind, request.dependencyInputs, snapshot)) return;
  state[quoteKey] = null;
  state[requestKey] = { ...request, status: "idle", error: null };
}
