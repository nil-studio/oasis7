import { createPasskeyAccountClient, encodePasskeyBytes } from "./viewer_passkey_account_module.js";
import { installSession, updateRegistrationGrant, clearSession, authCredentials } from "./viewer_auth_session_module.js";

import { createHostedWorldRecovery } from "./viewer_hosted_world_recovery.js";
import { gameplayRecoveryScopeFingerprint, durableGameplayRecoveryScopeFingerprint } from "./viewer_gameplay_recovery_scope.js";

export function createHostedWorldAccount({ state, generateKeypair, register, render, clearLegacy, recovery = createHostedWorldRecovery(), getGameplayScope = () => null }) {
  let recoveryLoaded = false;
  const client = createPasskeyAccountClient();
  let pending = null;
  let active = null;
  let inFlight = null;
  let timer = null;
  let renewal = null;
  let generation=0;
  function disableLegacy() {
    clearLegacy();
    if (state.auth.source === "hosted_browser_storage") clearSession(state, { source: "guest_only", registrationStatus: "guest", runtimeStatus: "guest" });
    if (state.auth.source !== "hosted_account_cookie") { state.auth.error = null; state.auth.rebindNotice = null; render(); }
  }
  function join(account, { takeover = false, expectedEpoch } = {}) {
    if(state.auth.hostedAccountId&&state.auth.hostedAccountId!==account.account_id)invalidate();
    if(inFlight)return inFlight;
    const revision=generation;
    const task=performJoin(account,takeover,expectedEpoch,revision).finally(()=>{if(inFlight===task)inFlight=null;});
    inFlight=task;return task;
  }
  async function performJoin(account, takeover, expectedEpoch, revision) {
    const check=()=>{if(generation!==revision)throw new Error("state_changed");};
    disableLegacy();
    if(!recoveryLoaded){ pending=await recovery.load(account.account_id);check();recoveryLoaded=true; }
    if(!pending&&active?.candidate?.gameplayRequest)throw new Error("original_gameplay_outcome_pending");
    if(!pending||pending.accountId!==account.account_id){const keypair=await generateKeypair();check();pending={accountId:account.account_id,attempt:encodePasskeyBytes(crypto.getRandomValues(new Uint8Array(32))),keypair};}
    const candidate = pending;
    try { if(!await recovery.save(candidate)) state.auth.rebindNotice="Reload recovery is unavailable in this browser; keep this page open or explicitly confirm control again after reload."; } catch(error) {
      if(String(error).includes("context_in_use"))throw error;
      state.auth.rebindNotice="Reload recovery is unavailable in this browser. Continuing in this page requires the current control context.";
    }
    check();
    await client.capabilities();check();
    const payload = await client.joinWorld({ public_key: candidate.keypair.publicKey, join_attempt_id: candidate.attempt, takeover, expected_epoch: expectedEpoch });
    check();
    if (pending !== candidate) throw new Error("state_changed");
    const grant = payload.grant;
    if (!grant?.registration_grant || !grant.player_id) throw new Error("world_admission_unavailable");
    if(payload.account&&payload.account.account_id!==account.account_id)throw new Error("state_changed");
    const previous = state.auth;
    const previousLogin = {...previous,...authCredentials(previous)};
    const auth = await installSession(state, {
      ...previous, available: true, hostedAccountId: account.account_id, playerId: grant.player_id,
      ...candidate.keypair, registrationGrant: grant.registration_grant, releaseToken: null,
      deviceSessionId: grant.device_session_id, source: "hosted_account_cookie", registrationStatus: "issued",
      runtimeStatus: "issued", sessionEpoch: grant.world_session_epoch, error: null,
    });
    check();
    try {
      await register(null, { forceRebind: false });
      check();
      if (state.auth !== auth) throw new Error("state_changed");
      active = { auth, candidate, publicKey: candidate.keypair.publicKey, deviceSessionId: grant.device_session_id, epoch: grant.world_session_epoch };
      pending = null;
      restoreCandidateGameplayRequest(candidate, getGameplayScope());
      startRenewal();
      render();
      return payload;
    } catch (error) {
      if(generation!==revision)throw error;
      try {
        await client.capabilities();
        // Only join can reconcile a pending candidate. Refresh deliberately
        // accepts current controllers only.
        const status=await client.joinWorld({public_key:candidate.keypair.publicKey,join_attempt_id:candidate.attempt,takeover,expected_epoch:expectedEpoch});
        check();
        if(state.auth!==auth)throw new Error("state_changed");
        if(status.world?.state==="bound") {
          updateRegistrationGrant(auth,status.grant.registration_grant,status.grant.device_session_id);
          auth.registrationStatus="registered";
          auth.runtimeStatus=auth.boundAgentId ? "registered" : "registered_unbound";
          auth.error=null;
          active={auth,candidate,publicKey:candidate.keypair.publicKey,deviceSessionId:status.grant.device_session_id,epoch:status.grant.world_session_epoch};
          pending=null;restoreCandidateGameplayRequest(candidate, getGameplayScope());startRenewal();render();return status;
        }
        if(status.world?.state==="registration_pending"&&previousLogin.available&&previousLogin.privateKey&&active?.auth===previous){
          const confirmed=await client.refreshWorld({public_key:active.publicKey,device_session_id:active.deviceSessionId,expected_epoch:active.epoch});
          check();
          if(confirmed.world?.state==="bound"&&state.auth===auth){
            const restored=await installSession(state,{...previousLogin,registrationGrant:confirmed.grant.registration_grant},auth);
            active.auth=restored;startRenewal();
          }
        }
      }catch(_){ /* Preserve the candidate until its original outcome can be checked. */ }
      check();state.auth.error=String(error);render();throw error;
    }
  }
  function scopeFingerprint(scope) {
    return gameplayRecoveryScopeFingerprint(scope);
  }
  function sameScope(left, right) { const value = scopeFingerprint(left); return value != null && value === scopeFingerprint(right); }
  function sameDurableScope(left, right) { const value = durableGameplayRecoveryScopeFingerprint(left); return value != null && value === durableGameplayRecoveryScopeFingerprint(right); }
  function restoreCandidateGameplayRequest(candidate, scope) {
    const request = candidate.gameplayRequest;
    const currentFeedback = state.lastGameplayActionFeedback;
    if (!sameDurableScope(candidate.gameplayScope, scope)) {
      if (currentFeedback?.recoveredFromCapsule) state.lastGameplayActionFeedback = null;
      return;
    }
    if (!request || request.public_key !== state.auth.publicKey || request.auth?.public_key !== state.auth.publicKey
      || request.player_id !== state.auth.playerId || request.auth?.player_id !== state.auth.playerId) return;
    if (currentFeedback?.requestAuthNonce === request.auth.nonce) return;
    state.lastGameplayActionFeedback = {
      recoveredFromCapsule: true, id: `recovered-${request.auth.nonce}`, kind: "gameplay_action", action: request.action_id,
      targetAgentId: request.target_agent_id, agentId: request.target_agent_id,
      requestAuthNonce: request.auth.nonce, originalRequest: structuredClone(request),
      gameplayScope: structuredClone(scope), operation: { sessionRefreshRetryAttempted: true }, stage: "unknown", ok: false, accepted: false,
      effect: "Original request outcome unknown; waiting for its authoritative result", reason: "outcome_unknown_after_reload",
    };
  }
  async function rememberGameplayRequest(request, scope) {
    if (state.auth.source !== "hosted_account_cookie") return;
    const current = active;
    if (!scopeFingerprint(scope)) throw new Error("world_recovery_scope_unavailable");
    if (!sameScope(scope, getGameplayScope())) throw new Error("world_recovery_context_changed");
    if (!current || current.auth !== state.auth || !current.candidate
      || request.public_key !== current.publicKey || request.auth?.public_key !== current.publicKey
      || request.player_id !== state.auth.playerId || request.auth?.player_id !== state.auth.playerId)
      throw new Error("world_recovery_context_changed");
    if (pending && pending !== current.candidate) throw new Error("world_recovery_pending_control");
    const existing = current.candidate.gameplayRequest;
    if (current.settlingGameplay || (existing && (!sameDurableScope(current.candidate.gameplayScope, scope) || JSON.stringify(existing) !== JSON.stringify(request)))) throw new Error("original_gameplay_outcome_pending");
    const existingScope = current.candidate.gameplayScope;
    current.candidate.gameplayRequest = structuredClone(request);
    current.candidate.gameplayScope = structuredClone(scope);
    try {
      if (!await recovery.save(structuredClone(current.candidate))) throw new Error("world_recovery_unavailable");
    } catch (error) { current.candidate.gameplayRequest = existing; current.candidate.gameplayScope = existingScope; throw error; }
    // Persistence completed. A changed world cannot send it, but must retain
    // this original for its own scope rather than allowing a fresh nonce.
    if (active !== current || current.auth !== state.auth || !sameScope(scope, getGameplayScope()))
      throw new Error("world_recovery_context_changed");
  }
  async function settleGameplayRequest(nonce, scope) {
    if (state.auth.source !== "hosted_account_cookie") return false;
    const current = active;
    if (!current || current.auth !== state.auth || !sameScope(scope, getGameplayScope()) || !sameDurableScope(current.candidate?.gameplayScope, scope) || current.candidate?.gameplayRequest?.auth?.nonce !== nonce) return false;
    if (current.settlingGameplay) return false;
    const request = current.candidate.gameplayRequest;
    const settled = structuredClone(current.candidate);
    delete settled.gameplayRequest;
    delete settled.gameplayScope;
    current.settlingGameplay = true;
    try {
      if (!await recovery.save(settled)) throw new Error("world_recovery_unavailable");
      if (active !== current || current.auth !== state.auth || current.candidate.gameplayRequest !== request)
        throw new Error("world_recovery_context_changed");
      delete current.candidate.gameplayRequest;
      delete current.candidate.gameplayScope;
    } finally { current.settlingGameplay = false; }
    return true;
  }
  function startRenewal() {
    if (timer) clearInterval(timer);
    timer = setInterval(() => { void renew().catch(error => { if (active?.auth === state.auth) { state.auth.error = String(error); render(); } }); }, 10_000);
  }
  function renew() {
    if (renewal) return renewal;
    renewal=performRenew().finally(()=>{renewal=null;});return renewal;
  }
  async function performRenew() {
    const current = active;
    if (!current || state.auth !== current.auth) return null;
    await client.capabilities();
    const result = await client.refreshWorld({ public_key: current.publicKey, device_session_id: current.deviceSessionId, expected_epoch: current.epoch });
    if (active !== current || state.auth !== current.auth) return null;
    updateRegistrationGrant(current.auth, result.grant.registration_grant, result.grant.device_session_id);
    await register(null, { forceRebind: false, renewAuthorization: true });
    return result;
  }
  async function release() {
    const revision=++generation;inFlight=null;
    const current = active;
    if (timer) clearInterval(timer); timer = null;
    if (current) { await client.capabilities(); await client.releaseWorld({ public_key: current.publicKey, device_session_id: current.deviceSessionId, expected_epoch: current.epoch }); }
    if(generation!==revision)return;
    await recovery.clear();
    active = pending = null;recoveryLoaded=false;
    if (state.auth.source === "hosted_account_cookie") state.lastGameplayActionFeedback = null;
    clearSession(state, { source: "guest_only", registrationStatus: "guest", runtimeStatus: "guest" }); render();
  }
  function invalidate() {
    generation++;inFlight=null;recoveryLoaded=false;
    void recovery.clear().catch(()=>{});
    if (timer) clearInterval(timer);timer=null;active=pending=null;
    if (state.auth.source === "hosted_account_cookie") state.lastGameplayActionFeedback = null;
    clearSession(state,{source:"guest_only",registrationStatus:"guest",runtimeStatus:"guest"});render();
  }
  return { join, renew, release, disableLegacy, invalidate, rememberGameplayRequest, settleGameplayRequest,
    hasPendingGameplayRequest() { return Boolean(active?.auth === state.auth && (active?.candidate?.gameplayRequest || active?.settlingGameplay)); },
    restoreGameplayRequest(scope) { if (active?.auth === state.auth) restoreCandidateGameplayRequest(active.candidate, sameScope(scope, getGameplayScope()) ? scope : null); } };
}
