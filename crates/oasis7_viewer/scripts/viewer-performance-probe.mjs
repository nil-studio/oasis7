import { createServer } from "node:http";
import { appendFileSync, cpSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join, normalize, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { densityScript, installSnapshotFeedScript, nativeCameraGesture } from "./viewer-performance-interaction-window.mjs";
import { installPaintTraceScript, matchInputPaintTrace } from "./viewer-performance-paint-trace.mjs";
import { collectNativeSelectionSamples } from "./viewer-performance-native-input.mjs";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";
import {
  VIEWER_PERF_PROFILES,
  buildViewerPerformanceMarkdown,
  normalizeViewerPerfThresholds,
  summarizeViewerPerformance,
} from "../software_safe_src/performance_metrics.js";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const browserLaunchArgs = process.env.AGENT_BROWSER_HEADED === "1" ? ["--headed"] : [];
const viewerRoot = resolve(scriptDir, "..");
const repoRoot = resolve(viewerRoot, "../..");
const configuredAgentBrowserBin = process.env.AGENT_BROWSER_BIN || "agent-browser";
const agentBrowserNpxPackage = process.env.AGENT_BROWSER_NPX_PACKAGE || "agent-browser@0.37.1";
const npxBin = process.platform === "win32" ? "npx.cmd" : "npx";
const session = `viewer-performance-probe-${process.pid}`;
let staticRoot = viewerRoot;

function commandAvailable(command, args = ["--version"]) {
  const result = spawnSync(command, args, { stdio: "ignore" });
  return !result.error;
}

function resolveAgentBrowserInvocation() {
  if (process.env.AGENT_BROWSER_BIN) {
    return {
      command: configuredAgentBrowserBin,
      prefixArgs: [],
      label: configuredAgentBrowserBin,
    };
  }
  if (commandAvailable(configuredAgentBrowserBin)) {
    return {
      command: configuredAgentBrowserBin,
      prefixArgs: [],
      label: configuredAgentBrowserBin,
    };
  }
  if (commandAvailable(npxBin)) {
    return {
      command: npxBin,
      prefixArgs: ["--yes", agentBrowserNpxPackage],
      label: `${npxBin} --yes ${agentBrowserNpxPackage}`,
    };
  }
  throw new Error(`missing required browser automation command: ${configuredAgentBrowserBin} (or npx fallback)`);
}

const agentBrowserInvocation = resolveAgentBrowserInvocation();
const browserLifecycle = createOwnedSessionLifecycle({
  command: agentBrowserInvocation.command,
  prefixArgs: [...agentBrowserInvocation.prefixArgs, ...browserLaunchArgs],
  session,
});
const closeBrowser = browserLifecycle.close;
const prepareBrowserSession = browserLifecycle.prepare;

function positiveInteger(value, fallback) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) {
    return fallback;
  }
  return Math.max(1, Math.floor(numeric));
}

function normalizeProbeOptions(options) {
  options.agents = positiveInteger(options.agents, 80);
  options.locations = positiveInteger(options.locations, 24);
  options.viewport = [
    positiveInteger(options.viewport?.[0], 1440),
    positiveInteger(options.viewport?.[1], 1000),
  ];
  return options;
}

