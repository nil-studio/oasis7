import { createSignal } from "solid-js";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { createBrowserParticipantGateway } from "./browser_participant_entry.js";
vi.mock("./model_connection_panel.jsx", () => ({ ModelConnectionPanel: () => <p>Legacy</p> }));
import { BrowserByokSurface } from "./browser_byok_surface.jsx";
afterEach(cleanup);
const config = { endpoint: "/api/public/hosted-account/v2", servicePublicKey: "test" };
const canary = "SECRET_KEY_SIGNATURE_PRIVATE_GOAL";
const session = () => new Response(JSON.stringify({ api_version: 2, ok: true, account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) }));
function gateway(response) {
  return createBrowserParticipantGateway({ config, accountId: "account", fetchImpl: vi.fn(async (url) => url.endsWith("/session") ? session() : response()) });
}
it("retains only safe HTTP and known IdentityReply error codes", async () => {
  for (const code of ["owner_control_unavailable", canary]) {
    const client = gateway(() => new Response(JSON.stringify({ api_version: 2, ok: false, error_code: code, message: canary, diagnostic_id: canary, next_action: canary }), { status: 503 }));
    const error = await client.gatewayCall("/entry-owner/control", {}).catch((value) => value);
    expect(error).toMatchObject({ code: "participant_request_failed", diagnostic: { stage: "owner_control", httpStatus: 503 } });
    expect(error.diagnostic.serverCode).toBe(code === canary ? undefined : code);
    expect(JSON.stringify(error)).not.toContain(canary);
  }
});
it("distinguishes fetch failure from unverified signed replies without leaking raw exceptions", async () => {
  const error = await gateway(() => { throw new TypeError(canary); }).gatewayCall("/entry-owner/control", {}).catch((value) => value);
  expect(error).toMatchObject({ code: "participant_network_failed", diagnostic: { stage: "owner_control" } });
  expect(JSON.stringify(error)).not.toContain(canary);
});
const owner = { publicKey: "owner", sign: vi.fn() };
function surface(entry, extra = {}) {
  return render(() => <BrowserByokSurface locale="en" config={config} ownerSigner={owner} accountId="account" protocolLoader={async () => ({})} createOwnerEntry={() => entry} {...extra} />);
}
it("shows actionable inspect diagnostics, handles pending codes and clears them on successful read-only inspection", async () => {
  const inspect = vi.fn().mockRejectedValueOnce({ code: "owner_proof_invalid", diagnostic: { stage: "owner_task_lookup" }, message: canary })
    .mockResolvedValueOnce({ state: "pending", code: "owner_response_unknown", diagnostic: { stage: "owner_task_lookup" } })
    .mockResolvedValue({ state: "needs_task" });
  const view = surface({ inspect });
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain("owner_proof_invalid"));
  expect(view.getByRole("alert").textContent).toContain("owner_task_lookup");
  expect(view.container.textContent).not.toContain(canary);
  expect(view.container.textContent).toContain("Entry inspection failed");
  fireEvent.click(view.getByRole("button", { name: "Look up account entry originals" }));
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain("owner_response_unknown"));
  await waitFor(() => expect(view.container.textContent).toContain("Original result unconfirmed"));
  fireEvent.click(view.getByRole("button", { name: "Look up account entry originals" }));
  await waitFor(() => expect(view.queryByRole("alert")).toBeNull());
});
it("never reflects arbitrary client codes, stages or error text", async () => {
  const view = surface({ inspect: async () => { throw { code: canary, diagnostic: { stage: canary, serverCode: canary, httpStatus: canary }, message: canary }; } });
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain("game_operation_failed"));
  expect(view.container.textContent).not.toContain(canary);
});
it.each(["result", "error"])("fences a late account %s and leaves current diagnostics intact", async (kind) => {
  let late;
  const [account, setAccount] = createSignal("old");
  const inspect = vi.fn(() => account() === "old" ? new Promise((resolve, reject) => { late = kind === "error" ? reject : resolve; }) : Promise.reject({ code: "participant_storage_failed" }));
  const view = surface({ inspect, dispose: vi.fn() }, { accountId: account });
  await waitFor(() => expect(late).toBeTypeOf("function"));
  setAccount("new");
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain("participant_storage_failed"));
  late(kind === "error" ? { code: "owner_response_unknown" } : { state: "pending", code: "owner_response_unknown" });
  await Promise.resolve();
  expect(view.getByRole("alert").textContent).toContain("participant_storage_failed");
  expect(view.getByRole("alert").textContent).not.toContain("owner_response_unknown");
});

