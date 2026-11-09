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
import { createHash } from "node:crypto";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join, normalize, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";

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

export function selectCases(raw) {
  const selected = raw.map((value) => value.trim()).filter(Boolean);
  if (selected.length === 0) throw new Error("case selection is empty; a run must select at least one case");
  for (const caseId of selected) {
    if (!SELECTABLE_CASES.includes(caseId)) throw new Error(`unknown case id: ${caseId}`);
  }
  return selected;
}

function selectViewports(raw) {
  const selected = raw.map((value) => value.trim()).filter(Boolean);
  if (selected.length === 0) throw new Error("viewport selection is empty; a run must select at least one viewport");
  for (const name of selected) {
    if (!EXPECTED_VIEWPORTS[name]) throw new Error(`unknown viewport: ${name}`);
  }
  return selected;
}

function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function fileFingerprint(relPath) {
  const absPath = resolve(repoRoot, relPath);
  if (!existsSync(absPath)) return { path: relPath, missing: true };
  return { path: relPath, bytes: statSync(absPath).size, sha256: sha256File(absPath) };
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
    closeSocket() { const socket = currentSocket(); if (socket) socket.close(); },
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

function createFixtureServer({ releaseOnly = false } = {}) {
  const server = createServer((request, response) => {
    let url;
    try { url = new URL(request.url || "/", "http://127.0.0.1"); }
    catch { response.writeHead(400); response.end("bad request"); return; }
    const port = server.address().port;
    const pathname = decodeURIComponent(url.pathname === "/" ? "/viewer.html" : url.pathname);
    const wsEndpoint = `ws://127.0.0.1:${port}/viewer-ws`;
    const config = `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify({ deploymentMode: "trusted_local_only", viewerWsEndpoint: wsEndpoint })}</script>`;
    if (pathname === "/viewer.js") {
      const bundle = releaseOnly ? releaseBundlePath : visualTestBundlePath;
      if (!existsSync(bundle)) { response.writeHead(500); response.end("missing bundle"); return; }
      response.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" });
      response.end(readFileSync(bundle));
      return;
    }
    if (pathname === "/pixel-world-bridge/pixel_world_bridge.js") {
      response.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" });
      response.end(bridgeModuleSource);
      return;
    }
    if (pathname === "/viewer.html") {
      let html = readFileSync(resolve(viewerRoot, "viewer.html"), "utf8");
      const injected = releaseOnly
        ? `${config}`
        : `${config}<script>${canvasCompatSource}</script><script>${deriveSource()}</script><script>${fakeTransportSource(wsEndpoint)}</script>`;
      html = html.replace('<script type="module" src="./viewer.js"></script>', `${injected}<script type="module" src="./viewer.js"></script>`);
      response.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
      response.end(html);
      return;
    }
    const filePath = resolveStatic(pathname);
    if (!filePath) { response.writeHead(403); response.end("forbidden"); return; }
    try {
      if (!statSync(filePath).isFile()) throw new Error("not a file");
      response.writeHead(200, { "Content-Type": contentTypeFor(filePath), "Cache-Control": "no-store" });
      response.end(readFileSync(filePath));
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

  return { binary, session, run, runJson, evalJson, screenshot, close, closeWithVerification, prepare: lifecycle.prepare };
}

export function resolveBrowserBinary(candidate) {
  if (!candidate) throw new Error("agent-browser binary is required");
  const probe = spawnSync("sh", ["-c", `command -v ${JSON.stringify(candidate)}`], { encoding: "utf8" });
  if (probe.status !== 0 && !existsSync(candidate)) {
    throw new Error(`required browser automation binary is unavailable: ${candidate}`);
  }
  return candidate;
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

async function deliver(driver, message, trace = null) {
  trace?.push("deliver", { type: message.type, status: message?.ack?.status || message?.error?.status || null });
  return driver.evalJson(`(() => JSON.stringify({ delivered: window.__BFR__.deliver(${JSON.stringify(message)}) }))()`);
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

async function openAdvancedPromptDetails(driver, trace = null) {
  const summarySelector = "#viewer-details-panel details.command-surface__advanced-details > summary";
  await driver.runJson(["scrollintoview", summarySelector]);
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await driver.runJson(["click", summarySelector]);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 120));
    const opened = await driver.evalJson(`(() => JSON.stringify({ open: document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true }))()`);
    if (opened.open) {
      trace?.push("ui-click", { selector: summarySelector, open: true, attempt });
      return;
    }
  }
  throw new Error("advanced prompt settings did not open from the visible summary control");
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

async function clickApply(driver, trace = null) {
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
    const screenshotArtifacts = artifacts.written.filter((artifactPath) => artifactPath.endsWith(".png"));
    assert(screenshotArtifacts.length > 0, "case produced no failure/state screenshots");
    for (const artifactPath of screenshotArtifacts) {
      assert(statSync(artifactPath).size > 0, `screenshot artifact is empty: ${artifactPath}`);
    }
    record.artifacts = artifacts.written.map((artifactPath) => {
      const bytes = statSync(artifactPath).size;
      const repoRelative = relative(repoRoot, artifactPath).split("\\").join("/");
      return {
        path: repoRelative.startsWith("..") ? artifactPath : repoRelative,
        bytes,
        sha256: bytes > 0 ? sha256File(artifactPath) : null,
      };
    });
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
  await check("missing-browser-binary", () => expectFailure(async () => resolveBrowserBinary("/nonexistent/agent-browser-for-bfr"), "missing browser binary"));
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
    `- coverage: ${summary.executed?.partialMatrix ? "partial matrix" : "declared P01-P03 matrix"} (${summary.executed?.cases ?? 0}/${summary.executed?.expected ?? 0} runs)`,
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
    caseId: "BFR-P01-P03",
    profile: "fixture-functional",
    command: `node crates/oasis7_viewer/scripts/browser-functional-regression.mjs ${process.argv.slice(2).join(" ")}`.trim(),
    startedAt,
    status: "running",
    viewports: options.viewports.map((name) => ({ name, ...EXPECTED_VIEWPORTS[name] })),
    environment: {},
    cases: [],
    failures: [],
    coverage: {
      covered: ["BFR-P01 loading", "BFR-P01 empty", "BFR-P02 unready-no-send", "BFR-P02 accepted/applied", "BFR-P03 rejected", "BFR-P03 stale", "BFR-P03 correlated control-lost receipt", "BFR-P03 hidden blocked runtime error"],
      notCovered: ["BFR-P04 late results", "BFR-P05 duplicate/out-of-order", "BFR-P06 reconnect recovery", "BFR-P07 unmount/remount", "CI selector wiring", "real auth/provider/runtime/world verification"],
    },
  };
  let serverHandle = null;
  let releaseHandle = null;
  try {
    const version = spawnSync("agent-browser", ["--version"], { encoding: "utf8" });
    const gitHead = (() => {
      try { return execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim(); }
      catch { return null; }
    })();
    const gitDirty = (() => {
      try { return execFileSync("git", ["status", "--porcelain"], { cwd: repoRoot, encoding: "utf8" }).trim().length > 0; }
      catch { return null; }
    })();
    summary.environment = {
      agentBrowserVersion: (version.stdout || "").trim() || "unknown",
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
  const expectedRuns = options.viewports.length * options.cases.length;
  if (!options.selfCheck && executed !== expectedRuns) {
    summary.failures.push({ caseId: "runner", viewport: { name: "n/a" }, failure: `zero/incomplete execution: executed=${executed} expected=${expectedRuns}; a partial or empty run cannot pass` });
  }
  summary.executed = { cases: executed, expected: options.selfCheck ? 0 : expectedRuns, partialMatrix: options.cases.length !== ALL_CASES.length || options.viewports.length !== Object.keys(EXPECTED_VIEWPORTS).length };
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
      viewerMounted: Boolean(document.querySelector(".viewer-shell")),
      query: location.search,
    }))()`);
    if (observed.testApi !== "undefined") failures.push("release bundle exposed the test API with test_api=1");
    if (observed.transportShim !== "undefined") failures.push("release bundle page installed a transport controller");
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