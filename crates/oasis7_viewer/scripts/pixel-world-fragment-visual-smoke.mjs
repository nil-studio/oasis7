import { serveStaticFile } from "./viewer-static-files.mjs";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const viewerRoot = resolve(scriptDir, "..");
const repoRoot = resolve(viewerRoot, "../..");
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = resolve(repoRoot, "output/playwright/pixel-world-fragment-visual", runId);
const screenshotPath = join(outDir, "fragment-rendered-visual.png");
const actionReceiptScreenshotPath = join(outDir, "action-receipt-visual.png");
const mobileActionReceiptScreenshotPath = join(outDir, "mobile-action-receipt-visual.png");
const mobileFocusOverlayScreenshotPath = join(outDir, "mobile-focus-overlay-visual.png");
const shellLiteDesktopScreenshotPath = join(outDir, "shell-lite-desktop-visual.png");
const shellLiteMobileScreenshotPath = join(outDir, "shell-lite-mobile-visual.png");
const shellLiteCompactScreenshotPath = join(outDir, "shell-lite-compact-mobile-visual.png");
const shellLiteCjkScreenshotPath = join(outDir, "shell-lite-cjk-mobile-visual.png");
const rendererFallbackScreenshotPath = join(outDir, "renderer-fallback-visual.png");
const summaryPath = join(outDir, "summary.json");
const agentBrowserBin = process.env.AGENT_BROWSER_BIN || "agent-browser";
const session = `pixel-world-fragment-visual-${process.pid}`;
const shellLiteOnly = process.argv.includes("--shell-lite-only");
const summary = {
  status: "running",
  startedAt: new Date().toISOString(),
};

mkdirSync(outDir, { recursive: true });

function assert(condition, message, details = undefined) {
  if (condition) {
    return;
  }
  const suffix = details === undefined ? "" : `\n${JSON.stringify(details, null, 2)}`;
  throw new Error(`${message}${suffix}`);
}

function ensureAgentBrowser() {
  const result = spawnSync(agentBrowserBin, ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `missing required browser automation command: ${agentBrowserBin}\n`
        + "Install or expose agent-browser, or set AGENT_BROWSER_BIN to a compatible executable.",
    );
  }
}

function ensurePixelWorldBridgeDist() {
  const required = [
    resolve(viewerRoot, "dist/pixel-world-bridge/pixel_world_bridge.js"),
    resolve(viewerRoot, "dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen.js"),
    resolve(viewerRoot, "dist/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm"),
  ];
  const missing = required.filter((path) => {
    try {
      return !statSync(path).isFile();
    } catch {
      return true;
    }
  });
  if (missing.length > 0) {
    throw new Error(`pixel-world visual smoke requires built bridge dist; run ./scripts/build-viewer-software-safe.sh\nmissing:\n${missing.join("\n")}`);
  }
}

function contentType(pathname) {
  switch (extname(pathname)) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
    case ".mjs":
      return "text/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".wasm":
      return "application/wasm";
    case ".ico":
      return "image/x-icon";
    case ".png":
      return "image/png";
    default:
      return "application/octet-stream";
  }
}

function serveFile(request, response) {
  serveStaticFile(request, response, viewerRoot, contentType, { bridgeFromDist: true });
}

function runAgentBrowser(args, options = {}) {
  const timeout = options.timeout ?? 30_000;
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(agentBrowserBin, ["--session", session, ...args], {
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      rejectRun(new Error(`agent-browser timed out: ${args.join(" ")}`));
    }, timeout);

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
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

    if (options.input !== undefined) {
      child.stdin.end(options.input);
    } else {
      child.stdin.end();
    }
  });
}

async function runAgentBrowserJson(args, options = {}) {
  const output = await runAgentBrowser(["--json", ...args], options);
  const parsed = JSON.parse(output);
  if (!parsed.success) {
    throw new Error(parsed.error || `agent-browser command failed: ${args.join(" ")}`);
  }
  return parsed.data;
}

async function evalJson(source) {
  const data = await runAgentBrowserJson(["eval", "--stdin"], { input: source });
  const result = data.result;
  if (typeof result === "string") {
    return JSON.parse(result);
  }
  return result;
}

const browserLifecycle = createOwnedSessionLifecycle({ command: agentBrowserBin, session });
const closeBrowser = browserLifecycle.close;
const prepareBrowserSession = browserLifecycle.prepare;

