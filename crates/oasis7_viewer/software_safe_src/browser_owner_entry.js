import { createBrowserParticipantEntry, BrowserParticipantError } from "./browser_participant_entry.js";
import { createBrowserGameDatabase } from "./browser_game_identity.js";
const fail = (code) => { throw new BrowserParticipantError(code); };
const hex = (bytes) => Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
const VIEW = "/v1/world/view-json", SUBMIT = "/v1/world/submit", LOOKUP = "/v1/world/lookup", GOAL = "/v1/world/private-goal/prepare";

// Explicit account entry is separate from credentials and executor activation.
export function createBrowserOwnerEntry({ config, codec, accountId, ownerSigner, indexedDB, storage,
 locks = globalThis.navigator?.locks, fetchImpl = globalThis.fetch, cryptoImpl = globalThis.crypto,
 signal, isCurrent = () => true } = {}) {
 let disposed = false;
 const participant = createBrowserParticipantEntry({ config, codec, accountId, ownerSigner, indexedDB, storage, locks, fetchImpl, cryptoImpl, signal, isCurrent: () => !disposed && isCurrent() });
 const database = storage ?? createBrowserGameDatabase(indexedDB);
 const key = `owner-entry:${JSON.stringify([config.world, accountId, ownerSigner.publicKeyHex])}`;
 const active = () => { if (disposed || signal?.aborted || !isCurrent()) fail("participant_stopped"); };
 const build = (kind, input) => { try { return codec.build(kind, typeof input === "string" ? input : JSON.stringify(input)); } catch { fail("owner_proof_invalid"); } };
 const field = (raw, path) => build("field", { json: raw, path });
 const decoded = JSON.parse;
 const freshId = (name) => `browser-${name}-${hex(cryptoImpl.getRandomValues(new Uint8Array(16)))}`;
 function check(record) {
  if (record === undefined) return { schema: 1, key };
  if (record?.schema !== 1 || record.key !== key || JSON.stringify(record).length > 262144) fail("owner_storage_corrupt");
  return record;
 }
 async function load() { try { return check(await database.get(key)); } catch (error) { if (error instanceof BrowserParticipantError) throw error; fail("owner_storage_failed"); } }
 async function update(mutate) {
  if (!locks?.request) fail("participant_locks_unavailable");
  try { return await locks.request(`oasis7:${key}`, { mode: "exclusive" }, () => database.update(key, (record) => mutate(check(record)))); }
  catch (error) { if (error instanceof BrowserParticipantError) throw error; fail("owner_storage_failed"); }
 }
 async function signed(domain, requestRaw) {
  active(); let signature;
  try { signature = hex(await ownerSigner.sign(codec.signingBytes(domain, requestRaw))); }
  catch { fail("participant_signing_failed"); }
  active(); return `{"request":${requestRaw},"subject_public_key":${JSON.stringify(ownerSigner.publicKeyHex)},"signature_hex":${JSON.stringify(signature)}}`;
 }
 async function access() {
  active(); const proofs = await participant.getParticipantProofs(); active();
  const agentId = decoded(field(proofs.ownerControl.rawJson, ["view", "canonical_agent_owner", "agent_id"]));
  const readRaw = JSON.stringify({ contract_version: 1, observation_nonce: hex(cryptoImpl.getRandomValues(new Uint8Array(32))), formal_anchor: null, world: config.world, scope_id: `agent:${agentId}`, min_commit: null, fixed_commit: null, deadline_unix_ms: null });
  const read = await signed(VIEW, readRaw);
  return `{"owner":${JSON.stringify(proofs.owner)},"admission":${JSON.stringify(proofs.admission)},"read":${read},"response_profile":"owner_control_v1"}`;
 }
 async function ownerCall(kind, requestRaw, mutation = false) {
  const accessRaw = await access(); active();
  const rawBody = `{"access":${accessRaw},"operation":{"kind":${JSON.stringify(kind)},"request":${requestRaw}}}`;
  return participant.gatewayCall("/entry-owner/control", null, { rawBody, signal, beforeSend: mutation ? async () => active() : undefined });
 }
 function verify(domain, requestRaw, reply, kind, validationRequest = requestRaw) {
  try { const raw = codec.verifyGatewayResponse(domain, requestRaw, reply, config.servicePublicKey); return codec.validate(kind, validationRequest, raw); }
  catch { fail("owner_response_unknown"); }
 }
 async function lookup(originalRaw) {
  const payload = field(originalRaw, ["signed_payload"]), correlation = field(originalRaw, ["correlation"]);
  const expected = build("correlation", { world: config.world, payload_json: payload });
  if (field(expected, []) !== field(correlation, [])) fail("owner_original_conflict");
  const request = `{"request":{"contract_version":1,"key":${field(correlation, ["key"])}},"original":${payload}}`;
  const raw = verify(LOOKUP, request, await ownerCall("lookup", request), "lookup", correlation);
  const state = decoded(field(raw, ["outcome", "status"]));
  if (state !== "committed") {
   if (["rejected", "expired"].includes(state)) fail("owner_operation_rejected");
   fail("owner_operation_pending");
  }
  return raw;
 }
 async function originalIntent(slot, domain, kind, requestRaw) {
  let record = await load();
  if (record[slot]) return { raw: record[slot], fresh: false };
  const proof = await signed(domain, requestRaw), payload = `{"kind":${JSON.stringify(kind)},"payload":${proof}}`;
  const correlation = build("correlation", { world: config.world, payload_json: payload });
  const raw = `{"contract_version":1,"correlation":${correlation},"deadline_unix_ms":null,"signed_payload":${payload}}`;
  record = await update((current) => { if (!current[slot]) current[slot] = raw; return current; });
  if (record[slot] !== raw) fail("owner_original_conflict");
  return { raw, fresh: true };
 }
 async function settleIntent(slot, original, send) {
  if (send) {
   try { verify(SUBMIT, original, await ownerCall("submit", original, true), "intent", field(original, ["correlation"])); }
   catch { /* The persisted original owns any unknown effect. Never replace it. */ }
  }
  const receipt = await lookup(original);
  await update((record) => { record[`${slot}Receipt`] = receipt; return record; });
  return receipt;
 }
 async function reconcile() {
  const record = await load();
  for (const slot of ["taskRaw", "publishRaw"]) if (record[slot] && !record[`${slot}Receipt`]) await settleIntent(slot, record[slot], false);
 }
 async function inspect() {
  active(); const binding = await participant.inspect(); active();
  if (binding.state !== "bound") return binding;
  try { await reconcile(); }
  catch (error) { if (["owner_operation_pending", "owner_response_unknown"].includes(error.code)) return { state: "pending", code: error.code, agentId: binding.agentId }; throw error; }
  const refreshed = await participant.getParticipantProofs(); active(); const controlRaw = refreshed.ownerControl.rawJson;
  const taskRaw = field(controlRaw, ["view", "game_task_control"]), task = decoded(taskRaw);
  const base = { agentId: binding.agentId, ownerControl: refreshed.ownerControl };
  if (!task) return { ...base, state: "needs_task" };
  const fence = decoded(build("fence", taskRaw));
  return { ...base, taskId: task.task_id, fence, state: task.active_goal_reference && task.active_goal_revision !== "0" ? "ready" : "needs_goal" };
 }
 async function prepare({ goal, consent } = {}) {
  if (consent !== true) fail("owner_consent_required");
  if (typeof goal !== "string" || !goal.trim() || new TextEncoder().encode(goal).byteLength > 8192) fail("owner_goal_invalid");
  if (!locks?.request) fail("participant_locks_unavailable");
  return locks.request(`oasis7:prepare:${key}`, { mode: "exclusive" }, async () => {
   active(); let status = await inspect();
   if (status.state === "ready" || status.state === "pending") return status;
   if (status.state === "needs_participant") {
    try { await participant.registerParticipant(true); }
    catch (error) { if (["participant_registration_pending", "participant_response_unknown"].includes(error.code)) return { state: "pending", code: error.code }; throw error; }
    status = await inspect();
   }
   if (status.state === "needs_task") {
    const request = build("task_create", { owner_control_json: status.ownerControl.rawJson, client_operation_id: freshId("task") });
    const original = await originalIntent("taskRaw", "game_task_create", "game_task_create", request);
    await settleIntent("taskRaw", original.raw, original.fresh); status = await inspect();
   }
   if (status.state !== "needs_goal") return status;
   let record = await load(), original = record.goalPrepareRaw;
   const content = { goal_text: goal.trim(), soft_constraints: [] };
   if (original && field(original, ["request", "content"]) !== JSON.stringify(content)) {
    const old = decoded(field(original, ["request", "content"]));
    if (old.goal_text !== content.goal_text || JSON.stringify(old.soft_constraints) !== "[]") fail("owner_goal_original_conflict");
   }
   if (!original) {
    const taskRaw = field(status.ownerControl.rawJson, ["view", "game_task_control"]);
    const request = build("goal_prepare", { task_json: taskRaw, client_operation_id: freshId("goal"), client_nonce: hex(cryptoImpl.getRandomValues(new Uint8Array(32))), content });
    original = await signed("game_goal_prepare_private/v1", request);
    record = await update((current) => { if (!current.goalPrepareRaw) current.goalPrepareRaw = original; return current; });
    if (record.goalPrepareRaw !== original) fail("owner_original_conflict");
   }
   if (!record.goalReceiptRaw) {
    // Private preparation is replayed only with its identical persisted proof.
    const receipt = verify(GOAL, original, await ownerCall("prepare_goal", original, true), "goal_prepared", original);
    record = await update((current) => { current.goalReceiptRaw = receipt; return current; });
   }
   const publication = build("goal_publish", { original_json: original, receipt_json: record.goalReceiptRaw, client_operation_id: freshId("publish") });
   const publish = await originalIntent("publishRaw", "game_goal_publish/v1", "game_goal_publish", publication);
   await settleIntent("publishRaw", publish.raw, publish.fresh);
   active(); return inspect();
  }).catch((error) => {
   if (["owner_operation_pending", "owner_response_unknown", "participant_response_unknown"].includes(error.code)) return { state: "pending", code: error.code };
   throw error;
  });
 }
 return { inspect, prepare, recover: inspect, dispose: () => { disposed = true; } };
}
