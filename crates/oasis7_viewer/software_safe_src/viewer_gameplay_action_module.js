import { requestGameplayResult } from "./viewer_gameplay_result_recovery.js";
import { getGameplayRecoveryScope, gameplaySendContextCurrent } from "./viewer_gameplay_recovery_scope.js";
import { normalizeGameplayActionRequest } from "./viewer_feedback_actions.js";
import { matchesGameplayReply, settleGameplayOriginal } from "./viewer_gameplay_receipt_feedback.js";
import { authConnectionGeneration, captureSessionContext } from "./viewer_auth_session_module.js";

export function createViewerGameplayActionModule({
  state, getSocket, hostedWorldAccount, quoteProtocolFacade, clone, nextRequestId, render, sendJson,
  requestSnapshotSafe, sendControl, currentBoundAgentControlError, sameSemanticFeedback,
  snapshotSemanticFeedback, createSemanticFeedback, ensureHostedPlayerAuthAvailable,
  assertSemanticCapability, ensureRegisteredPlayerSession, buildGameplayActionAuthProof,
  scheduleGameplayActionAckTimeout, clearPendingGameplayActionAckTimer,
  markCurrentGameplayActionFeedbackError, acceptRuntimeAckIdentity, isHostedAuthExpired,
}) {
  const gameplayActionRequiresActorAgent = (id) => id === "claim_agent" || id === "release_agent_claim";
  const resolveRegistrationAgentId = (actionId, targetAgentId, actorAgentId) => (
    gameplayActionRequiresActorAgent(actionId)
      ? actorAgentId || state.auth.boundAgentId || targetAgentId
      : actionId === "claim_first_agent" ? null : targetAgentId
  );

  function gameplayActionControlError(action) {
    const normalized = normalizeGameplayActionRequest(action);
    if (!normalized || normalized.protocol_action !== "gameplay_action.submit") return null;
    const actionId = String(normalized.action_id || "").trim();
    const targetAgentId = String(normalized.target_agent_id || "").trim();
    const actorAgentId = String(normalized.actor_agent_id || "").trim();
    if (!actionId || !targetAgentId || actionId === "claim_first_agent") return null;
    if (gameplayActionRequiresActorAgent(actionId)) {
      const actorControlError = currentBoundAgentControlError(actorAgentId || state.auth.boundAgentId, `${actionId} actor`);
      if (actorControlError) return actorControlError;
      if (actorAgentId && actorAgentId !== state.auth.boundAgentId) return `${actionId} actor ${actorAgentId} does not match current bound Agent ${state.auth.boundAgentId}`;
      return null;
    }
    return currentBoundAgentControlError(targetAgentId, actionId);
  }

  function resolveGameplayActionRequest(actionOrId) {
    if (typeof actionOrId === "string") {
      const actions = Array.isArray(state.snapshot?.player_gameplay?.available_actions) ? state.snapshot.player_gameplay.available_actions : [];
      return actions.find((action) => action?.action_id === actionOrId) || null;
    }
    if (!actionOrId || typeof actionOrId !== "object") return null;
    if (typeof actionOrId.actionId === "string" && actionOrId.actionId.trim()) {
      const resolved = resolveGameplayActionRequest(actionOrId.actionId.trim());
      if (resolved) return resolved;
    }
    return normalizeGameplayActionRequest(actionOrId);
  }

  function sendGameplayAction(actionOrId, operation = { sessionRefreshRetryAttempted: false }, retryFeedback = null) {
    const action = resolveGameplayActionRequest(actionOrId);
    if (!action) return { ok: false, reason: "gameplay action is unavailable in the current snapshot" };
    const protocolAction = String(action.protocol_action || "").trim();
    if (protocolAction === "request_snapshot" || protocolAction === "world.request_snapshot") {
      const actionName = action.action_id || "request_snapshot";
      const targetId = action.target_agent_id || null;
      const setSnapshotFeedback = (ok, stage, reason, effect, extra = {}) => {
        state.lastGameplayActionFeedback = {
          id: nextRequestId(), kind: "gameplay_action", action: actionName, agentId: targetId,
          accepted: ok, ok, stage, reason, effect,
          response: { action_id: actionName, target_agent_id: targetId || "", ...extra, message: reason || effect },
        };
        render();
        return { ok, ...(reason ? { reason } : {}), feedback: snapshotSemanticFeedback(state.lastGameplayActionFeedback) };
      };
      if (requestSnapshotSafe() === false) {
        return setSnapshotFeedback(false, "rejected", "viewer websocket is not connected", "snapshot refresh failed to send");
      }
      if (hostedWorldAccount.hasPendingGameplayRequest()) {
        requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson);
        return { ok: true, feedback: snapshotSemanticFeedback(state.lastGameplayActionFeedback) };
      }
      return setSnapshotFeedback(true, "ack", null, "snapshot refresh requested", { accepted_at_tick: state.logicalTime });
    }
    if (protocolAction === "live_control.step") return { ok: true, feedback: sendControl("step", { count: 1 }) };
    if (protocolAction === "live_control.play") return { ok: true, feedback: sendControl("play", null) };
    if (protocolAction === "live_control.pause") return { ok: true, feedback: sendControl("pause", null) };
    if (protocolAction !== "gameplay_action.submit") return { ok: false, reason: `unsupported gameplay action protocol: ${protocolAction || "(empty)"}` };
    const actionId = String(action.action_id || "").trim();
    const targetAgentId = String(action.target_agent_id || "").trim();
    const actorAgentId = String(action.actor_agent_id || "").trim();
    if (!actionId || !targetAgentId) return { ok: false, reason: "gameplay_action.submit requires action_id and target_agent_id" };
    const disabledReason = String(action.disabled_reason || "").trim();
    if (disabledReason) return { ok: false, reason: disabledReason };
    const controlError = gameplayActionControlError(action);
    if (controlError) return { ok: false, reason: controlError };
    const previous = state.lastGameplayActionFeedback;
    const internalRetry = retryFeedback && sameSemanticFeedback(previous, retryFeedback)
      && previous.stage === "registering" && operation === retryFeedback.operation && operation.sessionRefreshRetryAttempted === true;
    if (hostedWorldAccount.hasPendingGameplayRequest() || (!internalRetry && previous && ((previous.action === actionId && (previous.targetAgentId || previous.agentId) === targetAgentId
      && ["sent", "pending", "received", "unknown"].includes(previous.stage)) || (state.auth.source === "hosted_account_cookie"
      && ["queued", "registering", "signing", "sent", "pending", "received", "unknown"].includes(previous.stage))))) {
      requestSnapshotSafe();
      return { ok: false, reason: "original gameplay request outcome is unresolved; refresh its authoritative result before submitting again" };
    }
    const feedback = createSemanticFeedback("gameplay_action", actionId, targetAgentId, { effect: "queued for signing and send", targetAgentId, protocolAction });
    feedback.operation = operation;
    state.lastGameplayActionFeedback = feedback;
    render();
    void (async () => {
      try {
        await ensureHostedPlayerAuthAvailable();
        assertSemanticCapability(actionId);
        feedback.stage = "registering"; feedback.effect = "registering player session"; render();
        await ensureRegisteredPlayerSession(resolveRegistrationAgentId(actionId, targetAgentId, actorAgentId));
        if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
        const socket = getSocket();
        if (!socket || socket.readyState !== WebSocket.OPEN) throw new Error("viewer websocket is not connected");
        feedback.stage = "signing"; feedback.effect = "building auth proof"; render();
        const request = { action_id: actionId, target_agent_id: targetAgentId, player_id: state.auth.playerId, public_key: state.auth.publicKey };
        if (gameplayActionRequiresActorAgent(actionId)) request.actor_agent_id = actorAgentId || state.auth.boundAgentId || targetAgentId;
        const sendContext = { socket, scope: getGameplayRecoveryScope(state), session: captureSessionContext(state, authConnectionGeneration(), state.wsUrl) };
        request.auth = await buildGameplayActionAuthProof(request, state.auth);
        if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
        if (request.auth.player_id !== state.auth.playerId || request.auth.public_key !== state.auth.publicKey) throw new Error("gameplay identity changed during signing");
        feedback.requestAuthNonce = request.auth.nonce; feedback.originalRequest = clone(request); feedback.gameplayScope = structuredClone(sendContext.scope);
        await hostedWorldAccount.rememberGameplayRequest(request, sendContext.scope);
        if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
        if (!gameplaySendContextCurrent(state, getSocket(), sendContext)) {
          feedback.stage = "unknown"; feedback.effect = "original request context changed before send";
          state.lastGameplayActionFeedback = feedback; render(); return;
        }
        feedback.stage = "sent"; feedback.effect = "gameplay action sent; waiting for ack";
        state.lastGameplayActionFeedback = feedback;
        sendJson({ type: "gameplay_action", request });
        scheduleGameplayActionAckTimeout(feedback); render();
      } catch (error) {
        if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
        markCurrentGameplayActionFeedbackError(error); render();
      }
    })();
    return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
  }

  function handleGameplayActionAck(ack) {
    if (!acceptRuntimeAckIdentity(ack)) return;
    const pendingAction = state.lastGameplayActionFeedback;
    if (!pendingAction || !["sent", "pending", "received", "unknown"].includes(pendingAction.stage) || !matchesGameplayReply(ack, pendingAction)) return;
    clearPendingGameplayActionAckTimer();
    const feedback = state.lastGameplayActionFeedback || createSemanticFeedback("gameplay_action", ack?.action_id || "gameplay_action", ack?.target_agent_id || null);
    feedback.stage = ack?.submission_status || "ack";
    feedback.ok = ack?.submission_status == null || ack.submission_status === "committed";
    feedback.accepted = feedback.ok; feedback.reason = null;
    feedback.effect = ack?.message || `gameplay action accepted at tick ${Number(ack?.accepted_at_tick || state.logicalTime)}`;
    feedback.response = clone(ack); state.lastGameplayActionFeedback = feedback;
    if (ack?.submission_status === "committed") settleGameplayOriginal(hostedWorldAccount, feedback.requestAuthNonce, getGameplayRecoveryScope(state), state);
    if (ack?.action_id === "claim_first_agent" || Number(ack?.runtime_action_id) === 0) {
      feedback.stage = ["received", "pending", "committed"].includes(ack?.submission_status) ? ack.submission_status : "pending";
      feedback.ok = false; feedback.accepted = false;
      feedback.effect = "request received; waiting for authoritative result and owner binding";
      scheduleGameplayActionAckTimeout(feedback);
    }
    requestSnapshotSafe();
  }

  async function retryGameplayActionAfterMissingSession(feedback, error, settlement) {
    const actionId = String(error?.action_id || feedback?.action || "").trim();
    const targetAgentId = String(error?.target_agent_id || feedback?.agentId || "").trim();
    if (!actionId || !targetAgentId || feedback?.operation?.sessionRefreshRetryAttempted) return;
    const action = resolveGameplayActionRequest(actionId) || normalizeGameplayActionRequest({ protocol_action: "gameplay_action.submit", action_id: actionId, target_agent_id: targetAgentId });
    if (!action) return;
    const controlError = gameplayActionControlError(action);
    if (controlError) {
      feedback.stage = "error"; feedback.ok = false; feedback.accepted = false;
      feedback.reason = controlError; feedback.effect = "player session refresh blocked by current account Agent boundary";
      state.lastGameplayActionFeedback = feedback; render(); return;
    }
    feedback.operation ||= { sessionRefreshRetryAttempted: false };
    feedback.operation.sessionRefreshRetryAttempted = true;
    feedback.stage = "registering"; feedback.ok = false; feedback.accepted = false;
    feedback.reason = "runtime session was missing; refreshing player session and retrying";
    feedback.effect = "refreshing player session"; state.lastGameplayActionFeedback = feedback; render();
    try {
      const settled = await settlement;
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      if (state.auth.source === "hosted_account_cookie" && settled !== true) {
        feedback.stage = "unknown"; feedback.reason = "original gameplay request settlement is not confirmed";
        state.lastGameplayActionFeedback = feedback; render(); return;
      }
      await ensureRegisteredPlayerSession(resolveRegistrationAgentId(actionId, targetAgentId, action.actor_agent_id || action.actorAgentId), { forceRebind: true });
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      sendGameplayAction(action, feedback.operation, feedback);
    } catch (retryError) {
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      markCurrentGameplayActionFeedbackError(retryError, "player session refresh failed before retrying gameplay action"); render();
    }
  }

  function handleGameplayActionError(error) {
    if (quoteProtocolFacade.handleQuoteGameplayActionError(error)) return;
    const feedback = state.lastGameplayActionFeedback;
    if (!feedback || !["sent", "pending", "received", "unknown"].includes(feedback.stage) || !matchesGameplayReply(error, feedback)) return;
    clearPendingGameplayActionAckTimer();
    const authExpired = isHostedAuthExpired(error);
    feedback.stage = (error?.code === "world_service_outcome_unknown" || authExpired) ? "unknown" : "error";
    feedback.ok = false; feedback.accepted = false;
    feedback.reason = error?.message || error?.code || "gameplay action failed";
    feedback.effect = authExpired ? "hosted authorization expired; reconciling authoritative result" : (error?.code || "gameplay action error");
    feedback.response = clone(error);
    state.lastGameplayActionFeedback = feedback;
    const settlement = feedback.stage !== "unknown" ? settleGameplayOriginal(hostedWorldAccount, feedback.requestAuthNonce, getGameplayRecoveryScope(state), state) : Promise.resolve(false);
    if (authExpired) {
      hostedWorldAccount.handleAuthorizationExpired?.(error);
      void hostedWorldAccount.renew().then(() => { state.gameplayResultLookup = null; requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson); }).catch(() => {});
      return;
    }
    if (String(error?.code || "").trim() === "session_not_found") void retryGameplayActionAfterMissingSession(feedback, error, settlement);
  }

  return { sendGameplayAction, handleGameplayActionAck, handleGameplayActionError };
}
