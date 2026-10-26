import { authConnectionGeneration, captureSessionContext, invalidateAuthConnection } from "./viewer_auth_session_module.js";
import { it, expect } from "vitest";
import { durableGameplayRecoveryScopeFingerprint, gameplayRecoveryScopeFingerprint, getGameplayRecoveryScope, gameplaySendContextCurrent } from "./viewer_gameplay_recovery_scope.js";
const state = () => ({worldId:"w",wsUrl:"wss://example.test/world?token=hidden",viewerProtocol:{authorityEpoch:"a",capabilities:["canonical_agent_chat_v1"]},
  worldFeed:{worldId:"w",reorgEpoch:"2",stale:false,cursor:`wfs1.${"a".repeat(64)}.cursor-one`}});
it("preserves original read scope across append, omits credentials, and fences branch/world/authority", () => {
  const current=state(), before=getGameplayRecoveryScope(current);
  expect(before.endpointId).toBe("wss://example.test/world");
  current.worldFeed.cursor=`wfs1.${"a".repeat(64)}.cursor-two`;
  expect(getGameplayRecoveryScope(current)).toEqual(before);
  current.worldFeed.cursor=`wfs1.${"b".repeat(64)}.cursor-three`;
  expect(getGameplayRecoveryScope(current)).not.toEqual(before);
  current.worldFeed.worldId="other"; expect(getGameplayRecoveryScope(current)).toBeNull();
});
it("keeps the durable capsule scope stable across runtime authority epochs while preserving world fences", () => {
  const current = state();
  const beforeRestart = getGameplayRecoveryScope(current);
  current.viewerProtocol.authorityEpoch = "runtime-after-restart";
  const afterRestart = getGameplayRecoveryScope(current);
  expect(gameplayRecoveryScopeFingerprint(beforeRestart)).not.toBe(gameplayRecoveryScopeFingerprint(afterRestart));
  expect(durableGameplayRecoveryScopeFingerprint(beforeRestart)).toBe(durableGameplayRecoveryScopeFingerprint(afterRestart));

  for (const changed of [
    { ...afterRestart, endpointId: "wss://other.test/world" },
    { ...afterRestart, worldId: "other-world" },
    { ...afterRestart, sourceFence: `wfs1.${"b".repeat(64)}` },
    { ...afterRestart, reorgEpoch: afterRestart.reorgEpoch + 1 },
  ]) {
    expect(durableGameplayRecoveryScopeFingerprint(changed)).not.toBe(durableGameplayRecoveryScopeFingerprint(beforeRestart));
  }
});
it("requires current proven scope and rejects stale, absent, and wrong transport", () => {
  for(const change of [s=>s.worldFeed.stale=true,s=>s.worldFeed.cursor="unknown",s=>s.wsUrl="https://example.test/",s=>s.worldFeed.reorgEpoch=null]){
    const s=state();change(s);expect(getGameplayRecoveryScope(s)).toBeNull();
  }
});

it("fences pre-send custody against replaced or closed sockets, changed world scope, and revoked session generation", () => {
 const current={...state(),auth:{playerId:"p",publicKey:"key"}}, socket={readyState:WebSocket.OPEN};
 const captured={socket,scope:getGameplayRecoveryScope(current),session:captureSessionContext(current,authConnectionGeneration(),current.wsUrl)};
 expect(gameplaySendContextCurrent(current,socket,captured)).toBe(true);
 expect(gameplaySendContextCurrent(current,{readyState:WebSocket.OPEN},captured)).toBe(false);
 socket.readyState=WebSocket.CLOSED;expect(gameplaySendContextCurrent(current,socket,captured)).toBe(false);socket.readyState=WebSocket.OPEN;
 current.worldFeed.cursor=`wfs1.${"b".repeat(64)}.new-branch`;expect(gameplaySendContextCurrent(current,socket,captured)).toBe(false);
 current.worldFeed.cursor=`wfs1.${"a".repeat(64)}.same-branch`;expect(gameplaySendContextCurrent(current,socket,captured)).toBe(true);
 current.viewerProtocol.authorityEpoch="runtime-rotated";expect(gameplaySendContextCurrent(current,socket,captured)).toBe(false);
 current.viewerProtocol.authorityEpoch="a";expect(gameplaySendContextCurrent(current,socket,captured)).toBe(true);
 invalidateAuthConnection();expect(gameplaySendContextCurrent(current,socket,captured)).toBe(false);
});
