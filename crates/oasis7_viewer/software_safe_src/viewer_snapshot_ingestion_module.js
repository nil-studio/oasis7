export function createViewerSnapshotIngestionModule({
  state,
  windowRef = window,
  MAX_EVENTS,
  MAX_DECISION_TRACES,
  EMPTY_ENTITY_SNAPSHOT_REFRESH_DELAY_MS,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  STARTER_AGENT_ID,
  LOCAL_TEST_PLAYER_ID_PREFIX,
  isWorldScopedCrisisRuntimeEvent,
  onSnapshotReceived,
  clearInitialSnapshotRetryTimer,
  quoteProtocolFacade,
  recoverGameplayFeedback,
  hostedWorldAccount,
  getGameplayRecoveryScope,
  clearPendingGameplayActionAckTimer,
  requestGameplayResult,
  sendJson,
  entityCollections,
  applySelection,
  syncAgentInteractionDrafts,
  hydrateChatHistoryFromStorage,
  reconcilePendingPromptAuthoritativeRefresh,
  canonicalOwnerRead,
  worldFeedTransport,
  isTestApiEnabled,
  getSocket,
  requestSnapshotSafe,
  persistHostedPlayerSession,
  hasPendingSessionRegisterWaiter,
  ensureRegisteredPlayerSession,
  clone,
  render,
  getState,
  acceptRuntimeAckIdentity,
  pendingControlFeedback,
  liveControlState,
}) {
  let emptyEntitySnapshotRefreshTimer = null;
  let localTestStarterRebindAttemptKey = null;

  function summarizeEventTitle(event) {
    const kind = event?.kind?.type || "unknown";
    return kind.replace(/_/g, " ");
  }

  function addRecentEvent(event) {
    state.eventSeq = Math.max(state.eventSeq, Number(event?.id || 0));
    if (isWorldScopedCrisisRuntimeEvent(event)) return;
    state.recentEvents.unshift(event);
    state.recentEvents = state.recentEvents.slice(0, MAX_EVENTS);
    state.eventCount = state.recentEvents.length;
  }

  function normalizedGameplayActions(snapshot = state.snapshot) {
    const actions = snapshot?.player_gameplay?.available_actions
      || snapshot?.player_gameplay?.availableActions
      || [];
    return Array.isArray(actions) ? actions : [];
  }

  function gameplayActionId(action) {
    return String(action?.action_id || action?.actionId || "").trim();
  }

  function gameplayProtocolAction(action) {
    return String(action?.protocol_action || action?.protocolAction || "").trim();
  }

  function hasGameplayAction(snapshot, actionId) {
    return normalizedGameplayActions(snapshot).some((action) => gameplayActionId(action) === actionId);
  }

  function hasSnapshotRefreshAction(snapshot) {
    return normalizedGameplayActions(snapshot).some((action) => {
      const protocol = gameplayProtocolAction(action);
      return gameplayActionId(action) === "request_snapshot"
        || protocol === "request_snapshot"
        || protocol === "world.request_snapshot";
    });
  }

  function needsEmptyEntitySnapshotRefresh(snapshot = state.snapshot) {
    const gameplay = snapshot?.player_gameplay || {};
    const blockerKind = String(gameplay.blocker_kind || gameplay.blockerKind || "").trim();
    if (blockerKind !== "runtime_snapshot_empty_entities") return false;
    if (hasGameplayAction(snapshot, "claim_first_agent")) return false;
    return hasSnapshotRefreshAction(snapshot);
  }

  function clearEmptyEntitySnapshotRefreshTimer() {
    if (emptyEntitySnapshotRefreshTimer) {
      windowRef.clearTimeout(emptyEntitySnapshotRefreshTimer);
      emptyEntitySnapshotRefreshTimer = null;
    }
  }

  function needsEmptyEntitySnapshotRefreshForTest(snapshot = state.snapshot) {
    if (!isTestApiEnabled()) throw new Error("needsEmptyEntitySnapshotRefreshForTest requires test_api=1");
    return needsEmptyEntitySnapshotRefresh(snapshot);
  }

  function isEmptyEntitySnapshotRefreshPendingForTest() {
    if (!isTestApiEnabled()) throw new Error("isEmptyEntitySnapshotRefreshPendingForTest requires test_api=1");
    return Boolean(emptyEntitySnapshotRefreshTimer);
  }

  function syncEmptyEntitySnapshotRefreshLoop() {
    if (!needsEmptyEntitySnapshotRefresh()) {
      clearEmptyEntitySnapshotRefreshTimer();
      return;
    }
    const socket = getSocket();
    if (emptyEntitySnapshotRefreshTimer || !socket || socket.readyState !== WebSocket.OPEN) return;
    emptyEntitySnapshotRefreshTimer = windowRef.setTimeout(() => {
      emptyEntitySnapshotRefreshTimer = null;
      const activeSocket = getSocket();
      if (!needsEmptyEntitySnapshotRefresh() || !activeSocket || activeSocket.readyState !== WebSocket.OPEN) return;
      requestSnapshotSafe();
      syncEmptyEntitySnapshotRefreshLoop();
    }, EMPTY_ENTITY_SNAPSHOT_REFRESH_DELAY_MS);
  }

  function adoptCurrentPlayerBindingFromSnapshot(snapshot) {
    // Public player-to-Agent relations do not prove the current signing key's
    // control. Hosted/WorldService sessions adopt canonical registration ACKs.
    if (state.auth.source === "hosted_account_cookie" || state.viewerProtocol?.capabilities?.includes("canonical_agent_chat_v1")) return;
    const playerId = String(state.auth.playerId || "").trim();
    if (!playerId) return;
    const bindings = snapshot?.model?.agent_player_bindings || {};
    const boundAgentId = Object.entries(bindings)
      .find(([, boundPlayerId]) => String(boundPlayerId || "").trim() === playerId)?.[0] || null;
    if (!boundAgentId || state.auth.boundAgentId === boundAgentId) return;
    state.auth.boundAgentId = boundAgentId;
    state.auth.pendingRequestedAgentId = boundAgentId;
    state.auth.registrationStatus = "registered";
    state.auth.runtimeStatus = "registered";
    state.auth.error = null;
    if (state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      persistHostedPlayerSession(state.auth);
    }
  }

  function maybeRecoverLocalTestStarterBindingFromSnapshot(snapshot) {
    if (!isTestApiEnabled() || state.auth.source !== "local_test_api_ephemeral" || !state.auth.available) return;
    if (state.auth.boundAgentId || state.auth.syncInFlight || hasPendingSessionRegisterWaiter()) return;
    const socket = getSocket();
    if (!socket || socket.readyState !== WebSocket.OPEN) return;
    const agents = snapshot?.model?.agents || {};
    if (!agents[STARTER_AGENT_ID]) return;
    const currentPlayerId = String(state.auth.playerId || "").trim();
    const boundPlayerId = String(snapshot?.model?.agent_player_bindings?.[STARTER_AGENT_ID] || "").trim();
    if (!currentPlayerId || !boundPlayerId.startsWith(LOCAL_TEST_PLAYER_ID_PREFIX) || boundPlayerId === currentPlayerId) return;
    const attemptKey = `${currentPlayerId}:${boundPlayerId}:${STARTER_AGENT_ID}`;
    if (localTestStarterRebindAttemptKey === attemptKey) return;
    localTestStarterRebindAttemptKey = attemptKey;
    state.auth.pendingRequestedAgentId = STARTER_AGENT_ID;
    state.auth.pendingForceRebind = true;
    state.auth.rebindNotice = `Local test player is taking over ${STARTER_AGENT_ID} from an earlier local test session.`;
    void ensureRegisteredPlayerSession(STARTER_AGENT_ID, { forceRebind: true }).catch((error) => {
      localTestStarterRebindAttemptKey = null;
      state.auth.syncInFlight = false;
      state.auth.pendingForceRebind = false;
      state.auth.runtimeStatus = "error";
      if (state.auth.recoveryErrorCode !== "session_register_timeout") {
        state.auth.recoveryErrorCode = "local_test_starter_rebind_failed";
        state.auth.recoveryErrorMessage = String(error);
      }
      state.auth.error = String(error);
      render();
    });
  }

  function handleSnapshot(snapshot) {
    onSnapshotReceived();
    clearInitialSnapshotRetryTimer(); quoteProtocolFacade.invalidateSnapshotBoundQuotes(snapshot);
    state.snapshot = snapshot;
    recoverGameplayFeedback(snapshot, state, hostedWorldAccount, getGameplayRecoveryScope(state), clearPendingGameplayActionAckTimer);
    requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson);
    state.logicalTime = Math.max(state.logicalTime, Number(snapshot?.time || 0));
    state.tick = state.logicalTime;
    adoptCurrentPlayerBindingFromSnapshot(snapshot);
    maybeRecoverLocalTestStarterBindingFromSnapshot(snapshot);
    const { agents, locations } = entityCollections();
    if (!state.selectedObject) {
      if (agents[0]) applySelection({ kind: "agent", id: agents[0].id });
      else if (locations[0]) applySelection({ kind: "location", id: locations[0].id });
    } else if (state.selectedKind && state.selectedId) {
      if (!applySelection({ kind: state.selectedKind, id: state.selectedId }, { preserveInteractionDrafts: true })) {
        state.selectedKind = null; state.selectedId = null; state.selectedObject = null; syncAgentInteractionDrafts(true);
      }
    }
    hydrateChatHistoryFromStorage();
    syncAgentInteractionDrafts(reconcilePendingPromptAuthoritativeRefresh(snapshot));
    void canonicalOwnerRead.refresh().catch(() => {}); syncEmptyEntitySnapshotRefreshLoop(); worldFeedTransport.refreshAfterSnapshot();
  }

  function injectSnapshot(snapshot, options = {}) {
    if (!isTestApiEnabled()) throw new Error("injectSnapshot requires test_api=1");
    handleSnapshot(clone(snapshot));
    render();
    return options?.returnState === false ? { ok: true } : getState();
  }

  function handleMetrics(time, metrics) {
    state.metrics = metrics || null;
    state.traceCount = Number(metrics?.decision_trace_count || 0);
    state.logicalTime = Math.max(state.logicalTime, Number(time || 0), Number(metrics?.total_ticks || 0));
    state.tick = state.logicalTime;
  }

  function clipTraceText(value, limit = 480) {
    const text = String(value || "").trim();
    if (!text) return null;
    return text.length <= limit ? text : `${text.slice(0, limit)}…`;
  }

  function snapshotDecisionTrace(trace) {
    if (!trace || typeof trace !== "object") return null;
    return {
      agent_id: trace.agent_id || null,
      time: Number(trace.time || 0),
      decision: clone(trace.decision || null),
      llm_error: trace.llm_error || null,
      parse_error: trace.parse_error || null,
      llm_input_excerpt: clipTraceText(trace.llm_input),
      llm_output_excerpt: clipTraceText(trace.llm_output),
      llm_diagnostics: clone(trace.llm_diagnostics || null),
    };
  }

  function handleDecisionTrace(trace) {
    if (!trace || typeof trace !== "object") return;
    state.recentDecisionTraces.unshift(clone(trace));
    state.recentDecisionTraces = state.recentDecisionTraces.slice(0, MAX_DECISION_TRACES);
    state.traceCount = Math.max(state.traceCount, state.recentDecisionTraces.length);
    state.logicalTime = Math.max(state.logicalTime, Number(trace?.time || 0));
    state.tick = state.logicalTime;
  }

  function handleControlCompletionAck(ack) {
    if (!acceptRuntimeAckIdentity(ack)) return;
    const feedback = pendingControlFeedback.get(ack?.request_id);
    if (!feedback) return;
    liveControlState.blocked(ack);
    feedback.deltaLogicalTime = Number(ack?.delta_logical_time || 0);
    feedback.deltaEventSeq = Number(ack?.delta_event_seq || 0);
    if (ack?.status === "advanced") {
      feedback.stage = "completed_advanced";
      feedback.effect = `control ack advanced: logicalTime +${feedback.deltaLogicalTime}, eventSeq +${feedback.deltaEventSeq}`;
      feedback.reason = null;
    } else if (ack?.status === "blocked") {
      feedback.stage = "blocked";
      feedback.reason = ack?.error_message || ack?.error_code || "control was blocked before runtime advance";
      feedback.hint = state.snapshot?.player_gameplay?.next_step_hint || feedback.hint;
      feedback.effect = `gameplay blocked before requested advance completed: logicalTime +${feedback.deltaLogicalTime}, eventSeq +${feedback.deltaEventSeq}`;
    } else {
      feedback.stage = "completed_no_progress";
      feedback.reason = "timeout_no_progress";
      feedback.effect = `no visible world delta: logicalTime +${feedback.deltaLogicalTime}, eventSeq +${feedback.deltaEventSeq}`;
    }
    state.lastControlFeedback = feedback;
    pendingControlFeedback.delete(feedback.requestId);
    if (feedback.stage === "completed_advanced") requestSnapshotSafe();
    render();
  }

  return {
    summarizeEventTitle, addRecentEvent, handleSnapshot, normalizedGameplayActions, gameplayActionId,
    gameplayProtocolAction, hasGameplayAction, hasSnapshotRefreshAction, needsEmptyEntitySnapshotRefresh,
    clearEmptyEntitySnapshotRefreshTimer, needsEmptyEntitySnapshotRefreshForTest,
    isEmptyEntitySnapshotRefreshPendingForTest, syncEmptyEntitySnapshotRefreshLoop,
    adoptCurrentPlayerBindingFromSnapshot, maybeRecoverLocalTestStarterBindingFromSnapshot,
    injectSnapshot, handleMetrics, snapshotDecisionTrace, handleDecisionTrace, handleControlCompletionAck,
  };
}
