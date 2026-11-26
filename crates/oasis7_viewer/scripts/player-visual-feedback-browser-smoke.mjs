import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { readFileSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, extname, join, normalize, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";
import {
  bridgeModule,
  fakeWebSocket,
  fixtureCanvasCompatibility,
  fixtureVisualOverlayBootstrap,
} from "./player_visual_feedback_browser_smoke_fixtures.mjs";

// QA-owned S6 browser fixture. It exercises the player-visible Viewer shell
// with a deterministic local bridge and fake world-feed transport. It does
// not start a runtime, provider, chain, or real gameplay session.
const scriptDir = dirname(fileURLToPath(import.meta.url));
const viewerRoot = resolve(scriptDir, "..");
const repoRoot = resolve(viewerRoot, "../..");
const taskUid = "task_872ecd04e2824c02a685e1fd6df63d03";
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = resolve(repoRoot, ".pm/scratch", taskUid, "browser", runId);
const bundlePath = resolve(viewerRoot, ".software-safe-build/viewer.js");
const session = `player-visual-feedback-${process.pid}`;
const browserBin = process.env.AGENT_BROWSER_BIN || "agent-browser";
const skipBuild = process.argv.includes("--skip-build");
const onlyProduction = process.argv.includes("--only-production");
const onlyOffscreen = process.argv.includes("--only-offscreen");
const onlyCamera = process.argv.includes("--only-camera");
const onlyTouch = process.argv.includes("--only-touch");
const onlyTooltipFeed = process.argv.includes("--only-tooltip-feed");
const onlyThirdReview = process.argv.includes("--only-third-review");
const onlyFocus = process.argv.includes("--only-focus");
const statuses = ["ready", "replay", "empty", "gap", "unavailable"];
const viewports = [
  { name: "mobile", width: 390, height: 844 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "desktop", width: 1440, height: 1000 },
];
const shortLandscapeViewports = [
  { name: "landscape-phone", width: 844, height: 390 },
  { name: "landscape-compact", width: 640, height: 360 },
];
const summary = {
  caseId: "S6-Q1-player-visual-feedback",
  taskUid,
  mode: "viewer_test_api_fixture",
  inputMode: "visible-ui-controls-plus-dom-readback",
  mockDisabled: false,
  fixtureBoundary: "deterministic QA bridge and fake world_feed; no runtime/provider/playability claim",
  phase: onlyOffscreen ? "offscreen-only" : onlyThirdReview ? "third-review-only" : onlyProduction ? "production-hotspot-only"
    : onlyCamera ? "camera-regression-only"
      : onlyTouch ? "touch-regression-only"
        : onlyTooltipFeed ? "tooltip-feed-regression-only"
          : onlyFocus ? "focus-regression-only"
            : "full-matrix",
  status: "running",
  startedAt: new Date().toISOString(),
  viewports: {},
  shortLandscape: {},
  feedStatuses: {},
  interactions: {},
  productionHotspot: {},
  thirdReview: { keyboard: [], longTooltips: [], glow: [] },
  secondReview: {
    camera: {},
    touchTargets: {},
    tooltipFeed: {},
    focusRestoration: {},
  },
};

mkdirSync(outDir, { recursive: true });

function assert(condition, message, details = undefined) {
  if (condition) return;
  const suffix = details === undefined ? "" : `\n${JSON.stringify(details, null, 2)}`;
  throw new Error(`${message}${suffix}`);
}

function run(command, args, { input, timeout = 30_000 } = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      rejectRun(new Error(`timed out: ${command} ${args.join(" ")}`));
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
        `command failed: ${command} ${args.join(" ")}`,
        `exit=${code ?? "null"} signal=${signal ?? "null"}`,
        stdout.trim() ? `stdout:\n${stdout.trim()}` : null,
        stderr.trim() ? `stderr:\n${stderr.trim()}` : null,
      ].filter(Boolean).join("\n")));
    });
    if (input !== undefined) child.stdin.end(input);
    else child.stdin.end();
  });
}

async function browserJson(args, options = {}) {
  const output = await run(browserBin, ["--session", session, "--headed", "--json", ...args], options);
  const parsed = JSON.parse(output);
  if (!parsed.success) throw new Error(parsed.error || `agent-browser failed: ${args.join(" ")}`);
  return parsed.data;
}

async function browserRaw(args, options = {}) {
  return run(browserBin, ["--session", session, "--headed", ...args], options);
}

async function evalJson(source) {
  const data = await browserJson(["eval", "--stdin"], { input: source });
  const result = data.result;
  return typeof result === "string" ? JSON.parse(result) : result;
}

const browserLifecycle = createOwnedSessionLifecycle({ command: browserBin, session });
const closeBrowser = browserLifecycle.close;
const prepareBrowserSession = browserLifecycle.prepare;

function contentType(pathname) {
  const type = extname(pathname);
  if (type === ".html") return "text/html; charset=utf-8";
  if (type === ".js" || type === ".mjs") return "text/javascript; charset=utf-8";
  if (type === ".css") return "text/css; charset=utf-8";
  if (type === ".wasm") return "application/wasm";
  if (type === ".ico") return "image/x-icon";
  return "application/octet-stream";
}

function serveFile(request, response) {
  const requestUrl = new URL(request.url || "/", "http://127.0.0.1");
  const pathname = requestUrl.pathname;
  if (pathname === "/pixel-world-bridge/pixel_world_bridge.js") {
    response.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" });
    response.end(bridgeModule);
    return;
  }
  const rawPath = decodeURIComponent(pathname === "/" ? "/viewer.html" : pathname);
  const normalized = normalize(rawPath).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = pathname === "/viewer.js" && statSafe(bundlePath)
    ? bundlePath
    : resolve(viewerRoot, `.${normalized}`);
  if (!relative(viewerRoot, filePath) || relative(viewerRoot, filePath).startsWith("..")) {
    response.writeHead(403); response.end("forbidden"); return;
  }
  try {
    let body = readFileSync(filePath, "utf8");
    if (pathname === "/viewer.html" && requestUrl.searchParams.get("browser_fixture") === "1") {
      const visualOverlayBootstrap = requestUrl.searchParams.get("visual_fixture") === "1"
        ? fixtureVisualOverlayBootstrap
        : "";
      body = body.replace('<script type="module" src="./viewer.js"></script>', `<script>${fixtureCanvasCompatibility}${visualOverlayBootstrap}</script><script>${fakeWebSocket}</script><script type="module" src="./viewer.js"></script>`);
    }
    response.writeHead(200, { "Content-Type": contentType(filePath), "Cache-Control": "no-store" });
    response.end(body);
  } catch {
    response.writeHead(404); response.end("not found");
  }
}

function statSafe(path) {
  try { return statSync(path).isFile(); } catch { return false; }
}

function fixtureUrl(port, { status = "ready", locale = "en", visualFixture = true, longHotspot = false } = {}) {
  const params = new URLSearchParams({
    browser_fixture: "1", test_api: "1", connect: "1", hosted_bootstrap: "0", ws: "ws://qa-fixture",
    locale, feed_status: status, t: Date.now().toString(),
  });
  if (longHotspot) params.set("long_hotspot", "1");
  if (visualFixture) {
    params.set("visual_fixture", "1");
    params.set("viewer_visual_fixture", "shell_selected_blocker");
    params.set("pixel_world_visual_fixture", "selected_blocker");
  }
  return `http://127.0.0.1:${port}/viewer.html?${params}`;
}

