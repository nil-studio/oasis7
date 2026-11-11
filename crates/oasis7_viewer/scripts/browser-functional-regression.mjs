#!/usr/bin/env node
// Browser functional regression (fixture-functional) runner for issue #5387.
//
// This is a test-only entry. It loads the same-candidate visual-test Viewer
// bundle (real viewer_app.jsx / legacy_core.js / PromptControl + feedback
// modules) into a real Chromium via agent-browser and replaces only the
// transport, auth lane, render projection and static asset origin. Scenarios
// deliver versioned messages through bounded barriers; assertions are
// independent of the modules under test. It never starts a runtime, provider
// or chain, and it never installs a controller in the release bundle.
import { createServer } from "node:http";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { accessSync, constants, existsSync, mkdirSync, statSync, writeFileSync } from "node:fs";
import { delimiter, dirname, extname, join, normalize, relative, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";
import { fileFingerprint as fingerprintRegularFile, readRegularFile } from "./viewer-static-files.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const viewerRoot = resolve(scriptDir, "..");
const repoRoot = resolve(viewerRoot, "../..");
const runStamp = new Date().toISOString().replace(/[:.]/g, "-");
const defaultOutDir = resolve(repoRoot, "output/playwright/browser-functional-regression", runStamp);
const visualTestBundlePath = resolve(viewerRoot, ".software-safe-test-build/viewer.js");
const releaseBundlePath = resolve(viewerRoot, ".software-safe-build/viewer.js");
const EXPECTED_VIEWPORTS = {
  desktop: { width: 1440, height: 1000 },
  narrow: { width: 390, height: 844 },
};
const ALL_CASES = [
  "p01-loading",
  "p01-empty",
  "p02-unready-no-send",
  "p02-success",
  "p03-rejected",
  "p03-stale",
  "p03-control-lost",
  "p03-control-lost-unknown",
  "p04-signing-deferred",
  "p04-sent-late-error",
  "p05-duplicate-out-of-order",
  "p06-reconnect-recovery",
  "p07-unmount-remount",
];
const SELECTABLE_CASES = [...ALL_CASES];
const PRODUCTION_MODULES = [
  "crates/oasis7_viewer/software_safe_src/viewer_app.jsx",
  "crates/oasis7_viewer/software_safe_src/legacy_core.js",
  "crates/oasis7_viewer/software_safe_src/viewer_prompt_control_module.js",
  "crates/oasis7_viewer/software_safe_src/viewer_prompt_feedback_module.js",
  "crates/oasis7_viewer/software_safe_src/viewer_feedback_module.js",
  "crates/oasis7_viewer/software_safe_src/viewer_auth_session_module.js",
  "crates/oasis7_viewer/software_safe_src/pixel_world_bevy_bridge.js",
  "crates/oasis7_viewer/viewer.html",
];

function usage() {
  console.log(`Usage: node crates/oasis7_viewer/scripts/browser-functional-regression.mjs [options]

Options:
  --out-dir <path>       Artifact directory (default: output/playwright/browser-functional-regression/<run>)
  --cases <a,b>          Comma-separated case ids (default: all of ${ALL_CASES.join(",")})
  --viewports <a,b>      desktop,narrow (default: both)
  --skip-build           Reuse existing bundles
  --headed               Run headed Chromium
  --chrome-trace         Also archive a native Chrome DevTools trace per case (large)
  --self-check           Run runner negative checks (no case matrix execution)
  --json                 Print the machine-readable summary only
  --help                 Show this help`);
}

function parseArgs(argv) {
  const options = {
    outDir: defaultOutDir,
    cases: [...ALL_CASES],
    viewports: Object.keys(EXPECTED_VIEWPORTS),
    skipBuild: false,
    headed: false,
    chromeTrace: false,
    selfCheck: false,
    json: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case "--out-dir": options.outDir = resolve(argv[++index] || ""); break;
      case "--cases": options.cases = selectCases(String(argv[++index] || "").split(",")); break;
      case "--viewports": options.viewports = selectViewports(String(argv[++index] || "").split(",")); break;
      case "--skip-build": options.skipBuild = true; break;
      case "--headed": options.headed = true; break;
      case "--chrome-trace": options.chromeTrace = true; break;
      case "--self-check": options.selfCheck = true; break;
      case "--json": options.json = true; break;
      case "--help":
      case "-h": usage(); process.exit(0); break;
      default: throw new Error(`unknown option: ${arg}`);
    }
  }
  return options;
}

function rejectDuplicateSelection(selected, kind) {
  const seen = new Set();
  for (const value of selected) {
    if (seen.has(value)) throw new Error(`duplicate ${kind} selection: ${value}; duplicates would inflate the expected run count without widening coverage`);
    seen.add(value);
  }
}

export function selectCases(raw) {
  const selected = raw.map((value) => value.trim()).filter(Boolean);
  if (selected.length === 0) throw new Error("case selection is empty; a run must select at least one case");
  for (const caseId of selected) {
    if (!SELECTABLE_CASES.includes(caseId)) throw new Error(`unknown case id: ${caseId}`);
  }
  rejectDuplicateSelection(selected, "case");
  return selected;
}

export function selectViewports(raw) {
  const selected = raw.map((value) => value.trim()).filter(Boolean);
  if (selected.length === 0) throw new Error("viewport selection is empty; a run must select at least one viewport");
  for (const name of selected) {
    if (!EXPECTED_VIEWPORTS[name]) throw new Error(`unknown viewport: ${name}`);
  }
  rejectDuplicateSelection(selected, "viewport");
  return selected;
}

export function fileFingerprint(relPath) {
  const absPath = resolve(repoRoot, relPath);
  try {
    return { path: relPath, ...fingerprintRegularFile(absPath) };
  } catch (error) {
    if (error.code === "ENOENT" || error.code === "ENOTDIR") return { path: relPath, missing: true };
    throw error;
  }
}

function assert(condition, message, details = undefined) {
  if (!condition) {
    const suffix = details === undefined ? "" : `\n${JSON.stringify(details, null, 2)}`;
    throw new Error(`${message}${suffix}`);
  }
}

function runBlocking(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd || repoRoot,
    encoding: "utf8",
    timeout: options.timeout || 10_000,
    env: options.env || process.env,
  });
  return {
    command: [command, ...args].join(" "),
    status: result.status,
    stdout: result.stdout || "",
    stderr: result.stderr || "",
    error: result.error ? String(result.error) : null,
  };
}

// ---- page fixture -------------------------------------------------------

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

const emptySnapshot = {
  time: 1,
  config: { space: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 } },
  model: { agents: {}, locations: {} },
  player_gameplay: { stage_status: "active", available_actions: [] },
};

function promptSnapshot({ version = 3, override = null, time = 7 } = {}) {
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

const helloAck = {
  type: "hello_ack",
  server: "bfr-fixture",
  world_id: "bfr-world",
  version: 2,
  control_profile: "playback",
  capabilities: ["prompt_control_result_v1"],
  authority_epoch: "bfr-authority-1",
};

const DRAFT_TEXT = "BFR fixture system prompt";

// ---- agent-browser driver ----------------------------------------------

function createBrowserDriver({ sessionPrefix, headed, outDir }) {
  const binary = resolveBrowserBinary(process.env.AGENT_BROWSER_BIN || "agent-browser");
  // agent-browser derives a Unix socket path from the calling worktree; the
  // deep task worktree path can exceed the platform socket-name budget, so
  // keep the run-scoped socket directory at a short root path. Prefer the
  // short /tmp root when it exists; macOS TMPDIR is far too long.
  const shortRoot = existsSync("/tmp") ? "/tmp" : (process.env.TMPDIR || tmpdir());
  const socketDir = process.env.AGENT_BROWSER_SOCKET_DIR || join(shortRoot, "oasis7-bfr-sockets");
  mkdirSync(socketDir, { recursive: true });
  const session = `${sessionPrefix}-${process.pid}`.replace(/[^A-Za-z0-9._-]/g, "-").slice(0, 40);
  const lifecycle = createOwnedSessionLifecycle({
    command: binary,
    session,
    spawnSyncImpl: (command, args, options = {}) => spawnSync(command, args, {
      ...options,
      env: { ...process.env, AGENT_BROWSER_SOCKET_DIR: socketDir, ...(options.env || {}) },
    }),
  });
  const baseArgs = ["--session", session, ...(headed ? ["--headed"] : []), "--allowed-domains", "127.0.0.1"];

  // These must stay async: the fixture HTTP server runs in this process, so a
  // blocking spawn while the browser opens the page would deadlock the page
  // fetch.
  function run(args, options = {}) {
    return new Promise((resolveRun, rejectRun) => {
      const child = spawn(binary, [...baseArgs, ...args], {
        env: { ...process.env, AGENT_BROWSER_SOCKET_DIR: socketDir, ...(options.env || {}) },
        stdio: ["pipe", "pipe", "pipe"],
      });
      let stdout = "";
      let stderr = "";
      const timer = setTimeout(() => {
        child.kill("SIGTERM");
        rejectRun(new Error(`agent-browser ${args[0]} timed out after ${options.timeout || 30_000}ms`));
      }, options.timeout || 30_000);
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", (chunk) => { stdout += chunk; });
      child.stderr.on("data", (chunk) => { stderr += chunk; });
      child.on("error", (error) => { clearTimeout(timer); rejectRun(error); });
      child.on("close", (code) => {
        clearTimeout(timer);
        resolveRun({ status: code, stdout, stderr });
      });
      if (options.input !== undefined) child.stdin.end(options.input);
      else child.stdin.end();
    });
  }

  async function runJson(args, options = {}) {
    const result = await run([...args, "--json"], options);
    if (result.status !== 0) throw new Error(`agent-browser ${args.join(" ")} failed (${result.status}): ${result.stderr || result.stdout}`);
    const parsed = JSON.parse(result.stdout);
    if (!parsed.success) throw new Error(`agent-browser ${args.join(" ")} failed: ${parsed.error}`);
    return parsed.data;
  }

  async function evalJson(source, options = {}) {
    const data = await runJson(["eval", "--stdin"], { ...options, input: source });
    const value = data.result;
    return typeof value === "string" ? JSON.parse(value) : value;
  }

  async function screenshot(path) {
    mkdirSync(dirname(path), { recursive: true });
    await runJson(["screenshot", path], { timeout: 30_000 });
    return path;
  }

  function closeWithVerification(timeoutMs = 4000) {
    lifecycle.close();
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const listed = spawnSync(binary, ["session", "list", "--json"], {
        encoding: "utf8",
        env: { ...process.env, AGENT_BROWSER_SOCKET_DIR: socketDir },
        timeout: 5000,
      });
      try {
        const sessions = JSON.parse(listed.stdout || "{}")?.data?.sessions || [];
        if (!sessions.includes(session)) return { closed: true, session };
      } catch (_) {
        // Keep polling; a transient parse failure is not proof of cleanup.
      }
      const wait = new Int32Array(new SharedArrayBuffer(4));
      Atomics.wait(wait, 0, 0, 200);
    }
    return { closed: false, session, error: "owned browser session was still listed after close" };
  }

  function close() {
    lifecycle.close();
  }

  // Each case owns a unique pid-scoped session, so there is no same-name
  // session to clear before opening. Pre-open close races the agent-browser
  // daemon socket on 0.37.x and produces "Failed to connect"; keep cleanup on
  // exit and skip the unnecessary pre-open close.
  return { binary, session, run, runJson, evalJson, screenshot, close, closeWithVerification, prepare() {} };
}

export function resolveBrowserBinary(candidate) {
  if (typeof candidate !== "string" || !candidate) throw new Error("agent-browser binary is required");
  const paths = candidate.includes("/") || candidate.includes(sep)
    ? [resolve(candidate)]
    : (process.env.PATH ?? "/usr/bin:/bin").split(delimiter).map(directory => resolve(directory, candidate));
  for (const path of paths) {
    try {
      if (!statSync(path).isFile()) continue;
      accessSync(path, constants.X_OK);
      return path;
    } catch {
      // A missing or unusable PATH entry does not hide a later executable.
    }
  }
  throw new Error(`required browser automation binary is unavailable: ${candidate}`);
}

function waitForState(driver, readExpression, predicateSource, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    let last = null;
    while (Date.now() < deadline) {
      last = await driver.evalJson(`(() => { const state = ${readExpression}; return JSON.stringify({ value: (${predicateSource})(state), state }); })()`);
      if (last.value) return last.state;
      await new Promise((resolvePromise) => setTimeout(resolvePromise, 50));
    }
    throw new Error(`condition not reached: ${predicateSource}\n${JSON.stringify(last?.state ?? null).slice(0, 1200)}`);
  })();
}

