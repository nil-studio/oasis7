import { describe, it, expect, vi } from "vitest";
import { createBrowserParticipantEntry, createBrowserParticipantGateway } from "./browser_participant_entry.js";

const config = { endpoint: "/api/public/hosted-account/v2", servicePublicKey: "11".repeat(32),
  world: { world_id: "world", genesis_digest: "genesis" }, identity: { service_id: "identity", environment_id: "test", instance_epoch: "1" },
  participantIssuerPublicKey: "22".repeat(32) };
function setup() {
  const records = new Map(); let committed = false; let wrongAgent = false; let badPin = false; let lookupState = "committed"; let registrationOwnerMismatch = false;
  const storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => {
    const next = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(next)); return structuredClone(next);
  } };
  let owner;
  const calls = [];
  const respond = (value) => new Response(JSON.stringify({ api_version: 2, ok: true, ...value }), { status: 200 });
  const fetchImpl = vi.fn(async (url, options) => {
    const path = url.slice(config.endpoint.length); calls.push({ path, options });
    if (path === "/entry/discovery") return respond({ schema_revision: 1, world: config.world,
      world_service_public_key: badPin ? "33".repeat(32) : config.servicePublicKey, identity: config.identity,
      participant_policy: { policy: { issuer_public_key: config.participantIssuerPublicKey } }, phase_capabilities: { participant_recovery: true } });
    if (path === "/session") return respond({ account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "ab".repeat(32) });
    const body = JSON.parse(options.body);
    if (path === "/participant/authorize") { owner = body;
      return respond({ phase: "participant_authorized", new_registration_enabled: true, admission: { request: { ...body.request, business_digest: "digest" }, signature_hex: "admission", subject_public_key: config.participantIssuerPublicKey } }); }
    if (path === "/participant/submit" || path === "/participant/lookup") {
      const original = path === "/participant/submit" ? body.signed_payload : body.original;
      expect(original.kind).toBe("game_participant_register");
      if (path === "/participant/submit" && !registrationOwnerMismatch) committed = true;
      return respond({ world_response: { correlation: { key: { id: "original-key" } }, outcome: {
        status: registrationOwnerMismatch ? "rejected" : lookupState,
        receipt: { account_id: "account", owner_public_key: owner.subject_public_key, agent_id: "agent" }
      } } });
    }
    if (path === "/participant/owner-view") {
      expect(body.read.request.scope_id).toBe("agent:agent");
      expect(body.response_profile).toBe("owner_control_v1");
      expect(body.read.request).not.toHaveProperty("formal_anchor");
      expect([...records.values()][0].ownerRead).toEqual(body);
      return respond({ world_response: committed ? { view: { canonical_agent_owner: {
        public_key: owner.subject_public_key, agent_id: wrongAgent ? "unexpected-agent" : "agent" } } } : null,
        upstream_read_error: committed ? null : "binding unavailable" });
    }
    throw new Error("unexpected route");
  });
  const codec = {
    signingBytes: vi.fn(() => new Uint8Array([1, 2, 3])),
    build: vi.fn((kind, raw) => { const input = JSON.parse(raw);
      if (kind === "correlation") {
        const payload = input.payload ?? JSON.parse(input.payload_json);
        expect(payload.kind).toBe("game_participant_register");
        expect(payload.payload.owner.request.owner_key_version).toBe("1");
        return JSON.stringify({ key: { id: "original-key" } });
      }
      if (kind === "participant_agent_id") return JSON.stringify("agent");
      if (kind === "field") return JSON.stringify(input.path.reduce((value, field) => value[field], JSON.parse(input.json)));
      throw new Error("unsupported build");
    }),
    validate: vi.fn((_kind, _request, raw) => raw),
    verifyGatewayResponse: vi.fn((_domain, _request, raw, key) => {
      expect(key).toBe(config.servicePublicKey);
      const reply = JSON.parse(raw); if (!reply.world_response || reply.upstream_read_error) throw new Error("read failed");
      return JSON.stringify(reply.world_response);
    }),
  };
  const ownerSigner = { publicKeyHex: "44".repeat(32), sign: vi.fn(async () => new Uint8Array(64)) };
  const create = (override = {}) => createBrowserParticipantEntry({ config, codec, ownerSigner, accountId: "account", agentId: "agent", storage,
    locks: { request: async (_key, _options, action) => action() }, fetchImpl, cryptoImpl: { getRandomValues: (bytes) => bytes.fill(7) }, ...override });
  return { create, storage, records, calls, codec, ownerSigner, fetchImpl,
    setLookup: (value) => { lookupState = value; }, setOwnerMismatch: () => { registrationOwnerMismatch = true; }, setCommitted: (value) => { committed = value; }, setWrongAgent: (value) => { wrongAgent = value; }, setBadPin: (value) => { badPin = value; } };
}
describe("browser participant entry", () => {
  it("requires explicit preparation and only restores an existing confirmed binding", async () => {
    const env = setup(); const entry = env.create();
    await expect(entry.getParticipantProofs()).rejects.toMatchObject({ code: "participant_prepare_required" });
    expect(env.ownerSigner.sign).not.toHaveBeenCalled();
    env.setCommitted(true);
    const result = await entry.prepare();
    expect(JSON.parse(result.ownerControl.rawJson).view.canonical_agent_owner.agent_id).toBe("agent");
    expect(result).not.toHaveProperty("receipt");
    expect(env.codec.signingBytes.mock.calls[0][0]).toBe("game_participant_register_v1");
    await env.create().getParticipantProofs();
    await env.create().prepare();
    expect(env.calls.filter(({ path }) => path === "/participant/submit")).toHaveLength(0);
    expect(env.codec.signingBytes.mock.calls.filter(([domain]) => domain === "game_participant_register_v1")).toHaveLength(1);
    expect(env.codec.validate.mock.calls.some(([kind]) => kind === "participant_admission")).toBe(true);
    expect(env.codec.validate.mock.calls.some(([kind]) => kind === "owner_control")).toBe(true);
  });
  it("retains the owner proof when canonical participant binding is missing without submitting", async () => {
    const env = setup();
    await expect(env.create().prepare()).rejects.toMatchObject({ code: "participant_binding_required" });
    const original = structuredClone([...env.records.values()][0].owner);
    await expect(env.create().prepare()).rejects.toMatchObject({ code: "participant_binding_required" });
    expect(env.calls.filter(({ path }) => path === "/participant/submit")).toHaveLength(0);
    expect([...env.records.values()][0].owner).toEqual(original);
    expect(env.calls.some(({ path }) => path === "/participant/lookup")).toBe(false);
    expect(env.codec.signingBytes.mock.calls.filter(([domain]) => domain === "game_participant_register_v1")).toHaveLength(1);
    await expect(env.create().prepare({ existingOnly: false })).rejects.toMatchObject({ code: "participant_consent_required" });
  });
  it("preserves old business operation identity while renewing read-only proofs", async () => {
    const env = setup(); env.setCommitted(true); await env.create().prepare();
    const stored = [...env.records.values()][0]; stored.sent = true;
    const operation = stored.owner.request.client_operation_id;
    env.setCommitted(false);
    await expect(env.create().prepare()).rejects.toMatchObject({ code: "participant_binding_required" });
    expect([...env.records.values()][0].owner.request.client_operation_id).toBe(operation);
    expect(env.calls.filter(({ path }) => path === "/participant/submit")).toHaveLength(0);
  });
  it("rejects discovery pin drift and selected Agent mismatch", async () => {
    const env = setup(); env.setBadPin(true);
    await expect(env.create().prepare()).rejects.toMatchObject({ code: "participant_discovery_mismatch" });
    expect(env.ownerSigner.sign).not.toHaveBeenCalled();
    env.setBadPin(false); env.setCommitted(true); env.setWrongAgent(true);
    await expect(env.create().prepare()).rejects.toMatchObject({ code: "participant_binding_required" });
    expect([...env.records.values()][0].receiptRaw).toBeUndefined();
  });
  it("fails closed before submitting on storage errors and missing locks", async () => {
    const env = setup();
    env.storage.update = async () => { throw new Error("private target must not leak"); };
    await expect(env.create().prepare()).rejects.toMatchObject({ code: "participant_storage_failed" });
    expect(env.calls.some(({ path }) => path === "/participant/submit")).toBe(false);
    await expect(env.create({ locks: {} }).prepare()).rejects.toMatchObject({ code: "participant_locks_unavailable" });
  });
  it("registers a fresh account explicitly and uses the canonical receipt Agent", async () => {
    const env = setup(); const entry = env.create({ agentId: undefined });
    expect(await entry.inspect()).toMatchObject({ state: "needs_participant" });
    expect(env.calls.some(({ path }) => path === "/participant/submit")).toBe(false);
    const result = await entry.prepare({ existingOnly: false, consent: true });
    expect(JSON.parse(result.ownerControl.rawJson).view.canonical_agent_owner.agent_id).toBe("agent");
    expect(env.calls.filter(({ path }) => path === "/participant/submit")).toHaveLength(1);
    expect(await env.create({ agentId: undefined }).inspect()).toMatchObject({ state: "bound", agentId: "agent" });
  });
  it("only looks up an unknown original registration on reload without submitting another operation", async () => {
    const env = setup(); env.setLookup("unknown");
    await expect(env.create({ agentId: undefined }).prepare({ existingOnly: false, consent: true })).rejects.toMatchObject({ code: "participant_registration_pending" });
    const original = [...env.records.values()][0].registrationRaw;
    expect(await env.create({ agentId: undefined }).inspect()).toMatchObject({ state: "pending" });
    await expect(env.create({ agentId: undefined }).prepare({ existingOnly: false, consent: true })).rejects.toMatchObject({ code: "participant_registration_pending" });
    expect(env.calls.filter(({ path }) => path === "/participant/submit")).toHaveLength(1);
    expect([...env.records.values()][0].registrationRaw).toBe(original);
    env.setLookup("committed");
    expect(await env.create({ agentId: undefined }).inspect()).toMatchObject({ state: "bound", agentId: "agent" });
  });
  it("never replaces an existing account's mismatched owner after canonical rejection", async () => {
    const env = setup(); env.setOwnerMismatch();
    await expect(env.create({ agentId: undefined }).prepare({ existingOnly: false, consent: true })).rejects.toMatchObject({ code: "participant_registration_rejected" });
    const original = [...env.records.values()][0].registrationRaw;
    await expect(env.create({ agentId: undefined }).prepare({ existingOnly: false, consent: true })).rejects.toMatchObject({ code: "participant_registration_rejected" });
    expect(env.calls.filter(({ path }) => path === "/participant/submit")).toHaveLength(1);
    expect([...env.records.values()][0].registrationRaw).toBe(original);
  });
  it("uses cookie+CSRF on fixed same-origin routes and preserves raw signed JSON", async () => {
    const rawReply = '{"api_version":2,"ok":true,"world_response":{"height":18446744073709551615}}';
    const fetchImpl = vi.fn(async (url) => url.endsWith("/session") ? new Response(JSON.stringify({ api_version: 2, ok: true,
      account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) })) : new Response(rawReply));
    const gateway = createBrowserParticipantGateway({ config, accountId: "account", fetchImpl });
    const rawBody = '{"nonce":18446744073709551615}';
    expect(await gateway.gatewayCall("/entry-owner/control", null, { rawBody })).toBe(rawReply);
    const options = fetchImpl.mock.calls[1][1];
    expect(options).toMatchObject({ credentials: "same-origin", mode: "same-origin", redirect: "error", body: rawBody });
    expect(options.headers["X-Oasis7-CSRF"]).toBe("aa".repeat(32));
    await expect(gateway.gatewayCall("https://untrusted.invalid", {})).rejects.toMatchObject({ code: "participant_route_forbidden" });
    await expect(createBrowserParticipantGateway({ config, accountId: "wrong", fetchImpl }).gatewayCall("/participant/authorize", {}))
      .rejects.toMatchObject({ code: "participant_account_changed" });
  });
  it("aborts pending session reads and ignores a late response without posting", async () => {
    const abort = new AbortController(); let resolveFetch;
    const fetchImpl = vi.fn(() => new Promise((resolve) => { resolveFetch = resolve; }));
    const gateway = createBrowserParticipantGateway({ config, accountId: "account", fetchImpl, signal: abort.signal });
    const result = expect(gateway.gatewayCall("/entry-owner/control", {})).rejects.toMatchObject({ code: "participant_stopped" });
    abort.abort(); await result;
    resolveFetch(new Response(JSON.stringify({ api_version: 2, ok: true, account: { account_id: "account" },
      authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) })));
    await Promise.resolve(); await Promise.resolve();
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][1].signal.aborted).toBe(true);
    await expect(gateway.gatewayCall("/entry-owner/control", {})).rejects.toMatchObject({ code: "participant_stopped" });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });
  it("bounds pending fetches to 30 seconds and never posts after a late session", async () => {
    vi.useFakeTimers();
    try {
      let resolveFetch;
      const fetchImpl = vi.fn(() => new Promise((resolve) => { resolveFetch = resolve; }));
      const gateway = createBrowserParticipantGateway({ config, accountId: "account", fetchImpl });
      const result = expect(gateway.gatewayCall("/entry-owner/control", {})).rejects.toMatchObject({ code: "participant_request_timeout" });
      await vi.advanceTimersByTimeAsync(30000); await result;
      resolveFetch(new Response(JSON.stringify({ api_version: 2, ok: true, account: { account_id: "account" },
        authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) })));
      await vi.advanceTimersByTimeAsync(1);
      expect(fetchImpl).toHaveBeenCalledTimes(1);
      expect(fetchImpl.mock.calls[0][1].signal.aborted).toBe(true);
    } finally { vi.useRealTimers(); }
  });
  it("shares one deadline across session fetch and the complete POST body read", async () => {
    vi.useFakeTimers();
    try {
      const session = JSON.stringify({ api_version: 2, ok: true, account: { account_id: "account" },
        authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) });
      const fetchImpl = vi.fn((url) => url.endsWith("/session")
        ? new Promise((resolve) => setTimeout(() => resolve(new Response(session)), 20000))
        : Promise.resolve(new Response(new ReadableStream({ start() { /* body never terminates */ } }))));
      const gateway = createBrowserParticipantGateway({ config, accountId: "account", fetchImpl });
      const result = expect(gateway.gatewayCall("/entry-owner/control", {})).rejects.toMatchObject({ code: "participant_request_timeout" });
      await vi.advanceTimersByTimeAsync(20000);
      expect(fetchImpl).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(10000); await result;
      expect(fetchImpl.mock.calls[1][1].signal.aborted).toBe(true);
    } finally { vi.useRealTimers(); }
  });
  it("limits UTF8 raw requests before fetching session and limits response bodies", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ api_version: 2, ok: true, data: "x".repeat(65536) })));
    const gateway = createBrowserParticipantGateway({ config, accountId: "account", fetchImpl });
    await expect(gateway.gatewayCall("/entry-owner/control", null, { rawBody: "😀".repeat(20000) }))
      .rejects.toMatchObject({ code: "participant_request_limit" });
    expect(fetchImpl).not.toHaveBeenCalled();
    await expect(gateway.discover()).rejects.toMatchObject({ code: "participant_response_limit" });
  });
  it("checks the mutation permit after CSRF and aborts its late callback before POST", async () => {
    const abort = new AbortController(); let release;
    const pending = new Promise((resolve) => { release = resolve; });
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ api_version: 2, ok: true,
      account: { account_id: "account" }, authentication: { state: "authenticated" }, csrf_token: "aa".repeat(32) })));
    const beforeSend = vi.fn(() => pending);
    const gateway = createBrowserParticipantGateway({ config, accountId: "account", fetchImpl });
    const result = expect(gateway.gatewayCall("/entry-owner/control", {}, { beforeSend, signal: abort.signal }))
      .rejects.toMatchObject({ code: "participant_stopped" });
    for (let i = 0; i < 30; i += 1) await Promise.resolve();
    expect(beforeSend).toHaveBeenCalledTimes(1); expect(fetchImpl).toHaveBeenCalledTimes(1);
    abort.abort(); await result;
    release(); for (let i = 0; i < 10; i += 1) await Promise.resolve();
    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });
});