function visualProbeScript(options = {}) {
  const shellLiteProbe = options.shellLiteOnly === true ? "true" : "false";
  return String.raw`
    (async () => {
      const snapshot = {
        time: 24,
        config: { space: { width_cm: 3000000, depth_cm: 2000000, height_cm: 500000 } },
        model: {
          agents: {
            "agent-0": {
              id: "agent-0",
              name: "Survey Agent",
              location_id: "loc-0",
              kind: "surveyor",
              resources: { energy: { amount: 72, unit: "%" } },
            },
            "agent-1": {
              id: "agent-1",
              name: "Relay Agent",
              location_id: "loc-1",
              kind: "surveyor",
              resources: { energy: { amount: 72, unit: "%" } },
            },
          },
          locations: {
            "loc-0": {
              id: "loc-0",
              name: "Fragment Field Anchor",
              pos: { x_cm: 1500000, y_cm: 1000000, z_cm: 0 },
              profile: { radius_cm: 30000, radiation_emission_per_tick: 0, material: "silicate" },
              fragment_profile: {
                blocks: {
                  blocks: [
                    {
                      origin_cm: { x_cm: 3000, y_cm: 0, z_cm: 6000 },
                      size_cm: { x_cm: 9000, y_cm: 6000, z_cm: 9000 },
                      density_kg_per_m3: 3200,
                      compounds: { ppm: { silicate_matrix: 820000, water_ice: 180000 } },
                    },
                    {
                      origin_cm: { x_cm: 18500, y_cm: 0, z_cm: 12000 },
                      size_cm: { x_cm: 12000, y_cm: 6000, z_cm: 9000 },
                      density_kg_per_m3: 7800,
                      compounds: { ppm: { iron_nickel_alloy: 910000, sulfide_ore: 90000 } },
                    },
                    {
                      origin_cm: { x_cm: 10000, y_cm: 0, z_cm: 27000 },
                      size_cm: { x_cm: 9000, y_cm: 6000, z_cm: 12000 },
                      density_kg_per_m3: 920,
                      compounds: { ppm: { water_ice: 720000, hydrated_mineral: 280000 } },
                    },
                  ],
                },
              },
              resources: { ore: { amount: 28, unit: "t" } },
            },
            "loc-1": {
              id: "loc-1",
              name: "Survey Relay Anchor",
              pos: { x_cm: 2100000, y_cm: 1300000, z_cm: 0 },
              profile: { radius_cm: 30000, radiation_emission_per_tick: 0, material: "silicate" },
              resources: { ore: { amount: 28, unit: "t" } },
            },
          },
          agent_prompt_profiles: {},
          agent_execution_debug_contexts: {},
          agent_player_bindings: {
            "agent-0": "viewer-bound",
          },
          agent_player_public_key_bindings: {
            "agent-0": "oc:pk:viewer-session-key",
          },
        },
        player_gameplay: {
          stage_id: "post_onboarding",
          stage_status: "blocked",
          goal_id: "post_onboarding.recover_capability",
          goal_kind: "RecoverCapability",
          goal_title: "Recover sustainable capability",
          objective: "Stabilize the first production line before expanding.",
          progress_detail: "The primary line is blocked by missing material input.",
          progress_percent: 68,
          blocker_kind: "material_shortage",
          blocker_detail: "iron input exhausted at factory-0",
          blocker_supplemental_detail: null,
          next_step_hint: "Replenish upstream materials, then advance again to confirm the line resumes.",
          branch_hint: null,
          available_actions: [],
          recent_feedback: null,
          agent_claim: null,
        },
      };

      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for visual probe condition");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const badges = () => Array.from(document.querySelectorAll(".badge"))
        .map((element) => element.textContent.trim())
        .filter((text) => /^(locations|fragments|agents|links|hotspots|derived_positions|world_bounds|renderer|runtime|zoom|pan)=/.test(text));
      const textOf = (selector, root = document) => root.querySelector(selector)?.textContent.trim() || null;
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        };
      };
      const receiptOf = () => {
        const receipt = document.querySelector(".pixel-world-action-receipt");
        if (!receipt) {
          return null;
        }
        return {
          present: receipt.dataset.receiptPresent || null,
          state: receipt.dataset.receiptState || null,
          confidence: receipt.dataset.receiptConfidence || null,
          title: textOf(".pixel-world-action-receipt__title", receipt),
          summary: textOf(".pixel-world-action-receipt__summary", receipt),
          detail: textOf(".pixel-world-action-receipt__detail", receipt),
          meta: textOf(".pixel-world-action-receipt__meta", receipt),
          rect: rectOf(receipt),
        };
      };
      const zIndexOf = (element) => Number.parseInt(getComputedStyle(element).zIndex || "0", 10) || 0;

      await waitFor(() => window.__AW_TEST__?.injectSnapshot);
      const authSession = await waitFor(() => {
        const current = state();
        if (current.authReady && current.authPlayerId) {
          return {
            playerId: current.authPlayerId,
            publicKey: current.authPublicKey,
          };
        }
        throw new Error("auth fixture not ready: " + JSON.stringify({
          appFixture: document.getElementById("app")?.dataset.viewerVisualFixture || null,
          bodyFixture: document.body?.dataset.viewerVisualFixture || null,
          authReady: current.authReady,
          authPlayerId: current.authPlayerId,
          authBoundAgentId: current.authBoundAgentId,
          authSource: current.authSource,
        }));
      });
      snapshot.model.agent_player_bindings["agent-0"] = authSession.playerId;
      snapshot.model.agent_player_bindings["agent-1"] = authSession.playerId;
      if (authSession.publicKey) {
        snapshot.model.agent_player_public_key_bindings["agent-0"] = authSession.publicKey;
        snapshot.model.agent_player_public_key_bindings["agent-1"] = authSession.publicKey;
      }
      window.__PIXEL_WORLD_VISUAL_BASE_SNAPSHOT__ = snapshot;
      window.__AW_TEST__.injectSnapshot(snapshot);
      await waitFor(() => state().authBoundAgentId === "agent-0");
      await waitFor(() => document.querySelector("#pixel-world-embedded-runtime-canvas"));
      await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      const renderDto = await waitFor(() => window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.());

      const canvas = document.querySelector("#pixel-world-embedded-runtime-canvas");
      const canvasRect = canvas.getBoundingClientRect();
      const wheel = new WheelEvent("wheel", {
        deltaY: -120,
        clientX: canvasRect.left + (canvasRect.width / 2),
        clientY: canvasRect.top + (canvasRect.height / 2),
        bubbles: true,
        cancelable: true,
      });
      const wheelCanceled = !canvas.dispatchEvent(wheel);
      await waitFor(() => state().pixelWorldCamera?.zoom > 1);

      const ready = {
        runtimeStatus: state().pixelWorldRuntimeStatus,
        runtimeSource: state().pixelWorldRuntimeSource,
        fatal: state().pixelWorldFatal,
        camera: state().pixelWorldCamera,
        wheelCanceled,
        cursor: getComputedStyle(canvas).cursor,
        badges: badges(),
        renderDto: {
          fragments: renderDto.fragment_terrain?.length || 0,
          agents: renderDto.agents?.length || 0,
          locations: renderDto.locations?.length || 0,
          selection: renderDto.selection || null,
        },
      };

      if (${shellLiteProbe}) {
        const commandStrip = await waitFor(() => document.querySelector('[data-viewer-overlay="next-move"]'));
        const directRegions = Array.from(commandStrip.children)
          .filter((element) => element.matches("[data-shell-region]"));
        const primary = commandStrip.querySelector('[data-shell-region="next-move-primary"]');
        const supporting = commandStrip.querySelector('[data-shell-region="supporting-context"]');
        const primaryControls = primary ? Array.from(primary.querySelectorAll("a,button")) : [];
        const supportingControls = supporting ? Array.from(supporting.querySelectorAll("a,button")) : [];
        const allControls = Array.from(commandStrip.querySelectorAll("a,button"));
        const selectedAgent = supporting?.querySelector("[data-selected-agent-label]");
        const receiptElement = document.querySelector(".pixel-world-action-receipt");
        const viewport = {
          width: window.innerWidth,
          height: window.innerHeight,
          clientWidth: document.documentElement.clientWidth,
          scrollWidth: document.documentElement.scrollWidth,
        };
        const shellLite = {
          directRegionCount: directRegions.length,
          directRegionNames: directRegions.map((element) => element.dataset.shellRegion || null),
          directChildCount: commandStrip.children.length,
          primaryIndex: directRegions.indexOf(primary),
          supportingIndex: directRegions.indexOf(supporting),
          primaryCtaCount: primaryControls.length,
          primaryCtaLabels: primaryControls.map((element) => element.textContent.trim()),
          supportingControlCount: supportingControls.length,
          allControlCount: allControls.length,
          primaryRect: primary ? rectOf(primary) : null,
          supportingRect: supporting ? rectOf(supporting) : null,
          primaryOpacity: primary ? Number.parseFloat(getComputedStyle(primary).opacity) : null,
          supportingOpacity: supporting ? Number.parseFloat(getComputedStyle(supporting).opacity) : null,
          primaryDominates: Boolean(primary && supporting && rectOf(primary).width >= rectOf(supporting).width),
          objectiveText: textOf(".pixel-world-shell-context-group--objective", supporting),
          leverageText: textOf(".pixel-world-shell-context-group--leverage", supporting),
          selectedAgentLabel: selectedAgent?.dataset.selectedAgentLabel || null,
          selectedAgentText: selectedAgent?.textContent.trim() || null,
          selectedAgentReadable: Boolean(selectedAgent?.dataset.selectedAgentLabel && !/^agent-[a-z0-9_-]+$/i.test(selectedAgent.dataset.selectedAgentLabel)),
          receipt: receiptOf(),
          supportingReceiptGapPx: supporting && receiptElement ? Math.round(receiptElement.getBoundingClientRect().top - supporting.getBoundingClientRect().bottom) : null,
          receiptLabel: textOf(".pixel-world-action-receipt__label", receiptElement),
          receiptDistinct: Boolean(receiptElement && receiptElement.parentElement !== commandStrip && !receiptElement.closest('[data-viewer-overlay="next-move"]')),
          documentLocale: document.documentElement.lang || null,
          viewport,
          horizontalOverflowPx: Math.max(0, viewport.scrollWidth - viewport.clientWidth),
        };
        return JSON.stringify({ ready, shellLite });
      }

      const stage = document.querySelector(".pixel-world-canvas");
      const surfaceCanvas = stage.querySelector(".pixel-world-canvas__surface");
      const agents = Array.from(stage.querySelectorAll(".pixel-world-entity--agent.pixel-world-entity--canvas-hit-target"));
      const agent = agents.find((marker) => marker.dataset.agentId === "agent-0");
      const unselectedAgent = agents.find((marker) => marker.dataset.agentId === "agent-1");
      const agentRect = agent ? rectOf(agent) : null;
      const unselectedAgentRect = unselectedAgent ? rectOf(unselectedAgent) : null;
      const badgeValues = Object.fromEntries(badges().map((badge) => badge.split("=", 2)));
      const nextMoveCard = document.querySelector(".pixel-world-command-cell--next");
      const nextMoveFocusTarget = nextMoveCard.querySelector("a,button");
      const nextMoveRectBeforeFocus = rectOf(nextMoveCard);
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
      nextMoveFocusTarget.focus({ preventScroll: true });
      const nextMoveFocusedStyle = getComputedStyle(nextMoveFocusTarget);
      const nextMoveRectAfterFocus = rectOf(nextMoveCard);
      const nextMoveFocus = {
        active: document.activeElement === nextMoveFocusTarget,
        outlineStyle: nextMoveFocusedStyle.outlineStyle,
        outlineColor: nextMoveFocusedStyle.outlineColor,
        outlineWidth: nextMoveFocusedStyle.outlineWidth,
        boxShadow: nextMoveFocusedStyle.boxShadow,
        rectBefore: nextMoveRectBeforeFocus,
        rectAfter: nextMoveRectAfterFocus,
        layoutStable:
          nextMoveRectBeforeFocus.width === nextMoveRectAfterFocus.width
          && nextMoveRectBeforeFocus.height === nextMoveRectAfterFocus.height,
      };

      return JSON.stringify({
        ready,
        rendered: {
          runtimeStatus: state().pixelWorldRuntimeStatus,
          fragmentCount: Number(badgeValues.fragments),
          agentCount: agents.length,
          canvasRect: rectOf(surfaceCanvas),
          agentRect,
          unselectedAgentRect,
          selectedAgentSelected: agent?.dataset.selected || null,
          unselectedAgentSelected: unselectedAgent?.dataset.selected || null,
          agentPositionSource: agent?.dataset.positionSource || null,
          actionReceipt: receiptOf(),
          nextMoveFocus,
          badges: badges(),
        },
      });
    })()
  `;
}