const STATE_READER = "window.__AW_TEST__?.getState?.() || {}";

function createScenarioTrace() {
  const entries = [];
  return {
    entries,
    push(kind, detail = {}) {
      entries.push({ at: new Date().toISOString(), kind, ...detail });
      return entries.length;
    },
    jsonl() {
      return `${entries.map((entry) => JSON.stringify(entry)).join("\n")}\n`;
    },
  };
}

export function deliveryProbeSource(message, timeoutMs = 5000) {
  return `window.__BFR__.deliver(${JSON.stringify(message)}, ${timeoutMs}).then(
    (delivered) => JSON.stringify({ ok: true, delivered }),
    (error) => JSON.stringify({ ok: false, error: String(error && error.message ? error.message : error) })
  )`;
}

async function deliver(driver, message, trace = null) {
  trace?.push("deliver", { type: message.type, status: message?.ack?.status || message?.error?.status || null });
  const result = await driver.evalJson(deliveryProbeSource(message));
  if (!result.ok) {
    throw new Error(`fixture delivery barrier did not settle before its deadline for ${message.type}: ${result.error || "unknown error"}`);
  }
  trace?.push("deliver-settled", { type: message.type, delivered: result.delivered });
  return result;
}

async function waitOutbound(driver, fromIndex, spec, timeoutMs = 8000, trace = null) {
  const result = await driver.evalJson(`window.__BFR__.nextOutbound(${fromIndex}, ${JSON.stringify(spec)}, ${timeoutMs}).then((value) => JSON.stringify(value))`);
  if (!result.ok) throw new Error(`expected outbound ${JSON.stringify(spec)} was not observed: ${JSON.stringify(result)}`);
  trace?.push("outbound-request", { spec, cursor: result.cursor, requestId: result.entry?.message?.command?.request?.request_id || null });
  return result;
}

async function countOutbound(driver, spec) {
  return driver.evalJson(`JSON.stringify({ count: window.__BFR__.countOutbound(${JSON.stringify(spec)}), total: window.__BFR__.outboundCount() })`);
}

async function socketInfo(driver) {
  return driver.evalJson("JSON.stringify(window.__BFR__.socketInfo())");
}

async function currentSocketId(driver) {
  return driver.evalJson("JSON.stringify(window.__BFR__.currentSocketId())");
}

async function deliverToSocket(driver, socketId, message, trace = null) {
  trace?.push("deliver-old-socket", { socketId, type: message.type });
  const result = await driver.evalJson(`JSON.stringify(window.__BFR__.deliverToSocket(${socketId}, ${JSON.stringify(message)}))`);
  assert(result.ok === true, "fixture could not deliver a late message to the requested socket", result);
  return result;
}

async function armSigningGate(driver) {
  const armed = await driver.evalJson("JSON.stringify(window.__BFR__.armSigningGate())");
  assert(armed.installed === true, "deferred signer gate was not installed in the visual-test page", armed);
  return armed;
}

async function waitForSigning(driver, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await driver.evalJson("JSON.stringify(window.__BFR__.signingState())");
    if (last.entered === true && last.deferred === true) return last;
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 25));
  }
  throw new Error(`request did not enter the deferred signing stage\n${JSON.stringify(last)}`);
}

async function releaseSigning(driver, trace = null) {
  const released = await driver.evalJson("window.__BFR__.releaseSigning().then((value) => JSON.stringify(value))");
  trace?.push("release-signing", released);
  assert(released.released === true, "deferred signing release did not produce a real signature", released);
  return released;
}

async function expirePendingPromptAck(driver, trace = null) {
  const result = await driver.evalJson(`JSON.stringify((() => {
    const value = window.__AW_TEST__.expirePendingPromptControlAckTimeoutForTest();
    return { result: value, stage: window.__AW_TEST__.getState().lastPromptFeedback?.stage || null };
  })())`);
  trace?.push("expire-prompt-ack", result);
  assert(result.result === true, "test hook could not terminate the pending prompt request", result);
  return result;
}

async function promptFeedbackState(driver) {
  return driver.evalJson(`(() => {
    const feedback = window.__AW_TEST__.getState().lastPromptFeedback;
    return JSON.stringify(feedback ? {
      id: feedback.id,
      action: feedback.action,
      stage: feedback.stage,
      requestId: feedback.requestId ?? null,
      requestAuthNonce: feedback.requestAuthNonce ?? null,
      agentId: feedback.agentId ?? null,
      effect: feedback.effect ?? null,
    } : null);
  })()`);
}

async function promptResultState(driver) {
  return driver.evalJson(`(() => {
    const state = window.__AW_TEST__.getState();
    return JSON.stringify({
      selectedPromptVersion: state.selectedPromptVersion ?? null,
      promptDraftVersion: state.selectedPromptVersion ?? null,
      draftSystemPrompt: document.querySelector("#prompt-system")?.value ?? null,
      lastPromptFeedback: state.lastPromptFeedback ? {
        action: state.lastPromptFeedback.action,
        stage: state.lastPromptFeedback.stage,
        requestId: state.lastPromptFeedback.requestId ?? null,
      } : null,
    });
  })()`);
}

async function requestSnapshotCount(driver) {
  return driver.evalJson(`JSON.stringify({
    count: window.__BFR__.countOutbound({ type: "request_snapshot" }),
    total: window.__BFR__.outboundCount(),
  })`);
}

async function connectionGeneration(driver) {
  return driver.evalJson("window.__AW_TEST__.connectionGenerationForTest()");
}

async function waitForStableSockets(driver, minimum, { stableMs = 900, timeoutMs = 15000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  let signature = null;
  let stableSince = 0;
  while (Date.now() < deadline) {
    last = await socketInfo(driver);
    const outbound = await countOutbound(driver, {});
    const next = JSON.stringify({ sockets: last, outbound: outbound.total });
    if ((last || []).length >= minimum && next === signature) {
      if (Date.now() - stableSince >= stableMs) return last;
    } else {
      signature = next;
      stableSince = Date.now();
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));
  }
  throw new Error(`fixture socket set did not stabilize with ${minimum} sockets\n${JSON.stringify(last)}`);
}

async function beginRollbackRequest(driver, trace = null) {
  await driver.runJson(["fill", "#prompt-rollback-version", "999"]);
  trace?.push("ui-input", { selector: "#prompt-rollback-version", value: 999 });
  const before = await countOutbound(driver, {});
  await scrollIntoViewport(driver, '[data-prompt-action="rollback"]', trace);
  await driver.runJson(["click", '[data-prompt-action="rollback"]']);
  trace?.push("ui-click", { selector: "[data-prompt-action=rollback]" });
  const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "rollback" }, 8000, trace);
  await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "sent" && state.lastPromptFeedback?.action === "prompt_rollback"`);
  return sent.entry.message.command.request;
}

function promptControlError(request, message = "late result for an already terminated request") {
  return {
    type: "prompt_control_error",
    request_auth_nonce: request.auth.nonce,
    error: {
      request_id: request.request_id,
      agent_id: request.agent_id,
      status: "blocked",
      reason_code: "control_lost",
      value_visibility: "hidden",
      next_step: "reauthenticate_and_refresh_binding",
      message,
    },
  };
}

function promptControlApplied(request, version = 4, override = DRAFT_TEXT) {
  return {
    type: "prompt_control_ack",
    request_auth_nonce: request.auth.nonce,
    ack: {
      request_id: request.request_id,
      agent_id: request.agent_id,
      operation: "apply",
      status: "applied",
      version,
      applied_fields: ["system_prompt_override"],
      applied_scope: "current_runtime",
      persistence_scope: "none",
      sync_scope: "none",
      system_prompt_override: override,
    },
  };
}

async function remountState(driver) {
  return driver.evalJson(`(() => {
    const api = window.__BFR_REMOUNT__;
    return JSON.stringify({
      available: Boolean(api),
      mountCount: api ? api.mountCount() : null,
      rootChildren: document.getElementById("app")?.childElementCount ?? null,
      viewerShells: document.querySelectorAll("#app .viewer-shell").length,
      promptPanels: document.querySelectorAll(".command-surface__prompt-panel").length,
      applyControls: document.querySelectorAll('[data-prompt-action="apply"]').length,
      detailsPanel: Boolean(document.getElementById("viewer-details-panel")),
    });
  })()`);
}

async function disposeViewer(driver, trace = null) {
  const result = await driver.evalJson(`JSON.stringify((() => {
    const api = window.__BFR_REMOUNT__;
    if (!api) return { available: false };
    api.dispose();
    return { available: true, mountCount: api.mountCount() };
  })())`);
  trace?.push("dispose-viewer", result);
  assert(result.available === true, "visual-test page did not expose the real viewer disposer", result);
  return result;
}

async function mountViewer(driver, trace = null) {
  const count = await driver.evalJson("JSON.stringify((() => window.__BFR_REMOUNT__.mount())())");
  trace?.push("mount-viewer", { mountCount: count });
  return count;
}

async function ensurePromptEditorVisible(driver, trace = null) {
  const summarySelector = "#viewer-details-panel details.command-surface__advanced-details > summary";
  await scrollIntoViewport(driver, summarySelector, trace);
  const open = await driver.evalJson(`document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true`);
  if (!open) {
    await driver.runJson(["click", summarySelector]);
    trace?.push("ui-click", { selector: summarySelector });
    await waitForState(driver, STATE_READER, `() => document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true`, 8000);
  }
  const visible = await driver.evalJson(`Boolean(document.querySelector(".command-surface__prompt-panel"))`);
  if (!visible) {
    await scrollIntoViewport(driver, "[data-prompt-visibility-toggle=\"1\"]", trace);
    await driver.runJson(["click", "[data-prompt-visibility-toggle=\"1\"]"]);
    trace?.push("ui-click", { selector: "[data-prompt-visibility-toggle]" });
  }
  await waitForState(driver, STATE_READER, `() => Boolean(document.querySelector('.command-surface__prompt-panel')) && document.querySelector('[data-prompt-action="apply"]')?.disabled === false`, 8000);
}

async function openAdvancedPromptDetails(driver, trace = null) {
  const summarySelector = "#viewer-details-panel details.command-surface__advanced-details > summary";
  await driver.runJson(["scrollintoview", summarySelector]);
  await waitForState(
    driver,
    STATE_READER,
    `() => {
      const entry = document.querySelector(${JSON.stringify(summarySelector)});
      if (!entry) return false;
      const rect = entry.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    }`,
    8000,
  );
  const initial = await driver.evalJson(`(() => JSON.stringify({ open: document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true }))()`);
  assert(initial.open === false, "advanced prompt settings were already open before the player clicked the entry", initial);
  await driver.runJson(["click", summarySelector]);
  trace?.push("ui-click", { selector: summarySelector });
  await waitForState(
    driver,
    STATE_READER,
    `() => document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true`,
    8000,
  );
  trace?.push("ui-state", { selector: summarySelector, open: true });
}

async function openPromptEditor(driver, trace = null) {
  await openAdvancedPromptDetails(driver, trace);
  await driver.runJson(["scrollintoview", "[data-prompt-visibility-toggle=\"1\"]"]);
  await driver.runJson(["click", "[data-prompt-visibility-toggle=\"1\"]"]);
  trace?.push("ui-click", { selector: "[data-prompt-visibility-toggle]" });
  await waitForState(driver, STATE_READER, `() => Boolean(document.querySelector(".command-surface__prompt-panel")) && document.querySelector('[data-prompt-action="apply"]')?.disabled === false`, 8000);
  await driver.runJson(["scrollintoview", "[data-prompt-action=\"apply\"]"]);
}

async function typeDraft(driver, text, trace = null) {
  await driver.runJson(["fill", "#prompt-system", text]);
  trace?.push("ui-input", { selector: "#prompt-system", length: text.length });
}

async function scrollIntoViewport(driver, selector, trace = null) {
  // The viewer scrolls inside its own panels, so a window-level scroll probe can
  // stay off-screen after a reconnect re-render. Scroll the owning container and
  // require a stable in-viewport rect before a real click.
  const deadline = Date.now() + 8000;
  let probe = null;
  while (Date.now() < deadline) {
    probe = await driver.evalJson(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element) return JSON.stringify({ found: false });
      let rect = element.getBoundingClientRect();
      for (let node = element.parentElement; node && node !== document.documentElement; node = node.parentElement) {
        const style = getComputedStyle(node);
        if (/(auto|scroll)/.test(style.overflowY)) {
          node.scrollTop = Math.max(0, rect.top - node.clientHeight / 2 + rect.height / 2 + node.scrollTop);
          rect = element.getBoundingClientRect();
        }
      }
      if (rect.top < 0 || rect.bottom > window.innerHeight) {
        const scroller = document.scrollingElement || document.documentElement;
        scroller.scrollTop += rect.top - window.innerHeight / 2 + rect.height / 2;
        rect = element.getBoundingClientRect();
      }
      return JSON.stringify({
        found: true,
        inView: rect.width > 0 && rect.height > 0 && rect.top >= 0 && rect.left >= 0
          && rect.bottom <= window.innerHeight && rect.right <= window.innerWidth,
        top: rect.top,
        bottom: rect.bottom,
        innerHeight: window.innerHeight,
      });
    })()`);
    if (probe?.found && probe.inView) {
      trace?.push("ui-scroll", { selector, top: probe.top });
      return;
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 80));
  }
  throw new Error(`could not scroll ${selector} into the viewport\n${JSON.stringify(probe)}`);
}