it.each([202, 403])("classifies HTTP %s known errors without equating gateway errors with world rejection", async (status) => {
  const serverCode = status === 202 ? "operation_pending" : "csrf_failed";
  const error = await gateway(() => new Response(JSON.stringify({ api_version: 2, ok: false, error_code: serverCode }), { status })).gatewayCall("/entry-owner/control", {}).catch((value) => value);
  expect(error.diagnostic).toMatchObject({ httpStatus: status, serverCode, category: status === 202 ? "pending" : "permission" });
});
it.each(["malformed", "oversized", "wrong-schema"])("bounds and sanitizes %s gateway errors", async (kind) => {
  const cancel = vi.fn();
  const bytes = new TextEncoder().encode(kind === "oversized" ? canary.repeat(500) : kind === "malformed" ? canary : JSON.stringify({ api_version: 9, ok: false, error_code: "csrf_failed", message: canary }));
  const chunks = kind === "oversized" ? [bytes.slice(0, 2048), bytes.slice(2048, 4096), bytes.slice(4096)] : [bytes];
  let index = 0;
  const response = { ok: false, status: 503, body: { getReader: () => ({ read: async () => index < chunks.length ? { value: chunks[index++], done: false } : { done: true }, cancel }) } };
  const error = await gateway(() => response).gatewayCall("/entry-owner/control", {}).catch((value) => value);
  expect(error.diagnostic).toMatchObject({ httpStatus: 503, stage: "owner_control" });
  expect(error.diagnostic.serverCode).toBeUndefined();
  expect(JSON.stringify(error)).not.toContain(canary);
  expect(cancel).toHaveBeenCalledTimes(1);
});
it.each([["pending", "pending"], ["unknown", "unknown"], ["history_unavailable", "history_unavailable"]])("shows verified %s with its distinct classification", async (outcome, category) => {
  const view = surface({ inspect: async () => ({ state: "pending", code: "owner_operation_pending", diagnostic: { stage: "owner_task_lookup", outcome } }) });
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain(`category=${category}`));
  expect(view.getByRole("button", { name: "Prepare player, Agent and Task" }).disabled).toBe(true);
  expect(view.getByRole("alert").textContent).not.toContain("world rejected");
});

it("does not let an unrelated session notification erase a failed operation", async () => {
  let notify; const session = { snapshot: () => ({ phase: "unprepared", ready: false }), subscribe: (fn) => { notify = fn; return () => {}; },
    prepare: async () => { throw { code: "participant_storage_failed" }; }, dispose: vi.fn(), modelSession: { subscribe: () => () => {}, snapshot: () => ({ hasKey: false }) } };
  const view = surface({ inspect: async () => ({ state: "ready", agentId: "agent", taskId: "task" }) }, { createSession: () => session });
  await waitFor(() => expect(view.getByRole("button", { name: "Inspect current task" })).toBeTruthy());
  fireEvent.click(view.getByRole("button", { name: "Inspect current task" }));
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain("participant_storage_failed"));
  notify();
  expect(view.getByRole("alert").textContent).toContain("participant_storage_failed");
});
it.each([["owner_operation_rejected", "rejected"], ["owner_operation_expired", "expired"]])("does not confuse confirmed terminal %s with unknown", async (code, category) => {
  const view = surface({ inspect: async () => { throw { code, diagnostic: { stage: "owner_task_lookup" } }; } });
  await waitFor(() => expect(view.getByRole("alert").textContent).toContain(`category=${category}`));
  expect(view.getByRole("alert").textContent).toContain(category === "expired" ? "original expired" : "world rejected");
});
