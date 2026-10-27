import { BrowserParticipantError } from "./browser_participant_entry.js";
import { describe, expect, it, vi } from "vitest";
import { createBrowserGameClient, WORLD_PATHS } from "./browser_game_client.js";
import { createBrowserGameRecovery } from "./browser_game_recovery.js";
const key = "a".repeat(64), executor = "b".repeat(64);
const fence = { world_id: "world", genesis_digest: "genesis", branch_id: "main", agent_id: "agent", task_id: "task", task_status: "active", owner_public_key: key, owner_generation: "1", owner_binding: "player", task_revision: "1", executor_binding: { executor_public_key: executor, delegation_generation: "1" } };
const signer = (publicKeyHex) => ({ publicKeyHex, sign: vi.fn(async () => new Uint8Array(64)) });
function fixture(recoveryOverride) {
 const entries = [], order = [], signed = [], calls = []; let stateFence = structuredClone(fence), lookupStatus = "committed", rejectWrite = false, logicalTick = 1, outcomeLookupStatus, effectGate = async () => order.push("effect_gate");
 const recovery = {
  load: async () => ({ operations: entries }),
  remember: async (original) => { order.push(`persist:${original.kind}`); if (rejectWrite) throw new Error("disk unavailable"); entries.push({ original, state: "unknown" }); return original; },
  settle: async (id, value) => { Object.assign(entries.find((entry) => entry.original.operationId === id), value); },
  hasUnresolved: async () => entries.some((entry) => !["committed", "rejected", "expired"].includes(entry.state)),
  reconcile: async (lookup) => { for (const entry of entries) if (entry.state === "unknown") Object.assign(entry, await lookup(entry.original)); return { operations: entries }; },
 };
 const prepared = { identity: { fence, decision_request_id: "decision", valid_until_tick: "1" }, snapshot: { active_goal: { owner_proof: { request: { content: "goal" } } } } };
 const codec = {
  signingBytes: (domain, request) => { signed.push({ domain, request: JSON.parse(request) }); return new Uint8Array([1]); },
  verifyGatewayResponse: vi.fn((domain, request, response, serviceKey) => { expect(serviceKey).toBe(key); const parsed = JSON.parse(response); if (parsed.tampered) throw new Error("bad signature"); return JSON.stringify(parsed.world_response.payload); }),
  validate: vi.fn((kind, request, payload) => payload),
  build: (kind, value) => { const v = JSON.parse(value); if (kind === "field") return JSON.stringify(v.path.reduce((current, name) => current[name], JSON.parse(v.json))); if (kind === "fence") return JSON.stringify(v); if (kind === "correlation") return JSON.stringify({ key: { world: v.world, request_id_or_nonce: JSON.parse(v.payload_json).kind }, payload_digest: JSON.parse(v.payload_json).kind }); if (kind === "prefix") return JSON.stringify({ operation: { operation: "task_provider_prefix", data: { context: v.identity } } }); if (kind === "action") return JSON.stringify({ request: { agent_id: "agent" }, action: v.action }); if (kind === "retire_context") return JSON.stringify({ operation: { operation: "task_provider_failure", data: { context: JSON.parse(v.context_json), reason: "context_expired" } } }); if (kind === "outcome") return JSON.stringify({ operation: { operation: "task_provider_failure", data: { context: JSON.parse(v.captured_json).prepared.identity, reason: v.reason } } }); throw new Error("unsupported"); },
 };
 const gatewayCall = vi.fn(async (path, body, options) => {
  await options?.beforeSend?.();
  calls.push({ path, body }); order.push(`network:${body.operation?.kind ?? "owner_view"}`);
  if (path === "/participant/owner-view") return { world_response: { payload: { view: { game_task_control: stateFence, canonical_agent_owner: { agent_id: "agent" } }, logical_tick: logicalTick } } };
  const op = body.operation;
  let payload;
  if (op.kind === "prepare_context") payload = prepared;
  else if (op.kind === "read_view") payload = { logical_tick: logicalTick, view: { game_task_control: stateFence, runtime_binding: { base_tick: logicalTick } } };
  else if (op.kind === "read_context") payload = { prepared, current_public_context: { captured_base_binding: {}, actor_observation: {} } };
  else if (op.kind === "submit" || op.kind === "lookup" || op.kind === "lookup_executor") payload = { correlation: op.request.correlation ?? { key: op.request.request.key }, outcome: { status: op.kind === "submit" ? "pending" : (op.request.original?.payload?.request?.operation?.operation === "task_provider_failure" ? outcomeLookupStatus ?? lookupStatus : lookupStatus) } };
  else payload = {};
  return { world_response: { payload } };
 });
 const client = createBrowserGameClient({ config: { endpoint: "/api/public/hosted-account/v2", world: { world_id: "world", genesis_digest: "genesis" }, servicePublicKey: key }, codec, ownerSigner: signer(key), executorSigner: signer(executor), participantProofs: async () => ({ owner: { request: "owner" }, admission: { request: "admission" } }), agentId: "agent", taskId: "task", recovery: recoveryOverride ?? recovery, gatewayCall, beforeEffect: () => effectGate() });
 return { client, codec, gatewayCall, recovery, entries, calls, order, signed, prepared, setGate: (gate) => { effectGate = gate; }, setFence: (value) => { stateFence = value; }, setOutcomeLookup: (value) => { outcomeLookupStatus = value; }, setTick: (value) => { logicalTick = value; }, setLookup: (value) => { lookupStatus = value; }, rejectWrite: () => { rejectWrite = true; } };
}
describe("browser WorldService client", () => {
 it("discovers an owner-signed canonical task and binds its complete fence", async () => {
  const f = fixture(); const task = await f.client.readTask(); expect(task.fence).toEqual(fence);
  await f.client.bindFence(task.fence);
  expect(f.calls[0]).toMatchObject({ path: "/participant/owner-view", body: { response_profile: "owner_control_v1", read: { subject_public_key: key } } });
  expect(f.codec.verifyGatewayResponse).toHaveBeenCalledWith(WORLD_PATHS.owner, expect.any(String), expect.any(String), key);
  f.setFence({ ...fence, task_revision: "2" }); await expect(f.client.freshFence()).rejects.toMatchObject({ code: "task_fence_changed" });
 });
 it("persists prepare and prefix originals before submit, then looks up before reading Context", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("op"); await f.client.captureContext("op", prepared);
  expect(f.order.indexOf("persist:prefix")).toBeLessThan(f.order.indexOf("network:submit"));
  expect(f.order.indexOf("network:lookup_executor")).toBeLessThan(f.order.indexOf("network:read_context"));
  const submit = f.calls.find((call) => call.body.operation?.kind === "submit");
  expect(submit.path).toBe("/entry-executor/control"); expect(submit.body.access.fence).toEqual(fence);
  expect(submit.body.operation.request.signed_payload.kind).toBe("scheduler");
  const preparedRecord = f.entries.find((entry) => entry.original.kind === "prepare"); expect(preparedRecord.receipt).toEqual({ identity: prepared.identity });
  expect(await f.client.hasUnresolved()).toBe(true); // Captured Context has no terminal original yet.
 });
 it("does not submit if persistence fails", async () => {
  const f = fixture(); await f.client.bindFence(fence); f.rejectWrite();
  await expect(f.client.action("op", { prepared: f.prepared }, { type: "MoveAgent" })).rejects.toThrow();
  expect(f.calls.some((call) => call.body.operation?.kind === "submit")).toBe(false);
 });
 it("rechecks mutation permission at transport send after asynchronous gateway preparation", async () => {
  const f = fixture(); await f.client.bindFence(fence); let gates = 0;
  f.setGate(async () => { if (++gates === 2) throw new Error("stopped after signing"); });
  await expect(f.client.action("stop-race", { prepared: f.prepared }, { type: "MoveAgent" })).rejects.toMatchObject({ code: "operation_unknown" });
  expect(gates).toBe(2); expect(f.calls.some((call) => call.body.operation?.kind === "submit")).toBe(false);
  expect(f.entries.find((entry) => entry.original.kind === "action").state).toBe("unknown");
 });
 it("retains unknown originals and reconciles them with owner lookup without new effects", async () => {
  const f = fixture(); await f.client.bindFence(fence); f.setLookup("unknown");
  await f.client.action("op", { prepared: f.prepared }, { type: "MoveAgent" });
  expect(await f.client.hasUnresolved()).toBe(true);
  const submits = f.calls.filter((call) => call.body.operation?.kind === "submit").length;
  await f.client.recover(); await f.client.action("op", { prepared: f.prepared }, { type: "MoveAgent", data: { changed: true } });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(submits);
  expect(f.calls.at(-1)).toMatchObject({ path: "/entry-owner/control", body: { operation: { kind: "lookup_executor" } } });
 });
 it("rejects response proof failure before exposing a task", async () => {
  const f = fixture(); f.codec.verifyGatewayResponse.mockImplementation(() => { throw new Error("private upstream data"); });
  await expect(f.client.readTask()).rejects.toMatchObject({ code: "game_response_invalid", message: "game_response_invalid" });
 });
 it("rejects arbitrary endpoint configuration", () => {
  expect(() => createBrowserGameClient({ config: { endpoint: "https://model.invalid" } })).toThrow("game_configuration_invalid");
 });
 it("resumes a committed delegation's exact generation before binding after reload", async () => {
  const f = fixture();
  const original = { operationId: "registration:delegation", kind: "delegation", signedRequest: { rawJson: JSON.stringify({ contract_version: 1, correlation: { key: { world: { world_id: "world", genesis_digest: "genesis" }, request_id_or_nonce: "delegation" }, payload_digest: "delegation" }, deadline_unix_ms: null, signed_payload: { kind: "delegation", payload: { subject_public_key: key, signature_hex: "00".repeat(64), request: { world_id: "world", branch_id: "main", agent_id: "agent", owner_binding: "player", agent_identity_generation: 1, generation: 1, delegate_public_key: executor, revoked: false, nonce: 11 } } } }) }, correlation: {}, context: null, runBudget: {} };
  f.entries.push({ original, state: "committed" });
  await f.client.registerExecutor({ operationId: "registration", executorId: "browser-executor", generation: 2, nonce: 22, confirmedFence: fence });
  const submissions = f.calls.filter((call) => call.body.operation?.kind === "submit");
  expect(submissions).toHaveLength(1);
  expect(submissions[0].body.operation.request.signed_payload).toMatchObject({ kind: "game_executor_control", payload: { request: { operation: { delegation_generation: "1" } } } });
  expect(f.signed.some((proof) => proof.domain === "delegation")).toBe(false);
  expect(JSON.parse(original.signedRequest.rawJson).signed_payload.payload.request.nonce).toBe(11);
 });
 it("requires the same confirmed Task fence before delegation and again before binding", async () => {
  const f = fixture(); let gates = 0;
  f.setGate(async () => { if (++gates === 2) f.setFence({ ...fence, task_revision: "2" }); });
  await expect(f.client.registerExecutor({ operationId: "registration", executorId: "browser-executor", generation: 1, nonce: 1, confirmedFence: fence })).rejects.toMatchObject({ code: "task_fence_changed" });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(1);
  expect(f.entries.some((entry) => entry.original.kind === "executor_bind")).toBe(false);
  const mismatch = fixture(); mismatch.setFence({ ...fence, task_revision: "2" });
  await expect(mismatch.client.registerExecutor({ operationId: "registration", executorId: "browser-executor", generation: 1, nonce: 1, confirmedFence: fence })).rejects.toMatchObject({ code: "task_fence_changed" });
  expect(mismatch.calls.some((call) => call.body.operation?.kind === "submit")).toBe(false);
 });
 it("retires an exact expired committed prefix using public evidence and a durable terminal original", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("expired"); await f.client.captureContext("expired", prepared);
  const privateReads = f.calls.filter((call) => call.body.operation?.kind === "read_context").length;
  f.setTick(2);
  expect(await f.client.retireExpiredContexts()).toEqual({ retired: 1 });
  expect(await f.client.hasUnresolved()).toBe(false);
  expect(f.calls.filter((call) => call.body.operation?.kind === "read_context")).toHaveLength(privateReads);
  const original = f.entries.find((entry) => entry.original.kind === "outcome").original;
  expect(JSON.parse(original.signedRequest.rawJson).signed_payload.payload.request.operation.data).toMatchObject({ context: prepared.identity, reason: "context_expired" });
  expect(f.order.indexOf("persist:outcome")).toBeLessThan(f.order.lastIndexOf("network:submit"));
  expect(f.calls.at(-1).body.operation.kind).toBe("lookup_executor");
 });
 it("keeps an unexpired prefix and never retires over an unknown original action", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("active"); const captured = await f.client.captureContext("active", prepared);
  const before = f.calls.filter((call) => call.body.operation?.kind === "submit").length;
  await expect(f.client.retireExpiredContexts()).rejects.toMatchObject({ code: "context_not_expired" });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(before);
  f.setLookup("unknown"); await f.client.action("active", captured, { type: "MoveAgent" }); f.setTick(2);
  await expect(f.client.retireExpiredContexts()).rejects.toMatchObject({ code: "recovery_required" });
  expect(f.entries.some((entry) => entry.original.kind === "outcome")).toBe(false);
 });
 it("keeps a pending retirement original and never creates a competing outcome", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("pending-retirement"); await f.client.captureContext("pending-retirement", prepared);
  f.setTick(2);
  // Prefix lookup must remain committed; only the newly written outcome is pending.
  f.setOutcomeLookup("pending");
  await expect(f.client.retireExpiredContexts()).rejects.toMatchObject({ code: "operation_pending" });
  const before = f.calls.filter((call) => call.body.operation?.kind === "submit").length;
  await expect(f.client.retireExpiredContexts()).rejects.toMatchObject({ code: "recovery_required" });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(before);
  expect(f.entries.filter((entry) => entry.original.kind === "outcome")).toHaveLength(1);
 });
 it("uses the durable recovery schema for the full prefix and action flow", async () => {
  const records = new Map(), storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
  const recovery = createBrowserGameRecovery({ scope: { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent", taskId: "task", fence }, storage, locks: { request: async (_name, _options, action) => action() } });
  const f = fixture(recovery); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("durable-op", { runId: "run" });
  const captured = await f.client.captureContext("durable-op", prepared, { runId: "run" });
  await f.client.action("durable-op", captured, { type: "MoveAgent", data: { agent_id: "agent", to: { x_cm: 1, y_cm: 2, z_cm: 3 } } }, { runId: "run" });
  expect(await f.client.hasUnresolved()).toBe(false);
  const loaded = await recovery.load(); expect(loaded.operations).toHaveLength(3);
  expect(loaded.operations.every((entry) => typeof entry.original.signedRequest.rawJson === "string")).toBe(true);
 });
});

it("preserves safe gateway diagnostics through task reads without raw messages", async () => {
 const f = fixture();
 f.gatewayCall.mockRejectedValueOnce(new BrowserParticipantError("participant_request_failed", { stage: "participant_owner_view", httpStatus: 403, serverCode: "csrf_failed" }));
 await expect(f.client.readTask()).rejects.toMatchObject({ code: "participant_request_failed", diagnostic: { stage: "participant_owner_view", httpStatus: 403, serverCode: "csrf_failed" } });
});
it("retains unknown action identity while preserving its safe gateway failure classification", async () => {
 const f = fixture(); await f.client.bindFence(fence);
 const transport = f.gatewayCall.getMockImplementation();
 f.gatewayCall.mockImplementation((path, body, options) => {
  if (body.operation?.kind === "submit") throw new BrowserParticipantError("participant_request_failed", { stage: "executor_control", httpStatus: 503, serverCode: "executor_control_unavailable" });
  return transport(path, body, options);
 });
 await expect(f.client.action("same-action", { prepared: f.prepared }, { type: "MoveAgent" })).rejects.toMatchObject({ code: "operation_unknown", diagnostic: { httpStatus: 503, serverCode: "executor_control_unavailable", causeCode: "participant_request_failed" } });
 expect(f.entries.find((entry) => entry.original.kind === "action").state).toBe("unknown");
});

it("preserves the existing Context rejection code after a verified prefix terminal result", async () => {
 const f = fixture(); await f.client.bindFence(fence);
 const prepared = await f.client.prepareContext("prefix-terminal"); f.setLookup("rejected");
 await expect(f.client.captureContext("prefix-terminal", prepared)).rejects.toMatchObject({ code: "context_rejected" });
});
