import { durableGameplayRecoveryScopeFingerprint } from "./viewer_gameplay_recovery_scope.js";
import { settleGameplayOriginal } from "./viewer_gameplay_receipt_feedback.js";

const scopeKey = scope => scope && JSON.stringify(Object.keys(scope).sort().map(key => [key, scope[key]]));
function originalFeedback(state, scope) {
  const feedback = state.lastGameplayActionFeedback;
  const request = feedback?.originalRequest;
  if (!request?.auth || !durableGameplayRecoveryScopeFingerprint(scope) || durableGameplayRecoveryScopeFingerprint(scope) !== durableGameplayRecoveryScopeFingerprint(feedback.gameplayScope)
    || request.player_id !== state.auth.playerId || request.public_key !== state.auth.publicKey
    || request.auth.player_id !== request.player_id || request.auth.public_key !== request.public_key
    || request.auth.nonce !== feedback.requestAuthNonce) return null;
  return feedback;
}

export function requestGameplayResult(state, scope, send, now = Date.now()) {
  const feedback = originalFeedback(state, scope);
  if (!feedback || !["unknown", "received", "pending"].includes(feedback.stage)
    || state.auth.registrationStatus !== "registered"
    || !state.viewerProtocol?.capabilities?.includes("gameplay_result_lookup_v1")) return false;
  const previous = state.gameplayResultLookup;
  if (previous?.feedback === feedback && previous.auth === state.auth && previous.scope === scopeKey(scope) && now - previous.sentAt < 10_000) return false;
  try {
    send({ type: "lookup_gameplay_action", request: structuredClone(feedback.originalRequest) });
    state.gameplayResultLookup = { feedback, auth: state.auth, scope: scopeKey(scope), sentAt: now };
    return true;
  } catch (_) { return false; }
}

export function receiveGameplayResult(state, scope, account, result) {
  const feedback = originalFeedback(state, scope);
  const pending = state.gameplayResultLookup;
  if (!feedback || !["unknown", "received", "pending"].includes(feedback.stage) || pending?.feedback !== feedback || pending.auth !== state.auth || pending.scope !== scopeKey(scope)
    || result.player_id !== state.auth.playerId || result.request_auth_nonce !== feedback.requestAuthNonce
    || result.action_id !== feedback.originalRequest.action_id
    || result.target_agent_id !== feedback.originalRequest.target_agent_id) return false;
  const terminal = ["committed", "rejected", "expired"].includes(result.status);
  feedback.stage = terminal ? result.status : "unknown";
  feedback.ok = result.status === "committed";
  feedback.accepted = feedback.ok;
  feedback.effect = terminal ? "Original request has an authoritative " + result.status + " result"
    : "Original request result is " + result.status + "; retain it and retry the original result query";
  feedback.response = structuredClone(result);
  feedback.reason = terminal ? null : "original_result_" + result.status;
  if (terminal) {
    state.gameplayResultLookup = null;
    settleGameplayOriginal(account, feedback.requestAuthNonce, scope, state);
  }
  return true;
}