async function clickApply(driver, trace = null) {
  await scrollIntoViewport(driver, "[data-prompt-action=\"apply\"]", trace);
  await driver.runJson(["click", "[data-prompt-action=\"apply\"]"]);
  trace?.push("ui-click", { selector: "[data-prompt-action=apply]" });
}

// ---- case execution -----------------------------------------------------

function caseUrl(port, { hash = "", extra = "" } = {}) {
  return `http://127.0.0.1:${port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en${extra}${hash}`;
}

async function bootstrapPromptScenario(driver, phaseLog, trace = null) {
  await waitOutbound(driver, 0, { type: "hello_v2" }, 8000, trace);
  phaseLog.push("hello_v2 outbound observed");
  await deliver(driver, helloAck, trace);
  const reconnect = await waitOutbound(driver, 0, { type: "authoritative_recovery", mode: "reconnect_sync" }, 8000, trace);
  phaseLog.push("reconnect_sync outbound observed");
  const identity = await driver.evalJson("JSON.stringify(window.__BFR__.identity())");
  assert(identity.playerId && identity.publicKey, "local test session identity was not issued", identity);
  await deliver(driver, {
    type: "authoritative_recovery_ack",
    ack: {
      status: "catch_up_ready",
      player_id: identity.playerId,
      session_pubkey: identity.publicKey,
      agent_id: "agent-0",
      session_epoch: 12,
      binding_epoch: 9,
    },
  }, trace);
  await waitForState(driver, STATE_READER, `(state) => state.authRegistrationStatus === "registered" && state.authBoundAgentId === "agent-0" && state.authBindingEpoch === 9 && state.authSessionEpoch === 12`);
  phaseLog.push("player session and Agent binding registered through authoritative recovery ack");
  await deliver(driver, { type: "snapshot", snapshot: promptSnapshot() }, trace);
  await waitForState(driver, STATE_READER, `(state) => state.selectedKind === "agent" && state.selectedId === "agent-0"`);
  phaseLog.push("canonical snapshot adopted and agent-0 selected");
  return { reconnect, identity };
}

