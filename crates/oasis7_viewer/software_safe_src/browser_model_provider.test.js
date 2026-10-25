import { describe, expect, it, vi } from "vitest";
import { BrowserModelError, callBrowserModel, probeBrowserModel, PROVIDER_PRESETS } from "./browser_model_provider.js";
const tool = { type: "function", function: { name: "read_fixture", parameters: { type: "object", properties: {}, additionalProperties: false } } };
const complete = (message = { role: "assistant", content: "OK" }, finish_reason = "stop") => ({ model: "deepseek-flash", choices: [{ message, finish_reason }], usage: { prompt_tokens: 10, completion_tokens: 2, total_tokens: 12 } });
const toolMessage = (args = "{}", name = "read_fixture") => ({ role: "assistant", content: null, tool_calls: [{ id: "call_1", type: "function", function: { name, arguments: args } }] });
const reply = (raw, status = 200) => new Response(JSON.stringify(raw), { status, headers: { "content-type": "application/json" } });
const options = { key: "fixture-canary-secret", messages: [{ role: "user", content: "fixture" }] };
describe("browser model provider", () => {
 it("performs a bounded read-only tool round trip, reserving and settling every physical call", async () => {
  const beforeSend = vi.fn(async () => "reservation"), afterResponse = vi.fn();
  const fetchImpl = vi.fn(async () => reply(fetchImpl.mock.calls.length === 1 ? complete(toolMessage(), "tool_calls") : complete()));
  expect(await probeBrowserModel({ key: options.key, fetchImpl, beforeSend, afterResponse })).toMatchObject({ tool_round_trip: true, model: "deepseek-flash" });
  expect(beforeSend).toHaveBeenCalledTimes(2); expect(afterResponse).toHaveBeenCalledTimes(2);
  for (const [url, init] of fetchImpl.mock.calls) {
   expect(url).toBe("https://api.deepseek.com/chat/completions");
   expect(init).toMatchObject({ credentials: "omit", redirect: "error", referrerPolicy: "no-referrer", mode: "cors" });
   expect(init.headers.Authorization).toBe(`Bearer ${options.key}`);
   expect(init.body).not.toContain(options.key);
   expect(JSON.parse(init.body)).toMatchObject({ model: "deepseek-flash", max_tokens: 128, stream: false, thinking: { type: "disabled" } });
  }
  const second = JSON.parse(fetchImpl.mock.calls[1][1].body);
  expect(second.messages.at(-1)).toEqual({ role: "tool", tool_call_id: "call_1", content: '{"fixture":"OK"}' });
  expect(second.tool_choice).toBe("none");
  expect(PROVIDER_PRESETS.find((preset) => preset.provider_id === "openrouter").supported).toBe(false);
 });
 it("does not reserve or send an oversized candidate and does not send after failed reservation", async () => {
  const beforeSend = vi.fn(), fetchImpl = vi.fn();
  await expect(callBrowserModel({ ...options, messages: [{ role: "user", content: "x".repeat(65536) }], fetchImpl, beforeSend })).rejects.toMatchObject({ code: "input_limit" });
  expect(beforeSend).not.toHaveBeenCalled();
  await expect(callBrowserModel({ ...options, fetchImpl, beforeSend: () => { throw new BrowserModelError("budget_exhausted"); } })).rejects.toMatchObject({ code: "budget_exhausted" });
  expect(fetchImpl).not.toHaveBeenCalled();
 });
 it("rejects unsupported schema constraints before reserving a paid call", async () => {
  const fetchImpl = vi.fn(), beforeSend = vi.fn();
  const unsupported = { ...tool, function: { ...tool.function, parameters: { ...tool.function.parameters, oneOf: [{}] } } };
  await expect(callBrowserModel({ ...options, tools: [unsupported], fetchImpl, beforeSend })).rejects.toMatchObject({ code: "input_invalid" });
  expect(beforeSend).not.toHaveBeenCalled(); expect(fetchImpl).not.toHaveBeenCalled();
 });
 it("re-reads credentials after each reservation, without a key argument", async () => {
  const order = [], beforeSend = vi.fn(async () => { order.push("reserve"); return beforeSend.mock.calls.length; });
  const getCredential = vi.fn(async (reservation) => { order.push(`credential:${reservation}`); return `fixture-key-${reservation}`; });
  const fetchImpl = vi.fn(async () => { order.push("send"); return reply(fetchImpl.mock.calls.length === 1 ? complete(toolMessage(), "tool_calls") : complete()); });
  await probeBrowserModel({ beforeSend, getCredential, fetchImpl });
  expect(order).toEqual(["reserve", "credential:1", "send", "reserve", "credential:2", "send"]);
  expect(fetchImpl.mock.calls[1][1].headers.Authorization).toBe("Bearer fixture-key-2");
 });
 it("does not send after the credential gate rejects or Stop aborts during reservation", async () => {
  const fetchImpl = vi.fn();
  await expect(callBrowserModel({ messages: options.messages, beforeSend: async () => "r", getCredential: async () => { throw new BrowserModelError("stopped"); }, fetchImpl })).rejects.toMatchObject({ code: "stopped" });
  const controller = new AbortController();
  await expect(callBrowserModel({ messages: options.messages, signal: controller.signal, beforeSend: async () => { controller.abort(); }, getCredential: async () => options.key, fetchImpl })).rejects.toMatchObject({ code: "cancelled" });
  expect(fetchImpl).not.toHaveBeenCalled();
 });
 it.each([
  [complete(toolMessage('{"extra":true}'), "tool_calls"), "tool_invalid"],
  [complete(toolMessage("{}", "write_world"), "tool_calls"), "tool_invalid"],
  [complete(toolMessage("{"), "tool_calls"), "tool_invalid"],
  [complete(toolMessage(), "stop"), "tool_invalid"],
  [complete(undefined, "length"), "protocol_incompatible"],
  [{ ...complete(), model: "different-model" }, "protocol_incompatible"],
 ])("rejects incompatible terminal/tool envelopes before any tool execution", async (raw, code) => {
  await expect(callBrowserModel({ ...options, tools: [tool], fetchImpl: async () => reply(raw) })).rejects.toMatchObject({ code });
 });
 it("drops untrusted extra fields and preserves unknown usage", async () => {
  const raw = { ...complete(), private_debug: options.key, usage: { total_tokens: 0 } };
  raw.choices[0].message.reasoning_content = options.key;
  const afterResponse = vi.fn();
  const result = await callBrowserModel({ ...options, fetchImpl: async () => reply(raw), afterResponse });
  expect(result.usage).toBeNull(); expect(JSON.stringify(result)).not.toContain(options.key);
  expect(afterResponse).toHaveBeenCalledWith({ reservation: undefined, usage: null, status: "completed" });
 });
 it("bounds streamed accumulation and keeps failed usage reserved", async () => {
  const afterResponse = vi.fn(); let cancelled = false;
  const body = new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array(262145)); }, cancel() { cancelled = true; } });
  await expect(callBrowserModel({ ...options, fetchImpl: async () => new Response(body), afterResponse })).rejects.toMatchObject({ code: "output_limit" });
  expect(cancelled).toBe(true); expect(afterResponse).toHaveBeenCalledWith({ reservation: undefined, usage: null, status: "unknown" });
 });
 it("never exposes provider errors or transport exception text", async () => {
  for (const fetchImpl of [async () => reply({ error: options.key }, 401), async () => { throw new Error(options.key); }]) {
   try { await callBrowserModel({ ...options, fetchImpl }); throw new Error("expected failure"); }
   catch (error) { expect(error).toBeInstanceOf(BrowserModelError); expect(error.message).not.toContain(options.key); }
  }
 });
 it("settles a hung physical request as unknown at the timeout boundary", async () => {
  vi.useFakeTimers();
  try {
   const afterResponse = vi.fn();
   const call = callBrowserModel({ ...options, fetchImpl: () => new Promise(() => {}), afterResponse });
   const assertion = expect(call).rejects.toMatchObject({ code: "timeout" });
   await vi.advanceTimersByTimeAsync(30000); await assertion;
   expect(afterResponse).toHaveBeenCalledWith({ reservation: undefined, usage: null, status: "unknown" });
  } finally { vi.useRealTimers(); }
 });
 it("does not call the provider for an already aborted operation", async () => {
  const controller = new AbortController(); controller.abort(); const fetchImpl = vi.fn(), beforeSend = vi.fn();
  await expect(callBrowserModel({ ...options, signal: controller.signal, fetchImpl, beforeSend })).rejects.toMatchObject({ code: "cancelled" });
  expect(fetchImpl).not.toHaveBeenCalled(); expect(beforeSend).not.toHaveBeenCalled();
 });
});
