import { receiveGameplayResult } from "./viewer_gameplay_result_recovery.js";
import { getGameplayRecoveryScope } from "./viewer_gameplay_recovery_scope.js";
import { LIVE_CONTROL_STATE_CAPABILITY } from "./viewer_live_control_state.js";
import { invalidateAuthConnection } from "./viewer_auth_session_module.js";
import { LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE } from "./software_safe_constants.js";

export function createViewerWsTransportModule({
  state, getSocket, setSocket, getViewerPromptControlModule,
  helloAckTimeoutMs, initialSnapshotRetryDelayMs, initialSnapshotSlowRetryDelayMs,
  initialSnapshotSlowRetryAfter, promptControlResultCapability,
  sendJson, initialWsUrl, render, reportFatalError, resetViewerProtocolForConnection,
  worldFeedTransport, worldViewTransport, scopeSnapshotTransport, quoteProtocolFacade,
  liveControlState, hostedWorldAccount, safeAlternative, canonicalOwnerRead,
  hydrateChatHistoryFromStorage, ensureHostedPlayerAuthAvailable, syncHostedPlayerSessionOnConnect,
  handleSnapshot, addRecentEvent, extractAgentSpokeEntry, pushChatHistory,
  handleMetrics, handleDecisionTrace, handleControlCompletionAck,
  handlePromptControlAck, handlePromptControlError, handleAgentChatAck, handleAgentChatError,
  clearPendingGameplayActionAckTimer, requestSnapshotSafe,
  handleGameplayActionAck, handleGameplayActionError,
  handleAuthoritativeRecoveryAck, handleAuthoritativeRecoveryError,
  cancelHostedLeaseRefresh, clearHostedRuntimeSyncTimer, clearPendingSessionRegisterWaiter,
  failPendingAgentChatAck, failPendingPromptControlAck, failPendingGameplayActionAck,
  stopHostedSessionRefreshLoop, syncHostedSessionRefreshLoop,
}) {
  let reconnectTimer = null;
  let helloAckTimer = null;
  let initialSnapshotRequested = false;
  let currentConnectionSnapshotReceived = false;
  let initialSnapshotReadFailed = false;
  let initialSnapshotRetryTimer = null;
  let initialSnapshotRetryCount = 0;

  function clearInitialSnapshotRetryTimer() {
    if (initialSnapshotRetryTimer) {
      window.clearTimeout(initialSnapshotRetryTimer);
      initialSnapshotRetryTimer = null;
    }
  }

  function clearHelloAckTimer() {
    if (helloAckTimer) {
      window.clearTimeout(helloAckTimer);
      helloAckTimer = null;
    }
  }

  function closeSocketForReconnect(targetSocket) {
    if (!targetSocket || targetSocket.readyState === WebSocket.CLOSING || targetSocket.readyState === WebSocket.CLOSED) return;
    try { targetSocket.close(); } catch (_) {}
  }

  function scheduleHelloAckTimeout(targetSocket) {
    clearHelloAckTimer();
    helloAckTimer = window.setTimeout(() => {
      helloAckTimer = null;
      if (getSocket() !== targetSocket || state.server || targetSocket.readyState !== WebSocket.OPEN) return;
      closeSocketForReconnect(targetSocket);
    }, helloAckTimeoutMs);
  }

  function sendInitialSnapshotRequest() {
    sendJson({ type: "subscribe", streams: ["snapshot", "events", "metrics"], event_kinds: [] });
    worldFeedTransport.requestWorldFeed({ cursor: null });
    sendJson({ type: "request_snapshot" });
  }

  function scheduleInitialSnapshotRetry() {
    clearInitialSnapshotRetryTimer();
    if (currentConnectionSnapshotReceived) return;
    if (initialSnapshotRetryCount >= 8) {
      initialSnapshotReadFailed = true;
      state.lastError = "World snapshot unavailable. Refresh the snapshot to retry.";
      render();
      return;
    }
    const retryDelay = initialSnapshotRetryCount >= initialSnapshotSlowRetryAfter
      ? initialSnapshotSlowRetryDelayMs : initialSnapshotRetryDelayMs;
    initialSnapshotRetryTimer = window.setTimeout(() => {
      initialSnapshotRetryTimer = null;
      const socket = getSocket();
      if (currentConnectionSnapshotReceived || !socket || socket.readyState !== WebSocket.OPEN) return;
      initialSnapshotRetryCount += 1;
      try { sendInitialSnapshotRequest(); } catch (_) {} finally { scheduleInitialSnapshotRetry(); }
    }, retryDelay);
  }

  function markSnapshotReceived() {
    if (initialSnapshotReadFailed) state.lastError = null;
    initialSnapshotReadFailed = false;
    currentConnectionSnapshotReceived = true;
  }

  function getInitialSnapshotState() {
    return {
      snapshotReceived: currentConnectionSnapshotReceived,
      snapshotReadFailed: initialSnapshotReadFailed,
      snapshotRequested: initialSnapshotRequested,
      snapshotRetryCount: initialSnapshotRetryCount,
    };
  }

  function handleViewerMessage(message, sourceSocket = null) {
    if (sourceSocket && getSocket() !== sourceSocket) return;
    if (message?.request_auth_nonce != null) {
      for (const key of ["ack", "error", "quote"]) if (message[key]) message[key].request_auth_nonce = message.request_auth_nonce;
    }
    if (scopeSnapshotTransport.handle(message)) return;
    if (worldViewTransport.handle(message)) return;
    if (quoteProtocolFacade.handleQuoteViewerMessage(message)) return;
    switch (message?.type) {
      case "hello_ack":
        clearHelloAckTimer();
        getViewerPromptControlModule()?.handleHelloAck(message);
        state.server = message.server || null;
        state.worldId = message.world_id || null;
        state.controlProfile = message.control_profile || "playback";
        liveControlState.hello(message);
        hydrateChatHistoryFromStorage();
        if (!initialSnapshotRequested) {
          initialSnapshotRequested = true;
          initialSnapshotRetryCount = 0;
          sendInitialSnapshotRequest();
          scheduleInitialSnapshotRetry();
        }
        void ensureHostedPlayerAuthAvailable().then(() => {
          void syncHostedPlayerSessionOnConnect().catch((error) => { state.auth.error = String(error); render(); });
          render();
        }).catch((error) => { state.auth.error = String(error); render(); });
        break;
      case "live_control_state": liveControlState.receive(message.state); break;
      case "snapshot": handleSnapshot(message.snapshot); break;
      case "world_feed":
        worldFeedTransport.handleWorldFeed(message.feed, sourceSocket);
        hostedWorldAccount.restoreGameplayRequest(getGameplayRecoveryScope(state));
        break;
      case "event": {
        addRecentEvent(message.event);
        const chatEntry = extractAgentSpokeEntry(message.event);
        if (chatEntry) pushChatHistory(chatEntry);
        state.logicalTime = Math.max(state.logicalTime, Number(message.event?.time || 0));
        state.tick = state.logicalTime;
        worldFeedTransport.refreshAfterWorldActivity();
        break;
      }
      case "metrics": handleMetrics(message.time, message.metrics); break;
      case "decision_trace": handleDecisionTrace(message.trace); break;
      case "control_completion_ack": handleControlCompletionAck(message.ack); break;
      case "prompt_control_ack": handlePromptControlAck(message.ack); break;
      case "prompt_control_error": handlePromptControlError(message.error); break;
      case "safe_alternative_result": safeAlternative.receive(message); break;
      case "canonical_agent_owner_read_context": void canonicalOwnerRead.handleContext(message.context); break;
      case "canonical_agent_owner_view": canonicalOwnerRead.handleView(message); break;
      case "agent_chat_ack": handleAgentChatAck(message.ack); break;
      case "agent_chat_error": handleAgentChatError(message.error); break;
      case "gameplay_action_result":
        if (receiveGameplayResult(state, getGameplayRecoveryScope(state), hostedWorldAccount, message)) {
          clearPendingGameplayActionAckTimer();
          requestSnapshotSafe();
          render();
        }
        break;
      case "gameplay_action_ack": handleGameplayActionAck(message.ack); break;
      case "gameplay_action_error": handleGameplayActionError(message.error); break;
      case "authoritative_recovery_ack": handleAuthoritativeRecoveryAck(message.ack); break;
      case "authoritative_recovery_error": handleAuthoritativeRecoveryError(message.error); break;
      case "error": reportFatalError(message.message, "viewer"); break;
      default: break;
    }
    render({ pixelWorldChanged: message?.type !== "metrics" && message?.type !== "decision_trace" });
  }

  function attachSocket(ws) {
    ws.addEventListener("open", () => {
      if (getSocket() !== ws) return;
      worldFeedTransport.resetGeneration(ws);
      resetViewerProtocolForConnection();
      state.connectionStatus = "connected";
      state.lastError = null; state.server = null; state.worldId = null;
      initialSnapshotRequested = false; currentConnectionSnapshotReceived = false;
      initialSnapshotReadFailed = false; initialSnapshotRetryCount = 0;
      clearHelloAckTimer(); clearInitialSnapshotRetryTimer();
      sendJson({
        type: "hello_v2", client: "viewer", version: 2,
        capabilities: [promptControlResultCapability, LIVE_CONTROL_STATE_CAPABILITY, "gameplay_result_lookup_v1", "canonical_agent_chat_v1", "world_view", "world_view_pins_v1", "world_view_delta_v1", "world_view_aggregate_v1", "world_view_event_anchor_v1"],
      });
      scheduleHelloAckTimeout(ws);
      syncHostedSessionRefreshLoop();
      render();
    });

    ws.addEventListener("message", (event) => {
      if (getSocket() !== ws) return;
      try {
        handleViewerMessage(JSON.parse(String(event.data || "null")), ws);
      } catch (error) {
        reportFatalError(String(error), "viewer.parse");
      }
    });

    ws.addEventListener("error", () => {
      if (getSocket() !== ws) return;
      reportFatalError("websocket error", "viewer.ws");
    });

    ws.addEventListener("close", () => {
      if (getSocket() !== ws) return;
      invalidateAuthConnection(); cancelHostedLeaseRefresh(); worldFeedTransport.markDisconnected(ws);
      resetViewerProtocolForConnection();
      state.connectionStatus = "connecting";
      clearHostedRuntimeSyncTimer();
      if (state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
        state.auth.syncInFlight = false; state.auth.runtimeStatus = "disconnected";
      }
      worldViewTransport.clear(); scopeSnapshotTransport.clear();
      clearPendingSessionRegisterWaiter("websocket disconnected during player session registration");
      failPendingAgentChatAck("websocket disconnected before agent_chat ack/error returned", "agent_chat websocket disconnected");
      failPendingPromptControlAck("websocket disconnected before prompt_control ack/error returned", "prompt_control websocket disconnected");
      failPendingGameplayActionAck("websocket disconnected before gameplay_action ack/error returned", "gameplay_action websocket disconnected");
      stopHostedSessionRefreshLoop();
      render();
      if (reconnectTimer) window.clearTimeout(reconnectTimer);
      clearHelloAckTimer(); clearInitialSnapshotRetryTimer();
      reconnectTimer = window.setTimeout(connect, 1200);
    });
  }

  function connect() {
    invalidateAuthConnection();
    const currentSocket = getSocket();
    if (currentSocket) { try { currentSocket.close(); } catch (_) {} }
    cancelHostedLeaseRefresh();
    state.wsUrl = initialWsUrl();
    state.connectionStatus = "connecting";
    render();
    const nextSocket = new WebSocket(state.wsUrl);
    setSocket(nextSocket);
    attachSocket(nextSocket);
  }

  return {
    clearInitialSnapshotRetryTimer,
    markSnapshotReceived,
    getInitialSnapshotState,
    handleViewerMessage,
    connect,
  };
}
