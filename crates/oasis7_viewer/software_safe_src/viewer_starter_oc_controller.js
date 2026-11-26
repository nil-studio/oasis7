import { createSignal } from "solid-js";
import * as core from "./legacy_core.js";
import { claimField } from "./viewer_claim_formatters.js";
import { gameplayActionDisabledReason } from "./viewer_gameplay_action_helpers.js";

function uiLocale() {
  return core.state.uiLocale;
}

function normalizedId(value) {
  return String(value || "").trim();
}

export function starterOcAction(gameplay) {
  if (starterOcOnboardingCompletedForCurrentAgent()) {
    return null;
  }
  const pendingTargetAgentId = normalizedId(starterOcOnboardingState.targetAgentId);
  if (starterOcOnboardingState.pending && pendingTargetAgentId) {
    return null;
  }
  const existing = (gameplay?.availableActions || []).find((action) => action.actionId === "claim_starter_oc" && !action.disabledReason);
  if (existing) {
    return existing;
  }
  return null;
}

const starterOcOnboardingState = {
  pending: false,
  targetAgentId: null,
  completedTargetAgentId: null,
};
const [starterOcOnboardingRevision, setStarterOcOnboardingRevision] = createSignal(0);
let starterOcBackgroundConfirmTimer = null;

export function touchStarterOcOnboardingState() {
  setStarterOcOnboardingRevision((value) => value + 1);
}

export function markStarterOcClaimPending(action) {
  if (action?.actionId !== "claim_starter_oc") {
    return;
  }
  starterOcOnboardingState.pending = true;
  starterOcOnboardingState.targetAgentId = normalizedId(action.targetAgentId || action.target_agent_id);
  starterOcOnboardingState.completedTargetAgentId = null;
  touchStarterOcOnboardingState();
}

export function scheduleStarterOcBackgroundConfirmation() {
  if (starterOcBackgroundConfirmTimer != null) {
    window.clearTimeout(starterOcBackgroundConfirmTimer);
  }
  starterOcBackgroundConfirmTimer = window.setTimeout(() => {
    const refreshAction = (core.buildGameplaySummary(uiLocale()).availableActions || [])
      .find((action) => action.executeKind === "request_snapshot");
    if (refreshAction) {
      core.sendGameplayAction(refreshAction);
    }
    starterOcBackgroundConfirmTimer = null;
  }, 450);
}

export function clearStarterOcClaimPending() {
  starterOcOnboardingState.pending = false;
  starterOcOnboardingState.targetAgentId = null;
  touchStarterOcOnboardingState();
}

export function completeStarterOcOnboarding() {
  starterOcOnboardingState.completedTargetAgentId = normalizedId(
    starterOcOnboardingState.targetAgentId || core.state.auth.boundAgentId,
  );
  clearStarterOcClaimPending();
  touchStarterOcOnboardingState();
}

export function __markStarterOcOnboardingCompleteForTest(agentId = core.state.auth.boundAgentId) {
  starterOcOnboardingState.pending = false;
  starterOcOnboardingState.targetAgentId = null;
  starterOcOnboardingState.completedTargetAgentId = normalizedId(agentId);
  touchStarterOcOnboardingState();
}

export function starterOcClaimPendingForCurrentAgent() {
  starterOcOnboardingRevision();
  if (!starterOcOnboardingState.pending) {
    return false;
  }
  const targetAgentId = normalizedId(starterOcOnboardingState.targetAgentId);
  return Boolean(targetAgentId && targetAgentId === normalizedId(core.state.auth.boundAgentId));
}

export function starterOcOnboardingCompletedForCurrentAgent() {
  starterOcOnboardingRevision();
  const completedTargetAgentId = normalizedId(starterOcOnboardingState.completedTargetAgentId);
  return Boolean(
    completedTargetAgentId
      && completedTargetAgentId === normalizedId(core.state.auth.boundAgentId),
  );
}

export function starterOcCreditVisibleForCurrentAgent() {
  const agentId = normalizedId(core.state.auth.boundAgentId);
  if (!agentId) {
    return false;
  }
  const snapshot = core.state.snapshot || {};
  const model = snapshot.model || {};
  const runtimeState = model.state || snapshot.state || model;
  const starterOcClaim = runtimeState.starter_oc_claims?.[agentId]
    || runtimeState.starterOcClaims?.[agentId]
    || model.starter_oc_claims?.[agentId]
    || model.starterOcClaims?.[agentId]
    || snapshot.starter_oc_claims?.[agentId]
    || snapshot.starterOcClaims?.[agentId]
    || null;
  if (starterOcClaim) {
    return true;
  }
  const balance = runtimeState.main_token_balances?.[agentId]
    || runtimeState.mainTokenBalances?.[agentId]
    || model.main_token_balances?.[agentId]
    || model.mainTokenBalances?.[agentId]
    || snapshot.main_token_balances?.[agentId]
    || snapshot.mainTokenBalances?.[agentId]
    || null;
  const liquidBalance = Number(claimField(balance, "liquid_balance", "liquidBalance", "liquid", "balance") || 0);
  return Number.isFinite(liquidBalance) && liquidBalance > 0;
}

export function rawStarterOcActionAvailable() {
  return (core.state.snapshot?.player_gameplay?.available_actions || [])
    .some((action) => action?.action_id === "claim_starter_oc");
}

export function starterOcSubmittedFeedback() {
  if (starterOcOnboardingCompletedForCurrentAgent() || starterOcCreditVisibleForCurrentAgent()) {
    return null;
  }
  const feedback = core.state.lastGameplayActionFeedback;
  if (
    feedback?.kind === "gameplay_action"
    && String(feedback?.action || "").includes("claim_starter_oc")
    && (feedback?.accepted || feedback?.stage === "ack" || feedback?.stage === "sent")
  ) {
    return feedback;
  }
  const runtimeFeedback = core.state.snapshot?.player_gameplay?.recent_feedback;
  if (
    String(runtimeFeedback?.action || "").includes("claim_starter_oc")
    && ["accepted", "queued", "sent"].includes(String(runtimeFeedback?.stage || ""))
  ) {
    return runtimeFeedback;
  }
  return null;
}

export function starterOcFeedbackNeedsLocalAdvance(feedback = starterOcSubmittedFeedback()) {
  if (!feedback) {
    return false;
  }
  const stage = String(feedback.stage || "").toLowerCase();
  if (stage === "submitted") {
    return false;
  }
  const effect = String(feedback.effect || "").toLowerCase();
  return ["accepted", "ack", "queued", "sent"].includes(stage)
    || effect.includes("queued gameplay action")
    || effect.includes("advance");
}

export function shouldShowStarterOcRequiredGate(gameplay = core.buildGameplaySummary(uiLocale())) {
  return Boolean(starterOcAction(gameplay) || starterOcSubmittedFeedback() || starterOcClaimPendingForCurrentAgent());
}

export function visibleGameplayActionsForPanels(gameplay) {
  const actions = Array.isArray(gameplay?.availableActions) ? gameplay.availableActions : [];
  if (!shouldShowStarterOcRequiredGate(gameplay)) {
    return actions;
  }
  return actions.filter((action) => action.actionId !== "claim_starter_oc");
}

export function gameplayProgressionAction(gameplay) {
  return (gameplay?.availableActions || []).find((action) => action.executeKind === "step")
    || (gameplay?.availableActions || []).find((action) => action.executeKind === "request_snapshot")
    || null;
}

export function firstAgentChatAction(gameplay) {
  return (gameplay?.availableActions || []).find((action) => action.executeKind === "agent_chat" && !gameplayActionDisabledReason(action, gameplay, uiLocale()))
    || null;
}
