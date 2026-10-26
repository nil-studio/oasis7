import { webcrypto } from "node:crypto";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const seam = vi.hoisted(() => ({
  sign: vi.fn(),
  client: {
    capabilities: vi.fn(), joinWorld: vi.fn(), refreshWorld: vi.fn(), releaseWorld: vi.fn(),
  },
}));
vi.mock("./viewer_passkey_account_module.js", async (original) => {
  const module = await original();
  return { ...module, createPasskeyAccountClient: () => seam.client };
});
vi.mock("./viewer_auth_crypto.js", async (original) => ({
  ...await original(),
  importSigningIdentity: vi.fn(async (publicKey) => ({ publicKey, identityId: `fixture:${publicKey}` })),
  signAuthPayload: (...args) => seam.sign(...args),
}));

import { authCredentials, installSession, clearSession } from "./viewer_auth_session_module.js";
import { createHostedWorldAccount } from "./viewer_hosted_world_account_module.js";
import { requestGameplayResult, receiveGameplayResult } from "./viewer_gameplay_result_recovery.js";

const account = { account_id: "account-a" };
const gameplayScope = { endpointId: "ws://localhost/synthetic", worldId: "synthetic-world", authorityEpoch: "epoch-1" };
const pair = (number) => ({ publicKey: String(number).repeat(64), privateKey: `fixture-secret-${number}` });
const grant = (number, epoch = number) => ({
  ok: true, world: { state: "registration_pending" },
  grant: { player_id: "hosted-player-a", device_session_id: `device-${number}`,
    world_session_epoch: epoch, registration_grant: `signed-fixture-grant-${number}` },
});
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const instances = [];
let storage;
function fixture(initial = { available: false, source: "guest_only" }, recovery) {
  const state = { auth: initial };
  const generateKeypair = vi.fn(async () => pair(1));
  const register = vi.fn(async () => {});
  const render = vi.fn();
  const clearLegacy = vi.fn();
  const module = createHostedWorldAccount({ state, generateKeypair, register, render, clearLegacy, recovery, getGameplayScope: () => gameplayScope });
  instances.push(module);
  return { module, state, generateKeypair, register, render, clearLegacy };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  vi.stubGlobal("crypto", webcrypto);
  storage = { setItem: vi.fn(), getItem: vi.fn(), removeItem: vi.fn() };
  vi.stubGlobal("localStorage", storage);
  seam.client.capabilities.mockResolvedValue({ ok: true });
  seam.sign.mockResolvedValue("fixture-session-proof");
  seam.client.joinWorld.mockResolvedValue(grant(1));
  seam.client.refreshWorld.mockResolvedValue({ ...grant(1), world: { state: "bound" } });
  seam.client.releaseWorld.mockResolvedValue({ ok: true, world: { state: "released" } });
});
afterEach(() => {
  for (const module of instances.splice(0)) module.invalidate();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("hosted account world bridge contracts", () => {
  it("clears legacy bootstrap errors after v2 detection while preserving active v2 control errors", () => {
    const guest = fixture({ source: "guest_only", error: "viewer auth bootstrap is unavailable", rebindNotice: "legacy" });
    guest.module.disableLegacy();
    expect(guest.state.auth).toMatchObject({ error: null, rebindNotice: null });
    expect(guest.render).toHaveBeenCalledOnce();
    const active = fixture({ source: "hosted_account_cookie", error: "session_revoked" });
    active.module.disableLegacy();
    expect(active.state.auth.error).toBe("session_revoked");
  });
  it("does not turn an existing account login into an automatic world join", async () => {
    const { module, generateKeypair, register } = fixture({
      available: true, source: "hosted_account_cookie", hostedAccountId: account.account_id,
    });
    await vi.advanceTimersByTimeAsync(30_000);
    expect(await module.renew()).toBeNull();
    expect(generateKeypair).not.toHaveBeenCalled();
    expect(seam.client.joinWorld).not.toHaveBeenCalled();
    expect(register).not.toHaveBeenCalled();
  });

  it("creates a 43-character base64url attempt and shares duplicate join work", async () => {
    const waiting = deferred();
    seam.client.joinWorld.mockReturnValue(waiting.promise);
    const { module, generateKeypair, register } = fixture();
    const first = module.join(account), second = module.join(account);
    await vi.waitFor(() => expect(seam.client.joinWorld).toHaveBeenCalledTimes(1));
    const payload = seam.client.joinWorld.mock.calls[0][0];
    expect(payload.join_attempt_id).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(payload).toMatchObject({ public_key: pair(1).publicKey, takeover: false });
    expect(payload).not.toHaveProperty("account_id");
    expect(payload).not.toHaveProperty("player_id");
    waiting.resolve(grant(1));
    await Promise.all([first, second]);
    expect(generateKeypair).toHaveBeenCalledTimes(1);
    expect(register).toHaveBeenCalledTimes(1);
    expect(register).toHaveBeenCalledWith(null, { forceRebind: false });
  });

  it("sends takeover and the explicit server epoch only after confirmation", async () => {
    seam.client.joinWorld.mockResolvedValue(grant(1, 8));
    const { module } = fixture();
    await module.join(account, { takeover: true, expectedEpoch: 7 });
    expect(seam.client.joinWorld).toHaveBeenCalledWith(expect.objectContaining({
      takeover: true, expected_epoch: 7,
    }));
  });

  it("renews a fresh signed grant and re-registers the same control key every ten seconds", async () => {
    const { module, state, generateKeypair, register } = fixture();
    await module.join(account);
    seam.client.refreshWorld.mockResolvedValue({
      ...grant(1), world: { state: "bound" },
      grant: { ...grant(1).grant, registration_grant: "fresh-signed-grant" },
    });
    await vi.advanceTimersByTimeAsync(9_999);
    expect(seam.client.refreshWorld).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(seam.client.refreshWorld).toHaveBeenCalledExactlyOnceWith({
      public_key: pair(1).publicKey, device_session_id: "device-1", expected_epoch: 1,
    });
    expect(authCredentials(state.auth).registrationGrant).toBe("fresh-signed-grant");
    expect(register).toHaveBeenCalledTimes(2);
    expect(register).toHaveBeenLastCalledWith(null, { forceRebind: false, renewAuthorization: true });
    expect(generateKeypair).toHaveBeenCalledTimes(1);
  });

  it("retains the original attempt and key when the join result is unknown", async () => {
    seam.client.joinWorld.mockRejectedValueOnce(new Error("operation_pending"));
    const { module, generateKeypair, register } = fixture();
    await expect(module.join(account)).rejects.toThrow("operation_pending");
    const original = seam.client.joinWorld.mock.calls[0][0];
    expect(register).not.toHaveBeenCalled();
    await module.join(account);
    expect(seam.client.joinWorld.mock.calls[1][0]).toEqual(original);
    expect(generateKeypair).toHaveBeenCalledTimes(1);
  });

  it("restores the previous signing identity and its renewal when new binding is confirmed pending", async () => {
    seam.client.joinWorld.mockResolvedValueOnce(grant(1)).mockResolvedValueOnce(grant(2));
    const { module, state, generateKeypair, register } = fixture();
    generateKeypair.mockResolvedValueOnce(pair(1)).mockResolvedValueOnce(pair(2));
    await module.join(account);
    register.mockRejectedValueOnce(new Error("candidate bind rejected"));
    seam.client.joinWorld.mockResolvedValueOnce(grant(2));
    seam.client.refreshWorld.mockResolvedValueOnce({ ...grant(1), world: { state: "bound" } });
    await expect(module.join(account, { takeover: true, expectedEpoch: 1 })).rejects.toThrow("candidate bind rejected");
    expect(state.auth.publicKey).toBe(pair(1).publicKey);
    expect(authCredentials(state.auth).privateKey).toBe(pair(1).privateKey);
    seam.client.refreshWorld.mockResolvedValue({ ...grant(1), world: { state: "bound" } });
    await module.renew();
    expect(seam.client.refreshWorld).toHaveBeenLastCalledWith({
      public_key: pair(1).publicKey, device_session_id: "device-1", expected_epoch: 1,
    });
    expect(register).toHaveBeenCalledTimes(3);
    expect(register).toHaveBeenLastCalledWith(null, { forceRebind: false, renewAuthorization: true });
  });

  it("finishes a lost registration ACK through the original join attempt and renews beyond expiry", async () => {
    const { module, register, state } = fixture();
    register.mockRejectedValueOnce(new Error("ACK timeout"));
    seam.client.joinWorld.mockResolvedValueOnce(grant(1)).mockResolvedValueOnce({ ...grant(1), world: { state: "bound" } });
    await expect(module.join(account)).resolves.toMatchObject({ world: { state: "bound" } });
    expect(seam.client.joinWorld.mock.calls[1][0]).toEqual(seam.client.joinWorld.mock.calls[0][0]);
    expect(state.auth).toMatchObject({registrationStatus:"registered",runtimeStatus:"registered_unbound"});
    await vi.advanceTimersByTimeAsync(60_000);
    expect(register).toHaveBeenCalledTimes(7);
  });

  it("restores a reload capsule through the original attempt without generating a new controller", async () => {
    const state = { auth: { available: false, source: "guest_only" } };
    const recovery = { load: vi.fn(async () => ({ accountId: account.account_id, attempt: "original-reload-attempt", keypair: pair(1) })), save: vi.fn(async () => true), clear: vi.fn(async () => {}) };
    const generateKeypair = vi.fn();
    const register = vi.fn(async () => {});
    const module = createHostedWorldAccount({ state, recovery, generateKeypair, register, render: vi.fn(), clearLegacy: vi.fn() });
    instances.push(module);
    await module.join(account);
    expect(generateKeypair).not.toHaveBeenCalled();
    expect(seam.client.joinWorld).toHaveBeenCalledWith(expect.objectContaining({ join_attempt_id: "original-reload-attempt", public_key: pair(1).publicKey, takeover: false }));
  });

  it("keeps a candidate unresolved when old-controller authority cannot be confirmed", async () => {
    seam.client.joinWorld.mockResolvedValueOnce(grant(1)).mockResolvedValue(grant(2));
    const { module, state, generateKeypair, register } = fixture();
    generateKeypair.mockResolvedValueOnce(pair(1)).mockResolvedValueOnce(pair(2));
    await module.join(account);
    register.mockRejectedValueOnce(new Error("candidate bind rejected"));
    seam.client.refreshWorld.mockRejectedValueOnce(new Error("control_replaced"));
    await expect(module.join(account, { takeover: true, expectedEpoch: 1 })).rejects.toThrow("candidate bind rejected");
    expect(state.auth.publicKey).toBe(pair(2).publicKey);
    expect(await module.renew()).toBeNull();
  });

  it("coalesces concurrent renewal so an older response cannot overwrite another grant", async () => {
    const { module, register } = fixture();
    await module.join(account);
    const response = deferred();
    seam.client.refreshWorld.mockReturnValueOnce(response.promise);
    const first = module.renew(), second = module.renew();
    expect(first).toBe(second);
    await vi.waitFor(() => expect(seam.client.refreshWorld).toHaveBeenCalledTimes(1));
    response.resolve({ ...grant(1), world: { state: "bound" } });
    await Promise.all([first, second]);
    expect(register).toHaveBeenCalledTimes(2);
  });

  it("forces the real registered-session consumer to dispatch renewed authorization", async () => {
    const priorLocation = window.location.href;
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&hosted_access=%7B%22deployment_mode%22%3A%22hosted_public_join%22%7D");
    const core = await import("./legacy_core.js");
    await installSession(core.state, {
      available: true, source: "hosted_account_cookie", playerId: "hosted-player-a",
      ...pair(1), registrationGrant: "fresh-signed-grant", registrationStatus: "registered",
      runtimeStatus: "registered_unbound", deviceSessionId: "device-1",
    });
    try {
      await expect(core.registerPlayerSessionForTest(null, { forceRebind: false })).resolves.toMatchObject({ registrationStatus: "registered" });
      expect(seam.sign).not.toHaveBeenCalled();
      // The fixture has no socket. A renewal must reach signing/send instead of
      // resolving via the existing registered-session shortcut.
      await expect(core.registerPlayerSessionForTest(null, { forceRebind: false, renewAuthorization: true })).rejects.toThrow("viewer websocket is not connected");
      expect(seam.sign).toHaveBeenCalledTimes(1);
    } finally {
      clearSession(core.state);
      window.history.replaceState({}, "", priorLocation);
    }
  });

  it("keeps control proofs in memory and never exposes a release bearer in state, storage or requests", async () => {
    const { module, state } = fixture();
    await module.join(account);
    expect(state.auth).not.toHaveProperty("privateKey");
    expect(state.auth).not.toHaveProperty("registrationGrant");
    expect(state.auth).not.toHaveProperty("releaseToken");
    expect(authCredentials(state.auth).releaseToken).toBeNull();
    await module.renew();
    await module.release();
    for (const call of [seam.client.joinWorld, seam.client.refreshWorld, seam.client.releaseWorld]) {
      for (const [payload] of call.mock.calls) {
        expect(payload).not.toHaveProperty("release_token");
        expect(payload).not.toHaveProperty("private_key");
        expect(payload).not.toHaveProperty("account_token");
      }
    }
    expect(storage.setItem).not.toHaveBeenCalled();
    expect(state.auth.available).toBe(false);
  });

  it("does not revive an invalidated account while key generation is pending", async () => {
    const ready = deferred();
    const { module, state, generateKeypair, register } = fixture();
    generateKeypair.mockReturnValueOnce(ready.promise);
    const joining = module.join(account);
    module.invalidate();
    ready.resolve(pair(1));
    await expect(joining).rejects.toThrow(/state_changed|context changed/);
    expect(seam.client.joinWorld).not.toHaveBeenCalled();
    expect(register).not.toHaveBeenCalled();
    expect(state.auth.available).toBe(false);
  });
});

const signedGameplay = () => ({ action_id: "claim_first_agent", target_agent_id: "starter-agent-0", player_id: "hosted-player-a", public_key: pair(1).publicKey,
  auth: { player_id: "hosted-player-a", public_key: pair(1).publicKey, nonce: 1234, signature: "synthetic-signed-original" } });
function persistentGameplayCapsule() {
  const store = { capsule: null };
  const recovery = {
    load: vi.fn(async () => store.capsule && structuredClone(store.capsule)),
    save: vi.fn(async value => { store.capsule = structuredClone(value); return true; }),
    clear: vi.fn(async () => {}),
  };
  return { store, recovery };
}
async function reloadGameplayCapsule(recovery) {
  const first = fixture(undefined, recovery);
  await first.module.join(account);
  const original = signedGameplay();
  await first.module.rememberGameplayRequest(original, gameplayScope);
  const originalCapsule = recovery.save.mock.calls.at(-1)[0];
  const originalAttempt = originalCapsule.attempt;
  expect(originalCapsule.gameplayRequest).toEqual(original);
  first.module.invalidate();

  const second = fixture(undefined, recovery);
  second.register.mockImplementation(async () => { second.state.auth.registrationStatus = "registered"; });
  await second.module.join(account);
  second.state.viewerProtocol = { capabilities: ["gameplay_result_lookup_v1"] };
  return { second, original, originalAttempt };
}

it("restores the encrypted-capsule original after reload without submitting or changing its nonce", async () => {
  let capsule = null;
  const recovery = { load: vi.fn(async () => capsule && structuredClone(capsule)), save: vi.fn(async value => { capsule = structuredClone(value); return true; }), clear: vi.fn(async () => {}) };
  const first = fixture(undefined, recovery);
  await first.module.join(account);
  const original = signedGameplay();
  await first.module.rememberGameplayRequest(original, gameplayScope);
  const originalAttempt = capsule.attempt;
  await expect(first.module.join(account, { takeover: true, expectedEpoch: 1 })).rejects.toThrow("original_gameplay_outcome_pending");
  expect(capsule.gameplayRequest).toEqual(original);
  first.module.invalidate(); // Simulate page lifetime ending; the capsule seam retains the encrypted record.
  const second = fixture(undefined, recovery);
  await second.module.join(account);
  expect(seam.client.joinWorld).toHaveBeenLastCalledWith(expect.objectContaining({ join_attempt_id: originalAttempt }));
  expect(second.state.lastGameplayActionFeedback).toMatchObject({ stage: "unknown", requestAuthNonce: 1234, originalRequest: original, action: "claim_first_agent" });
  await expect(second.module.rememberGameplayRequest({ ...original, auth: { ...original.auth, nonce: 1235 } }, gameplayScope)).rejects.toThrow("original_gameplay_outcome_pending");
  expect(await second.module.settleGameplayRequest(9999, gameplayScope)).toBe(false);
  expect(capsule.gameplayRequest).toEqual(original);
  expect(await second.module.settleGameplayRequest(1234, gameplayScope)).toBe(true);
  expect(capsule).not.toHaveProperty("gameplayRequest");
});

it("looks up and settles the restored capsule original after account recreation", async () => {
  const { store, recovery } = persistentGameplayCapsule();
  const { second, original, originalAttempt } = await reloadGameplayCapsule(recovery);
  expect(seam.client.joinWorld).toHaveBeenLastCalledWith(expect.objectContaining({ join_attempt_id: originalAttempt }));
  expect(second.state.lastGameplayActionFeedback).toMatchObject({
    stage: "unknown", requestAuthNonce: original.auth.nonce, originalRequest: original,
  });
  expect(second.module.hasPendingGameplayRequest()).toBe(true);

  const send = vi.fn();
  expect(requestGameplayResult(second.state, gameplayScope, send, 100)).toBe(true);
  expect(send).toHaveBeenCalledExactlyOnceWith({ type: "lookup_gameplay_action", request: original });
  const settle = vi.spyOn(second.module, "settleGameplayRequest");
  expect(receiveGameplayResult(second.state, gameplayScope, second.module, {
    player_id: original.player_id,
    action_id: original.action_id,
    target_agent_id: original.target_agent_id,
    request_auth_nonce: original.auth.nonce,
    status: "committed",
  })).toBe(true);
  expect(settle).toHaveBeenCalledExactlyOnceWith(original.auth.nonce, gameplayScope);
  await expect(settle.mock.results[0].value).resolves.toBe(true);
  expect(store.capsule).not.toHaveProperty("gameplayRequest");
  expect(second.module.hasPendingGameplayRequest()).toBe(false);
});

it("retains the restored capsule after a foreign result and an unavailable lookup result", async () => {
  const { store, recovery } = persistentGameplayCapsule();
  const { second, original } = await reloadGameplayCapsule(recovery);
  const send = vi.fn();
  expect(requestGameplayResult(second.state, gameplayScope, send, 100)).toBe(true);
  expect(send).toHaveBeenCalledExactlyOnceWith({ type: "lookup_gameplay_action", request: original });
  const settle = vi.spyOn(second.module, "settleGameplayRequest");

  expect(receiveGameplayResult(second.state, gameplayScope, second.module, {
    player_id: "foreign-player",
    action_id: original.action_id,
    target_agent_id: original.target_agent_id,
    request_auth_nonce: original.auth.nonce,
    status: "committed",
  })).toBe(false);
  expect(store.capsule.gameplayRequest).toEqual(original);

  expect(receiveGameplayResult(second.state, gameplayScope, second.module, {
    player_id: original.player_id,
    action_id: original.action_id,
    target_agent_id: original.target_agent_id,
    request_auth_nonce: original.auth.nonce,
    status: "unavailable",
  })).toBe(true);
  expect(settle).not.toHaveBeenCalled();
  expect(second.module.hasPendingGameplayRequest()).toBe(true);
  expect(second.state.lastGameplayActionFeedback).toMatchObject({ stage: "unknown", originalRequest: original });
  expect(store.capsule.gameplayRequest).toEqual(original);
});

it("blocks hosted signed requests when encrypted persistence is unsupported or denied", async () => {
  const recovery = { load: vi.fn(async () => null), save: vi.fn(async () => true), clear: vi.fn(async () => {}) };
  const { module } = fixture(undefined, recovery);
  await module.join(account);
  recovery.save.mockResolvedValueOnce(false);
  await expect(module.rememberGameplayRequest(signedGameplay(), gameplayScope)).rejects.toThrow("world_recovery_unavailable");
  recovery.save.mockRejectedValueOnce(new Error("quota denied"));
  await expect(module.rememberGameplayRequest(signedGameplay(), gameplayScope)).rejects.toThrow("quota denied");
});

it("holds the exact original while durable settlement is pending or fails", async () => {
  const recovery = { load: vi.fn(async () => null), save: vi.fn(async () => true), clear: vi.fn(async () => {}) };
  const { module } = fixture(undefined, recovery);
  await module.join(account);
  const original = signedGameplay();
  await module.rememberGameplayRequest(original, gameplayScope);
  await expect(module.rememberGameplayRequest({ ...original, target_agent_id: "changed-target" }, gameplayScope)).rejects.toThrow("original_gameplay_outcome_pending");
  const waiting = deferred();
  recovery.save.mockReturnValueOnce(waiting.promise);
  const settling = module.settleGameplayRequest(original.auth.nonce, gameplayScope);
  expect(module.hasPendingGameplayRequest()).toBe(true);
  const failed = expect(settling).rejects.toThrow("quota denied");
  await expect(module.rememberGameplayRequest({ ...original, auth: { ...original.auth, nonce: 5555 } }, gameplayScope)).rejects.toThrow("original_gameplay_outcome_pending");
  waiting.reject(new Error("quota denied"));
  await failed;
  await expect(module.rememberGameplayRequest({ ...original, auth: { ...original.auth, nonce: 5555 } }, gameplayScope)).rejects.toThrow("original_gameplay_outcome_pending");
  expect(await module.settleGameplayRequest(original.auth.nonce, gameplayScope)).toBe(true);
  expect(module.hasPendingGameplayRequest()).toBe(false);
});

it("does not project or settle an original request in another or unverified world scope", async () => {
  let capsule;
  const recovery = { load: vi.fn(async () => capsule && structuredClone(capsule)), save: vi.fn(async value => { capsule = structuredClone(value); return true; }), clear: vi.fn(async () => {}) };
  const first = fixture(undefined, recovery);
  await first.module.join(account);
  await first.module.rememberGameplayRequest(signedGameplay(), gameplayScope);
  first.module.invalidate();
  const state = { auth: { available: false, source: "guest_only" } };
  let otherScope = { ...gameplayScope, worldId: "other-world" };
  const module = createHostedWorldAccount({ state, recovery, getGameplayScope: () => otherScope, generateKeypair: vi.fn(), register: vi.fn(async () => {}), render: vi.fn(), clearLegacy: vi.fn() });
  instances.push(module);
  await module.join(account);
  expect(state.lastGameplayActionFeedback).toBeUndefined();
  expect(module.hasPendingGameplayRequest()).toBe(true);
  expect(await module.settleGameplayRequest(1234, otherScope)).toBe(false);
  expect(await module.settleGameplayRequest(1234, null)).toBe(false);
  module.restoreGameplayRequest(null);
  expect(state.lastGameplayActionFeedback).toBeUndefined();
  otherScope = gameplayScope;
  module.restoreGameplayRequest(gameplayScope);
  expect(state.lastGameplayActionFeedback).toMatchObject({ stage: "unknown", requestAuthNonce: 1234, gameplayScope });
  const feedback = state.lastGameplayActionFeedback;
  feedback.stage = "pending";
  module.restoreGameplayRequest(gameplayScope);
  expect(state.lastGameplayActionFeedback).toBe(feedback);
  expect(feedback.stage).toBe("pending");
  otherScope = { ...gameplayScope, worldId: "other-world" };
  module.restoreGameplayRequest(otherScope);
  expect(state.lastGameplayActionFeedback).toBeNull();
  expect(capsule.gameplayRequest.auth.nonce).toBe(1234);
});

it("retains durable world-A original when the verified scope changes during persistence", async () => {
  let scope = gameplayScope;
  const recovery = { load: vi.fn(async () => null), save: vi.fn(async () => true), clear: vi.fn(async () => {}) };
  const state = { auth: { available: false, source: "guest_only" } };
  const module = createHostedWorldAccount({ state, recovery, getGameplayScope: () => scope, generateKeypair: vi.fn(async () => pair(1)), register: vi.fn(async () => {}), render: vi.fn(), clearLegacy: vi.fn() });
  instances.push(module);
  await module.join(account);
  const waiting = deferred(); recovery.save.mockReturnValueOnce(waiting.promise);
  const remembering = module.rememberGameplayRequest(signedGameplay(), gameplayScope);
  const rejected = expect(remembering).rejects.toThrow("world_recovery_context_changed");
  scope = { ...gameplayScope, worldId: "world-b" };
  waiting.resolve(true); await rejected;
  await expect(module.rememberGameplayRequest({ ...signedGameplay(), auth: { ...signedGameplay().auth, nonce: 9999 } }, scope)).rejects.toThrow("original_gameplay_outcome_pending");
  expect(await module.settleGameplayRequest(1234, gameplayScope)).toBe(false);
  scope = gameplayScope;
  module.restoreGameplayRequest(scope);
  expect(state.lastGameplayActionFeedback).toMatchObject({ stage: "unknown", requestAuthNonce: 1234 });
});

it("removes private unresolved gameplay projection on explicit account invalidation and release", async () => {
  const recovery = { load: vi.fn(async () => null), save: vi.fn(async () => true), clear: vi.fn(async () => {}) };
  const { module, state } = fixture(undefined, recovery);
  await module.join(account);
  state.lastGameplayActionFeedback = { kind: "gameplay_action", stage: "unknown", originalRequest: signedGameplay() };
  module.invalidate();
  expect(state.lastGameplayActionFeedback).toBeNull();
  await module.join(account);
  state.lastGameplayActionFeedback = { kind: "gameplay_action", stage: "unknown", originalRequest: signedGameplay() };
  await module.release();
  expect(state.lastGameplayActionFeedback).toBeNull();
});
