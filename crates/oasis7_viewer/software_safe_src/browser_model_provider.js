// CallerSnapshot history: the caller supplies the complete authorized conversation.
export const PROVIDER_PRESETS = Object.freeze([
 Object.freeze({ id: "deepseek-browser-v1", provider_id: "deepseek", model: "deepseek-flash", protocol: "chat_completions", endpoint: "https://api.deepseek.com/chat/completions", supported: true }),
 Object.freeze({ id: "openrouter-browser-unavailable", provider_id: "openrouter", supported: false, reason: "provider_not_supported" }),
]);
const PRESET = PROVIDER_PRESETS[0];
const INPUT_LIMIT = 65536, OUTPUT_LIMIT = 262144, TIMEOUT_MS = 30000;
export class BrowserModelError extends Error {
 constructor(code) { super(code); this.name = "BrowserModelError"; this.code = code; }
}
const fail = (code) => { throw new BrowserModelError(code); };
const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const bytes = (value) => new TextEncoder().encode(value).byteLength;
function supportedSchema(schema, depth = 0) {
 if (!object(schema) || depth > 12) return false;
 const common = ["type", "description", "enum"];
 const fields = { object: ["properties", "required", "additionalProperties"], array: ["items", "minItems", "maxItems"], string: ["minLength", "maxLength"], integer: ["minimum", "maximum"], number: ["minimum", "maximum"], boolean: [], null: [] };
 if (!Object.hasOwn(fields, schema.type) || Object.keys(schema).some((name) => ![...common, ...fields[schema.type]].includes(name))) return false;
 if (schema.enum !== undefined && (!Array.isArray(schema.enum) || !schema.enum.length)) return false;
 for (const name of ["minItems", "maxItems", "minLength", "maxLength"]) if (schema[name] !== undefined && (!Number.isSafeInteger(schema[name]) || schema[name] < 0)) return false;
 for (const name of ["minimum", "maximum"]) if (schema[name] !== undefined && (typeof schema[name] !== "number" || !Number.isFinite(schema[name]))) return false;
 if (schema.type === "object") return object(schema.properties) && schema.additionalProperties === false &&
  (schema.required === undefined || (Array.isArray(schema.required) && schema.required.every((name) => typeof name === "string" && Object.hasOwn(schema.properties, name)))) &&
  Object.values(schema.properties).every((item) => supportedSchema(item, depth + 1));
 if (schema.type === "array") return supportedSchema(schema.items, depth + 1);
 return true;
}
function schemaMatches(value, schema, depth = 0) {
 if (!object(schema) || depth > 12) return false;
 if (schema.enum && (!Array.isArray(schema.enum) || !schema.enum.some((item) => JSON.stringify(item) === JSON.stringify(value)))) return false;
 switch (schema.type) {
 case "object": return object(value) && object(schema.properties) && schema.additionalProperties === false &&
  (schema.required ?? []).every((name) => Object.hasOwn(value, name)) &&
  Object.entries(value).every(([name, item]) => Object.hasOwn(schema.properties, name) && schemaMatches(item, schema.properties[name], depth + 1));
 case "array": return Array.isArray(value) && value.length >= (schema.minItems ?? 0) && value.length <= Math.min(schema.maxItems ?? 64, 64) && value.every((item) => schemaMatches(item, schema.items, depth + 1));
 case "string": return typeof value === "string" && value.length >= (schema.minLength ?? 0) && value.length <= Math.min(schema.maxLength ?? 8192, 8192);
 case "integer": return Number.isSafeInteger(value) && value >= (schema.minimum ?? -Number.MAX_SAFE_INTEGER) && value <= (schema.maximum ?? Number.MAX_SAFE_INTEGER);
 case "number": return typeof value === "number" && Number.isFinite(value) && value >= (schema.minimum ?? -Number.MAX_VALUE) && value <= (schema.maximum ?? Number.MAX_VALUE);
 case "boolean": return typeof value === "boolean";
 case "null": return value === null;
 default: return false;
 }
}
function normalizeResponse(raw, tools) {
 if (!object(raw) || raw.model !== PRESET.model || !Array.isArray(raw.choices) || raw.choices.length !== 1) fail("protocol_incompatible");
 const choice = raw.choices[0], message = choice?.message;
 if (!object(message) || message.role !== "assistant" || !["stop", "tool_calls"].includes(choice.finish_reason)) fail("protocol_incompatible");
 if (message.content !== null && message.content !== undefined && typeof message.content !== "string") fail("protocol_incompatible");
 const clean = { role: "assistant", content: message.content ?? null };
 if (choice.finish_reason === "tool_calls") {
  if (!Array.isArray(message.tool_calls) || !message.tool_calls.length || message.tool_calls.length > 8) fail("tool_invalid");
  const ids = new Set();
  clean.tool_calls = message.tool_calls.map((call) => {
   const fn = call?.function, definition = tools.find((tool) => tool.function.name === fn?.name);
   if (call?.type !== "function" || typeof call.id !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(call.id) || ids.has(call.id) || !definition || typeof fn.arguments !== "string" || bytes(fn.arguments) > 16384) fail("tool_invalid");
   ids.add(call.id);
   let args; try { args = JSON.parse(fn.arguments); } catch { fail("tool_invalid"); }
   if (!schemaMatches(args, definition.function.parameters)) fail("tool_invalid");
   return { id: call.id, type: "function", function: { name: fn.name, arguments: fn.arguments } };
  });
 } else if (message.tool_calls?.length) fail("tool_invalid");
 let usage = null;
 if (object(raw.usage) && ["prompt_tokens", "completion_tokens", "total_tokens"].every((name) => Number.isSafeInteger(raw.usage[name]) && raw.usage[name] >= 0) && raw.usage.total_tokens === raw.usage.prompt_tokens + raw.usage.completion_tokens) {
  usage = { prompt_tokens: raw.usage.prompt_tokens, completion_tokens: raw.usage.completion_tokens, total_tokens: raw.usage.total_tokens };
 }
 return { model: PRESET.model, choices: [{ index: 0, finish_reason: choice.finish_reason, message: clean }], usage };
}
async function readBounded(response) {
 const declared = Number(response.headers?.get("content-length"));
 if (declared > OUTPUT_LIMIT) fail("output_limit");
 if (!response.body?.getReader) fail("protocol_incompatible");
 const reader = response.body.getReader(), decoder = new TextDecoder("utf-8", { fatal: true });
 let length = 0, result = "";
 try {
  for (;;) {
   const { done, value } = await reader.read();
   if (done) break;
   length += value.byteLength;
   if (length > OUTPUT_LIMIT) fail("output_limit");
   result += decoder.decode(value, { stream: true });
  }
  result += decoder.decode();
 } finally { try { await reader.cancel(); } catch { /* no upstream diagnostics */ } }
 try { return JSON.parse(result); } catch { fail("protocol_incompatible"); }
}
export async function callBrowserModel({ key, getCredential, messages, tools = [], toolChoice, signal, fetchImpl = globalThis.fetch, maxOutputTokens = 128, beforeSend, afterResponse } = {}) {
 const validKey = (value) => typeof value === "string" && value.trim() && !/[\r\n]/.test(value);
 if (typeof getCredential !== "function" && !validKey(key)) fail("credential_invalid");
 if (!Array.isArray(messages) || !messages.length || !Array.isArray(tools) || tools.length > 16 || tools.some((tool) => tool?.type !== "function" || !/^[A-Za-z0-9_-]{1,64}$/.test(tool.function?.name) || !supportedSchema(tool.function?.parameters))) fail("input_invalid");
 if (new Set(tools.map((tool) => tool.function.name)).size !== tools.length) fail("input_invalid");
 if (!Number.isSafeInteger(maxOutputTokens) || maxOutputTokens < 1 || maxOutputTokens > 2048) fail("input_limit");
 let body;
 try { body = JSON.stringify({ model: PRESET.model, messages, ...(tools.length ? { tools, ...(toolChoice ? { tool_choice: toolChoice } : {}) } : {}), stream: false, max_tokens: maxOutputTokens, thinking: { type: "disabled" } }); } catch { fail("input_invalid"); }
 const inputBytes = bytes(body);
 if (inputBytes > INPUT_LIMIT) fail("input_limit");
 if (signal?.aborted) fail("cancelled");
 // Durable budget reservation and current-generation checks belong to this hook.
 const reservation = await beforeSend?.({ maxOutputTokens, inputBytes });
 const controller = new AbortController();
 const abort = () => controller.abort();
 signal?.addEventListener("abort", abort, { once: true });
 let timeout, result, sent = false;
 try {
  const timedOut = new Promise((_, reject) => { timeout = setTimeout(() => { controller.abort(); reject(new BrowserModelError("timeout")); }, TIMEOUT_MS); });
  result = await Promise.race([(async () => {
   // Re-read after durable reservation: Stop/Forget must invalidate this sending gate.
   const sendingKey = getCredential ? await getCredential(reservation) : key;
   if (!validKey(sendingKey)) fail("credential_invalid");
   if (signal?.aborted || controller.signal.aborted) fail("cancelled");
   sent = true;
   const response = await fetchImpl(PRESET.endpoint, { method: "POST", mode: "cors", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer", cache: "no-store", headers: { "Content-Type": "application/json", Authorization: `Bearer ${sendingKey}` }, body, signal: controller.signal });
   if (!response.ok) fail(response.status === 401 || response.status === 403 ? "authentication_failed" : response.status === 429 ? "rate_limited" : "provider_failed");
   return normalizeResponse(await readBounded(response), tools);
  })(), timedOut]);
 } catch (error) {
  if (sent) await afterResponse?.({ reservation, usage: null, status: "unknown" });
  if (error instanceof BrowserModelError) throw error;
  fail(signal?.aborted ? "cancelled" : "network_or_cors");
 } finally { clearTimeout(timeout); signal?.removeEventListener("abort", abort); controller.abort(); }
 await afterResponse?.({ reservation, usage: result.usage, status: "completed" });
 return result;
}
export async function probeBrowserModel(options = {}) {
 const tools = [{ type: "function", function: { name: "read_fixture", description: "Read a static connectivity fixture; no world access.", parameters: { type: "object", properties: {}, required: [], additionalProperties: false } } }];
 const messages = [{ role: "user", content: "Call read_fixture exactly once. After its result, answer exactly OK." }];
 const first = await callBrowserModel({ ...options, maxOutputTokens: 128, messages, tools, toolChoice: { type: "function", function: { name: "read_fixture" } } });
 const assistant = first.choices[0].message;
 if (assistant.tool_calls?.length !== 1) fail("probe_incompatible");
 const final = await callBrowserModel({ ...options, maxOutputTokens: 128, tools, toolChoice: "none", messages: [...messages, assistant, { role: "tool", tool_call_id: assistant.tool_calls[0].id, content: '{"fixture":"OK"}' }] });
 if (final.choices[0].finish_reason !== "stop" || final.choices[0].message.content?.trim() !== "OK") fail("probe_incompatible");
 return { provider_id: PRESET.provider_id, model: PRESET.model, protocol: PRESET.protocol, tool_round_trip: true };
}