const CASES = {
  "p01-loading": async (context) => {
    const { driver, phaseLog, evidence, trace, artifacts } = context;
    const pending = await driver.evalJson(`(() => {
      const node = document.querySelector('#viewer-targets-panel [aria-busy="true"]');
      return JSON.stringify({
        pendingVisible: Boolean(node),
        pendingText: node ? node.textContent.replace(/\\s+/g, " ").trim() : null,
        promptActionCount: document.querySelectorAll("[data-prompt-action]").length,
        state: window.__AW_TEST__?.getState?.() || {},
      });
    })()`);
    phaseLog.push("unsynced loading state observed before hello_ack");
    evidence.observations.loading = pending;
    assert(pending.pendingVisible, "targets panel did not expose an aria-busy pending state before sync");
    assert(/Syncing agents/i.test(pending.pendingText || ""), "loading state does not name the pending agent sync", pending.pendingText);
    assert(pending.promptActionCount === 0, "prompt controls existed before the session was ready", pending.promptActionCount);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 600));
    const promptCounts = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.outboundBeforeReady = promptCounts;
    assert(promptCounts.count === 0, "a prompt request was sent before readiness", promptCounts);
    const stateBeforeReady = await driver.evalJson(`JSON.stringify({
      connectionStatus: (${STATE_READER}).connectionStatus,
      protocolNegotiated: (${STATE_READER}).viewerProtocol?.negotiated ?? null,
      authReady: (${STATE_READER}).authReady,
    })`);
    evidence.observations.stateBeforeReady = stateBeforeReady;
    assert(stateBeforeReady.protocolNegotiated !== true, "viewer protocol must still be unnegotiated before hello_ack", stateBeforeReady);
    assert(stateBeforeReady.connectionStatus === "connecting" || stateBeforeReady.connectionStatus === "connected", "unexpected connection state before hello_ack", stateBeforeReady);
    await artifacts.screenshot("loading");
  },

  "p01-empty": async (context) => {
    const { driver, phaseLog, evidence, trace, artifacts } = context;
    await waitOutbound(driver, 0, { type: "hello_v2" });
    await deliver(driver, helloAck, trace);
    await deliver(driver, { type: "snapshot", snapshot: emptySnapshot }, trace);
    const observed = await waitForState(driver, STATE_READER, `(state) => state.selectedId === null && (document.body.innerText.includes("No agents in current snapshot.") || document.body.innerText.includes("当前快照里没有行动体。"))`);
    phaseLog.push("empty canonical snapshot rendered an explicit empty state");
    const dom = await driver.evalJson(`(() => JSON.stringify({
      agentsEmpty: document.body.innerText.includes("No agents in current snapshot."),
      promptActionCount: document.querySelectorAll("[data-prompt-action]").length,
      promptOverridesPanel: Boolean(document.querySelector(".command-surface__prompt-panel")),
      worldFailClaimed: /world available|world ready/i.test(document.body.innerText),
    }))()`);
    evidence.observations.empty = { ...dom, selectedId: observed.selectedId };
    assert(dom.agentsEmpty, "empty snapshot did not render the explicit no-agents empty state");
    assert(dom.promptActionCount === 0, "empty snapshot still exposed prompt controls", dom.promptActionCount);
    assert(dom.worldFailClaimed === false, "empty state fabricated world availability");
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 500));
    const promptCounts = await countOutbound(driver, { type: "prompt_control" });
    assert(promptCounts.count === 0, "no prompt request may be sent from the empty state", promptCounts);
    evidence.observations.outbound = await countOutbound(driver, {});
    await artifacts.screenshot("empty");
  },

  // Negative: the canonical snapshot is visible and the Agent row is
  // selectable, but the runtime has not acknowledged the player session
  // registration (no session/binding epochs). Prompt submission must stay
  // disabled and must never hit the transport.
  "p02-unready-no-send": async (context) => {
    const { driver, phaseLog, evidence, trace, artifacts } = context;
    await waitOutbound(driver, 0, { type: "hello_v2" }, 8000, trace);
    await deliver(driver, helloAck, trace);
    const reconnect = await waitOutbound(driver, 0, { type: "authoritative_recovery", mode: "reconnect_sync" }, 8000, trace);
    const identity = await driver.evalJson("JSON.stringify(window.__BFR__.identity())");
    assert(identity.playerId && identity.publicKey, "local test session identity was not issued before the negative probe", identity);
    // Bind the Agent but deliberately withhold session_epoch/binding_epoch, so
    // the runtime still has no prompt-control epochs for this session.
    await deliver(driver, {
      type: "authoritative_recovery_ack",
      ack: {
        status: "catch_up_ready",
        player_id: identity.playerId,
        session_pubkey: identity.publicKey,
        agent_id: "agent-0",
      },
    }, trace);
    const snapshot = promptSnapshot();
    snapshot.model.agent_player_bindings = { "agent-0": identity.playerId };
    snapshot.model.agent_player_public_key_bindings = { "agent-0": identity.publicKey };
    await deliver(driver, { type: "snapshot", snapshot }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.selectedKind === "agent" && state.selectedId === "agent-0" && state.authBoundAgentId === "agent-0" && state.authSessionEpoch == null && state.authBindingEpoch == null`);
    phaseLog.push("canonical snapshot adopted while the player-session registration ACK is still pending");
    await openAdvancedPromptDetails(driver, trace);
    await driver.runJson(["scrollintoview", "[data-prompt-visibility-toggle=\"1\"]"]);
    await driver.runJson(["click", "[data-prompt-visibility-toggle=\"1\"]"]);
    await waitForState(driver, STATE_READER, `() => Boolean(document.querySelector('.command-surface__prompt-panel'))`, 6000);
    const readiness = await driver.evalJson(`(() => JSON.stringify({
      readinessText: document.querySelector('[data-prompt-readiness="blocked"]')?.textContent?.trim() || null,
      applyFound: Boolean(document.querySelector('[data-prompt-action="apply"]')),
      applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
      state: (() => { const value = window.__AW_TEST__.getState(); return { registrationStatus: value.authRegistrationStatus, sessionEpoch: value.authSessionEpoch, bindingEpoch: value.authBindingEpoch, capability: value.authSurface?.capabilities?.prompt_control?.code || null }; })(),
    }))()`);
    evidence.observations.unready = readiness;
    assert(readiness.applyFound, "Apply control was not rendered in the unready state", readiness);
    assert(readiness.applyDisabled === true, "Apply control was enabled before the player session registration ACK", readiness.applyDisabled);
    assert(readiness.readinessText && /waiting for a current registered player session/i.test(readiness.readinessText), "unready prompt panel did not explain the pending player session", readiness.readinessText);
    assert(readiness.state.registrationStatus !== "registered" || readiness.state.sessionEpoch == null, "unready negative probe unexpectedly had a registered session epoch", readiness.state);
    // Real click on the disabled control must not reach the transport.
    try {
      await driver.runJson(["click", "[data-prompt-action=\"apply\"]"]);
    } catch (_) {
      // agent-browser may refuse a disabled target; the request count below
      // is the authority for the assertion.
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 600));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.unreadyOutbound = totals;
    assert(totals.count === 0, "unready state sent a prompt request from the disabled control", totals);
    phaseLog.push("unready Apply control stayed disabled and emitted no prompt request");
    await artifacts.screenshot("unready");
  },

  "p02-success": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const enabled = await driver.evalJson(`(() => { const node = document.querySelector('[data-prompt-action="apply"]'); return JSON.stringify({ found: Boolean(node), disabled: node ? node.disabled : null }); })()`);
    assert(enabled.found && enabled.disabled === false, "Apply Prompt control did not become enabled after readiness", enabled);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const draftBefore = await driver.evalJson(`(() => { const state = window.__AW_TEST__.getState(); return JSON.stringify({ value: document.querySelector("#prompt-system").value, draft: state && window.__AW_TEST__.getState ? null : null }); })()`);
    assert(draftBefore.value === DRAFT_TEXT, "visible draft textarea did not receive the typed draft", draftBefore);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    phaseLog.push("apply request observed once after a single visible click");
    const request = sent.entry.message.command.request;
    evidence.observations.request = {
      requestId: request.request_id,
      sessionEpoch: request.session_epoch,
      bindingEpoch: request.binding_epoch,
      expectedAuthorityEpoch: request.expected_authority_epoch,
      expectedVersion: request.expected_version,
      systemPromptOverride: request.system_prompt_override,
      nonce: request.auth?.nonce,
    };
    assert(/^pc-/.test(request.request_id || ""), "apply request is missing a correlated request id", request);
    assert(request.session_epoch === 12 && request.binding_epoch === 9, "apply request is missing the registered session/binding epochs", request);
    assert(request.expected_authority_epoch === "bfr-authority-1", "apply request is missing the negotiated authority epoch", request);
    assert(request.expected_version === 3, "apply request expected version should come from the visible profile", request.expected_version);
    assert(request.system_prompt_override === DRAFT_TEXT, "apply request did not carry the typed draft", request.system_prompt_override);
    await artifacts.screenshot("pending");
    await deliver(driver, { type: "prompt_control_ack", request_auth_nonce: request.auth.nonce, ack: { request_id: request.request_id, agent_id: "agent-0", operation: "apply", status: "accepted" } }, trace);
    const acceptedState = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "accepted"`);
    const acceptedDom = await driver.evalJson(`(() => JSON.stringify({
      stageAttr: document.querySelector(".command-surface__prompt-panel .feedback-card")?.getAttribute("data-feedback-stage") || null,
      label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
      text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
      draft: document.querySelector("#prompt-system")?.value ?? null,
      applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
      draftState: (() => { const value = window.__AW_TEST__.getState(); return { selectedPromptVersion: value.selectedPromptVersion }; })(),
    }))()`);
    phaseLog.push("accepted receipt did not advance the applied version");
    evidence.observations.accepted = { stage: acceptedState.lastPromptFeedback.stage, dom: acceptedDom };
    assert(acceptedDom.label === "Accepted", "accepted receipt was not rendered as accepted", acceptedDom.label);
    assert(!/Prompt applied to the current runtime/i.test(acceptedDom.text), "accepted receipt rendered the applied copy before an applied result");
    assert(acceptedDom.draft === DRAFT_TEXT, "accepted receipt cleared the unsubmitted draft", acceptedDom.draft);
    assert(acceptedDom.draftState.selectedPromptVersion === 3, "accepted receipt advanced the visible prompt version early", acceptedDom.draftState);
    await artifacts.screenshot("accepted");
    await deliver(driver, { type: "prompt_control_ack", request_auth_nonce: request.auth.nonce, ack: { request_id: request.request_id, agent_id: "agent-0", operation: "apply", status: "applied", version: 4, applied_fields: ["system_prompt_override"], applied_scope: "current_runtime", persistence_scope: "none", sync_scope: "none" } }, trace);
    const refresh = await waitOutbound(driver, before.total, { type: "request_snapshot" });
    phaseLog.push("applied receipt triggered an authoritative snapshot refresh request");
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 4, override: DRAFT_TEXT, time: 8 }) }, trace);
    const appliedState = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "applied" && state.selectedPromptVersion === 4`);
    const appliedDom = await driver.evalJson(`(() => JSON.stringify({
      stageAttr: document.querySelector(".command-surface__prompt-panel .feedback-card")?.getAttribute("data-feedback-stage") || null,
      label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
      text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
      draft: document.querySelector("#prompt-system")?.value ?? null,
      applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
    }))()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 400));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.applied = { stage: appliedState.lastPromptFeedback.stage, dom: appliedDom, promptRequestCount: totals.count, refreshObserved: refresh.cursor };
    assert(appliedDom.label === "Applied", "applied receipt was not rendered as applied", appliedDom.label);
    assert(/Prompt applied to the current runtime/i.test(appliedDom.text), "applied receipt did not render the applied copy", appliedDom.text.slice(0, 400));
    assert(appliedDom.draft === DRAFT_TEXT, "authoritative refresh did not keep the submitted draft value", appliedDom.draft);
    assert(appliedDom.applyDisabled === false, "controls did not return to an enabled state after the applied refresh", appliedDom.applyDisabled);
    assert(totals.count === 1, "one visible apply click produced more than one prompt request", totals);
    await artifacts.screenshot("final");
  },

  "p03-rejected": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    evidence.observations.requestId = request.request_id;
    await artifacts.screenshot("pending");
    await deliver(driver, { type: "prompt_control_error", request_auth_nonce: request.auth.nonce, error: { request_id: request.request_id, agent_id: "agent-0", status: "rejected", reason_code: "request_rejected", message: "fixture rejected the request", next_step: "correct_request_and_retry" } }, trace);
    const state = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "rejected"`);
    const dom = await driver.evalJson(`(() => JSON.stringify({
      stageAttr: document.querySelector(".command-surface__prompt-panel .feedback-card")?.getAttribute("data-feedback-stage") || null,
      label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
      text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
      draft: document.querySelector("#prompt-system")?.value ?? null,
      applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
      draftDirty: (() => { const value = window.__AW_TEST__.getState(); return value.selectedPromptVersion; })(),
    }))()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 400));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.rejected = { stage: state.lastPromptFeedback.stage, dom, promptRequestCount: totals.count };
    phaseLog.push("correlated rejection preserved the unsubmitted draft and controls");
    assert(dom.label === "Rejected", "rejection was not rendered as rejected", dom.label);
    assert(dom.draft === DRAFT_TEXT, "rejection cleared the unsubmitted draft", dom.draft);
    assert(dom.applyDisabled === false, "rejection did not restore the Apply control", dom.applyDisabled);
    assert(dom.draftDirty === 3, "rejection advanced the visible prompt version", dom.draftDirty);
    assert(!/Prompt applied to the current runtime/i.test(dom.text), "rejection path showed applied before rollback", dom.text.slice(0, 400));
    assert(/Next step.*Correct the request/i.test(dom.text), "rejection did not surface the safe next step", dom.text.slice(0, 500));
    assert(totals.count === 1, "rejection path sent additional prompt requests", totals);
    await artifacts.screenshot("final");
  },

  "p03-stale": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    await artifacts.screenshot("pending");
    await deliver(driver, { type: "prompt_control_error", request_auth_nonce: request.auth.nonce, error: { request_id: request.request_id, agent_id: "agent-0", reason_code: "version_conflict", message: "fixture detected a stale prompt version" } }, trace);
    const state = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "stale"`);
    const dom = await driver.evalJson(`(() => JSON.stringify({
      stageAttr: document.querySelector(".command-surface__prompt-panel .feedback-card")?.getAttribute("data-feedback-stage") || null,
      label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
      text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
      draft: document.querySelector("#prompt-system")?.value ?? null,
      applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
      selectedPromptVersion: window.__AW_TEST__.getState().selectedPromptVersion,
    }))()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 400));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.stale = { stage: state.lastPromptFeedback.stage, dom, promptRequestCount: totals.count };
    phaseLog.push("stale result preserved the draft and surfaced the version-refresh next step");
    assert(dom.label === "Stale", "stale result was not rendered as stale", dom.label);
    assert(dom.draft === DRAFT_TEXT, "stale result cleared the unsubmitted draft", dom.draft);
    assert(dom.applyDisabled === false, "stale result did not restore the Apply control", dom.applyDisabled);
    assert(dom.selectedPromptVersion === 3, "stale result advanced the visible prompt version", dom.selectedPromptVersion);
    assert(/Next step.*Refresh the current version/i.test(dom.text), "stale result did not surface the version-refresh next step", dom.text.slice(0, 500));
    assert(!/Prompt applied to the current runtime/i.test(dom.text), "stale path showed applied before rollback", dom.text.slice(0, 400));
    assert(totals.count === 1, "stale path sent additional prompt requests", totals);
    await artifacts.screenshot("final");
  },

  "p03-control-lost": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    await artifacts.screenshot("pending");
    await deliver(driver, {
      type: "prompt_control_ack",
      request_auth_nonce: request.auth.nonce,
      ack: {
        request_id: request.request_id,
        agent_id: "agent-0",
        operation: "apply",
        status: "blocked",
        reason_code: "control_lost",
        value_visibility: "hidden",
        next_step: "reauthenticate_and_refresh_binding",
        message: "Agent control was lost; re-authenticate and refresh the current binding before retrying.",
      },
    }, trace);
    const state = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "blocked"`, 4000);
    const dom = await driver.evalJson(`(() => {
      const feedback = window.__AW_TEST__.getState().lastPromptFeedback;
      const response = feedback?.response || {};
      return JSON.stringify({
        label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
        text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
        draft: document.querySelector("#prompt-system")?.value ?? null,
        applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
        promptPanel: Boolean(document.querySelector(".command-surface__prompt-panel")),
        recoveryCta: Boolean(document.querySelector('[data-prompt-recovery-action="refresh-binding"]')),
        recoveryText: document.querySelector('[data-prompt-recovery-action="refresh-binding"]')?.textContent?.trim() || null,
        responseKeys: Object.keys(response).sort(),
        responseVisibility: response.value_visibility ?? null,
        responseHasPromptValue: Object.prototype.hasOwnProperty.call(response, "system_prompt_override"),
        bodyMentionsTypedDraft: document.querySelector("#viewer-details-panel")?.innerText.includes(${JSON.stringify(DRAFT_TEXT)}) ?? null,
      });
    })()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 500));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.controlLost = { stage: state.lastPromptFeedback.stage, dom, promptRequestCount: totals.count };
    phaseLog.push("correlated control-lost receipt rendered its safe reason, next step and recovery action");
    assert(dom.label === "Blocked", "control-lost receipt was not rendered as blocked", dom.label);
    assert(dom.promptPanel === true, "control-lost receipt removed the prompt surface", dom.promptPanel);
    assert(dom.draft === DRAFT_TEXT, "control-lost receipt cleared the unsubmitted draft", dom.draft);
    assert(dom.applyDisabled === false, "control-lost receipt left the Apply control stuck", dom.applyDisabled);
    assert(dom.recoveryCta === true, "control-lost receipt did not expose the binding recovery action", dom.recoveryText);
    assert(/Next step.*Re-authenticate and refresh the current binding/i.test(dom.text), "control-lost receipt did not surface the safe next step", dom.text.slice(0, 500));
    assert(!/Prompt applied to the current runtime/i.test(dom.text), "control-lost path showed applied before any result", dom.text.slice(0, 400));
    assert(dom.responseVisibility === "hidden", "control-lost response did not mark hidden value visibility", dom.responseKeys);
    assert(dom.responseHasPromptValue === false, "control-lost response leaked the submitted prompt value", dom.responseKeys);
    assert(dom.bodyMentionsTypedDraft === false, "control-lost diagnostics rendered the submitted prompt value outside the draft editor", dom.bodyMentionsTypedDraft);
    assert(totals.count === 1, "control-lost path retried the prompt request", totals);
    await artifacts.screenshot("final");
  },

  // Second control-loss shape. The enhanced runtime converts a lost
  // authority/binding into a hidden blocked error (`prompt_control_result_unknown`)
  // instead of a bare agent_control_forbidden legacy error; the player surface
  // must still keep the draft, keep the control lane usable, show the safe next
  // step and never leak submitted values.
  "p03-control-lost-unknown": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    await artifacts.screenshot("pending");
    await deliver(driver, {
      type: "prompt_control_error",
      request_auth_nonce: request.auth.nonce,
      error: {
        request_id: request.request_id,
        agent_id: "agent-0",
        operation: "apply",
        code: "prompt_control_result_unknown",
        reason_code: "result_unknown",
        status: "blocked",
        value_visibility: "hidden",
        next_step: "refresh_authority_and_retry_with_new_request_id",
        message: "prompt control result is unknown for the current authority epoch",
        authority_epoch: "stale-authority-0",
        session_epoch: 99,
        binding_epoch: 99,
        current_version: 9,
        system_prompt_override: "runtime secret prompt value",
      },
    }, trace);
    const state = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "blocked"`, 4000);
    const dom = await driver.evalJson(`(() => {
      const value = window.__AW_TEST__.getState();
      const feedback = value.lastPromptFeedback;
      const response = feedback?.response || {};
      return JSON.stringify({
        label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
        text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
        draft: document.querySelector("#prompt-system")?.value ?? null,
        applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
        promptPanel: Boolean(document.querySelector(".command-surface__prompt-panel")),
        selectedKind: value.selectedKind,
        selectedId: value.selectedId,
        responseKeys: Object.keys(response).sort(),
        responseVisibility: response.value_visibility ?? null,
        leaksPromptValue: JSON.stringify(response).includes("runtime secret prompt value"),
        leaksEpoch: JSON.stringify(response).includes("99"),
        bodyLeaksPromptValue: (document.getElementById("viewer-details-panel")?.innerText || "").includes("runtime secret prompt value"),
      });
    })()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 500));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.controlLostUnknown = { stage: state.lastPromptFeedback.stage, dom, promptRequestCount: totals.count };
    phaseLog.push("hidden blocked runtime error kept selection, draft and usable controls");
    assert(dom.label === "Blocked", "hidden blocked error was not rendered as blocked", dom.label);
    assert(dom.selectedKind === "agent" && dom.selectedId === "agent-0", "hidden blocked error cleared the selected Agent", { kind: dom.selectedKind, id: dom.selectedId });
    assert(dom.promptPanel === true, "hidden blocked error removed the prompt surface", dom.promptPanel);
    assert(dom.draft === DRAFT_TEXT, "hidden blocked error cleared the unsubmitted draft", dom.draft);
    assert(dom.applyDisabled === false, "hidden blocked error left the Apply control stuck", dom.applyDisabled);
    assert(/Refresh authority, then retry with a new request id/i.test(dom.text), "hidden blocked error did not surface the safe next step", dom.text.slice(0, 500));
    assert(!/Prompt applied to the current runtime/i.test(dom.text), "hidden blocked error showed applied before any result", dom.text.slice(0, 400));
    assert(dom.responseVisibility === "hidden", "hidden blocked response did not retain hidden visibility", dom.responseKeys);
    assert(dom.leaksPromptValue === false, "hidden blocked response leaked the runtime prompt value", dom.responseKeys);
    assert(dom.leaksEpoch === false, "hidden blocked response leaked stale binding/session epochs", dom.responseKeys);
    assert(dom.bodyLeaksPromptValue === false, "hidden blocked diagnostics rendered the runtime prompt value", dom.responseKeys);
    assert(totals.count === 1, "hidden blocked path retried the prompt request", totals);
    await artifacts.screenshot("final");
  },

  // P04: the old request is terminated while the new request is fixed in its
  // real signing stage. A late result for A must not touch B's timer, request
  // identity, feedback or transport. The deferred signer only delays the real
  // WebCrypto signature result; it never fabricates or edits the payload.
  "p04-signing-deferred": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const rollback = await beginRollbackRequest(driver, trace);
    phaseLog.push("old A rollback request observed once through real UI controls");
    const terminated = await expirePendingPromptAck(driver, trace);
    assert(terminated.stage === "error", "old A request did not terminate before B started", terminated);
    const armed = await armSigningGate(driver);
    evidence.observations.signingGate = armed;
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const signing = await waitForSigning(driver);
    const signingFeedback = await promptFeedbackState(driver);
    evidence.observations.signing = { signing, feedback: signingFeedback };
    assert(signingFeedback.stage === "signing", "B was not fixed in its signing stage", signingFeedback);
    const outboundWhileSigning = await countOutbound(driver, { type: "prompt_control" });
    assert(outboundWhileSigning.count === 1, "B reached the transport before its real signature completed", outboundWhileSigning);
    await deliver(driver, promptControlError(rollback, "late error for terminated request A"), trace);
    const afterLate = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterLateA = afterLate;
    assert(afterLate.stage === "signing", "a late result for A moved B out of signing", { before: signingFeedback, after: afterLate });
    assert(afterLate.requestId === signingFeedback.requestId && afterLate.requestAuthNonce === signingFeedback.requestAuthNonce, "a late result for A rewrote B request identity", { before: signingFeedback, after: afterLate });
    const released = await releaseSigning(driver, trace);
    evidence.observations.released = released;
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" }, 8000, trace);
    const requestB = sent.entry.message.command.request;
    evidence.observations.requestA = { requestId: rollback.request_id, nonce: rollback.auth.nonce, toVersion: rollback.to_version };
    evidence.observations.requestB = {
      requestId: requestB.request_id,
      nonce: requestB.auth.nonce,
      expectedVersion: requestB.expected_version,
      sessionEpoch: requestB.session_epoch,
      bindingEpoch: requestB.binding_epoch,
    };
    assert(requestB.request_id && requestB.request_id !== rollback.request_id, "B reused the terminated A request identity", { A: rollback.request_id, B: requestB.request_id });
    assert(requestB.auth.nonce !== rollback.auth.nonce, "B reused the terminated A auth nonce", { A: rollback.auth.nonce, B: requestB.auth.nonce });
    assert(requestB.expected_version === 3, "B request did not carry the visible prompt version", requestB.expected_version);
    const sentFeedback = await promptFeedbackState(driver);
    assert(sentFeedback.stage === "sent" && sentFeedback.requestId === requestB.request_id, "B feedback did not follow its real send", sentFeedback);
    await deliver(driver, promptControlError(rollback, "late error for terminated request A after B sent"), trace);
    const afterSentLate = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterSentLateA = afterSentLate;
    assert(afterSentLate.stage === "sent", "a late result for A changed B after B was sent", afterSentLate);
    assert(afterSentLate.requestId === requestB.request_id && afterSentLate.requestAuthNonce === requestB.auth.nonce, "a late result for A rewrote B identity after B was sent", afterSentLate);
    const timer = await expirePendingPromptAck(driver, trace);
    evidence.observations.timerOwnership = timer;
    assert(timer.result === true, "B lost its own acknowledgement timer to the late A result", timer);
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 2, "A/B scenario sent an unexpected prompt request count", totals);
    await artifacts.screenshot("final");
  },

  // P04 variant: B is already sent when A's late error arrives. B must keep its
  // own pending state and only its own correlated applied result may settle it.
  "p04-sent-late-error": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const rollback = await beginRollbackRequest(driver, trace);
    const terminated = await expirePendingPromptAck(driver, trace);
    assert(terminated.stage === "error", "old A request did not terminate", terminated);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const requestB = sent.entry.message.command.request;
    evidence.observations.requestA = { requestId: rollback.request_id, nonce: rollback.auth.nonce };
    evidence.observations.requestB = { requestId: requestB.request_id, nonce: requestB.auth.nonce };
    await deliver(driver, promptControlError(rollback, "late error for terminated request A"), trace);
    const afterLate = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterLateA = afterLate;
    assert(afterLate.stage === "sent", "a late result for A settled B after B was sent", afterLate);
    assert(afterLate.requestId === requestB.request_id && afterLate.requestAuthNonce === requestB.auth.nonce, "a late result for A rewrote the sent B identity", afterLate);
    await deliver(driver, promptControlApplied(requestB), trace);
    const applied = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "applied"`);
    evidence.observations.applied = { stage: applied.lastPromptFeedback.stage, requestId: applied.lastPromptFeedback.requestId };
    assert(applied.lastPromptFeedback.requestId === requestB.request_id, "applied result was not correlated with B", applied.lastPromptFeedback.requestId);
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 2, "late A error caused an extra prompt request", totals);
    await artifacts.screenshot("final");
  },

  // P05: duplicate and out-of-order results after B settled must not create a
  // second refresh/version advance or let the old A result overwrite B. A
  // locally timed-out request is not authoritative: its correlated server
  // result must still settle the feedback, once, without a later duplicate
  // re-triggering the refresh.
  "p05-duplicate-out-of-order": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const rollback = await beginRollbackRequest(driver, trace);
    const terminated = await expirePendingPromptAck(driver, trace);
    assert(terminated.stage === "error", "old A request did not terminate", terminated);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const requestB = sent.entry.message.command.request;
    evidence.observations.requestA = { requestId: rollback.request_id, nonce: rollback.auth.nonce };
    evidence.observations.requestB = { requestId: requestB.request_id, nonce: requestB.auth.nonce };
    await deliver(driver, {
      type: "prompt_control_ack",
      request_auth_nonce: requestB.auth.nonce,
      ack: { request_id: requestB.request_id, agent_id: "agent-0", operation: "apply", status: "accepted" },
    }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "accepted"`);
    await deliver(driver, promptControlApplied(requestB), trace);
    await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "applied"`);
    await waitOutbound(driver, 0, { type: "request_snapshot" }, 8000, trace);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 4, override: DRAFT_TEXT, time: 8 }) }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.selectedPromptVersion === 4`);
    phaseLog.push("B applied result and its authoritative version 4 snapshot were adopted");
    const beforeDuplicate = await requestSnapshotCount(driver);
    await deliver(driver, promptControlApplied(requestB), trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterDuplicate = await requestSnapshotCount(driver);
    const duplicateResult = await promptResultState(driver);
    evidence.observations.duplicate = { beforeDuplicate, afterDuplicate, result: duplicateResult };
    assert(afterDuplicate.count === beforeDuplicate.count, "a duplicate applied result triggered another snapshot refresh/commit", { beforeDuplicate, afterDuplicate });
    assert(duplicateResult.selectedPromptVersion === 4, "a duplicate applied result advanced or reset the visible version", duplicateResult);
    assert(duplicateResult.draftSystemPrompt === DRAFT_TEXT, "a duplicate applied result rewrote the applied draft value", duplicateResult);
    await deliver(driver, promptControlError(rollback, "old A error after B applied"), trace);
    await deliver(driver, promptControlApplied(rollback), trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const outOfOrder = await promptResultState(driver);
    evidence.observations.outOfOrder = outOfOrder;
    assert(outOfOrder.lastPromptFeedback?.stage === "applied", "an old A result overwrote the applied B feedback", outOfOrder);
    assert(outOfOrder.lastPromptFeedback?.requestId === requestB.request_id, "an old A result replaced the current B request id", outOfOrder);
    assert(outOfOrder.selectedPromptVersion === 4, "an old A result reverted the visible prompt version", outOfOrder);
    const LATE_TEXT = "late authoritative prompt";
    await typeDraft(driver, LATE_TEXT, trace);
    const beforeC = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sentC = await waitOutbound(driver, beforeC.total, { type: "prompt_control", mode: "apply" }, 8000, trace);
    const requestC = sentC.entry.message.command.request;
    evidence.observations.requestC = { requestId: requestC.request_id, nonce: requestC.auth.nonce };
    const timedOut = await expirePendingPromptAck(driver, trace);
    const timedOutState = await promptFeedbackState(driver);
    evidence.observations.timedOutC = { timer: timedOut, feedback: timedOutState };
    assert(timedOutState.stage === "error" && timedOutState.requestId === requestC.request_id, "C did not settle locally as timed out", timedOutState);
    const beforeLateAck = await requestSnapshotCount(driver);
    await deliver(driver, promptControlApplied(requestC, 5, LATE_TEXT), trace);
    const lateSettled = await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "applied" && state.lastPromptFeedback?.requestId === ${JSON.stringify(requestC.request_id)}`);
    await waitOutbound(driver, beforeLateAck.total, { type: "request_snapshot" }, 8000, trace);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 5, override: LATE_TEXT, time: 9 }) }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.selectedPromptVersion === 5`);
    const lateResult = await promptResultState(driver);
    evidence.observations.lateAuthoritative = { settled: lateSettled.lastPromptFeedback, result: lateResult };
    assert(lateResult.lastPromptFeedback?.stage === "applied" && lateResult.lastPromptFeedback?.requestId === requestC.request_id, "the authoritative result did not settle the locally timed-out request", lateResult);
    assert(lateResult.selectedPromptVersion === 5, "the authoritative result did not refresh the timed-out request version", lateResult);
    const beforeLateDuplicate = await requestSnapshotCount(driver);
    await deliver(driver, promptControlApplied(requestC, 5, LATE_TEXT), trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterLateDuplicate = await requestSnapshotCount(driver);
    evidence.observations.lateDuplicate = { beforeDuplicate: beforeLateDuplicate, afterDuplicate: afterLateDuplicate };
    assert(afterLateDuplicate.count === beforeLateDuplicate.count, "a duplicate late result re-triggered the settled request refresh", { beforeLateDuplicate, afterLateDuplicate });
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 3, "duplicate/out-of-order or late-result handling caused an extra prompt request", totals);
    await artifacts.screenshot("final");
  },

  // P06: a held request is stranded by the disconnect; the next connection gets
  // its own generation, old-socket events cannot negotiate or overwrite the new
  // snapshot, submission stays closed until the new binding is restored, and a
  // new visible request settles only on its own correlated result.
  "p06-reconnect-recovery": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const requestA = sent.entry.message.command.request;
    evidence.observations.heldRequest = { requestId: requestA.request_id, nonce: requestA.auth.nonce, sessionEpoch: requestA.session_epoch, bindingEpoch: requestA.binding_epoch };
    const generationBefore = await connectionGeneration(driver);
    const oldSocketId = await currentSocketId(driver);
    const afterCloseIndex = (await countOutbound(driver, {})).total;
    await driver.evalJson("JSON.stringify(window.__BFR__.closeSocket())");
    trace.push("close-socket", { socketId: oldSocketId });
    const socketList = await waitForStableSockets(driver, 2);
    const newSocketId = await currentSocketId(driver);
    await waitOutbound(driver, afterCloseIndex, { type: "hello_v2" }, 12000, trace);
    const generationAfter = await connectionGeneration(driver);
    evidence.observations.connection = { generationBefore, generationAfter, oldSocketId, newSocketId, sockets: socketList };
    assert(newSocketId !== oldSocketId, "reconnect reused the previous socket", { oldSocketId, newSocketId });
    assert(generationAfter > generationBefore, "reconnect did not advance the auth connection generation", { generationBefore, generationAfter });
    // Old-socket events are queued after the new socket exists, then released.
    await deliverToSocket(driver, oldSocketId, helloAck, trace);
    await deliverToSocket(driver, oldSocketId, { type: "snapshot", snapshot: promptSnapshot({ version: 9, override: "old-connection-secret", time: 99 }) }, trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterOldHandshake = await driver.evalJson(`(() => JSON.stringify({
      state: window.__AW_TEST__.getState(),
      selectedPromptVersion: window.__AW_TEST__.getState().selectedPromptVersion,
    }))()`);
    evidence.observations.afterOldHandshake = afterOldHandshake;
    assert(afterOldHandshake.state.viewerProtocol?.negotiated === false, "an old-connection hello negotiated the new connection", afterOldHandshake.state.viewerProtocol);
    assert(afterOldHandshake.selectedPromptVersion !== 9, "an old-connection snapshot was adopted as the current world state", afterOldHandshake);
    const recoveryFrom = (await countOutbound(driver, {})).total;
    await deliver(driver, helloAck, trace);
    await waitOutbound(driver, recoveryFrom, { type: "authoritative_recovery", mode: "reconnect_sync" }, 8000, trace);
    const unready = await driver.evalJson(`(() => {
      const apply = document.querySelector('[data-prompt-action="apply"]');
      return JSON.stringify({
        applyFound: Boolean(apply),
        applyEnabled: apply ? apply.disabled === false : false,
        registration: window.__AW_TEST__.getState().authRegistrationStatus,
        sessionEpoch: window.__AW_TEST__.getState().authSessionEpoch,
      });
    })()`);
    evidence.observations.beforeRebind = unready;
    assert(unready.applyEnabled === false, "prompt submission opened before the new connection restored the session binding", unready);
    const identity = await driver.evalJson("JSON.stringify(window.__BFR__.identity())");
    await deliver(driver, {
      type: "authoritative_recovery_ack",
      ack: { status: "catch_up_ready", player_id: identity.playerId, session_pubkey: identity.publicKey, agent_id: "agent-0", session_epoch: 13, binding_epoch: 10 },
    }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.authRegistrationStatus === "registered" && state.authBindingEpoch === 10 && state.authSessionEpoch === 13`);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot() }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.selectedKind === "agent" && state.selectedId === "agent-0"`);
    phaseLog.push("new connection restored hello, session/binding epochs and canonical snapshot");
    await deliverToSocket(driver, oldSocketId, { type: "snapshot", snapshot: promptSnapshot({ version: 9, override: "old-connection-secret", time: 99 }) }, trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterOldSnapshot = await promptResultState(driver);
    evidence.observations.afterOldSocketSnapshot = afterOldSnapshot;
    assert(afterOldSnapshot.selectedPromptVersion === 3, "an old-connection snapshot overwrote the new connection profile", afterOldSnapshot);
    assert(afterOldSnapshot.draftSystemPrompt !== "old-connection-secret", "an old-connection snapshot leaked into the current prompt draft", afterOldSnapshot);
    // The reconnect re-render recreates the advanced details element closed;
    // re-open it through the real summary control before submitting.
    await openAdvancedPromptDetails(driver, trace);
    await waitForState(driver, STATE_READER, `() => document.querySelector('[data-prompt-action="apply"]')?.disabled === false`, 8000);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const beforeB = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sentB = await waitOutbound(driver, beforeB.total, { type: "prompt_control", mode: "apply" });
    const requestB = sentB.entry.message.command.request;
    evidence.observations.recoveredRequest = { requestId: requestB.request_id, nonce: requestB.auth.nonce, sessionEpoch: requestB.session_epoch, bindingEpoch: requestB.binding_epoch };
    assert(requestB.request_id !== requestA.request_id, "new connection reused the stranded request identity", { A: requestA.request_id, B: requestB.request_id });
    assert(requestB.session_epoch === 13 && requestB.binding_epoch === 10, "new request did not use the recovered session/binding epochs", requestB);
    const sentFeedback = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterSend = sentFeedback;
    assert(sentFeedback.stage === "sent" && sentFeedback.requestId === requestB.request_id, "new request feedback did not follow its real send", sentFeedback);
    await deliverToSocket(driver, oldSocketId, promptControlError(requestA, "old-connection error for the pre-disconnect request"), trace);
    const afterOldError = await promptFeedbackState(driver);
    evidence.observations.afterOldSocketError = afterOldError;
    assert(afterOldError.stage === "sent" && afterOldError.requestId === requestB.request_id, "an old-connection result consumed the new request wait", afterOldError);
    await deliver(driver, promptControlApplied(requestB), trace);
    await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "applied"`);
    phaseLog.push("old-connection late events were ignored and the new request settled only on its own result");
    await artifacts.screenshot("final");
  },

  // P07: the real disposer stops DOM updates without logging out or closing the
  // shared transport; remounting reuses that transport and one visible click
  // still sends exactly one request with one prompt surface.
  "p07-unmount-remount": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const initial = await remountState(driver);
    evidence.observations.initial = initial;
    assert(initial.available === true && initial.mountCount === 1, "visual-test page did not expose the initial mount lifecycle", initial);
    assert(initial.detailsPanel === true && initial.applyControls === 1, "initial mount did not render one prompt surface", initial);
    const socketBefore = await currentSocketId(driver);
    const socketsBefore = await socketInfo(driver);
    await disposeViewer(driver, trace);
    const disposed = await remountState(driver);
    evidence.observations.disposed = disposed;
    assert(disposed.rootChildren === 0 && disposed.viewerShells === 0 && disposed.detailsPanel === false, "real disposer left a rendered viewer tree mounted", disposed);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 4, override: "post-dispose-prompt", time: 9 }) }, trace);
    await waitForState(driver, STATE_READER, `(state) => state.selectedPromptVersion === 4`, 6000);
    const disposedAfterUpdate = await remountState(driver);
    evidence.observations.disposedAfterUpdate = disposedAfterUpdate;
    assert(disposedAfterUpdate.viewerShells === 0 && disposedAfterUpdate.detailsPanel === false, "a disposed viewer updated DOM after a transport message", disposedAfterUpdate);
    const mountCount = await mountViewer(driver, trace);
    assert(mountCount === 2, "real remount did not run through the viewer mount entry", mountCount);
    await waitForState(driver, STATE_READER, `() => Boolean(document.querySelector(".viewer-shell"))`, 8000);
    // The pixel-world focus UI session outlives the component lifecycle, so a
    // remount restores cinematic focus. Leave it through the visible control
    // before exercising the command surface.
    // A replaced route target keeps the old URL fragment but Chrome does not
    // re-resolve :target for the recreated element, so the command route stays
    // hidden after a real remount. Re-enter the same in-page route the way the
    // shell's links do; this is hash navigation, not a reload.
    const routeState = await driver.evalJson(`(() => {
      window.location.hash = "#viewer-stage-panel";
      window.location.hash = "#viewer-details-panel";
      return JSON.stringify({ hash: window.location.hash, route: document.getElementById("viewer-details-panel")?.getAttribute("data-route-open") ?? null });
    })()`);
    evidence.observations.routeAfterRemount = routeState;
    await waitForState(driver, STATE_READER, `() => getComputedStyle(document.getElementById("viewer-details-panel")).display !== "none"`, 5000);
    await ensurePromptEditorVisible(driver, trace);
    await ensurePromptEditorVisible(driver, trace);
    await ensurePromptEditorVisible(driver, trace);
    const remounted = await remountState(driver);
    evidence.observations.remounted = remounted;
    assert(remounted.viewerShells === 1 && remounted.promptPanels === 1 && remounted.applyControls === 1, "remount duplicated the viewer or prompt surface", remounted);
    assert(remounted.mountCount === 2, "remount count did not advance exactly once", remounted);
    const socketAfter = await currentSocketId(driver);
    const socketsAfter = await socketInfo(driver);
    evidence.observations.transport = { socketBefore, socketAfter, socketsBefore, socketsAfter };
    assert(socketAfter === socketBefore, "dispose/remount replaced the shared transport socket", { socketBefore, socketAfter });
    const currentAfter = (socketsAfter || []).find((entry) => entry.id === socketAfter);
    assert(currentAfter?.readyState === 1, "dispose/remount closed the current shared transport socket", socketsAfter);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    evidence.observations.request = { requestId: request.request_id, nonce: request.auth.nonce, expectedVersion: request.expected_version };
    assert(request.expected_version === 4, "remounted visible form did not carry the current prompt version", request.expected_version);
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 1, "one visible click after remount produced duplicate requests", totals);
    await deliver(driver, promptControlApplied(request), trace);
    await waitForState(driver, STATE_READER, `(state) => state.lastPromptFeedback?.stage === "applied"`);
    phaseLog.push("single visible click after remount sent one request and settled on its correlated result");
    await artifacts.screenshot("final");
  },
};

// ---- artifact helpers ---------------------------------------------------

function createArtifactWriter(outDir, caseId, viewportName, screenshotFn, trace = null) {
  const dir = join(outDir, caseId, viewportName);
  mkdirSync(dir, { recursive: true });
  const written = [];
  return {
    dir,
    screenshot(name) {
      const path = join(dir, `${name}.png`);
      written.push(path);
      trace?.push("screenshot", { name, path });
      return screenshotFn(path);
    },
    writeJson(name, value) {
      const path = join(dir, name);
      writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
      written.push(path);
      return path;
    },
    writeText(name, value) {
      const path = join(dir, name);
      writeFileSync(path, value);
      written.push(path);
      return path;
    },
    written,
  };
}

export function describeArtifacts(written) {
  return written.map((artifactPath) => {
    const { bytes, sha256 } = fingerprintRegularFile(artifactPath);
    const repoRelative = relative(repoRoot, artifactPath).split("\\").join("/");
    return {
      path: repoRelative.startsWith("..") ? artifactPath : repoRelative,
      bytes,
      sha256: bytes > 0 ? sha256 : null,
    };
  });
}

async function runCase({ caseId, viewportName, options, port, summary }) {
  const outDir = options.outDir;
  const viewport = EXPECTED_VIEWPORTS[viewportName];
  const driver = createBrowserDriver({ sessionPrefix: `bfr-${caseId}-${viewportName}`, headed: options.headed, outDir });
  const trace = createScenarioTrace();
  const artifacts = createArtifactWriter(outDir, caseId, viewportName, (path) => driver.screenshot(path), trace);
  const phaseLog = [];
  const evidence = { observations: {}, artifacts: [] };
  const startedAt = new Date().toISOString();
  const record = { caseId, viewport: { name: viewportName, ...viewport }, startedAt, status: "running", phaseLog, evidence, assertions: [] };
  try {
    driver.prepare();
    await driver.runJson(["set", "viewport", String(viewport.width), String(viewport.height)]);
    if (options.chromeTrace) await driver.runJson(["trace", "start", join(artifacts.dir, "chrome-trace.json")]);
    const hash = caseId.startsWith("p01") ? "#viewer-targets-panel" : "#viewer-details-panel";
    await driver.runJson(["open", caseUrl(port, { hash })]);
    trace.push("page-open", { url: caseUrl(port, { hash }) });
    await waitForState(driver, STATE_READER, `(state) => state.connectionStatus === "connecting" || state.connectionStatus === "connected"`, 8000);
    const pageEnv = await driver.evalJson(`(() => JSON.stringify({
      userAgent: navigator.userAgent,
      testApi: typeof window.__AW_TEST__,
      transportShim: typeof window.__BFR__,
      secureContext: window.isSecureContext,
      width: window.innerWidth,
      height: window.innerHeight,
      viewerShells: document.querySelectorAll("#app .viewer-shell").length,
    }))()`);
    evidence.observations.pageEnvironment = pageEnv;
    assert(pageEnv.testApi !== "undefined", "visual-test bundle did not install the read-only test API");
    assert(pageEnv.transportShim !== "undefined", "visual-test bundle did not install the fixture transport");
    assert(pageEnv.secureContext, "loopback fixture page must be a secure context for WebCrypto");
    assert(pageEnv.width === viewport.width && pageEnv.height === viewport.height, "declared viewport was not applied to the browser", { declared: viewport, actual: { width: pageEnv.width, height: pageEnv.height } });
    const storage = await driver.evalJson("JSON.stringify({ keys: window.__BFR__.storageKeys() })");
    evidence.observations.initialStorageKeys = storage.keys;
    assert(storage.keys.every((key) => key.startsWith("oasis7.")), "private context leaked unexpected storage keys before the scenario ran", storage.keys);
    const guard = await driver.evalJson("window.__BFR__.networkGuardProbes().then((value) => value)");
    evidence.observations.networkGuard = guard;
    assert(guard.results.http !== "not-blocked", "unknown HTTP request was not blocked before reaching the network");
    assert(guard.results.websocket !== "not-blocked", "unknown WebSocket was not blocked before connecting");
    assert(guard.results.worker !== "not-blocked", "unknown worker was not blocked before loading");
    assert(!guard.location.includes("example.invalid"), "cross-origin navigation was not blocked", guard.location);
    const guardKinds = (guard.violations || []).map((entry) => entry.kind).sort();
    assert(["http", "navigation", "websocket", "worker"].every((kind) => guardKinds.includes(kind)), "network guard did not record every probe attempt", guardKinds);
    await CASES[caseId]({ driver, phaseLog, evidence, artifacts, viewport, trace });
    if (options.chromeTrace) await driver.runJson(["trace", "stop", join(artifacts.dir, "chrome-trace.json")]);
    const consoleMessages = await driver.runJson(["console"]);
    const pageErrors = await driver.runJson(["errors"]);
    const network = await driver.runJson(["network", "requests"]);
    const sessionInfo = await driver.runJson(["session", "info"]);
    evidence.artifacts.push(artifacts.writeJson("console.json", consoleMessages));
    evidence.artifacts.push(artifacts.writeJson("page-errors.json", pageErrors));
    evidence.artifacts.push(artifacts.writeJson("network.json", network));
    evidence.artifacts.push(artifacts.writeJson("browser-session.json", sessionInfo));
    const structured = await driver.evalJson("JSON.stringify(window.__BFR__.snapshot())");
    evidence.artifacts.push(artifacts.writeJson("scenario-trace.json", structured));
    evidence.artifacts.push(artifacts.writeText("structured-trace.jsonl", trace.jsonl()));
    const errorEntries = (pageErrors.errors || []).filter(Boolean);
    assert(errorEntries.length === 0, "unexpected page errors were recorded", errorEntries.slice(0, 5));
    const consoleErrors = (consoleMessages.messages || []).filter((entry) => String(entry?.type || "").toLowerCase() === "error");
    assert(consoleErrors.length === 0, "unexpected console errors were recorded without a per-case allowlist", consoleErrors.map((entry) => entry.text).slice(0, 5));
    const failedRequests = (network.requests || []).filter((entry) => !entry.status || entry.status >= 400);
    assert(failedRequests.length === 0, "unexpected failed network requests were recorded", failedRequests.map((entry) => ({ url: entry.url, status: entry.status })).slice(0, 5));
    const foreignRequests = (network.requests || []).filter((entry) => !String(entry.url || "").startsWith(`http://127.0.0.1:${port}/`));
    assert(foreignRequests.length === 0, "the browser reached a network origin outside the private loopback fixture", foreignRequests.map((entry) => entry.url).slice(0, 5));
    const delivered = structured.delivered || [];
    const outbound = structured.outbound || [];
    evidence.observations.requestCounts = {
      outboundTotal: outbound.length,
      promptControl: outbound.filter((entry) => entry.message?.type === "prompt_control").length,
      deliveredTotal: delivered.length,
      promptControlAcks: delivered.filter((entry) => entry.message?.type === "prompt_control_ack").length,
      promptControlErrors: delivered.filter((entry) => entry.message?.type === "prompt_control_error").length,
      networkRequests: (network.requests || []).length,
    };
    record.artifacts = describeArtifacts(artifacts.written);
    const screenshotArtifacts = record.artifacts.filter((artifact) => artifact.path.endsWith(".png"));
    assert(screenshotArtifacts.length > 0, "case produced no failure/state screenshots");
    for (const artifact of screenshotArtifacts) {
      assert(artifact.bytes > 0, `screenshot artifact is empty: ${artifact.path}`);
    }
    const storageAfter = await driver.evalJson("JSON.stringify({ keys: window.__BFR__.storageKeys() })");
    evidence.observations.finalStorageKeys = storageAfter.keys;
    record.status = "passed";
  } catch (error) {
    record.status = "failed";
    record.failure = String(error && error.stack ? error.stack : error);
    try { trace.push("failed", { message: String(error && error.message ? error.message : error) }); } catch (_) {}
    try { await artifacts.screenshot("failure"); } catch (_) {}
    try {
      record.failureDom = await driver.evalJson(`(() => JSON.stringify({
        detailsPanel: document.getElementById("viewer-details-panel")?.innerText?.slice(0, 3000) || null,
        hasAdvancedDetails: Boolean(document.querySelector("#viewer-details-panel details.command-surface__advanced-details")),
        promptActionCount: document.querySelectorAll("[data-prompt-action]").length,
        visibilityToggle: Boolean(document.querySelector('[data-prompt-visibility-toggle="1"]')),
        promptPanel: Boolean(document.querySelector(".command-surface__prompt-panel")),
        interactionPanel: Boolean(document.querySelector("#viewer-details-panel .command-surface")),
      }))()`);
    } catch (evaluationError) {
      record.failureDom = { error: String(evaluationError) };
    }
    for (const [name, args] of [
      ["console.json", ["console"]],
      ["page-errors.json", ["errors"]],
      ["network.json", ["network", "requests"]],
    ]) {
      try { evidence.artifacts.push(artifacts.writeJson(name, await driver.runJson(args))); } catch (_) {}
    }
    try {
      evidence.artifacts.push(artifacts.writeJson("scenario-trace.json", await driver.evalJson("JSON.stringify(window.__BFR__.snapshot())")));
    } catch (_) {}
    try { evidence.artifacts.push(artifacts.writeText("structured-trace.jsonl", trace.jsonl())); } catch (_) {}
    record.artifacts = describeArtifacts(artifacts.written);
  } finally {
    try {
      if (options.chromeTrace) await driver.run(["trace", "stop", join(artifacts.dir, "chrome-trace.json")]);
      record.browserSession = driver.session;
      record.cleanup = driver.closeWithVerification();
      if (!record.cleanup.closed) throw new Error(record.cleanup.error);
    } catch (error) {
      record.cleanup = { ...(record.cleanup || {}), closed: false, error: String(error) };
      record.status = "failed";
      record.failure = `cleanup failed: ${String(error && error.message ? error.message : error)}`;
    }
    record.finishedAt = new Date().toISOString();
  }
  summary.cases.push(record);
  return record;
}