function parseArgs(argv) {
  const options = {
    profile: "smoke",
    durationMs: null,
    outDir: null,
    agents: 80,
    locations: 24,
    interactionSamples: 20,
    interactionWindowMs: 0,
    paintTrace: false,
    compactScene: false,
    mixCamera: false,
    snapshotFeed: false,
    minimumViewportMarkers: 0,
    disposeFixtureControls: false,
    locale: "en",
    viewport: [1440, 1000],
    thresholds: {},
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) throw new Error(`missing value for ${arg}`);
      return argv[index];
    };
    switch (arg) {
      case "--profile": options.profile = next(); break;
      case "--duration-ms": options.durationMs = Number(next()); break;
      case "--out-dir": options.outDir = next(); break;
      case "--agents": options.agents = Number(next()); break;
      case "--locations": options.locations = Number(next()); break;
      case "--interaction-window-ms": options.interactionWindowMs = Number(next()); break;
      case "--paint-trace": options.paintTrace = true; break;
      case "--mixed-camera": options.mixCamera = true; break;
      case "--snapshot-feed": options.snapshotFeed = true; break;
      case "--min-viewport-markers": options.minimumViewportMarkers = Number(next()); break;
      case "--compact-scene": options.compactScene = true; break;
      case "--dispose-fixture-controls": options.disposeFixtureControls = true; break;
      case "--locale": options.locale = next(); break;
      case "--interaction-samples": options.interactionSamples = positiveInteger(next(), 20); break;
      case "--viewport": {
        const [width, height] = next().split(/[x,]/).map((part) => Number(part.trim()));
        options.viewport = [width, height];
        break;
      }
      case "--min-frame-samples": options.thresholds.minFrameSamples = Number(next()); break;
      case "--min-fps": options.thresholds.minFps = Number(next()); break;
      case "--max-frame-p95-ms": options.thresholds.maxFrameP95Ms = Number(next()); break;
      case "--max-frame-p99-ms": options.thresholds.maxFrameP99Ms = Number(next()); break;
      case "--max-long-task-count": options.thresholds.maxLongTaskCount = Number(next()); break;
      case "--max-long-task-total-ms": options.thresholds.maxLongTaskTotalMs = Number(next()); break;
      case "--max-dom-content-loaded-ms": options.thresholds.maxDomContentLoadedMs = Number(next()); break;
      case "--max-load-event-ms": options.thresholds.maxLoadEventMs = Number(next()); break;
      case "--max-interaction-p95-ms": options.thresholds.maxInteractionP95Ms = Number(next()); break;
      case "-h":
      case "--help":
        options.help = true;
        break;
      default:
        throw new Error(`unknown option: ${arg}`);
    }
  }
  if(!Number.isSafeInteger(options.interactionWindowMs)||options.interactionWindowMs<0||options.interactionWindowMs>3600000)throw Error("interaction window must be 0..3600000 integer milliseconds");
  if(options.compactScene&&options.agents+options.locations>500)throw Error("compact scene has500 controlled cells; do not claim higher density");
  if(!Number.isSafeInteger(options.minimumViewportMarkers)||options.minimumViewportMarkers<0)throw Error("invalid minimum viewport marker count");
  if(!["en","zh"].includes(options.locale))throw Error("locale must be en or zh");
  return normalizeProbeOptions(options);
}

function usage() {
  return `Usage: node ./scripts/viewer-performance-probe.mjs [options]

Measures the current canonical Viewer web entry in a real browser with agent-browser.

Options:
  --profile <smoke|release>       Threshold/duration profile (default: smoke)
  --interaction-samples <n>      Trusted native selection inputs (default: 20)
  --interaction-window-ms <n>    Minimum first-input to last-feedback span (0 disables)
  --paint-trace                  Passive DOM mutation and native EventTiming trace
  --compact-scene                Controlled 500-cell grid; actual budgets still apply
  --mixed-camera                 Include trusted native pan/zoom between selections
  --snapshot-feed                Controlled2Hz snapshots and5-second10Hz bursts (not WorldFeed)
  --min-viewport-markers <n>      Reject windows below measured projected/in-viewport density
  --dispose-fixture-controls     Remove QA-only diagnostic panel/timer before sampling
  --locale <en|zh>               Fixture UI locale (default: en)
  --duration-ms <n>               Override sampling duration
  --out-dir <path>                Artifact directory
  --agents <n>                    Synthetic agents in injected snapshot (default: 80)
  --locations <n>                 Synthetic locations in injected snapshot (default: 24)
  --viewport <width>x<height>     Browser viewport (default: 1440x1000)
  --min-frame-samples <n>         Gate: minimum frame samples
  --min-fps <n>                   Gate: average FPS
  --max-frame-p95-ms <n>          Gate: p95 frame interval
  --max-frame-p99-ms <n>          Gate: p99 frame interval
  --max-long-task-count <n>       Gate: browser long task count
  --max-long-task-total-ms <n>    Gate: browser long task total time
  --max-dom-content-loaded-ms <n> Gate: DOMContentLoaded
  --max-load-event-ms <n>         Gate: load event
  --max-interaction-p95-ms <n>    Gate: interaction p95
`;
}

function contentType(pathname) {
  switch (extname(pathname)) {
    case ".html": return "text/html; charset=utf-8";
    case ".js":
    case ".mjs": return "text/javascript; charset=utf-8";
    case ".wasm": return "application/wasm";
    case ".css": return "text/css; charset=utf-8";
    case ".ico": return "image/x-icon";
    case ".png": return "image/png";
    default: return "application/octet-stream";
  }
}

function serveFile(request, response) {
  const requestUrl = new URL(request.url || "/", "http://127.0.0.1");
  let rawPath = "";
  try {
    rawPath = decodeURIComponent(requestUrl.pathname === "/" ? "/viewer.html" : requestUrl.pathname);
  } catch {
    response.writeHead(400);
    response.end("bad request");
    return;
  }
  const normalized = normalize(rawPath).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = resolve(staticRoot, `.${normalized}`);
  const relativePath = relative(staticRoot, filePath);
  if (!relativePath || relativePath.startsWith("..")) {
    response.writeHead(403);
    response.end("forbidden");
    return;
  }
  try {
    const stats = statSync(filePath);
    if (!stats.isFile()) {
      response.writeHead(404);
      response.end("not found");
      return;
    }
    response.writeHead(200, { "Content-Type": contentType(filePath), "Cache-Control": "no-store" });
    response.end(readFileSync(filePath));
  } catch {
    response.writeHead(404);
    response.end("not found");
  }
}

