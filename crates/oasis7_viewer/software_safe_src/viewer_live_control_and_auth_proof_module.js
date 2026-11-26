import { quoteDependencyFingerprint } from "./viewer_quote_dependencies.js";
import { buildAuthEnvelope, signAuthPayload } from "./viewer_auth_crypto.js";

export function createViewerLiveControlAndAuthProofModule({
  state, getSocket, getViewerPromptControlModule, viewerAgentChatAuthModule,
  pendingControlFeedback, liveControlState,
  nextRequestId, nextAuthNonce, parseStepCount, snapshotControlFeedback,
  normalizedGameplayActions, gameplayProtocolAction,
  sendJson, render, clone,
  ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession,
}) {
  let refineQuoteRequestGeneration = 0;

  function gameplayActionByProtocolAction(protocolAction) {
    return normalizedGameplayActions().find((action) => gameplayProtocolAction(action) === protocolAction) || null;
  }

  function viewerControlGate(normalizedAction) {
    const protocolAction = state.controlProfile === "live"
      ? normalizedAction === "play" ? "live_control.play" : normalizedAction === "step" ? "live_control.step" : null
      : null;
    if (!protocolAction) return null;
    const gameplayAction = gameplayActionByProtocolAction(protocolAction);
    const disabledReason = String(gameplayAction?.disabled_reason || gameplayAction?.disabledReason || "").trim();
    if (!disabledReason) return null;
    return {
      reason: disabledReason,
      effect: `control blocked by gameplay gate: ${disabledReason}`,
      hint: state.snapshot?.player_gameplay?.next_step_hint || null,
    };
  }

  function sendViewerControl(action, payload) {
    const normalized = String(action || "").trim().toLowerCase();
    const currentRequestId = nextRequestId();
    const feedback = {
      id: currentRequestId, action: normalized, accepted: false, stage: "rejected",
      reason: null, hint: null, effect: null,
      baselineLogicalTime: state.logicalTime, baselineEventSeq: state.eventSeq,
      deltaLogicalTime: 0, deltaEventSeq: 0, deltaTraceCount: 0, requestId: currentRequestId,
    };
    let mode = null;
    if (normalized === "play") {
      mode = { mode: "play" };
    } else if (normalized === "pause") {
      mode = { mode: "pause" };
    } else if (normalized === "step") {
      const count = parseStepCount(payload);
      if (!count) {
        feedback.reason = "step requires numeric payload.count >= 1";
        feedback.effect = "request rejected before send";
        state.lastControlFeedback = feedback; render();
        return snapshotControlFeedback(feedback);
      }
      mode = { mode: "step", count };
    } else {
      feedback.reason = `unsupported action: ${normalized}`;
      feedback.effect = "request rejected before send";
      state.lastControlFeedback = feedback; render();
      return snapshotControlFeedback(feedback);
    }

    const gate = viewerControlGate(normalized);
    if (gate) {
      feedback.stage = "blocked"; feedback.reason = gate.reason; feedback.hint = gate.hint; feedback.effect = gate.effect;
      state.lastControlFeedback = feedback; render();
      return snapshotControlFeedback(feedback);
    }

    try {
      const type = state.controlProfile === "live" ? "live_control"
        : state.controlProfile === "playback" ? "playback_control" : "control";
      sendJson({ type, mode, request_id: currentRequestId });
      feedback.accepted = true; feedback.stage = "queued";
      feedback.effect = "queued, check getState().lastControlFeedback for world delta";
      pendingControlFeedback.set(currentRequestId, feedback);
      liveControlState.sent(feedback);
      state.lastControlFeedback = feedback; render();
      return snapshotControlFeedback(feedback);
    } catch (error) {
      feedback.reason = String(error); feedback.effect = "request send failed";
      state.lastControlFeedback = feedback; render();
      return snapshotControlFeedback(feedback);
    }
  }

  function sendControl(action, payload = null) { return sendViewerControl(action, payload); }

  function runSteps(payload) {
    const count = parseStepCount(payload);
    if (!count) return { ok: false, reason: "payload must be non-empty step string or count" };
    const feedback = sendControl("step", { count });
    return { ok: Boolean(feedback?.accepted), count, feedback };
  }

  function setMode() { return { ok: false, reason: "viewer does not expose 2d/3d camera modes" }; }

  async function buildAgentChatAuthProof(request, auth) {
    return viewerAgentChatAuthModule.buildAuthProof(request, auth);
  }

  async function buildPromptControlAuthProof(mode, request, auth) {
    return getViewerPromptControlModule()?.buildAuthProof(mode, request, auth);
  }

  async function buildPromptRollbackAuthProof(request, auth) {
    return getViewerPromptControlModule()?.buildRollbackAuthProof(request, auth);
  }

  async function buildSessionRegisterAuthProof(request, auth) {
    const nonce = nextAuthNonce();
    const payload = { operation: "session_register", player_id: auth.playerId, public_key: auth.publicKey, nonce };
    if (request.requested_agent_id != null) payload.requested_agent_id = request.requested_agent_id;
    payload.force_rebind = request.force_rebind === true;
    const signingPayload = buildAuthEnvelope(payload);
    return { scheme: "ed25519", player_id: auth.playerId, public_key: auth.publicKey, nonce, signature: await signAuthPayload(signingPayload, auth) };
  }

  async function buildGameplayActionAuthProof(request, auth) {
    const nonce = nextAuthNonce();
    const payload = { operation: "gameplay_action", action_id: request.action_id, target_agent_id: request.target_agent_id };
    if (request.actor_agent_id != null) payload.actor_agent_id = request.actor_agent_id;
    payload.player_id = auth.playerId; payload.public_key = auth.publicKey; payload.nonce = nonce;
    const signingPayload = buildAuthEnvelope(payload);
    return { scheme: "ed25519", player_id: auth.playerId, public_key: auth.publicKey, nonce, signature: await signAuthPayload(signingPayload, auth) };
  }

  async function buildRefineQuoteAuthProof(request, auth) {
    const nonce = nextAuthNonce();
    const signingPayload = buildAuthEnvelope({
      operation: "gameplay_action", action_id: "quote_refine_compound",
      target_agent_id: `compound_mass_g:${request.compound_mass_g}`,
      player_id: auth.playerId, public_key: auth.publicKey, nonce,
    });
    return { scheme: "ed25519", player_id: auth.playerId, public_key: auth.publicKey, nonce, signature: await signAuthPayload(signingPayload, auth) };
  }

  async function requestRefineQuote(compoundMassG) {
    const generation = ++refineQuoteRequestGeneration;
    const compoundMassGNumber = Number(compoundMassG);
    if (!Number.isSafeInteger(compoundMassGNumber) || compoundMassGNumber <= 0) {
      state.refineQuoteRequest = { status: "error", errorCode: "invalid_amount", error: "refine quote requires a positive whole-number compound mass in grams" };
      return { ok: false, reason: "refine quote requires a positive whole-number compound mass in grams" };
    }
    const socket = getSocket();
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      state.refineQuoteRequest = { status: "error", errorCode: "connection_unavailable", error: "refine quote requires a connected viewer websocket" };
      return { ok: false, reason: "refine quote requires a connected viewer websocket" };
    }
    try {
      await ensureHostedPlayerAuthAvailable();
      if (!state.auth.available) {
        state.refineQuoteRequest = { status: "error", errorCode: "player_session_required", error: state.auth.error || "refine quote requires an active player session" };
        return { ok: false, reason: state.auth.error || "refine quote requires an active player session" };
      }
      const boundAgentId = String(state.auth.boundAgentId || "").trim();
      if (!boundAgentId) {
        state.refineQuoteRequest = { status: "error", errorCode: "player_agent_binding_required", error: "refine quote requires a bound player Agent" };
        return { ok: false, reason: "refine quote requires a bound player Agent" };
      }
      await ensureRegisteredPlayerSession(boundAgentId);
      const request = { compound_mass_g: compoundMassGNumber, player_id: state.auth.playerId, public_key: state.auth.publicKey };
      const dependencyFingerprint = quoteDependencyFingerprint(state, "refine");
      const requestSocket = getSocket();
      request.auth = await buildRefineQuoteAuthProof(request, state.auth);
      if (generation !== refineQuoteRequestGeneration || getSocket() !== requestSocket
        || requestSocket?.readyState !== WebSocket.OPEN
        || dependencyFingerprint !== quoteDependencyFingerprint(state, "refine")) {
        return { ok: false, reason: "refine quote inputs changed during signing" };
      }
      state.refineQuotePreflight = null;
      state.refineQuoteRequest = { status: "pending", error: null, requestAuthNonce: request.auth.nonce, dependencyFingerprint, dependencyInputs: {} };
      sendJson({ type: "quote_refine_compound", request });
      return { ok: true, request: clone(request) };
    } catch (error) {
      if (generation !== refineQuoteRequestGeneration) return { ok: false, reason: "refine quote superseded" };
      const reason = `refine quote request failed: ${String(error)}`;
      state.refineQuoteRequest = { status: "error", errorCode: "session_sync_failed", error: reason };
      return { ok: false, reason };
    }
  }

  return {
    sendControl, runSteps, setMode,
    buildAgentChatAuthProof, buildPromptControlAuthProof, buildPromptRollbackAuthProof,
    buildSessionRegisterAuthProof, buildGameplayActionAuthProof, requestRefineQuote,
  };
}
