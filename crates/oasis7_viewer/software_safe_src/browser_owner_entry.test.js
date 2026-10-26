import { describe, expect, it, vi } from "vitest";
import { fixture } from "./test_support/browser_owner_entry_fixture.js";
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