function prepareViewerWebDist(distDir) {
  const testDist = resolve(process.env.AGENT_VISUAL_ARTIFACT_ROOT || resolve(viewerRoot, ".viewer-test-dist"));
  if (!statSync(testDist, { throwIfNoEntry: false })?.isDirectory()) {
    throw new Error("viewer performance probe requires npm run build:viewer:visual-test");
  }
  mkdirSync(distDir, { recursive: true });
  cpSync(testDist, distDir, { recursive: true });
  const pixelWorldRuntimePath = resolve(distDir, "pixel-world-bridge/pixel_world_bridge.js");
  try {
    const stats = statSync(pixelWorldRuntimePath);
    if (!stats.isFile()) throw new Error("not a file");
  } catch {
    throw new Error([
      "viewer performance probe requires the real pixel world renderer runtime",
      `missing: ${pixelWorldRuntimePath}`,
      "run npm run build:viewer:visual-test before the performance probe",
    ].join("\n"));
  }
  return distDir;
}

function ensureAgentBrowser() {
  const result = spawnSync(
    agentBrowserInvocation.command,
    [...agentBrowserInvocation.prefixArgs, "--version"],
    { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
  if (result.status !== 0) {
    throw new Error(`missing required browser automation command: ${agentBrowserInvocation.label}`);
  }
}

function runAgentBrowser(args, options = {}) {
  const timeout = options.timeout ?? 30_000;
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(
      agentBrowserInvocation.command,
      [...agentBrowserInvocation.prefixArgs, "--session", session, ...browserLaunchArgs, ...args],
      { stdio: ["pipe", "pipe", "pipe"] },
    );
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      rejectRun(new Error(`agent-browser timed out: ${args.join(" ")}`));
    }, timeout);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => {
      clearTimeout(timer);
      rejectRun(error);
    });
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      if (code === 0) {
        resolveRun(stdout);
        return;
      }
      rejectRun(new Error([
        `agent-browser failed: ${args.join(" ")}`,
        `exit=${code ?? "null"} signal=${signal ?? "null"}`,
        stdout.trim() ? `stdout:\n${stdout.trim()}` : null,
        stderr.trim() ? `stderr:\n${stderr.trim()}` : null,
      ].filter(Boolean).join("\n")));
    });
    if (options.input !== undefined) child.stdin.end(options.input);
  });
}

async function runAgentBrowserJson(args, options = {}) {
  const output = await runAgentBrowser(["--json", ...args], options);
  const parsed = JSON.parse(output);
  if (!parsed.success) throw new Error(parsed.error || `agent-browser command failed: ${args.join(" ")}`);
  return parsed.data;
}

async function evalJson(source, options = {}) {
  const data = await runAgentBrowserJson(["eval", source], options);
  return typeof data.result === "string" ? JSON.parse(data.result) : data.result;
}

function processSnapshot() {
  const result = spawnSync("ps", ["-axo", "pid=,ppid=,pcpu=,command="], { encoding: "utf8" });
  if (result.status !== 0) return null;
  return result.stdout.split("\n").map((line) => {
    const match = line.trim().match(/^(\d+)\s+(\d+)\s+([\d.]+)\s+(.*)$/);
    return match ? { pid: Number(match[1]), ppid: Number(match[2]), cpu: Number(match[3]), command: match[4] } : null;
  }).filter(Boolean);
}

function establishCpuAttribution(before) {
  const after = processSnapshot();
  if (!before || !after) return { unavailableReason: "ps_process_listing_failed" };
  const known = new Set(before.map((process) => process.pid));
  const roots = after.filter((process) => !known.has(process.pid)
    && /chrome|chromium|agent-browser|playwright/i.test(process.command)).map((process) => process.pid);
  return roots.length ? { roots } : { unavailableReason: "session_process_attribution_unavailable_or_reused" };
}

function collectProcessTreeCpuSample(attribution) {
  const processes = processSnapshot();
  if (!processes) return { attempted: true, processTreeCpuPct: null, unavailableReason: "ps_process_listing_failed" };
  if (!attribution.roots) return { attempted: true, processTreeCpuPct: null, unavailableReason: attribution.unavailableReason };
  const included = new Set(attribution.roots);
  for (let changed = true; changed;) {
    changed = false;
    for (const process of processes) {
      if (!included.has(process.pid) && included.has(process.ppid)) {
        included.add(process.pid);
        changed = true;
      }
    }
  }
  return {
    attempted: true,
    processTreeCpuPct: processes.filter((process) => included.has(process.pid))
      .reduce((total, process) => total + process.cpu, 0),
    processCount: included.size,
  };
}