// ---- self checks --------------------------------------------------------

async function runSelfChecks({ outDir, serverUrl, options }) {
  const checks = [];
  const check = async (name, fn) => {
    try { await fn(); checks.push({ name, status: "passed" }); }
    catch (error) { checks.push({ name, status: "failed", failure: String(error && error.message ? error.message : error) }); }
  };
  const expectFailure = async (fn, label) => {
    let failed = false;
    try { await fn(); } catch { failed = true; }
    if (!failed) throw new Error(`${label} was accepted but must fail`);
  };
  await check("empty-case-selection", () => expectFailure(async () => selectCases([]), "empty case selection"));
  await check("unknown-case-id", () => expectFailure(async () => selectCases(["p99-not-a-case"]), "unknown case id"));
  await check("duplicate-case-selection", () => expectFailure(async () => selectCases(["p01-loading", "p01-loading"]), "duplicate case selection"));
  await check("duplicate-viewport-selection", () => expectFailure(async () => selectViewports(["desktop", "desktop"]), "duplicate viewport selection"));
  await check("missing-browser-binary", () => expectFailure(async () => resolveBrowserBinary("/nonexistent/agent-browser-for-bfr"), "missing browser binary"));
  await check("host-deliver-propagates-barrier-failure", () => expectFailure(async () => {
    const fakeDriver = {
      evalJson: async (source) => {
        assert(String(source).includes("window.__BFR__.deliver("), "host deliver must await the page-side delivery promise");
        return { ok: false, error: "no open fixture websocket" };
      },
    };
    await deliver(fakeDriver, { type: "snapshot", snapshot: {} });
  }, "host delivery with an unsettled barrier"));
  await check("artifact-write-failure", async () => {
    const path = join(outDir, "self-check", "directory-as-file");
    mkdirSync(path, { recursive: true });
    await expectFailure(async () => writeFileSync(path, "x"), "writing an artifact over a directory");
  });
  await check("timeout-surfaces-as-failure", async () => {
    const driver = createBrowserDriver({ sessionPrefix: `bfr-timeout-${Date.now()}`, headed: options.headed, outDir });
    try {
      driver.prepare();
      await driver.runJson(["set", "viewport", "800", "600"]);
      await driver.runJson(["open", caseUrl(serverUrl.port, { hash: "#viewer-details-panel" })]);
      await driver.evalJson(`window.__BFR__.nextOutbound(0, { type: "prompt_control" }, 250).then((value) => JSON.stringify(value))`).then((result) => {
        if (result.ok) throw new Error("timeout probe unexpectedly observed a prompt request");
      });
    } finally {
      driver.close();
    }
  });
  await check("delivery-barrier-times-out-without-open-socket", async () => {
    const driver = createBrowserDriver({ sessionPrefix: `bfr-deliver-timeout-${Date.now()}`, headed: options.headed, outDir });
    try {
      driver.prepare();
      await driver.runJson(["open", `http://127.0.0.1:${serverUrl.port}/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en`]);
      await waitForState(driver, STATE_READER, `() => typeof window.__BFR__ === "object"`, 8000);
      const result = await driver.evalJson(deliveryProbeSource({ type: "snapshot", snapshot: {} }, 300));
      assert(result.ok === false, "delivery without an open fixture socket was reported as delivered", result);
      assert(/no open fixture websocket/.test(result.error || ""), "delivery timeout did not surface the socket deadline", result);
    } finally {
      driver.close();
    }
  });
  await check("old-socket-delivery-requires-a-known-fixture-socket", async () => {
    const driver = createBrowserDriver({ sessionPrefix: `bfr-unknown-socket-${Date.now()}`, headed: options.headed, outDir });
    try {
      driver.prepare();
      await driver.runJson(["open", `http://127.0.0.1:${serverUrl.port}/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en`]);
      await waitForState(driver, STATE_READER, `() => typeof window.__BFR__ === "object"`, 8000);
      const result = await driver.evalJson(`JSON.stringify(window.__BFR__.deliverToSocket(9999, { type: "snapshot", snapshot: {} }))`);
      assert(result.ok === false, "an unknown fixture socket was reported as delivered", result);
      assert(/unknown fixture socket/.test(result.reason || ""), "unknown socket delivery did not surface its reason", result);
    } finally {
      driver.close();
    }
  });
  await check("remount-lifecycle-requires-explicit-test-api", async () => {
    const driver = createBrowserDriver({ sessionPrefix: `bfr-remount-guard-${Date.now()}`, headed: options.headed, outDir });
    try {
      driver.prepare();
      await driver.runJson(["open", `http://127.0.0.1:${serverUrl.port}/viewer.html?connect=0&hosted_bootstrap=0&locale=en`]);
      const observed = await driver.evalJson(`(() => JSON.stringify({
        remountApi: typeof window.__BFR_REMOUNT__,
        viewerShell: Boolean(document.querySelector(".viewer-shell")),
      }))()`);
      assert(observed.remountApi === "undefined", "the remount lifecycle leak is exposed without test_api=1", observed);
      assert(observed.viewerShell === true, "the no-test-api page did not mount the real viewer", observed);
    } finally {
      driver.close();
    }
  });
  await check("delivery-barrier-settles-after-page-delivery", async () => {
    const driver = createBrowserDriver({ sessionPrefix: `bfr-deliver-settles-${Date.now()}`, headed: options.headed, outDir });
    try {
      driver.prepare();
      await driver.runJson(["open", `http://127.0.0.1:${serverUrl.port}/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en`]);
      await waitForState(driver, STATE_READER, `() => typeof window.__BFR__ === "object"`, 8000);
      const probe = await driver.evalJson(`(() => {
        new WebSocket(window.__BFR__.endpoint);
        return ${deliveryProbeSource({ type: "bfr-self-check-delivery" }, 3000)}.then((raw) => {
          const settled = JSON.parse(raw);
          const delivered = window.__BFR__.delivered;
          return JSON.stringify({
            settled,
            total: delivered.length,
            lastType: delivered[delivered.length - 1]?.message?.type || null,
          });
        });
      })()`);
      assert(probe.settled.ok === true, "delivery with an open fixture socket did not settle", probe);
      assert(probe.total >= 1, "delivery settled before the message was recorded", probe);
      assert(probe.lastType === "bfr-self-check-delivery", "delivery settled before the message reached the fixture socket", probe);
    } finally {
      driver.close();
    }
  });
  const failed = checks.filter((entry) => entry.status !== "passed");
  const path = join(outDir, "self-check.json");
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify({ checks, failures: failed.length }, null, 2)}\n`);
  return { checks, failures: failed.length, path };
}

// ---- build and summary --------------------------------------------------

function buildBundles(outDir) {
  const logs = [];
  for (const [name, args] of [
    ["visual-test", ["vite", "build", "--mode", "visual-test", "--config", "vite.software-safe.config.mjs"]],
    ["release", ["vite", "build", "--config", "vite.software-safe.config.mjs"]],
  ]) {
    const result = spawnSync("npx", args, { cwd: viewerRoot, encoding: "utf8", timeout: 300_000 });
    logs.push({ name, status: result.status, stdout: result.stdout || "", stderr: result.stderr || "" });
    if (result.status !== 0) {
      writeFileSync(join(outDir, `build-${name}.log`), `${result.stdout || ""}${result.stderr || ""}`);
      throw new Error(`viewer ${name} bundle build failed; see build-${name}.log`);
    }
  }
  writeFileSync(join(outDir, "build.log"), logs.map((entry) => `## ${entry.name}\n${entry.stdout}${entry.stderr}`).join("\n"));
  return logs;
}

