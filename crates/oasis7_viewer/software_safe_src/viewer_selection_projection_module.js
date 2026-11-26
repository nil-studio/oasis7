export function createViewerSelectionProjectionModule({
  state,
  STARTER_AGENT_ID,
  LOCAL_TEST_PLAYER_ID_PREFIX,
  isTestApiEnabled,
  isAgentControlLost,
  render,
}) {
  function parseSelectionPayload(payload) {
    if (payload == null) return null;
    if (typeof payload === "string") {
      const trimmed = payload.trim();
      if (!trimmed) return null;
      const parts = trimmed.split(":");
      if (parts.length >= 2) return { kind: parts[0], id: parts.slice(1).join(":") };
      return { kind: "agent", id: trimmed };
    }
    if (typeof payload === "object") {
      const kind = payload.kind || payload.targetKind || payload.type;
      const id = payload.id || payload.targetId || payload.value;
      if (!kind || !id) return null;
      return { kind: String(kind), id: String(id) };
    }
    return null;
  }

  function normalizeModuleVisualEntity(entry, fallbackId = "") {
    if (!entry || typeof entry !== "object") return null;
    const id = String(entry.entity_id || entry.id || fallbackId || "").trim();
    if (!id) return null;
    return {
      ...entry,
      id,
      module_id: String(entry.module_id || entry.moduleId || "").trim(),
      kind: String(entry.kind || "artifact").trim() || "artifact",
      label: entry.label == null ? null : String(entry.label).trim() || null,
      anchor: entry.anchor || null,
    };
  }

  function entityCollections() {
    const model = state.snapshot?.model || {};
    return {
      agents: Object.values(model.agents || {}),
      locations: Object.values(model.locations || {}),
      moduleVisualEntities: Object.values(model.module_visual_entities || {})
        .map((entry) => normalizeModuleVisualEntity(entry))
        .filter(Boolean),
    };
  }

  function moduleVisualEntityIdFromEvent(event) {
    if (!event || typeof event !== "object" || typeof event.module_visual_entity_id !== "string") return null;
    const id = event.module_visual_entity_id.trim();
    return id || null;
  }

  function moduleVisualEntityFromEvent(event) {
    const id = moduleVisualEntityIdFromEvent(event);
    return id ? entityCollections().moduleVisualEntities.find((entry) => entry.id === id) || null : null;
  }

  function focusModuleFromEvent(event) {
    const target = moduleVisualEntityFromEvent(event);
    return target ? applySelection({ kind: "module_visual", id: target.id }) : null;
  }

  function agentBindingForId(agentId) {
    const id = String(agentId || "").trim();
    if (!id) return { playerId: null, publicKey: null };
    return {
      playerId: state.snapshot?.model?.agent_player_bindings?.[id] || null,
      publicKey: state.snapshot?.model?.agent_player_public_key_bindings?.[id] || null,
    };
  }

  function isAgentVisibleToCurrentSession(agentId) {
    const id = String(agentId || "").trim();
    if (!id) return false;
    const boundAgentId = String(state.auth.boundAgentId || "").trim();
    const currentPlayerId = String(state.auth.playerId || "").trim();
    const binding = agentBindingForId(id);
    const boundPlayerId = String(binding.playerId || "").trim();
    if (boundAgentId && id === boundAgentId) return true;
    if (boundPlayerId && currentPlayerId && boundPlayerId === currentPlayerId) return true;
    if (
      id === STARTER_AGENT_ID
      && isTestApiEnabled()
      && state.auth.source === "local_test_api_ephemeral"
      && boundPlayerId.startsWith(LOCAL_TEST_PLAYER_ID_PREFIX)
    ) {
      return true;
    }
    return false;
  }

  function currentBoundAgentControlError(agentId, actionLabel = "agent action") {
    const id = String(agentId || "").trim();
    if (!id) return `${actionLabel} requires a non-empty agent id`;
    const boundAgentId = String(state.auth.boundAgentId || "").trim();
    if (!boundAgentId) return `${actionLabel} requires the current account to have a bound Agent`;
    if (id !== boundAgentId) return `${actionLabel} target ${id} does not match current bound Agent ${boundAgentId}`;
    if (isAgentControlLost(id)) return `${actionLabel} control was lost; re-authenticate and refresh the current Agent binding`;
    return null;
  }

  function selectedAgentId() {
    return state.selectedKind === "agent" ? state.selectedId : null;
  }

  function selectedAgentPromptProfile() {
    const agentId = selectedAgentId();
    if (!agentId) return null;
    return state.snapshot?.model?.agent_prompt_profiles?.[agentId] || {
      agent_id: agentId,
      version: 0,
      updated_at_tick: 0,
      updated_by: "",
      system_prompt_override: null,
      short_term_goal_override: null,
      long_term_goal_override: null,
    };
  }

  function selectedAgentBindingInfo() {
    const agentId = selectedAgentId();
    return agentId ? agentBindingForId(agentId) : null;
  }

  function selectedAgentExecutionDebugContext() {
    const agentId = selectedAgentId();
    return agentId ? state.snapshot?.model?.agent_execution_debug_contexts?.[agentId] || null : null;
  }

  function syncAgentInteractionDrafts(force = false) {
    const agentId = selectedAgentId();
    const profile = selectedAgentPromptProfile();
    if (force || state.promptDraft.agentId !== agentId || (!state.promptDraft.dirty && agentId)) {
      const currentVersion = Number(profile?.version || 0);
      state.promptDraft = {
        agentId,
        currentVersion,
        rollbackTargetVersion: Math.max(0, currentVersion - 1),
        updatedBy: String(profile?.updated_by || ""),
        updatedAtTick: Number(profile?.updated_at_tick || 0),
        systemPrompt: String(profile?.system_prompt_override || ""),
        shortTermGoal: String(profile?.short_term_goal_override || ""),
        longTermGoal: String(profile?.long_term_goal_override || ""),
        dirty: false,
      };
    }
    if (force || state.chatDraft.agentId !== agentId) {
      state.chatDraft = {
        agentId,
        message: agentId === state.chatDraft.agentId ? state.chatDraft.message : "",
        dirty: false,
      };
    }
  }

  function applySelection(selection, options = {}) {
    if (!selection) return null;
    const kind = String(selection.kind || "").toLowerCase();
    const id = String(selection.id || "");
    const { agents, locations, moduleVisualEntities } = entityCollections();
    let object = null;
    if (kind === "agent") object = agents.find((entry) => entry.id === id) || null;
    else if (kind === "location") object = locations.find((entry) => entry.id === id) || null;
    else if (kind === "module_visual") object = moduleVisualEntities.find((entry) => entry.id === id) || null;
    if (!object) return null;
    state.selectedKind = kind;
    state.selectedId = id;
    state.selectedObject = object;
    syncAgentInteractionDrafts(options.preserveInteractionDrafts !== true);
    render();
    return { kind, id };
  }

  function select(payload) {
    const parsed = parseSelectionPayload(payload);
    if (!parsed) return { ok: false, reason: "invalid selection payload" };
    const applied = applySelection(parsed);
    if (!applied) return { ok: false, reason: `target not found: ${parsed.kind}:${parsed.id}` };
    return { ok: true, ...applied };
  }

  function focus(payload) {
    return select(payload);
  }

  function parseStepCount(payload) {
    if (payload == null) return 1;
    if (typeof payload === "number" && Number.isFinite(payload) && payload >= 1) return Math.floor(payload);
    if (typeof payload === "string") {
      const trimmed = payload.trim();
      if (!trimmed || trimmed === "step") return 1;
      const numeric = Number(trimmed);
      if (Number.isFinite(numeric) && numeric >= 1) return Math.floor(numeric);
      const matched = trimmed.match(/step\s*[:=]\s*(\d+)/i);
      return matched ? Number(matched[1]) : null;
    }
    if (typeof payload === "object") {
      const numeric = Number(payload.count);
      if (Number.isFinite(numeric) && numeric >= 1) return Math.floor(numeric);
    }
    return null;
  }

  return {
    parseSelectionPayload, entityCollections, normalizeModuleVisualEntity,
    moduleVisualEntityIdFromEvent, moduleVisualEntityFromEvent, focusModuleFromEvent,
    agentBindingForId, isAgentVisibleToCurrentSession, currentBoundAgentControlError,
    selectedAgentId, selectedAgentPromptProfile, selectedAgentBindingInfo,
    selectedAgentExecutionDebugContext, syncAgentInteractionDrafts,
    applySelection, select, focus, parseStepCount,
  };
}
