import { createServer } from "node:http";
import { dirname, extname, normalize, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readRegularFile } from "./viewer-static-files.mjs";
import { viewerBuildPaths } from "./viewer-local-output.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const viewerRoot = resolve(scriptDir, "..");
const visualTestBundlePath = resolve(viewerBuildPaths("visual-test").bundleDir, "viewer.js");
const releaseBundlePath = resolve(viewerBuildPaths("release").bundleDir, "viewer.js");

function fakeTransportSource(endpoint) {
  return `(() => {
  const ENDPOINT = ${JSON.stringify(endpoint)};
  const state = { outbound: [], delivered: [], violations: [] };
  const listeners = new WeakMap();
  const sockets = [];
  let nextSocketId = 0;
  const emit = (socket, type, payload = {}) => {
    for (const callback of listeners.get(socket)?.[type] || []) callback({ type, ...payload });
  };
  const violation = (kind, url) => {
    state.violations.push({ kind, url: String(url), at: Date.now() });
  };
  const origin = () => location.origin;
  const sameOrigin = (url) => { try { return new URL(url, location.href).origin === origin(); } catch { return false; } };
  const nativeFetch = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const url = typeof input === "string" ? input : input?.url;
    if (!sameOrigin(url)) {
      violation("http", url);
      return Promise.reject(new Error("blocked by browser functional regression network guard: " + url));
    }
    return nativeFetch(input, init);
  };
  window.Worker = class BlockedWorker {
    constructor(url) { violation("worker", url); throw new Error("blocked by browser functional regression network guard: worker " + url); }
  };
  const NativeXhrOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function blockedOpen(method, url, ...rest) {
    if (!sameOrigin(url)) {
      violation("http", url);
      throw new Error("blocked by browser functional regression network guard: xhr " + url);
    }
    return NativeXhrOpen.call(this, method, url, ...rest);
  };
  window.EventSource = class BlockedEventSource {
    constructor(url) { violation("http", url); throw new Error("blocked by browser functional regression network guard: event source " + url); }
  };
  if (typeof navigator.sendBeacon === "function") {
    navigator.sendBeacon = (url) => {
      violation("http", url);
      return false;
    };
  }
  if (navigator.serviceWorker && typeof navigator.serviceWorker.register === "function") {
    navigator.serviceWorker.register = (url) => {
      violation("service-worker", url);
      return Promise.reject(new Error("blocked by browser functional regression network guard: service worker " + url));
    };
  }
  window.addEventListener("click", (event) => {
    const anchor = event.target && event.target.closest ? event.target.closest("a[href]") : null;
    if (!anchor) return;
    const href = anchor.getAttribute("href");
    if (!href || href.startsWith("#")) return;
    let url;
    try { url = new URL(href, location.href); } catch { return; }
    if (url.origin === origin()) return;
    event.preventDefault();
    violation("navigation", href);
  }, true);
  class FixtureWebSocket {
    static OPEN = 1;
    static CONNECTING = 0;
    static CLOSING = 2;
    static CLOSED = 3;
    constructor(url) {
      if (String(url) !== ENDPOINT) {
        violation("websocket", url);
        throw new Error("blocked by browser functional regression network guard: websocket " + url);
      }
      this.url = String(url);
      this.__bfrId = ++nextSocketId;
      this.readyState = FixtureWebSocket.CONNECTING;
      listeners.set(this, {});
      sockets.push(this);
      queueMicrotask(() => {
        if (this.readyState !== FixtureWebSocket.CONNECTING) return;
        this.readyState = FixtureWebSocket.OPEN;
        emit(this, "open");
      });
    }
    addEventListener(type, callback) { const table = listeners.get(this); table[type] ||= []; table[type].push(callback); }
    removeEventListener(type, callback) { const list = listeners.get(this)?.[type] || []; const index = list.indexOf(callback); if (index >= 0) list.splice(index, 1); }
    send(raw) { state.outbound.push({ index: state.outbound.length + 1, at: Date.now(), message: JSON.parse(raw) }); }
    close() { if (this.readyState === FixtureWebSocket.CLOSED) return; this.readyState = FixtureWebSocket.CLOSED; emit(this, "close"); }
    __bfrDeliver(message) { state.delivered.push({ index: state.delivered.length + 1, at: Date.now(), message }); emit(this, "message", { data: JSON.stringify(message) }); }
  }
  window.WebSocket = FixtureWebSocket;
  const currentSocket = () => sockets[sockets.length - 1] || null;
  const matches = (message, spec) => {
    if (!message) return false;
    if (spec.type && message.type !== spec.type) return false;
    if (spec.mode && (message.command?.mode) !== spec.mode) return false;
    if (spec.mode) {
      if (message.type === "prompt_control" && message.command?.mode !== spec.mode) return false;
      if (message.type === "authoritative_recovery" && message.command?.mode !== spec.mode) return false;
    }
    if (spec.innerMode && message.command?.request?.mode !== spec.innerMode) return false;
    return true;
  };
  window.__BFR__ = {
    origin: origin(),
    endpoint: ENDPOINT,
    snapshot: () => JSON.parse(JSON.stringify(state)),
    outbound: state.outbound,
    delivered: state.delivered,
    violations: state.violations,
    identity() {
      const value = window.__AW_TEST__?.getState?.() || {};
      return { playerId: value.authPlayerId, publicKey: value.authPublicKey };
    },
    nextOutbound(fromIndex, spec = {}, timeoutMs = 8000) {
      const start = performance.now();
      return new Promise((resolveResult) => {
        const poll = () => {
          for (let index = Math.max(0, fromIndex); index < state.outbound.length; index += 1) {
            if (matches(state.outbound[index].message, spec)) {
              resolveResult({ ok: true, cursor: index + 1, entry: state.outbound[index] });
              return;
            }
          }
          if (performance.now() - start > timeoutMs) {
            resolveResult({ ok: false, reason: "timeout", fromIndex, outboundCount: state.outbound.length });
            return;
          }
          setTimeout(poll, 20);
        };
        poll();
      });
    },
    countOutbound(spec = {}) { return state.outbound.filter((entry) => matches(entry.message, spec)).length; },
    outboundCount() { return state.outbound.length; },
    deliver(message, timeoutMs = 5000) {
      // The Viewer schedules its own reconnects (hello timeout, close), so a
      // delivery may briefly race a reconnect. Wait for the current fixture
      // socket instead of failing the scenario on a transport-internal retry.
      return new Promise((resolveDeliver, rejectDeliver) => {
        const started = performance.now();
        const attempt = () => {
          const socket = currentSocket();
          if (socket && socket.readyState === FixtureWebSocket.OPEN) {
            socket.__bfrDeliver(message);
            resolveDeliver(state.delivered.length);
            return;
          }
          if (performance.now() - started > timeoutMs) {
            rejectDeliver(new Error("no open fixture websocket"));
            return;
          }
          setTimeout(attempt, 25);
        };
        attempt();
      });
    },
    closeSocket() { const socket = currentSocket(); if (socket) socket.close(); return socket ? socket.__bfrId : null; },
    socketInfo() { return sockets.map((socket) => ({ id: socket.__bfrId, readyState: socket.readyState })); },
    currentSocketId() { return currentSocket()?.__bfrId ?? null; },
    deliverToSocket(socketId, message) {
      const socket = sockets.find((entry) => entry.__bfrId === socketId);
      if (!socket) return { ok: false, reason: "unknown fixture socket", socketId };
      state.delivered.push({ index: state.delivered.length + 1, at: Date.now(), message, socketId });
      emit(socket, "message", { data: JSON.stringify(message) });
      return { ok: true, delivered: state.delivered.length, socketId };
    },
    armSigningGate() {
      const signer = window.__BFR_SIGNER__;
      if (!signer || !signer.installed) return { installed: false, reason: signer?.reason || "deferred signer gate is unavailable" };
      return { installed: true, ...signer.arm() };
    },
    signingState() {
      const signer = window.__BFR_SIGNER__;
      if (!signer) return { installed: false };
      return { installed: signer.installed, ...signer.entered() };
    },
    releaseSigning() {
      const signer = window.__BFR_SIGNER__;
      if (!signer) return Promise.resolve({ released: false, reason: "deferred signer gate is unavailable" });
      return signer.release();
    },
    storageKeys() { return Object.keys(localStorage).sort(); },
    async networkGuardProbes() {
      const results = {};
      const attempt = async (kind, action) => {
        try { await action(); results[kind] = "not-blocked"; }
        catch (error) { results[kind] = String(error && error.message ? error.message : error); }
      };
      await attempt("http", () => fetch("https://example.invalid/bfr-http-probe"));
      await attempt("websocket", () => new WebSocket("wss://example.invalid/bfr-ws-probe"));
      await attempt("worker", () => new Worker("/bfr-unknown-worker.js"));
      const anchor = document.createElement("a");
      anchor.href = "https://example.invalid/bfr-navigation-probe";
      anchor.textContent = "bfr-probe";
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      return JSON.stringify({ results, violations: state.violations, location: location.href });
    },
  };
})();`;
}

