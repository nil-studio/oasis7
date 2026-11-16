import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve, extname } from "node:path";
import { spawn } from "node:child_process";
import assert from "node:assert/strict";
import { viewerBuildPaths } from "./viewer-local-output.mjs";

const root = resolve(import.meta.dirname, "..");
const buildPaths = viewerBuildPaths("release");
const session = `viewer-auth-security-${process.pid}`;
const browser = process.env.AGENT_BROWSER_BIN || "agent-browser";
const counters = { login: 0, refresh: 0, release: 0, register: 0 };
let publicKey, runtimeSocket;
function frame(message) {
  const body = Buffer.from(JSON.stringify(message));
  const header = body.length < 126 ? Buffer.from([0x81, body.length]) : Buffer.from([0x81, 126, body.length >> 8, body.length & 255]);
  return Buffer.concat([header, body]);
}
function send(message) { runtimeSocket.write(frame(message)); }
const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url, "http://localhost");
    let body = ""; for await (const chunk of req) body += chunk;
    const json = (value) => { res.setHeader("Content-Type", "application/json"); res.end(JSON.stringify(value)); };
    if (url.pathname.endsWith("/login/start")) return json({ ok: true, challenge: { challenge_id: "challenge", delivery_mode: "email" } });
    if (url.pathname.endsWith("/login/complete")) {
      counters.login++; publicKey = JSON.parse(body).public_key;
      return json({ ok: true, account: { hosted_account_id: "account" }, grant: { player_id: "alice", release_token: "issuer-token", registration_grant: "issuer-grant", device_session_id: "device" } });
    }
    if (url.pathname.endsWith("/refresh")) { counters.refresh++; publicKey = JSON.parse(body).public_key; return json({ ok: true, registration_grant: "refreshed-grant" }); }
    if (url.pathname.endsWith("/release")) { counters.release++; return json({ ok: true }); }
    if (url.pathname.startsWith("/api/")) return json({ ok: true, admission: {} });
    const config = `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify({ deploymentMode: "hosted_public_join", viewerWsEndpoint: `ws://127.0.0.1:${server.address().port}/runtime` })}</script>`;
    if (url.pathname === "/harness.html") {
      res.setHeader("Content-Type", "text/html");
      return res.end(`<!doctype html><head>${config}<script>window.__OASIS7_VISUAL_TEST__=true</script><script type="importmap">{"imports":{"solid-js/store":"/node_modules/solid-js/store/dist/store.js","solid-js":"/node_modules/solid-js/dist/solid.js"}}</script></head><body><script type="module">import * as core from '/software_safe_src/legacy_core.js'; await core.initializeSoftwareSafeCore(); window.securityCore=core; window.ready=true;</script></body>`);
    }
    if (url.pathname === "/release.html") {
      const html = await readFile(resolve(root, "viewer.html"), "utf8");
      res.setHeader("Content-Type", "text/html"); return res.end(html.replace("</head>", `${config}</head>`));
    }
    if (url.pathname === "/viewer.js") {
      res.setHeader("Content-Type", "text/javascript");
      return res.end(await readFile(resolve(buildPaths.bundleDir, "viewer.js")));
    }
    const file = resolve(root, `.${url.pathname}`);
    if (!file.startsWith(root + "/")) { res.writeHead(403); return res.end(); }
    res.setHeader("Content-Type", extname(file) === ".js" ? "text/javascript" : "application/octet-stream");
    res.end(await readFile(file));
  } catch { res.writeHead(500); res.end("mock failed"); }
});
server.on("upgrade", (req, socket) => {
  runtimeSocket = socket;
  const accept = createHash("sha1").update(req.headers["sec-websocket-key"] + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64");
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  let buffer = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length >= 2) {
      const opcode = buffer[0] & 15; const lengthMarker = buffer[1] & 127; let size = lengthMarker; let offset = 2;
      if (lengthMarker === 126) { if (buffer.length < 4) return; size = buffer.readUInt16BE(2); offset = 4; }
      if (lengthMarker === 127 || size > 65535) return socket.destroy();
      const masked = buffer[1] & 128; const mask = masked ? buffer.subarray(offset, offset + 4) : null; offset += masked ? 4 : 0;
      if (buffer.length < offset + size) return;
      const payload = Buffer.from(buffer.subarray(offset, offset + size)); buffer = buffer.subarray(offset + size);
      if (mask) for (let n = 0; n < payload.length; n++) payload[n] ^= mask[n % 4];
      if (opcode === 8) { socket.end(Buffer.from([0x88, 0])); return; }
      if (opcode !== 1) continue;
      const message = JSON.parse(payload.toString());
      if (message.type === "hello_v2") send({ type: "hello_ack", server: "mock", world_id: "world", version: 2, authority_epoch: "authority", capabilities: ["prompt_control_result_v1"] });
      if (message.command?.mode === "register_session") {
        const request = message.command.request; counters.register++;
        assert.equal(request.player_id, "alice"); assert.equal(request.public_key, publicKey); assert.ok(request.registration_grant); assert.ok(request.auth.signature);
        send({ type: "authoritative_recovery_ack", ack: { status: "session_registered", player_id: "alice", session_pubkey: publicKey, agent_id: "agent-0", session_epoch: 1, binding_epoch: 1 } });
      }
    }
  });
  socket.on("error", () => {});
});
function run(args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(browser, ["--session", session, "--json", ...args]); let out = "", err = "";
    child.stdout.on("data", c => { out += c; }); child.stderr.on("data", c => { err += c; });
    const timer = setTimeout(() => { child.kill(); reject(new Error("browser command timeout")); }, 30000);
    child.on("error", reject); child.on("close", code => { clearTimeout(timer); if (code) return reject(new Error(err || out)); const result = JSON.parse(out); if (!result.success) return reject(new Error(result.error)); resolveRun(result.data); });
  });
}
async function evaluate(code) { return (await run(["eval", code])).result; }
await new Promise(r => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;
try {
  await run(["open", `${base}/harness.html?test_api=1&connect=1&hosted_bootstrap=0`]);
  await evaluate(`(async()=>{for(let n=0;n<100;n++){if(window.ready&&securityCore.state.connectionStatus==='connected')return true;await new Promise(r=>setTimeout(r,20));}throw Error('core not ready')})()`);
  const login = await evaluate(`(async()=>{securityCore.state.hostedLogin.handle='alice@example.test';await securityCore.startHostedAccountLogin();securityCore.state.hostedLogin.code='123456';await securityCore.completeHostedAccountLogin();await securityCore.registerPlayerSessionForTest('agent-0');return {player:securityCore.state.auth.playerId,registered:securityCore.state.auth.registrationStatus,secretFields:['privateKey','releaseToken','registrationGrant'].filter(k=>securityCore.state.auth[k]!=null)}})()`);
  assert.equal(login.player, "alice"); assert.equal(login.registered, "registered"); assert.deepEqual(login.secretFields, []); assert.equal(counters.register, 1);
  await evaluate(`new Promise(r=>setTimeout(r,100))`);
  const before = { ...counters };
  const firstKey = publicKey;
  send({ type: "authoritative_recovery_ack", ack: { status: "session_revoked" } });
  await evaluate(`(async()=>{for(let n=0;n<100;n++){if(securityCore.state.connectionStatus==='error')return true;await new Promise(r=>setTimeout(r,20));}throw Error('identity-less revoke did not close connection')})()`);
  assert.equal(await evaluate(`securityCore.state.auth.playerId`), "alice");
  assert.deepEqual(counters, before);
  await run(["open", `${base}/harness.html?test_api=1&connect=1&hosted_bootstrap=0`]);
  await evaluate(`(async()=>{for(let n=0;n<100;n++){if(window.ready&&securityCore.state.connectionStatus==='connected')return true;await new Promise(r=>setTimeout(r,20));}throw Error('reload core not ready')})()`);
  const restored = await evaluate(`(async()=>{await securityCore.registerPlayerSessionForTest('agent-0');return {player:securityCore.state.auth.playerId,key:securityCore.state.auth.publicKey}})()`);
  assert.equal(restored.player, "alice"); assert.notEqual(restored.key, firstKey);
  await evaluate(`new Promise(r=>setTimeout(r,100))`);
  const beforeConflict = { ...counters };
  send({ type: "authoritative_recovery_ack", ack: { status: "catch_up_ready", player_id: "mallory", session_pubkey: "00".repeat(32), session_epoch: 99 } });
  await evaluate(`(async()=>{for(let n=0;n<100;n++){if(securityCore.state.connectionStatus==='error')return true;await new Promise(r=>setTimeout(r,20));}throw Error('conflict did not close connection')})()`);
  const conflict = await evaluate(`({player:securityCore.state.auth.playerId,publicKey:securityCore.state.auth.publicKey,epoch:securityCore.state.auth.sessionEpoch})`);
  assert.equal(conflict.player, "alice"); assert.equal(conflict.publicKey, publicKey); assert.equal(conflict.epoch, 1);
  assert.deepEqual(counters, beforeConflict);
  await evaluate(`localStorage.clear()`);
  await run(["open", `${base}/harness.html?test_api=1&connect=1&hosted_bootstrap=0`]);
  await evaluate(`(async()=>{for(let n=0;n<100;n++){if(window.ready&&securityCore.state.connectionStatus==='connected')return true;await new Promise(r=>setTimeout(r,20));}throw Error('guest core not ready')})()`);
  send({ type: "gameplay_action_ack", ack: { player_id: "mallory", action_id: "unrequested", target_agent_id: "agent-0" } });
  await evaluate(`(async()=>{for(let n=0;n<100;n++){if(securityCore.state.connectionStatus==='error')return true;await new Promise(r=>setTimeout(r,20));}throw Error('guest conflict did not close connection')})()`);
  assert.equal(await evaluate(`securityCore.state.auth.available`), false); assert.deepEqual(counters, beforeConflict);
  await run(["open", `${base}/release.html?test_api=1&connect=0&hosted_bootstrap=0&viewer_visual_fixture=agent_chat_history&pixel_world_visual_fixture=selected_blocker&fixture=refine_quote_preflight&auto_mount=1`]);
  const release = await evaluate(`({testApi:typeof window.__AW_TEST__,viewerFixture:typeof window.__OASIS7_VIEWER_VISUAL_FIXTURES__,pixelFixture:typeof window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__,fixture:document.body.dataset.viewerVisualFixture||null})`);
  assert.deepEqual(release, { testApi: "undefined", viewerFixture: "undefined", pixelFixture: "undefined", fixture: null });
  console.log("viewer auth browser security smoke: passed (real Ed25519, issuer/WS registration, reload key recovery, malformed revoke and guest/identity conflict zero side effects, release query isolation)");
  if (buildPaths.local) console.log(JSON.stringify({
    expected_count: 1, count_ok: true, passed: true,
    case_ids: ["viewer-auth-browser-security-smoke"],
  }));
} catch (error) {
  console.error(await evaluate(`({ready:window.ready,core:typeof window.securityCore,error:window.securityCore?.state.lastError,status:window.securityCore?.state.connectionStatus})`).catch(()=>null));
  console.error(await run(["errors"]).catch(()=>null));
  throw error;
} finally {
  await run(["close"]).catch(() => {}); runtimeSocket?.destroy(); server.closeAllConnections(); await new Promise(r => server.close(r));
}