const probe = String.raw`(() => {
  const rect = (element) => { if (!element) return null; const r = element.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), right: Math.round(r.right), bottom: Math.round(r.bottom), width: Math.round(r.width), height: Math.round(r.height) }; };
  const text = (selector, root = document) => root.querySelector(selector)?.textContent.trim() || null;
  const intersects = (a, b) => Boolean(a && b && a.right > b.x && a.x < b.right && a.bottom > b.y && a.y < b.bottom);
  const stackAt = (box) => {
    if (!box) return [];
    const x = Math.max(1, Math.min(innerWidth - 1, box.x + Math.max(1, box.width / 2)));
    const y = Math.max(1, Math.min(innerHeight - 1, box.y + Math.max(1, box.height / 2)));
    return document.elementsFromPoint(x, y).slice(0, 6).map((node) => ({ tag: node.tagName, id: node.id || null, className: node.className || null, tooltipAncestor: node.closest?.(".pixel-world-canvas__hotspot-tooltip")?.className || null, overlay: node.getAttribute?.('data-viewer-overlay') || null, text: String(node.textContent || '').trim().slice(0, 160) }));
  };
  const feed = document.querySelector('[data-viewer-overlay="feed"]');
  const feedSummary = feed?.querySelector("summary");
  const command = document.querySelector('[data-viewer-overlay="next-move"]');
  const primary = command?.querySelector('[data-shell-region="next-move-primary"]') || command;
  const primaryAction = primary?.querySelector('.pixel-world-command-cell__action');
  const supporting = command?.querySelector('[data-shell-region="supporting-context"]');
  const receipt = document.querySelector('.pixel-world-action-receipt');
  const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
  const selection = document.querySelector('.pixel-world-canvas__selection');
  const readout = document.querySelector('.pixel-world-readout');
  const hotspotNodes = [...document.querySelectorAll('[data-hotspot-kind]')];
  const tooltipNode = document.querySelector('[data-hotspot-tooltip]');
  const tooltipClose = tooltipNode?.querySelector('.pixel-world-canvas__hotspot-tooltip-close');
  return JSON.stringify({
    runtime: window.__AW_TEST__?.getState?.() || null,
    feed: feed ? { status: feed.dataset.worldFeedStatus || null, open: feed.open, text: feed.textContent.trim(), rect: rect(feed), summaryRect: rect(feedSummary), statusRow: text('.world-feed__status-row', feed), scrollTop: feed.scrollTop, scrollHeight: feed.scrollHeight, clientHeight: feed.clientHeight } : null,
    readout: readout ? { text: readout.textContent.trim(), classes: [...readout.querySelectorAll('.badge')].map((node) => ({ text: node.textContent.trim(), className: node.className })) } : null,
    selection: selection ? { text: selection.textContent.trim(), rect: rect(selection) } : null,
    primary: primary ? { text: primary.textContent.trim(), rect: rect(primary), visible: getComputedStyle(primary).display !== 'none' && getComputedStyle(primary).visibility !== 'hidden', scrollTop: primary.scrollTop, scrollHeight: primary.scrollHeight, clientHeight: primary.clientHeight, actionRect: rect(primaryAction), actionText: primaryAction?.textContent.trim() || null } : null,
    supporting: supporting ? { text: supporting.textContent.trim(), rect: rect(supporting), visible: getComputedStyle(supporting).display !== 'none' } : null,
    receipt: receipt ? { present: receipt.dataset.receiptPresent, state: receipt.dataset.receiptState, confidence: receipt.dataset.receiptConfidence, text: receipt.textContent.trim(), rect: rect(receipt), visible: getComputedStyle(receipt).display !== 'none', scrollTop: receipt.scrollTop, scrollHeight: receipt.scrollHeight, clientHeight: receipt.clientHeight } : null,
    canvas: canvas ? { rect: rect(canvas), width: canvas.width, height: canvas.height } : null,
    tooltip: tooltipNode ? { text: text('[data-hotspot-tooltip]'), rect: rect(tooltipNode), stack: stackAt(rect(tooltipNode)), close: tooltipClose ? { ariaLabel: tooltipClose.getAttribute('aria-label'), rect: rect(tooltipClose), stack: stackAt(rect(tooltipClose)) } : null } : null,
    hotspots: hotspotNodes.map((node) => ({ tag: node.tagName, kind: node.dataset.hotspotKind, label: node.getAttribute('aria-label'), title: node.getAttribute('title'), role: node.getAttribute('role'), tabIndex: node.tabIndex, text: node.textContent.trim(), rect: rect(node), glyphRect: rect(node.querySelector(".pixel-world-hotspot__glyph")) })),
    viewport: { width: innerWidth, height: innerHeight, clientWidth: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth, overflowX: Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth) },
    visualFixture: Boolean(document.body.getAttribute("data-viewer-visual-fixture")),
    activeElement: (() => { const node = document.activeElement; return node ? { tag: node.tagName, id: node.id || null, className: node.className || null, ariaLabel: node.getAttribute?.('aria-label') || null, kind: node.getAttribute?.('data-hotspot-kind') || null, originalTrigger: node === window.__qaOriginalTrigger } : null; })(),
    hitTest: { selection: stackAt(selection ? rect(selection) : null), feed: stackAt(feed ? rect(feed) : null) },
    bodyText: document.body.innerText,
  });
})()`;

const waitReady = String.raw`(async () => { const end = Date.now() + 15000; while (Date.now() < end) { const state = window.__AW_TEST__?.getState?.() || {}; if (state.pixelWorldRuntimeStatus === "ready" && document.querySelector('[data-viewer-overlay="feed"]')) return JSON.stringify(state); await new Promise((resolve) => setTimeout(resolve, 100)); } throw new Error(JSON.stringify(window.__AW_TEST__?.getState?.() || null)); })()`;

function visible(textValue) { return textValue && textValue.trim().length > 0; }

function intersects(a, b) {
  return Boolean(a && b && a.right > b.x && a.x < b.right && a.bottom > b.y && a.y < b.bottom);
}

function rectCenter(rect) {
  return rect ? { x: rect.x + (rect.width / 2), y: rect.y + (rect.height / 2) } : null;
}

function cameraHotspotWorld(kind) {
  return {
    blocker: { x: 2_900_000, y: 3_450_000 },
    goal: { x: 7_150_000, y: 2_200_000 },
    info: { x: 4_550_000, y: 1_200_000 },
  }[kind] || null;
}

function projectedCanvasPoint(canvas, world, camera) {
  const width = Number(canvas?.width || 960);
  const height = Number(canvas?.height || 540);
  const cssWidth = Number(canvas?.rect?.width || 0);
  const cssHeight = Number(canvas?.rect?.height || 0);
  const normalizedX = Math.min(1, Math.max(0, Number(world?.x || 0) / 10_000_000));
  const normalizedY = Math.min(1, Math.max(0, Number(world?.y || 0) / 5_000_000));
  const baseX = 20 + (normalizedX * Math.max(1, width - 40));
  const baseY = 20 + (normalizedY * Math.max(1, height - 40));
  const zoom = Math.max(0.5, Number(camera?.zoom) || 1);
  const panX = Number(camera?.pan_x_px) || 0;
  const panY = Number(camera?.pan_y_px) || 0;
  const point = {
    x: (width / 2) + ((baseX - (width / 2)) * zoom) + panX,
    y: (height / 2) + ((baseY - (height / 2)) * zoom) + panY,
  };
  return {
    x: Number(canvas?.rect?.x || 0) + (point.x * (cssWidth / width)),
    y: Number(canvas?.rect?.y || 0) + (point.y * (cssHeight / height)),
    canvasPoint: point,
  };
}

function assertPointNear(actual, expected, tolerance, message, details = {}) {
  assert(actual && expected, `${message}: missing point`, { actual, expected, ...details });
  const dx = Math.abs(actual.x - expected.x);
  const dy = Math.abs(actual.y - expected.y);
  assert(dx <= tolerance && dy <= tolerance, `${message}: projected point drifted by ${dx.toFixed(2)}px/${dy.toFixed(2)}px`, {
    actual,
    expected,
    dx,
    dy,
    tolerance,
    ...details,
  });
}

function runtimeStable(before, after) {
  return before?.logicalTime === after?.logicalTime
    && before?.eventSeq === after?.eventSeq
    && before?.selectedKind === after?.selectedKind
    && before?.selectedId === after?.selectedId;
}

function stackHasClass(stack, classFragment) {
  return String(stack?.[0]?.className || "").includes(classFragment) || String(stack?.[0]?.tooltipAncestor || "").includes(classFragment);
}

async function waitForFeedStatus(status) {
  const end = Date.now() + 10_000;
  while (Date.now() < end) {
    const result = await evalJson(probe);
    if (result.feed?.status === status && result.runtime?.connectionStatus === "connected") return result;
    await browserRaw(["wait", "100"]);
  }
  return evalJson(probe);
}

async function waitForProductionHotspots() {
  const end = Date.now() + 10_000;
  while (Date.now() < end) {
    const result = await evalJson(probe);
    if (result.visualFixture === false && result.hotspots?.some((hotspot) => hotspot.kind === "goal" && hotspot.rect?.width > 0 && hotspot.rect?.height > 0)) return result;
    await browserRaw(["wait", "100"]);
  }
  return evalJson(probe);
}

async function openFixture(port, options) {
  await browserJson(["open", fixtureUrl(port, options)], { timeout: 45_000 });
  await browserJson(["set", "viewport", String(options.width || 390), String(options.height || 844)]);
  await evalJson(waitReady);
}

async function clickVisible(selector) {
  await browserJson(["scrollintoview", selector]);
  await browserJson(["click", selector]);
  return { selector };
}

async function clickVisibleInPlace(selector) {
  const selectorLiteral = JSON.stringify(selector);
  const target = await evalJson(`(() => { const node = document.querySelector(${selectorLiteral}); if (!node) throw new Error("missing visible target"); const rect = node.getBoundingClientRect(); const x = Math.max(1, Math.min(innerWidth - 1, rect.left + Math.max(1, rect.width / 2))); const y = Math.max(1, Math.min(innerHeight - 1, rect.top + Math.max(1, rect.height / 2))); const visible = rect.width > 0 && rect.height > 0 && rect.top >= 0 && rect.bottom <= innerHeight; const topHit = document.elementsFromPoint(x, y).slice(0, 4).map((hit) => ({ tag: hit.tagName, className: hit.className || null, kind: hit.getAttribute?.("data-hotspot-kind") || null })); return JSON.stringify({ visible, targetTop: node === document.elementFromPoint(x, y) || node.contains(document.elementFromPoint(x, y)), rect: { x: Math.round(rect.x), y: Math.round(rect.y), right: Math.round(rect.right), bottom: Math.round(rect.bottom) }, topHit }); })()`);
  assert(target.visible && target.targetTop, `target is not the visible top hit in place: ${selector}`, target);
  const x = Math.round((target.rect.x + target.rect.right) / 2);
  const y = Math.round((target.rect.y + target.rect.bottom) / 2);
  await browserJson(["mouse", "move", String(x), String(y)]);
  await browserJson(["mouse", "down"]);
  await browserJson(["mouse", "up"]);
  return { selector, target };
}

