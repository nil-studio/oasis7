export function claimField(value, ...names) {
  if (!value || typeof value !== "object") return null;
  for (const name of names) {
    if (value[name] !== undefined && value[name] !== null) {
      return value[name];
    }
  }
  return null;
}
export function compactValue(value) {
  if (value === null || value === undefined || value === "") return "-";
  if (typeof value === "number") return Number.isFinite(value) ? String(value) : "-";
  if (typeof value === "boolean") return value ? "true" : "false";
  return String(value);
}
export function claimMoney(value) {
  const amount = claimField(value, "amount", "tokens", "balance", "value");
  const symbol = claimField(value, "symbol", "denom", "currency");
  if (amount !== null) {
    return symbol ? `${amount} ${symbol}` : compactValue(amount);
  }
  return compactValue(value);
}
export function claimQuoteRows(quote) {
  if (!quote || typeof quote !== "object") return [];
  return [
    ["Slot", claimField(quote, "slot_index", "slot", "slot_id", "slotId", "claim_slot")],
    ["Reputation tier", claimField(quote, "reputation_tier", "reputationTier", "tier")],
    ["Owned / cap", [
      claimField(quote, "owned_claim_count", "owned", "owned_count", "ownedCount"),
      claimField(quote, "cap", "claim_cap", "claimCap"),
    ].filter((part) => part !== null && part !== undefined).join(" / ")],
    ["Total upfront", claimMoney(claimField(quote, "total_upfront_amount", "total_upfront", "totalUpfront", "upfront_total", "upfrontTotal"))],
    ["Activation fee", claimMoney(claimField(quote, "activation_fee_amount", "activation_fee", "activationFee"))],
    ["Bond", claimMoney(claimField(quote, "claim_bond_amount", "bond", "locked_bond", "lockedBond"))],
    ["Upkeep / epoch", claimMoney(claimField(quote, "upkeep_per_epoch", "upkeepPerEpoch", "upkeep"))],
    ["Eligible balance", claimMoney(claimField(quote, "eligible_claim_balance", "eligible_balance", "eligibleBalance"))],
    ["Liquid balance", claimMoney(claimField(quote, "transferable_liquid_balance", "liquid_balance", "liquidBalance"))],
    ["Restricted starter", claimMoney(claimField(quote, "restricted_starter_claim_balance", "restricted_starter_balance", "restrictedStarterBalance"))],
    ["Auto starter", claimMoney(claimField(quote, "auto_restricted_starter_claim_amount", "auto_starter_amount", "autoStarterAmount"))],
    ["Cooldown", claimField(quote, "release_cooldown_epochs", "cooldown_epochs", "cooldownEpochs", "cooldown")],
    ["Grace", claimField(quote, "grace_epochs", "graceEpochs", "grace")],
    ["Idle warning", claimField(quote, "idle_warning_epochs", "idleWarningEpochs")],
    ["Forced reclaim", claimField(quote, "forced_idle_reclaim_epochs", "forcedIdleReclaimEpochs")],
    ["Penalty bps", claimField(quote, "forced_reclaim_penalty_bps", "forcedReclaimPenaltyBps")],
    ["Reclaim terms", claimField(quote, "reclaim_terms", "reclaimTerms", "reclaim")],
  ].filter(([, value]) => value !== null && value !== undefined && value !== "");
}
const PRIMARY_CLAIM_QUOTE_LABELS = new Set(["Total upfront", "Eligible balance", "Owned / cap"]);
export function claimQuoteMetricClass(label) {
  return [
    "metric",
    PRIMARY_CLAIM_QUOTE_LABELS.has(label) ? "metric--claim-primary" : null,
    label === "Total upfront" ? "metric--claim-total" : null,
  ].filter(Boolean).join(" ");
}
export function claimTarget(claim) {
  return claimField(claim, "target_agent_id", "targetAgentId", "agent_id", "agentId", "target") || "agent";
}
export function claimStatusText(claim) {
  const status = claimField(claim, "status", "claim_status", "claimStatus") || "active";
  const paidThrough = claimField(claim, "upkeep_paid_through_epoch", "upkeepPaidThroughEpoch");
  const grace = claimField(claim, "grace_remaining_epochs", "graceRemainingEpochs", "grace_remaining", "graceRemaining");
  const releaseReadyIn = claimField(claim, "release_ready_in_epochs", "releaseReadyInEpochs");
  const releaseReadyAt = claimField(claim, "release_ready_at_epoch", "releaseReadyAtEpoch");
  const idleWarningIn = claimField(claim, "idle_warning_in_epochs", "idleWarningInEpochs");
  const reclaim = claimField(claim, "forced_reclaim_in_epochs", "forcedReclaimInEpochs", "forced_reclaim_epoch", "forcedReclaimEpoch", "forced_reclaim_at", "forcedReclaimAt");
  return [
    `status=${status}`,
    paidThrough !== null ? `upkeep paid through epoch ${paidThrough}` : null,
    releaseReadyIn !== null ? `release ready in ${releaseReadyIn}` : null,
    releaseReadyAt !== null ? `release ready at epoch ${releaseReadyAt}` : null,
    grace !== null ? `grace remaining ${grace}` : null,
    idleWarningIn !== null ? `idle warning in ${idleWarningIn}` : null,
    reclaim !== null ? `forced reclaim in ${reclaim}` : null,
  ].filter(Boolean).join(" · ");
}
export function claimOwnedDetail(claim) {
  const restrictedBond = claimField(claim, "claim_bond_locked_restricted_amount", "lockedBondRestricted");
  const liquidBond = claimField(claim, "claim_bond_locked_liquid_amount", "lockedBondLiquid");
  const restrictedSpent = claimField(claim, "upfront_restricted_spent_amount", "upfrontRestrictedSpent");
  const liquidSpent = claimField(claim, "upfront_liquid_spent_amount", "upfrontLiquidSpent");
  return [
    claimField(claim, "idle_warning", "idleWarning"),
    claimField(claim, "locked_bond_split", "lockedBondSplit"),
    restrictedBond !== null || liquidBond !== null
      ? `bond restricted=${compactValue(restrictedBond)} liquid=${compactValue(liquidBond)}`
      : null,
    restrictedSpent !== null || liquidSpent !== null
      ? `upfront restricted=${compactValue(restrictedSpent)} liquid=${compactValue(liquidSpent)}`
      : null,
  ].filter(Boolean).join(" · ");
}
export function releaseClaimActionState(actions) {
  let published = false;
  let disabledReason = null;
  const available = (actions || []).some((action) => {
    const raw = `${action.actionId || ""} ${action.label || ""} ${action.protocolAction || ""}`.toLowerCase();
    const isRelease = raw.includes("release_agent_claim") || raw.includes("release claim") || raw.includes("release_agent");
    if (!isRelease) {
      return false;
    }
    published = true;
    disabledReason = action.disabledReason || disabledReason;
    return !action.disabledReason;
  });
  return { available, published, disabledReason };
}
export function expansionBranchCards(gameplay, locale) {
  const goal = String(gameplay?.goalKind || "").toLowerCase();
  if (goal !== "choosefirstexpansiontradeoff" && goal !== "choosemidlooppath") {
    return [];
  }
  const actions = gameplay?.availableActions || [];
  const recommendations = Array.isArray(gameplay?.branchRecommendations)
    ? gameplay.branchRecommendations
    : [];
  if (recommendations.length === 0) {
    return gameplay?.branchHint ? [{ legacy: true }] : [];
  }
  return recommendations.map((recommendation) => {
    const action = actions.find((candidate) => candidate.actionId === recommendation.actionId) || null;
    const futureBeats = Array.isArray(recommendation.futureBeats) ? recommendation.futureBeats.map((beat) => String(beat || "").trim()) : [];
    const complete = [recommendation.routeLabel, recommendation.immediateGain, recommendation.riskOrLockin, recommendation.nextSessionHook]
      .every((value) => Boolean(String(value || "").trim())) && futureBeats.length === 2 && futureBeats.every(Boolean) && new Set(futureBeats).size === 2;
    return {
      ...recommendation,
      action,
      complete,
      futureBeats,
    };
  });
}
