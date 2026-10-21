import { describe, expect, it, vi } from "vitest";
import { acceptsModelCredentials, createViewerModelConnectionClient, modelConnectionSnapshot } from "./viewer_model_connection_client.js";
const snapshot = { id: "connection-1", version: 1, preset_id: "deepseek-v1", state: "ready", run_state: "idle", expires_at_ms: 200000, absolute_expires_at_ms: 900000 };
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
describe("model connection client", () => {
 it("uses same-origin no-store requests and fresh identity CSRF for every write", async () => {
  const fetch = vi.fn(async (url) => response(url.endsWith("/hosted-account/v2/capabilities") ? { csrf_token: "identity-csrf" } : { ok: true, connection: snapshot }));
  const client = createViewerModelConnectionClient({ fetch, operationId: () => "operation-1" });
  await client.create({ preset_id: "deepseek-v1", raw_key: "private-canary-key", consent: true });
  await client.probe(snapshot); await client.activate(snapshot, { target_ref: { agent_id: "a" }, limits: { max_calls: 1 } });
  await client.lease(snapshot); await client.stop(snapshot); await client.forget(snapshot);
  expect(fetch.mock.calls.filter(([, init]) => init.method === "GET")).toHaveLength(6);
  const writes = fetch.mock.calls.filter(([, init]) => init.method !== "GET");
  expect(writes).toHaveLength(6);
  expect(writes[0][0]).toBe("/v1/agent-client/model-connections");
  expect(JSON.parse(writes[0][1].body)).toEqual({ preset_id: "deepseek-v1", raw_key: "private-canary-key", consent: true });
  for (const [url, init] of fetch.mock.calls) {
   expect(url.startsWith("/")).toBe(true); expect(url).not.toContain("private-canary");
   expect(init).toMatchObject({ credentials: "same-origin", mode: "same-origin", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer" });
   if (init.method !== "GET") expect(init.headers["X-Oasis7-CSRF"]).toBe("identity-csrf");
  }
  expect(writes.slice(1).every(([, init]) => JSON.parse(init.body).expected_version === 1)).toBe(true);
  expect(writes.at(-1)[1].method).toBe("DELETE");
 });
 it("refuses writes without CSRF before sending a key and never exposes upstream errors", async () => {
  const fetch = vi.fn(async () => response({ ok: true }));
  const client = createViewerModelConnectionClient({ fetch });
  await expect(client.create({ preset_id: "p", raw_key: "secret", consent: true })).rejects.toMatchObject({ code: "authentication_required" });
  expect(fetch).toHaveBeenCalledTimes(1);
  const failing = createViewerModelConnectionClient({ fetch: async () => { throw new Error("private-secret error body"); } });
  await expect(failing.capabilities()).rejects.toMatchObject({ message: "transport_failed" });
 });
 it("retains the same probe operation identity after an unknown transport result", async () => {
  let attempts = 0;
  const fetch = vi.fn(async (url) => {
   if (url.endsWith("/capabilities")) return response({ csrf_token: "csrf" });
   if (++attempts === 1) throw new Error("lost response");
   return response({ ok: true, connection: snapshot });
  });
  const operationId = vi.fn(() => "probe-original");
  const client = createViewerModelConnectionClient({ fetch, operationId });
  await expect(client.probe(snapshot)).rejects.toMatchObject({ code: "transport_failed" });
  await client.probe(snapshot);
  const sent = fetch.mock.calls.filter(([url]) => url.endsWith("/probe")).map(([, init]) => JSON.parse(init.body));
  expect(sent).toHaveLength(2); expect(sent[0]).toEqual(sent[1]); expect(operationId).toHaveBeenCalledTimes(1);
 });
 it("gates credentials on authenticated same-origin trust and complete bounded limits", () => {
  const cap = { ok: true, enabled: true, authenticated: true, trusted_executor: { trusted: true, display_name: "Executor", location: "node", origin: "https://game.example" }, presets: [{}], limits: { max_calls: 1, max_input_tokens: 10, max_output_tokens: 10, max_concurrent: 1, lease_ms: 120000, absolute_ms: 7200000, renew_interval_ms: 30000 } };
  expect(acceptsModelCredentials(cap, "https://game.example")).toBe(true);
  for (const changed of [{ ...cap, authenticated: false }, { ...cap, enabled: false }, { ...cap, trusted_executor: { ...cap.trusted_executor, trusted: false } }, { ...cap, limits: {} }]) expect(acceptsModelCredentials(changed, "https://game.example")).toBe(false);
  expect(acceptsModelCredentials(cap, "https://other.example")).toBe(false);
 });
 it("maps conflicts safely and drops unsolicited secret-bearing response fields", async () => {
  const client = createViewerModelConnectionClient({ fetch: async (url) => response(url.endsWith("/capabilities") ? { csrf_token: "csrf" } : { ok: true, connection: { ...snapshot, raw_key: "private-canary", error_code: "private-canary" } }) });
  const result = await client.probe(snapshot);
  expect(result.raw_key).toBeUndefined(); expect(result.error_code).toBe("request_failed");
  expect(JSON.stringify(result)).not.toContain("private-canary");
  const conflict = createViewerModelConnectionClient({ fetch: async (url) => url.endsWith("/capabilities") ? response({ csrf_token: "csrf" }) : response({ error: "private-canary" }, 409) });
  await expect(conflict.stop(snapshot)).rejects.toMatchObject({ code: "request_failed", status: 409 });
 });
 it("preserves safe model and budget errors instead of turning all conflicts into CAS errors", async () => {
  for (const code of ["budget_exhausted", "model_unavailable", "game_executor_unavailable"]) {
   const client = createViewerModelConnectionClient({ fetch: async (url) => url.endsWith("/capabilities") ? response({ csrf_token: "csrf" }) : response({ error_code: code, error: "private-canary" }, 409) });
   await expect(client.stop(snapshot)).rejects.toMatchObject({ code, status: 409 });
  }
 });

 it("keeps only bounded native execution status and hides unsolicited fields", () => {
  const result = modelConnectionSnapshot({ ...snapshot, execution: { phase: "finished", reason: "completed", successful_effects: 4, model_calls_spent: 6, raw_key: "private-canary", goal_completion_verified: true } });
  expect(result.execution).toEqual({ phase: "finished", reason: "completed", successful_effects: 4, model_calls_spent: 6, model_failure: null });
  expect(JSON.stringify(result)).not.toContain("private-canary");
  expect(modelConnectionSnapshot({ ...snapshot, execution: { phase: "finished", reason: "private-canary", successful_effects: -1 } }).execution.reason).toBeNull();
 });

});

it("allowlists model diagnostics and preserves legacy snapshots without raw details", () => {
 for (const code of ["input_limit", "output_limit", "model_identity_mismatch", "unexpected_streaming", "unsupported_finish_reason", "unsupported_tool", "invalid_tool_identity", "unsupported_content", "capability_unsupported", "other"]) {
  expect(modelConnectionSnapshot({ ...snapshot, execution: { phase: "finished", reason: "input_limit", model_failure: code, response: "private-canary" } }).execution).toMatchObject({ reason: "input_limit", model_failure: code });
 }
 for (const code of [undefined, "private-canary", { code: "output_limit", raw: "private-canary" }]) {
  const result = modelConnectionSnapshot({ ...snapshot, execution: { phase: "finished", reason: "model_failed", model_failure: code, response: "private-canary" } });
  expect(result.execution.model_failure).toBeNull();
  expect(JSON.stringify(result)).not.toContain("private-canary");
 }
});

it("uses a new explicit probe identity only after a confirmed terminal failure", async () => {
 let attempts = 0;
 const fetch = vi.fn(async (url) => url.endsWith("/capabilities") ? response({csrf_token:"csrf"}) : ++attempts === 1 ? response({error_code:"rate_limited", operation_state:"failed"},409) : response({ok:true,connection:snapshot,operation_state:"ready"}));
 let id=0; const client=createViewerModelConnectionClient({fetch,operationId:()=>`probe-${++id}`});
 await expect(client.probe(snapshot)).rejects.toMatchObject({code:"rate_limited"}); await client.probe(snapshot);
 expect(fetch.mock.calls.filter(([url])=>url.endsWith("/probe")).map(([,init])=>JSON.parse(init.body).operation_id)).toEqual(["probe-1","probe-2"]);
});
it("preserves typed protocol failures and never infers a CAS conflict from status alone", async () => {
 for (const code of ["model_identity_mismatch","invalid_response","unexpected_streaming","private-secret"]) {
 const client=createViewerModelConnectionClient({fetch:async()=>response({error_code:code},409)});
 await expect(client.capabilities()).rejects.toMatchObject({code:code==="private-secret"?"request_failed":code});
 }
});
it("reads 65 connections over coherent bounded pages", async () => {
 const items=Array.from({length:65},(_,i)=>({...snapshot,id:`connection-${i}`}));
 const fetch=vi.fn(async url=>response(url.endsWith("/pages/cursor-1")?{connections:items.slice(64),page_limit:64,list_cut:"cut",next_cursor:null}:{connections:items.slice(0,64),page_limit:64,list_cut:"cut",next_cursor:"cursor-1"}));
 expect(await createViewerModelConnectionClient({fetch}).list()).toHaveLength(65); expect(fetch).toHaveBeenCalledTimes(2);
});

it("keeps unknown probe identity and original version across read-only reconciliation", async () => {
 const fetch=vi.fn(async url=>url.endsWith("/capabilities")?response({csrf_token:"csrf"}):response({error_code:"request_failed"},502));
 let id=0;const client=createViewerModelConnectionClient({fetch,operationId:()=>`probe-${++id}`});
 await expect(client.probe(snapshot)).rejects.toMatchObject({code:"request_failed"});
 await expect(client.probe({...snapshot,version:2})).rejects.toMatchObject({code:"request_failed"});
 const sent=fetch.mock.calls.filter(([url])=>url.endsWith("/probe")).map(([,init])=>JSON.parse(init.body));
 expect(sent[0]).toEqual(sent[1]); expect(id).toBe(1);
});
it("rejects inconsistent, repeated, or oversized pages without returning partial management results", async () => {
 for(const second of [{connections:[snapshot],page_limit:64,list_cut:"changed",next_cursor:null},{connections:[snapshot],page_limit:64,list_cut:"cut",next_cursor:null},{connections:Array(65).fill(snapshot),page_limit:64,list_cut:"cut",next_cursor:null}]) {
 let calls=0;const client=createViewerModelConnectionClient({fetch:async()=>response(++calls===1?{connections:[snapshot],page_limit:64,list_cut:"cut",next_cursor:"cursor"}:second)});
 await expect(client.list()).rejects.toMatchObject({code:"request_failed"});
 }
});

it.each(["probe", "stop"])("starts a new explicit %s after authoritative non-admission and refreshed CAS version", async (action) => {
 let writes=0,id=0;
 const fetch=vi.fn(async(url,init)=>url.endsWith("/capabilities")?response({csrf_token:"csrf"}):init.method==="GET"?response({connection:{...snapshot,version:2}}):++writes===1?response({error_code:"version_conflict",operation_state:"not_started"},409):response({connection:{...snapshot,version:2},operation_state:"ready"}));
 const client=createViewerModelConnectionClient({fetch,operationId:()=>`probe-${++id}`});
 await expect(client[action](snapshot)).rejects.toMatchObject({code:"version_conflict"});
 await client[action](await client.read(snapshot));
 const sent=fetch.mock.calls.filter(([url])=>url.endsWith(`/${action}`)).map(([,init])=>JSON.parse(init.body));
 expect(sent.map(v=>[v.operation_id,v.expected_version])).toEqual([["probe-1",1],["probe-2",2]]);
});