async function clickRecordedRect(rect, description) {
  assert(rect?.width > 0 && rect?.height > 0 && rect.x >= 0 && rect.y >= 0, `${description} is not visible in place`, rect);
  const x = Math.round((rect.x + rect.right) / 2);
  const y = Math.round((rect.y + rect.bottom) / 2);
  await browserJson(["mouse", "move", String(x), String(y)]);
  await browserJson(["mouse", "down"]);
  await browserJson(["mouse", "up"]);
  return { rect, x, y };
}

async function clickAtCoordinates(x, y, selector, description) {
  const selectorLiteral = JSON.stringify(selector);
  const point = await evalJson(`(() => {
    const target = document.querySelector(${selectorLiteral});
    const node = document.elementFromPoint(${Number(x)}, ${Number(y)});
    const stack = document.elementsFromPoint(${Number(x)}, ${Number(y)}).slice(0, 6).map((hit) => ({
      tag: hit.tagName,
      className: hit.className || null,
      kind: hit.getAttribute?.("data-hotspot-kind") || null,
      overlay: hit.getAttribute?.("data-viewer-overlay") || null,
    }));
    return JSON.stringify({
      targetPresent: Boolean(target),
      targetTop: Boolean(target && (node === target || target.contains(node))),
      stack,
    });
  })()`);
  assert(point.targetPresent && point.targetTop, `${description} is not the top hit at the requested point`, point);
  await browserJson(["mouse", "move", String(Math.round(x)), String(Math.round(y))]);
  await browserJson(["mouse", "down"]);
  await browserJson(["mouse", "up"]);
  return { x: Math.round(x), y: Math.round(y), point };
}

async function dispatchCanvasPan(deltaX, deltaY) {
  const delta = await evalJson(`(() => {
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    if (!canvas) throw new Error("missing runtime canvas");
    const rect = canvas.getBoundingClientRect();
    const start = { x: rect.left + (rect.width * 0.63), y: rect.top + (rect.height * 0.31) };
    const pointer = (type, x, y, buttons) => canvas.dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, pointerId: 7331, clientX: x, clientY: y, buttons, button: 0,
    }));
    pointer("pointerdown", start.x, start.y, 1);
    pointer("pointermove", start.x + ${Number(deltaX)}, start.y + ${Number(deltaY)}, 1);
    pointer("pointerup", start.x + ${Number(deltaX)}, start.y + ${Number(deltaY)}, 0);
    return JSON.stringify({ start, end: { x: start.x + ${Number(deltaX)}, y: start.y + ${Number(deltaY)} } });
  })()`);
  await waitShort();
  return delta;
}

async function dispatchCanvasZoom(deltaY) {
  const point = await evalJson(`(() => {
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    if (!canvas) throw new Error("missing runtime canvas");
    const rect = canvas.getBoundingClientRect();
    const x = rect.left + (rect.width * 0.56);
    const y = rect.top + (rect.height * 0.42);
    canvas.dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: ${Number(deltaY)}, deltaX: 0, clientX: x, clientY: y }));
    return JSON.stringify({ x, y });
  })()`);
  await waitShort();
  return point;
}

function cameraState(result) {
  return result.runtime?.pixelWorldCamera || result.runtime?.camera || null;
}

async function waitShort() { await browserRaw(["wait", "180"]); }

async function waitForTooltip() {
  const end = Date.now() + 5_000;
  while (Date.now() < end) {
    const result = await evalJson(probe);
    if (result.tooltip) return result;
    await browserRaw(["wait", "100"]);
  }
  return evalJson(probe);
}

function hotspotFor(result, kind) {
  return result.hotspots?.find((hotspot) => hotspot.kind === kind) || null;
}

function assertHotspotTooltipPainted(result, label) {
  const tooltip = result.tooltip;
  const viewport = result.viewport;
  assert(tooltip?.rect?.width > 0 && tooltip.rect.height > 0, `${label}: tooltip is not visible`, tooltip);
  assert(tooltip.rect.x >= 0 && tooltip.rect.y >= 0
    && tooltip.rect.right <= viewport.width && tooltip.rect.bottom <= viewport.height,
  `${label}: tooltip leaves viewport`, { tooltip, viewport });
  assert(stackHasClass(tooltip.stack, "pixel-world-canvas__hotspot-tooltip"),
    `${label}: tooltip is not the painted hit at its center`, tooltip);
  assert(tooltip.close?.rect?.width > 0 && tooltip.close.rect.height > 0,
    `${label}: tooltip close control is not visible`, tooltip);
  assert(stackHasClass(tooltip.close.stack, "pixel-world-canvas__hotspot-tooltip-close"),
    `${label}: tooltip close control is obscured at its painted center`, tooltip.close);
}

async function runOffscreenRegression(port) {
  summary.offscreen = [];
  for (const visualFixture of [true, false]) {
    await openFixture(port, { visualFixture, width: 768, height: 1024 });
    await dispatchCanvasZoom(-100);
    for (const edge of ["left", "right", "top", "bottom"]) {
      for (const partial of [false, true]) {
        const before = await evalJson(probe);
        const projected = projectedCanvasPoint(before.canvas, cameraHotspotWorld("info"), cameraState(before));
        const stage = before.canvas.rect;
        const outside = partial ? -3 : -80;
        const x = edge === "left" ? outside : edge === "right" ? stage.width - outside : stage.width / 2;
        const y = edge === "top" ? outside : edge === "bottom" ? stage.height - outside : stage.height / 2;
        await dispatchCanvasPan((stage.x + x - projected.x) * before.canvas.width / stage.width,
          (stage.y + y - projected.y) * before.canvas.height / stage.height);
        const state = await evalJson(`(() => {
          const node = document.querySelector('[data-hotspot-kind="info"]');
          window.__qaOffscreenNode ||= node;
          const stage = node.parentElement.getBoundingClientRect();
          const r = node.getBoundingClientRect();
          const glyph = node.querySelector('.pixel-world-hotspot__glyph').getBoundingClientRect();
          const rect = (r) => ({ x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height });
          const x = Math.max(stage.left + 22, Math.min(stage.right - 22, stage.left + ${x}));
          const y = Math.max(stage.top + 22, Math.min(stage.bottom - 22, stage.top + ${y}));
          document.querySelector('.mobile-rail__link').focus();
          node.focus();
          return JSON.stringify({ sameNode: node === window.__qaOffscreenNode, disabled:node.disabled, tabIndex:node.tabIndex,
            display:getComputedStyle(node).display, focused:document.activeElement === node,
            hitPresent:document.elementsFromPoint(x,y).some((hit) => hit === node || node.contains(hit)),
            rect:rect(r),glyph:rect(glyph),stage:rect(stage),x,y });
        })()`);
        assert(state.sameNode, "offscreen recovery replaced original control", state);
        if (partial) {
          assert(!state.disabled && state.tabIndex >= 0 && state.focused && state.rect.width >= 44 && state.rect.height >= 44,
            "partial glyph did not restore44px focusable target", { edge, state });
          assert(state.glyph.right > state.stage.x && state.glyph.x < state.stage.right && state.glyph.bottom > state.stage.y && state.glyph.y < state.stage.bottom,
            "partial-return glyph does not intersect stage", { edge, state });
          assert(state.rect.x >= state.stage.x && state.rect.y >= state.stage.y && state.rect.right <= state.stage.right + 1 && state.rect.bottom <= state.stage.bottom + 1,
            "partial target escapes stage", { edge, state });
          await browserRaw(["press", "Escape"]);
        } else {
          assert(state.disabled && state.tabIndex === -1 && !state.focused && !state.hitPresent && state.display === "none",
            "fully offscreen hotspot remains hit/focusable", { edge, state });
          await browserJson(["mouse", "move", String(Math.round(state.x)), String(Math.round(state.y))]);
          await browserJson(["mouse", "down"]); await browserJson(["mouse", "up"]);
          const clicked = await evalJson("JSON.stringify({ infoTooltip: Boolean(document.querySelector('[data-hotspot-tooltip][id$=info]')) })");
          assert(!clicked.infoTooltip, "offscreen edge click reopened invisible hotspot", { edge, clicked });
        }
        summary.offscreen.push({ visualFixture, edge, partial, state });
      }
    }
  }
}

