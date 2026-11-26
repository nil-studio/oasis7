export function createViewerSemanticTimeoutModule({
  state,
  windowRef = window,
  SESSION_REGISTER_ACK_TIMEOUT_MS,
  AGENT_CHAT_OVERALL_TIMEOUT_MS,
  AGENT_CHAT_ACK_TIMEOUT_MS,
  SEMANTIC_ACTION_ACK_TIMEOUT_MS,
  hasPendingSessionRegisterWaiter,
  maybeRecoverLocalTestStarterBindingFromSnapshot,
  isTestApiEnabled,
  authConnectionGeneration,
  render,
}) {
  let hostedRuntimeSyncTimer = null;
  let pendingAgentChatAckTimer = null;
  let pendingAgentChatOverallTimer = null;
  let pendingPromptControlAckTimer = null;
  let pendingGameplayActionAckTimer = null;

  function clearPendingAgentChatAckTimer() {
    if (pendingAgentChatAckTimer) {
      windowRef.clearTimeout(pendingAgentChatAckTimer);
      pendingAgentChatAckTimer = null;
    }
  }

  function setPendingAgentChatAckTimer(timer) {
    pendingAgentChatAckTimer = timer;
  }

  function clearPendingAgentChatOverallTimer() {
    if (pendingAgentChatOverallTimer) {
      windowRef.clearTimeout(pendingAgentChatOverallTimer);
      pendingAgentChatOverallTimer = null;
    }
  }

  function clearPendingPromptControlAckTimer() {
    if (pendingPromptControlAckTimer) {
      windowRef.clearTimeout(pendingPromptControlAckTimer);
      pendingPromptControlAckTimer = null;
    }
  }

  function clearPendingGameplayActionAckTimer() {
    if (pendingGameplayActionAckTimer) {
      windowRef.clearTimeout(pendingGameplayActionAckTimer);
      pendingGameplayActionAckTimer = null;
    }
  }

  function clearHostedRuntimeSyncTimer() {
    if (hostedRuntimeSyncTimer) {
      windowRef.clearTimeout(hostedRuntimeSyncTimer);
      hostedRuntimeSyncTimer = null;
    }
  }

  function expireHostedRuntimeSyncTimeout() {
    hostedRuntimeSyncTimer = null;
    if (!state.auth.syncInFlight || hasPendingSessionRegisterWaiter()) {
      return;
    }
    state.auth.syncInFlight = false;
    state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
    state.auth.runtimeStatus = "error";
    state.auth.recoveryErrorCode = "runtime_sync_timeout";
    state.auth.recoveryErrorMessage = "runtime session sync timed out waiting for ack/error from live server";
    state.auth.error = state.auth.recoveryErrorMessage;
    maybeRecoverLocalTestStarterBindingFromSnapshot(state.snapshot);
    render();
  }

  function scheduleHostedRuntimeSyncTimeout() {
    clearHostedRuntimeSyncTimer();
    hostedRuntimeSyncTimer = windowRef.setTimeout(
      expireHostedRuntimeSyncTimeout,
      SESSION_REGISTER_ACK_TIMEOUT_MS,
    );
  }

  function expireHostedRuntimeSyncTimeoutForTest() {
    if (!isTestApiEnabled()) {
      throw new Error("expireHostedRuntimeSyncTimeoutForTest requires test_api=1");
    }
    clearHostedRuntimeSyncTimer();
    expireHostedRuntimeSyncTimeout();
  }

  function agentChatFeedbackInFlight(feedback) {
    return feedback && ["queued", "registering", "signing", "sent", "pending"].includes(String(feedback.stage || ""));
  }

  function semanticFeedbackInFlight(feedback) {
    return feedback && ["queued", "registering", "authorizing", "signing", "sent"].includes(String(feedback.stage || ""));
  }

  function sameAgentChatFeedback(left, right) {
    if (!left || !right) {
      return false;
    }
    if (left === right) {
      return true;
    }
    return left.id === right.id
      && left.kind === right.kind
      && left.action === right.action
      && left.agentId === right.agentId;
  }

  function sameSemanticFeedback(left, right) {
    if (!left || !right) {
      return false;
    }
    if (left === right) {
      return true;
    }
    return left.id === right.id
      && left.kind === right.kind
      && left.action === right.action
      && left.agentId === right.agentId;
  }

  function isAgentChatInFlight() {
    return agentChatFeedbackInFlight(state.lastChatFeedback);
  }

  function markAgentChatFeedbackError(feedback, reason, effect = "agent_chat failed") {
    if (!feedback) {
      return;
    }
    feedback.stage = feedback.canonicalRequest ? "unknown" : "error";
    feedback.ok = false;
    feedback.accepted = false;
    feedback.reason = reason;
    feedback.effect = feedback.canonicalRequest ? "canonical goal result unknown; refresh current world state" : effect;
    state.lastChatFeedback = feedback;
  }

  function expireAgentChatOverallTimeout(feedback) {
    if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || !agentChatFeedbackInFlight(state.lastChatFeedback)) {
      return false;
    }
    clearPendingAgentChatOverallTimer();
    clearPendingAgentChatAckTimer();
    markAgentChatFeedbackError(
      feedback,
      "agent_chat timed out before live server ack/error completed",
      "agent_chat overall timeout",
    );
    render();
    return true;
  }

  function scheduleAgentChatOverallTimeout(feedback) {
    clearPendingAgentChatOverallTimer();
    pendingAgentChatOverallTimer = windowRef.setTimeout(() => {
      pendingAgentChatOverallTimer = null;
      expireAgentChatOverallTimeout(feedback);
    }, AGENT_CHAT_OVERALL_TIMEOUT_MS);
  }

  function expirePendingAgentChatOverallTimeoutForTest() {
    if (!isTestApiEnabled()) {
      throw new Error("expirePendingAgentChatOverallTimeoutForTest requires test_api=1");
    }
    clearPendingAgentChatOverallTimer();
    return expireAgentChatOverallTimeout(state.lastChatFeedback);
  }

  function scheduleAgentChatAckTimeout(feedback) {
    clearPendingAgentChatAckTimer();
    pendingAgentChatAckTimer = windowRef.setTimeout(() => {
      pendingAgentChatAckTimer = null;
      if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || !["sent", "pending"].includes(state.lastChatFeedback.stage)) {
        return;
      }
      clearPendingAgentChatOverallTimer();
      markAgentChatFeedbackError(
        feedback,
        "agent_chat timed out waiting for ack/error from live server",
        "agent_chat ack timeout",
      );
      render();
    }, AGENT_CHAT_ACK_TIMEOUT_MS);
  }

  function failPendingAgentChatAck(reason, effect = "agent_chat ack failed") {
    clearPendingAgentChatAckTimer();
    clearPendingAgentChatOverallTimer();
    const feedback = state.lastChatFeedback;
    if (!agentChatFeedbackInFlight(feedback)) {
      return;
    }
    markAgentChatFeedbackError(feedback, reason, effect);
  }

  function markSemanticFeedbackError(feedback, reason, effect) {
    if (!feedback) {
      return;
    }
    feedback.stage = "error";
    feedback.ok = false;
    feedback.accepted = false;
    feedback.reason = reason;
    feedback.effect = effect;
  }

  function expirePromptControlAckTimeout(feedback) {
    if (!sameSemanticFeedback(state.lastPromptFeedback, feedback) || state.lastPromptFeedback.stage !== "sent") {
      return false;
    }
    clearPendingPromptControlAckTimer();
    markSemanticFeedbackError(
      feedback,
      "prompt_control timed out waiting for ack/error from live server",
      "prompt_control ack timeout",
    );
    state.lastPromptFeedback = feedback;
    render();
    return true;
  }

  function expireGameplayActionAckTimeout(feedback) {
    if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback) || !["sent", "pending", "received"].includes(state.lastGameplayActionFeedback.stage)) {
      return false;
    }
    clearPendingGameplayActionAckTimer();
    markSemanticFeedbackError(
      feedback,
      "gameplay_action timed out waiting for ack/error from live server",
      "gameplay_action ack timeout",
    );
    feedback.stage = "unknown";
    state.lastGameplayActionFeedback = feedback;
    render();
    return true;
  }

  function schedulePromptControlAckTimeout(feedback) {
    clearPendingPromptControlAckTimer();
    pendingPromptControlAckTimer = windowRef.setTimeout(() => {
      pendingPromptControlAckTimer = null;
      expirePromptControlAckTimeout(feedback);
    }, SEMANTIC_ACTION_ACK_TIMEOUT_MS);
  }

  function scheduleGameplayActionAckTimeout(feedback) {
    clearPendingGameplayActionAckTimer();
    pendingGameplayActionAckTimer = windowRef.setTimeout(() => {
      pendingGameplayActionAckTimer = null;
      expireGameplayActionAckTimeout(feedback);
    }, SEMANTIC_ACTION_ACK_TIMEOUT_MS);
  }

  function expirePendingPromptControlAckTimeoutForTest() {
    if (!isTestApiEnabled()) {
      throw new Error("expirePendingPromptControlAckTimeoutForTest requires test_api=1");
    }
    clearPendingPromptControlAckTimer();
    return expirePromptControlAckTimeout(state.lastPromptFeedback);
  }

  function connectionGenerationForTest() {
    if (!isTestApiEnabled()) {
      throw new Error("connectionGenerationForTest requires test_api=1");
    }
    return authConnectionGeneration();
  }

  function expirePendingGameplayActionAckTimeoutForTest() {
    if (!isTestApiEnabled()) {
      throw new Error("expirePendingGameplayActionAckTimeoutForTest requires test_api=1");
    }
    clearPendingGameplayActionAckTimer();
    return expireGameplayActionAckTimeout(state.lastGameplayActionFeedback);
  }

  function failPendingPromptControlAck(reason, effect = "prompt_control ack failed") {
    clearPendingPromptControlAckTimer();
    const feedback = state.lastPromptFeedback;
    if (!feedback || feedback.stage !== "sent") {
      return;
    }
    markSemanticFeedbackError(feedback, reason, effect);
    state.lastPromptFeedback = feedback;
  }

  function failPendingGameplayActionAck(reason, effect = "gameplay_action ack failed") {
    clearPendingGameplayActionAckTimer();
    const feedback = state.lastGameplayActionFeedback;
    if (!feedback || !["sent", "pending", "received"].includes(feedback.stage)) {
      return;
    }
    markSemanticFeedbackError(feedback, reason, effect);
    feedback.stage = "unknown";
    state.lastGameplayActionFeedback = feedback;
  }

  return {
    clearPendingAgentChatAckTimer,
    setPendingAgentChatAckTimer,
    clearPendingAgentChatOverallTimer,
    clearPendingPromptControlAckTimer,
    clearPendingGameplayActionAckTimer,
    clearHostedRuntimeSyncTimer,
    expireHostedRuntimeSyncTimeout,
    scheduleHostedRuntimeSyncTimeout,
    expireHostedRuntimeSyncTimeoutForTest,
    agentChatFeedbackInFlight,
    semanticFeedbackInFlight,
    sameAgentChatFeedback,
    sameSemanticFeedback,
    isAgentChatInFlight,
    markAgentChatFeedbackError,
    expireAgentChatOverallTimeout,
    scheduleAgentChatOverallTimeout,
    expirePendingAgentChatOverallTimeoutForTest,
    scheduleAgentChatAckTimeout,
    failPendingAgentChatAck,
    markSemanticFeedbackError,
    expirePromptControlAckTimeout,
    expireGameplayActionAckTimeout,
    schedulePromptControlAckTimeout,
    scheduleGameplayActionAckTimeout,
    expirePendingPromptControlAckTimeoutForTest,
    connectionGenerationForTest,
    expirePendingGameplayActionAckTimeoutForTest,
    failPendingPromptControlAck,
    failPendingGameplayActionAck,
  };
}