function shellLiteViewportProbeScript() {
  return String.raw`
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for Shell Lite viewport probe condition");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const textOf = (selector, root = document) => root.querySelector(selector)?.textContent.trim() || null;
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          right: Math.round(rect.right),
          bottom: Math.round(rect.bottom),
        };
      };
      const receiptOf = () => {
        const receipt = document.querySelector(".pixel-world-action-receipt");
        if (!receipt) {
          return null;
        }
        return {
          present: receipt.dataset.receiptPresent || null,
          state: receipt.dataset.receiptState || null,
          confidence: receipt.dataset.receiptConfidence || null,
          label: textOf(".pixel-world-action-receipt__label", receipt),
          title: textOf(".pixel-world-action-receipt__title", receipt),
          rect: rectOf(receipt),
        };
      };

      await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      const commandStrip = await waitFor(() => document.querySelector('[data-viewer-overlay="next-move"]'));
      const directChildren = Array.from(commandStrip.children);
      const directRegions = directChildren.filter((element) => element.matches("[data-shell-region]"));
      const primary = commandStrip.querySelector('[data-shell-region="next-move-primary"]');
      const supporting = commandStrip.querySelector('[data-shell-region="supporting-context"]');
      const primaryControls = primary ? Array.from(primary.querySelectorAll("a,button")) : [];
      const supportingControls = supporting ? Array.from(supporting.querySelectorAll("a,button")) : [];
      const allControls = Array.from(commandStrip.querySelectorAll("a,button"));
      const selectedAgent = supporting?.querySelector("[data-selected-agent-label]");
      const receiptElement = document.querySelector(".pixel-world-action-receipt");
      const feedElement = document.querySelector('[data-viewer-overlay="feed"]');
      const readoutElement = document.querySelector(".pixel-world-readout");
      const selectedMarkerElement = document.querySelector(".pixel-world-entity--canvas-hit-target[data-selected='true']");
      const viewport = {
        width: window.innerWidth,
        height: window.innerHeight,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
      };
      const primaryRect = primary ? rectOf(primary) : null;
      const supportingRect = supporting ? rectOf(supporting) : null;
      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        shellLite: {
          directRegionCount: directRegions.length,
          directRegionNames: directRegions.map((element) => element.dataset.shellRegion || null),
          directChildCount: directChildren.length,
          primaryIndex: directRegions.indexOf(primary),
          supportingIndex: directRegions.indexOf(supporting),
          primaryCtaCount: primaryControls.length,
          primaryCtaLabels: primaryControls.map((element) => element.textContent.trim()),
          supportingControlCount: supportingControls.length,
          allControlCount: allControls.length,
          primaryRect,
          supportingRect,
          primaryOpacity: primary ? Number.parseFloat(getComputedStyle(primary).opacity) : null,
          supportingOpacity: supporting ? Number.parseFloat(getComputedStyle(supporting).opacity) : null,
          primaryDominates: Boolean(primaryRect && supportingRect && primaryRect.width >= supportingRect.width),
          objectiveText: textOf(".pixel-world-shell-context-group--objective", supporting),
          leverageText: textOf(".pixel-world-shell-context-group--leverage", supporting),
          selectedAgentLabel: selectedAgent?.dataset.selectedAgentLabel || null,
          selectedAgentText: selectedAgent?.textContent.trim() || null,
          selectedAgentReadable: Boolean(selectedAgent?.dataset.selectedAgentLabel && !/^agent-[a-z0-9_-]+$/i.test(selectedAgent.dataset.selectedAgentLabel)),
          receipt: receiptOf(),
          supportingReceiptGapPx: supporting && receiptElement ? Math.round(receiptElement.getBoundingClientRect().top - supporting.getBoundingClientRect().bottom) : null,
          receiptDistinct: Boolean(receiptElement && receiptElement.parentElement !== commandStrip && !receiptElement.closest('[data-viewer-overlay="next-move"]')),
          feedRect: feedElement ? rectOf(feedElement) : null,
          readoutRect: readoutElement ? rectOf(readoutElement) : null,
          readoutDisplay: readoutElement ? getComputedStyle(readoutElement).display : null,
          selectedMarkerRect: selectedMarkerElement ? rectOf(selectedMarkerElement) : null,
          commandStripRect: rectOf(commandStrip),
          documentLocale: document.documentElement.lang || null,
          viewport,
          horizontalOverflowPx: Math.max(0, viewport.scrollWidth - viewport.clientWidth),
        },
      });
    })()
  `;
}