async function runCameraProjectionRegression(port, { visualFixture, label }) {
  await openFixture(port, { status: "ready", locale: "en", width: 768, height: 1024, visualFixture });
  const initial = await waitForFeedStatus("ready");
  const hotspot = hotspotFor(initial, "info");
  const camera = cameraState(initial);
  assert(camera && initial.canvas?.rect?.width > 0 && initial.canvas?.rect?.height > 0,
    `${label}: camera/canvas state is unavailable`, { camera, canvas: initial.canvas });
  const world = cameraHotspotWorld("info");
  const initialExpected = projectedCanvasPoint(initial.canvas, world, camera);
  const initialActual = rectCenter(hotspot?.glyphRect || hotspot?.rect);
  assertPointNear(initialActual, initialExpected, 4, `${label}: default camera hotspot misses 20px-inset projection`, {
    camera,
    world,
    canvas: initial.canvas,
    hotspot,
  });

  const steps = [
    { name: "pan-both-axes", action: () => dispatchCanvasPan(48, -32) },
    { name: "zoom-in", action: () => dispatchCanvasZoom(-100) },
    { name: "zoom-out", action: () => dispatchCanvasZoom(100) },
  ];
  const checks = [{ name: "default", before: camera, after: camera, actual: initialActual, expected: initialExpected }];
  let previous = initial;
  for (const step of steps) {
    const beforeCamera = cameraState(previous);
    const beforeHotspot = hotspotFor(previous, "info");
    await step.action();
    const after = await evalJson(probe);
    const afterCamera = cameraState(after);
    const afterHotspot = hotspotFor(after, "info");
    assert(afterCamera && afterHotspot, `${label}/${step.name}: camera or hotspot disappeared`, { afterCamera, afterHotspot: afterHotspot?.rect, after });
    assert(afterCamera.pan_x_px !== beforeCamera.pan_x_px || afterCamera.pan_y_px !== beforeCamera.pan_y_px || afterCamera.zoom !== beforeCamera.zoom,
      `${label}/${step.name}: camera input did not change camera state`, { beforeCamera, afterCamera });
    const expected = projectedCanvasPoint(after.canvas, world, afterCamera);
    const actual = rectCenter(afterHotspot.glyphRect || afterHotspot.rect);
    assertPointNear(actual, expected, 4, `${label}/${step.name}: hotspot does not follow canvas projection`, {
      beforeCamera,
      afterCamera,
      beforeHotspot: beforeHotspot?.rect,
      afterHotspot: afterHotspot.rect,
      world,
      canvas: after.canvas,
    });
    checks.push({ name: step.name, before: beforeCamera, after: afterCamera, actual, expected });
    previous = after;
  }
  const beforeEdge = previous;
  const edgePoint = projectedCanvasPoint(beforeEdge.canvas, world, cameraState(beforeEdge));
  await dispatchCanvasPan(20 - edgePoint.canvasPoint.x, 0);
  const edge = await evalJson(probe);
  const edgeHotspot = hotspotFor(edge, "info");
  const edgeExpected = projectedCanvasPoint(edge.canvas, world, cameraState(edge));
  const edgeActual = rectCenter(edgeHotspot?.glyphRect || edgeHotspot?.rect);
  assertPointNear(edgeActual, edgeExpected, 4, `${label}: near-inset glyph projection drifted`, { edgeHotspot, camera: cameraState(edge) });
  assert(edgeHotspot.rect.x >= edge.canvas.rect.x - 1 && edgeHotspot.rect.right <= edge.canvas.rect.right + 1,
    `${label}: near-inset 44px target escapes stage`, { hotspot: edgeHotspot, canvas: edge.canvas });
  checks.push({ name: "near-20px-inset", actual: edgeActual, expected: edgeExpected, target: edgeHotspot.rect, camera: cameraState(edge) });
  const screenshot = join(outDir, `s3-camera-inset-${label}.png`);
  await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
  summary.secondReview.camera[label] = { screenshot, visualFixture, initial, checks };
}

async function runTouchTargetSurface(port, { visualFixture, label }) {
  await openFixture(port, { status: "ready", locale: "en", width: 390, height: 844, visualFixture });
  const initial = await waitForFeedStatus("ready");
  const expectedRuntime = initial.runtime;
  const taps = [];
  for (const kind of ["blocker", "goal", "info"]) {
    const beforePan = await evalJson(probe);
    const projected = projectedCanvasPoint(beforePan.canvas, cameraHotspotWorld(kind), cameraState(beforePan));
    // The fixed command HUD owns its painted region. Move the world point
    // through the real camera handler into clear stage before pointer taps.
    await dispatchCanvasPan(((kind === "goal" ? 380 : 180) - projected.x) * beforePan.canvas.width / beforePan.canvas.rect.width,
      (280 - projected.y) * beforePan.canvas.height / beforePan.canvas.rect.height);
    const reachable = await evalJson(probe);
    const hotspot = hotspotFor(reachable, kind);
    assert(hotspot?.rect?.width >= 44 && hotspot.rect.height >= 44,
      `${label}/${kind}: hotspot hit box is below 44px`, hotspot);
    assert(hotspot.rect.x >= 0 && hotspot.rect.y >= 0
      && hotspot.rect.right <= initial.viewport.width && hotspot.rect.bottom <= initial.viewport.height,
    `${label}/${kind}: hotspot hit box is not clamped into viewport`, { hotspot, viewport: initial.viewport });
    const points = [
      { edge: "top-left", x: hotspot.rect.x + 2, y: hotspot.rect.y + 2 },
      { edge: "top-right", x: hotspot.rect.right - 2, y: hotspot.rect.y + 2 },
      { edge: "bottom-left", x: hotspot.rect.x + 2, y: hotspot.rect.bottom - 2 },
      { edge: "bottom-right", x: hotspot.rect.right - 2, y: hotspot.rect.bottom - 2 },
    ];
    for (const point of points) {
      assert(point.x >= 0 && point.y >= 0 && point.x < initial.viewport.width && point.y < initial.viewport.height,
        `${label}/${kind}/${point.edge}: edge tap leaves viewport`, { point, hotspot: hotspot.rect, viewport: initial.viewport });
      const tap = await clickAtCoordinates(point.x, point.y, `[data-hotspot-kind="${kind}"]`, `${label}/${kind}/${point.edge}`);
      const opened = await waitForTooltip();
      assert(opened.tooltip?.text?.startsWith(`${kind === "blocker" ? "Blocker" : kind === "goal" ? "Goal" : "Info"}:`),
        `${label}/${kind}/${point.edge}: wrong tooltip opened`, { tap, tooltip: opened.tooltip, hotspots: opened.hotspots });
      assert(runtimeStable(expectedRuntime, opened.runtime), `${label}/${kind}/${point.edge}: hotspot tap changed runtime selection/time`, {
        before: expectedRuntime,
        after: opened.runtime,
      });
      assertHotspotTooltipPainted(opened, `${label}/${kind}/${point.edge}`);
      taps.push({ kind, camera: cameraState(reachable), cameraBefore: cameraState(beforePan), reason: "pan world target clear of fixed command HUD", edge: point.edge, point, target: hotspot.rect, opened: opened.tooltip, runtime: opened.runtime });
      await clickRecordedRect(opened.tooltip.close.rect, `${label}/${kind}/${point.edge} close`);
      await waitShort();
      const closed = await evalJson(probe);
      assert(!closed.tooltip, `${label}/${kind}/${point.edge}: close did not dismiss tooltip`, closed);
    }
  }
  summary.secondReview.touchTargets[label] = { visualFixture, initial, taps };
}

async function runTouchTargetRegression(port) {
  await runTouchTargetSurface(port, { visualFixture: true, label: "fixture" });
  await runTouchTargetSurface(port, { visualFixture: false, label: "production" });
}

async function runTooltipFeedSurface(port, { visualFixture, label, width, height, hotspotKind }) {
  await openFixture(port, { status: "ready", locale: "en", width, height, visualFixture });
  await waitForFeedStatus("ready");
  await clickVisible('[data-viewer-overlay="feed"] > summary');
  await waitShort();
  const feedOpen = await evalJson(probe);
  assert(feedOpen.feed?.open === true, `${label}: Feed did not open`, feedOpen.feed);
  const target = hotspotFor(feedOpen, hotspotKind);
  assert(target?.rect?.width > 0 && target.rect.height > 0, `${label}: ${hotspotKind} hotspot is not visible`, { target, feed: feedOpen.feed });
  await evalJson(`(() => { const node = document.querySelector('[data-hotspot-kind="${hotspotKind}"]'); if (!node) throw new Error("missing hotspot"); node.focus(); return JSON.stringify({ kind: node.dataset.hotspotKind, describedBy: node.getAttribute("aria-describedby") }); })()`);
  await browserRaw(["press", "Enter"]);
  const opened = await waitForTooltip();
  assert(opened.tooltip?.text?.startsWith(`${hotspotKind === "blocker" ? "Blocker" : "Goal"}:`), `${label}: wrong tooltip opened`, opened.tooltip);
  assertHotspotTooltipPainted(opened, label);
  assert(!intersects(opened.tooltip.rect, opened.feed?.summaryRect), `${label}: tooltip covers Feed summary action`, {
    tooltip: opened.tooltip.rect,
    summary: opened.feed?.summaryRect,
  });
  const screenshot = join(outDir, `s3-tooltip-feed-${label}.png`);
  await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
  await clickRecordedRect(opened.tooltip.close.rect, `${label} tooltip close`);
  await waitShort();
  const closed = await evalJson(probe);
  assert(!closed.tooltip, `${label}: pointer close did not dismiss tooltip`, closed);

  const blocker = hotspotFor(closed, "blocker");
  assert(blocker?.rect?.width > 0 && blocker.rect.height > 0, `${label}: blocker hotspot is not visible for Escape path`, { blocker, feed: closed.feed });
  await evalJson(`(() => { const node = document.querySelector('[data-hotspot-kind="blocker"]'); node.focus(); return JSON.stringify({ kind: node.dataset.hotspotKind }); })()`);
  await browserRaw(["press", "Enter"]);
  const beforeEscape = await waitForTooltip();
  assertHotspotTooltipPainted(beforeEscape, `${label} Escape path`);
  await browserRaw(["press", "Escape"]);
  await waitShort();
  const afterEscape = await evalJson(probe);
  assert(!afterEscape.tooltip, `${label}: Escape did not dismiss Feed-open tooltip`, afterEscape);
  summary.secondReview.tooltipFeed[label] = { screenshot, visualFixture, viewport: { width, height }, feedOpen, opened, closed, beforeEscape, afterEscape };
}

