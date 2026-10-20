import { cborCanonicalEncode, signAuthPayload } from "./viewer_auth_crypto.js";
import { canonicalAgentChatView } from "./viewer_canonical_owner_read_module.js";
const uuid = () => crypto.randomUUID();
const equal = (a, b) => { const x = cborCanonicalEncode(a); const y = cborCanonicalEncode(b); return x.length === y.length && x.every((v, i) => v === y[i]); };
export function createSafeAlternativeModule({ state, sendJson, render, refreshOwner }) {
  state.safeAlternative = { record: null, original: null, status: "idle", error: "" };
  const storageKey = () => `oasis7.safe-alternative.v1:${state.auth.publicKey}:${state.auth.boundAgentId}:${state.worldFeed?.worldId}`;
  function saved() {
    const text = sessionStorage.getItem(storageKey());
    return text ? JSON.parse(text) : null;
  }
  async function command(operation) {
    const ui = state.safeAlternative;
    if (saved() || (ui.original && ui.status === "unknown")) throw new Error("Recover the original command first");
    await refreshOwner();
    const view = state.canonicalAgentOwnerView || canonicalAgentChatView(state);
    if (operation?.operation === "propose") {
      const original = operation.data?.original;
      if (!original?.goal?.trim() || !Array.isArray(original.waypoints) || !original.waypoints.length || original.waypoints.length > 8
        || original.waypoints.some(p => ![p.x_cm, p.y_cm, p.z_cm].every(Number.isSafeInteger))
        || (original.arrival_tick != null && (!Number.isSafeInteger(original.arrival_tick) || original.arrival_tick < 0))) throw new Error("Valid original goal, centimeter coordinates and optional arrival tick required");
      if (original.waypoints.length === 1 && original.arrival_tick == null) throw new Error("A single untimed destination is already supported; provide the actual route or timing constraint");
    }
    const world = state.canonicalOwnerWorldIdentity;
    if (!view || !world || state.worldFeed?.stale === true || view.world_id !== state.worldFeed?.worldId || view.reorg_epoch !== state.worldFeed?.reorgEpoch || view.player_id !== state.auth.playerId || view.public_key !== state.auth.publicKey || !world || world.world_id !== view.world_id || view.agent_id !== state.auth.boundAgentId) throw new Error("Current owner authority is required");
    const request = { api_rev: 1, client_operation_id: uuid(), world_id: world.world_id,
      genesis_digest: world.genesis_digest, branch_id: view.canonical_authority.branch_id,
      reorg_epoch: view.reorg_epoch, agent_id: view.agent_id, owner_binding: view.player_id,
      owner_generation: view.canonical_authority.agent_identity_generation, operation };
    const domain = `safe_alternative.${operation.operation}.v1`;
    const signature = await signAuthPayload(cborCanonicalEncode(["oasis7.world-service.v1", domain, request]), state.auth);
    if (!/^awviewauth:v1:[0-9a-f]{128}$/.test(signature) || !equal(view, state.canonicalAgentOwnerView || canonicalAgentChatView(state))) throw new Error("Owner authority changed while signing");
    const original = { request, subject_public_key: view.public_key, signature_hex: signature.slice(14) };
    // Store before the first send. Transport failures retain the exact signed command.
    sessionStorage.setItem(storageKey(), JSON.stringify(original));
    ui.original = original; ui.status = "unknown"; ui.error = "";
    sendJson({ type: "safe_alternative", request: original }); render();
  }
  function recover(retry = false) {
    const original = state.safeAlternative.original || saved();
    if (!original) throw new Error("No original command to recover");
    if (original.subject_public_key !== state.auth.publicKey || original.request.owner_binding !== state.auth.playerId
      || original.request.agent_id !== state.auth.boundAgentId || original.request.world_id !== state.worldFeed?.worldId
      || original.request.reorg_epoch !== state.worldFeed?.reorgEpoch) throw new Error("Original command belongs to another owner or world authority");
    state.safeAlternative.original = original; state.safeAlternative.status = "unknown";
    sendJson({ type: "safe_alternative", request: original, recover: !retry }); render();
  }
  function receive(message) {
    const ui = state.safeAlternative;
    if (!ui.original || !equal(message.request, ui.original)) return;
    const command = ui.original.request;
    if (command.agent_id !== state.auth.boundAgentId || command.owner_binding !== state.auth.playerId
      || command.reorg_epoch !== state.worldFeed?.reorgEpoch || command.world_id !== state.worldFeed?.worldId
      || ui.original.subject_public_key !== state.auth.publicKey) return;
    const owner = state.canonicalAgentOwnerView;
    if (owner && (owner.canonical_authority?.branch_id !== command.branch_id
      || owner.canonical_authority?.agent_identity_generation !== command.owner_generation)) return;
    const result = message.result;
    const status = result?.outcome?.status;
    ui.error = result?.error || "";
    if (status === "committed") {
      const record = result.outcome.receipt;
      if (record?.request_id !== command.operation.data.request_id || record?.agent_id !== command.agent_id) {
        ui.error = "Mismatched alternative receipt"; ui.status = "unknown";
      } else { ui.record = record; ui.status = record.state; ui.original = null; sessionStorage.removeItem(storageKey()); }
    } else if (["rejected", "expired"].includes(status)) {
      ui.error = JSON.stringify(result.outcome); ui.status = status; ui.original = null; sessionStorage.removeItem(storageKey());
    } else { ui.status = "unknown"; }
    render();
  }
  return { command, recover, receive, uuid };
}