function rendererFallbackProbeScript() {
  return String.raw`
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for renderer-unavailable recovery surface");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          right: Math.round(rect.right),
          bottom: Math.round(rect.bottom),
        };
      };
      let fallback;
      try {
        fallback = await waitFor(() => document.querySelector('[data-viewer-overlay="renderer-unavailable"][data-renderer-state="unavailable"]'));
      } catch (error) {
        const diagnostics = document.querySelector(".pixel-world-render-diagnostics");
        throw new Error([
          "renderer deferral route did not become unavailable",
          error instanceof Error ? error.message : String(error),
          JSON.stringify({
            query: window.location.search,
            runtimeStatus: state().pixelWorldRuntimeStatus ?? null,
            runtimeSource: state().pixelWorldRuntimeSource ?? null,
            fatal: state().pixelWorldFatal || null,
            rendererOverlayState: document.querySelector('[data-viewer-overlay="renderer-unavailable"]')?.dataset.rendererState || null,
            diagnosticsState: diagnostics?.dataset.rendererState || null,
            diagnosticsOpen: diagnostics?.open ?? null,
            shellLitePresent: Boolean(document.querySelector('[data-viewer-overlay="next-move"]')),
            actionReceiptPresent: Boolean(document.querySelector(".pixel-world-action-receipt")),
            canvasPresent: Boolean(document.querySelector("#pixel-world-embedded-runtime-canvas")),
          }, null, 2),
        ].join("\n"));
      }
      const diagnostics = document.querySelector(".pixel-world-render-diagnostics");
      const diagnosticText = diagnostics?.textContent.trim() || null;
      const viewport = {
        width: window.innerWidth,
        height: window.innerHeight,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
      };
      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        runtimeSource: state().pixelWorldRuntimeSource,
        fatal: state().pixelWorldFatal || null,
        fallback: {
          visible: getComputedStyle(fallback).display !== "none",
          text: fallback.textContent.trim(),
          rect: rectOf(fallback),
          retryText: fallback.querySelector("button")?.textContent.trim() || null,
          retryCount: fallback.querySelectorAll("button").length,
          rawFatalPromoted: /pixel_world_|CONTEXT_LOST_WEBGL|fatal/i.test(fallback.textContent),
        },
        diagnostics: {
          present: Boolean(diagnostics),
          open: diagnostics?.open ?? null,
          rendererState: diagnostics?.dataset.rendererState || null,
          text: diagnosticText,
          rawFatalFolded: Boolean(diagnostics && diagnostics.open === false && /pixel_world_|fatal/i.test(diagnosticText || "")),
        },
        shellLitePresent: Boolean(document.querySelector('[data-viewer-overlay="next-move"]')),
        actionReceiptPresent: Boolean(document.querySelector(".pixel-world-action-receipt")),
        canvasPresent: Boolean(document.querySelector("#pixel-world-embedded-runtime-canvas")),
        viewport,
        horizontalOverflowPx: Math.max(0, viewport.scrollWidth - viewport.clientWidth),
      });
    })()
  `;
}

function actionReceiptProbeScript() {
  return String.raw`
    (async () => {
      const baseSnapshot = window.__PIXEL_WORLD_VISUAL_BASE_SNAPSHOT__;
      if (!baseSnapshot) {
        throw new Error("missing base visual snapshot for action receipt probe");
      }
      const snapshot = JSON.parse(JSON.stringify(baseSnapshot));
      const gameplay = snapshot.player_gameplay || {};
      snapshot.player_gameplay = gameplay;
      gameplay.stage_status = "blocked";
      gameplay.execution_state = "blocked";
      gameplay.accepted_intent_id = "gameplay_action:build_factory_smelter_mk1";
      gameplay.intent_summary = "Queue build_factory_smelter_mk1 for agent-0";
      gameplay.intent_scope = "gameplay_action";
      gameplay.intent_target = "agent-0";
      gameplay.causality_kind = "world_constraint";
      gameplay.causality_detail = "iron input exhausted at factory-0";
      gameplay.last_world_change = "Smelter build request reached factory-0; iron shortage blocks construction.";
      gameplay.available_actions = [
        {
          action_id: "build_factory_smelter_mk1",
          target_agent_id: "agent-0",
          label: "Build smelter mk1",
          protocol_action: "gameplay_action.submit",
          disabled_reason: null,
        },
      ];
      gameplay.recent_feedback = {
        action: "build_factory_smelter_mk1",
        stage: "completed_no_progress",
        effect: "Smelter build request reached factory-0; iron shortage blocks construction.",
        reason: "iron input exhausted at factory-0",
        hint: "Replenish upstream materials, then advance again.",
        delta_logical_time: 1,
        delta_event_seq: 2,
      };

      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for action receipt visual probe condition");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const textOf = (selector, root = document) => root.querySelector(selector)?.textContent.trim() || null;
      const badgeCount = (name) => {
        const badge = Array.from(document.querySelectorAll(".badge"))
          .map((element) => element.textContent.trim())
          .find((text) => text.startsWith(name + "="));
        return Number(badge?.slice(name.length + 1));
      };
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        };
      };
      const receiptOf = () => {
        const receipt = document.querySelector(".pixel-world-action-receipt");
        if (!receipt) {
          return null;
        }
        return {
          present: receipt.dataset.receiptPresent || null,
          state: receipt.dataset.receiptState || null,
          confidence: receipt.dataset.receiptConfidence || null,
          title: textOf(".pixel-world-action-receipt__title", receipt),
          summary: textOf(".pixel-world-action-receipt__summary", receipt),
          detail: textOf(".pixel-world-action-receipt__detail", receipt),
          meta: textOf(".pixel-world-action-receipt__meta", receipt),
          rect: rectOf(receipt),
        };
      };

      await waitFor(() => window.__AW_TEST__?.injectSnapshot);
      if (state().pixelWorldRuntimeStatus !== "ready") {
        const reattachButton = Array.from(document.querySelectorAll("button"))
          .find((button) => /Reattach Embedded Renderer|重新挂载/.test(button.textContent));
        if (!reattachButton) {
          throw new Error("missing Reattach Embedded Renderer button for action receipt probe");
        }
        reattachButton.click();
        await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      }
      window.__AW_TEST__.injectSnapshot(snapshot);
      const receipt = await waitFor(() => {
        const current = receiptOf();
        return current?.present === "true" && current?.confidence === "world_delta" ? current : null;
      });
      await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      const renderDto = await waitFor(() => window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.());
      const agentRect = await waitFor(() => {
        const current = document.querySelector(".pixel-world-entity--agent");
        const rect = current ? rectOf(current) : null;
        return rect?.width > 0 && rect?.height > 0 ? rect : null;
      });
      const blockerBadge = Array.from(document.querySelectorAll(".badge"))
        .map((element) => element.textContent.trim())
        .find((text) => text.startsWith("blocker=")) || null;
      await sleep(150);

      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        receipt,
        playerLeverageText: textOf(".pixel-world-shell-context-group--leverage"),
        blockerBadge,
        fragmentCount: badgeCount("fragments"),
        agentRect,
        renderDto: {
          fragments: renderDto.fragment_terrain?.length || 0,
          agents: renderDto.agents?.length || 0,
          locations: renderDto.locations?.length || 0,
        },
      });
    })()
  `;
}