// Test-only signing barrier. The real installed identity still produces the
// signature through WebCrypto; the gate only defers delivery of the next sign
// Promise so a scenario can fix one request in its signing stage. It never
// fabricates signature bytes and never changes the signed payload.
function deferredSignerSource() {
  return `(() => {
  const subtle = window.crypto && window.crypto.subtle;
  if (!subtle || typeof subtle.sign !== "function") {
    window.__BFR_SIGNER__ = { installed: false, reason: "SubtleCrypto.sign is unavailable" };
    return;
  }
  const nativeSign = subtle.sign.bind(subtle);
  const gate = { armed: false, entered: false, calls: 0, released: 0, pending: null };
  const wrapped = function deferredSign(algorithm, key, data) {
    gate.calls += 1;
    if (!gate.armed || gate.entered) return nativeSign(algorithm, key, data);
    gate.armed = false;
    gate.entered = true;
    return new Promise((resolveResult, rejectResult) => {
      gate.pending = { resolveResult, rejectResult, args: [algorithm, key, data] };
    });
  };
  let installed = false;
  try {
    Object.defineProperty(subtle, "sign", { value: wrapped, configurable: true });
    installed = subtle.sign === wrapped;
  } catch (_) {}
  if (!installed) {
    try {
      Object.defineProperty(Object.getPrototypeOf(subtle), "sign", { value: wrapped, configurable: true, writable: true });
      installed = subtle.sign === wrapped;
    } catch (_) {}
  }
  window.__BFR_SIGNER__ = {
    installed,
    arm() { gate.armed = true; gate.entered = false; return { calls: gate.calls }; },
    entered() { return { entered: gate.entered, calls: gate.calls, released: gate.released, deferred: Boolean(gate.pending) }; },
    release() {
      const pending = gate.pending;
      if (!pending) return Promise.resolve({ released: false, calls: gate.calls });
      gate.pending = null;
      gate.entered = false;
      return nativeSign(...pending.args).then(
        (signature) => { gate.released += 1; pending.resolveResult(signature); return { released: true, calls: gate.calls }; },
        (error) => { pending.rejectResult(error); return { released: false, calls: gate.calls, error: String(error) }; },
      );
    },
  };
})();`;
}

