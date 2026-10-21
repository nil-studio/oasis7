const API = "/v1/agent-client";
const IDENTITY = "/api/public/hosted-account/v2/capabilities";
const CODES = new Set(["authentication_required", "authentication_failed", "quota_unavailable", "budget_exhausted", "rate_limited", "model_unavailable", "capability_unsupported", "transport_failed", "session_expired", "version_conflict", "executor_unavailable", "executor_busy", "game_executor_unavailable", "deployment_unavailable", "untrusted_executor", "invalid_request", "request_failed", "input_limit", "output_limit", "response_limit", "model_identity_mismatch", "unexpected_streaming", "unsupported_finish_reason", "unsupported_tool", "invalid_tool_identity", "unsupported_content", "invalid_response", "list_changed", "connection_capacity_exceeded"]);
export class ModelConnectionError extends Error {
  constructor(code, status = 0, operationState = null) { super(CODES.has(code) ? code : "request_failed"); this.code = this.message; this.status = status; this.operationState = ["pending", "ready", "failed", "not_started"].includes(operationState) ? operationState : null; }
}
const text = (value, max = 256) => typeof value === "string" && value.length <= max ? value : "";
const number = (value) => Number.isSafeInteger(value) && value >= 0 ? value : null;
const RUN_REASONS = new Set(["completed", "budget_exhausted", "pending_original", "rejected_original", "expired_original", "control_changed", "context_expired", "model_failed", "input_limit", "model_timeout", "invalid_model_output", "aborted", "host_failure"]);
const MODEL_FAILURES = new Set(["input_limit", "output_limit", "model_identity_mismatch", "unexpected_streaming", "unsupported_finish_reason", "unsupported_tool", "invalid_tool_identity", "unsupported_content", "capability_unsupported", "other"]);
function executionSnapshot(value) {
  if (!value || !["running", "finished", "stopped"].includes(value.phase)) return null;
  return { phase: value.phase, reason: RUN_REASONS.has(value.reason) ? value.reason : null, model_failure: MODEL_FAILURES.has(value.model_failure) ? value.model_failure : null, successful_effects: number(value.successful_effects), model_calls_spent: number(value.model_calls_spent) };
}
export function modelConnectionSnapshot(value) {
  if (!value || !/^[a-zA-Z0-9_-]{1,128}$/.test(value.id || "") || !Number.isSafeInteger(value.version) || value.version < 1) throw new ModelConnectionError("request_failed");
  return { execution: executionSnapshot(value.execution), id: value.id, version: value.version, preset_id: text(value.preset_id), state: ["checking", "ready", "blocked", "revoked", "expired"].includes(value.state) ? value.state : "unknown", run_state: ["running", "stopped", "idle", "unavailable"].includes(value.run_state) ? value.run_state : "unknown", run_generation: number(value.run_generation), expires_at_ms: number(value.expires_at_ms), absolute_expires_at_ms: number(value.absolute_expires_at_ms), calls_spent: number(value.calls_spent), tokens_reserved: number(value.tokens_reserved), error_code: CODES.has(value.error_code) ? value.error_code : value.error_code ? "request_failed" : "" };
}
export function acceptsModelCredentials(capabilities, origin = globalThis.location?.origin) {
  const executor = capabilities?.trusted_executor;
  return capabilities?.ok === true && capabilities.enabled === true && capabilities.authenticated === true && executor?.trusted === true && executor.origin === origin && !!executor.display_name && !!executor.location && capabilities.presets?.length > 0 && ["max_calls", "max_input_tokens", "max_output_tokens", "max_concurrent", "lease_ms", "absolute_ms", "renew_interval_ms"].every((key) => Number.isSafeInteger(capabilities.limits?.[key]) && capabilities.limits[key] > 0);
}
export function createViewerModelConnectionClient({ fetch: fetchImpl = globalThis.fetch, operationId = () => globalThis.crypto.randomUUID() } = {}) {
  const pending = new Map();
  async function request(path, body, method = body === undefined ? "GET" : "POST", identity = false) {
    let csrf;
    if (body !== undefined) {
      const capabilities = await request(IDENTITY, undefined, "GET", true);
      csrf = text(capabilities.csrf_token, 4096);
      if (!csrf) throw new ModelConnectionError("authentication_required");
    }
    let response;
    try {
      response = await fetchImpl(identity ? path : API + path, { method, credentials: "same-origin", mode: "same-origin", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer", headers: { Accept: "application/json", ...(body === undefined ? {} : { "Content-Type": "application/json", "X-Oasis7-CSRF": csrf }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    } catch { throw new ModelConnectionError("transport_failed"); }
    let value;
    try { value = await response.json(); } catch { throw new ModelConnectionError("request_failed", response.status); }
    if (!response.ok || value?.ok === false) throw new ModelConnectionError(CODES.has(value?.error_code) ? value.error_code : response.status === 401 ? "authentication_required" : "request_failed", response.status, value?.operation_state);
    return value;
  }
  const path = (connection) => {
    if (!/^[a-zA-Z0-9_-]{1,128}$/.test(connection?.id || "") || !Number.isSafeInteger(connection.version) || connection.version < 1) throw new ModelConnectionError("invalid_request");
    return `/model-connections/${encodeURIComponent(connection.id)}`;
  };
  async function mutate(connection, action, extra = {}, method = "POST") {
    const key = `${connection.id}:${action}`;
    const needsId = action === "probe" || action === "stop";
    if (needsId && !pending.has(key)) pending.set(key, { id: operationId(), version: connection.version });
    const operation = pending.get(key);
    const complete = () => { if (pending.get(key) === operation) pending.delete(key); };
    try {
      const value = await request(path(connection) + (action ? `/${action}` : ""), { expected_version: needsId ? operation.version : connection.version, ...extra, ...(needsId ? { operation_id: operation.id } : {}) }, method);
      const result = modelConnectionSnapshot(value.connection);
      if (value.operation_state !== "pending") complete();
      return result;
    } catch (failure) {
      // HTTP status alone does not prove that the original operation ended.
      if (["ready", "failed", "not_started"].includes(failure.operationState)) complete();
      throw failure;
    }
  }
  return {
    async capabilities() {
      const value = await request("/capabilities");
      const executor = value.trusted_executor;
      const limits = Object.fromEntries(["max_calls", "max_tokens", "max_input_tokens", "max_output_tokens", "max_concurrent", "lease_ms", "absolute_ms", "renew_interval_ms"].map((key) => [key, number(value.limits?.[key])]));
      return { ok: value.ok === true, enabled: value.enabled === true, authenticated: value.authenticated === true, executor_available: value.executor_available === true, reason: CODES.has(value.reason) ? value.reason : value.reason ? "request_failed" : "", trusted_executor: executor ? { trusted: executor.trusted === true, display_name: text(executor.display_name), location: text(executor.location), origin: text(executor.origin, 2048) } : null, limits, presets: Array.isArray(value.presets) ? value.presets.slice(0, 64).filter((preset) => preset && /^[a-zA-Z0-9_.:-]{1,128}$/.test(preset.id || "") && ["deepseek", "openrouter"].includes(preset.provider_id)).map((preset) => ({ id: preset.id, provider_id: preset.provider_id, model_id: text(preset.model_id), protocol: text(preset.protocol, 64) })) : [] };
    },
    async read(connection) { const value = await request(path(connection)); return modelConnectionSnapshot(value.connection); },
    async list() {
      const items = [], ids = new Set(), cursors = new Set();
      let cursor = null, cut;
      do {
        const value = await request(cursor ? `/model-connections/pages/${encodeURIComponent(cursor)}` : "/model-connections");
        if (value.page_limit !== 64 || !Array.isArray(value.connections) || value.connections.length > 64 || !text(value.list_cut, 128) || (cut && cut !== value.list_cut) || !(value.next_cursor === null || /^[a-zA-Z0-9_-]{1,256}$/.test(value.next_cursor || ""))) throw new ModelConnectionError("request_failed");
        cut = value.list_cut;
        for (const raw of value.connections) {
          const connection = modelConnectionSnapshot(raw);
          if (ids.has(connection.id) || items.length >= 4096) throw new ModelConnectionError("request_failed");
          ids.add(connection.id); items.push(connection);
        }
        cursor = value.next_cursor;
        if (cursor && (cursors.has(cursor) || value.connections.length === 0)) throw new ModelConnectionError("request_failed");
        cursors.add(cursor);
      } while (cursor);
      return items;
    },
    async create({ preset_id, raw_key, consent }) { try { const value = await request("/model-connections", { preset_id, raw_key, consent: consent === true }); return modelConnectionSnapshot(value.connection); } finally { raw_key = ""; } },
    probe(connection) { return mutate(connection, "probe", { consent: true }); },
    activate(connection, { target_ref, limits }) { return mutate(connection, "activate", { consent: true, target_ref, limits }); },
    lease(connection) { return mutate(connection, "lease"); },
    stop(connection) { return mutate(connection, "stop"); },
    forget(connection) { return mutate(connection, "", {}, "DELETE"); },
  };
}