function mobileActionReceiptProbeScript() {
  return String.raw`
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for mobile action receipt visual probe condition");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const textOf = (selector, root = document) => root.querySelector(selector)?.textContent.trim() || null;
      const badgeCount = (name) => {
        const badge = Array.from(document.querySelectorAll(".badge"))
          .map((element) => element.textContent.trim())
          .find((text) => text.startsWith(name + "="));
        return Number(badge?.slice(name.length + 1));
      };
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          right: Math.round(rect.right),
          bottom: Math.round(rect.bottom),
        };
      };
      const receiptOf = () => {
        const receipt = document.querySelector(".pixel-world-action-receipt");
        if (!receipt) {
          return null;
        }
        return {
          present: receipt.dataset.receiptPresent || null,
          state: receipt.dataset.receiptState || null,
          confidence: receipt.dataset.receiptConfidence || null,
          title: textOf(".pixel-world-action-receipt__title", receipt),
          summary: textOf(".pixel-world-action-receipt__summary", receipt),
          detail: textOf(".pixel-world-action-receipt__detail", receipt),
          meta: textOf(".pixel-world-action-receipt__meta", receipt),
          rect: rectOf(receipt),
        };
      };

      await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      const renderDto = await waitFor(() => window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.());
      const receiptElement = await waitFor(() => document.querySelector(".pixel-world-action-receipt"));
      receiptElement.scrollIntoView({ block: "start", inline: "nearest" });
      window.scrollBy(0, -8);
      await sleep(180);

      const agentRect = await waitFor(() => {
        const current = document.querySelector(".pixel-world-entity--agent");
        const rect = current ? rectOf(current) : null;
        return rect?.width > 0 && rect?.height > 0 ? rect : null;
      });

      const receipt = receiptOf();
      const viewport = {
        width: window.innerWidth,
        height: window.innerHeight,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
        scrollX: Math.round(window.scrollX),
        scrollY: Math.round(window.scrollY),
      };
      const horizontalOverflowPx = Math.max(0, viewport.scrollWidth - viewport.clientWidth);
      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        viewport,
        horizontalOverflowPx,
        receipt,
        fragmentCount: badgeCount("fragments"),
        agentRect,
        renderDto: {
          fragments: renderDto.fragment_terrain?.length || 0,
          agents: renderDto.agents?.length || 0,
          locations: renderDto.locations?.length || 0,
        },
        commandStripRect: rectOf(document.querySelector(".pixel-world-command-strip")),
      });
    })()
  `;
}

function mobileFocusOverlayProbeScript() {
  return String.raw`
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for mobile focus overlay visual probe condition");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          right: Math.round(rect.right),
          bottom: Math.round(rect.bottom),
        };
      };

      await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      const focusButton = Array.from(document.querySelectorAll("button"))
        .find((button) => /Cinematic View|Enter World Focus|电影视图|进入沉浸模式/.test(button.textContent || ""));
      if (!focusButton) {
        throw new Error("missing Enter World Focus button for mobile focus overlay probe");
      }
      focusButton.click();
      await waitFor(() => document.body.classList.contains("pixel-world-focus-active"));
      const commandDrawer = document.querySelector(".pixel-world-focus-drawer--command");
      commandDrawer?.removeAttribute("open");
      await sleep(180);

      const viewport = {
        width: window.innerWidth,
        height: window.innerHeight,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
      };
      const hud = rectOf(document.querySelector(".pixel-world-focus-hud"));
      const controls = rectOf(document.querySelector(".pixel-world-focus-controls"));
      const prompt = rectOf(document.querySelector(".pixel-world-focus-hud__cell--prompt"));
      const blocker = rectOf(document.querySelector(".pixel-world-focus-hud__cell--blocker"));
      const receiptCell = rectOf(document.querySelector(".pixel-world-focus-hud__cell--receipt"));
      const map = rectOf(document.querySelector('[data-focus-minimap="true"]'));
      const receipt = rectOf(document.querySelector(".pixel-world-focus-receipt"));
      const selectedMarkerElement = document.querySelector(".pixel-world-entity--canvas-hit-target[data-selected='true']");
      const selectedMarker = selectedMarkerElement ? rectOf(selectedMarkerElement) : null;
      const markerOverlapsHud = Boolean(selectedMarker
        && selectedMarker.right > hud.x && selectedMarker.x < hud.right
        && selectedMarker.bottom > hud.y && selectedMarker.y < hud.bottom);
      const horizontalOverflowPx = Math.max(0, viewport.scrollWidth - viewport.clientWidth);

      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        focusActive: document.body.classList.contains("pixel-world-focus-active"),
        viewport,
        horizontalOverflowPx,
        hud,
        controls,
        prompt,
        blocker,
        receiptCell,
        map,
        receipt,
        selectedMarker,
        markerOverlapsHud,
        gaps: {
          hudToMap: map.y - hud.bottom,
          mapToReceipt: receipt.y - map.bottom,
          controlsToPrompt: controls.y - prompt.bottom,
          blockerToReceiptCell: receiptCell.y - blocker.bottom,
        },
      });
    })()
  `;
}