function writeSummary(outDir, summary) {
  const jsonPath = join(outDir, "summary.json");
  writeFileSync(jsonPath, `${JSON.stringify(summary, null, 2)}\n`);
  const lines = [
    "# Browser functional regression (fixture-functional) summary",
    "",
    `- status: ${summary.status}`,
    `- startedAt: ${summary.startedAt}`,
    `- finishedAt: ${summary.finishedAt}`,
    `- command: ${summary.command}`,
    `- browser: agent-browser ${summary.environment.agentBrowserVersion}`,
    `- bundle: ${summary.environment.bundleProfile}`,
    `- viewports: ${summary.viewports.map((viewport) => `${viewport.name}=${viewport.width}x${viewport.height}`).join(", ")}`,
    "",
    "| case | viewport | status | artifacts | prompt requests |",
    "| --- | --- | --- | --- | --- |",
    ...summary.cases.map((entry) => `| ${entry.caseId} | ${entry.viewport.name} | ${entry.status} | ${(entry.artifacts || []).length} | ${entry.evidence.observations.requestCounts?.promptControl ?? "-"} |`),
    "",
    `- coverage: ${summary.executed?.partialMatrix ? "partial matrix" : "declared P01-P07 matrix"} (${summary.executed?.cases ?? 0}/${summary.executed?.expected ?? 0} runs)`,
    `- release-controller-probe: ${summary.releaseProbe?.status ?? "not-run"}`,
    `- runner-negative-self-check: ${summary.selfCheck ? (summary.selfCheck.failures === 0 ? "passed" : "failed") : "not-run"}`,
    `- failures: ${summary.failures.length}`,
    ...summary.failures.map((failure) => `  - ${failure.caseId}/${failure.viewport.name}: ${String(failure.failure || "").split("\n")[0]}`),
  ];
  const mdPath = join(outDir, "summary.md");
  writeFileSync(mdPath, `${lines.join("\n")}\n`);
  return { jsonPath, mdPath };
}

