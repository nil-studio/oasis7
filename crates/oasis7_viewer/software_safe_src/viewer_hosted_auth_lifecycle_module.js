export function createViewerHostedAuthLifecycleModule({
  state,
  windowRef = window,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  HOSTED_ACCOUNT_LOGIN_START_ROUTE,
  HOSTED_STRONG_AUTH_GRANT_ROUTE,
  HOSTED_PLAYER_SESSION_RELEASE_ROUTE,
  isHostedPublicJoinDeploymentMode,
  initialWsUrl,
  resolveStoredLocalTestPlayerSession,
  installSession,
  generateEphemeralEd25519Keypair,
  getAuthNonceCounter,
  persistLocalTestPlayerSession,
  maybeRecoverLocalTestStarterBindingFromSnapshot,
  completeHostedAccountLogin,
  ensureHostedAuthSigningKey,
  authCredentials,
  scheduleHostedRuntimeSyncTimeout,
  sendJson,
  clone,
  render,
}) {
  let localTestPlayerSessionInFlight = null;

  function canAutoIssueHostedPlayerSession() {
    return isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
      && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE;
  }

  function isLoopbackHostname(raw) {
    const value = String(raw || "").trim().toLowerCase();
    return value === "localhost" || value === "127.0.0.1" || value === "::1" || value === "[::1]" || value === "";
  }

  function hostnameFromUrl(raw, base = windowRef.location.href) {
    const value = String(raw || "").trim();
    if (!value) return null;
    try {
      return new URL(value, base).hostname || null;
    } catch (_) {
      return null;
    }
  }

  function canAutoIssueLocalTestPlayerSession() {
    if (state.auth.available || isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)) return false;
    const pageHost = String(windowRef.location.hostname || "").trim();
    const wsHost = hostnameFromUrl(state.wsUrl || initialWsUrl());
    return isLoopbackHostname(pageHost) && isLoopbackHostname(wsHost);
  }

  async function issueLocalTestPlayerSession(expectedAuth) {
    const stored = resolveStoredLocalTestPlayerSession();
    let installed;
    try {
      if (stored) {
        installed = await installSession(state, stored, expectedAuth);
      } else {
        const keypair = await generateEphemeralEd25519Keypair();
        if (state.auth !== expectedAuth || state.auth.available) return state.auth;
        const playerId = `local-test-player-${Date.now().toString(36)}-${getAuthNonceCounter() + 1}`;
        installed = await installSession(state, {
          available: true, hostedAccountId: null, playerId, loginChannel: null, maskedLoginHint: null,
          deviceSessionId: playerId, publicKey: keypair.publicKey, privateKey: keypair.privateKey,
          releaseToken: null, error: null, revokeReason: null, revokedBy: null,
          source: "local_test_api_ephemeral", registrationStatus: "issued", sessionEpoch: null,
          issuedAtUnixMs: Date.now(), recoveryErrorCode: null, recoveryErrorMessage: null,
          issueInFlight: false, syncInFlight: false, runtimeStatus: "issued",
          boundAgentId: null, controlLostAgentId: null, pendingRequestedAgentId: null,
          pendingForceRebind: false, rebindNotice: null,
        }, expectedAuth);
      }
    } catch (error) {
      if (state.auth !== expectedAuth) return state.auth;
      throw error;
    }
    if (state.auth !== installed) return state.auth;
    if (!stored) {
      if (__OASIS7_VISUAL_TEST__ === true) windowRef.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT__?.();
      persistLocalTestPlayerSession(state.auth);
    }
    render();
    maybeRecoverLocalTestStarterBindingFromSnapshot(state.snapshot);
    return state.auth;
  }

  async function startHostedAccountLogin() {
    if (!canAutoIssueHostedPlayerSession()) {
      return { ok: false, reason: "hosted account login is unavailable on this lane" };
    }
    const channel = "email";
    state.hostedLogin.channel = channel;
    const handle = String(state.hostedLogin.handle || "").trim();
    if (!handle) {
      state.hostedLogin.error = "email is required before login can start";
      render();
      return { ok: false, reason: state.hostedLogin.error };
    }
    state.hostedLogin.startInFlight = true;
    state.hostedLogin.error = null;
    state.hostedLogin.retryAfterSeconds = null;
    render();
    try {
      const response = await fetch(HOSTED_ACCOUNT_LOGIN_START_ROUTE, {
        method: "POST", cache: "no-store",
        headers: { Accept: "application/json", "Content-Type": "application/json" },
        body: JSON.stringify({ channel, handle }),
      });
      const payload = await response.json();
      if (!response.ok || !payload?.ok || !payload?.challenge?.challenge_id) {
        const retryAfterSeconds = payload?.retry_after_seconds == null ? null : Number(payload.retry_after_seconds);
        const baseMessage = payload?.error || payload?.error_code || `hosted account login start failed with HTTP ${response.status}`;
        const message = retryAfterSeconds && Number.isFinite(retryAfterSeconds)
          ? `${baseMessage} (retry in ${retryAfterSeconds}s)`
          : baseMessage;
        const hostedLoginError = new Error(message);
        hostedLoginError.hostedLoginRetryAfterSeconds = retryAfterSeconds;
        throw hostedLoginError;
      }
      state.hostedLogin.challengeId = String(payload.challenge.challenge_id || "").trim() || null;
      state.hostedLogin.maskedLoginHint = String(payload.challenge.masked_login_hint || "").trim() || null;
      state.hostedLogin.deliveryMode = String(payload.challenge.delivery_mode || "").trim() || null;
      state.hostedLogin.code = "";
      state.hostedLogin.expiresAtUnixMs = payload?.challenge?.expires_at_unix_ms == null ? null : Number(payload.challenge.expires_at_unix_ms);
      state.hostedLogin.retryAfterSeconds = null;
      state.hostedLogin.accountExists = false;
      state.hostedLogin.startInFlight = false;
      state.hostedLogin.completeInFlight = false;
      state.hostedLogin.error = null;
      render();
      return { ok: true, challengeId: state.hostedLogin.challengeId };
    } catch (error) {
      state.hostedLogin.startInFlight = false;
      if (error?.hostedLoginRetryAfterSeconds != null) {
        state.hostedLogin.retryAfterSeconds = Number(error.hostedLoginRetryAfterSeconds);
      }
      state.hostedLogin.error = String(error);
      render();
      return { ok: false, reason: state.hostedLogin.error };
    }
  }

  async function issueHostedPlayerIdentity() {
    return completeHostedAccountLogin();
  }

  async function ensureHostedPlayerAuthAvailable() {
    if (canAutoIssueLocalTestPlayerSession()) {
      const expectedAuth = state.auth;
      if (localTestPlayerSessionInFlight?.auth !== expectedAuth) {
        const pending = { auth: expectedAuth, promise: null };
        pending.promise = issueLocalTestPlayerSession(expectedAuth).finally(() => {
          if (localTestPlayerSessionInFlight === pending) localTestPlayerSessionInFlight = null;
        });
        localTestPlayerSessionInFlight = pending;
      }
      return localTestPlayerSessionInFlight.promise;
    }
    return state.auth;
  }

  async function retryHostedPlayerIdentityIssue() {
    if (!canAutoIssueHostedPlayerSession()) {
      return { ok: false, reason: "hosted account login is unavailable on this lane" };
    }
    const auth = state.hostedLogin.challengeId ? await completeHostedAccountLogin() : await startHostedAccountLogin();
    render();
    return {
      ok: auth?.available === true || auth?.ok === true,
      playerId: auth?.playerId || null,
      error: auth?.error || state.hostedLogin.error,
    };
  }

  async function requestHostedStrongAuthGrant(actionId, agentId) {
    const auth = await ensureHostedAuthSigningKey(state.auth);
    const playerId = String(auth.playerId || "").trim();
    const publicKey = String(auth.publicKey || "").trim();
    const releaseToken = String(authCredentials(state.auth).releaseToken || "").trim();
    const approvalCode = String(state.strongAuth.approvalCode || "").trim();
    if (!playerId || !publicKey || !releaseToken) {
      throw new Error("hosted strong-auth grant requires an active player_session with release token and browser session signing key");
    }
    if (!approvalCode) {
      throw new Error("backend approval code is required before hosted strong auth can be granted");
    }
    const query = new URLSearchParams({
      player_id: playerId, public_key: publicKey, release_token: releaseToken,
      agent_id: String(agentId || "").trim(), action_id: String(actionId || "").trim(), approval_code: approvalCode,
    });
    const response = await fetch(`${HOSTED_STRONG_AUTH_GRANT_ROUTE}?${query.toString()}`, {
      method: "GET", cache: "no-store", headers: { Accept: "application/json" },
    });
    const payload = await response.json();
    if (payload?.admission) state.hostedAdmission = clone(payload.admission);
    if (!response.ok || !payload?.ok || !payload?.grant) {
      state.strongAuth.lastGrantError = payload?.error || payload?.error_code || `hosted strong-auth grant failed with HTTP ${response.status}`;
      throw new Error(state.strongAuth.lastGrantError);
    }
    state.strongAuth.lastGrantActionId = String(payload.grant.action_id || "").trim() || actionId;
    state.strongAuth.lastGrantExpiresAtUnixMs = payload?.grant?.expires_at_unix_ms == null ? null : Number(payload.grant.expires_at_unix_ms);
    state.strongAuth.lastGrantError = null;
    return payload.grant;
  }

  async function sendReconnectSync() {
    if (!state.auth.available || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) return;
    const auth = await ensureHostedAuthSigningKey(state.auth);
    state.auth.syncInFlight = true;
    state.auth.registrationStatus = "registering";
    state.auth.runtimeStatus = "probing";
    state.auth.recoveryErrorCode = null;
    state.auth.recoveryErrorMessage = null;
    scheduleHostedRuntimeSyncTimeout();
    sendJson({
      type: "authoritative_recovery",
      command: { mode: "reconnect_sync", request: { player_id: auth.playerId, session_pubkey: auth.publicKey } },
    });
  }

  function probeHostedRuntimeSession() {
    if (
      !state.auth.available
      || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
      || state.connectionStatus !== "connected"
      || state.auth.registrationStatus !== "registered"
    ) return;
    state.auth.syncInFlight = true;
    state.auth.runtimeStatus = "probing";
    scheduleHostedRuntimeSyncTimeout();
    sendJson({
      type: "authoritative_recovery",
      command: { mode: "reconnect_sync", request: { player_id: state.auth.playerId, session_pubkey: state.auth.publicKey } },
    });
  }

  async function releaseHostedPlayerSlot() {
    const playerId = String(state.auth.playerId || "").trim();
    const releaseToken = String(authCredentials(state.auth).releaseToken || "").trim();
    if (!playerId || !releaseToken || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      return { ok: false, skipped: true };
    }
    const query = `player_id=${encodeURIComponent(playerId)}&release_token=${encodeURIComponent(releaseToken)}`;
    const response = await fetch(`${HOSTED_PLAYER_SESSION_RELEASE_ROUTE}?${query}`, {
      method: "POST", cache: "no-store", headers: { Accept: "application/json" },
    });
    const payload = await response.json();
    if (!response.ok || !payload?.ok) {
      if (payload?.admission) state.hostedAdmission = clone(payload.admission);
      throw new Error(payload?.error || payload?.error_code || `hosted player-session release failed with HTTP ${response.status}`);
    }
    state.hostedAdmission = payload?.admission ? clone(payload.admission) : state.hostedAdmission;
    return payload;
  }

  return {
    canAutoIssueHostedPlayerSession, isLoopbackHostname, hostnameFromUrl,
    canAutoIssueLocalTestPlayerSession, issueLocalTestPlayerSession, startHostedAccountLogin,
    issueHostedPlayerIdentity, ensureHostedPlayerAuthAvailable, retryHostedPlayerIdentityIssue,
    requestHostedStrongAuthGrant, sendReconnectSync, probeHostedRuntimeSession, releaseHostedPlayerSlot,
  };
}