async function runTooltipFeedRegression(port) {
  const cases = [
    { width: 390, height: 844, hotspotKind: "goal", label: "fixture-mobile-goal" },
    { width: 844, height: 390, hotspotKind: "blocker", label: "fixture-landscape-blocker" },
    { width: 640, height: 360, hotspotKind: "goal", label: "fixture-compact-goal" },
    { width: 390, height: 844, hotspotKind: "goal", label: "production-mobile-goal", visualFixture: false },
    { width: 844, height: 390, hotspotKind: "blocker", label: "production-landscape-blocker", visualFixture: false },
    { width: 640, height: 360, hotspotKind: "goal", label: "production-compact-goal", visualFixture: false },
  ];
  for (const testCase of cases) {
    await runTooltipFeedSurface(port, { visualFixture: testCase.visualFixture !== false, ...testCase });
  }
}

async function focusHotspotAndActivate(kind) {
  const focused = await evalJson(`(() => { const node = document.querySelector('[data-hotspot-kind="${kind}"]'); if (!node) throw new Error("missing hotspot"); node.focus(); window.__qaOriginalTrigger = node; return JSON.stringify({ kind: node.dataset.hotspotKind, label: node.getAttribute("aria-label"), describedBy: node.getAttribute("aria-describedby"), active: document.activeElement === node }); })()`);
  assert(focused.active && focused.label && focused.describedBy, `focus/${kind}: trigger did not receive accessible focus`, focused);
  await browserRaw(["press", "Enter"]);
  const opened = await waitForTooltip();
  const relation = await evalJson(`(() => { const node = document.querySelector('[data-hotspot-kind="${kind}"]'); const id = node?.getAttribute("aria-describedby"); return JSON.stringify({ label: node?.getAttribute("aria-label"), describedBy: id, describedNode: Boolean(id && document.getElementById(id)), active: document.activeElement === node }); })()`);
  assert(relation.label && relation.describedNode, `focus/${kind}: open tooltip is not linked by aria-describedby`, relation);
  assert(opened.tooltip, `focus/${kind}: Enter did not open tooltip`, opened);
  return { focused, opened, relation };
}

async function runFocusRestorationSurface(port, { visualFixture, label }) {
  const cases = [];
  for (const kind of ["goal", "blocker"]) {
    await openFixture(port, { status: "ready", locale: "en", width: 390, height: 844, visualFixture });
    const triggerEscape = await focusHotspotAndActivate(kind);
    await browserRaw(["press", "Escape"]);
    await waitShort();
    const afterTriggerEscape = await evalJson(probe);
    assert(!afterTriggerEscape.tooltip && afterTriggerEscape.activeElement?.kind === kind && afterTriggerEscape.activeElement?.tag === "BUTTON" && afterTriggerEscape.activeElement?.originalTrigger,
      `${label}/${kind}: trigger Escape did not restore focus`, { afterTriggerEscape, triggerEscape });

    await openFixture(port, { status: "ready", locale: "en", width: 390, height: 844, visualFixture });
    const pointerClose = await focusHotspotAndActivate(kind);
    await clickRecordedRect(pointerClose.opened.tooltip.close.rect, `${label}/${kind} pointer close`);
    await waitShort();
    const afterPointerClose = await evalJson(probe);
    assert(!afterPointerClose.tooltip && afterPointerClose.activeElement?.kind === kind && afterPointerClose.activeElement?.tag === "BUTTON" && afterPointerClose.activeElement?.originalTrigger,
      `${label}/${kind}: pointer close did not restore focus`, { afterPointerClose, pointerClose });

    await openFixture(port, { status: "ready", locale: "en", width: 390, height: 844, visualFixture });
    const keyboardClose = await focusHotspotAndActivate(kind);
    let closeFocused = false;
    for (let tab = 0; tab < 8; tab += 1) {
      await browserRaw(["press", "Tab"]);
      const focus = await evalJson(probe);
      if (focus.activeElement?.className?.includes("pixel-world-canvas__hotspot-tooltip-close")) {
        closeFocused = true;
        break;
      }
    }
    assert(closeFocused, `${label}/${kind}: Tab did not reach tooltip close`, keyboardClose.opened);
    await browserRaw(["press", "Escape"]);
    await waitShort();
    const afterKeyboardClose = await evalJson(probe);
    assert(!afterKeyboardClose.tooltip && afterKeyboardClose.activeElement?.kind === kind && afterKeyboardClose.activeElement?.tag === "BUTTON" && afterKeyboardClose.activeElement?.originalTrigger,
      `${label}/${kind}: close Escape did not restore focus`, { afterKeyboardClose, keyboardClose });
    for (const [name, before, after] of [["trigger-Escape", triggerEscape.opened, afterTriggerEscape], ["pointer-close", pointerClose.opened, afterPointerClose], ["close-Escape", keyboardClose.opened, afterKeyboardClose]]) {
      assert(runtimeStable(before.runtime, after.runtime), `${label}/${kind}/${name}: inspection changed runtime`, { before: before.runtime, after: after.runtime });
    }
    cases.push({ kind, triggerEscape, afterTriggerEscape, pointerClose, afterPointerClose, keyboardClose, afterKeyboardClose });
  }
  summary.secondReview.focusRestoration[label] = { visualFixture, cases };
}

async function runFocusRestorationRegression(port) {
  await runFocusRestorationSurface(port, { visualFixture: true, label: "fixture" });
  await runFocusRestorationSurface(port, { visualFixture: false, label: "production" });
}

