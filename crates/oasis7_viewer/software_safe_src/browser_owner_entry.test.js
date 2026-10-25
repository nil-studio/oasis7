import { describe, expect, it, vi } from "vitest";
import { createBrowserOwnerEntry } from "./browser_owner_entry.js";
const ownerKey = "44".repeat(32);
const config = { endpoint: "/api/public/hosted-account/v2", servicePublicKey: "11".repeat(32), participantIssuerPublicKey: "22".repeat(32), world: { world_id: "world", genesis_digest: "genesis" }, identity: { service_id: "identity", environment_id: "test", instance_epoch: "1" } };
function fixture() {
 const records = new Map(), calls = []; let bound = false, task = null, originalOwner, unknown = false, rejectOwner = false, unknownOwnerKind = null;
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
 const fetchImpl = vi.fn(async (url, options) => {
  const path = url.slice(config.endpoint.length); const body = options?.body ? JSON.parse(options.body) : null; calls.push({ path, body });
  if (path === "/entry/discovery") return respond({ schema_revision: 1, world: config.world, world_service_public_key: config.servicePublicKey, identity: config.identity, participant_policy: { policy: { issuer_public_key: config.participantIssuerPublicKey } }, phase_capabilities: { participant_recovery: true } });
  if (path === "/session") return respond({ account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) });
  if (path === "/participant/authorize") { originalOwner = body; return respond({ phase: "participant_authorized", new_registration_enabled: true, admission: { subject_public_key: config.participantIssuerPublicKey, request: {} } }); }
  if (path === "/participant/owner-view") return respond({ world_response: bound && !rejectOwner ? control() : null });
  if (path === "/participant/submit" || path === "/participant/lookup") {
   if (path.endsWith("submit") && !rejectOwner) bound = true;
   return respond({ world_response: { outcome: { status: rejectOwner ? "rejected" : unknown ? "unknown" : "committed", receipt: { account_id: "account", owner_public_key: originalOwner.subject_public_key, agent_id: "canonical-agent" } } } });
  }
  if (path === "/entry-owner/control") {
   expect(body.access.read.request).not.toHaveProperty("formal_anchor");
   const operation = body.operation;
   if (operation.kind === "prepare_goal") return respond({ world_response: { owner_proof: operation.request, goal_ref: "fixture-goal" } });
   const payload = operation.kind === "submit" ? operation.request.signed_payload : operation.request.original;
   if (operation.kind === "submit" && payload.kind === "game_task_create") task = { task_id: "canonical-task", agent_id: "canonical-agent", active_goal_revision: "0", active_goal_reference: null };
   if (operation.kind === "submit" && payload.kind === "game_goal_publish") task = { ...task, active_goal_revision: "1", active_goal_reference: { goal_ref: "fixture-goal" } };
   return respond({ world_response: { outcome: { status: unknown || unknownOwnerKind === payload.kind ? "unknown" : "committed", receipt: { original: payload.kind } } } });
  }
  throw new Error(`unexpected fixture route ${path}`);
 });
 const create = (extra = {}) => createBrowserOwnerEntry({ config, codec, accountId: "account", ownerSigner: { publicKeyHex: ownerKey, sign: async () => new Uint8Array(64) }, storage, locks: { request: async (_name, _options, run) => run() }, fetchImpl, cryptoImpl: { getRandomValues: (bytes) => bytes.fill(7) }, ...extra });
 return { create, calls, records, fetchImpl, setOwnerUnknown: (kind) => { unknownOwnerKind = kind; }, setUnknown: (value) => { unknown = value; }, existing: () => { bound = true; task = { task_id: "canonical-task", agent_id: "canonical-agent", active_goal_revision: "1", active_goal_reference: { goal_ref: "existing" } }; }, mismatch: () => { rejectOwner = true; } };
}
describe("browser canonical owner entry", () => {
 it("prepares a fresh account, real Task and goal before any model/executor action", async () => {
  const f = fixture(), entry = f.create();
  expect(await entry.inspect()).toMatchObject({ state: "needs_participant" });
  expect(f.calls.some((call) => call.path.endsWith("submit"))).toBe(false);
  expect(await entry.prepare({ goal: "Build a workshop", consent: true })).toMatchObject({ state: "ready", agentId: "canonical-agent", taskId: "canonical-task" });
  const mutations = f.calls.filter((call) => call.body?.operation?.kind === "submit").map((call) => call.body.operation.request.signed_payload.kind);
  expect(mutations).toEqual(["game_task_create", "game_goal_publish"]);
  expect(f.calls.some((call) => call.path === "/entry-executor/control")).toBe(false);
  expect([...f.records.values()].find((record) => record.taskRaw).goalPrepareRaw).toContain("Build a workshop");
 });
 it("keeps unknown registration immutable and recovers read-only before further effects", async () => {
  const f = fixture(); f.setUnknown(true);
  expect(await f.create().prepare({ goal: "Goal", consent: true })).toMatchObject({ state: "pending" });
  const submitted = f.calls.filter((call) => call.path === "/participant/submit"); expect(submitted).toHaveLength(1);
  expect(await f.create().inspect()).toMatchObject({ state: "pending" });
  expect(await f.create().prepare({ goal: "Other goal", consent: true })).toMatchObject({ state: "pending" });
  expect(f.calls.filter((call) => call.path === "/participant/submit")).toHaveLength(1);
  f.setUnknown(false);
  expect(await f.create().inspect()).toMatchObject({ state: "needs_task", agentId: "canonical-agent" });
 });
 it.each(["game_task_create", "game_goal_publish"])("looks up an unknown %s original without creating a replacement", async (kind) => {
  const f = fixture(); f.setOwnerUnknown(kind);
  expect(await f.create().prepare({ goal: "Original goal", consent: true })).toMatchObject({ state: "pending" });
  const before = f.calls.filter((call) => call.body?.operation?.kind === "submit").length;
  const original = [...f.records.values()].find((record) => record.taskRaw);
  const expectedRaw = kind === "game_task_create" ? original.taskRaw : original.publishRaw;
  expect(await f.create().inspect()).toMatchObject({ state: "pending" });
  expect(await f.create().prepare({ goal: "Replacement goal", consent: true })).toMatchObject({ state: "pending" });
  expect(f.calls.filter((call) => call.body?.operation?.kind === "submit")).toHaveLength(before);
  expect([...f.records.values()].find((record) => record.taskRaw)[kind === "game_task_create" ? "taskRaw" : "publishRaw"]).toBe(expectedRaw);
  f.setOwnerUnknown(null);
  expect((await f.create().inspect()).state).toBe(kind === "game_task_create" ? "needs_goal" : "ready");
 });
 it("reuses an existing canonical Agent and active goal without rewriting it", async () => {
  const f = fixture(); f.existing();
  expect(await f.create().prepare({ goal: "Do not replace existing", consent: true })).toMatchObject({ state: "ready", agentId: "canonical-agent" });
  expect(f.calls.some((call) => call.path === "/participant/submit" || call.body?.operation?.kind === "submit")).toBe(false);
 });
 it("fails closed on a different existing owner and never creates an alternative Agent", async () => {
  const f = fixture(); f.mismatch();
  await expect(f.create().prepare({ goal: "Goal", consent: true })).rejects.toMatchObject({ code: "participant_registration_rejected" });
  await expect(f.create().prepare({ goal: "Goal", consent: true })).rejects.toMatchObject({ code: "participant_registration_rejected" });
  expect(f.calls.filter((call) => call.path === "/participant/submit")).toHaveLength(1);
  expect(f.calls.some((call) => call.body?.operation?.kind === "submit")).toBe(false);
 });
 it("aborts a real entry session gate before any authorization or world POST", async () => {
  const f = fixture(), abort = new AbortController(); let release;
  const fetchImpl = vi.fn((url, options) => url.endsWith("/session") ? new Promise((resolve) => { release = resolve; }) : f.fetchImpl(url, options));
  const entry = f.create({ signal: abort.signal, fetchImpl });
  const assertion = expect(entry.prepare({ goal: "Goal", consent: true })).rejects.toMatchObject({ code: "participant_stopped" });
  await vi.waitFor(() => expect(release).toBeTypeOf("function"));
  abort.abort(); await assertion;
  release(new Response(JSON.stringify({ api_version: 2, ok: true, account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) })));
  await Promise.resolve(); await Promise.resolve();
  expect(fetchImpl.mock.calls.some(([, options]) => options.method === "POST")).toBe(false);
  expect(f.calls.some((call) => call.path === "/participant/submit" || call.path === "/entry-owner/control")).toBe(false);
 });
 it("requires explicit consent and ignores a disposed entry", async () => {
  const f = fixture(), entry = f.create();
  await expect(entry.prepare({ goal: "Goal" })).rejects.toMatchObject({ code: "owner_consent_required" });
  entry.dispose(); await expect(entry.inspect()).rejects.toMatchObject({ code: "participant_stopped" });
  expect(f.calls).toHaveLength(0);
 });
});
