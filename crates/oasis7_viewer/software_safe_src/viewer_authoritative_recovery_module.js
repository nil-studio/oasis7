import { requestGameplayResult } from "./viewer_gameplay_result_recovery.js";
import { getGameplayRecoveryScope } from "./viewer_gameplay_recovery_scope.js";
import { authConnectionGeneration, installSession, clearSession, updateRegistrationGrant, isSessionContextCurrent } from "./viewer_auth_session_module.js";
import { LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE } from "./software_safe_constants.js";

export function createViewerAuthoritativeRecoveryModule({
  state, getPendingSessionRegisterWaiter, getViewerControlLossModule, hostedWorldAccount,
  cancelHostedLeaseRefresh, clearPendingSessionRegisterWaiter, stopHostedSessionRefreshLoop,
  clearHostedPlayerSession, resolveAuthBootstrap, refreshHostedAdmissionState,
  releaseHostedPlayerSlot, sendJson, render, acceptRuntimeAckIdentity, clearHostedRuntimeSyncTimer,
  persistHostedPlayerSession, refreshHostedPlayerLease, syncHostedSessionRefreshLoop,
  resolvePendingSessionRegisterWaiterAfterRuntimeAck, maybeRecoverLocalTestStarterBindingFromSnapshot,
  requestSnapshotSafe, canAutoIssueHostedPlayerSession, isHostedAuthExpired,
  recoveryErrorRequiresExplicitRebind, ensureRegisteredPlayerSession, latestRequestedAgentId,
  issueHostedPlayerIdentity, rebindAgentIdFromRecoveryError, markPendingSemanticRebind,
  retryPendingSessionRegisterWaiterWithForceRebind,
}) {
  function resetHostedPlayerAuthState(errorMessage = null, revocationMeta = null) {
    cancelHostedLeaseRefresh();
    clearPendingSessionRegisterWaiter("authentication session was invalidated");
    stopHostedSessionRefreshLoop();
    clearHostedPlayerSession();
    const bootstrap = resolveAuthBootstrap();
    const revokeReason = String(revocationMeta?.revokeReason || "").trim() || null;
    const revokedBy = String(revocationMeta?.revokedBy || "").trim() || null;
    clearSession(state, {
      ...bootstrap, source: "guest_only", registrationStatus: "guest", error: errorMessage,
      revokeReason, revokedBy, sessionEpoch: null, issuedAtUnixMs: null, releaseToken: null,
      recoveryErrorCode: null, recoveryErrorMessage: null, issueInFlight: false, syncInFlight: false,
      runtimeStatus: "guest", boundAgentId: null, controlLostAgentId: null,
      pendingRequestedAgentId: null, pendingForceRebind: false, rebindNotice: null,
    });
    if (bootstrap.available) void installSession(state, bootstrap).then(() => render());
    void refreshHostedAdmissionState().then(() => render());
  }

  async function logoutHostedPlayerSession() {
    if (!state.auth.available || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      return { ok: false, reason: "hosted browser session is unavailable" };
    }
    const revokeRequest = {
      player_id: state.auth.playerId, session_pubkey: state.auth.publicKey,
      revoke_reason: "player_logout", revoked_by: state.auth.playerId,
    };
    try {
      if (state.connectionStatus === "connected") {
        sendJson({ type: "authoritative_recovery", command: { mode: "revoke_session", request: revokeRequest } });
      }
    } catch (_) {}
    try {
      await releaseHostedPlayerSlot();
    } finally {
      resetHostedPlayerAuthState("hosted player session released locally");
      render();
    }
    return { ok: true };
  }

  function markHostedSessionRefreshFailure() {
    state.auth.syncInFlight = false;
    state.auth.registrationStatus = "issued";
    state.auth.runtimeStatus = "error";
    state.auth.recoveryErrorCode = "session_refresh_failed";
    state.auth.recoveryErrorMessage = state.auth.error || "hosted player session refresh failed; retry to recover this browser session";
    render();
  }

  function adoptHostedRecoveryAck(ack) {
    if (!ack || !state.auth.available || (ack.status === "catch_up_ready" && ack.message === "snapshot_sync_metadata" && [ack.player_id, ack.session_pubkey, ack.session_epoch, ack.binding_epoch, ack.agent_id].every((value) => value == null))) return;
    if (!acceptRuntimeAckIdentity(ack, ack.status === "session_registered" || ack.status === "session_revoked")) return;
    const waiter = getPendingSessionRegisterWaiter();
    if (ack.status === "session_registered" && (!waiter || !isSessionContextCurrent(state, waiter.context, authConnectionGeneration(), state.wsUrl))) return;
    clearHostedRuntimeSyncTimer();
    const usesLegacyPreviewBootstrap = state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE;
    const hadPendingForceRebind = state.auth.pendingForceRebind === true;
    const previousRequestedAgentId = state.auth.pendingRequestedAgentId;
    const nextBoundAgentId = Object.prototype.hasOwnProperty.call(ack, "agent_id") ? (ack.agent_id || null) : (state.auth.boundAgentId || null);
    const nextRequestedAgentId = ack.agent_id || state.auth.pendingRequestedAgentId || state.auth.boundAgentId || null;
    state.auth.syncInFlight = false;
    state.auth.recoveryErrorCode = null; state.auth.recoveryErrorMessage = null;
    state.auth.error = null; state.auth.revokeReason = null; state.auth.revokedBy = null;
    if (ack.session_epoch != null) state.auth.sessionEpoch = Number(ack.session_epoch);
    if (Object.prototype.hasOwnProperty.call(ack, "binding_epoch")) {
      state.auth.bindingEpoch = ack.binding_epoch == null ? null : Number(ack.binding_epoch);
    }
    state.auth.boundAgentId = nextBoundAgentId;
    if (ack.status === "session_registered" || ack.status === "catch_up_ready") {
      getViewerControlLossModule()?.clearControlLost(nextBoundAgentId);
    }
    state.auth.pendingRequestedAgentId = nextRequestedAgentId;
    state.auth.pendingForceRebind = false;
    if (ack.status === "session_registered" && hadPendingForceRebind) {
      state.auth.rebindNotice = `Player session switched to ${ack.agent_id || previousRequestedAgentId || "requested agent"}.`;
    }
    state.auth.registrationStatus = ack.status === "session_registered" || ack.status === "catch_up_ready"
      ? "registered" : ack.status === "session_revoked" ? "guest" : "issued";
    if (ack.status === "session_registered" || ack.status === "catch_up_ready") updateRegistrationGrant(state.auth, null);
    state.auth.runtimeStatus = ack.status === "session_revoked" ? "revoked" : nextBoundAgentId ? "registered" : "registered_unbound";
    if (ack.status === "session_revoked") {
      if (usesLegacyPreviewBootstrap) {
        state.auth.registrationStatus = "issued"; state.auth.runtimeStatus = "revoked";
        state.auth.error = ack.message || "legacy preview player session was revoked";
        state.auth.pendingRequestedAgentId = null; state.auth.pendingForceRebind = false;
      } else {
        void releaseHostedPlayerSlot().catch(() => {});
        resetHostedPlayerAuthState(ack.message || "hosted player session was revoked", {
          revokeReason: ack.revoke_reason || ack.message || null, revokedBy: ack.revoked_by || null,
        });
      }
    } else if (!usesLegacyPreviewBootstrap) {
      persistHostedPlayerSession(state.auth);
      void refreshHostedPlayerLease();
      syncHostedSessionRefreshLoop();
    }
    if (ack.status === "session_registered") resolvePendingSessionRegisterWaiterAfterRuntimeAck(ack);
    maybeRecoverLocalTestStarterBindingFromSnapshot(state.snapshot);
    if (ack.status === "session_registered") {
      requestSnapshotSafe();
      requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson);
    }
  }

  async function recoverHostedSessionFromError(error) {
    if ((!canAutoIssueHostedPlayerSession() && !state.auth.available) || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) return;
    if (isHostedAuthExpired(error)) {
      if (state.auth.source === "hosted_account_cookie") { await hostedWorldAccount.renew(); return; }
    }
    const code = String(error?.code || "").trim();
    if (code === "session_not_found" && getPendingSessionRegisterWaiter()?.forceRebind) return;
    if (recoveryErrorRequiresExplicitRebind(error) && state.auth.pendingRequestedAgentId && !state.auth.pendingForceRebind) {
      await ensureRegisteredPlayerSession(state.auth.pendingRequestedAgentId, { forceRebind: true });
      return;
    }
    if (code === "session_not_found") { await ensureRegisteredPlayerSession(latestRequestedAgentId()); return; }
    if (code === "session_revoked") {
      void releaseHostedPlayerSlot().catch(() => {});
      resetHostedPlayerAuthState(error?.message || code || "hosted player session failed", {
        revokeReason: error?.revoke_reason || error?.message || null, revokedBy: error?.revoked_by || null,
      });
      render();
      return;
    }
    if (["session_key_mismatch", "session_player_id_invalid"].includes(code)) {
      void releaseHostedPlayerSlot().catch(() => {});
      resetHostedPlayerAuthState(error?.message || code || "hosted player session failed");
      render();
      await issueHostedPlayerIdentity();
      if (state.auth.available) await ensureRegisteredPlayerSession(latestRequestedAgentId());
    }
  }

  function handleAuthoritativeRecoveryAck(ack) { adoptHostedRecoveryAck(ack); }

  function handleAuthoritativeRecoveryError(error) {
    clearHostedRuntimeSyncTimer();
    const waiter = getPendingSessionRegisterWaiter();
    if (String(error?.code || "").trim() === "session_not_found" && waiter?.forceRebind) {
      state.auth.recoveryErrorCode = null; state.auth.recoveryErrorMessage = null;
      state.auth.runtimeStatus = "rebind_registering"; state.auth.error = null;
      render(); return;
    }
    const rebindAgentId = rebindAgentIdFromRecoveryError(error);
    if (waiter && recoveryErrorRequiresExplicitRebind(error) && (waiter.requestedAgentId || rebindAgentId) && !waiter.forceRebind) {
      if (!waiter.requestedAgentId && rebindAgentId) {
        waiter.requestedAgentId = rebindAgentId; state.auth.pendingRequestedAgentId = rebindAgentId;
      }
      state.auth.recoveryErrorCode = error?.code || null; state.auth.recoveryErrorMessage = error?.message || null;
      state.auth.error = error?.message || error?.code || "authoritative recovery failed";
      state.auth.registrationStatus = "registering"; state.auth.runtimeStatus = "rebind_retrying";
      state.auth.pendingForceRebind = true;
      state.auth.rebindNotice = `Requested agent ${state.auth.pendingRequestedAgentId || "-"} needs explicit rebind; retrying now.`;
      markPendingSemanticRebind("explicit rebind required; retrying registration for the requested agent");
      render();
      void retryPendingSessionRegisterWaiterWithForceRebind().catch((retryError) => {
        handleAuthoritativeRecoveryError({ code: "player_bind_failed", message: String(retryError) });
      });
      return;
    }
    if (!waiter && state.auth.source === "local_test_api_ephemeral" && recoveryErrorRequiresExplicitRebind(error) && rebindAgentId) {
      state.auth.recoveryErrorCode = error?.code || null; state.auth.recoveryErrorMessage = error?.message || null;
      state.auth.error = error?.message || error?.code || "authoritative recovery failed";
      state.auth.registrationStatus = "registering"; state.auth.runtimeStatus = "rebind_retrying";
      state.auth.pendingRequestedAgentId = rebindAgentId; state.auth.pendingForceRebind = true;
      state.auth.rebindNotice = `Requested agent ${rebindAgentId} needs explicit rebind; retrying now.`;
      render();
      void ensureRegisteredPlayerSession(rebindAgentId, { forceRebind: true }).catch((retryError) => {
        handleAuthoritativeRecoveryError({ code: "player_bind_failed", message: String(retryError), agent_id: rebindAgentId });
      });
      return;
    }
    if (!state.auth.available || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      clearPendingSessionRegisterWaiter(error?.message || error?.code || "authoritative recovery failed");
      return;
    }
    if (isHostedAuthExpired(error)) {
      clearPendingSessionRegisterWaiter(error?.message || error?.code || "hosted authorization expired");
      hostedWorldAccount.handleAuthorizationExpired?.(error);
      return;
    }
    state.auth.syncInFlight = false;
    state.auth.recoveryErrorCode = error?.code || null; state.auth.recoveryErrorMessage = error?.message || null;
    state.auth.error = error?.message || error?.code || "authoritative recovery failed";
    state.auth.revokeReason = error?.revoke_reason || null; state.auth.revokedBy = error?.revoked_by || null;
    state.auth.registrationStatus = "issued";
    state.auth.runtimeStatus = error?.code === "session_revoked" ? "revoked" : error?.code === "session_not_found" ? "missing" : "error";
    if (!recoveryErrorRequiresExplicitRebind(error) && !isHostedAuthExpired(error)) state.auth.boundAgentId = null;
    clearPendingSessionRegisterWaiter(error?.message || error?.code || "authoritative recovery failed");
    syncHostedSessionRefreshLoop();
    void recoverHostedSessionFromError(error);
  }

  return {
    resetHostedPlayerAuthState,
    logoutHostedPlayerSession,
    markHostedSessionRefreshFailure,
    handleAuthoritativeRecoveryAck,
    handleAuthoritativeRecoveryError,
  };
}
