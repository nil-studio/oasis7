const PROMPT_RESULT_STATUSES = new Set(["accepted", "applied", "stale", "rejected", "blocked"]);

export function createViewerPromptControlModule({
  applyPromptAckLocally,
  assertPromptFeedbackActive,
  buildAuthEnvelope,
  buildPromptControlSigningPayload,
  clearPendingPromptControlAckTimer,
  clearPendingSessionRegisterWaiter,
  clone,
  createSemanticFeedback,
  ensureHostedPlayerAuthAvailable,
  ensureRegisteredPlayerSession,
  nextRequestId,
  nextAuthNonce,
  onControlLost,
  render,
  requestSnapshotSafe,
  selectedAgentId,
  selectedAgentPromptProfile,
  signAuthPayload,
  state,
}) {
  let pendingAuthoritativeRefresh = null;

  function resetForConnection() {
    pendingAuthoritativeRefresh = null;
    state.viewerProtocol = {
      negotiated: false,
      version: null,
      capabilities: [],
      authorityEpoch: null,
    };
    state.auth.authorityEpoch = null;
    state.auth.bindingEpoch = null;
    state.auth.sessionEpoch = null;
  }

  function handleHelloAck(message) {
    state.viewerProtocol.negotiated = true;
    state.viewerProtocol.version = Number(message.version || message.max_version || 1);
    state.viewerProtocol.capabilities = Array.isArray(message.capabilities)
      ? message.capabilities.map((capability) => String(capability || "").trim()).filter(Boolean)
      : [];
    state.viewerProtocol.authorityEpoch = state.viewerProtocol.capabilities.includes("prompt_control_result_v1")
      ? String(message.authority_epoch || "").trim() || null
      : null;
    state.auth.authorityEpoch = state.viewerProtocol.authorityEpoch;
  }

  function capabilitySelected() {
    return state.viewerProtocol?.negotiated === true
      && Array.isArray(state.viewerProtocol.capabilities)
      && state.viewerProtocol.capabilities.includes("prompt_control_result_v1");
  }

  function resultSelected() {
    return capabilitySelected() && !!String(state.viewerProtocol.authorityEpoch || "").trim();
  }

  function readinessError(agentId) {
    if (!capabilitySelected()) return null;
    if (!String(state.viewerProtocol.authorityEpoch || "").trim()) {
      return "prompt control is waiting for a current runtime authority epoch";
    }
    if (state.auth.registrationStatus !== "registered" || state.auth.sessionEpoch == null) {
      return "prompt control is waiting for a current registered player session";
    }
    if (String(state.auth.boundAgentId || "").trim() !== String(agentId || "").trim() || state.auth.bindingEpoch == null) {
      return "prompt control is waiting for a current Agent binding recovery ack";
    }
    if (!Number.isSafeInteger(Number(selectedAgentPromptProfile()?.version))) {
      return "prompt control is waiting for the authorized prompt version";
    }
    return null;
  }

  function attachIdentity(request) {
    if (!resultSelected()) return request;
    request.request_id = request.request_id || `pc-${String(nextRequestId()).padStart(8, "0")}`;
    request.session_epoch = Number(state.auth.sessionEpoch);
    request.binding_epoch = Number(state.auth.bindingEpoch);
    request.expected_authority_epoch = String(state.viewerProtocol.authorityEpoch || "").trim();
    return request;
  }

  async function buildAuthProof(mode, request, auth) {
    const nonce = nextAuthNonce();
    request.nonce = nonce;
    const payload = resultSelected()
      ? buildPromptControlSigningPayload(mode, request, auth)
      : {
          operation: mode === "preview" ? "prompt_control_preview" : "prompt_control_apply",
          agent_id: request.agent_id,
          player_id: auth.playerId,
          public_key: auth.publicKey,
          nonce,
          expected_version: request.expected_version ?? null,
          updated_by: request.updated_by ?? null,
          system_prompt_override: request.system_prompt_override,
          short_term_goal_override: request.short_term_goal_override,
          long_term_goal_override: request.long_term_goal_override,
        };
    const signingPayload = buildAuthEnvelope(payload);
    return {
      scheme: "ed25519",
      player_id: auth.playerId,
      public_key: auth.publicKey,
      nonce,
      signature: await signAuthPayload(signingPayload, auth),
    };
  }

  async function buildRollbackAuthProof(request, auth) {
    const nonce = nextAuthNonce();
    request.nonce = nonce;
    const payload = resultSelected()
      ? buildPromptControlSigningPayload("rollback", request, auth)
      : {
          operation: "prompt_control_rollback",
          agent_id: request.agent_id,
          player_id: auth.playerId,
          public_key: auth.publicKey,
          nonce,
          to_version: request.to_version,
          expected_version: request.expected_version ?? null,
          updated_by: request.updated_by ?? null,
        };
    const signingPayload = buildAuthEnvelope(payload);
    return {
      scheme: "ed25519",
      player_id: auth.playerId,
      public_key: auth.publicKey,
      nonce,
      signature: await signAuthPayload(signingPayload, auth),
    };
  }

  function draftValuesForRequest(request) {
    const valueForPatch = (patch, fallback) => patch?.mode === "clear"
      ? ""
      : patch?.mode === "set" ? String(patch.value || "") : String(fallback || "");
    return {
      agentId: String(request?.agent_id || "").trim(),
      systemPrompt: valueForPatch(request?.system_prompt_override, state.promptDraft.systemPrompt),
      shortTermGoal: valueForPatch(request?.short_term_goal_override, state.promptDraft.shortTermGoal),
      longTermGoal: valueForPatch(request?.long_term_goal_override, state.promptDraft.longTermGoal),
    };
  }

  function reconcilePendingAuthoritativeRefresh(snapshot) {
    const pending = pendingAuthoritativeRefresh;
    if (!pending) return false;
    const profile = snapshot?.model?.agent_prompt_profiles?.[pending.agentId];
    const version = Number(profile?.version);
    if (!Number.isFinite(version) || version < pending.version) return false;
    pendingAuthoritativeRefresh = null;
    if (selectedAgentId() !== pending.agentId) return false;
    const submitted = pending.submittedDraft;
    const draftChangedAfterSubmit = state.promptDraft.dirty && submitted && (
      String(state.promptDraft.systemPrompt || "") !== String(submitted.systemPrompt || "")
      || String(state.promptDraft.shortTermGoal || "") !== String(submitted.shortTermGoal || "")
      || String(state.promptDraft.longTermGoal || "") !== String(submitted.longTermGoal || "")
    );
    return !draftChangedAfterSubmit;
  }

  async function refreshBinding() {
    const agentId = String(state.auth.boundAgentId || selectedAgentId() || "").trim();
    if (!agentId) return { ok: false, reason: "prompt control recovery requires a selected Agent" };
    await ensureHostedPlayerAuthAvailable();
    if (!state.auth.available) {
      return { ok: false, reason: state.auth.error || "player session auth is unavailable" };
    }
    pendingAuthoritativeRefresh = null;
    state.auth.sessionEpoch = null;
    state.auth.bindingEpoch = null;
    state.auth.registrationStatus = "issued";
    state.auth.runtimeStatus = "recovery_pending_binding";
    state.auth.syncInFlight = false;
    state.auth.recoveryErrorCode = null;
    state.auth.recoveryErrorMessage = null;
    state.auth.error = null;
    state.auth.pendingRequestedAgentId = agentId;
    render();
    try {
      await ensureRegisteredPlayerSession(agentId);
      requestSnapshotSafe();
      render();
      return { ok: true, agentId, sessionEpoch: state.auth.sessionEpoch, bindingEpoch: state.auth.bindingEpoch };
    } catch (error) {
      state.auth.recoveryErrorCode = "prompt_binding_refresh_failed";
      state.auth.recoveryErrorMessage = String(error);
      state.auth.error = String(error);
      render();
      return { ok: false, agentId, reason: String(error) };
    }
  }

  function handleAck(ack) {
    const pending = state.lastPromptFeedback;
    if (!pending || (pending.requestAuthNonce != null && ack?.request_auth_nonce !== pending.requestAuthNonce) || (ack?.request_id != null && ack.request_id !== pending.requestId) || ack?.agent_id !== pending.agentId) return;
    clearPendingPromptControlAckTimer();
    const feedback = state.lastPromptFeedback || createSemanticFeedback("prompt", "prompt_ack", ack?.agent_id || null);
    const operation = String(ack?.operation || (ack?.preview ? "preview" : "apply"));
    const enhanced = ack?.status != null || ack?.request_id != null || ack?.authority_epoch != null;
    const resultStatus = String(ack?.status || "").trim().toLowerCase();
    if (enhanced && PROMPT_RESULT_STATUSES.has(resultStatus)) {
      feedback.stage = resultStatus;
      feedback.ok = resultStatus === "accepted" || resultStatus === "applied";
      feedback.accepted = feedback.ok;
      feedback.reason = ack?.reason_code || null;
      feedback.effect = `prompt ${resultStatus}; request=${ack?.request_id || feedback.requestId || "-"}`;
    } else {
      feedback.stage = ack?.preview ? "preview_ack" : operation === "rollback" ? "rollback_ack" : "apply_ack";
      feedback.ok = true;
      feedback.accepted = true;
      feedback.reason = null;
      feedback.effect = ack?.preview
        ? `prompt preview ready: version=${ack.version}`
        : operation === "rollback"
          ? `prompt rolled back via version=${ack.version} → target=${Number(ack?.rolled_back_to_version || 0)}`
          : `prompt applied: version=${ack.version}`;
    }
    if (ack?.request_id) feedback.requestId = ack.request_id;
    feedback.response = clone(ack);
    state.lastPromptFeedback = feedback;
    if (enhanced && resultStatus === "applied") {
      const version = Number(ack?.version);
      if (Number.isFinite(version)) {
        pendingAuthoritativeRefresh = {
          agentId: String(ack?.agent_id || feedback.agentId || "").trim(),
          requestId: ack?.request_id || feedback.requestId || null,
          version,
          submittedDraft: feedback.submittedDraft || null,
        };
        requestSnapshotSafe();
      }
    }
    if (ack?.preview || PROMPT_RESULT_STATUSES.has(resultStatus)) return;
    if (operation === "rollback") {
      if (enhanced) {
        requestSnapshotSafe();
        return;
      }
      state.promptDraft.currentVersion = Number(ack?.version || state.promptDraft.currentVersion || 0);
      state.promptDraft.rollbackTargetVersion = Math.max(0, state.promptDraft.currentVersion - 1);
      state.promptDraft.dirty = false;
      requestSnapshotSafe();
      return;
    }
    if (enhanced) requestSnapshotSafe();
    else applyPromptAckLocally(ack);
  }

  function handleError(error) {
    const pending = state.lastPromptFeedback;
    if (!pending || (pending.requestAuthNonce != null && error?.request_auth_nonce !== pending.requestAuthNonce)
      || (error?.agent_id != null && error.agent_id !== pending.agentId)
      || (pending.requestId != null && error?.request_id !== pending.requestId)) return;
    clearPendingPromptControlAckTimer();
    const controlLost = String(error?.code || error?.reason_code || "").trim().toLowerCase() === "agent_control_forbidden"
      || String(error?.code || error?.reason_code || "").trim().toLowerCase() === "control_lost";
    const feedback = state.lastPromptFeedback || createSemanticFeedback("prompt", "prompt_error", error?.agent_id || selectedAgentId());
    const status = String(error?.status || "").trim().toLowerCase();
    const reasonCode = String(error?.reason_code || error?.code || "").trim().toLowerCase();
    const hidden = String(error?.value_visibility || "").trim().toLowerCase() === "hidden";
    const enhanced = !!status || !!error?.request_id || !!error?.reason_code || hidden;
    const normalizedStatus = PROMPT_RESULT_STATUSES.has(status)
      ? status
      : reasonCode === "version_conflict"
        ? "stale"
      : reasonCode === "request_id_conflict"
        ? "rejected"
        : controlLost || (enhanced && hidden) ? "blocked" : "error";
    feedback.stage = normalizedStatus;
    feedback.ok = normalizedStatus === "accepted" || normalizedStatus === "applied";
    feedback.accepted = feedback.ok;
    feedback.reason = error?.reason_code || error?.code || error?.message || "prompt control failed";
    feedback.effect = enhanced ? `prompt ${normalizedStatus}` : error?.code || "prompt control error";
    if (error?.request_id) feedback.requestId = error.request_id;
    feedback.response = controlLost && onControlLost
      ? onControlLost(error?.agent_id || feedback.agentId || selectedAgentId())
      : clone(error);
    if (feedback.response && !feedback.response.status && ["stale", "rejected"].includes(normalizedStatus)) {
      feedback.response.status = normalizedStatus;
    }
    state.lastPromptFeedback = feedback;
  }

  return {
    attachIdentity,
    buildAuthProof,
    buildRollbackAuthProof,
    capabilitySelected,
    draftValuesForRequest,
    handleAck,
    handleError,
    handleHelloAck,
    readinessError,
    reconcilePendingAuthoritativeRefresh,
    refreshBinding,
    resetForConnection,
    resultSelected,
    clearPendingAuthoritativeRefresh() {
      pendingAuthoritativeRefresh = null;
    },
  };
}
