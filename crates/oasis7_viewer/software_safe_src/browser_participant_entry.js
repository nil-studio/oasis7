import { createBrowserGameDatabase, browserIdentityScope } from "./browser_game_identity.js";

export class BrowserParticipantError extends Error {
  constructor(code) { super(code); this.name = "BrowserParticipantError"; this.code = code; }
}
const fail = (code) => { throw new BrowserParticipantError(code); };
const json = JSON.stringify;
const clone = (value) => JSON.parse(json(value));
const hex = (bytes) => Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
const DOMAIN = "game_participant_register_v1";
const OWNER_VIEW = "/v1/world/participant-owner-view";
const VIEW = "/v1/world/view-json";
const routes = new Set(["/participant/authorize", "/participant/submit", "/participant/lookup",
  "/participant/owner-view", "/entry-owner/control", "/entry-executor/control"]);

async function boundedText(response, maximum) {
  if (!response.body?.getReader) fail("participant_response_invalid");
  const reader = response.body.getReader(); const decoder = new TextDecoder("utf-8", { fatal: true });
  let text = "", size = 0;
  try {
    for (;;) { const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength; if (size > maximum) fail("participant_response_limit");
      text += decoder.decode(value, { stream: true }); }
    return text + decoder.decode();
  } finally { try { await reader.cancel(); } catch { /* no raw errors */ } }
}

// Same-origin authenticated transport shared by the participant and Game Client.
// Responses remain raw JSON until Rust has verified their signed numeric values.
export function createBrowserParticipantGateway({ config, accountId, fetchImpl = globalThis.fetch, signal } = {}) {
  if (config?.endpoint !== "/api/public/hosted-account/v2" || typeof accountId !== "string" || !accountId) fail("participant_configuration_invalid");
  async function request(path, options = {}, deadline = Date.now() + 30000, callSignal) {
    const signals = [signal, callSignal].filter(Boolean);
    if (signals.some((item) => item.aborted)) fail("participant_stopped");
    if (Date.now() >= deadline) fail("participant_request_timeout");
    const controller = new AbortController();
    let timer; const removers = [];
    const stopped = () => { if (signals.some((item) => item.aborted) || controller.signal.aborted) fail("participant_stopped"); };
    try {
      const maximum = path === "/entry-executor/control" ? 4 * 1024 * 1024 : 65536;
      if (options.body !== undefined && (typeof options.body !== "string"
        || new TextEncoder().encode(options.body).byteLength > maximum)) fail("participant_request_limit");
      const cancelled = new Promise((_resolve, reject) => {
        timer = setTimeout(() => { controller.abort(); reject(new BrowserParticipantError("participant_request_timeout")); }, Math.max(0, deadline - Date.now()));
        for (const parent of signals) {
          const abort = () => { controller.abort(); reject(new BrowserParticipantError("participant_stopped")); };
          parent.addEventListener("abort", abort, { once: true });
          removers.push(() => parent.removeEventListener("abort", abort));
          if (parent.aborted) abort();
        }
      });
      const operation = (async () => {
        stopped();
        const { beforeSend, ...fetchOptions } = options;
        if (beforeSend) { await beforeSend(); stopped(); }
        if (Date.now() >= deadline) fail("participant_request_timeout");
        const response = await fetchImpl(`${config.endpoint}${path}`, { credentials: "same-origin", mode: "same-origin",
          cache: "no-store", redirect: "error", referrerPolicy: "no-referrer", ...fetchOptions, signal: controller.signal });
        stopped();
        if (!response.ok) fail("participant_request_failed");
        const declared = response.headers?.get?.("content-length");
        if (declared !== null && declared !== undefined && (!/^[0-9]+$/.test(declared) || Number(declared) > maximum)) fail("participant_response_limit");
        const raw = await boundedText(response, maximum);
        stopped();
        const status = JSON.parse(raw);
        if (status.api_version !== 2 || status.ok !== true) fail("participant_request_failed");
        return raw;
      })();
      return await Promise.race([operation, cancelled]);
    } catch (error) { if (error instanceof BrowserParticipantError) throw error; fail("participant_response_unknown"); }
    finally { clearTimeout(timer); for (const remove of removers) remove(); }
  }
  return {
    discover: () => request("/entry/discovery"),
    async gatewayCall(path, body, { rawBody, beforeSend, signal: callSignal } = {}) {
      const deadline = Date.now() + 30000;
      if (!routes.has(path)) fail("participant_route_forbidden");
      if (signal?.aborted || callSignal?.aborted) fail("participant_stopped");
      if (beforeSend !== undefined && typeof beforeSend !== "function") fail("participant_request_invalid");
      let encodedBody;
      try { encodedBody = rawBody ?? json(body); } catch { fail("participant_request_invalid"); }
      if (typeof encodedBody !== "string") fail("participant_request_invalid");
      const maximum = path === "/entry-executor/control" ? 4 * 1024 * 1024 : 65536;
      if (new TextEncoder().encode(encodedBody).byteLength > maximum) fail("participant_request_limit");
      const session = JSON.parse(await request("/session", {}, deadline, callSignal));
      if (session.account?.account_id !== accountId || session.authentication?.state !== "authenticated"
        || !/^[0-9a-fA-F]{64}$/.test(session.csrf_token ?? "")) fail("participant_account_changed");
      if (rawBody !== undefined && typeof rawBody !== "string") fail("participant_request_invalid");
      return request(path, { method: "POST", headers: { "Content-Type": "application/json", "X-Oasis7-CSRF": session.csrf_token },
        body: encodedBody, beforeSend }, deadline, callSignal);
    },
  };
}