function assertShellLiteViewport(label, probe, expectedWidth) {
  const runtimeStatus = probe.runtimeStatus || probe.ready?.runtimeStatus;
  assert(runtimeStatus === "ready", `${label} runtime did not stay ready`, probe);
  const shell = probe.shellLite;
  assert(shell?.directChildCount === 2, `${label} command strip must have exactly two direct children`, probe);
  assert(shell?.directRegionCount === 2, `${label} command strip must have exactly two semantic regions`, probe);
  assert(
    JSON.stringify(shell.directRegionNames) === JSON.stringify(["next-move-primary", "supporting-context"]),
    `${label} semantic regions are not primary-then-supporting`,
    probe,
  );
  assert(shell.primaryIndex === 0 && shell.supportingIndex === 1, `${label} primary region must precede supporting context`, probe);
  assert(shell.primaryCtaCount === 1, `${label} primary region must expose one CTA`, probe);
  assert(shell.allControlCount === 1, `${label} command strip must expose one total CTA`, probe);
  assert(shell.supportingControlCount === 0, `${label} supporting context must remain noninteractive`, probe);
  assert(shell.primaryDominates === true, `${label} primary region must not be narrower than supporting context`, probe);
  assert(/Objective|目标/.test(shell.objectiveText || ""), `${label} objective context is missing`, probe);
  assert(/Player Leverage|玩家杠杆/.test(shell.leverageText || ""), `${label} player leverage context is missing`, probe);
  assert(shell.selectedAgentReadable === true, `${label} selected-agent label is not player-readable`, probe);
  assert(shell.selectedAgentLabel && !/^agent-[a-z0-9_-]+$/i.test(shell.selectedAgentLabel), `${label} selected-agent label exposed a raw internal ID`, probe);
  assert(shell.receiptDistinct === true, `${label} Action Receipt is not distinct from Shell Lite`, probe);
  assert(/Action Receipt|行动回执/.test(shell.receiptLabel || shell.receipt?.label || ""), `${label} Action Receipt label is missing`, probe);
  assert(shell.viewport?.width === expectedWidth, `${label} probe used the wrong viewport width`, probe);
  assert(shell.horizontalOverflowPx <= 2, `${label} Shell Lite has horizontal overflow`, probe);
  if (expectedWidth <= 640) {
    const receiptGapPx = shell.viewport.height <= 650
      ? shell.receipt.rect.y - shell.commandStripRect.bottom
      : shell.supportingReceiptGapPx;
    assert(receiptGapPx >= 8, `${label} command content must clear Action Receipt by at least 8px`, probe);
    const feedReadoutSeparated = shell.readoutDisplay === "none" || !shell.feedRect || !shell.readoutRect
      || shell.feedRect.bottom + 8 <= shell.readoutRect.y || shell.readoutRect.bottom + 8 <= shell.feedRect.y;
    assert(feedReadoutSeparated, `${label} Feed and world readout overlap`, probe);
    assert(shell.selectedMarkerRect?.width >= 44 && shell.selectedMarkerRect?.height >= 44, `${label} selected marker lost its mobile hit area`, probe);
    assert(shell.selectedMarkerRect.bottom + 8 <= shell.commandStripRect.y, `${label} selected marker does not clear Next Move by 8px`, probe);
  }
}

function assertCjkShellLiteViewport(probe) {
  const shell = probe.shellLite;
  assert(shell.documentLocale === "zh-CN", "CJK Shell Lite did not apply zh-CN document locale", probe);
  assert(/目标/.test(shell.objectiveText || ""), "CJK Shell Lite objective label is not localized", probe);
  assert(/玩家杠杆/.test(shell.leverageText || ""), "CJK Shell Lite leverage label is not localized", probe);
  assert(/行动回执/.test(shell.receiptLabel || shell.receipt?.label || ""), "CJK Shell Lite receipt label is not localized", probe);
}

function assertRendererFallback(probe, expectedWidth) {
  assert(probe.runtimeStatus === "unavailable", "renderer deferral did not expose unavailable runtime state", probe);
  assert(probe.fallback?.visible === true, "renderer-unavailable recovery surface is not visible", probe);
  assert(/Graphics unavailable in this browser|此浏览器中的图形不可用/.test(probe.fallback.text || ""), "renderer-unavailable copy is not player-readable", probe);
  assert(probe.fallback.retryCount === 1, "renderer-unavailable recovery surface must expose one retry control", probe);
  assert(probe.fallback.rawFatalPromoted === false, "raw renderer fatal details were promoted into recovery copy", probe);
  assert(probe.diagnostics?.rendererState === "unavailable", "renderer diagnostics did not retain unavailable state", probe);
  assert(probe.diagnostics?.open === false, "raw renderer diagnostics are not folded by default", probe);
  assert(probe.diagnostics?.rawFatalFolded === true, "raw renderer fatal details were not retained behind folded diagnostics", probe);
  assert(probe.shellLitePresent === false, "Shell Lite should not render without a derived world surface", probe);
  assert(probe.actionReceiptPresent === false, "Action Receipt should not claim a surface without a derived world state", probe);
  assert(probe.canvasPresent === false, "renderer-unavailable route must not expose a ready canvas", probe);
  assert(probe.viewport?.width === expectedWidth, "renderer fallback probe used the wrong viewport width", probe);
  assert(probe.horizontalOverflowPx <= 2, "renderer-unavailable surface has horizontal overflow", probe);
  assert(probe.fallback.rect?.right <= probe.viewport.clientWidth + 2, "renderer-unavailable surface extends beyond the viewport", probe);
}

async function runShellLiteOnlyMode(url) {
  summary.mode = "shell-lite-only";
  await runAgentBrowserJson(["open", url], { timeout: 45_000 });
  await runAgentBrowserJson(["set", "viewport", "1440", "1000"]);
  console.log("probing Shell Lite desktop semantic surface");
  summary.desktopShellLite = await evalJson(visualProbeScript({ shellLiteOnly: true }));
  assertShellLiteViewport("desktop Shell Lite", summary.desktopShellLite, 1440);
  await runAgentBrowser(["screenshot", shellLiteDesktopScreenshotPath], { timeout: 20_000 });
  summary.shellLiteDesktopScreenshot = shellLiteDesktopScreenshotPath;

  await runAgentBrowserJson(["set", "viewport", "390", "844"]);
  console.log("probing Shell Lite mobile semantic surface");
  summary.mobileShellLite = await evalJson(shellLiteViewportProbeScript());
  assertShellLiteViewport("mobile Shell Lite", summary.mobileShellLite, 390);
  await runAgentBrowser(["screenshot", shellLiteMobileScreenshotPath], { timeout: 20_000 });
  summary.shellLiteMobileScreenshot = shellLiteMobileScreenshotPath;

  await runAgentBrowserJson(["set", "viewport", "320", "568"]);
  console.log("probing compact Shell Lite mobile safe area");
  summary.compactShellLite = await evalJson(shellLiteViewportProbeScript());
  assertShellLiteViewport("compact Shell Lite", summary.compactShellLite, 320);
  await runAgentBrowser(["screenshot", shellLiteCompactScreenshotPath], { timeout: 20_000 });
  summary.shellLiteCompactScreenshot = shellLiteCompactScreenshotPath;

  const cjkUrl = new URL(url);
  cjkUrl.searchParams.set("locale", "zh-CN");
  await runAgentBrowserJson(["open", cjkUrl.toString()], { timeout: 45_000 });
  await runAgentBrowserJson(["set", "viewport", "390", "844"]);
  console.log("probing CJK Shell Lite mobile semantic surface");
  summary.cjkShellLite = await evalJson(shellLiteViewportProbeScript());
  assertShellLiteViewport("CJK Shell Lite", summary.cjkShellLite, 390);
  assertCjkShellLiteViewport(summary.cjkShellLite);
  await runAgentBrowser(["screenshot", shellLiteCjkScreenshotPath], { timeout: 20_000 });
  summary.shellLiteCjkScreenshot = shellLiteCjkScreenshotPath;

  const fallbackUrl = new URL(url);
  fallbackUrl.searchParams.set("pixel_world_renderer", "defer");
  await runAgentBrowserJson(["open", fallbackUrl.toString()], { timeout: 45_000 });
  await runAgentBrowserJson(["set", "viewport", "1440", "1000"]);
  console.log("probing renderer-unavailable recovery surface");
  summary.rendererFallback = await evalJson(rendererFallbackProbeScript());
  assertRendererFallback(summary.rendererFallback, 1440);
  await runAgentBrowser(["screenshot", rendererFallbackScreenshotPath], { timeout: 20_000 });
  summary.rendererFallbackScreenshot = rendererFallbackScreenshotPath;
  summary.cjkUrl = cjkUrl.toString();
  summary.rendererFallbackUrl = fallbackUrl.toString();
}

