import { BrowserParticipantError } from "./browser_participant_entry.js";
import { describe, expect, it, vi } from "vitest";
import { createBrowserGameClient, WORLD_PATHS } from "./browser_game_client.js";
import { createBrowserGameRecovery } from "./browser_game_recovery.js";
import { readBrowserActionPreview, endBrowserActionPreview, openBrowserPreviews } from "./browser_action_preview.js";
import { executeBrowserDirectAction } from "./browser_direct_executor.js";
import { directActionSelection } from "./browser_game_permit.js";
import { expiredBrowserPreviewContexts, openBrowserContexts } from "./browser_context_recovery.js";
const key = "a".repeat(64), executor = "b".repeat(64);
const fence = { world_id: "world", genesis_digest: "genesis", branch_id: "main", agent_id: "agent", task_id: "task", task_status: "active", owner_public_key: key, owner_generation: "1", owner_binding: "player", task_revision: "1", executor_binding: { executor_public_key: executor, delegation_generation: "1" } };
const signer = (publicKeyHex) => ({ publicKeyHex, sign: vi.fn(async () => new Uint8Array(64)) });
function fixture(recoveryOverride) {
 const entries = [], order = [], signed = [], calls = []; let stateFence = structuredClone(fence), lookupStatus = "committed", rejectWrite = false, logicalTick = 1, outcomeLookupStatus, ownerExtras = {}, effectGate = async () => order.push("effect_gate");
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
  build: (kind, value) => { const v = JSON.parse(value); if (kind === "replace") { const root = JSON.parse(v.json), parent = v.path.slice(0, -1).reduce((item, key) => item[key], root); parent[v.path.at(-1)] = JSON.parse(v.value_json); return JSON.stringify(root); } if (kind === "native_delegation") return JSON.stringify({ api_rev: 1, client_operation_id: v.client_operation_id, fence: JSON.parse(v.fence_json), operation: v.operation }); if (kind === "field") return JSON.stringify(v.path.reduce((current, name) => current[name], JSON.parse(v.json))); if (kind === "fence") return JSON.stringify(v); if (kind === "correlation") return JSON.stringify({ key: { world: v.world, request_id_or_nonce: JSON.parse(v.payload_json).kind }, payload_digest: JSON.parse(v.payload_json).kind }); if (kind === "prefix") return JSON.stringify({ operation: { operation: "task_provider_prefix", data: { context: v.identity } } }); if (kind === "action") return JSON.stringify({ request: { agent_id: "agent" }, action: v.action }); if (kind === "retire_context") return JSON.stringify({ operation: { operation: "task_provider_failure", data: { context: JSON.parse(v.context_json), reason: "context_expired" } } }); if (kind === "outcome") return JSON.stringify({ captured_base_binding: JSON.parse(v.captured_json).current_public_context.captured_base_binding, operation: { operation: "task_provider_failure", data: { context: JSON.parse(v.captured_json).prepared.identity, reason: v.reason } } }); throw new Error("unsupported"); },
 };
 const gatewayCall = vi.fn(async (path, body, options) => {
  await options?.beforeSend?.();
  calls.push({ path, body }); order.push(`network:${body.operation?.kind ?? "owner_view"}`);
  if (path === "/participant/owner-view") return { world_response: { payload: { view: { game_task_control: stateFence, canonical_agent_owner: { agent_id: "agent" }, ...ownerExtras }, logical_tick: logicalTick } } };
  const op = body.operation;
  let payload;
  if (op.kind === "prepare_context") payload = prepared;
  else if (["recipe_quote", "factory_quote"].includes(op.kind)) payload = { current_public_context: { captured_base_binding: { base_tick: logicalTick } }, quote: { rejection: { type: "InsufficientMaterial" }, preview: null } };
  else if (op.kind === "read_view") payload = { logical_tick: logicalTick, view: { game_task_control: stateFence, runtime_binding: { base_tick: logicalTick } } };
  else if (op.kind === "read_context") payload = { prepared, current_public_context: { captured_base_binding: { base_tick: logicalTick }, actor_observation: {} } };
  else if (op.kind === "submit" || op.kind === "lookup" || op.kind === "lookup_executor") payload = { correlation: op.request.correlation ?? { key: op.request.request.key }, outcome: { status: op.kind === "submit" ? "pending" : (op.request.original?.payload?.request?.operation?.operation === "task_provider_failure" ? outcomeLookupStatus ?? lookupStatus : lookupStatus) } };
  else payload = {};
  return { world_response: { payload } };
 });
 const client = createBrowserGameClient({ config: { endpoint: "/api/public/hosted-account/v2", world: { world_id: "world", genesis_digest: "genesis" }, servicePublicKey: key }, codec, ownerSigner: signer(key), executorSigner: signer(executor), participantProofs: async () => ({ owner: { request: "owner" }, admission: { request: "admission" } }), agentId: "agent", taskId: "task", recovery: recoveryOverride ?? recovery, gatewayCall, beforeEffect: () => effectGate() });
 return { client, codec, gatewayCall, recovery: recoveryOverride ?? recovery, entries, calls, order, signed, prepared, setOwner: (value) => { ownerExtras = value; }, setGate: (gate) => { effectGate = gate; }, setFence: (value) => { stateFence = value; }, setOutcomeLookup: (value) => { outcomeLookupStatus = value; }, setTick: (value) => { logicalTick = value; }, setLookup: (value) => { lookupStatus = value; }, rejectWrite: () => { rejectWrite = true; } };
}
describe("browser WorldService client", () => {
 it.each(["rejected", "unknown"])("rechecks a confirmed replacement fence before retiring old task Contexts with a %s cleanup original", async (cleanupState) => {
  const records = new Map(), storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
  const createRecovery = (currentFence) => createBrowserGameRecovery({ scope: { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent", taskId: "task", fence: currentFence }, storage, locks: { request: async (_name, _options, action) => action() } });
  const oldRecovery = createRecovery(fence), old = fixture(oldRecovery); await old.client.bindFence(fence);
  await oldRecovery.initializeRun("original-run", { fence }); await oldRecovery.reserveModelCall("original-run", 100, 512);
  old.prepared.identity.valid_until_tick = "10";
  await old.client.context("preview:old-fence", { previewPermitId: "old-fence" }); old.setOutcomeLookup(cleanupState);
  await expect(endBrowserActionPreview({ gameClient: old.client, recovery: oldRecovery, confirmedFence: fence, permitId: "old-fence", contextOperationId: "preview:old-fence" }))
   .rejects.toMatchObject({ code: "original_result_requires_recovery" });
  const before = await oldRecovery.load();
  const nextFence = { ...fence, task_revision: "2", executor_binding: { ...fence.executor_binding, delegation_generation: "2" } };
  const recovery = createRecovery(nextFence), next = fixture(recovery); next.setFence(nextFence); next.prepared.identity.fence = nextFence;
  next.prepared.identity.valid_until_tick = "10"; next.setOutcomeLookup(cleanupState);
  const input = { gameClient: next.client, recovery, confirmedFence: nextFence, permitId: "explicit-new-fence", kind: "schedule", selection: { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 } };
  await expect(readBrowserActionPreview(input)).rejects.toMatchObject({ code: cleanupState === "unknown" ? "recovery_required" : "quote_rejected" });
  const after = await recovery.load();
  expect(after.runBudgets).toEqual(before.runBudgets);
  for (const original of before.operations) expect(after.operations.find((entry) => entry.original.operationId === original.original.operationId)).toEqual(original);
  if (cleanupState === "unknown") {
   expect(next.calls.some((call) => ["prepare_context", "submit"].includes(call.body.operation?.kind))).toBe(false);
   expect(openBrowserContexts(after.operations)).toHaveLength(1);
  } else {
   const prefix = after.operations.find((entry) => entry.original.operationId === "preview:explicit-new-fence:prefix");
   expect(prefix).toMatchObject({ state: "committed", original: { context: { fence: nextFence }, runBudget: { expiredContextRetirements: ["preview:old-fence"] } } });
   expect(openBrowserContexts(after.operations).map((entry) => entry.original.operationId)).toEqual(["preview:explicit-new-fence:prefix"]);
  }
 });
 it.each(["world_id", "genesis_digest", "agent_id", "task_id"])("never retires a Context through another %s scope", (field) => {
  const original = { operationId: "preview:old:prefix", kind: "prefix", runBudget: { previewPermitId: "old" }, context: { fence, valid_until_tick: "10" } };
  const old = { original, state: "committed" }, changed = { ...fence, [field]: "another-scope" };
  expect(expiredBrowserPreviewContexts([old], { fence: changed, logicalTick: "20" })).toBeNull();
  const successor = { state: "committed", original: { ...original, operationId: "preview:other:prefix", context: { fence: changed, valid_until_tick: "30" }, runBudget: { previewPermitId: "other", expiredContextRetirements: ["preview:old"] } } };
  expect(openBrowserContexts([old, successor])).toHaveLength(2);
 });
 it("uses a new Context attempt after a canonical prefix rejection without consuming another direct permission", async () => {
  const records = new Map(), storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
  const recovery = createBrowserGameRecovery({ scope: { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent", taskId: "task", fence }, storage, locks: { request: async (_name, _options, action) => action() } });
  const f = fixture(recovery); await f.client.bindFence(fence); f.setLookup("rejected");
  const input = { gameClient: f.client, recovery, confirmedFence: fence, permitId: "prefix-rejected",
   selection: directActionSelection({ type: "WorldRule", data: { actor_id: "agent", request_id: "claim", command: { kind: "fixture_claim" } } }, "agent") };
  await expect(executeBrowserDirectAction(input)).rejects.toMatchObject({ code: "context_rejected" });
  expect((await recovery.getActionPermit(input.permitId)).originalOperationId).toBeNull();
  const first = (await recovery.load()).operations.find((entry) => entry.original.kind === "prefix");
  f.setLookup("committed");
  expect(await executeBrowserDirectAction(input)).toMatchObject({ originalOperationId: "direct:prefix-rejected:retry:1:action", submissionState: "committed" });
  const journal = await recovery.load(); expect(journal.actionPermits).toHaveLength(1);
  expect(journal.operations.find((entry) => entry.original.operationId === first.original.operationId)).toEqual(first);
  expect(journal.operations.filter((entry) => entry.original.kind === "action")).toHaveLength(1);
 });
 it("continues an unconsumed direct permit from its latest open Context across repeated verified expiries", async () => {
  const records = new Map(), storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
  const recovery = createBrowserGameRecovery({ scope: { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent", taskId: "task", fence }, storage, locks: { request: async (_name, _options, action) => action() } });
  const f = fixture(recovery); await f.client.bindFence(fence);
  const input = { gameClient: f.client, recovery, confirmedFence: fence, permitId: "same-player-permission",
   selection: directActionSelection({ type: "WorldRule", data: { actor_id: "agent", request_id: "same-claim", command: { kind: "fixture_claim" } } }, "agent") };
  const stoppedBeforeOriginal = vi.spyOn(f.client, "action").mockRejectedValue(Object.assign(new Error("stopped"), { code: "stopped" }));
  await expect(executeBrowserDirectAction(input)).rejects.toMatchObject({ code: "stopped" });
  f.setTick(2); f.prepared.identity.valid_until_tick = "3";
  await expect(executeBrowserDirectAction(input)).rejects.toMatchObject({ code: "stopped" });
  expect((await recovery.getActionPermit(input.permitId)).originalOperationId).toBeNull();
  stoppedBeforeOriginal.mockRestore(); f.setTick(4); f.prepared.identity.valid_until_tick = "5";
  expect(await executeBrowserDirectAction(input)).toMatchObject({ submissionState: "committed", originalOperationId: "direct:same-player-permission:retry:2:action" });
  const journal = await recovery.load();
  expect(journal.operations.filter((entry) => entry.original.kind === "prefix")).toHaveLength(3);
  expect(journal.operations.filter((entry) => entry.original.kind === "action")).toHaveLength(1);
  expect(await f.client.hasUnresolved()).toBe(false);
  expect(journal.runBudgets).toEqual([]);
 });
 it.each(["rejected", "unknown"])("keeps a %s direct original immutable and permits a new player Context only after known rejection and expiry", async (firstState) => {
  const records = new Map(), storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
  const recovery = createBrowserGameRecovery({ scope: { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent", taskId: "task", fence }, storage, locks: { request: async (_name, _options, action) => action() } });
  const f = fixture(recovery); await f.client.bindFence(fence);
  let actionState = firstState;
  const gateway = f.gatewayCall.getMockImplementation();
  f.gatewayCall.mockImplementation(async (...args) => {
   const response = await gateway(...args), op = args[1].operation;
   if (["lookup", "lookup_executor"].includes(op?.kind) && op.request.original?.kind === "cognition") response.world_response.payload.outcome.status = actionState;
   return response;
  });
  const selection = directActionSelection({ type: "WorldRule", data: { actor_id: "agent", request_id: "first-claim", command: { kind: "fixture_claim" } } }, "agent");
  const input = { gameClient: f.client, recovery, confirmedFence: fence, permitId: "old-player-permission", selection };
  expect(await executeBrowserDirectAction(input)).toMatchObject({ submissionState: firstState, agentDelegated: false });
  const old = (await recovery.load()).operations.find((entry) => entry.original.kind === "action");
  const originalBytes = old.original.signedRequest.rawJson;
  expect(await f.client.hasUnresolved()).toBe(true);
  expect(openBrowserPreviews(await recovery.load())).toMatchObject([{ mode: "direct", originalStates: [firstState] }]);
  const nextSelection = directActionSelection({ type: "WorldRule", data: { actor_id: "agent", request_id: "explicit-new-claim", command: { kind: "fixture_claim" } } }, "agent");
  const next = { ...input, permitId: "explicit-new-player-permission", selection: nextSelection };
  await expect(executeBrowserDirectAction(next)).rejects.toMatchObject({ code: "recovery_required" });
  f.setTick(2); f.prepared.identity.valid_until_tick = "10";
  if (firstState === "unknown") {
   const calls = f.calls.filter((call) => call.body.operation?.kind === "submit").length;
   await expect(executeBrowserDirectAction(next)).rejects.toMatchObject({ code: "recovery_required" });
   expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(calls);
  } else {
   actionState = "committed";
   expect(await executeBrowserDirectAction(next)).toMatchObject({ submissionState: "committed", agentDelegated: false });
   expect(await f.client.hasUnresolved()).toBe(false);
   const prefix = (await recovery.load()).operations.find((entry) => entry.original.operationId === "direct:explicit-new-player-permission:prefix");
   expect(prefix.original.runBudget.expiredContextRetirements).toEqual(["direct:old-player-permission"]);
  }
  const loaded = await recovery.load();
  expect(loaded.operations.find((entry) => entry.original.operationId === old.original.operationId)).toMatchObject({ state: firstState, original: { signedRequest: { rawJson: originalBytes } } });
  expect((await recovery.getActionPermit("old-player-permission")).originalOperationId).toBe(old.original.operationId);
  expect(loaded.runBudgets).toEqual([]);
 });
 it("ends a quote-rejected preview through the real client and releases the exact original Context for a new explicit decision", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const args = { gameClient: f.client, recovery: f.recovery, confirmedFence: fence, permitId: "no-key", kind: "schedule",
   selection: { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 } };
  await expect(readBrowserActionPreview(args)).rejects.toMatchObject({ code: "quote_rejected" });
  expect(await f.client.hasUnresolved()).toBe(true);
  await endBrowserActionPreview({ ...args, contextOperationId: "preview:no-key" });
  expect(await f.client.hasUnresolved()).toBe(false);
  const ended = f.entries.find((entry) => entry.original.operationId === "preview:no-key:outcome");
  expect(ended.state).toBe("committed");
  expect(JSON.parse(ended.original.signedRequest.rawJson).signed_payload.payload.request.operation.data.reason).toBe("executor_aborted");
  await f.client.context("preview:explicit-next", { previewPermitId: "explicit-next", previewRequest: args.selection });
  expect(f.entries.some((entry) => entry.original.operationId === "preview:explicit-next:prefix")).toBe(true);
  expect(f.entries.filter((entry) => entry.original.operationId === "preview:no-key:prefix")).toHaveLength(1);
 });
 it("retires an expired preview through the real client's precise outcome lane without any model dependency", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  await f.client.context("preview:no-key", { previewPermitId: "no-key" }); f.setTick(2);
  await endBrowserActionPreview({ gameClient: f.client, recovery: f.recovery, confirmedFence: fence,
   permitId: "no-key", contextOperationId: "preview:no-key" });
  const ended = f.entries.find((entry) => entry.original.operationId === "preview:no-key:outcome");
  expect(ended.state).toBe("committed");
  expect(JSON.parse(ended.original.signedRequest.rawJson).signed_payload.payload.request.operation.data.reason).toBe("context_expired");
  expect(await f.client.hasUnresolved()).toBe(false);
 });
 it("keeps unknown preview outcomes lookup-only and never creates another cleanup or Context", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  await f.client.context("preview:no-key", { previewPermitId: "no-key" }); f.setTick(2); f.setLookup("unknown");
  const writes = f.calls.filter((call) => call.body.operation?.kind === "submit").length;
  await expect(endBrowserActionPreview({ gameClient: f.client, recovery: f.recovery, confirmedFence: fence,
   permitId: "no-key", contextOperationId: "preview:no-key" })).rejects.toMatchObject({ code: "recovery_required" });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(writes);
  expect(f.entries.some((entry) => entry.original.kind === "outcome")).toBe(false);
 });
 it.each(["rejected", "expired"])("does not confuse a %s cleanup request with a closed Context", async (status) => {
  const f = fixture(); await f.client.bindFence(fence);
  await f.client.context("preview:no-key", { previewPermitId: "no-key" }); f.setOutcomeLookup(status);
  const input = { gameClient: f.client, recovery: f.recovery, confirmedFence: fence, permitId: "no-key", contextOperationId: "preview:no-key" };
  await expect(endBrowserActionPreview(input)).rejects.toMatchObject({ code: "original_result_requires_recovery" });
  expect(await f.client.hasUnresolved()).toBe(true);
  expect(openBrowserPreviews(await f.recovery.load())).toMatchObject([{ contextOperationId: "preview:no-key", hasOriginal: true }]);
  const writes = f.calls.filter((call) => call.body.operation?.kind === "submit").length;
  await expect(endBrowserActionPreview(input)).rejects.toMatchObject({ code: "original_result_requires_recovery" });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(writes);
  await expect(f.client.context("preview:next-explicit-choice", { previewPermitId: "next" })).rejects.toMatchObject({ code: "recovery_required" });
 });
 it("permits a new explicitly confirmed preview only after verified expiry, retiring the old Context without rewriting its rejected cleanup", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const selection = { factory: "factory", recipe: "recipe.smelter.iron_ingot", batches: 1 };
  await f.client.context("preview:old", { previewPermitId: "old", previewRequest: { kind: "schedule", selection } });
  f.setOutcomeLookup("rejected");
  await expect(endBrowserActionPreview({ gameClient: f.client, recovery: f.recovery, confirmedFence: fence, permitId: "old", contextOperationId: "preview:old" }))
   .rejects.toMatchObject({ code: "original_result_requires_recovery" });
  const next = { gameClient: f.client, recovery: f.recovery, confirmedFence: fence, permitId: "new", kind: "schedule", selection };
  await expect(readBrowserActionPreview(next)).rejects.toMatchObject({ code: "recovery_required" });
  expect(f.entries.some((entry) => entry.original.operationId === "preview:new:prefix")).toBe(false);
  f.setTick(2); f.prepared.identity.valid_until_tick = "10";
  await expect(readBrowserActionPreview(next)).rejects.toMatchObject({ code: "quote_rejected" });
  const newPrefix = f.entries.find((entry) => entry.original.operationId === "preview:new:prefix");
  expect(newPrefix.state).toBe("committed"); expect(newPrefix.original.runBudget.expiredContextRetirements).toEqual(["preview:old"]);
  expect(f.entries.find((entry) => entry.original.operationId === "preview:old:outcome").state).toBe("rejected");
  expect(openBrowserPreviews(await f.recovery.load()).map((entry) => entry.contextOperationId)).toEqual(["preview:new"]);
  f.setOutcomeLookup("committed");
  await endBrowserActionPreview({ ...next, contextOperationId: "preview:new" });
  expect(await f.client.hasUnresolved()).toBe(false);
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit" && call.body.operation.request.signed_payload.payload.request.operation?.data?.context?.valid_until_tick === "1"
   && call.body.operation.request.signed_payload.payload.request.operation.operation === "task_provider_failure")).toHaveLength(1);
 });
 const grantOperation = { operation: "install", grant_id: "grant", object_id: "factory", action_kinds: ["ScheduleRecipe"],
  period_id: "first", valid_from_tick: "1", valid_until_tick: "100", limit_units: "8" };
 const grantRead = () => ({ native_delegation_source_id: "owner-source", native_delegation_authorizations: [{
  grant: { ...grantOperation, source_id: "owner-source", agent_id: "agent", revision: 1,
   valid_from_tick: 1, valid_until_tick: 100, limit_units: 8, revoked: false }, spent_units: 0, remaining_units: 8 }] });
 it("persists and owner-signs native action authority, then requires committed receipt and matching authoritative grant", async () => {
  const f = fixture(); f.setOwner(grantRead()); await f.client.bindFence(fence);
  const result = await f.client.nativeGrant({ permitId: "one", confirmedFence: fence, operation: grantOperation });
  expect(result.grant).toMatchObject({ object_id: "factory", limit_units: 8 });
  expect(f.order.indexOf("persist:native_grant")).toBeLessThan(f.order.indexOf("network:submit"));
  const request = f.calls.find((call) => call.body.operation?.kind === "submit");
  expect(request).toMatchObject({ path: "/entry-owner/control", body: { operation: { request: { signed_payload: {
   kind: "game_native_delegation", payload: { subject_public_key: key, request: { operation: grantOperation } } } } } } });
  expect(f.signed.some((item) => item.domain === "game_native_delegation/v1")).toBe(true);
  expect(f.signed.some((item) => item.domain === "delegation")).toBe(false);
 });
 it.each(["pending", "unknown", "rejected"])("does not unlock an observed grant from a %s original", async (status) => {
  const f = fixture(); f.setOwner(grantRead()); await f.client.bindFence(fence); f.setLookup(status);
  const input = { permitId: "one", confirmedFence: fence, operation: grantOperation };
  await expect(f.client.nativeGrant(input)).rejects.toMatchObject({ code: "native_grant_not_committed" });
  await expect(f.client.nativeGrant(input)).rejects.toMatchObject({ code: "native_grant_not_committed" });
  expect(f.calls.filter((call) => call.body.operation?.kind === "submit")).toHaveLength(1);
  expect(f.signed.filter((item) => item.domain === "game_native_delegation/v1")).toHaveLength(1);
 });
 it("rejects native grant readback with the wrong source, widened selection or imprecise counters", async () => {
  for (const change of [(grant) => { grant.source_id = "another-owner"; }, (grant) => { grant.object_id = "another-factory"; },
   (grant) => { grant.limit_units = Number.MAX_SAFE_INTEGER + 1; }]) {
   const f = fixture(), observed = grantRead(); change(observed.native_delegation_authorizations[0].grant); f.setOwner(observed);
   await f.client.bindFence(fence);
   await expect(f.client.nativeGrant({ permitId: "one", confirmedFence: fence, operation: grantOperation })).rejects.toMatchObject({ code: "native_grant_readback_required" });
  }
 });
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

describe("semantic fence equality", () => {
 const reordered = (v) => Array.isArray(v) ? v.map(reordered) : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().reverse().map((k) => [k, reordered(v[k])])) : v;
 it("accepts reordered nested canonical fences", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  f.setFence(reordered(fence));
  expect((await f.client.freshFence()).fence).toEqual(fence);
  expect(f.signed.at(-1).request).toBeDefined();
 });
 it("retires the original prefix when codec extraction and canonical fence key order differ", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("order-recovery"); await f.client.captureContext("order-recovery", prepared);
  const build = f.codec.build;
  f.codec.build = (kind, value) => kind === "field" ? JSON.stringify(reordered(JSON.parse(build(kind, value)))) : build(kind, value);
  f.setTick(2);
  await expect(f.client.retireExpiredContexts()).resolves.toEqual({ retired: 1 });
  expect(f.entries.filter((entry) => entry.original.kind === "outcome")).toHaveLength(1);
 });
 for (const [name, change] of [
  ["revision", (v) => ({ ...v, task_revision: "2" })],
  ["type", (v) => ({ ...v, owner_generation: 1 })],
  ["large integer", (v) => ({ ...v, owner_generation: "9007199254740994" })],
  ["array order", (v) => ({ ...v, test_sequence: [2, 1] })],
 ]) it(`rejects ${name} differences before new effects`, async () => {
  const base = { ...fence, owner_generation: "9007199254740993", test_sequence: [1, 2] };
  const f = fixture(); f.setFence(base); await f.client.bindFence(base);
  f.setFence(reordered(change(base)));
  await expect(f.client.freshFence()).rejects.toMatchObject({ code: "task_fence_changed" });
  expect(f.calls.some((call) => call.body.operation?.kind === "submit")).toBe(false);
 });
});

describe("original context resume", () => {
 it("reads the original committed prefix without replaying prepare or submitting effects", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  const prepared = await f.client.prepareContext("resume"); await f.client.captureContext("resume", prepared);
  const before = f.calls.length;
  const captured = await f.client.context("resume");
  expect(captured.prepared.identity).toEqual(prepared.identity);
  const resumed = f.calls.slice(before);
  expect(resumed.some((c) => ["prepare_context", "submit"].includes(c.body.operation?.kind))).toBe(false);
  expect(resumed.some((c) => c.body.operation?.kind === "read_context")).toBe(true);
 });
 it("creates once and resumes the same operation without replacing original budgets", async () => {
  const f = fixture(); await f.client.bindFence(fence);
  await f.client.context("new", { runId: "first" });
  const originals = structuredClone(f.entries.map((e) => e.original)); const count = f.calls.length;
  await f.client.context("new", { runId: "replacement" });
  expect(f.entries.map((e) => e.original)).toEqual(originals);
  expect(f.calls.slice(count).some((c) => ["prepare_context", "submit"].includes(c.body.operation?.kind))).toBe(false);
 });
 for (const status of ["unknown", "pending", "rejected", "expired"]) it(`does not read private Context for a ${status} prefix`, async () => {
  const f = fixture(); await f.client.bindFence(fence); const prepared = await f.client.prepareContext("not-ready");
  f.setLookup(status); try { await f.client.captureContext("not-ready", prepared); } catch {}
  const before = f.calls.length;
  await expect(f.client.context("not-ready")).rejects.toMatchObject({ code: ["unknown", "pending"].includes(status) ? "operation_pending" : "context_rejected" });
  expect(f.calls.slice(before).some((c) => ["prepare_context", "submit", "read_context"].includes(c.body.operation?.kind))).toBe(false);
 });
 it("preserves original action and outcome lanes instead of reopening private Context", async () => {
  for (const lane of ["action", "outcome"]) {
   const f = fixture(); await f.client.bindFence(fence); const prepared = await f.client.prepareContext("terminal"); const captured = await f.client.captureContext("terminal", prepared);
   if (lane === "action") await f.client.action("terminal", captured, { type: "MoveAgent" });
   else await f.client.outcome("terminal", captured, "executor_aborted");
   const before = f.calls.length;
   await expect(f.client.context("terminal")).rejects.toMatchObject({ code: "original_result_requires_recovery" });
   expect(f.calls).toHaveLength(before);
  }
 });
});

describe("context resume permission boundaries", () => {
 it("rechecks current fence before exposing private context", async () => {
  const f = fixture(); await f.client.bindFence(fence); await f.client.context("fence-resume");
  f.setFence({ ...fence, task_revision: "2" }); const before = f.calls.length;
  await expect(f.client.context("fence-resume")).rejects.toMatchObject({ code: "task_fence_changed" });
  expect(f.calls.slice(before).some((c) => ["submit", "read_context", "prepare_context"].includes(c.body.operation?.kind))).toBe(false);
 });
 it("does not replace an expired read with a new preparation", async () => {
  const f = fixture(); await f.client.bindFence(fence); await f.client.context("expired-read");
  const gateway = f.gatewayCall.getMockImplementation();
  f.gatewayCall.mockImplementation(async (...args) => { if (args[1].operation?.kind === "read_context") throw new Error("private expiry detail"); return gateway(...args); });
  const originals = structuredClone(f.entries.map((e) => e.original)); const before = f.calls.length;
  await expect(f.client.context("expired-read")).rejects.toMatchObject({ code: "game_response_invalid" });
  expect(f.entries.map((e) => e.original)).toEqual(originals);
  expect(f.calls.slice(before).some((c) => ["submit", "prepare_context"].includes(c.body.operation?.kind))).toBe(false);
 });
});

it("resumes a prefix through durable recovery after discarding client-local capture", async () => {
 const records = new Map();
 const storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
 const makeRecovery = () => createBrowserGameRecovery({ scope: { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent", taskId: "task", fence }, storage, locks: { request: async (_name, _options, action) => action() } });
 const first = fixture(makeRecovery()); await first.client.bindFence(fence);
 await first.client.context("persisted", { runId: "original" });
 const before = await first.recovery.load();
 const resumed = fixture(makeRecovery()); await resumed.client.bindFence(fence);
 const captured = await resumed.client.context("persisted", { runId: "must-not-replace" });
 expect(captured.prepared.identity).toEqual(first.prepared.identity);
 expect(resumed.calls.some((c) => ["prepare_context", "submit"].includes(c.body.operation?.kind))).toBe(false);
 expect((await resumed.recovery.load()).operations.map((e) => e.original)).toEqual(before.operations.map((e) => e.original));
});

it("signs the first abort against a refreshed context and only looks up its original on retry", async () => {
 const f = fixture(); await f.client.bindFence(fence);
 const prepared = await f.client.prepareContext("abort-refresh");
 const captured = await f.client.captureContext("abort-refresh", prepared);
 f.setTick(2);
 await f.client.outcome("abort-refresh", captured, "executor_aborted");
 const original = f.entries.find((entry) => entry.original.kind === "outcome").original;
 expect(JSON.parse(original.signedRequest.rawJson).signed_payload.payload.request.captured_base_binding).toEqual({ base_tick: 2 });
 const before = f.calls.length;
 await f.client.outcome("abort-refresh", captured, "executor_aborted");
 expect(f.calls.slice(before).map((call) => call.body.operation.kind)).toEqual(["lookup_executor"]);
});

it.each(["read rejection", "fence change"])("does not submit a new outcome after %s", async (failure) => {
 const f = fixture(); await f.client.bindFence(fence);
 const captured = await f.client.context("abort-blocked");
 if (failure === "fence change") f.setFence({ ...fence, task_revision: "2" });
 else {
  const gateway = f.gatewayCall.getMockImplementation();
  f.gatewayCall.mockImplementation(async (...args) => {
   if (args[1].operation?.kind === "read_context") throw new Error("read rejected");
   return gateway(...args);
  });
 }
 const before = f.calls.length;
 await expect(f.client.outcome("abort-blocked", captured, "executor_aborted")).rejects.toMatchObject({
  code: failure === "fence change" ? "task_fence_changed" : "game_response_invalid",
 });
 expect(f.entries.some((entry) => entry.original.kind === "outcome")).toBe(false);
 expect(f.calls.slice(before).some((call) => call.body.operation?.kind === "submit")).toBe(false);
});