async function runThirdReview(port) {
  for (const visualFixture of [true, false]) {
    for (const { kind, feedOpen } of [{ kind: "blocker" }, { kind: "goal" }, { kind: "info" }, { kind: "info", feedOpen: true }]) {
      await openFixture(port, { visualFixture, width: 390, height: 844 });
      if (feedOpen) await clickVisibleInPlace(".world-feed__summary");
      const start = await focusHotspotAndActivate(kind);
      const successor = await evalJson(`(() => {
        const origin = window.__qaOriginalTrigger;
        const controls = [...document.querySelectorAll('button,a[href],input,select,textarea,summary,[tabindex]')].filter((node) =>
          node.tabIndex >= 0 && !node.disabled && !node.closest('[data-hotspot-tooltip]')
          && node.checkVisibility({ visibilityProperty: true }) && node.getClientRects().length);
        window.__qaNextControl = controls[controls.indexOf(origin) + 1];
        return JSON.stringify({ exists: Boolean(window.__qaNextControl), kind: window.__qaNextControl?.dataset?.hotspotKind, label: window.__qaNextControl?.getAttribute('aria-label'), tag: window.__qaNextControl?.tagName, className: window.__qaNextControl?.className, closedDetailsHiddenButtons: [...document.querySelectorAll('details:not([open]) button')].filter((node) => !node.checkVisibility({ visibilityProperty: true })).length });
      })()`);
      assert(successor.exists, "T2 keyboard: no native successor found", successor);
      if (kind === "info") assert(successor.tag === "SUMMARY" && successor.className.includes("world-feed__summary"), "F2 keyboard: last hotspot native successor must be visible Feed summary", successor);
      if (kind === "info" && !feedOpen) assert(successor.closedDetailsHiddenButtons > 0, "F2 keyboard: real DOM must contain hidden closed-details controls", successor);
      await browserRaw(["press", "Tab"]);
      let state = await evalJson(probe);
      assert(state.activeElement?.className?.includes("hotspot-tooltip-close"), "T2 keyboard: first Tab did not reach close", state.activeElement);
      await browserRaw(["press", "Shift+Tab"]);
      const backward = await evalJson("JSON.stringify({ original: document.activeElement === window.__qaOriginalTrigger })");
      assert(backward.original, "T2 keyboard: ShiftTab did not return to exact origin", backward);
      await browserRaw(["press", "Tab"]);
      state = await evalJson(probe);
      assert(state.activeElement?.className?.includes("hotspot-tooltip-close"), "T2 keyboard: repeated Tab did not reach close", state.activeElement);
      await browserRaw(["press", "Tab"]);
      const forward = await evalJson("JSON.stringify({ next: document.activeElement === window.__qaNextControl, tag: document.activeElement?.tagName, kind: document.activeElement?.dataset?.hotspotKind })");
      assert(forward.next, "T2 keyboard: forward Tab restarted document or trapped focus", { successor, forward });
      const after = await evalJson(probe);
      assert(runtimeStable(start.opened.runtime, after.runtime), "T2 keyboard traversal changed runtime", after.runtime);
      summary.thirdReview.keyboard.push({ visualFixture, kind, feedOpen: Boolean(feedOpen), successor, backward, forward });
    }
  }
  for (const viewport of [viewports[0], ...shortLandscapeViewports]) {
    for (const feedOpen of [false, true]) {
      await openFixture(port, { visualFixture: false, longHotspot: true, width: viewport.width, height: viewport.height });
      await waitForFeedStatus("ready");
      if (feedOpen) await clickVisible('[data-viewer-overlay="feed"] > summary');
      const label = `${viewport.name}-${feedOpen ? "open" : "collapsed"}`;
      const initial = await focusHotspotAndActivate("goal");
      assert(initial.opened.tooltip.text.length > 2000 && initial.opened.tooltip.text.includes("QA_LABEL_END"), `T2 ${label}: normal snapshot did not deliver long explanation`, initial.opened.tooltip);
      assertHotspotTooltipPainted(initial.opened, `T2 ${label}`);
      if (feedOpen) assert(!intersects(initial.opened.tooltip.rect, initial.opened.feed.summaryRect), `T2 ${label}: long tooltip covers Feed summary`, initial.opened);
      const bodySelector = '.pixel-world-canvas__hotspot-tooltip > span';
      const bodyBefore = await evalJson(`(() => { const node = document.querySelector('${bodySelector}'); return JSON.stringify({ scrollTop: node?.scrollTop, scrollHeight: node?.scrollHeight, clientHeight: node?.clientHeight }); })()`);
      assert(bodyBefore.scrollHeight > bodyBefore.clientHeight + 5, `T2 ${label}: long tooltip lacks bounded scroll body`, bodyBefore);
      await browserRaw(["scroll", "down", "10000", "--selector", bodySelector]);
      const bodyAfter = await evalJson(`(() => { const node = document.querySelector('${bodySelector}'); return JSON.stringify({ scrollTop: node.scrollTop, scrollHeight: node.scrollHeight, clientHeight: node.clientHeight }); })()`);
      assert(bodyAfter.scrollTop > 0 && bodyAfter.scrollTop + bodyAfter.clientHeight >= bodyAfter.scrollHeight - 3, `T2 ${label}: long explanation end cannot be reached`, bodyAfter);
      const scrolled = await evalJson(probe);
      assertHotspotTooltipPainted(scrolled, `T2 ${label} scrolled`);
      const screenshot = join(outDir, `t2-long-tooltip-${label}.png`);
      await browserRaw(["screenshot", screenshot]);
      const taps = [];
      for (const [edge, xSide, ySide] of [["top-left", "x", "y"], ["top-right", "right", "y"], ["bottom-left", "x", "bottom"], ["bottom-right", "right", "bottom"]]) {
        const opened = await evalJson(probe);
        const close = opened.tooltip?.close?.rect;
        assert(close?.width >= 44 && close.height >= 44, `T2 ${label}: sole touch close is below44px`, close);
        const x = close[xSide] + (xSide === "x" ? 2 : -2);
        const y = close[ySide] + (ySide === "y" ? 2 : -2);
        const tap = await clickAtCoordinates(x, y, '.pixel-world-canvas__hotspot-tooltip-close', `T2 ${label}/${edge}`);
        await waitShort();
        const closed = await evalJson(probe);
        assert(!closed.tooltip && closed.activeElement?.originalTrigger, `T2 ${label}/${edge}: close failed to dismiss and restore`, closed);
        assert(runtimeStable(initial.opened.runtime, closed.runtime), `T2 ${label}/${edge}: close changed runtime`, closed.runtime);
        taps.push({ edge, close, tap });
        if (edge !== "bottom-right") await focusHotspotAndActivate("goal");
      }
      summary.thirdReview.longTooltips.push({ viewport, feedOpen, screenshot, initial: initial.opened, bodyBefore, bodyAfter, taps });
    }
  }
  for (const visualFixture of [true, false]) {
    for (const viewport of [viewports[0], viewports[1]]) {
      await openFixture(port, { visualFixture, width: viewport.width, height: viewport.height });
      await waitForFeedStatus("ready");
      const glow = await evalJson(`JSON.stringify([...document.querySelectorAll('[data-hotspot-kind]')].map((node) => { const glyph = node.querySelector('.pixel-world-hotspot__glyph'); return { kind: node.dataset.hotspotKind, outerShadow: getComputedStyle(node).boxShadow, glyphShadow: getComputedStyle(glyph).boxShadow, outerWidth: node.getBoundingClientRect().width, glyphWidth: glyph.getBoundingClientRect().width }; }))`);
      assert(glow.length === 3 && glow.every((node) => node.outerShadow === "none" && node.glyphShadow !== "none" && node.outerWidth >= 44 && node.glyphWidth <= 32), "T2 responsive glow leaks onto44px hitbox", glow);
      const screenshot = join(outDir, `t2-glow-${visualFixture ? "fixture" : "production"}-${viewport.name}.png`);
      await browserRaw(["screenshot", screenshot]);
      summary.thirdReview.glow.push({ visualFixture, viewport, glow, screenshot });
    }
  }
}

function assertBase(label, result, expectedStatus) {
  assert(result.runtime?.pixelWorldRuntimeStatus === "ready", `${label}: runtime not ready`, result);
  assert(result.runtime?.renderMode === "viewer" || result.runtime?.renderMode === "software_safe", `${label}: unexpected render mode`, result.runtime);
  assert(result.feed?.status === expectedStatus, `${label}: expected feed status ${expectedStatus}`, result.feed);
  assert(result.selection?.rect?.width > 0 && visible(result.selection?.text), `${label}: selected object confirmation missing`, result.selection);
  assert(!/^.*agent-[a-z0-9_-]+.*$/i.test(result.selection?.text || ""), `${label}: raw selected agent id leaked`, result.selection);
  assert(result.primary?.visible && result.primary?.rect?.width > 0, `${label}: Next Move is not visible`, result.primary);
  assert(result.receipt?.visible && result.receipt?.rect?.width > 0, `${label}: Action Receipt is not visible`, result.receipt);
  assert(result.viewport.overflowX <= 2, `${label}: horizontal overflow ${result.viewport.overflowX}px`, result.viewport);
  if (result.feed && !result.feed.open) {
    assert(!intersects(result.feed.rect, result.selection.rect), `${label}: collapsed Feed overlaps selected-object confirmation`, {
      feed: result.feed.rect,
      selection: result.selection.rect,
      topHit: result.hitTest?.selection?.[0],
    });
  }
}

async function runStatusMatrix(port) {
  for (const status of statuses) {
    await openFixture(port, { status, locale: "en", width: 390, height: 844 });
    const result = await waitForFeedStatus(status);
    assertBase(`A2/${status}`, result, status);
    const readout = result.readout?.text || "";
    const expected = { ready: "LIVE", replay: "REPLAY", empty: "NO EVENTS", gap: "GAP", unavailable: "UNAVAILABLE" }[status];
    assert(readout.includes(expected), `A2/${status}: readout missing ${expected}`, result.readout);
    if (status !== "ready") assert(!readout.includes("LIVE"), `A2/${status}: context incorrectly labeled LIVE`, result.readout);
    const screenshot = join(outDir, `a2-${status}-mobile.png`);
    await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
    summary.feedStatuses[status] = { screenshot, result };
  }
}