ensureAgentBrowser();
ensurePixelWorldBridgeDist();

const server = createServer(serveFile);

try {
  await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
  const address = server.address();
  const url = `http://127.0.0.1:${address.port}/viewer.html?test_api=1&connect=0&locale=en&viewer_visual_fixture=shell_selected_blocker&pixel_world_visual_fixture=selected_blocker&t=${Date.now()}`;

  prepareBrowserSession();
  if (shellLiteOnly) {
    await runShellLiteOnlyMode(url);
    summary.url = url;
    summary.status = "passed";
    summary.completedAt = new Date().toISOString();
    writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
    console.log(`Shell Lite visual smoke passed: ${summaryPath}`);
    console.log(`desktop screenshot: ${shellLiteDesktopScreenshotPath}`);
    console.log(`mobile screenshot: ${shellLiteMobileScreenshotPath}`);
  } else {
    console.log(`opening pixel-world viewer visual smoke: ${url}`);
    await runAgentBrowserJson(["open", url], { timeout: 45_000 });
    await runAgentBrowserJson(["set", "viewport", "1440", "1000"]);

  console.log("probing wasm bridge and rendered visual hierarchy");
  Object.assign(summary, await evalJson(visualProbeScript()));

  assert(summary.ready.runtimeStatus === "ready", "wasm runtime did not become ready", summary.ready);
  assert(summary.ready.runtimeSource === "wasm_bindgen_runtime", "viewer did not use wasm bindgen runtime", summary.ready);
  assert(summary.ready.fatal === null, "pixel-world runtime reported a fatal error", summary.ready);
  assert(summary.ready.camera?.zoom > 1, "wheel interaction did not produce camera zoom telemetry", summary.ready);
  assert(summary.ready.wheelCanceled === true, "canvas did not capture wheel interaction", summary.ready);
  assert(summary.ready.renderDto?.fragments === 3, "current Rust render DTO lost its three fragment projections", summary.ready);
  assert(summary.ready.renderDto?.agents === 2, "current Rust render DTO lost its two agent projections", summary.ready);
  assert(summary.ready.renderDto?.locations >= 2, "current Rust render DTO lost its location projections", summary.ready);
  assert(summary.ready.renderDto?.selection?.kind === "agent", "current Rust render DTO lost agent selection", summary.ready);
  assert(summary.rendered.runtimeStatus === "ready", "rendered bridge surface was not ready for canvas assertions", summary.rendered);
  assert(summary.rendered.fragmentCount === 3, "Rust render projection did not report exactly three fragments", summary.rendered);
  assert(summary.rendered.agentCount === 2, "expected exactly two accessible canvas agent hit targets", summary.rendered);
  assert(summary.rendered.canvasRect?.width > 0 && summary.rendered.canvasRect?.height > 0, "authoritative canvas has no measurable footprint", summary.rendered);
  assert(summary.rendered.selectedAgentSelected === "true", "agent-0 must remain the selected authenticated agent", summary.rendered);
  assert(summary.rendered.unselectedAgentSelected === "false", "agent-1 must remain an unselected visual-only fixture agent", summary.rendered);
  assert(summary.rendered.unselectedAgentRect?.width > 0, "second agent marker has no measurable visual footprint", summary.rendered);
  assert(
    summary.rendered.agentRect.x !== summary.rendered.unselectedAgentRect.x
      || summary.rendered.agentRect.y !== summary.rendered.unselectedAgentRect.y,
    "two-agent fixture did not produce distinct readable marker positions",
    summary.rendered,
  );
  assert(summary.rendered.agentPositionSource === "location_derived", "agent position was not derived from its location", summary.rendered);
  assert(summary.rendered.actionReceipt?.present === "false", "rendered hierarchy fixture should start from an honest no-receipt state", summary.rendered);
  assert(summary.rendered.actionReceipt?.confidence === "none", "no-receipt fixture should not claim action receipt confidence", summary.rendered);
  assert(summary.rendered.actionReceipt?.meta === null, "no-receipt fixture should not show receipt metadata", summary.rendered);
  assert(summary.rendered.nextMoveFocus?.active === true, "next move control did not receive keyboard focus", summary.rendered.nextMoveFocus);
  assert(summary.rendered.nextMoveFocus?.outlineStyle !== "none", "next move control focus state is not visibly outlined", summary.rendered.nextMoveFocus);
  assert(summary.rendered.nextMoveFocus?.outlineWidth !== "0px", "next move control focus outline has no measurable width", summary.rendered.nextMoveFocus);
  assert(summary.rendered.nextMoveFocus?.layoutStable === true, "next move focus state caused layout shift", summary.rendered.nextMoveFocus);

  await runAgentBrowser(["screenshot", screenshotPath], { timeout: 20_000 });
  console.log("probing action receipt visual state");
  summary.actionReceipt = await evalJson(actionReceiptProbeScript());
  assert(summary.actionReceipt.receipt?.present === "true", "action receipt did not become visible", summary.actionReceipt);
  assert(summary.actionReceipt.receipt?.state === "blocked", "action receipt did not expose the blocked state", summary.actionReceipt);
  assert(summary.actionReceipt.receipt?.confidence === "world_delta", "action receipt did not use world_delta confidence", summary.actionReceipt);
  assert(summary.actionReceipt.receipt?.title === "Action blocked", "action receipt title was not player-readable", summary.actionReceipt);
  assert(summary.actionReceipt.runtimeStatus === "ready", "action receipt screenshot did not stay on the Rust bridge surface", summary.actionReceipt);
  assert(
    /iron shortage blocks construction/.test(summary.actionReceipt.receipt?.summary || ""),
    "action receipt summary did not describe the confirmed blocker",
    summary.actionReceipt,
  );
  assert(/Agent 0/.test(summary.actionReceipt.receipt?.meta || ""), "action receipt did not use a readable target-agent label", summary.actionReceipt);
  assert(!/agent=agent-0/.test(summary.actionReceipt.receipt?.meta || ""), "action receipt leaked the raw target-agent id", summary.actionReceipt);
  assert(/Survey Agent/.test(summary.actionReceipt.playerLeverageText || ""), "Player Leverage did not use the readable agent name", summary.actionReceipt);
  assert(!/agent-0/.test(summary.actionReceipt.playerLeverageText || ""), "Player Leverage leaked the raw agent id", summary.actionReceipt);
  assert(summary.actionReceipt.receipt?.rect?.height > 40, "action receipt did not render with a measurable visual footprint", summary.actionReceipt);
  assert(summary.actionReceipt.fragmentCount === 3, "action receipt scenario should preserve fragment background markers", summary.actionReceipt);
  assert(summary.actionReceipt.renderDto?.fragments === 3 && summary.actionReceipt.renderDto?.agents === 2, "action receipt probe did not preserve the current Rust render DTO", summary.actionReceipt);
  assert(summary.actionReceipt.agentRect?.width > 0, "action receipt scenario lost the readable agent marker", summary.actionReceipt);
  await runAgentBrowser(["screenshot", actionReceiptScreenshotPath], { timeout: 20_000 });

  console.log("probing mobile action receipt visual state");
  await runAgentBrowserJson(["set", "viewport", "390", "844"]);
  summary.mobileActionReceipt = await evalJson(mobileActionReceiptProbeScript());
  assert(summary.mobileActionReceipt.runtimeStatus === "ready", "mobile action receipt did not stay on the Rust bridge surface", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.viewport?.clientWidth <= 430, "mobile visual pass did not use a phone-width viewport", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.horizontalOverflowPx <= 2, "mobile pixel-world surface has horizontal overflow", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.receipt?.present === "true", "mobile action receipt is not visible", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.receipt?.state === "blocked", "mobile action receipt did not preserve blocked state", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.receipt?.confidence === "world_delta", "mobile action receipt did not preserve world_delta confidence", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.receipt?.rect?.x >= 0, "mobile receipt starts outside the viewport", summary.mobileActionReceipt);
  assert(
    summary.mobileActionReceipt.receipt?.rect?.right <= summary.mobileActionReceipt.viewport.clientWidth + 2,
    "mobile receipt extends beyond the viewport",
    summary.mobileActionReceipt,
  );
  assert(summary.mobileActionReceipt.receipt?.rect?.bottom <= summary.mobileActionReceipt.viewport.height, "mobile receipt is not fully visible in the screenshot viewport", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.fragmentCount === 3, "mobile action receipt scenario lost fragment background markers", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.renderDto?.fragments === 3 && summary.mobileActionReceipt.renderDto?.agents === 2, "mobile receipt probe did not preserve the current Rust render DTO", summary.mobileActionReceipt);
  assert(summary.mobileActionReceipt.agentRect?.width > 0, "mobile action receipt scenario lost the readable agent marker", summary.mobileActionReceipt);
  await runAgentBrowser(["screenshot", mobileActionReceiptScreenshotPath], { timeout: 20_000 });

  console.log("probing mobile focus overlay safe areas");
  summary.mobileFocusOverlay = await evalJson(mobileFocusOverlayProbeScript());
  assert(summary.mobileFocusOverlay.runtimeStatus === "ready", "mobile focus overlay did not stay on the Rust bridge surface", summary.mobileFocusOverlay);
  assert(summary.mobileFocusOverlay.focusActive === true, "mobile focus overlay did not enter focus mode", summary.mobileFocusOverlay);
  assert(summary.mobileFocusOverlay.horizontalOverflowPx <= 2, "mobile focus overlay has horizontal overflow", summary.mobileFocusOverlay);
  assert(summary.mobileFocusOverlay.gaps.controlsToPrompt >= 4, "mobile focus controls overlap the prompt cell", summary.mobileFocusOverlay);
  assert(summary.mobileFocusOverlay.gaps.blockerToReceiptCell >= 0, "mobile focus blocker and receipt cells overlap", summary.mobileFocusOverlay);
  assert(summary.mobileFocusOverlay.selectedMarker?.width >= 44, "mobile selected marker lost its minimum hit target", summary.mobileFocusOverlay);
  assert(summary.mobileFocusOverlay.markerOverlapsHud === false, "mobile selected marker overlaps the focus HUD", summary.mobileFocusOverlay);
  await runAgentBrowser(["screenshot", mobileFocusOverlayScreenshotPath], { timeout: 20_000 });

  summary.url = url;
  summary.screenshot = screenshotPath;
  summary.actionReceiptScreenshot = actionReceiptScreenshotPath;
  summary.mobileActionReceiptScreenshot = mobileActionReceiptScreenshotPath;
  summary.mobileFocusOverlayScreenshot = mobileFocusOverlayScreenshotPath;
  summary.status = "passed";
  summary.completedAt = new Date().toISOString();
  writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  console.log(`pixel-world fragment visual smoke passed: ${summaryPath}`);
  console.log(`screenshot: ${screenshotPath}`);
  console.log(`action receipt screenshot: ${actionReceiptScreenshotPath}`);
  console.log(`mobile action receipt screenshot: ${mobileActionReceiptScreenshotPath}`);
  console.log(`mobile focus overlay screenshot: ${mobileFocusOverlayScreenshotPath}`);
  }
} catch (error) {
  summary.status = "failed";
  summary.completedAt = new Date().toISOString();
  summary.failure = {
    name: error instanceof Error ? error.name : "Error",
    message: error instanceof Error ? error.message : String(error),
    stack: error instanceof Error ? error.stack : null,
  };
  if (!summary.mobileFocusOverlayScreenshot) {
    try {
      await runAgentBrowser(["screenshot", mobileFocusOverlayScreenshotPath], { timeout: 20_000 });
      summary.mobileFocusOverlayScreenshot = mobileFocusOverlayScreenshotPath;
    } catch (screenshotError) {
      summary.mobileFocusOverlayScreenshotError = screenshotError instanceof Error
        ? screenshotError.message
        : String(screenshotError);
    }
  }
  writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  console.error(`pixel-world fragment visual smoke failed: ${summaryPath}`);
  throw error;
} finally {
  closeBrowser();
  await new Promise((resolveClose) => server.close(resolveClose));
}