function probeScript({ durationMs, agents, locations, compactScene }) {
  return `
    (async () => {
      const startedAt = performance.now();
      const api = window.__AW_TEST__;
      if (!api) throw new Error("__AW_TEST__ is unavailable; open viewer with test_api=1");
      const waitForRendererReady = async () => {
        const deadline = performance.now() + 20000;
        let state = api.getState ? api.getState() : null;
        while (performance.now() < deadline) {
          state = api.getState ? api.getState() : null;
          if (
            state?.pixelWorldRuntimeStatus === "ready" &&
            state?.pixelWorldRuntimeSource === "wasm_bindgen_runtime"
          ) {
            return state;
          }
          if (state?.pixelWorldFatal) {
            throw new Error("pixel world renderer fatal before performance sampling: " + JSON.stringify(state.pixelWorldFatal));
          }
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
        throw new Error("pixel world wasm renderer did not reach ready state before performance sampling: " + JSON.stringify({
          pixelWorldRuntimeStatus: state?.pixelWorldRuntimeStatus ?? null,
          pixelWorldRuntimeSource: state?.pixelWorldRuntimeSource ?? null,
          pixelWorldRuntimeModuleUrl: state?.pixelWorldRuntimeModuleUrl ?? null,
          pixelWorldFatal: state?.pixelWorldFatal ?? null,
          lastError: state?.lastError ?? null,
        }));
      };
      const rendererReadyState = await waitForRendererReady();
      const grid = [];
      if (${compactScene}) {
        for(const [col,row] of [[12,10],[13,10]])grid.push({x_cm:3000000+(col-12)*120000,y_cm:2000000+(row-10)*100000,z_cm:0});
        for(let row=0;row<20;row++)for(let col=0;col<25;col++)if(!(row===10&&(col===12||col===13)))grid.push({x_cm:3000000+(col-12)*120000,y_cm:2000000+(row-10)*100000,z_cm:0});
      }
      const model = { agents: {}, locations: {}, agent_prompt_profiles: {}, agent_execution_debug_contexts: {}, agent_player_bindings: {}, agent_player_public_key_bindings: {} };
      for (let index = 0; index < ${locations}; index += 1) {
        const locationId = "loc-" + index;
        model.locations[locationId] = {
          id: locationId,
          name: "Perf Location " + index,
          pos: grid[${agents}+index] || { x_cm: (index % 8) * 400000, y_cm: Math.floor(index / 8) * 260000, z_cm: 0 },
          profile: { radius_cm: 25000 + (index % 4) * 5000, radiation_emission_per_tick: index % 3, material: "silicate" },
          resources: { ore: { amount: 20 + index, unit: "t" }, energy: { amount: 60 + (index % 20), unit: "%" } },
        };
      }
      for (let index = 0; index < ${agents}; index += 1) {
        const agentId = "agent-" + index;
        model.agent_player_bindings[agentId] = rendererReadyState.authPlayerId;
        model.agent_player_public_key_bindings[agentId] = rendererReadyState.authPublicKey;
        model.agents[agentId] = {
          id: agentId,
          name: ${compactScene} ? (index<2 ? "性能观察者"+index : "P"+String(index).padStart(4,"0")) : "Perf Agent " + index,
          pos: grid[index] || { x_cm: 1000000 + (index % 32) * 100000, y_cm: 1000000 + Math.floor(index / 32) * 60000, z_cm: 0 },
          location_id: "loc-" + (index % ${locations}),
          kind: index % 2 === 0 ? "builder" : "surveyor",
          resources: { energy: { amount: 40 + (index % 60), unit: "%" }, ore: { amount: index % 9, unit: "t" } },
        };
      }
      const sourceSnapshot = {
        time: 128,
        config: { space: { width_cm: 6000000, depth_cm: 4000000, height_cm: 800000 } },
        model,
        player_gameplay: {
          stage_id: "post_onboarding",
          stage_status: "blocked",
          execution_state: "blocked",
          accepted_intent_id: "gameplay_action:build_factory_smelter_mk1",
          intent_summary: "Queue build_factory_smelter_mk1 for agent-0",
          intent_scope: "gameplay_action",
          intent_target: "agent-0",
          status_reason: "Performance probe fixture exercises dense viewer rendering.",
          last_world_change: null,
          resume_anchor: "Recover the first sustainable line",
          resume_next_step: "Advance after replenishing materials to confirm the line resumes.",
          goal_id: "post_onboarding.recover_capability",
          goal_kind: "RecoverCapability",
          goal_title: "Recover sustainable capability",
          objective: "Stabilize the first production line before expanding.",
          progress_detail: "The primary line is blocked by missing material input.",
          progress_percent: 68,
          blocker_kind: "material_shortage",
          blocker_detail: "iron input exhausted at factory-0",
          causality_kind: "world_constraint",
          causality_detail: "iron input exhausted at factory-0",
          next_step_hint: "Replenish upstream materials, then advance again to confirm the line resumes.",
          branch_hint: null,
          available_actions: [
            { action_id: "build_factory_smelter_mk1", target_agent_id: "agent-0", label: "Build smelter mk1", protocol_action: "gameplay_action.submit", disabled_reason: null },
            { action_id: "request_snapshot", label: "Request snapshot", protocol_action: "world.request_snapshot", disabled_reason: null },
          ],
          recent_feedback: null,
          agent_claim: null,
        },
      };
      window.__VIEWER_PERF_SOURCE__ = structuredClone(sourceSnapshot);
      api.injectSnapshot(sourceSnapshot, { returnState: false });
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const readyMs = performance.now() - startedAt;
      const frameIntervals = [];
      const longTasks = [];
      let observer = null;
      if ("PerformanceObserver" in window) {
        try {
          observer = new PerformanceObserver((list) => {
            for (const entry of list.getEntries()) {
              longTasks.push({ name: entry.name || "longtask", startTime: entry.startTime, duration: entry.duration });
            }
          });
          observer.observe({ type: "longtask" });
        } catch {}
      }
      const heartbeat = document.createElement("div");
      heartbeat.setAttribute("aria-hidden", "true");
      heartbeat.style.cssText = [
        "position:fixed",
        "left:0",
        "top:0",
        "width:1px",
        "height:1px",
        "opacity:0",
        "pointer-events:none",
        "will-change:transform",
      ].join(";");
      document.body.appendChild(heartbeat);
      let previous = null;
      const sampleStartedAt = performance.now();
      const until = sampleStartedAt + ${durationMs};
      await new Promise((resolve) => {
        const tick = (timestamp) => {
          if (previous != null) frameIntervals.push(timestamp - previous);
          previous = timestamp;
          heartbeat.style.transform = "translateX(" + (frameIntervals.length % 2) + "px)";
          if (performance.now() >= until) {
            resolve();
            return;
          }
          requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      });
      const sampleDurationMs = performance.now() - sampleStartedAt;
      heartbeat.remove();
      const navigation = performance.getEntriesByType("navigation")[0] || {};
      observer?.disconnect?.();
      return JSON.stringify({
        readyMs,
        durationMs: ${durationMs},
        sampleDurationMs,
        frameIntervals,
        longTasks,
        domReadiness: {
          domInteractiveMs: navigation.domInteractive ?? null,
          domContentLoadedMs: navigation.domContentLoadedEventEnd ?? null,
          loadEventMs: navigation.loadEventEnd ?? null,
          responseEndMs: navigation.responseEnd ?? null,
        },
        dom: {
          nodeCount: document.querySelectorAll("*").length,
          panelCount: document.querySelectorAll(".panel").length,
          interactiveElementCount: document.querySelectorAll("button,a,input,textarea,select,[tabindex]").length,
          renderedCanvasCount: document.querySelectorAll(".pixel-world-canvas--rendered").length,
          fallbackShellCount: document.querySelectorAll(".pixel-world-canvas__fallback").length,
        },
        viewport: { width: window.innerWidth, height: window.innerHeight, devicePixelRatio: window.devicePixelRatio },
        browser: { userAgent: navigator.userAgent, renderMeta: window.__OASIS7_VIEWER_RENDER_META || {} },
        renderer: (() => {
          const canvas = document.createElement("canvas");
          const gl = canvas.getContext("webgl2") || canvas.getContext("webgl");
          const debug = gl?.getExtension("WEBGL_debug_renderer_info");
          const renderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : null;
          return {
            rendererClass: /swiftshader|software/i.test(String(renderer || "")) ? "software" : (renderer ? "hardware" : "unknown"),
            renderer,
          };
        })(),
        rendererReadyState,
        actualProjection: {agents:window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.()?.agents?.length ?? null, nativeAgentTargets:document.querySelectorAll("button[data-agent-id][data-renderer-target]").length},
        finalState: api.getState ? api.getState() : null,
      });
    })()
  `;
}