async function runInteractions(port) {
  await openFixture(port, { status: "ready", locale: "en", width: 390, height: 844 });
  const before = await evalJson(probe);
  await clickVisible("button[data-pixel-world-agent-marker='true']");
  await waitShort();
  const afterAgent = await evalJson(probe);
  assert(afterAgent.selection?.rect?.width > 0 && visible(afterAgent.selection?.text), "A1/agent: selection confirmation missing after visible click", afterAgent.selection);
  await clickVisible("button[data-pixel-world-location-marker]");
  await waitShort();
  const afterLocation = await evalJson(probe);
  assert(afterLocation.selection?.rect?.width > 0 && visible(afterLocation.selection?.text), "A1/location: selection confirmation missing after visible click", afterLocation.selection);

  const feedBefore = afterLocation;
  await clickVisible('[data-viewer-overlay="feed"] > summary');
  await waitShort();
  const feedOpen = await evalJson(probe);
  assert(feedOpen.feed?.open === true, "A3: Feed did not open through visible summary control", feedOpen.feed);
  assert(feedOpen.primary?.visible && feedOpen.receipt?.visible, "A3: Next Move or Action Receipt hidden with Feed open", { primary: feedOpen.primary, receipt: feedOpen.receipt });
  assert(!intersects(feedOpen.feed.rect, feedOpen.primary.rect), "A3: Feed overlaps Next Move", { feed: feedOpen.feed.rect, primary: feedOpen.primary.rect });
  assert(!intersects(feedOpen.feed.rect, feedOpen.receipt.rect), "A3: Feed overlaps Action Receipt", { feed: feedOpen.feed.rect, receipt: feedOpen.receipt.rect });
  await browserRaw(["scroll", "down", "320", "--selector", '[data-viewer-overlay="feed"]']);
  await waitShort();
  const feedScrolled = await evalJson(probe);
  assert(feedScrolled.feed?.scrollTop > 0 || feedScrolled.feed?.scrollHeight <= feedScrolled.feed?.clientHeight, "A3: expanded Feed could not scroll its long detail surface", feedScrolled.feed);
  await browserRaw(["scroll", "down", "320", "--selector", '[data-viewer-overlay="receipt"]']);
  await waitShort();
  const receiptScrolled = await evalJson(probe);
  assert(receiptScrolled.receipt?.scrollTop > 0 || receiptScrolled.receipt?.scrollHeight <= receiptScrolled.receipt?.clientHeight, "A3: Action Receipt could not scroll its detail surface", receiptScrolled.receipt);
  const feedScreenshot = join(outDir, "a3-feed-open-mobile.png");
  await browserRaw(["screenshot", feedScreenshot], { timeout: 20_000 });
  await clickVisible('[data-viewer-overlay="feed"] > summary');

  const hotspots = feedOpen.hotspots || [];
  assert(hotspots.length >= 2, "A4: fixture did not expose blocker/goal hotspots", hotspots);
  for (const hotspot of hotspots) {
    assert(hotspot.tabIndex >= 0 || hotspot.role === "button" || hotspot.tag === "BUTTON" || visible(hotspot.label), `A4: hotspot ${hotspot.kind} has no keyboard/accessibility affordance`, hotspot);
  }
  const beforeInteractionState = before.runtime || {};
  // Scrolling the world can leave an Info explanation open. Close it through
  // its painted control before targeting a world point behind that overlay.
  const existingExplanation = await evalJson(probe);
  if (existingExplanation.tooltip) {
    assertHotspotTooltipPainted(existingExplanation, "A4 existing explanation");
    await clickRecordedRect(existingExplanation.tooltip.close.rect, "A4 existing explanation close");
    await waitShort();
    const dismissed = await evalJson(probe);
    assert(!dismissed.tooltip, "A4 existing explanation did not close before goal inspection", dismissed);
    assert(runtimeStable(existingExplanation.runtime, dismissed.runtime), "A4 precondition close changed runtime", dismissed.runtime);
  }
  const goalClick = await clickVisibleInPlace('[data-hotspot-kind="goal"]');
  const afterPointerHotspot = await waitForTooltip();
  assert(afterPointerHotspot.tooltip?.text?.includes("Goal: stabilize the first production line"), "A4: visible hotspot click did not expose a goal explanation", { tooltip: afterPointerHotspot.tooltip, goalClick });
  await clickRecordedRect(afterPointerHotspot.tooltip.close?.rect, "A4 tooltip close control");
  await waitShort();
  const afterPointerClose = await evalJson(probe);
  assert(!afterPointerClose.tooltip, "A4: visible tooltip close control did not dismiss the explanation", afterPointerClose);
  await evalJson(`(() => { const node = document.querySelector('[data-hotspot-kind="blocker"]'); if (!node) throw new Error("missing blocker hotspot"); node.scrollIntoView({ block: "center" }); node.focus?.(); return JSON.stringify({ active: document.activeElement === node, tag: node.tagName }); })()`);
  await browserRaw(["press", "Enter"]);
  const afterHotspot = await waitForTooltip();
  const explanation = afterHotspot.bodyText.includes("Blocker: iron input is exhausted") || afterHotspot.bodyText.includes("阻塞") || afterHotspot.bodyText.includes("iron input");
  assert(explanation, "A4: blocker hotspot did not expose a readable explanation after keyboard activation", { hotspots: afterHotspot.hotspots, bodyText: afterHotspot.bodyText.slice(-2500) });
  const afterHotspotState = afterHotspot.runtime || {};
  assert(afterHotspotState.logicalTime === beforeInteractionState.logicalTime && afterHotspotState.eventSeq === beforeInteractionState.eventSeq, "A4: hotspot inspection changed runtime time/event state", { before: beforeInteractionState, after: afterHotspotState });
  const hotspotScreenshot = join(outDir, "a4-hotspot-keyboard-mobile.png");
  await browserRaw(["screenshot", hotspotScreenshot], { timeout: 20_000 });
  await browserRaw(["press", "Escape"]);
  await waitShort();
  const afterEscape = await evalJson(probe);
  assert(!afterEscape.tooltip, "A4: Escape did not dismiss the keyboard-opened explanation", afterEscape);
  summary.interactions = { before: feedBefore, afterAgent, afterLocation, feedOpen: { ...feedOpen, screenshot: feedScreenshot, feedScrolled: feedScrolled.feed, receiptScrolled: receiptScrolled.receipt }, hotspot: { ...afterHotspot, screenshot: hotspotScreenshot }, pointerHotspot: afterPointerHotspot, pointerClose: afterPointerClose, afterEscape };
}

async function runViewportMatrix(port) {
  for (const viewport of viewports) {
    await openFixture(port, { status: "ready", locale: "en", width: viewport.width, height: viewport.height });
    const result = await waitForFeedStatus("ready");
    assertBase(`A1/A3/A5/${viewport.name}`, result, "ready");
    assert(result.bodyText.includes("Replenish upstream materials") || result.bodyText.includes("Missing Material"), `A5/${viewport.name}: blocking/next-action copy not present`, result.bodyText.slice(-2500));
    const screenshot = join(outDir, `a1-a3-a5-${viewport.name}.png`);
    await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
    summary.viewports[viewport.name] = { viewport, screenshot, result };
  }
  await openFixture(port, { status: "ready", locale: "zh-CN", width: 390, height: 844 });
  const cjk = await waitForFeedStatus("ready");
  assertBase("A5/CJK/mobile", cjk, "ready");
  assert(cjk.bodyText.includes("目标") || cjk.bodyText.includes("玩家杠杆") || cjk.bodyText.includes("行动回执"), "A5/CJK: localized hierarchy labels missing", cjk.bodyText.slice(-2500));
  const screenshot = join(outDir, "a5-cjk-mobile.png");
  await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
  summary.viewports.cjk = { viewport: { width: 390, height: 844 }, screenshot, result: cjk };

  await openFixture(port, { status: "ready", locale: "en", width: 390, height: 844 });
  const resized = {};
  for (const viewport of [
    { name: "resize-tablet", width: 768, height: 1024 },
    { name: "resize-desktop", width: 1440, height: 1000 },
    { name: "resize-mobile", width: 390, height: 844 },
  ]) {
    await browserJson(["set", "viewport", String(viewport.width), String(viewport.height)]);
    await waitShort();
    const result = await waitForFeedStatus("ready");
    assertBase(`A1/resize/${viewport.name}`, result, "ready");
    const screenshot = join(outDir, `a1-resize-${viewport.name}.png`);
    await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
    resized[viewport.name] = { viewport, screenshot, result };
  }
  summary.viewports.resize = resized;
}

async function runShortLandscapeMatrix(port) {
  for (const viewport of shortLandscapeViewports) {
    await openFixture(port, { status: "ready", locale: "en", width: viewport.width, height: viewport.height });
    const before = await waitForFeedStatus("ready");
    await clickVisible('[data-viewer-overlay="feed"] > summary');
    await waitShort();
    const opened = await evalJson(probe);
    const feedRect = opened.feed?.rect;
    const summaryRect = opened.feed?.summaryRect;
    assert(opened.feed?.open === true, `A3/${viewport.name}: Feed did not open`, opened.feed);
    assert(feedRect?.width > 0 && feedRect?.height > 0, `A3/${viewport.name}: Feed has no usable area`, opened.feed);
    const viewportHeight = Number(opened.viewport?.height);
    const feedTop = Number(feedRect?.y);
    const feedBottom = Number(feedRect?.bottom);
    const feedWithinViewport = Number.isFinite(viewportHeight) && Number.isFinite(feedTop) && Number.isFinite(feedBottom)
      && feedTop >= 0 && feedBottom <= viewportHeight;
    assert(feedWithinViewport, `A3/${viewport.name}: Feed leaves the viewport`, { feed: feedRect, viewport: opened.viewport, feedTop, feedBottom, viewportHeight });
    assert(feedRect.height >= Math.max(48, (summaryRect?.height || 0) + 8), `A3/${viewport.name}: Feed is too short for its summary`, { feed: feedRect, summary: summaryRect });
    assert(opened.feed.clientHeight > 0, `A3/${viewport.name}: Feed client height is zero`, opened.feed);
    assert(!intersects(feedRect, opened.primary?.rect), `A3/${viewport.name}: Feed overlaps Next Move`, { feed: feedRect, primary: opened.primary?.rect });
    assert(!intersects(feedRect, opened.receipt?.rect), `A3/${viewport.name}: Feed overlaps Action Receipt`, { feed: feedRect, receipt: opened.receipt?.rect });
    await browserRaw(["scroll", "down", "320", "--selector", '[data-viewer-overlay="feed"]']);
    await waitShort();
    const feedScrolled = await evalJson(probe);
    assert(feedScrolled.feed?.scrollTop > 0 || feedScrolled.feed?.scrollHeight <= feedScrolled.feed?.clientHeight, `A3/${viewport.name}: expanded Feed detail could not scroll`, feedScrolled.feed);
    await browserRaw(["scroll", "down", "320", "--selector", '[data-shell-region="next-move-primary"]']);
    await waitShort();
    const primaryScrolled = await evalJson(probe);
    const actionRect = primaryScrolled.primary?.actionRect;
    assert(primaryScrolled.primary?.scrollTop > 0 || primaryScrolled.primary?.scrollHeight <= primaryScrolled.primary?.clientHeight, `A3/${viewport.name}: Next Move detail could not scroll`, primaryScrolled.primary);
    assert(actionRect?.width > 0 && actionRect?.height > 0 && actionRect.y >= 0 && actionRect.bottom <= viewportHeight, `A3/${viewport.name}: Next Move action control is not reachable after scroll`, { primary: primaryScrolled.primary, viewport: primaryScrolled.viewport });
    const screenshot = join(outDir, `a3-${viewport.name}-feed-open.png`);
    await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
    summary.shortLandscape[viewport.name] = { viewport, before, opened: { ...opened, screenshot, feedScrolled: feedScrolled.feed, primaryScrolled: primaryScrolled.primary } };
    await clickVisible('[data-viewer-overlay="feed"] > summary');
  }
}

