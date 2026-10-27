import { atBrowserGameStage } from "./browser_game_diagnostics.js";
import { createBrowserParticipantEntry, BrowserParticipantError } from "./browser_participant_entry.js";
import { createBrowserGameDatabase } from "./browser_game_identity.js";
import { deliveryPhases, operationObservation, terminalDisposition } from "./browser_operation_lifecycle.js";
const fail = (code, diagnostic) => { throw new BrowserParticipantError(code, diagnostic); };
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
 const build = (kind, input) => { try { return codec.build(kind, typeof input === "string" ? input : JSON.stringify(input)); } catch { fail("owner_proof_invalid", { stage: ({ task_create: "owner_task_build", goal_prepare: "owner_goal_build", goal_publish: "owner_goal_build" })[kind] || "owner_proof_build" }); } };
 const field = (raw, path) => build("field", { json: raw, path });
 const decoded = JSON.parse;
 const freshId = (name) => `browser-${name}-${hex(cryptoImpl.getRandomValues(new Uint8Array(16)))}`;
 function check(record) {
  if (record === undefined) return { schema: 2, key };
  if (![1, 2].includes(record?.schema) || record.key !== key || JSON.stringify(record).length > 262144) fail("owner_storage_corrupt");
  for (const slot of ["taskRaw", "publishRaw", "goalPrepareRaw"]) {
   if (record[`${slot}Phase`] !== undefined && !deliveryPhases.has(record[`${slot}Phase`])) fail("owner_storage_corrupt");
  }
  if (record.schema === 2) for (const slot of ["taskRaw", "publishRaw"]) {
   const phase = record[`${slot}SendState`];
   if (phase !== undefined && (!record[slot] || !["not_sent", "may_have_sent"].includes(phase)
    || (phase === "not_sent" && (record[`${slot}Receipt`] || record[`${slot}TerminalReply`]
      || (record[`${slot}Phase`] !== undefined && record[`${slot}Phase`] !== "prepared_unsent"))))) fail("owner_storage_corrupt");
  }
  return record;
 }
 async function load() { try { return check(await database.get(key)); } catch (error) { if (error instanceof BrowserParticipantError) throw error; fail("owner_storage_failed"); } }
 async function update(mutate) {
  if (!locks?.request) fail("participant_locks_unavailable");
  try { return await locks.request(`oasis7:${key}`, { mode: "exclusive" }, () => database.update(key, (record) => check(mutate(check(record))))); }
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
  const readRaw = JSON.stringify({ contract_version: 1, observation_nonce: hex(cryptoImpl.getRandomValues(new Uint8Array(32))), world: config.world, scope_id: `agent:${agentId}`, min_commit: null, fixed_commit: null, deadline_unix_ms: null });
  const read = await signed(VIEW, readRaw);
  return { raw: `{"owner":${JSON.stringify(proofs.owner)},"admission":${JSON.stringify(proofs.admission)},"read":${read},"response_profile":"owner_control_v1"}`, controlRaw: proofs.ownerControl.rawJson };
 }
 const ownerCall = (kind, requestRaw, mutation = false, beforeDispatch) => atBrowserGameStage(kind === "prepare_goal" ? "owner_goal_prepare" : "owner_control", () => callOwner(kind, requestRaw, mutation, beforeDispatch));
 async function callOwner(kind, requestRaw, mutation = false, beforeDispatch) {
  const accessView = await access(); active(); const accessRaw = accessView.raw;
  const rawBody = `{"access":${accessRaw},"operation":{"kind":${JSON.stringify(kind)},"request":${requestRaw}}}`;
  return participant.gatewayCall("/entry-owner/control", null, { rawBody, signal, beforeSend: mutation ? async () => { active(); await beforeDispatch?.(accessView.controlRaw); active(); } : undefined });
 }
 function verify(domain, requestRaw, reply, kind, validationRequest = requestRaw) {
  try { const raw = codec.verifyGatewayResponse(domain, requestRaw, reply, config.servicePublicKey); return codec.validate(kind, validationRequest, raw); }
  catch { fail("owner_response_unknown", { stage: kind === "goal_prepared" ? "owner_goal_prepare" : "owner_response_verify" }); }
 }
 function lookupMaterial(originalRaw, slot) {
  if (decoded(field(originalRaw, ["signed_payload", "kind"])) !== ({ taskRaw: "game_task_create", publishRaw: "game_goal_publish" })[slot]) fail("owner_original_conflict");
  const payload = field(originalRaw, ["signed_payload"]), correlation = field(originalRaw, ["correlation"]);
  const expected = build("correlation", { world: config.world, payload_json: payload });
  if (field(expected, []) !== field(correlation, [])) fail("owner_original_conflict");
  const request = `{"request":{"contract_version":1,"key":${field(correlation, ["key"])}},"original":${payload}}`;
  return { request, correlation };
 }
 function terminalStatus(slot, record) {
  if (!record[`${slot}TerminalReply`]) return "unknown";
  const { request, correlation } = lookupMaterial(record[slot], slot);
  const raw = verify(LOOKUP, request, record[`${slot}TerminalReply`], "lookup", correlation);
  const status = decoded(field(raw, ["outcome", "status"]));
  if (!terminalDisposition(status)) fail("owner_storage_corrupt");
  return status;
 }
 const lookup = (originalRaw, slot) => atBrowserGameStage(slot === "taskRaw" ? "owner_task_lookup" : "owner_goal_lookup", () => lookupOriginal(originalRaw, slot));
 async function lookupOriginal(originalRaw, slot) {
  const { request, correlation } = lookupMaterial(originalRaw, slot);
  const reply = await ownerCall("lookup", request);
  const candidate = verify(LOOKUP, request, reply, "lookup", correlation);
  const winner = await update((current) => {
   if (current[slot] !== originalRaw) fail("owner_original_conflict");
   const raw = current[`${slot}TerminalReply`]
    ? verify(LOOKUP, request, current[`${slot}TerminalReply`], "lookup", correlation) : candidate;
   const state = decoded(field(raw, ["outcome", "status"]));
   if (current[`${slot}TerminalReply`] && !terminalDisposition(state)) fail("owner_storage_corrupt");
   if (!current[`${slot}TerminalReply`] && terminalDisposition(state)) current[`${slot}TerminalReply`] = reply;
   current[`${slot}Observation`] = operationObservation({ phase: current[`${slot}Phase`], status: state, verified: true });
   return current;
  });
  const raw = winner[`${slot}TerminalReply`]
   ? verify(LOOKUP, request, winner[`${slot}TerminalReply`], "lookup", correlation) : candidate;
  const state = decoded(field(raw, ["outcome", "status"]));
  if (state !== "committed") {
   if (["rejected", "expired"].includes(state)) fail(state === "expired" ? "owner_operation_expired" : "owner_operation_rejected");
   fail("owner_operation_pending", { outcome: state });
  }
  return raw;
 }
 function scopeStamp(slot, controlRaw) {
  const owner = ["agent_id", "player_id", "public_key", "world_id", "reorg_epoch", "canonical_authority"]
   .map((name) => field(controlRaw, ["view", "canonical_agent_owner", name]));
  return JSON.stringify([owner, field(controlRaw, ["view", "game_task_control"])]);
 }
 async function originalIntent(slot, domain, kind, requestRaw, controlRaw) {
  let record = await load();
  if (record[slot]) return { raw: record[slot], fresh: false };
  const proof = await signed(domain, requestRaw), payload = `{"kind":${JSON.stringify(kind)},"payload":${proof}}`;
  const correlation = build("correlation", { world: config.world, payload_json: payload });
  const raw = `{"contract_version":1,"correlation":${correlation},"deadline_unix_ms":null,"signed_payload":${payload}}`;
  let fresh = false;
  record = await update((current) => {
   if (!current[slot]) { active(); current.schema = 2; current[slot] = raw; current[`${slot}SendState`] = "not_sent"; current[`${slot}Phase`] = "prepared_unsent"; current[`${slot}Scope`] = scopeStamp(slot, controlRaw); fresh = true; }
   return current;
  });
  if (record[slot] !== raw) fail("owner_original_conflict");
  return { raw, fresh };
 }
 async function claimDispatch(slot, original, controlRaw, replay = false) {
  active();
  await update((record) => {
   if (replay) {
    const expected = build("goal_prepare", { task_json: field(controlRaw, ["view", "game_task_control"]),
     client_operation_id: decoded(field(original, ["request", "client_operation_id"])),
     client_nonce: decoded(field(original, ["request", "client_nonce"])), content: decoded(field(original, ["request", "content"])) });
    if (field(expected, []) !== field(original, ["request"])) fail("owner_original_scope_changed");
   } else if (record[`${slot}Scope`] !== scopeStamp(slot, controlRaw)) fail("owner_original_scope_changed");
   if (record[slot] !== original || record[`${slot}TerminalReply`] || (!replay && !isUnsent(record, slot))) fail("owner_original_conflict");
   if (!replay) record[`${slot}SendState`] = "may_have_sent";
   record[`${slot}Phase`] = "dispatch_may_have_started"; return record;
  });
  active();
 }
 const isUnsent = (record, slot) => record.schema === 2 && record[`${slot}SendState`] === "not_sent"
  && record[`${slot}Phase`] === "prepared_unsent" && typeof record[`${slot}Scope`] === "string" && !record[`${slot}TerminalReply`];
 const unsentStatus = (record, slot) => ({ state: "unsent", code: "owner_original_not_sent", continuation: slot,
  operation: pendingOperation(record),
  ...(record.goalPrepareRaw ? { goal: decoded(field(record.goalPrepareRaw, ["request", "content", "goal_text"])) } : {}) });
 async function settleIntent(slot, original, send) {
  const record = await load();
  if (typeof original !== "string" || record[slot] !== original) fail("owner_original_conflict");
  lookupMaterial(original, slot);
  const terminal = terminalStatus(slot, record);
  const advice = operationObservation({ phase: record[`${slot}Phase`], status: terminal, verified: terminalDisposition(terminal),
   intent: send ? "continue_authorized_entry" : "query_only", writeAllowed: isUnsent(record, slot) });
  if (advice.nextAction === "continue_original") {
   try { verify(SUBMIT, original, await ownerCall("submit", original, true,
    (controlRaw) => claimDispatch(slot, original, controlRaw)), "intent", field(original, ["correlation"])); }
   catch (error) {
    if (error.code === "owner_original_scope_changed") throw error;
    /* The exact persisted original and durable dispatch marker own uncertainty. */
   }
  }
  active(); const current = await load();
  if (current[slot] !== original) fail("owner_original_conflict");
  if (isUnsent(current, slot)) fail("owner_original_not_sent");
  // The signed terminal reply already retains its payload. Avoid keeping a
  // second full receipt copy within the unchanged bounded owner record.
  return lookup(original, slot);
 }
 async function reconcile() {
  const record = await load();
  for (const slot of ["taskRaw", "publishRaw"]) if (record[slot] && !settled(slot, record)) await settleIntent(slot, record[slot], false);
 }
 function settled(slot, record) { return !!record[`${slot}Receipt`] || terminalStatus(slot, record) === "committed"; }
 function pendingOperation(record) {
  for (const slot of ["taskRaw", "publishRaw"]) if (record[slot] && !settled(slot, record)) {
   return { cacheSlot: slot, logicalOperation: decoded(field(record[slot], ["signed_payload", "payload", "request", "client_operation_id"])), originalKind: decoded(field(record[slot], ["signed_payload", "kind"])), correlation: decoded(field(record[slot], ["correlation"])),
    ...(record[`${slot}Observation`] ?? operationObservation({ phase: record[`${slot}Phase`] })) };
  }
  return null;
 }
 async function inspect() {
  active(); const binding = await participant.inspect(); active();
  if (binding.state !== "bound") return binding;
  const local = await load();
  for (const slot of ["taskRaw", "publishRaw"]) if (local[slot] && !settled(slot, local) && isUnsent(local, slot)) return { agentId: binding.agentId, ...unsentStatus(local, slot) };
  try { await reconcile(); }
  catch (error) { if (["owner_operation_pending", "owner_response_unknown"].includes(error.code)) return { state: "pending", code: error.code, diagnostic: error.diagnostic, agentId: binding.agentId, operation: pendingOperation(await load()) }; throw error; }
  const refreshed = await participant.getParticipantProofs(); active(); const controlRaw = refreshed.ownerControl.rawJson;
  const taskRaw = field(controlRaw, ["view", "game_task_control"]), task = decoded(taskRaw);
  const base = { agentId: binding.agentId, ownerControl: refreshed.ownerControl };
  if (!task) return { ...base, state: "needs_task" };
  const fence = decoded(build("fence", taskRaw));
  return { ...base, taskId: task.task_id, fence, state: task.active_goal_reference && task.active_goal_revision !== "0" ? "ready" : "needs_goal" };
 }
 async function prepare({ goal, consent } = {}, continueUnsent = false) {
  if (consent !== true) fail("owner_consent_required");
  if ((!continueUnsent || goal !== undefined) && (typeof goal !== "string" || !goal.trim() || new TextEncoder().encode(goal).byteLength > 8192)) fail("owner_goal_invalid");
  if (!locks?.request) fail("participant_locks_unavailable");
  return locks.request(`oasis7:prepare:${key}`, { mode: "exclusive" }, async () => {
   active(); let status = await inspect();
   if (status.state === "ready" || status.state === "pending" || (continueUnsent && status.state !== "unsent")) return status;
   if (status.state === "unsent") {
    if (!continueUnsent) return status;
    if (status.continuation === "participant") await participant.continueOriginal(true);
    else {
     const record = await load(), slot = status.continuation;
     if (!["taskRaw", "publishRaw"].includes(slot)) fail("owner_original_conflict");
     if (status.goal && goal !== undefined && status.goal !== goal.trim()) fail("owner_goal_original_conflict");
     await settleIntent(slot, record[slot], true);
    }
    status = await inspect();
    if (goal === undefined) return status;
   }
   if (status.state === "needs_participant") {
    try { await participant.registerParticipant(true); }
    catch (error) { if (error.code === "participant_original_not_sent") return { state: "unsent", code: error.code, continuation: "participant" };
     if (["participant_registration_pending", "participant_response_unknown", "participant_network_failed"].includes(error.code)) return { state: "pending", code: error.code, diagnostic: error.diagnostic }; throw error; }
    status = await inspect();
   }
   if (status.state === "needs_task") {
    const request = build("task_create", { owner_control_json: status.ownerControl.rawJson, client_operation_id: freshId("task") });
    const original = await originalIntent("taskRaw", "game_task_create", "game_task_create", request, status.ownerControl.rawJson);
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
    record = await update((current) => { if (!current.goalPrepareRaw) { current.goalPrepareRaw = original; current.goalPrepareRawPhase = "prepared_unsent"; } return current; });
    if (record.goalPrepareRaw !== original) fail("owner_original_conflict");
   }
   if (!record.goalReceiptRaw) {
    // Private preparation is replayed only with its identical persisted proof.
    const advice = operationObservation({ phase: record.goalPrepareRawPhase, policy: "private_preparation_replay",
     intent: "continue_authorized_entry", writeAllowed: true });
    if (advice.nextAction !== "continue_original") fail("owner_original_retained");
    const receipt = verify(GOAL, original, await ownerCall("prepare_goal", original, true,
     (controlRaw) => claimDispatch("goalPrepareRaw", original, controlRaw, true)), "goal_prepared", original);
    record = await update((current) => { current.goalReceiptRaw = receipt; return current; });
   }
   const publication = build("goal_publish", { original_json: original, receipt_json: record.goalReceiptRaw, client_operation_id: freshId("publish") });
   const publish = await originalIntent("publishRaw", "game_goal_publish/v1", "game_goal_publish", publication, status.ownerControl.rawJson);
   await settleIntent("publishRaw", publish.raw, publish.fresh);
   active(); return inspect();
  }).catch(async (error) => {
   if (error.code === "participant_original_not_sent") return { state: "unsent", code: error.code, continuation: "participant" };
   if (error.code === "owner_original_not_sent") {
    const record = await load();
    for (const slot of ["taskRaw", "publishRaw"]) if (record[slot] && isUnsent(record, slot)) return unsentStatus(record, slot);
   }
   if (["owner_operation_pending", "owner_response_unknown", "participant_response_unknown", "participant_network_failed"].includes(error.code)) return { state: "pending", code: error.code, diagnostic: error.diagnostic, operation: pendingOperation(await load()) };
   throw error;
  });
 }
 return { inspect, prepare: (options) => prepare(options), continueOriginal: (options) => prepare(options, true), recover: inspect, dispose: () => { disposed = true; } };
}
