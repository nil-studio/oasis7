import { cborCanonicalEncode } from "./viewer_auth_crypto.js";

const DOMAIN = "/v1/world/view";
const hex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
const canonicalHex = (value) => hex(cborCanonicalEncode(value));

function validCommit(commit) {
  return Number.isSafeInteger(commit?.position) && commit.position >= 0
    && [commit?.execution_block_hash, commit?.state_root_ref, commit?.binding?.provider_world_id,
      commit?.binding?.branch_id, commit?.binding?.finality_ref, commit?.binding?.governing_manifest_ref]
      .every(value => typeof value === "string" && !!value.trim())
    && [commit?.binding?.reorg_generation, commit?.binding?.authority_generation, commit?.binding?.permission_generation]
      .every(value => Number.isSafeInteger(value) && value >= 0);
}

export function canonicalAgentChatView(state) {
  if (state.viewerProtocol?.capabilities?.includes("canonical_agent_chat_v1")) {
    const view = state.canonicalAgentOwnerView;
    return view && view.player_id === state.auth.playerId && view.public_key === state.auth.publicKey
      && view.agent_id === state.auth.boundAgentId && view.world_id === state.worldFeed?.worldId
      && view.reorg_epoch === state.worldFeed?.reorgEpoch && state.worldFeed?.stale !== true ? view : null;
  }
  return state.snapshot?.player_gameplay?.canonical_agent_chat || null;
}

export function createCanonicalOwnerReadModule({ state, sendJson, signAuthPayload, render, getSocket }) {
  let pending = null;
  function identity(agentId) {
    return { agentId, playerId: state.auth.playerId, publicKey: state.auth.publicKey,
      worldId: state.worldFeed?.worldId, reorgEpoch: state.worldFeed?.reorgEpoch };
  }
  function current(id) {
    return state.auth.available && state.auth.registrationStatus === "registered"
      && state.auth.boundAgentId === id.agentId && state.auth.playerId === id.playerId
      && state.auth.publicKey === id.publicKey && state.worldFeed?.worldId === id.worldId
      && state.worldFeed?.reorgEpoch === id.reorgEpoch && state.worldFeed?.stale !== true;
  }
  function matchesSubject(subject, id) {
    return subject?.agent_id === id.agentId && subject?.player_id === id.playerId
      && subject?.public_key === id.publicKey && subject?.world_id === id.worldId
      && subject?.reorg_epoch === id.reorgEpoch;
  }
  function matchesFence(fence, id) {
    return matchesSubject(fence, id) && typeof fence?.canonical_authority?.branch_id === "string"
      && !!fence.canonical_authority.branch_id && Number.isSafeInteger(fence.canonical_authority.agent_identity_generation)
      && fence.canonical_authority.agent_identity_generation > 0;
  }
  function fail(reason) {
    state.canonicalAgentOwnerView = null;
    const active = pending;
    pending = null;
    if (active) { clearTimeout(active.timer); active.reject(new Error(reason)); }
    render();
  }
  function refresh(agentId = state.auth.boundAgentId, force = false) {
    if (!force && !state.viewerProtocol?.capabilities?.includes("canonical_agent_chat_v1")) return Promise.resolve(null);
    const id = identity(agentId);
    if (!current(id) || !getSocket() || getSocket().readyState !== WebSocket.OPEN) {
      fail("canonical owner read requires the current registered connection");
      return Promise.reject(new Error("canonical owner read requires the current registered connection"));
    }
    if (pending) {
      if (canonicalHex(pending.id) === canonicalHex(id)) return pending.promise;
      fail("canonical owner read identity changed");
    }
    state.canonicalAgentOwnerView = null;
    let resolve; let reject;
    const promise = new Promise((ok, error) => { resolve = ok; reject = error; });
    pending = { id, promise, resolve, reject, timer: setTimeout(() => fail("canonical owner read timed out"), 10000) };
    sendJson({ type: "request_canonical_agent_owner_read_context", agent_id: agentId });
    return promise;
  }
  async function handleContext(context) {
    const active = pending;
    if (!active) return;
    try {
      const request = context?.request;
      const fence = context?.fence;
      const expected = context?.expected;
      if (!current(active.id) || !matchesSubject(expected, active.id)
        || typeof expected?.branch_id !== "string" || !expected.branch_id.trim()
        || (fence != null && (!matchesFence(fence, active.id) || fence.canonical_authority.branch_id !== expected.branch_id)) || context.signing_domain !== DOMAIN
        || request?.contract_version !== 1 || request?.world?.world_id !== active.id.worldId
        || (typeof request?.world?.genesis_digest !== "string" || !request.world.genesis_digest.trim()) || request.scope_id !== `agent:${active.id.agentId}`
        || !validCommit(request.min_commit) || request.fixed_commit !== null || request.deadline_unix_ms !== null
        || canonicalHex(request.min_commit.world) !== canonicalHex(request.world)
        || request.min_commit.binding?.branch_id !== expected.branch_id
        || request.min_commit.binding?.reorg_generation !== active.id.reorgEpoch) throw new Error("canonical owner read context mismatch");
      const signingBytes = cborCanonicalEncode(["oasis7.world-service.v1", DOMAIN, request]);
      if (hex(signingBytes) !== context.signing_bytes_hex) throw new Error("canonical owner read signing bytes mismatch");
      const signature = await signAuthPayload(signingBytes, state.auth);
      if (pending !== active || !current(active.id)) throw new Error("canonical owner read identity changed during signing");
      if (!/^awviewauth:v1:[0-9a-f]{128}$/.test(signature)) throw new Error("canonical owner read signature invalid");
      active.fence = fence == null ? null : structuredClone(fence);
      active.expected = structuredClone(expected);
      active.signed = { request: structuredClone(request), subject_public_key: active.id.publicKey, signature_hex: signature.slice("awviewauth:v1:".length) };
      sendJson({ type: "canonical_agent_owner_read", request: active.signed });
    } catch (error) { if (pending === active) fail(String(error)); }
  }
  function handleView(message) {
    const active = pending;
    if (!active?.signed) return;
    try {
      if (!current(active.id) || !/^blake3:[0-9a-f]{64}$/.test(message?.request_digest || "")
        || canonicalHex(message.request) !== canonicalHex(active.signed) || !matchesFence(message.view, active.id)
        || message.view.canonical_authority.branch_id !== active.expected.branch_id
        || (active.fence != null && canonicalHex(message.view.canonical_authority) !== canonicalHex(active.fence.canonical_authority))
        || canonicalHex(message.version?.commit?.world) !== canonicalHex(active.signed.request.world)
        || canonicalHex(message.version?.commit?.binding) !== canonicalHex(active.signed.request.min_commit.binding)
        || !validCommit(message.version?.commit)
        || message.version.commit.position < active.signed.request.min_commit.position
        || (message.version.commit.position === active.signed.request.min_commit.position
          && canonicalHex(message.version.commit) !== canonicalHex(active.signed.request.min_commit))) throw new Error("canonical owner read response mismatch");
      state.canonicalAgentOwnerView = structuredClone(message.view);
      state.canonicalOwnerWorldIdentity = structuredClone(active.signed.request.world);
      pending = null; clearTimeout(active.timer); active.resolve(state.canonicalAgentOwnerView); render();
    } catch (error) { if (pending === active) fail(String(error)); }
  }
  return { refresh, handleContext, handleView, reset: () => fail("canonical owner read connection reset") };
}