const options = parseArgs(process.argv.slice(2));
if (options.help) {
  console.log(usage());
  process.exit(0);
}
if((options.paintTrace||options.interactionWindowMs)&&!process.env.AGENT_VISUAL_SOURCE_HEAD)throw Error("declare AGENT_VISUAL_SOURCE_HEAD for traced/window measurements");

const profile = VIEWER_PERF_PROFILES[options.profile];
if (!profile) throw new Error(`unknown profile: ${options.profile}`);
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = resolve(options.outDir || join(repoRoot, "output/playwright/viewer-performance", runId));
const summaryJsonPath = join(outDir, "summary.json");
const summaryMdPath = join(outDir, "summary.md");
const screenshotPath = join(outDir, "viewer-performance.png");
const webDistDir = join(outDir, "web-dist");
const durationMs = Number.isFinite(options.durationMs) && options.durationMs > 0 ? options.durationMs : profile.durationMs;
const thresholds = normalizeViewerPerfThresholds({ ...profile.thresholds, ...options.thresholds });

mkdirSync(outDir, { recursive: true });
ensureAgentBrowser();
staticRoot = prepareViewerWebDist(webDistDir);
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const walk=dir=>readdirSync(dir,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?walk(resolve(dir,entry.name)):[resolve(dir,entry.name)]);
const manifest=()=>Object.fromEntries(walk(webDistDir).sort().map(path=>[relative(webDistDir,path),{bytes:statSync(path).size,sha256:hash(readFileSync(path))}]));
const assets=manifest();
const git=args=>{const result=spawnSync('git',args,{cwd:repoRoot,encoding:'utf8'});if(result.status!==0)throw Error(result.stderr);return result.stdout.trim();};
const commandText=(command,args)=>{const result=spawnSync(command,args,{encoding:'utf8'});return result.status===0?result.stdout.trim():'unknown';};
const context={runnerHead:git(['rev-parse','HEAD']),trackedWorktreeDirty:git(['status','--porcelain','--untracked-files=no'])!=='',declaredRendererSource:process.env.AGENT_VISUAL_SOURCE_HEAD?git(['rev-parse',process.env.AGENT_VISUAL_SOURCE_HEAD+'^{commit}']):'unknown',artifactRoot:process.env.AGENT_VISUAL_ARTIFACT_ROOT||'default',assets,scripts:Object.fromEntries(['viewer-performance-probe.mjs','viewer-performance-native-input.mjs','viewer-performance-paint-trace.mjs','viewer-performance-interaction-window.mjs','viewer-owned-cdp-input.mjs','viewer-owned-cdp-session.mjs'].map(path=>[path,hash(readFileSync(resolve(scriptDir,path)))])),hardware:commandText('sysctl',['hw.model','hw.memsize']),os:commandText('sw_vers',[]),power:commandText('pmset',['-g','batt']),refreshHz:'unknown; headless timing does not prove physical refresh/presentation',network:'unshaped loopback; not formal A/B',formalP6:false};
writeFileSync(join(outDir,'run-context.json'),JSON.stringify(context,null,2));


