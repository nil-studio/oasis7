import { getGameplayRecoveryScope } from "./viewer_gameplay_recovery_scope.js";
export function settleGameplayOriginal(account, nonce, scope, state) {
  const auth = state.auth;
  return account.settleGameplayRequest(nonce, scope).catch(error => {
    if (state.auth === auth && JSON.stringify(getGameplayRecoveryScope(state)) === JSON.stringify(scope)) state.auth.error = String(error);
    return false;
  });
}
export function recoverGameplayFeedback(snapshot, state, account, scope, clearTimer) {
  account.restoreGameplayRequest(scope);
  const result = adoptRuntimeGameplayFeedback(snapshot, state.lastGameplayActionFeedback, clearTimer);
  if (result && ["committed", "applied", "completed_advanced", "rejected", "blocked"].includes(result.stage)) {
    settleGameplayOriginal(account, result.request_auth_nonce, scope, state);
  }
}

export function matchesGameplayReply(reply, feedback) {
  return reply?.action_id === feedback.action && reply?.target_agent_id === (feedback.targetAgentId || feedback.agentId)
    && (feedback.requestAuthNonce == null || reply.request_auth_nonce === feedback.requestAuthNonce);
}
export function adoptRuntimeGameplayFeedback(snapshot, feedback, clearTimer) {
  const authoritative = matchingRuntimeGameplayFeedback(snapshot?.player_gameplay, feedback);
  if (!authoritative) return;
  // An older admission snapshot cannot undo an authenticated disposition read.
  if (["committed", "rejected", "expired"].includes(feedback.stage)
    && ["received", "pending", "queued", "submitted"].includes(authoritative.stage)) return;
  clearTimer();
  feedback.stage = authoritative.stage;
  feedback.effect = authoritative.effect;
  feedback.reason = authoritative.reason || null;
  feedback.ok = ["committed", "applied", "completed_advanced"].includes(authoritative.stage);
  feedback.accepted = feedback.ok;
  return authoritative;
}

export function matchingRuntimeGameplayFeedback(gameplay, localFeedback) {
  const feedback = gameplay?.recent_feedback;
  return localFeedback?.requestAuthNonce != null && feedback?.request_auth_nonce === localFeedback.requestAuthNonce
    && (!feedback.target_agent_id || feedback.target_agent_id === (localFeedback.targetAgentId || localFeedback.agentId))
    && (feedback.action === `gameplay_action:${localFeedback.action}` || feedback.action === localFeedback.action || feedback.action === "world_service_intent")
    ? feedback : null;
}

export function projectGameplayReceiptFeedback(gameplay, localFeedback, intentTarget) {
    const runtimeRecentFeedback = gameplay.recent_feedback && typeof gameplay.recent_feedback === "object"
      ? {
          source: "runtime",
          agentId: intentTarget,
          action: gameplay.recent_feedback.action || null,
          stage: gameplay.recent_feedback.stage || null,
          effect: gameplay.recent_feedback.effect || null,
          reason: gameplay.recent_feedback.reason || null,
          hint: gameplay.recent_feedback.hint || null,
          deltaLogicalTime: Number(gameplay.recent_feedback.delta_logical_time || 0),
          deltaEventSeq: Number(gameplay.recent_feedback.delta_event_seq || 0),
        }
      : null;
    const localGameplayFeedback = localFeedback?.kind === "gameplay_action"
      ? {
          source: "local_gameplay_action",
          agentId: Object.hasOwn(localFeedback, "agentId") ? localFeedback.agentId : localFeedback.targetAgentId ?? null,
          action: localFeedback.action || null,
          stage: localFeedback.stage || null,
          effect: localFeedback.effect || null,
          reason: localFeedback.reason || null,
          hint: localFeedback.response?.hint || null,
          deltaLogicalTime: Number(localFeedback.deltaLogicalTime || 0),
          deltaEventSeq: Number(localFeedback.deltaEventSeq || 0),
        }
      : null;
    const sameRequest = matchingRuntimeGameplayFeedback(gameplay, localFeedback);
    const recentFeedback = sameRequest && runtimeRecentFeedback
      ? runtimeRecentFeedback : localGameplayFeedback || runtimeRecentFeedback;
  return {runtimeRecentFeedback,localGameplayFeedback,recentFeedback};
}

export function firstClaimAwaitingOutcome(feedback) {
  return String(feedback?.action || "").replace(/^gameplay_action:/, "") === "claim_first_agent"
    && ["received", "pending", "unknown", "committed", "accepted", "submitted", "queued", "ack", "registering", "signing", "sent"].includes(feedback.stage);
}
export function gameplayExecutionStateLabel(state, locale, localeText) {
  const labels = {
    received: ["已收到请求", "Received"], pending: ["等待权威结果", "Pending"], unknown: ["结果待确认", "Result unknown"],
    accepted: ["已接受", "Accepted"], blocked: ["已阻塞", "Blocked"], completed: ["已完成", "Completed"],
    rejected: ["已拒绝", "Rejected"], executing: ["执行中", "Executing"],
  };
  return localeText(locale, ...(labels[state] || labels.executing));
}