export function createBrowserParticipantEntry({ config, codec, ownerSigner, accountId, agentId,
  indexedDB, storage, locks = globalThis.navigator?.locks, fetchImpl = globalThis.fetch,
  cryptoImpl = globalThis.crypto, signal, isCurrent = () => true } = {}) {
  const namespace = config?.identity;
  if (!config?.world?.world_id || !config?.world?.genesis_digest
    || !/^[0-9a-f]{64}$/.test(config?.servicePublicKey ?? "")
    || !namespace?.service_id || !namespace?.environment_id || !/^[0-9]+$/.test(namespace?.instance_epoch ?? "")
    || !/^[0-9a-f]{64}$/.test(config?.participantIssuerPublicKey ?? "")
    || !/^[0-9a-f]{64}$/.test(ownerSigner?.publicKeyHex ?? "") || typeof ownerSigner?.sign !== "function"
    || typeof codec?.verifyGatewayResponse !== "function") fail("participant_configuration_invalid");
  const scopeKey = browserIdentityScope({ worldId: config.world.world_id, genesisDigest: config.world.genesis_digest, accountId, agentId: "participant-account" });
  const key = `participant:${scopeKey}:${ownerSigner.publicKeyHex}`;
  let canonicalAgentId = agentId;
  const active = () => { if (signal?.aborted || !isCurrent()) fail("participant_stopped"); };
  const database = storage ?? createBrowserGameDatabase(indexedDB);
  const transport = createBrowserParticipantGateway({ config, accountId, fetchImpl, signal });
  const decoded = (value) => typeof value === "string" ? JSON.parse(value) : value;
  function build(kind, value) {
    try { return codec.build(kind, json(value)); } catch { fail("participant_proof_invalid"); }
  }
  function field(raw, path) { return build("field", { json: raw, path }); }
  function validate(kind, request, raw) {
    try { return codec.validate(kind, typeof request === "string" ? request : json(request), raw); }
    catch { fail("participant_proof_invalid"); }
  }
  function check(record) {
    if (record === undefined) return null;
    if (record?.schema !== 1 || record.scopeKey !== scopeKey || record.ownerPublicKey !== ownerSigner.publicKeyHex
      || !record.owner || typeof record.sent !== "boolean") fail("participant_storage_corrupt");
    const r = record.owner.request;
    if (r?.account_id !== accountId || r.owner_public_key !== ownerSigner.publicKeyHex
      || r.world_id !== config.world.world_id || r.genesis_digest !== config.world.genesis_digest
      || r.identity_namespace?.service_id !== namespace.service_id || r.identity_namespace?.environment_id !== namespace.environment_id
      || r.identity_namespace?.instance_epoch !== namespace.instance_epoch
      || record.owner.subject_public_key !== ownerSigner.publicKeyHex) fail("participant_storage_corrupt");
    return record;
  }
  async function update(mutate) {
    if (typeof locks?.request !== "function") fail("participant_locks_unavailable");
    try { return await locks.request(`oasis7:${key}`, { mode: "exclusive" }, () => database.update(key, (stored) => mutate(check(stored)))); }
    catch (error) { if (error instanceof BrowserParticipantError) throw error; fail("participant_storage_failed"); }
  }
  async function load() {
    try { return check(await database.get(key)); }
    catch (error) { if (error instanceof BrowserParticipantError) throw error; fail("participant_storage_failed"); }
  }
  async function discover() {
    const discovery = JSON.parse(await transport.discover());
    if (discovery.schema_revision !== 1 || discovery.world?.world_id !== config.world.world_id
      || discovery.world?.genesis_digest !== config.world.genesis_digest
      || discovery.world_service_public_key !== config.servicePublicKey
      || discovery.identity?.service_id !== namespace.service_id || discovery.identity?.environment_id !== namespace.environment_id
      || discovery.identity?.instance_epoch !== namespace.instance_epoch
      || discovery.participant_policy?.policy?.issuer_public_key !== config.participantIssuerPublicKey
      || discovery.phase_capabilities?.participant_recovery !== true) fail("participant_discovery_mismatch");
    return discovery;
  }
  async function authorize(owner) {
    active();
    const raw = await transport.gatewayCall("/participant/authorize", owner);
    const reply = JSON.parse(raw);
    if (reply.phase !== "participant_authorized") fail("participant_authorization_failed");
    const admissionRaw = field(raw, ["admission"]);
    validate("participant_admission", { owner, namespace, issuerPublicKey: config.participantIssuerPublicKey }, admissionRaw);
    return { admission: decoded(admissionRaw), registrationEnabled: reply.new_registration_enabled === true };
  }
  async function proofs(record, authorized) {
    const readRequest = { contract_version: 1, observation_nonce: hex(cryptoImpl.getRandomValues(new Uint8Array(32))),
      world: clone(config.world), scope_id: `agent:${canonicalAgentId}`, min_commit: null,
      fixed_commit: null, deadline_unix_ms: null };
    active(); let read;
    try { read = { request: readRequest, subject_public_key: ownerSigner.publicKeyHex,
      signature_hex: hex(await ownerSigner.sign(codec.signingBytes(VIEW, json(readRequest)))) }; }
    catch { fail("participant_signing_failed"); }
    active(); const access = { owner: record.owner, admission: authorized.admission, read, response_profile: "owner_control_v1" };
    // Persist local proof material before the read. This is not a registration
    // submission or a settlement receipt; it grants no new participant binding.
    await update((current) => {
      if (!current || json(current.owner) !== json(record.owner)) fail("participant_original_conflict");
      current.admission = clone(authorized.admission); current.ownerRead = clone(access); return current;
    });
    const reply = await transport.gatewayCall("/participant/owner-view", access);
    let raw;
    try { raw = codec.verifyGatewayResponse(OWNER_VIEW, json(access), reply, config.servicePublicKey); }
    catch { fail("participant_binding_required"); }
    validate("owner_control", access, raw);
    if (decoded(field(raw, ["view", "canonical_agent_owner", "agent_id"])) !== canonicalAgentId
      || decoded(field(raw, ["view", "canonical_agent_owner", "public_key"])) !== ownerSigner.publicKeyHex) fail("participant_binding_required");
    await update((current) => {
      if (!current || json(current.owner) !== json(record.owner)) fail("participant_original_conflict");
      current.ownerControlRaw = raw; current.agentId = canonicalAgentId; return current;
    });
    return { owner: clone(record.owner), admission: authorized.admission, ownerControl: { rawJson: raw } };
  }
  async function ensureOwner() {
      let record = await load();
      if (!record) {
        const operation = `browser-participant-${hex(cryptoImpl.getRandomValues(new Uint8Array(16)))}`;
        const request = { api_rev: 1, client_operation_id: operation, identity_namespace: clone(namespace), account_id: accountId,
          world_id: config.world.world_id, genesis_digest: config.world.genesis_digest, owner_public_key: ownerSigner.publicKeyHex, owner_key_version: "1" };
        let owner;
        try { owner = { request, subject_public_key: ownerSigner.publicKeyHex,
          signature_hex: hex(await ownerSigner.sign(codec.signingBytes(DOMAIN, json(request)))) }; }
        catch { fail("participant_signing_failed"); }
        active(); record = await update((current) => current ?? { schema: 1, scopeKey, ownerPublicKey: ownerSigner.publicKeyHex, owner, sent: false });
      }
    if (!canonicalAgentId) {
      try { canonicalAgentId = decoded(codec.build("participant_agent_id", json(record.owner.request))); }
      catch { fail("participant_proof_invalid"); }
    }
    return record;
  }
  function verified(domain, requestRaw, responseRaw, kind, validationRequest) {
    let raw;
    try { raw = codec.verifyGatewayResponse(domain, requestRaw, responseRaw, config.servicePublicKey); }
    catch { fail("participant_response_unknown"); }
    return validate(kind, validationRequest, raw);
  }
  async function registrationLookup(record) {
    const originalRaw = record.registrationRaw;
    if (!originalRaw) fail("participant_original_missing");
    const payloadRaw = field(originalRaw, ["signed_payload"]);
    const correlationRaw = build("correlation", { world: config.world, payload_json: payloadRaw });
    const keyRaw = field(correlationRaw, ["key"]);
    const lookupRaw = `{"request":{"contract_version":1,"key":${keyRaw}},"original":${payloadRaw}}`;
    const response = await transport.gatewayCall("/participant/lookup", null, { rawBody: lookupRaw });
    const raw = verified("/v1/world/lookup", lookupRaw, response, "lookup", correlationRaw);
    const state = decoded(field(raw, ["outcome", "status"]));
    if (state !== "committed") {
      if (["rejected", "expired"].includes(state)) fail("participant_registration_rejected");
      fail("participant_registration_pending");
    }
    validate("participant_receipt", record.owner.request, raw);
    const receipt = decoded(field(raw, ["outcome", "receipt"]));
    if (receipt.owner_public_key !== ownerSigner.publicKeyHex || receipt.account_id !== accountId) fail("participant_owner_mismatch");
    canonicalAgentId = receipt.agent_id;
    await update((current) => {
      if (current.registrationRaw !== originalRaw) fail("participant_original_conflict");
      current.receiptRaw = raw; current.agentId = canonicalAgentId; return current;
    });
    return receipt;
  }
  async function inspect() {
    active(); await discover(); active(); const record = await ensureOwner();
    if (record.agentId) canonicalAgentId = record.agentId;
    if (record.registrationRaw) {
      try { await registrationLookup(record); }
      catch (error) { if (error.code === "participant_registration_pending") return { state: "pending", code: error.code }; throw error; }
    }
    try {
      const result = await proofs(record, await authorize(record.owner));
      return { state: "bound", agentId: canonicalAgentId, ...result };
    } catch (error) {
      if (error.code === "participant_binding_required" || error.code === "participant_request_failed" || error.code === "participant_response_unknown") return { state: "needs_participant", code: "participant_binding_unverified" };
      throw error;
    }
  }
  async function registerParticipant(consent) {
    if (consent !== true) fail("participant_consent_required");
    active(); await discover(); let record = await ensureOwner(); active();
    if (!record.registrationRaw) {
      const authorized = await authorize(record.owner);
      const payload = { kind: "game_participant_register", payload: { owner: record.owner, admission: authorized.admission } };
      const correlationRaw = build("correlation", { world: config.world, payload });
      const raw = `{"contract_version":1,"correlation":${correlationRaw},"deadline_unix_ms":null,"signed_payload":${json(payload)}}`;
      let created = false;
      record = await update((current) => { if (current.registrationRaw) return current; created = true; current.registrationRaw = raw; current.sent = true; return current; });
      // Persist exact owner, admission, operation identity and request before send.
      // A lost response is reconciled by lookup; it never creates another draft.
      try { if (created) {
        const response = await transport.gatewayCall("/participant/submit", null, { rawBody: record.registrationRaw, beforeSend: async () => active() });
        verified("/v1/world/submit", record.registrationRaw, response, "intent", field(record.registrationRaw, ["correlation"]));
      }
      } catch { /* retain original and query it below */ }
    }
    await registrationLookup(record); active();
    return proofs(await load(), await authorize(record.owner));
  }
  return {
    discover, inspect,
    registerParticipant,
    gatewayCall: transport.gatewayCall,
    async prepare({ existingOnly = true, consent } = {}) {
      // The existing-only Game Client consumer cannot create a participant.
      // Account entry explicitly opts into canonical registration and consent.
      if (!existingOnly) return registerParticipant(consent);
      await discover();
      const record = await ensureOwner();
      const authorized = await authorize(record.owner);
      return proofs(record, authorized);
    },
    async getParticipantProofs() {
      await discover(); const record = await load();
      if (record?.agentId) canonicalAgentId = record.agentId;
      if (!record?.owner) fail("participant_prepare_required");
      return proofs(record, await authorize(record.owner));
    },
  };
}
