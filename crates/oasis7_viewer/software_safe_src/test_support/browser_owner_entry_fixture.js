import { createBrowserOwnerEntry } from "../browser_owner_entry.js";
export const ownerKey = "44".repeat(32);
export const config = { endpoint: "/api/public/hosted-account/v2", servicePublicKey: "11".repeat(32), participantIssuerPublicKey: "22".repeat(32), world: { world_id: "world", genesis_digest: "genesis" }, identity: { service_id: "identity", environment_id: "test", instance_epoch: "1" } };
export function fixture() {
 const records = new Map(), calls = [], effects = new Set(); let bound = false, task = null, originalOwner, unknown = false, rejectOwner = false, unknownOwnerKind = null;
 const storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
 const respond = (value) => new Response(JSON.stringify({ api_version: 2, ok: true, ...value }));
 const control = () => ({ view: { canonical_agent_owner: { agent_id: "canonical-agent", public_key: ownerKey, player_id: "player", world_id: "world" }, game_task_control: task } });
 const codec = {
  signingBytes: () => new Uint8Array([1]), validate: (_kind, _request, raw) => raw,
  verifyGatewayResponse: (_domain, _request, raw) => { const value = JSON.parse(raw); if (!value.world_response) throw new Error("unverified"); return JSON.stringify(value.world_response); },
  build(kind, input) {
   const value = JSON.parse(input);
   if (kind === "field") return JSON.stringify(value.path.reduce((current, name) => current[name], JSON.parse(value.json)));
   if (kind === "participant_agent_id") return JSON.stringify("canonical-agent");
   if (kind === "fence") return input;
   if (kind === "correlation") { const payload = value.payload ?? JSON.parse(value.payload_json); return JSON.stringify({ key: { operation: payload.payload?.request?.client_operation_id ?? payload.kind }, payload_digest: payload.kind }); }
   if (kind === "task_create") return JSON.stringify({ client_operation_id: value.client_operation_id, agent_id: "canonical-agent" });
   if (kind === "goal_prepare") return JSON.stringify({ client_operation_id: value.client_operation_id, client_nonce: value.client_nonce, content: value.content, task_id: "canonical-task" });
   if (kind === "goal_publish") return JSON.stringify({ client_operation_id: value.client_operation_id, task_id: "canonical-task" });
   throw new Error("unsupported");
  },
 };
 const fetchImpl = async (url, options) => {
  const path = url.slice(config.endpoint.length); const body = options?.body ? JSON.parse(options.body) : null; calls.push({ path, body, rawBody: options?.body });
  if (path === "/entry/discovery") return respond({ schema_revision: 1, world: config.world, world_service_public_key: config.servicePublicKey, identity: config.identity, participant_policy: { policy: { issuer_public_key: config.participantIssuerPublicKey } }, phase_capabilities: { participant_recovery: true } });
  if (path === "/session") return respond({ account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) });
  if (path === "/participant/authorize") { originalOwner = body; return respond({ phase: "participant_authorized", new_registration_enabled: true, admission: { subject_public_key: config.participantIssuerPublicKey, request: {} } }); }
  if (path === "/participant/owner-view") return respond({ world_response: bound && !rejectOwner ? control() : null });
  if (path === "/participant/submit" || path === "/participant/lookup") {
   if (path.endsWith("submit") && !rejectOwner) bound = true;
   return respond({ world_response: { outcome: { status: rejectOwner ? "rejected" : unknown || !bound ? "unknown" : "committed", receipt: { account_id: "account", owner_public_key: originalOwner.subject_public_key, agent_id: "canonical-agent" } } } });
  }
  if (path === "/entry-owner/control") {
   if (body.access.read.request.formal_anchor) throw new Error("fixture formal anchor forbidden");
   const operation = body.operation;
   if (operation.kind === "prepare_goal") return respond({ world_response: { owner_proof: operation.request, goal_ref: "fixture-goal" } });
   const payload = operation.kind === "submit" ? operation.request.signed_payload : operation.request.original;
   if (operation.kind === "submit") effects.add(JSON.stringify(payload));
   if (operation.kind === "submit" && payload.kind === "game_task_create") task = { task_id: "canonical-task", agent_id: "canonical-agent", active_goal_revision: "0", active_goal_reference: null };
   if (operation.kind === "submit" && payload.kind === "game_goal_publish") task = { ...task, active_goal_revision: "1", active_goal_reference: { goal_ref: "fixture-goal" } };
   return respond({ world_response: { outcome: { status: unknown || unknownOwnerKind === payload.kind || !effects.has(JSON.stringify(payload)) ? "unknown" : "committed", receipt: { original: payload.kind } } } });
  }
  throw new Error(`unexpected fixture route ${path}`);
 };
 const create = (extra = {}) => createBrowserOwnerEntry({ config, codec, accountId: "account", ownerSigner: { publicKeyHex: ownerKey, sign: async () => new Uint8Array(64) }, storage, locks: { request: async (_name, _options, run) => run() }, fetchImpl, cryptoImpl: { getRandomValues: (bytes) => bytes.fill(7) }, ...extra });
 return { create, calls, records, fetchImpl, storage, setOwnerUnknown: (kind) => { unknownOwnerKind = kind; }, setUnknown: (value) => { unknown = value; }, existing: () => { bound = true; task = { task_id: "canonical-task", agent_id: "canonical-agent", active_goal_revision: "1", active_goal_reference: { goal_ref: "existing" } }; }, mismatch: () => { rejectOwner = true; } };
}

export function sessionFailure(f, slot) {
 let failed = false;
 return async (url, options) => {
  if (!failed && url.endsWith("/session") && [...f.records.values()].some(r => r[slot] && !r[`${slot}Receipt`])) {
   failed = true; throw new Error("offline before submit");
  }
  return f.fetchImpl(url, options);
 };
}