// ---- main ---------------------------------------------------------------

async function main() {
  // Each case owns a short-lived browser session with its own cleanup signal
  // hooks; this runner can execute a dozen in one process.
  process.setMaxListeners(64);
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    usage();
    process.exit(2);
    return;
  }
  mkdirSync(options.outDir, { recursive: true });
  const startedAt = new Date().toISOString();
  const summary = {
    caseId: "BFR-P01-P07",
    profile: "fixture-functional",
    command: `node crates/oasis7_viewer/scripts/browser-functional-regression.mjs ${process.argv.slice(2).join(" ")}`.trim(),
    startedAt,
    status: "running",
    viewports: options.viewports.map((name) => ({ name, ...EXPECTED_VIEWPORTS[name] })),
    environment: {},
    cases: [],
    failures: [],
    coverage: {
      covered: [
        "BFR-P01 loading",
        "BFR-P01 empty",
        "BFR-P02 unready-no-send",
        "BFR-P02 accepted/applied",
        "BFR-P03 rejected",
        "BFR-P03 stale",
        "BFR-P03 correlated control-lost receipt",
        "BFR-P03 hidden blocked runtime error",
        "BFR-P04 deferred signing + sent late results",
        "BFR-P05 duplicate and out-of-order results",
        "BFR-P06 reconnect, new connection generation and old-socket isolation",
        "BFR-P07 real dispose/remount single-send",
      ],
      notCovered: [
        "real auth service/provider/runtime/world verification",
        "canonical receipt and world delta",
        "cross-browser/platform matrix beyond the selected Chromium viewports",
        "production StrongAuth/real signing identity",
      ],
    },
  };
  let serverHandle = null;
  let releaseHandle = null;
  try {
    const browserBinary = process.env.AGENT_BROWSER_BIN || "agent-browser";
    const version = spawnSync(browserBinary, ["--version"], { encoding: "utf8" });
    const gitHead = (() => {
      try { return execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim(); }
      catch { return null; }
    })();
    const gitDirty = (() => {
      try { return execFileSync("git", ["status", "--porcelain"], { cwd: repoRoot, encoding: "utf8" }).trim().length > 0; }
      catch { return null; }
    })();
    summary.environment = {
      agentBrowserVersion: String(version.stdout || "").trim().replace(/^agent-browser\s+/i, "") || "unknown",
      node: process.version,
      platform: process.platform,
      arch: process.arch,
      bundleProfile: "visual-test",
      chromeTraceEnabled: options.chromeTrace,
      gitHead,
      gitDirty,
      replacementBoundaries: {
        transport: "window.WebSocket replaced in the test-only page with a bounded fixture controller; production request parsing, identity checks, timers, state transitions and rendering unchanged",
        initialSessionAuth: "local_test_api_ephemeral lane issued by production viewer_auth_crypto (real Ed25519 WebCrypto); no real provider, launcher or chain identity",
        bridgeFeed: "pixel-world bridge module served from real software_safe_src/pixel_world_bevy_bridge.js with a deterministic test render-state projection; bridge selection only, no Runtime simulation",
        httpOrigin: "loopback static asset server on a per-case dynamic port; non-loopback HTTP/WebSocket/worker/navigation blocked before send",
        controls: "read-only __AW_TEST__ observation only; all player actions are real agent-browser clicks/typing on visible controls",
      },
      runner: fileFingerprint("crates/oasis7_viewer/scripts/browser-functional-regression.mjs"),
      productionModules: PRODUCTION_MODULES.map(fileFingerprint),
    };
    if (!options.skipBuild) {
      summary.build = buildBundles(options.outDir);
    }
    assert(existsSync(visualTestBundlePath), `missing visual-test bundle at ${visualTestBundlePath}`);
    assert(existsSync(releaseBundlePath), `missing release bundle at ${releaseBundlePath}`);
    summary.environment.bundles = {
      visualTest: fileFingerprint("crates/oasis7_viewer/.software-safe-test-build/viewer.js"),
      release: fileFingerprint("crates/oasis7_viewer/.software-safe-build/viewer.js"),
    };
    serverHandle = await createFixtureServer();
    releaseHandle = await createFixtureServer({ releaseOnly: true });
    summary.environment.fixtureServerPort = serverHandle.port;
    summary.environment.releaseProbePort = releaseHandle.port;
    summary.releaseProbe = await runReleaseProbe({ releasePort: releaseHandle.port, options });
    summary.failures.push(...summary.releaseProbe.failures.map((failure) => ({ caseId: "release-probe", viewport: { name: "desktop" }, failure })));
    if (options.selfCheck) {
      const selfCheck = await runSelfChecks({ outDir: options.outDir, serverUrl: serverHandle, options });
      summary.selfCheck = selfCheck;
      summary.failures.push(...selfCheck.checks.filter((entry) => entry.status !== "passed").map((entry) => ({ caseId: "self-check", viewport: { name: "n/a" }, failure: `${entry.name}: ${entry.failure || ""}` })));
    } else {
      summary.selfCheck = await runSelfChecks({ outDir: options.outDir, serverUrl: serverHandle, options });
      summary.failures.push(...summary.selfCheck.checks.filter((entry) => entry.status !== "passed").map((entry) => ({ caseId: "self-check", viewport: { name: "n/a" }, failure: `${entry.name}: ${entry.failure || ""}` })));
      for (const viewportName of options.viewports) {
        for (const caseId of options.cases) {
          const caseServer = await createFixtureServer();
          try {
            const record = await runCase({ caseId, viewportName, options, port: caseServer.port, summary });
            record.fixturePort = caseServer.port;
            if (record.status !== "passed") summary.failures.push({ caseId, viewport: record.viewport, failure: record.failure });
          } finally {
            caseServer.server.close();
          }
        }
      }
    }
  } catch (error) {
    summary.failures.push({ caseId: "runner", viewport: { name: "n/a" }, failure: String(error && error.stack ? error.stack : error) });
  } finally {
    serverHandle?.server?.close();
    releaseHandle?.server?.close();
  }
  const executed = summary.cases.length;
  const uniqueCases = [...new Set(options.cases)];
  const uniqueViewports = [...new Set(options.viewports)];
  const expectedRuns = uniqueViewports.length * uniqueCases.length;
  const uniquePairs = new Set(summary.cases.map((entry) => `${entry.caseId}/${entry.viewport.name}`)).size;
  if (!options.selfCheck && (executed !== expectedRuns || uniquePairs !== expectedRuns)) {
    summary.failures.push({ caseId: "runner", viewport: { name: "n/a" }, failure: `zero/incomplete execution: executed=${executed} uniquePairs=${uniquePairs} expected=${expectedRuns}; a partial, duplicated or empty run cannot pass` });
  }
  summary.executed = {
    cases: executed,
    expected: options.selfCheck ? 0 : expectedRuns,
    uniquePairs,
    partialMatrix: uniqueCases.length !== ALL_CASES.length || uniqueViewports.length !== Object.keys(EXPECTED_VIEWPORTS).length,
  };
  summary.status = summary.failures.length !== 0 || !summary.cases.every((entry) => entry.status === "passed")
    ? "failed"
    : options.selfCheck ? "self_check_passed" : "passed";
  summary.finishedAt = new Date().toISOString();
  const paths = writeSummary(options.outDir, summary);
  if (!options.json) console.log(`summary: ${paths.mdPath}`);
  else console.log(JSON.stringify(summary));
  process.exit(["passed", "self_check_passed"].includes(summary.status) ? 0 : 1);
}

