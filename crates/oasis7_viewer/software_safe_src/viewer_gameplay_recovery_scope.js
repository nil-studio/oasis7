import { authConnectionGeneration, isSessionContextCurrent } from "./viewer_auth_session_module.js";
// Stable read authority identity, never a transport generation or bearer token.
export function gameplayRecoveryScopeFingerprint(scope, { ignoreAuthorityEpoch = false } = {}) {
  if (!scope?.endpointId || !scope?.worldId) return null;
  return JSON.stringify(Object.keys(scope).filter(key => !(ignoreAuthorityEpoch && key === "authorityEpoch")).sort().map(key => [key, scope[key]]));
}

// authorityEpoch belongs to a live Prompt runtime and can change when that
// process restarts. It is retained in old capsules, but must not decide whether
// the same endpoint/world/source fence can recover the original request.
export function durableGameplayRecoveryScopeFingerprint(scope) {
  return gameplayRecoveryScopeFingerprint(scope, { ignoreAuthorityEpoch: true });
}

export function getGameplayRecoveryScope(state) {
  const feed = state.worldFeed;
  const epoch = feed?.reorgEpoch;
  if (!feed?.worldId || feed.worldId !== state.worldId || feed.stale === true
    || epoch == null || !/^[0-9]+$/.test(String(epoch)) || !Number.isSafeInteger(Number(epoch))) return null;
  let endpoint;
  try { endpoint = new URL(state.wsUrl); } catch { return null; }
  if (!["ws:", "wss:"].includes(endpoint.protocol)) return null;
  const source = /^wfs1\.([0-9a-f]{64})\./.exec(feed.cursor || "");
  if (state.viewerProtocol?.capabilities?.includes("canonical_agent_chat_v1") && !source) return null;
  return {
    endpointId: endpoint.origin + endpoint.pathname, worldId: feed.worldId,
    authorityEpoch: state.viewerProtocol?.authorityEpoch || null,
    sourceFence: source ? `wfs1.${source[1]}` : `local:${feed.worldId}:${Number(epoch)}`,
    reorgEpoch: Number(epoch),
  };
}

export function gameplaySendContextCurrent(state, socket, captured) {
  return socket === captured.socket && socket?.readyState === WebSocket.OPEN
    && JSON.stringify(getGameplayRecoveryScope(state)) === JSON.stringify(captured.scope)
    && isSessionContextCurrent(state, captured.session, authConnectionGeneration(), state.wsUrl);
}
