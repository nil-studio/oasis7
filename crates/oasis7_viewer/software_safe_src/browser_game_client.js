// WorldService task lane only. The WASM codec owns canonical bytes and typed proofs.
export class BrowserGameError extends Error {
 constructor(code) { super(code); this.name = "BrowserGameError"; this.code = code; }
}
const fail = (code) => { throw new BrowserGameError(code); };
const json = (value) => JSON.stringify(value);
const copy = (value) => JSON.parse(json(value));
const hex = (bytes) => Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
const same = (a, b) => json(a) === json(b);
export const WORLD_PATHS = Object.freeze({ describe: "/v1/world/describe", owner: "/v1/world/participant-owner-view", view: "/v1/world/view-json", submit: "/v1/world/submit", lookup: "/v1/world/lookup", snapshot: "/v1/world/private-task/goal-snapshot", prepare: "/v1/world/private-task/context-prepare", read: "/v1/world/private-task/context-read", recipe: "/v1/world/private-task/recipe-quote", factory: "/v1/world/private-task/factory-quote" });
const terminal = (status) => ["committed", "rejected", "expired"].includes(status);
export function createBrowserGameClient({ config, codec, ownerSigner, executorSigner, participantProofs, agentId, taskId, recovery, fetchImpl = globalThis.fetch, gatewayCall, cryptoImpl = globalThis.crypto, signal, beforeEffect = async () => {} } = {}) {
 if (config?.endpoint !== "/api/public/hosted-account/v2" || !/^[a-f0-9]{64}$/.test(config?.servicePublicKey ?? "") || !config?.world?.world_id || !config?.world?.genesis_digest || !agentId || !codec || !ownerSigner || !executorSigner || !recovery) fail("game_configuration_invalid");
 const world = copy(config.world), serviceKey = config.servicePublicKey;
 let boundFence = null;
 const rawValues = new WeakMap();
 const json = (value) => {
  if (value && typeof value === "object") {
   if (rawValues.has(value)) return rawValues.get(value);
   if (Array.isArray(value)) return `[${value.map(json).join(",")}]`;
   return `{${Object.entries(value).filter(([, item]) => item !== undefined).map(([name, item]) => `${JSON.stringify(name)}:${json(item)}`).join(",")}}`;
  }
  return JSON.stringify(value);
 };
 const nonce = () => hex(cryptoImpl.getRandomValues(new Uint8Array(32)));
 function decoded(value) {
  if (typeof value !== "string") return value;
  const result = JSON.parse(value);
  if (result && typeof result === "object") rawValues.set(result, value);
  return result;
 }
 function field(value, path) {
  return decoded(codec.build("field", JSON.stringify({ json: json(value), path })));
 }
 function build(kind, value) { try { return decoded(codec.build(kind, json(value))); } catch { fail("game_proof_invalid"); } }
 function validate(kind, request, payload) { try { return decoded(codec.validate(kind, json(request), json(payload))); } catch { fail("game_proof_invalid"); } }
 async function sign(domain, request, signer = executorSigner) {
  if (signal?.aborted) fail("stopped");
  try { return { request, subject_public_key: signer.publicKeyHex, signature_hex: hex(await signer.sign(codec.signingBytes(domain, json(request)))) }; }
  catch { fail("game_signing_failed"); }
 }
 async function ownerAccess() {
  const proofs = typeof participantProofs === "function" ? await participantProofs() : participantProofs;
  if (!proofs?.owner || !proofs?.admission) fail("participant_proofs_required");
  return { owner: proofs.owner, admission: proofs.admission, read: await sign(WORLD_PATHS.view, readRequest(), ownerSigner), response_profile: "owner_control_v1" };
 }
 async function gatewayEnvelope(path, wire) {
  if (path === WORLD_PATHS.owner) return { path: "/participant/owner-view", body: wire };
  if (path === WORLD_PATHS.submit || path === WORLD_PATHS.lookup) {
   const payload = path === WORLD_PATHS.submit ? wire.signed_payload : wire.original;
   const ownerOperation = ["delegation", "game_executor_control"].includes(payload.kind);
   if (ownerOperation || path === WORLD_PATHS.lookup) return { path: "/entry-owner/control", body: { access: await ownerAccess(), operation: { kind: path === WORLD_PATHS.submit ? "submit" : ownerOperation ? "lookup" : "lookup_executor", request: wire } } };
  }
  // Replaying the identical read-only prepare proof can recover its result on
  // a new page; it does not grant this page an active execution binding.
  const accessFence = boundFence ?? ([WORLD_PATHS.prepare, WORLD_PATHS.snapshot].includes(path) ? wire.request?.fence : null);
  if (!accessFence) fail("executor_not_bound");
  const kinds = { [WORLD_PATHS.describe]: "describe", [WORLD_PATHS.view]: "read_view", [WORLD_PATHS.submit]: "submit", [WORLD_PATHS.lookup]: "lookup", [WORLD_PATHS.snapshot]: "goal_snapshot", [WORLD_PATHS.prepare]: "prepare_context", [WORLD_PATHS.read]: "read_context", [WORLD_PATHS.recipe]: "recipe_quote", [WORLD_PATHS.factory]: "factory_quote" };
  if (!kinds[path]) fail("game_route_unsupported");
  return { path: "/entry-executor/control", body: { access: { read: await sign(WORLD_PATHS.view, readRequest()), fence: accessFence }, operation: { kind: kinds[path], request: wire } } };
 }
 async function call(path, wire, digestRequest, kind, validationRequest, mutationGate) {
  if (signal?.aborted) fail("stopped");
  let response;
  try {
   const gateway = await gatewayEnvelope(path, wire);
   let signed;
   if (gatewayCall) {
    const reply = await gatewayCall(gateway.path, gateway.body, { rawBody: json(gateway.body), beforeSend: mutationGate, signal });
    signed = typeof reply === "string" ? reply : JSON.stringify(reply);
   } else {
   const cap = await fetchImpl(`${config.endpoint}/capabilities`, { credentials: "same-origin", mode: "same-origin", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer", signal });
   const csrf = (await cap.json())?.csrf_token;
   if (!cap.ok || typeof csrf !== "string" || !csrf) fail("authentication_required");
   await mutationGate?.();
   if (signal?.aborted) fail("stopped");
   response = await fetchImpl(`${config.endpoint}${gateway.path}`, { method: "POST", credentials: "same-origin", mode: "same-origin", redirect: "error", referrerPolicy: "no-referrer", cache: "no-store", headers: { "Content-Type": "application/json", "X-Oasis7-CSRF": csrf }, body: json(gateway.body), signal });
   if (!response.ok) fail("game_request_failed");
   if (!response.body?.getReader) fail("game_response_invalid");
   const reader = response.body.getReader(), decoder = new TextDecoder("utf-8", { fatal: true });
   let text = "", size = 0;
   try { for (;;) { const { value, done } = await reader.read(); if (done) break; size += value.byteLength; if (size > 2 * 1024 * 1024) fail("game_response_limit"); text += decoder.decode(value, { stream: true }); } text += decoder.decode(); }
   finally { try { await reader.cancel(); } catch { /* no raw errors */ } }
   signed = text;
   }
   const raw = codec.verifyGatewayResponse(path, json(digestRequest), signed, serviceKey);
   const payload = validate(kind, validationRequest, decoded(raw));
   if (payload?.formal_profile || payload?.execution_evidence_scope != null) fail("unsupported_world_evidence");
   // Attach canonical raw nested values needed by later signed requests/builders.
   for (const nestedPath of [["identity"], ["prepared"], ["prepared", "identity"], ["snapshot"], ["current_public_context"], ["current_public_context", "captured_base_binding"], ["quote"], ["quote", "action"]]) {
    let parent = payload;
    for (const name of nestedPath.slice(0, -1)) parent = parent?.[name];
    const last = nestedPath.at(-1);
    if (parent?.[last] && typeof parent[last] === "object") parent[last] = field(payload, nestedPath);
   }
   return payload;
  } catch (error) { if (error instanceof BrowserGameError) throw error; fail("game_response_invalid"); }
 }
 const readRequest = () => ({ observation_nonce: nonce(), contract_version: 1, world, scope_id: `agent:${agentId}`, min_commit: null, fixed_commit: null, deadline_unix_ms: null });
 async function describe() {
  const request = { contract_version: 1, expected_world: world, trust_config_ref: serviceKey };
  const proof = await sign(WORLD_PATHS.describe, request);
  return call(WORLD_PATHS.describe, proof, proof, "describe", world);
 }
 async function readTask() {
  const request = await ownerAccess();
  const result = await call(WORLD_PATHS.owner, request, request, "owner_control", request);
  const task = field(result, ["view", "game_task_control"]);
  if (!task || (taskId && task.task_id !== taskId) || task.agent_id !== agentId || task.owner_public_key !== ownerSigner.publicKeyHex) fail("task_unavailable");
  const fence = build("fence", task);
  validate("fence", null, fence);
  return { fence, owner: result.view.canonical_agent_owner, logicalTick: codec.build("field", JSON.stringify({ json: json(result), path: ["logical_tick"] })) };
 }
 async function freshFence({ allowUnbound = false } = {}) {
  const current = await readTask();
  if (boundFence && !same(boundFence, current.fence)) fail("task_fence_changed");
  if (!allowUnbound && (current.fence.task_status !== "active" || current.fence.executor_binding?.executor_public_key !== executorSigner.publicKeyHex)) fail("executor_not_bound");
  return current;
 }
 async function bindFence(fence) {
  validate("fence", null, fence); const current = await freshFence();
  if (!same(current.fence, fence)) fail("task_fence_changed"); boundFence = copy(fence); return copy(boundFence);
 }
 async function publicView() {
  const request = readRequest();
  const proof = await sign(WORLD_PATHS.view, request);
  return call(WORLD_PATHS.view, proof, proof, "view", request);
 }
 async function snapshot() {
  const { fence } = await freshFence();
  const request = { authority_mode: "native_executor_v1", schema_revision: 2, fence };
  const proof = await sign(WORLD_PATHS.snapshot, request);
  return call(WORLD_PATHS.snapshot, proof, proof, "task_snapshot", request);
 }
 async function record(operationId, kind, signedRequest, context = null, runBudget = null) {
  return recovery.remember({ operationId, kind, signedRequest: { rawJson: json(signedRequest) }, correlation: signedRequest.correlation ?? { operationId }, context, runBudget: runBudget ?? {} });
 }
 async function find(operationId, lane) {
  const stored = await recovery.load();
  return stored.operations.find((entry) => entry.original.operationId === `${operationId}:${lane}`)?.original;
 }
 async function lookup(original) {
  if (original.kind === "prepare") {
   const proof = decoded(original.signedRequest.rawJson);
   const payload = await call(WORLD_PATHS.prepare, proof, proof, "context_prepare", proof);
   return { state: "committed", receipt: { identity: payload.identity }, prepared: payload };
  }
  const submitted = decoded(original.signedRequest.rawJson);
  const payload = field(submitted, ["signed_payload"]);
  const expected = build("correlation", { world, payload_json: json(payload) });
  if (!same(expected, submitted.correlation)) fail("original_correlation_invalid");
  const request = { request: { contract_version: 1, key: expected.key }, original: payload };
  const response = await call(WORLD_PATHS.lookup, request, request, "lookup", expected);
  return { state: response.outcome.status, receipt: response };
 }
 async function submitOriginal(original) {
  await beforeEffect();
  const request = decoded(original.signedRequest.rawJson);
  let result;
  try { result = await call(WORLD_PATHS.submit, request, request, "intent", request.correlation, beforeEffect); }
  catch { fail("operation_unknown"); }
  // Submission admission is never substituted for an original lookup result.
  const checked = await lookup(original);
  await recovery.settle(original.operationId, { state: checked.state, receipt: { rawJson: json(checked.receipt) } });
  return checked.receipt;
 }
 async function submit(operationId, lane, payload, context = null, runBudget = null) {
  const prior = await find(operationId, lane);
  if (prior) { const result = await lookup(prior); await recovery.settle(prior.operationId, { state: result.state, receipt: { rawJson: json(result.receipt) } }); return result.receipt; }
  const request = { contract_version: 1, correlation: build("correlation", { world, payload_json: json(payload) }), deadline_unix_ms: null, signed_payload: payload };
  const original = await record(`${operationId}:${lane}`, lane, request, context, runBudget);
  return submitOriginal(original);
 }
 async function recover() { return recovery.reconcile(async (original) => { const result = await lookup(original); return { state: result.state, receipt: { rawJson: json(result.receipt) } }; }); }
 async function hasUnresolved() {
  if (await recovery.hasUnresolved()) return true;
  const { operations } = await recovery.load();
  const actions = new Set(operations.filter((entry) => entry.original.kind === "action").map((entry) => entry.original.operationId.replace(/:action$/, "")));
  if (operations.some((entry) => entry.original.kind === "outcome" && actions.has(entry.original.operationId.replace(/:outcome$/, "")))) return true;
  // A committed prefix owns a live Context until its exact action/outcome settles.
  return operations.some((entry) => entry.original.kind === "prefix" && entry.state === "committed" &&
   !operations.some((other) => ["action", "outcome"].includes(other.original.kind) && terminal(other.state) &&
    other.original.operationId.replace(/:(action|outcome)$/, "") === entry.original.operationId.replace(/:prefix$/, "")));
 }
 async function retireExpiredContexts() {
  // This is an explicitly authorized effect path, separate from read-only
  // reconciliation. Any original action/outcome retains its immutable lane.
  const loaded = await recovery.load();
  if (loaded.operations.some((entry) => !terminal(entry.state))) fail("recovery_required");
  let retired = 0;
  for (const entry of loaded.operations) {
   if (entry.original.kind !== "prefix" || entry.state !== "committed") continue;
   const operationId = entry.original.operationId.replace(/:prefix$/, "");
   const action = await find(operationId, "action"), priorOutcome = await find(operationId, "outcome");
   if (action || priorOutcome) continue;
   const verified = await lookup(entry.original);
   if (verified.state !== "committed") fail("recovery_required");
   const submitted = decoded(entry.original.signedRequest.rawJson);
   const identity = field(submitted, ["signed_payload", "payload", "request", "operation", "data", "context"]);
   const current = await freshFence();
   if (!same(current.fence, identity.fence)) fail("task_fence_changed");
   if (BigInt(current.logicalTick) <= BigInt(identity.valid_until_tick)) fail("context_not_expired");
   const view = await publicView();
   const request = build("retire_context", { context_json: json(identity), view_json: json(view) });
   // Re-read all terminal lanes after the public read before first creation.
   if (await find(operationId, "action") || await find(operationId, "outcome")) fail("original_result_requires_recovery");
   const result = await submit(operationId, "outcome", { kind: "scheduler", payload: await sign("scheduler", request) }, identity, entry.original.runBudget);
   if (result.outcome.status !== "committed") fail("operation_pending");
   retired += 1;
  }
  return { retired };
 }
 async function prepareContext(operationId, runBudget = null) {
  const prior = await find(operationId, "prepare");
  if (prior) return (await lookup(prior)).prepared;
  if (await hasUnresolved()) fail("recovery_required");
  const { fence } = await freshFence();
  const proof = await sign(WORLD_PATHS.prepare, { authority_mode: "native_executor_v1", schema_revision: 2, fence, client_operation_id: operationId, client_nonce: nonce() });
  const original = await record(`${operationId}:prepare`, "prepare", proof, null, runBudget);
  const result = await lookup(original);
  await recovery.settle(original.operationId, { state: result.state, receipt: result.receipt });
  return result.prepared;
 }
 async function captureContext(operationId, prepared, runBudget = null) {
  await freshFence();
  const prefix = build("prefix", prepared);
  const result = await submit(operationId, "prefix", { kind: "scheduler", payload: await sign("scheduler", prefix) }, prepared.identity, runBudget);
  if (result.outcome.status !== "committed") fail(terminal(result.outcome.status) ? "context_rejected" : "operation_pending");
  return readContext(operationId, prepared.identity);
 }
 async function contextProof(operationId, identity) { return sign(WORLD_PATHS.read, { schema_revision: 1, context: identity, prepare_operation_id: operationId }); }
 async function readContext(operationId, identity) {
  await freshFence(); const proof = await contextProof(operationId, identity);
  return call(WORLD_PATHS.read, proof, proof, "context_read", proof);
 }
 async function quote(operationId, captured, kind, selection) {
  await freshFence();
  const current = await readContext(operationId, captured.prepared.identity);
  const path = kind === "schedule" ? WORLD_PATHS.recipe : kind === "build" ? WORLD_PATHS.factory : fail("unsupported_tool");
  const request = { schema_revision: 1, context_proof: await contextProof(operationId, captured.prepared.identity), expected_base_binding: current.current_public_context.captured_base_binding,
   ...(kind === "schedule" ? { factory_id: selection.factory, recipe_id: selection.recipe, batches: selection.batches } : { factory_id: selection.factory, factory_kind: selection.kind, site_id: selection.site }) };
  const proof = await sign(path, request);
  const quoted = await call(path, proof, proof, kind === "schedule" ? "recipe_quote" : "factory_quote", proof);
  return { captured: build("replace", { json: json(current), path: ["current_public_context"], value_json: json(quoted.current_public_context) }), quoted };
 }
 async function action(operationId, captured, action, runBudget = null) {
  const existing = await find(operationId, "action"), outcome = await find(operationId, "outcome");
  if (existing || outcome) { const original = existing ?? outcome, result = await lookup(original); await recovery.settle(original.operationId, { state: result.state, receipt: { rawJson: json(result.receipt) } }); return result.receipt; }
  await freshFence();
  const request = build("action", { captured_json: json(captured), action });
  return submit(operationId, "action", { kind: "cognition", payload: await sign("cognition", request) }, captured.prepared.identity, runBudget);
 }
 async function outcome(operationId, captured, reason, runBudget = null) {
  if (await find(operationId, "action")) fail("original_action_requires_recovery");
  await freshFence();
  return submit(operationId, "outcome", { kind: "scheduler", payload: await sign("scheduler", build("outcome", { captured_json: json(captured), reason })) }, captured.prepared.identity, runBudget);
 }
 async function registerExecutor({ operationId, executorId, generation, nonce: delegationNonce, confirmedFence }) {
  if (!confirmedFence) fail("run_confirmation_required");
  if (![generation, delegationNonce].every((value) => Number.isSafeInteger(value) && value > 0)) fail("delegation_revision_required");
  await recover(); if (await hasUnresolved()) fail("recovery_required");
  const current = await freshFence({ allowUnbound: true }), f = current.fence;
  if (!same(f, confirmedFence)) fail("task_fence_changed");
  const ownerGeneration = Number(f.owner_generation);
  if (!Number.isSafeInteger(ownerGeneration) || ownerGeneration <= 0) fail("delegation_revision_unsupported");
  const prior = await find(operationId, "delegation");
  let delegationPayload, effectiveGeneration = generation;
  if (prior) {
   const original = decoded(prior.signedRequest.rawJson);
   delegationPayload = field(original, ["signed_payload"]);
   const proof = delegationPayload.payload, request = proof?.request;
   if (delegationPayload.kind !== "delegation" || proof?.subject_public_key !== ownerSigner.publicKeyHex ||
    request?.world_id !== world.world_id || request.branch_id !== f.branch_id || request.agent_id !== agentId ||
    request.owner_binding !== f.owner_binding || request.agent_identity_generation !== ownerGeneration ||
    request.delegate_public_key !== executorSigner.publicKeyHex || request.revoked !== false ||
    ![request.generation, request.nonce].every((value) => Number.isSafeInteger(value) && value > 0)) fail("delegation_original_mismatch");
   // The committed delegation's exact revision owns this two-stage operation.
   // A refreshed caller draft must not silently bind an unregistered revision.
   effectiveGeneration = request.generation;
  } else {
   const request = { world_id: world.world_id, branch_id: f.branch_id, agent_id: agentId, owner_binding: f.owner_binding, agent_identity_generation: ownerGeneration, generation, delegate_public_key: executorSigner.publicKeyHex, revoked: false, nonce: delegationNonce };
   delegationPayload = { kind: "delegation", payload: await sign("delegation", request, ownerSigner) };
  }
  const delegated = await submit(operationId, "delegation", delegationPayload);
  if (delegated.outcome.status !== "committed") fail("delegation_not_committed");
  const next = await readTask();
  if (!same(next.fence, f)) fail("task_fence_changed");
  const binding = { api_rev: 1, client_operation_id: `${operationId}:bind`, fence: next.fence, operation: { operation: "bind", executor_id: executorId, executor_public_key: executorSigner.publicKeyHex, delegation_generation: String(effectiveGeneration) } };
  const bound = await submit(operationId, "executor_bind", { kind: "game_executor_control", payload: await sign("game_executor_control/v1", binding, ownerSigner) });
  if (bound.outcome.status !== "committed") fail("executor_not_bound");
  const confirmed = await freshFence(); await bindFence(confirmed.fence); return confirmed;
 }
 function modelContext(captured) {
  return json({ goal: field(captured, ["prepared", "snapshot", "active_goal", "owner_proof", "request", "content"]), context_identity: field(captured, ["prepared", "identity"]), actor_observation: field(captured, ["current_public_context", "actor_observation"]), native_authority: field(captured, ["current_public_context", "native_authority"]) });
 }
 return { describe, readTask, freshFence, bindFence, publicView, snapshot, prepareContext, captureContext, readContext, quote, action, outcome, lookup, recover, retireExpiredContexts, registerExecutor, hasUnresolved, modelContext, world: copy(world), agentId, executorPublicKey: executorSigner.publicKeyHex };
}