async function runReleaseProbe({ releasePort, options }) {
  const failures = [];
  const driver = createBrowserDriver({ sessionPrefix: `bfr-release-${Date.now()}`, headed: options.headed, outDir: options.outDir });
  let observed = null;
  try {
    driver.prepare();
    await driver.runJson(["set", "viewport", "1440", "1000"]);
    await driver.runJson(["open", `http://127.0.0.1:${releasePort}/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en`]);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 800));
    observed = await driver.evalJson(`(() => JSON.stringify({
      testApi: typeof window.__AW_TEST__,
      transportShim: typeof window.__BFR__,
      remountApi: typeof window.__BFR_REMOUNT__,
      viewerMounted: Boolean(document.querySelector(".viewer-shell")),
      query: location.search,
    }))()`);
    if (observed.testApi !== "undefined") failures.push("release bundle exposed the test API with test_api=1");
    if (observed.transportShim !== "undefined") failures.push("release bundle page installed a transport controller");
    if (observed.remountApi !== "undefined") failures.push("release bundle page exposed the remount lifecycle controller");
    if (!observed.viewerMounted) failures.push("release probe page did not mount the real viewer shell");
  } catch (error) {
    failures.push(String(error && error.stack ? error.stack : error));
  } finally {
    driver.close();
  }
  return { status: failures.length === 0 ? "passed" : "failed", observed, failures };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
