// Real WASM/canonical Rust interoperability. No provider requests or real World.
// Usage: node scripts/browser-game-protocol.integration.mjs <node-bindgen-dir> <fixture-binary>
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { createPrivateKey, createPublicKey, sign, webcrypto } from "node:crypto";
import { createBrowserGameClient } from "../software_safe_src/browser_game_client.js";
import { createBrowserGameRecovery } from "../software_safe_src/browser_game_recovery.js";
import { createBrowserAgentRunner } from "../software_safe_src/browser_agent_runner.js";
import { createBrowserParticipantEntry } from "../software_safe_src/browser_participant_entry.js";
import { createBrowserOwnerEntry } from "../software_safe_src/browser_owner_entry.js";

const wasm = createRequire(import.meta.url)(resolve(process.argv[2], "protocol.js"));
const codec = { signingBytes: wasm.world_signing_bytes, requestDigest: wasm.world_request_digest, verifyGatewayResponse: wasm.world_verify_gateway_response, verifyResponse: wasm.world_verify_response, validate: wasm.world_validate, build: wasm.world_build };
const processFixture = spawn(resolve(process.argv[3]), [], { stdio: ["pipe", "pipe", "pipe"] });
const pending = []; let stderr = "";
processFixture.stderr.on("data", (data) => { stderr += data; process.stderr.write(data); });
createInterface({ input: processFixture.stdout }).on("line", (line) => pending.shift()?.resolve(line));
processFixture.on("exit", (code) => { if (code) while (pending.length) pending.shift().reject(new Error(`fixture exited ${code}: ${stderr}`)); });
const rpc = (command) => new Promise((resolve, reject) => { pending.push({ resolve, reject }); processFixture.stdin.write(`${JSON.stringify(command)}\n`); });
const signer = (seed) => {
 const privateKey = createPrivateKey({ key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), Buffer.alloc(32, seed)]), format: "der", type: "pkcs8" });
 return { publicKeyHex: createPublicKey(privateKey).export({ format: "der", type: "spki" }).subarray(-32).toString("hex"), sign: async (bytes) => new Uint8Array(sign(null, bytes, privateKey)) };
};
try {
 const metadata = JSON.parse(await rpc({ kind: "init" }));
 const ownerSigner = signer(metadata.ownerSeed), executorSigner = signer(metadata.executorSeed);
 assert.equal(ownerSigner.publicKeyHex, metadata.control.owner_public_key);
 assert.equal(executorSigner.publicKeyHex, metadata.control.executor_binding.executor_public_key);
 const namespace = { service_id: "fixture", environment_id: "offline", instance_epoch: "1" };
 const register = { api_rev: 1, client_operation_id: "fixture-register", identity_namespace: namespace, account_id: "fixture-account", ...metadata.world, owner_public_key: ownerSigner.publicKeyHex, owner_key_version: "1" };
 const owner = { request: register, subject_public_key: ownerSigner.publicKeyHex, signature_hex: Buffer.from(await ownerSigner.sign(codec.signingBytes("game_participant_register_v1", JSON.stringify(register)))).toString("hex") };
 const admissionRaw = await rpc({ kind: "admission", owner });
 const admission = JSON.parse(admissionRaw);
 codec.validate("participant_admission", JSON.stringify({ owner, namespace, issuerPublicKey: metadata.servicePublicKey }), admissionRaw);
 const registrationCorrelation = JSON.parse(codec.build("correlation", JSON.stringify({ world: metadata.world, payload: { kind: "game_participant_register", payload: { owner, admission } } })));
 assert.equal(registrationCorrelation.key.verified_subject, ownerSigner.publicKeyHex);
 const stableAgent = JSON.parse(codec.build("participant_agent_id", JSON.stringify(register)));
 assert.match(stableAgent, /^participant:[a-f0-9]{64}$/);
 const goalRaw = codec.build("goal_prepare", JSON.stringify({ task_json: JSON.stringify(metadata.control), client_operation_id: "interop-owner-goal", client_nonce: "ab".repeat(32), content: { goal_text: "Prepare a workshop", soft_constraints: [] } }));
 const goalProofRaw = `{"request":${goalRaw},"subject_public_key":${JSON.stringify(ownerSigner.publicKeyHex)},"signature_hex":${JSON.stringify(Buffer.from(await ownerSigner.sign(codec.signingBytes("game_goal_prepare_private/v1", goalRaw))).toString("hex"))}}`;
 const goalGateway = await rpc({ kind: "goal_receipt", original: JSON.parse(goalProofRaw) });
 const goalReceiptRaw = codec.verifyGatewayResponse("/v1/world/private-goal/prepare", goalProofRaw, goalGateway, metadata.servicePublicKey);
 codec.validate("goal_prepared", goalProofRaw, goalReceiptRaw);
 const publicationRaw = codec.build("goal_publish", JSON.stringify({ original_json: goalProofRaw, receipt_json: goalReceiptRaw, client_operation_id: "interop-owner-publish" }));
 const publicationPayload = `{"kind":"game_goal_publish","payload":{"request":${publicationRaw},"subject_public_key":${JSON.stringify(ownerSigner.publicKeyHex)},"signature_hex":${JSON.stringify(Buffer.from(await ownerSigner.sign(codec.signingBytes("game_goal_publish/v1", publicationRaw))).toString("hex"))}}}`;
 const goalCorrelation = JSON.parse(codec.build("correlation", JSON.stringify({ world: metadata.world, payload_json: publicationPayload })));
 assert.equal(goalCorrelation.key.verified_subject, ownerSigner.publicKeyHex);

 const records = new Map();
 const storage = { get: async (key) => structuredClone(records.get(key)), update: async (key, mutate) => { const value = mutate(structuredClone(records.get(key))); records.set(key, structuredClone(value)); return structuredClone(value); } };
 const entryConfig = { endpoint: "/api/public/hosted-account/v2", world: metadata.world, servicePublicKey: metadata.servicePublicKey, participantIssuerPublicKey: metadata.servicePublicKey, identity: namespace };
 let emptyTask = false, ownerAccessCalls = 0;
 const entryFetch = async (url, options) => {
  const path = url.slice(entryConfig.endpoint.length);
  const response = (value) => new Response(JSON.stringify({ api_version: 2, ok: true, ...value }));
  if (path === "/entry/discovery") return response({ schema_revision: 1, identity: namespace, world: metadata.world, world_service_public_key: metadata.servicePublicKey, participant_policy: { policy: { issuer_public_key: metadata.servicePublicKey } }, phase_capabilities: { participant_recovery: true } });
  if (path === "/session") return response({ account: { account_id: "fixture-account" }, authentication: { state: "authenticated" }, csrf_token: "ab".repeat(32) });
  if (path === "/participant/authorize") return response({ phase: "participant_authorized", new_registration_enabled: true, admission: JSON.parse(await rpc({ kind: "admission", owner: JSON.parse(options.body) })) });
  if (path === "/participant/owner-view") return new Response(await rpc({ kind: emptyTask ? "owner_empty" : "gateway", path, rawBody: options.body }));
  if (path === "/entry-owner/control") { ownerAccessCalls += 1; return new Response(await rpc({ kind: "gateway", path, rawBody: options.body })); }
  throw new Error(`unexpected entry protocol route ${path}`);
 };
 const entryOptions = { config: entryConfig, codec, accountId: "fixture-account", ownerSigner, storage, locks: { request: async (_name, _options, callback) => callback() }, fetchImpl: entryFetch, cryptoImpl: webcrypto };
 await createBrowserParticipantEntry({ ...entryOptions, agentId: metadata.control.agent_id }).prepare();
 emptyTask = true;
 const ownerEntry = createBrowserOwnerEntry(entryOptions);
 assert.equal((await ownerEntry.inspect()).state, "needs_task");
 assert.equal((await ownerEntry.prepare({ goal: "Canonical read serialization fixture", consent: true })).state, "needs_task");
 assert(ownerAccessCalls > 0);
 ownerEntry.dispose();
 const fence = JSON.parse(codec.build("fence", JSON.stringify(metadata.control)));
 const recovery = createBrowserGameRecovery({ scope: { worldId: metadata.world.world_id, genesisDigest: metadata.world.genesis_digest, accountId: "fixture-account", agentId: fence.agent_id, taskId: fence.task_id, fence }, storage, locks: { request: async (_name, _options, callback) => callback() } });
 let effects = 0;
 const client = createBrowserGameClient({ config: { endpoint: "/api/public/hosted-account/v2", servicePublicKey: metadata.servicePublicKey, world: metadata.world }, codec, ownerSigner, executorSigner, participantProofs: async () => ({ owner, admission }), agentId: fence.agent_id, taskId: fence.task_id, recovery, cryptoImpl: webcrypto,
  gatewayCall: async (path, body, { rawBody, beforeSend } = {}) => { await beforeSend?.(); if (body.operation?.kind === "submit") effects += 1; return rpc({ kind: "gateway", path, rawBody }); },
 });
 const task = await client.readTask(); assert.deepEqual(task.fence, fence); await client.bindFence(fence);
 const prepared = await client.prepareContext("interop-direct", { runId: "interop" });
 const captured = await client.captureContext("interop-direct", prepared, { runId: "interop" });
 const receipt = await client.action("interop-direct", captured, { type: "MoveAgent", data: { agent_id: fence.agent_id, to: { x_cm: 1, y_cm: 2, z_cm: 3 } } }, { runId: "interop" });
 assert.equal(receipt.outcome.status, "committed");
 assert.equal(receipt.outcome.receipt.source, "lookup_executor");
 assert.equal(await client.hasUnresolved(), false);
 const runner = createBrowserAgentRunner({ gameClient: client, recovery, getCredential: async () => "fixture-unused-model-key", visibility: () => true, online: () => true,
  modelCall: async (options) => { const reservation = await options.beforeSend({ inputBytes: 500, maxOutputTokens: 512 }); await options.getCredential(reservation); await options.afterResponse({ reservation, usage: { prompt_tokens: 100, completion_tokens: 10 }, status: "completed" }); return { choices: [{ message: { role: "assistant", content: null, tool_calls: [{ id: "fixture-abort", type: "function", function: { name: "abort", arguments: "{}" } }] } }] }; },
 });
 const stopped = await runner.start({ confirmedFence: fence, runId: "interop-run" });
 assert.equal(stopped.code, "executor_aborted"); assert.equal(await client.hasUnresolved(), false);
 const originalEffects = effects; await client.recover(); assert.equal(effects, originalEffects);
 const loaded = await recovery.load(); assert.equal(loaded.operations.length, 6);
 assert(loaded.operations.every((entry) => entry.state === "committed"));
 assert.equal((await recovery.getRunBudget("interop-run")).callsReserved, 1);
 console.log("PASS real WASM + Rust typed fixture: participant admission/correlation/stable Agent, private goal prepare/publish proofs, real participant/owner access typed read signatures, owner discovery, signed prepare/prefix/lookup/capture/action, runner abort, durable lookup-only settlement, read-only recovery");
} finally { processFixture.stdin.end(); }