const server = createServer(serveFile);

try {
  await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
  const address = server.address();
  const url = `http://127.0.0.1:${address.port}/viewer.html?test_api=1&connect=0&locale=${options.locale}&hosted_bootstrap=0&pixel_world_visual_fixture=performance_regular&t=${Date.now()}`;

  prepareBrowserSession(); await new Promise((resolve) => setTimeout(resolve, 500));
  const cpuProcessBaseline = processSnapshot();
  console.log(`opening viewer performance probe: ${url}`);
  await runAgentBrowserJson(["open", url], { timeout: 120_000 });
  await runAgentBrowserJson(["eval", `new Promise((resolve,reject)=>{let n=0;const t=setInterval(()=>{if(window.__OASIS7_VIEWER_FIXTURE_READY__){clearInterval(t);resolve(true);}else if(++n>1200){clearInterval(t);reject(new Error("test assembly pending"));}},50);})`]);
  await runAgentBrowserJson(["set", "viewport", String(options.viewport[0]), String(options.viewport[1])]);
  const stylePrerequisite = await evalJson(`(async()=>{
    const result={passed:false,stylesheet:'viewer_terminal_shell.css'};
    const link=[...document.querySelectorAll('link[rel="stylesheet"]')].find(e=>new URL(e.href).pathname.endsWith('/viewer_terminal_shell.css'));
    if(!link)return {...result,error:'canonical stylesheet link missing'};
    const response=await fetch(link.href);Object.assign(result,{href:link.href,status:response.status,contentType:response.headers.get('content-type')});
    if(!response.ok||(result.contentType||'').split(';')[0].trim().toLowerCase()!=='text/css')return {...result,error:'canonical stylesheet requires successful text/css response'};
    const deadline=performance.now()+20000;let target;
    while(performance.now()<deadline){target=document.querySelector('.pixel-world-renderer-target:not([data-selected="true"]):not(:hover):not(:focus-visible)');if(target&&link.sheet)break;await new Promise(r=>setTimeout(r,50));}
    try{result.cssRules=link.sheet?.cssRules.length??0;}catch(error){return {...result,error:String(error)};}
    if(!target||!result.cssRules||link.disabled||link.sheet.disabled)return {...result,error:'canonical sheet or projected target not ready'};
    const css=getComputedStyle(target);result.target={id:target.dataset.agentId||target.dataset.locationId||target.dataset.moduleId,projection:target.closest('.pixel-world-canvas')?.dataset.rendererProjection,backgroundImage:css.backgroundImage,backgroundColor:css.backgroundColor,borders:[css.borderTopColor,css.borderRightColor,css.borderBottomColor,css.borderLeftColor],boxShadow:css.boxShadow};
    const clear=color=>color==='transparent'||color.replaceAll(' ','')==='rgba(0,0,0,0)';
    result.passed=result.target.projection==='true'&&css.backgroundImage==='none'&&clear(css.backgroundColor)&&result.target.borders.every(clear)&&css.boxShadow==='none';
    if(!result.passed)result.error='canonical hit-only target styles are not applied';return result;
  })()`);
  writeFileSync(join(outDir,"canonical-style-prerequisite.json"),JSON.stringify(stylePrerequisite,null,2));
  if(!stylePrerequisite.passed)throw Error("invalid performance entry: "+JSON.stringify(stylePrerequisite));
  if(options.disposeFixtureControls)await evalJson("(()=>{window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__?.dispose();if(document.querySelector('#pixel-world-performance-fixture-controls'))throw Error('QA control panel disposal failed');return true;})()");
  const cpuAttribution = establishCpuAttribution(cpuProcessBaseline);

  async function sampleScenario(name, agents, locations, iteration) {
    console.log(`sampling ${name}-${iteration} for ${durationMs}ms with profile=${options.profile}`);
    const cpuSamples = [];
    const sampleCpu = () => cpuSamples.push(collectProcessTreeCpuSample(cpuAttribution));
    sampleCpu();
    const sampler = setInterval(sampleCpu, 500);
    let rawProbe;
    try {
      rawProbe = await evalJson(probeScript({ durationMs, agents, locations, compactScene: options.compactScene }), {
        timeout: durationMs + 45_000,
      });
    } finally {
      clearInterval(sampler);
      sampleCpu();
    }
    rawProbe.cpuSamples = cpuSamples;
    rawProbe.scenario = { name, agents, locations, iteration };
    return rawProbe;
  }

  const idleRuns = [];
  for (const iteration of [1, 2]) {
    idleRuns.push(await sampleScenario("idle", 1, 1, iteration));
  }
  const denseRuns = [];
  for (const iteration of [1, 2]) {
    denseRuns.push(await sampleScenario("dense", options.agents, options.locations, iteration));
  }
  const rawProbe = denseRuns[denseRuns.length - 1];
  writeFileSync(join(outDir,"before-native-input.json"),JSON.stringify(rawProbe,null,2));
  const interactionPath = join(outDir, "native-interactions.json");
  const windowEvidence={density:[],gestures:[],cpuSamples:[],warmup:[],unsupported:['formal1000entities/200labels: current500marker/100label cap','formal8GiB A/B target unavailable','world query/follow/stop release UI not negotiated in legacy fixture','controlled snapshot bursts are not WorldFeed events','rAF does not prove actual world render frame interval']};
  const checkDensity=async()=>{const row=await evalJson(densityScript);windowEvidence.density.push(row);if(row.visibility!=='visible'||row.projectedMarkers<options.minimumViewportMarkers||row.inViewportTargets<options.minimumViewportMarkers)throw Error('interaction density/foreground prerequisite failed: '+JSON.stringify(row));};
  if(options.compactScene)windowEvidence.warmup=await collectNativeSelectionSamples({evaluate:evalJson,run:args=>runAgentBrowserJson(args),count:2});
  await checkDensity();
  if(options.snapshotFeed)windowEvidence.snapshotFeed=await evalJson(installSnapshotFeedScript);
  if(options.paintTrace)await evalJson(installPaintTraceScript);
  const interactionCpuTimer=setInterval(()=>windowEvidence.cpuSamples.push(collectProcessTreeCpuSample(cpuAttribution)),500);
  let interactionEvidence;
  try { interactionEvidence = await collectNativeSelectionSamples({
    evaluate: evalJson, run: args => runAgentBrowserJson(args), count: options.interactionSamples, minimumDurationMs: options.interactionWindowMs,
    onSample: samples => {appendFileSync(join(outDir,"native-interactions.jsonl"),JSON.stringify(samples.at(-1))+"\n");if(!options.interactionWindowMs||samples.length%50===0)writeFileSync(interactionPath,JSON.stringify({method:"trusted native Enter to selected DOM and two rAF paint opportunities",samples},null,2));},
    betweenSamples: async ({index})=>{
      if(options.mixCamera&&index%8===7)windowEvidence.gestures.push(await nativeCameraGesture({evaluate:evalJson,run:args=>runAgentBrowserJson(args),index}));
      if(options.snapshotFeed&&index%40===39)await evalJson("window.__VIEWER_SNAPSHOT_FEED__.burst();true");
      if(index%20===19)await checkDensity();
    },
  });
  } catch(error){windowEvidence.error=String(error);writeFileSync(join(outDir,"interaction-window.partial.json"),JSON.stringify(windowEvidence,null,2));throw error;} finally {clearInterval(interactionCpuTimer);}
  writeFileSync(interactionPath,JSON.stringify({method:"trusted native Enter to selected DOM and two rAF paint opportunities",samples:interactionEvidence},null,2));
  await checkDensity();
  if(options.snapshotFeed){windowEvidence.snapshotFeed=await evalJson("(()=>{__VIEWER_SNAPSHOT_FEED__.dispose();return JSON.stringify(__VIEWER_SNAPSHOT_FEED__);})()");}
  windowEvidence.actualSpanMs=interactionEvidence.at(-1).endTime-interactionEvidence[0].startTime;
  windowEvidence.formalP6=false;
  writeFileSync(join(outDir,"interaction-window.json"),JSON.stringify(windowEvidence,null,2));
  if(options.paintTrace) {
    await evalJson("new Promise(resolve=>setTimeout(()=>resolve(true),2000))");
    const trace=await evalJson("JSON.stringify(window.__VIEWER_INPUT_TRACE__)");
    rawProbe.inputPaintTrace=matchInputPaintTrace(interactionEvidence,trace);
    writeFileSync(join(outDir,"input-paint-trace.json"),JSON.stringify({trace,matched:rawProbe.inputPaintTrace},null,2));
    await evalJson("window.__VIEWER_INPUT_TRACE__.dispose();true");
  }
  rawProbe.interactionLatencies = interactionEvidence.map(sample => sample.durationMs);
  rawProbe.interactionEvidence = interactionEvidence;
  rawProbe.notes = [...(rawProbe.notes || []), "Selection samples measure local UI feedback only. Any controlled pan/zoom and interaction duration are recorded separately; Runtime/network completion, explicit stop/follow, formal 1000/200 density and A/B coverage remain unproved."];

  const cpuSamples = [...idleRuns, ...denseRuns].flatMap((run) => run.cpuSamples);
  const baselineRecord = (run, suffix) => {
    const measured = summarizeViewerPerformance({ runId: `${runId}-${suffix}`, thresholds, ...run });
    return {
      runId: measured.runId,
      frameP95Ms: measured.metrics.frameP95Ms,
      processTreeCpuPctAvg: measured.cpu.processTreeCpuPctAvg,
    };
  };
  const summary = summarizeViewerPerformance({
    runId,
    url,
    profile: options.profile,
    scenario: { name: "dense", agents: options.agents, locations: options.locations },
    thresholds,
    ...rawProbe,
    cpuSamples,
    baselines: {
      idle: idleRuns.map((run, index) => baselineRecord(run, `idle-${index + 1}`)),
      dense: denseRuns.map((run, index) => baselineRecord(run, `dense-${index + 1}`)),
    },
  });
  if(JSON.stringify(manifest())!==JSON.stringify(assets))throw Error("served artifact tree changed during measurement");
  summary.runContext=context;
  summary.canonicalStylePrerequisite=stylePrerequisite;
  summary.inputPaintTrace=rawProbe.inputPaintTrace;
  summary.interactionWindow=windowEvidence;
  summary.controlledSetup={rendererSource:process.env.AGENT_VISUAL_SOURCE_HEAD||"unknown",artifactRoot:process.env.AGENT_VISUAL_ARTIFACT_ROOT||"default",compactScene:options.compactScene,qaPanelDisposed:options.disposeFixtureControls,locale:options.locale,interactionWindowRequestedMs:options.interactionWindowMs};
  summary.artifacts = { summaryJson: summaryJsonPath, summaryMarkdown: summaryMdPath, screenshot: screenshotPath };

  await runAgentBrowser(["screenshot", screenshotPath], { timeout: 20_000 });
  writeFileSync(summaryJsonPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  writeFileSync(summaryMdPath, buildViewerPerformanceMarkdown(summary), "utf8");

  console.log(`viewer performance status: ${summary.status}`);
  console.log(`summary json: ${summaryJsonPath}`);
  console.log(`summary md: ${summaryMdPath}`);
  console.log(`screenshot: ${screenshotPath}`);
  if (summary.status !== "pass") {
    const failed = summary.gates
      .filter((gate) => gate.status === "fail")
      .map((gate) => `${gate.id} actual=${gate.actual} ${gate.comparator} threshold=${gate.threshold}`)
      .join("; ");
    throw new Error(`viewer performance gates failed: ${failed}`);
  }
} finally {
  closeBrowser();
  await new Promise((resolveClose) => server.close(resolveClose));
}