function deriveSource() {
  return `(() => {
  window.__BFR_DERIVE__ = (input) => {
    const snapshot = input?.snapshot || {};
    const model = snapshot.model || {};
    const agents = Object.values(model.agents || {}).map((agent, index) => ({
      id: agent.id,
      label: agent.name || agent.id,
      pos: agent.pos || { x_cm: 20000 + index * 15000, y_cm: 10000, z_cm: 0 },
      position_source: agent.pos ? "runtime_agent" : "location_derived",
    }));
    const locations = Object.values(model.locations || {}).map((location) => ({
      id: location.id,
      label: location.name || location.id,
      pos: location.pos || { x_cm: 0, y_cm: 0, z_cm: 0 },
      marker_role: "logic_anchor",
      marker_alpha: 0.32,
    }));
    const gameplay = input?.gameplay || snapshot.player_gameplay || {};
    return {
      world_bounds: snapshot.config?.space || { width_cm: 10000000, depth_cm: 5000000, height_cm: 1000000 },
      locations,
      agents,
      fragment_terrain: [],
      links: [],
      module_visual_entities: [],
      selection: input?.selectedKind && input?.selectedId ? { kind: input.selectedKind, id: input.selectedId } : null,
      goal_highlight: { title: gameplay.goal_title || "BFR fixture goal", objective: gameplay.objective || "BFR fixture objective" },
      blocker_highlight: null,
      visual_hotspots: [],
      commercial_surface: {
        objective: { title: "BFR fixture goal", detail: "BFR fixture objective", progress_percent: 10 },
        next_action: { label: "fixture", detail: "fixture", target_agent_id: input?.selectedId || null, execute_kind: "none" },
        active_agent_id: input?.selectedId || null,
        player_leverage: { state: "idle", label: "Idle", summary: "Fixture", detail: "Fixture" },
        action_receipt: { present: false, state: "waiting_for_intent", confidence: "none", title: "No receipt", summary: "No receipt", detail: "No receipt", target_agent_id: null },
        blocker: { label: null, detail: null },
        world_read: { agents: agents.length, routes: 0, fragments: 0, hotspots: 0 },
      },
    };
  };
})();`;
}

const canvasCompatSource = `(() => {
  const nativeGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function getContext(type, ...args) {
    if (type === "webgl2") return { __bfrProbeOnly: true };
    return nativeGetContext.call(this, type, ...args);
  };
})();`;