async function runProductionHotspotRegression(port) {
  // Keep the deterministic bridge/feed transport, but omit every visual-fixture
  // query and body marker. This exercises production overlay enablement rather
  // than allowing fixture-only controls to mask it.
  await openFixture(port, { status: "ready", locale: "zh-CN", width: 390, height: 844, visualFixture: false });
  await waitForFeedStatus("ready");
  const initial = await waitForProductionHotspots();
  assert(initial.visualFixture === false, "R2/production: visual fixture marker unexpectedly enabled", initial);
  assert(initial.hotspots.length >= 2, "R2/production: production path did not expose hotspot buttons", initial.hotspots);
  assert(initial.hotspots.every((hotspot) => hotspot.tag === "BUTTON" && hotspot.tabIndex >= 0), "R2/production: hotspot is not keyboard reachable", initial.hotspots);

  await evalJson(`(() => {
    window.__qaPointerTrace = [];
    for (const type of ["pointerdown", "pointerup", "click", "focusin", "mouseover", "mouseout"]) document.addEventListener(type, (event) => {
      const node = event.target;
      window.__qaPointerTrace.push({ type, time: performance.now(), x: event.clientX, y: event.clientY, tag: node?.tagName, className: node?.className, kind: node?.dataset?.hotspotKind, dismissed: node?.dataset?.dismissedHover, tooltip: Boolean(document.querySelector("[data-hotspot-tooltip]")) });
    }, true);
    return true;
  })()`);
  const zhGoalClick = await clickVisibleInPlace('[data-hotspot-kind="goal"]');
  const zhGoal = await waitForTooltip();
  assert(zhGoal.visualFixture === false, "R2/zh-CN: visual fixture marker unexpectedly enabled", zhGoal);
  assert(zhGoal.tooltip?.text?.includes("目标"), "R2/zh-CN: tooltip kind did not localize", { tooltip: zhGoal.tooltip, goalClick: zhGoalClick });
  assert(zhGoal.tooltip?.close?.ariaLabel?.includes("关闭"), "R2/zh-CN: tooltip close label did not localize", zhGoal.tooltip);
  await clickRecordedRect(zhGoal.tooltip.close?.rect, "R2/zh-CN tooltip close control");
  await waitShort();
  const zhClosed = await evalJson(probe);
  const pointerTrace = await evalJson("JSON.stringify(window.__qaPointerTrace)");
  writeFileSync(join(outDir, "production-pointer-trace.json"), JSON.stringify(pointerTrace, null, 2));
  assert(!zhClosed.tooltip, "R2/zh-CN: visible tooltip close did not dismiss", { zhClosed, zhGoal, trace: pointerTrace });
  for (const type of ["pointerdown", "pointerup", "click"]) {
    assert(pointerTrace.some((event) => event.type === type && event.className === "pixel-world-canvas__hotspot-tooltip-close"),
      `R2/zh-CN: close did not receive ${type}`, pointerTrace);
  }

  await evalJson(`(() => { const node = document.querySelector('[data-hotspot-kind="blocker"]'); if (!node) throw new Error("missing production blocker hotspot"); node.scrollIntoView({ block: "center" }); node.focus(); return JSON.stringify({ active: document.activeElement === node }); })()`);
  await browserRaw(["press", "Enter"]);
  const keyboardOpened = await waitForTooltip();
  assert(keyboardOpened.tooltip?.text?.includes("阻塞"), "R2/production: keyboard inspection did not expose localized blocker tooltip", keyboardOpened);
  let closeFocused = false;
  for (let tab = 0; tab < 8; tab += 1) {
    await browserRaw(["press", "Tab"]);
    const focus = await evalJson(probe);
    if (focus.activeElement?.className?.includes("pixel-world-canvas__hotspot-tooltip-close")) {
      closeFocused = true;
      assert(focus.activeElement.ariaLabel?.includes("关闭"), "R2/production: focused close control has wrong localized label", focus.activeElement);
      break;
    }
  }
  assert(closeFocused, "R2/production: Tab could not focus the tooltip close control", await evalJson(probe));
  await browserRaw(["press", "Escape"]);
  await waitShort();
  const afterTabEscape = await evalJson(probe);
  assert(!afterTabEscape.tooltip, "R2/production: Escape on focused tooltip close did not dismiss", afterTabEscape);
  const screenshot = join(outDir, "r2-production-zh-hotspot-tab-close.png");
  await browserRaw(["screenshot", screenshot], { timeout: 20_000 });
  summary.productionHotspot = { pointerTrace, initial, zhGoal, zhClosed, keyboardOpened, afterTabEscape, screenshot };
}

async function main() {
  if (!skipBuild) {
    const build = spawnSync("npm", ["--prefix", "crates/oasis7_viewer", "run", "build:viewer:bundle"], { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    writeFileSync(join(outDir, "bundle-build.log"), `${build.stdout || ""}${build.stderr || ""}`, "utf8");
    assert(build.status === 0, "Viewer bundle build failed; see bundle-build.log", { status: build.status, signal: build.signal });
  }
  assert(statSafe(bundlePath), `missing ${bundlePath}; build:viewer:bundle did not produce the test bundle`);
  summary.sourceDigests = Object.fromEntries([bundlePath, resolve(viewerRoot, "viewer.html"), resolve(viewerRoot, "viewer_base.css"), resolve(viewerRoot, "viewer_terminal_shell.css"), fileURLToPath(import.meta.url)].map((path) => [relative(repoRoot, path), createHash("sha256").update(readFileSync(path)).digest("hex")]));
  assert(spawnSync(browserBin, ["--version"], { stdio: "ignore" }).status === 0, `missing browser automation command: ${browserBin}`);
  const server = createServer(serveFile);
  await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
  const address = server.address();
  const port = address.port;
  try {
    prepareBrowserSession();
    if (onlyOffscreen) {
      await runOffscreenRegression(port);
    } else if (onlyThirdReview) {
      await runThirdReview(port);
    } else if (onlyProduction) {
      await runProductionHotspotRegression(port);
    } else if (onlyCamera) {
      await runCameraProjectionRegression(port, { visualFixture: true, label: "fixture" });
      await runCameraProjectionRegression(port, { visualFixture: false, label: "production" });
    } else if (onlyTouch) {
      await runTouchTargetRegression(port);
    } else if (onlyTooltipFeed) {
      await runTooltipFeedRegression(port);
    } else if (onlyFocus) {
      await runFocusRestorationRegression(port);
    } else {
      await runOffscreenRegression(port);
      await runThirdReview(port);
      await runStatusMatrix(port);
      await runViewportMatrix(port);
      await runShortLandscapeMatrix(port);
      await runCameraProjectionRegression(port, { visualFixture: true, label: "fixture" });
      await runCameraProjectionRegression(port, { visualFixture: false, label: "production" });
      await runTouchTargetRegression(port);
      await runTooltipFeedRegression(port);
      await runFocusRestorationRegression(port);
      await runProductionHotspotRegression(port);
      await runInteractions(port);
    }
    const consoleOutput = await browserRaw(["console"]);
    writeFileSync(join(outDir, "browser-console.log"), consoleOutput, "utf8");
    assert(!/\[(?:error|pageerror)\]|\b(?:fatal|CONTEXT_LOST_WEBGL)\b/i.test(consoleOutput), "browser console contains runtime errors", { consoleOutput });
    for (const [path, digest] of Object.entries(summary.sourceDigests)) {
      assert(createHash("sha256").update(readFileSync(resolve(repoRoot, path))).digest("hex") === digest,
        `source bytes changed during headed run: ${path}`);
    }
    summary.console = { path: join(outDir, "browser-console.log"), errors: 0 };
    summary.status = "passed";
  } catch (error) {
    summary.status = "failed";
    summary.failure = String(error?.stack || error);
    try {
      writeFileSync(join(outDir, "failure-state.json"), JSON.stringify(await evalJson(probe), null, 2), "utf8");
      await browserRaw(["screenshot", join(outDir, "failure.png")], { timeout: 20_000 });
      writeFileSync(join(outDir, "browser-console.log"), await browserRaw(["console"]), "utf8");
    } catch (diagnosticError) {
      summary.failureDiagnostics = String(diagnosticError?.stack || diagnosticError);
    }
    throw error;
  } finally {
    summary.completedAt = new Date().toISOString();
    writeFileSync(join(outDir, "summary.json"), `${JSON.stringify(summary, null, 2)}\n`, "utf8");
    closeBrowser();
    await new Promise((resolveServer) => server.close(resolveServer));
  }
  console.log(`player visual feedback browser smoke passed: ${outDir}`);
}

main().catch((error) => {
  console.error(error?.stack || error);
  process.exitCode = 1;
});
