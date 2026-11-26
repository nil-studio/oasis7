export function createViewerSessionRegisterModule({
  state,
  windowRef = window,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  SESSION_REGISTER_ACK_TIMEOUT_MS,
  getWaiter,
  setWaiter,
  getSocket,
  setSocket,
  validateRuntimeAckIdentity,
  invalidateAuthConnection,
  cancelHostedLeaseRefresh,
  hasSigningIdentity,
  updateRegistrationGrant,
  stopHostedSessionRefreshLoop,
  clearHostedRuntimeSyncTimer,
  isSessionContextCurrent,
  captureSessionContext,
  authConnectionGeneration,
  isTestApiEnabled,
  ensureHostedAuthSigningKey,
  authCredentials,
  hostedWorldAccount,
  refreshHostedPlayerLease,
  buildSessionRegisterAuthProof,
  sendJson,
  waitForHostedTestLogin,
  ensureHostedPlayerAuthAvailable,
  markCurrentGameplayActionFeedbackError,
  render,
}) {
  function clearPendingSessionRegisterWaiter(error = null, options = {}) {
    const waiter = getWaiter();
    if (!waiter) return;
    setWaiter(null);
    if (waiter.timeoutId) windowRef.clearTimeout(waiter.timeoutId);
    if (error != null && options.reject !== false) {
      waiter.reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  function acceptRuntimeAckIdentity(ack, registration = false) {
    const error = validateRuntimeAckIdentity(ack, state.auth, registration, getWaiter()?.requestedAgentId);
    if (!error) return true;
    invalidateAuthConnection();
    const invalidSocket = getSocket();
    setSocket(null);
    cancelHostedLeaseRefresh();
    invalidSocket?.close();
    clearPendingSessionRegisterWaiter(error);
    if (hasSigningIdentity(state.auth)) updateRegistrationGrant(state.auth, null);
    state.auth.syncInFlight = false;
    state.auth.runtimeStatus = "error";
    state.auth.error = error;
    state.connectionStatus = "error";
    state.lastError = error;
    stopHostedSessionRefreshLoop();
    clearHostedRuntimeSyncTimer();
    return false;
  }

  function recoverConnectedSessionStateAfterRuntimeAck(ack = null) {
    if (state.connectionStatus === "error" && /player session registration timed out/i.test(String(state.lastError || ""))) {
      state.connectionStatus = "connected";
      state.lastError = null;
    }
    state.auth.syncInFlight = false;
    state.auth.recoveryErrorCode = null;
    state.auth.recoveryErrorMessage = null;
    state.auth.error = null;
    if (ack?.session_epoch != null) state.auth.sessionEpoch = Number(ack.session_epoch);
    if (Object.prototype.hasOwnProperty.call(ack || {}, "binding_epoch")) {
      state.auth.bindingEpoch = ack.binding_epoch == null ? null : Number(ack.binding_epoch);
    }
    state.auth.authorityEpoch = state.viewerProtocol.authorityEpoch || null;
  }

  function resolvePendingSessionRegisterWaiterAfterRuntimeAck(ack = null) {
    const currentWaiter = getWaiter();
    if (currentWaiter && !isSessionContextCurrent(state, currentWaiter.context, authConnectionGeneration(), state.wsUrl)) return;
    recoverConnectedSessionStateAfterRuntimeAck(ack);
    const waiter = getWaiter();
    if (!waiter) return;
    setWaiter(null);
    if (waiter.timeoutId) windowRef.clearTimeout(waiter.timeoutId);
    waiter.resolve(ack);
  }

  function expirePendingSessionRegisterWaiterForTest() {
    if (!isTestApiEnabled()) throw new Error("expirePendingSessionRegisterWaiterForTest requires test_api=1");
    if (!getWaiter()) return false;
    const message = "player session registration timed out waiting for ack/error from live server";
    state.auth.syncInFlight = false;
    state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
    state.auth.runtimeStatus = "error";
    state.auth.error = message;
    state.auth.recoveryErrorCode = "session_register_timeout";
    state.auth.recoveryErrorMessage = message;
    clearPendingSessionRegisterWaiter(message);
    render();
    return true;
  }

  async function dispatchSessionRegisterRequest(requestedAgentId, forceRebind) {
    clearHostedRuntimeSyncTimer();
    const auth = state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
      ? state.auth
      : await ensureHostedAuthSigningKey(state.auth);
    const operationContext = captureSessionContext(state, authConnectionGeneration(), state.wsUrl);
    if (auth.source === "hosted_account_cookie" && !authCredentials(auth).registrationGrant) await hostedWorldAccount.renew();
    if (auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE && authCredentials(auth).releaseToken && !authCredentials(auth).registrationGrant) {
      const refreshed = await refreshHostedPlayerLease();
      if (!refreshed?.ok || !isSessionContextCurrent(state, operationContext, authConnectionGeneration(), state.wsUrl)) {
        throw new Error("registration grant refresh was invalidated");
      }
    }
    const normalizedRequestedAgentId = String(requestedAgentId || "").trim() || null;
    if (state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      state.auth.registrationStatus = "registering"; state.auth.syncInFlight = true;
      state.auth.recoveryErrorCode = state.auth.recoveryErrorMessage = null;
      state.auth.runtimeStatus = forceRebind === true ? "rebind_registering" : "registering";
    }
    if (forceRebind === true) {
      state.auth.rebindNotice = `Switching player session to ${normalizedRequestedAgentId || "requested agent"}...`;
    }
    const waiter = getWaiter();
    if (waiter) waiter.context = captureSessionContext(state, authConnectionGeneration(), state.wsUrl);
    state.auth.pendingRequestedAgentId = normalizedRequestedAgentId;
    state.auth.pendingForceRebind = forceRebind === true;
    const request = { player_id: auth.playerId, public_key: auth.publicKey };
    if (authCredentials(auth).registrationGrant) request.registration_grant = authCredentials(auth).registrationGrant;
    if (normalizedRequestedAgentId) request.requested_agent_id = normalizedRequestedAgentId;
    if (forceRebind === true) request.force_rebind = true;
    request.auth = await buildSessionRegisterAuthProof(request, auth);
    if (!isSessionContextCurrent(state, operationContext, authConnectionGeneration(), state.wsUrl)) {
      throw new Error("registration context was invalidated");
    }
    sendJson({ type: "authoritative_recovery", command: { mode: "register_session", request } });
    render();
  }

  async function retryPendingSessionRegisterWaiterWithForceRebind() {
    const waiter = getWaiter();
    if (!waiter) return;
    waiter.forceRebind = true;
    try {
      await dispatchSessionRegisterRequest(waiter.requestedAgentId, true);
    } catch (error) {
      clearPendingSessionRegisterWaiter(error);
      throw error;
    }
  }

  function rebindAgentIdFromRecoveryError(error) {
    const explicit = String(error?.agent_id || error?.agentId || error?.target_agent_id || error?.targetAgentId || "").trim();
    if (explicit) return explicit;
    const match = String(error?.message || "").match(/^agent\s+(\S+)\s+is bound to player\s+\S+,\s+not\s+\S+/);
    return match?.[1] || null;
  }

  function isHostedAuthExpired(error) {
    const c = String(error?.code || "").trim(), m = String(error?.message || "");
    return c === "hosted_authorization_expired" || m.includes("hosted_authorization_expired") || m.includes("invalid freshness window");
  }

  function latestRequestedAgentId(fallbackAgentId = null) {
    const agentId = String(fallbackAgentId || state.auth.pendingRequestedAgentId || state.auth.boundAgentId || "").trim();
    return agentId || null;
  }

  function recoveryErrorRequiresExplicitRebind(error) {
    if (String(error?.code || "").trim() !== "player_bind_failed") return false;
    const message = String(error?.message || "");
    return message.includes("explicit rebind required") || /^agent\s+\S+\s+is bound to player\s+\S+,\s+not\s+\S+/.test(message);
  }

  async function ensureRegisteredPlayerSession(requestedAgentId = null, options = {}) {
    await waitForHostedTestLogin();
    await ensureHostedPlayerAuthAvailable();
    if (!state.auth.available) throw new Error(state.auth.error || "player session auth is unavailable");
    const normalizedRequestedAgentId = String(requestedAgentId || "").trim() || null;
    const forceRebind = options?.forceRebind === true;
    if (
      state.auth.registrationStatus === "registered"
      && (state.auth.runtimeStatus === "registered" || state.auth.runtimeStatus === "registered_unbound")
      && !forceRebind
      && options?.renewAuthorization !== true
      && (normalizedRequestedAgentId == null || normalizedRequestedAgentId === state.auth.boundAgentId)
    ) {
      return state.auth;
    }
    const existing = getWaiter();
    if (existing) {
      const sameRequest = existing.requestedAgentId === normalizedRequestedAgentId && existing.forceRebind === forceRebind;
      if (!sameRequest) throw new Error("another player session registration is already in flight");
      return existing.promise;
    }
    let resolveWaiter;
    let rejectWaiter;
    const promise = new Promise((resolve, reject) => { resolveWaiter = resolve; rejectWaiter = reject; });
    const waiter = {
      context: captureSessionContext(state, authConnectionGeneration(), state.wsUrl),
      requestedAgentId: normalizedRequestedAgentId,
      forceRebind,
      promise,
      resolve: resolveWaiter,
      reject: rejectWaiter,
      timeoutId: null,
    };
    setWaiter(waiter);
    waiter.timeoutId = windowRef.setTimeout(() => {
      const active = getWaiter();
      if (!active || active.promise !== promise) return;
      const message = "player session registration timed out waiting for ack/error from live server";
      state.auth.syncInFlight = false;
      state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
      state.auth.runtimeStatus = "error";
      state.auth.error = message;
      state.auth.recoveryErrorCode = "session_register_timeout";
      state.auth.recoveryErrorMessage = message;
      clearPendingSessionRegisterWaiter(message);
      render();
    }, SESSION_REGISTER_ACK_TIMEOUT_MS);
    try {
      await dispatchSessionRegisterRequest(normalizedRequestedAgentId, forceRebind);
    } catch (error) {
      state.auth.syncInFlight = false;
      state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
      state.auth.runtimeStatus = "error";
      state.auth.error = String(error);
      state.auth.recoveryErrorCode = "session_register_send_failed";
      state.auth.recoveryErrorMessage = String(error);
      clearPendingSessionRegisterWaiter(error, { reject: false });
      markCurrentGameplayActionFeedbackError(error, "player session registration failed");
      render();
      throw error;
    }
    return promise;
  }

  function registerPlayerSessionForTest(requestedAgentId = null, options = {}) {
    if (!isTestApiEnabled()) throw new Error("registerPlayerSessionForTest requires test_api=1");
    return ensureRegisteredPlayerSession(requestedAgentId, options);
  }

  return {
    clearPendingSessionRegisterWaiter, acceptRuntimeAckIdentity,
    recoverConnectedSessionStateAfterRuntimeAck, resolvePendingSessionRegisterWaiterAfterRuntimeAck,
    expirePendingSessionRegisterWaiterForTest, dispatchSessionRegisterRequest,
    retryPendingSessionRegisterWaiterWithForceRebind, rebindAgentIdFromRecoveryError,
    isHostedAuthExpired, latestRequestedAgentId, recoveryErrorRequiresExplicitRebind,
    ensureRegisteredPlayerSession, registerPlayerSessionForTest,
  };
}
