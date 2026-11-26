import { isHostedPublicJoinDeploymentMode } from "./software_safe_constants.js";

export function createViewerPromptRequestModule({
  state,
  getViewerPromptControlModule,
  semanticActionOverallTimeoutMs,
  selectedAgentId,
  selectedAgentPromptProfile,
  currentBoundAgentControlError,
  promptControlCapabilitySelected,
  promptControlReadinessError,
  attachPromptControlIdentity,
  promptDraftValuesForRequest,
  nextRequestId,
  createSemanticFeedback,
  snapshotSemanticFeedback,
  enqueueSemanticCommand,
  ensureHostedPlayerAuthAvailable,
  assertPromptFeedbackActive,
  assertSemanticCapability,
  ensureRegisteredPlayerSession,
  requestHostedStrongAuthGrant,
  buildPromptControlAuthProof,
  buildPromptRollbackAuthProof,
  sendJson,
  schedulePromptControlAckTimeout,
  acceptRuntimeAckIdentity,
  render,
}) {
  function promptPatchFromDraft(currentValue, draftValue) {
    const current = currentValue == null ? "" : String(currentValue);
    const draft = String(draftValue ?? "");
    if (draft === current) return { mode: "unchanged" };
    if (draft.length === 0) return currentValue == null ? { mode: "unchanged" } : { mode: "clear" };
    return { mode: "set", value: draft };
  }

  function buildPromptRequestFromDraft(agentId, draftOverrides) {
    const currentProfile = selectedAgentPromptProfile();
    if (!agentId || !currentProfile) throw new Error("select an agent before editing prompt overrides");
    const request = {
      agent_id: agentId,
      player_id: state.auth.playerId,
      public_key: state.auth.publicKey,
      expected_version: Number(currentProfile.version || 0),
      updated_by: state.auth.playerId,
      system_prompt_override: promptPatchFromDraft(currentProfile.system_prompt_override, draftOverrides.systemPrompt),
      short_term_goal_override: promptPatchFromDraft(currentProfile.short_term_goal_override, draftOverrides.shortTermGoal),
      long_term_goal_override: promptPatchFromDraft(currentProfile.long_term_goal_override, draftOverrides.longTermGoal),
    };
    if (promptControlCapabilitySelected()) request.request_id = `pc-${String(nextRequestId()).padStart(8, "0")}`;
    return request;
  }

  function encodePromptRequestForJson(request) {
    const encodePatch = (patch) => {
      if (!patch || patch.mode === "unchanged") return undefined;
      if (patch.mode === "clear") return null;
      return patch.value;
    };
    return {
      agent_id: request.agent_id,
      player_id: request.player_id,
      ...(request.request_id ? { request_id: request.request_id } : {}),
      ...(request.session_epoch != null ? { session_epoch: request.session_epoch } : {}),
      ...(request.binding_epoch != null ? { binding_epoch: request.binding_epoch } : {}),
      ...(request.expected_authority_epoch ? { expected_authority_epoch: request.expected_authority_epoch } : {}),
      public_key: request.public_key,
      expected_version: request.expected_version,
      updated_by: request.updated_by,
      system_prompt_override: encodePatch(request.system_prompt_override),
      short_term_goal_override: encodePatch(request.short_term_goal_override),
      long_term_goal_override: encodePatch(request.long_term_goal_override),
    };
  }

  function buildPromptRollbackRequest(agentId, toVersion) {
    const profile = selectedAgentPromptProfile();
    const targetVersion = Number(toVersion);
    if (!agentId || !profile) throw new Error("select an agent before rolling back prompt overrides");
    if (!Number.isInteger(targetVersion) || targetVersion < 0) {
      throw new Error("prompt rollback requires integer toVersion >= 0");
    }
    const request = {
      agent_id: agentId,
      player_id: state.auth.playerId,
      public_key: state.auth.publicKey,
      to_version: targetVersion,
      expected_version: Number(profile.version || 0),
      updated_by: state.auth.playerId,
    };
    if (promptControlCapabilitySelected()) request.request_id = `pc-${String(nextRequestId()).padStart(8, "0")}`;
    return request;
  }

  function sendPromptControl(mode, payload = null) {
    const normalizedMode = String(mode || "").trim().toLowerCase();
    if (!["preview", "apply", "rollback"].includes(normalizedMode)) {
      return { ok: false, reason: "prompt control mode must be preview, apply, or rollback" };
    }
    const selectedId = selectedAgentId();
    const agentId = String(payload?.agentId || payload?.agent_id || selectedId || "");
    if (!agentId) return { ok: false, reason: "prompt control requires a selected agent or explicit agentId" };
    const controlError = currentBoundAgentControlError(agentId, "prompt_control");
    if (controlError) return { ok: false, reason: controlError };
    const protocolError = promptControlReadinessError(agentId);
    if (protocolError) return { ok: false, reason: protocolError };
    let request;
    try {
      if (normalizedMode === "rollback") {
        const currentVersion = Number(state.promptDraft.currentVersion || selectedAgentPromptProfile()?.version || 0);
        const fallbackVersion = Math.max(0, currentVersion - 1);
        const toVersion = payload?.toVersion ?? payload?.to_version ?? fallbackVersion;
        request = buildPromptRollbackRequest(agentId, toVersion);
      } else {
        request = buildPromptRequestFromDraft(agentId, {
          systemPrompt: payload?.systemPrompt ?? payload?.system_prompt_override ?? state.promptDraft.systemPrompt,
          shortTermGoal: payload?.shortTermGoal ?? payload?.short_term_goal_override ?? state.promptDraft.shortTermGoal,
          longTermGoal: payload?.longTermGoal ?? payload?.long_term_goal_override ?? state.promptDraft.longTermGoal,
        });
      }
    } catch (error) {
      return { ok: false, reason: String(error) };
    }

    const createdFeedback = createSemanticFeedback("prompt", `prompt_${normalizedMode}`, agentId, {
      effect: "queued for signing and send",
      toVersion: request.to_version ?? null,
      requestId: request.request_id || null,
    });
    state.lastPromptFeedback = createdFeedback;
    const feedback = state.lastPromptFeedback;
    enqueueSemanticCommand({
      kind: "prompt",
      feedback,
      timeoutMs: semanticActionOverallTimeoutMs,
      execute: async () => {
        await ensureHostedPlayerAuthAvailable();
        assertPromptFeedbackActive(feedback);
        assertSemanticCapability("prompt_control");
        feedback.stage = "registering"; feedback.effect = "registering player session"; render();
        await ensureRegisteredPlayerSession(agentId);
        assertPromptFeedbackActive(feedback);
        attachPromptControlIdentity(request);
        let strongAuthGrant = null;
        if (isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)) {
          feedback.stage = "authorizing"; feedback.effect = "requesting backend strong-auth grant"; render();
          strongAuthGrant = await requestHostedStrongAuthGrant(
            normalizedMode === "rollback" ? "prompt_control_rollback" : `prompt_control_${normalizedMode}`,
            agentId,
          );
          assertPromptFeedbackActive(feedback);
        }
        feedback.stage = "signing"; feedback.effect = "building auth proof"; render();
        let commandRequest;
        if (normalizedMode === "rollback") {
          commandRequest = { ...request, auth: await buildPromptRollbackAuthProof(request, state.auth) };
          if (strongAuthGrant) commandRequest.strong_auth_grant = strongAuthGrant;
        } else {
          commandRequest = encodePromptRequestForJson(request);
          commandRequest.auth = await buildPromptControlAuthProof(normalizedMode, request, state.auth);
          if (strongAuthGrant) commandRequest.strong_auth_grant = strongAuthGrant;
        }
        assertPromptFeedbackActive(feedback);
        feedback.stage = "sent";
        feedback.effect = `prompt ${normalizedMode} request sent; waiting for ack`;
        state.lastPromptFeedback = feedback;
        feedback.requestId = request.request_id || null;
        feedback.requestAuthNonce = commandRequest.auth?.nonce;
        feedback.submittedDraft = promptDraftValuesForRequest(request);
        sendJson({ type: "prompt_control", command: { mode: normalizedMode, request: commandRequest } });
        schedulePromptControlAckTimeout(feedback);
        render();
      },
    });
    render();
    return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
  }

  function applyPromptAckLocally(ack) {
    const agentId = ack?.agent_id;
    if (!agentId || !state.snapshot?.model) return;
    if (!state.snapshot.model.agent_prompt_profiles) state.snapshot.model.agent_prompt_profiles = {};
    const current = state.snapshot.model.agent_prompt_profiles[agentId] || { agent_id: agentId };
    const nextProfile = {
      ...current,
      agent_id: agentId,
      version: Number(ack.version || current.version || 0),
      updated_at_tick: Number(ack.updated_at_tick || state.logicalTime),
      updated_by: state.auth.playerId || current.updated_by || "",
    };
    if (!ack.preview) {
      nextProfile.system_prompt_override = state.promptDraft.systemPrompt || null;
      nextProfile.short_term_goal_override = state.promptDraft.shortTermGoal || null;
      nextProfile.long_term_goal_override = state.promptDraft.longTermGoal || null;
    }
    state.snapshot.model.agent_prompt_profiles[agentId] = nextProfile;
    if (selectedAgentId() === agentId) {
      state.promptDraft = {
        agentId,
        currentVersion: nextProfile.version,
        rollbackTargetVersion: Math.max(0, Number(nextProfile.version || 0) - 1),
        updatedBy: nextProfile.updated_by,
        updatedAtTick: nextProfile.updated_at_tick,
        systemPrompt: String(nextProfile.system_prompt_override || ""),
        shortTermGoal: String(nextProfile.short_term_goal_override || ""),
        longTermGoal: String(nextProfile.long_term_goal_override || ""),
        dirty: false,
      };
    }
  }

  function handlePromptControlAck(ack) {
    if (acceptRuntimeAckIdentity(ack)) getViewerPromptControlModule()?.handleAck(ack);
  }
  function handlePromptControlError(error) {
    getViewerPromptControlModule()?.handleError(error);
  }

  return {
    sendPromptControl,
    applyPromptAckLocally,
    handlePromptControlAck,
    handlePromptControlError,
  };
}
