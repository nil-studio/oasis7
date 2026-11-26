import { scheduleCanonicalGoalPending, applyCommittedChatAck, canonicalAckMatches } from "./viewer_canonical_goal_module.js";

export function createViewerSemanticQueueChatModule({
  state,
  getSocket,
  getViewerControlLossModule,
  canonicalOwnerRead,
  agentChatOverallTimeoutMs,
  clone,
  render,
  sendJson,
  requestSnapshotSafe,
  selectedAgentId,
  currentBoundAgentControlError,
  buildSemanticCapability,
  isAgentChatInFlight,
  sameAgentChatFeedback,
  agentChatFeedbackInFlight,
  sameSemanticFeedback,
  semanticFeedbackInFlight,
  markSemanticFeedbackError,
  createSemanticFeedback,
  snapshotSemanticFeedback,
  expireAgentChatOverallTimeout,
  scheduleAgentChatOverallTimeout,
  scheduleAgentChatAckTimeout,
  clearPendingAgentChatAckTimer,
  clearPendingAgentChatOverallTimer,
  setPendingAgentChatAckTimer,
  failPendingAgentChatAck,
  clearPendingPromptControlAckTimer,
  ensureHostedPlayerAuthAvailable,
  ensureRegisteredPlayerSession,
  buildAgentChatAuthProof,
  acceptRuntimeAckIdentity,
  pushChatHistory,
}) {
  let semanticSendLoop = null;
  const pendingSemanticCommands = [];

  function semanticCommandTimeoutError(command) {
    if (!Number.isFinite(command?.timeoutMs) || command.timeoutMs <= 0) return null;
    return new Promise((resolve) => {
      window.setTimeout(() => {
        resolve(new Error(`${command.kind || "semantic"} command timed out before send completed`));
      }, command.timeoutMs);
    });
  }

  async function executeSemanticCommand(command) {
    let executePromise;
    try {
      executePromise = Promise.resolve(command.execute());
    } catch (error) {
      executePromise = Promise.reject(error);
    }
    const timeoutPromise = semanticCommandTimeoutError(command);
    if (!timeoutPromise) {
      await executePromise;
      return;
    }
    const result = await Promise.race([executePromise.then(() => null), timeoutPromise]);
    if (result instanceof Error) {
      executePromise.catch(() => {});
      if (command.kind === "chat") {
        expireAgentChatOverallTimeout(command.feedback);
      } else if (command.kind === "prompt" && sameSemanticFeedback(state.lastPromptFeedback, command.feedback)) {
        markSemanticFeedbackError(
          command.feedback,
          "prompt_control timed out before live server ack/error completed",
          "prompt_control overall timeout",
        );
        state.lastPromptFeedback = command.feedback;
        render();
      }
    }
  }

  function handleSemanticCommandError(command, error) {
    if (command.kind === "chat") {
      if (!sameAgentChatFeedback(state.lastChatFeedback, command.feedback) || !agentChatFeedbackInFlight(state.lastChatFeedback)) {
        render();
        return;
      }
      clearPendingAgentChatAckTimer();
      clearPendingAgentChatOverallTimer();
    }
    if (command.kind === "prompt") {
      if (!sameSemanticFeedback(state.lastPromptFeedback, command.feedback) || !semanticFeedbackInFlight(state.lastPromptFeedback)) {
        render();
        return;
      }
      clearPendingPromptControlAckTimer();
    }
    const feedback = command.kind === "chat" ? state.lastChatFeedback : state.lastPromptFeedback;
    feedback.stage = "error";
    feedback.ok = false;
    feedback.accepted = false;
    feedback.reason = String(error);
    feedback.effect = "request build/send failed";
    render();
  }

  async function processSemanticCommands() {
    try {
      while (pendingSemanticCommands.length > 0) {
        const command = pendingSemanticCommands.shift();
        try {
          await executeSemanticCommand(command);
        } catch (error) {
          handleSemanticCommandError(command, error);
        }
      }
    } finally {
      semanticSendLoop = null;
      if (pendingSemanticCommands.length > 0) semanticSendLoop = processSemanticCommands();
    }
  }

  function enqueueSemanticCommand(command) {
    pendingSemanticCommands.push(command);
    if (!semanticSendLoop) semanticSendLoop = processSemanticCommands();
  }

  function assertSemanticCapability(actionId) {
    const capability = buildSemanticCapability(actionId);
    if (!capability.enabled) {
      throw new Error(capability.reason || state.auth.error || `${actionId} is unavailable`);
    }
  }

  function assertAgentChatFeedbackActive(feedback) {
    if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || !agentChatFeedbackInFlight(state.lastChatFeedback)) {
      throw new Error("agent_chat request expired before send completed");
    }
    state.lastChatFeedback = feedback;
  }

  function assertPromptFeedbackActive(feedback) {
    if (!sameSemanticFeedback(state.lastPromptFeedback, feedback) || !semanticFeedbackInFlight(state.lastPromptFeedback)) {
      throw new Error("prompt_control request expired before send completed");
    }
    state.lastPromptFeedback = feedback;
  }

  function sendAgentChat(agentIdOrPayload, maybeMessage) {
    let agentId = null;
    let message = null;
    if (typeof agentIdOrPayload === "object" && agentIdOrPayload !== null) {
      agentId = String(agentIdOrPayload.agentId || agentIdOrPayload.agent_id || selectedAgentId() || "");
      message = String(agentIdOrPayload.message || "");
    } else {
      agentId = String(agentIdOrPayload || selectedAgentId() || "");
      message = String(maybeMessage || "");
    }
    if (!agentId) return { ok: false, reason: "agent chat requires a selected agent or explicit agentId" };
    const controlError = currentBoundAgentControlError(agentId, "agent_chat");
    if (controlError) return { ok: false, reason: controlError };
    if (!message.trim()) return { ok: false, reason: "agent chat message cannot be empty" };
    if (isAgentChatInFlight()) {
      return { ok: false, reason: "agent_chat is already in flight; wait for ack/error before sending another message" };
    }
    const feedback = createSemanticFeedback("chat", "agent_chat", agentId, {
      effect: "queued for signing and send",
      pendingMessage: message,
      pendingPlayerId: state.auth.playerId || null,
    });
    state.lastChatFeedback = feedback;
    enqueueSemanticCommand({
      kind: "chat",
      feedback,
      timeoutMs: agentChatOverallTimeoutMs,
      execute: async () => {
        assertAgentChatFeedbackActive(feedback);
        scheduleAgentChatOverallTimeout(feedback);
        feedback.stage = "registering"; feedback.effect = "registering player session"; render();
        await ensureHostedPlayerAuthAvailable();
        assertAgentChatFeedbackActive(feedback);
        await ensureRegisteredPlayerSession(agentId);
        assertAgentChatFeedbackActive(feedback);
        await canonicalOwnerRead.refresh(agentId);
        assertSemanticCapability("agent_chat");
        feedback.stage = "signing"; feedback.effect = "building auth proof"; render();
        const request = {
          agent_id: agentId,
          message,
          player_id: state.auth.playerId,
          public_key: state.auth.publicKey,
        };
        request.auth = await buildAgentChatAuthProof(request, state.auth);
        assertAgentChatFeedbackActive(feedback);
        if (request.auth.player_id !== state.auth.playerId || request.auth.public_key !== state.auth.publicKey) {
          throw new Error("agent_chat identity changed during signing");
        }
        feedback.requestAuthNonce = request.auth?.nonce;
        feedback.canonicalRequest = request.canonical_authority ? request : null;
        feedback.canonicalRetryCount = 0;
        feedback.stage = "sent"; feedback.effect = "agent_chat request sent; waiting for ack";
        state.lastChatFeedback = feedback;
        sendJson({ type: "agent_chat", request });
        scheduleAgentChatAckTimeout(feedback);
        state.chatDraft.message = "";
        state.chatDraft.dirty = false;
        render();
      },
    });
    render();
    return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
  }

  function handleAgentChatAck(ack) {
    if (!acceptRuntimeAckIdentity(ack)) return;
    const feedback = state.lastChatFeedback;
    if (!agentChatFeedbackInFlight(feedback) || ack?.agent_id !== feedback.agentId) return;
    if (feedback.canonicalRequest && !canonicalAckMatches(ack, feedback.canonicalRequest)) return;
    if (feedback.requestAuthNonce != null && (ack?.request_auth_nonce ?? ack?.auth_nonce) !== feedback.requestAuthNonce) return;
    clearPendingAgentChatAckTimer();
    if (feedback.canonicalRequest && ack?.status === "pending") {
      setPendingAgentChatAckTimer(scheduleCanonicalGoalPending({
        ack, feedback, state, clone, requestSnapshotSafe, sameAgentChatFeedback,
        failPendingAgentChatAck, sendJson, scheduleAgentChatAckTimeout, getSocket,
      }));
      return;
    }
    if (feedback.canonicalRequest && (ack?.status !== "accepted" || !ack?.intent_id)) {
      failPendingAgentChatAck("canonical goal commit was not confirmed");
      return;
    }
    clearPendingAgentChatOverallTimer();
    applyCommittedChatAck({ ack, feedback, state, clone, pushChatHistory });
  }

  function handleAgentChatError(error) {
    const current = state.lastChatFeedback;
    if (!agentChatFeedbackInFlight(current) || error?.agent_id !== current.agentId) return;
    if (current.canonicalRequest && error?.request_auth_nonce !== current.canonicalRequest.auth?.nonce) return;
    if (current.requestAuthNonce != null && error?.request_auth_nonce !== current.requestAuthNonce) return;
    clearPendingAgentChatAckTimer();
    clearPendingAgentChatOverallTimer();
    const feedback = state.lastChatFeedback || createSemanticFeedback("chat", "agent_chat", error?.agent_id || selectedAgentId());
    const agentId = error?.agent_id || feedback.agentId || selectedAgentId() || null;
    const controlLoss = getViewerControlLossModule();
    const controlLost = controlLoss?.isControlLossError(error) === true;
    const response = controlLost ? controlLoss.markControlLost(agentId) : clone(error);
    feedback.stage = "error";
    feedback.ok = false;
    feedback.accepted = false;
    feedback.reason = controlLost ? "control_lost" : error?.message || error?.code || "agent chat failed";
    feedback.effect = controlLost ? "control_lost" : error?.code || "agent chat error";
    feedback.response = response;
    state.lastChatFeedback = feedback;
    pushChatHistory({
      id: `chat-error-${feedback.id}`,
      source: "error",
      agentId,
      targetAgentId: agentId,
      playerId: feedback.pendingPlayerId || state.auth.playerId || null,
      speaker: "runtime",
      message: feedback.reason,
      code: controlLost ? "control_lost" : error?.code || null,
      tick: Number(error?.accepted_at_tick || state.logicalTime || 0),
      locationId: error?.location_id || null,
      response,
    });
  }

  return {
    enqueueSemanticCommand,
    assertSemanticCapability,
    assertAgentChatFeedbackActive,
    assertPromptFeedbackActive,
    sendAgentChat,
    handleAgentChatAck,
    handleAgentChatError,
  };
}