const bridgeModuleSource = `import { createPixelWorldBevyBridge } from "/software_safe_src/pixel_world_bevy_bridge.js";
export const PIXEL_WORLD_RUNTIME_SOURCE = "bfr_fixture_bridge";
export async function createPixelWorldBridge(options) { return createPixelWorldBevyBridge(options); }
export function derivePixelWorldRenderState(input) {
  if (!globalThis.__BFR_DERIVE__) throw new Error("BFR derive fixture is not installed");
  return globalThis.__BFR_DERIVE__(input);
}`;

function contentTypeFor(path) {
  const extension = extname(path);
  if (extension === ".html") return "text/html; charset=utf-8";
  if (extension === ".js" || extension === ".mjs") return "text/javascript; charset=utf-8";
  if (extension === ".css") return "text/css; charset=utf-8";
  if (extension === ".ico") return "image/x-icon";
  return "application/octet-stream";
}

function resolveStatic(viewerPath) {
  const normalized = normalize(viewerPath).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = resolve(viewerRoot, `.${normalized}`);
  const rel = relative(viewerRoot, filePath);
  if (!rel || rel.startsWith("..")) return null;
  return filePath;
}

export function createFixtureServer({ releaseOnly = false } = {}) {
  const server = createServer((request, response) => {
    let pathname;
    try {
      const url = new URL(request.url || "/", "http://127.0.0.1");
      pathname = decodeURIComponent(url.pathname === "/" ? "/viewer.html" : url.pathname);
    }
    catch { response.writeHead(400); response.end("bad request"); return; }
    const port = server.address().port;
    const wsEndpoint = `ws://127.0.0.1:${port}/viewer-ws`;
    const config = `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify({ deploymentMode: "trusted_local_only", viewerWsEndpoint: wsEndpoint })}</script>`;
    if (pathname === "/viewer.js") {
      const bundle = releaseOnly ? releaseBundlePath : visualTestBundlePath;
      let content;
      try { content = readRegularFile(bundle); }
      catch { response.writeHead(500); response.end("missing bundle"); return; }
      response.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" });
      response.end(content);
      return;
    }
    if (pathname === "/pixel-world-bridge/pixel_world_bridge.js") {
      response.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" });
      response.end(bridgeModuleSource);
      return;
    }
    if (pathname === "/viewer.html") {
      let html;
      try { html = readRegularFile(resolve(viewerRoot, "viewer.html")).toString("utf8"); }
      catch { response.writeHead(404); response.end("not found"); return; }
      const injected = releaseOnly
        ? `${config}`
        : `${config}<script>${canvasCompatSource}</script><script>${deriveSource()}</script><script>${deferredSignerSource()}</script><script>${fakeTransportSource(wsEndpoint)}</script>`;
      html = html.replace('<script type="module" src="./viewer.js"></script>', `${injected}<script type="module" src="./viewer.js"></script>`);
      response.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
      response.end(html);
      return;
    }
    const filePath = resolveStatic(pathname);
    if (!filePath) { response.writeHead(403); response.end("forbidden"); return; }
    try {
      const content = readRegularFile(filePath);
      response.writeHead(200, { "Content-Type": contentTypeFor(filePath), "Cache-Control": "no-store" });
      response.end(content);
    } catch {
      response.writeHead(404); response.end("not found");
    }
  });
  return new Promise((resolveServer) => {
    server.listen(0, "127.0.0.1", () => resolveServer({ server, port: server.address().port }));
  });
}

// ---- scenario data ------------------------------------------------------

export const emptySnapshot = {
  time: 1,
  config: { space: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 } },
  model: { agents: {}, locations: {} },
  player_gameplay: { stage_status: "active", available_actions: [] },
};

export function promptSnapshot({ version = 3, override = null, time = 7 } = {}) {
  return {
    time,
    config: { space: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 } },
    model: {
      agents: { "agent-0": { id: "agent-0", name: "Agent 0", location_id: "loc-0", pos: { x_cm: 2_400_000, y_cm: 1_800_000, z_cm: 0 }, resources: {} } },
      locations: { "loc-0": { id: "loc-0", name: "Factory Anchor", pos: { x_cm: 2_000_000, y_cm: 1_500_000, z_cm: 0 } } },
      agent_prompt_profiles: {
        "agent-0": {
          agent_id: "agent-0",
          version,
          updated_by: "bfr-fixture",
          system_prompt_override: override,
          short_term_goal_override: null,
          long_term_goal_override: null,
        },
      },
    },
    player_gameplay: { stage_status: "active", available_actions: [] },
  };
}

export const helloAck = {
  type: "hello_ack",
  server: "bfr-fixture",
  world_id: "bfr-world",
  version: 2,
  control_profile: "playback",
  capabilities: ["prompt_control_result_v1"],
  authority_epoch: "bfr-authority-1",
};

export const DRAFT_TEXT = "BFR fixture system prompt";
