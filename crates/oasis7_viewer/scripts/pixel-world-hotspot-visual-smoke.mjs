import { serveStaticFile } from "./viewer-static-files.mjs";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { completionEvidence } from './pixel-world-completion-evidence.mjs';
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const browserLaunchArgs = process.env.AGENT_BROWSER_HEADED === "1" ? ["--headed"] : [];
const viewerRoot = resolve(scriptDir, "..");
const repoRoot = resolve(viewerRoot, "../..");
const artifactRoot = resolve(viewerRoot, ".viewer-test-dist");
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = resolve(repoRoot, "output/playwright/pixel-world-hotspot-visual", runId);
const agentBrowserBin = process.env.AGENT_BROWSER_BIN || "agent-browser";
const session = `pixel-world-hotspot-visual-${process.pid}`;
const routeMotionEvidence = process.argv.includes('--routes-and-motion');
const completionRun = process.argv.includes('--completion');
const fixtureName = routeMotionEvidence ? 'routes_and_events' : 'recent_event_glyphs';
const summary = { status: "running", fixtureName, startedAt: new Date().toISOString(), viewports: {} };
mkdirSync(outDir, { recursive: true });

function fail(message, details) { throw new Error(`${message}${details ? `\n${JSON.stringify(details, null, 2)}` : ""}`); }
function assert(condition, message, details) { if (!condition) fail(message, details); }
function writeJson(name, value) { const path = join(outDir, name); writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`); return path; }
function contentType(pathname) { return ({ ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".wasm": "application/wasm", ".css": "text/css; charset=utf-8" })[extname(pathname)] || "application/octet-stream"; }
function serveFile(request, response) {
  serveStaticFile(request, response, artifactRoot, contentType);
}
function ensureBrowser() { if (spawnSync(agentBrowserBin, ["--version"], { stdio: "ignore" }).status !== 0) fail(`missing required browser automation command: ${agentBrowserBin}`); }
const browserLifecycle = createOwnedSessionLifecycle({ command: agentBrowserBin, session, prefixArgs: browserLaunchArgs });
const closeBrowser = browserLifecycle.close;
const prepareBrowserSession = browserLifecycle.prepare;
function runBrowser(args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(agentBrowserBin, ["--session", session, ...browserLaunchArgs, ...args], { stdio: ["pipe", "pipe", "pipe"] }); let stdout = ""; let stderr = "";
    const timer = setTimeout(() => { child.kill("SIGTERM"); rejectRun(new Error(`agent-browser timed out: ${args.join(" ")}`)); }, options.timeout ?? 30_000);
    child.stdout.setEncoding("utf8"); child.stderr.setEncoding("utf8"); child.stdout.on("data", (chunk) => { stdout += chunk; }); child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", (error) => { clearTimeout(timer); rejectRun(error); }); child.on("close", (code) => { clearTimeout(timer); code === 0 ? resolveRun(stdout) : rejectRun(new Error(`agent-browser failed: ${args.join(" ")}\n${stdout}\n${stderr}`)); });
    if (options.input !== undefined) child.stdin.end(options.input);
  });
}
async function browserJson(args, options) { const output = await runBrowser(["--json", ...args], options); const parsed = JSON.parse(output); if (!parsed.success) fail(parsed.error || "agent-browser JSON failure"); return parsed.data; }
async function evalJson(source) { const data = await browserJson(["eval", source], { timeout: 60_000 }); return typeof data.result === "string" ? JSON.parse(data.result) : data.result; }
function screenshotBitmap(path) { const bmpPath = path.replace(/\.png$/, ".bmp"); const converted = spawnSync("sips", ["-s", "format", "bmp", path, "--out", bmpPath], { encoding: "utf8" }); if (converted.status !== 0) fail("could not inspect screenshot pixels", { path, stderr: converted.stderr }); const bytes = readFileSync(bmpPath); const offset = bytes.readUInt32LE(10); const width = bytes.readInt32LE(18); const height = Math.abs(bytes.readInt32LE(22)); const bitCount = bytes.readUInt16LE(28); return { bytes, offset, width, height, bitCount, stride: Math.ceil((width * bitCount) / 32) * 4 }; }
function pixelAt(bitmap, x, y) { const index = bitmap.offset + (y * bitmap.stride) + (x * (bitmap.bitCount / 8)); return [bitmap.bytes[index], bitmap.bytes[index + 1], bitmap.bytes[index + 2]]; }
function screenshotStats(path) { const bitmap = screenshotBitmap(path); let brightness = 0; let nonBlack = 0; let count = 0; for (let y = 0; y < bitmap.height; y += 1) { for (let x = 0; x < bitmap.width; x += 1) { const value = pixelAt(bitmap, x, y).reduce((total, channel) => total + channel, 0); brightness += value; if (value > 24) nonBlack += 1; count += 1; } } return { meanBrightness: Number((brightness / Math.max(1, count)).toFixed(2)), nonBlackRatio: Number((nonBlack / Math.max(1, count)).toFixed(4)) }; }
function screenshotDifference(leftPath, rightPath) { const left = screenshotBitmap(leftPath); const right = screenshotBitmap(rightPath); assert(left.width === right.width && left.height === right.height && left.bitCount === right.bitCount, "visible and cleared screenshot formats differ", { left, right }); let changed = 0; let totalDelta = 0; const total = left.width * left.height; for (let y = 0; y < left.height; y += 1) { for (let x = 0; x < left.width; x += 1) { const delta = pixelAt(left, x, y).reduce((sum, channel, index) => sum + Math.abs(channel - pixelAt(right, x, y)[index]), 0); totalDelta += delta; if (delta > 12) changed += 1; } } return { changedPixelRatio: Number((changed / Math.max(1, total)).toFixed(6)), meanChannelDelta: Number((totalDelta / Math.max(1, total * 3)).toFixed(4)) }; }
function cropGlyph(sourcePath, outputPath, center) { const side = 48; const offsetY = Math.max(0, Math.round(center.y - side / 2)); const offsetX = Math.max(0, Math.round(center.x - side / 2)); const cropped = spawnSync("sips", ["--cropToHeightWidth", String(side), String(side), "--cropOffset", String(offsetY), String(offsetX), sourcePath, "--out", outputPath], { encoding: "utf8" }); if (cropped.status !== 0) fail("could not crop glyph evidence", { sourcePath, outputPath, center, stderr: cropped.stderr }); const scaledPath = outputPath.replace(/\.png$/, "-8x.png"); const scaled = spawnSync("sips", ["--resampleWidth", String(side * 8), outputPath, "--out", scaledPath], { encoding: "utf8" }); if (scaled.status !== 0) fail("could not enlarge glyph evidence", { outputPath, scaledPath, stderr: scaled.stderr }); return scaledPath; }
function pageStateScript() { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); const state = window.__AW_TEST__?.getState?.() || {}; const gl = canvas?.getContext('webgl2') || canvas?.getContext('webgl'); const debug = gl?.getExtension('WEBGL_debug_renderer_info'); return { rendererReady: document.querySelector('.pixel-world-canvas')?.dataset.rendererReady === 'true', runtimeStatus: state.pixelWorldRuntimeStatus, runtimeSource: state.pixelWorldRuntimeSource, fatal: state.pixelWorldFatal || state.lastError || null, canvas: canvas ? { width: canvas.width, height: canvas.height, rect: (() => { const r = canvas.getBoundingClientRect(); return { width:r.width, height:r.height }; })() } : null, browserEnv: { userAgent: navigator.userAgent, devicePixelRatio: window.devicePixelRatio, webglRenderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : null, webglVendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : null }, fixture: document.querySelector('.pixel-world-host')?.dataset.visualFixture || null, renderDto: window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.() || null }; })()`; }
function bringCanvasIntoViewScript() { return String.raw`(async () => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); const host = document.querySelector('.pixel-world-host'); if (!canvas || !host) throw new Error('canvas host unavailable'); canvas.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' }); await new Promise((resolve) => requestAnimationFrame(() => resolve())); const rect = canvas.getBoundingClientRect(); return JSON.stringify({ scrollY: window.scrollY, viewport: { width: window.innerWidth, height: window.innerHeight }, devicePixelRatio: window.devicePixelRatio, canvas: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height, bitmapWidth: canvas.width, bitmapHeight: canvas.height, scaleX: canvas.width / rect.width, scaleY: canvas.height / rect.height } }); })()`; }
function panHotspotsIntoSafeBandScript() { return String.raw`(async () => {
  const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
  const probe = window.__OASIS7_PIXEL_WORLD_HOTSPOT_POINTER_PROBE__;
  const panel = document.querySelector('.pixel-world-decision-area');
  if (!canvas || !probe || !panel) throw new Error('hotspot viewport correction targets unavailable');
  const rect = canvas.getBoundingClientRect();
  const panelRect = panel.getBoundingClientRect();
  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  const targets = () => probe.targets().filter((target) => ['recent:resource-transfer-fixture', 'recent:build-queue-fixture'].includes(target.id));
  const project = (target) => ({ id: target.id, x: rect.left + (Number(target.canvas_x) / scaleX), y: rect.top + (Number(target.canvas_y) / scaleY) });
  const clearance = (point) => point.x + 22 <= panelRect.left || point.x - 22 >= panelRect.right || point.y + 22 + 7 <= panelRect.top;
  const pointerId = 731;
  const steps = [];
  let before = targets();
  const beforeViewport = before.map(project);
  let after = before;
  for (let iteration = 0; iteration < 8; iteration += 1) {
    const afterViewport = after.map(project);
    const obstructed = afterViewport.filter((point) => !clearance(point));
    if (obstructed.length === 0) break;
    const desiredY = Math.max(28, panelRect.top - 30);
    const deltaRawY = Math.round((desiredY - Math.min(...obstructed.map((point) => point.y))) * scaleY);
    if (deltaRawY === 0) break;
    const startX = rect.left + (rect.width / 2);
    const startY = rect.top + (rect.height / 2);
    const endY = startY + deltaRawY;
    const originalCapture = { set: canvas.setPointerCapture, release: canvas.releasePointerCapture };
    canvas.setPointerCapture = () => {};
    canvas.releasePointerCapture = () => {};
    try {
      canvas.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientX: startX, clientY: startY, pointerId: pointerId + iteration, buttons: 1 }));
      canvas.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: startX, clientY: endY, pointerId: pointerId + iteration, buttons: 1 }));
      canvas.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: startX, clientY: endY, pointerId: pointerId + iteration }));
    } finally {
      canvas.setPointerCapture = originalCapture.set;
      canvas.releasePointerCapture = originalCapture.release;
    }
    const prior = after;
    const deadline = Date.now() + 1500;
    while (Date.now() < deadline) {
      await new Promise((resolve) => requestAnimationFrame(resolve));
      after = targets();
      if (after.some((target, index) => Math.abs(Number(target.canvas_y) - Number(prior[index]?.canvas_y || target.canvas_y)) > 1)) break;
    }
    steps.push({ iteration, deltaRawY, before: prior, beforeViewport: prior.map(project), after, afterViewport: after.map(project) });
    if (!after.some((target, index) => Math.abs(Number(target.canvas_y) - Number(prior[index]?.canvas_y || target.canvas_y)) > 1)) break;
  }
  return JSON.stringify({
    canvas: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height, bitmapWidth: canvas.width, bitmapHeight: canvas.height, scaleX, scaleY },
    panel: { left: panelRect.left, top: panelRect.top, right: panelRect.right, bottom: panelRect.bottom },
    before,
    beforeViewport,
    after,
    afterViewport: after.map(project),
    pan: { pointerId, steps },
  });
})()`; }
function receiptScript(method, id) { return String.raw`(async () => {
  const probe = window.__OASIS7_PIXEL_WORLD_HOTSPOT_POINTER_PROBE__;
  const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
  if (!probe || !canvas) throw new Error('test-only hotspot pointer probe unavailable');
  const originalGetBoundingClientRect = canvas.getBoundingClientRect;
  const hotspot = ${id ? `probe.targets().find((target) => target.id === ${JSON.stringify(id)})` : 'null'};
  const originalRect = originalGetBoundingClientRect.call(canvas);
  const inputScale = {
    x: canvas.width / Math.max(1, originalRect.width),
    y: canvas.height / Math.max(1, originalRect.height),
  };
  if (${JSON.stringify(method)} === 'hover') {
    if (!hotspot) throw new Error('test-only hotspot target unavailable for scaled pointer probe');
    // The probe owns the authoritative target readback, while this temporary
    // rect converts its backing-pixel point into a CSS client coordinate for
    // the adapter's backing-scale input path.
    const probeRect = {
      ...originalRect,
      left: originalRect.left + (Number(hotspot.canvas_x) / inputScale.x) - Number(hotspot.canvas_x),
      top: originalRect.top + (Number(hotspot.canvas_y) / inputScale.y) - Number(hotspot.canvas_y),
    };
    let probeRectRead = false;
    canvas.getBoundingClientRect = () => {
      if (!probeRectRead) {
        probeRectRead = true;
        // Restore before probe.hover dispatches its PointerEvent so the
        // production adapter reads the actual canvas rect and scales once.
        canvas.getBoundingClientRect = originalGetBoundingClientRect;
        return probeRect;
      }
      return originalGetBoundingClientRect.call(canvas);
    };
  }
  let receipt;
  try {
    receipt = await probe.${method}(${id ? JSON.stringify(id) : ""});
  } finally {
    canvas.getBoundingClientRect = originalGetBoundingClientRect;
  }
  const tooltip = document.querySelector('[data-hotspot-tooltip]');
  const viewport = { width: window.innerWidth, height: window.innerHeight };
  const rect = tooltip?.getBoundingClientRect();
  return JSON.stringify({ receipt, viewport, inputScale, tooltip: tooltip ? { text: tooltip.textContent.trim(), visible: getComputedStyle(tooltip).display !== 'none', rect: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height } } : null });
})()`; }
function targetsScript() { return String.raw`(() => { const probe = window.__OASIS7_PIXEL_WORLD_HOTSPOT_POINTER_PROBE__; if (!probe) throw new Error('test-only hotspot pointer probe unavailable'); return JSON.stringify(probe.targets()); })()`; }
function selectionGeometryScript() { return String.raw`(async () => {
  await new Promise(resolve => setTimeout(resolve, 150));
  const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
  const rect = canvas.getBoundingClientRect();
  const scaleX = canvas.width / Math.max(1, rect.width);
  const scaleY = canvas.height / Math.max(1, rect.height);
  const state = window.__AW_TEST__.getState();
  const camera = state.pixelWorldCamera;
  const dto = window.__OASIS7_PIXEL_WORLD_RENDER_DTO__();
  const agent = dto.agents.find(agent => agent.id === 'agent-0');
  const targets = [...document.querySelectorAll('[data-renderer-target="true"][data-agent-id="agent-0"]')];
  const targetNode = targets[0];
  const target = targetNode?.getBoundingClientRect() || { left: 0, top: 0, width: 0, height: 0 };
  const targetStyle = targetNode ? getComputedStyle(targetNode) : null;
  const bounds = dto.world_bounds;
  const rendererWidth = canvas.width;
  const rendererHeight = canvas.height;
  const baseX = 20 + (agent.pos.x_cm / bounds.width_cm) * (rendererWidth - 40);
  const baseY = 20 + (agent.pos.y_cm / bounds.depth_cm) * (rendererHeight - 40);
  const expected = {
    x: rect.left + (rendererWidth / 2 + (baseX - rendererWidth / 2) * camera.zoom + camera.pan_x_px) / scaleX,
    y: rect.top + (rendererHeight / 2 + (baseY - rendererHeight / 2) * camera.zoom + camera.pan_y_px) / scaleY,
  };
  const actual = { x: target.left + target.width / 2, y: target.top + target.height / 2 };
  return JSON.stringify({ camera, expected, actual, count: targets.length, width: target.width, height: target.height, targetStyle: targetStyle ? { left: targetStyle.left, top: targetStyle.top, display: targetStyle.display, visibility: targetStyle.visibility } : null, canvas: { cssWidth: rect.width, cssHeight: rect.height, bitmapWidth: rendererWidth, bitmapHeight: rendererHeight, scaleX, scaleY }, error: Math.hypot(expected.x-actual.x,expected.y-actual.y), selection: dto.selection });
})()`; }

ensureBrowser();
const server = createServer(serveFile);
try {
  await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
  const address = server.address();
  const url = `http://127.0.0.1:${address.port}/viewer.html?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=${fixtureName}`;
  summary.url = url; prepareBrowserSession(); await new Promise((resolve) => setTimeout(resolve, 500)); await browserJson(["open", url], { timeout: 45_000 });
  for (const [name, width, height] of [["desktop", 1440, 1000], ["narrow", 390, 844], ...((completionRun || routeMotionEvidence) ? [['compact',320,568]] : [])]) {
    await browserJson(["set", "viewport", String(width), String(height)]);
    if (name !== 'desktop') await browserJson(['open',url]);
    await evalJson(`(async()=>{for(let n=0;n<1200;n++){if(window.__OASIS7_VIEWER_FIXTURE_READY__) return true; await new Promise(r=>setTimeout(r,50));} throw new Error("test fixture never became ready");})()`);
    let state = await evalJson(String.raw`(async()=>{const read=()=>(${pageStateScript()}); const deadline=Date.now()+15000; while(Date.now()<deadline){const s=read(); if(s.rendererReady && s.runtimeStatus==='ready') return JSON.stringify(s); await new Promise(r=>setTimeout(r,100));} throw new Error('renderer not ready');})()`);
    if (routeMotionEvidence || name === 'compact') {
      await evalJson(`(async()=>{for(let n=0;n<${name === 'compact' && routeMotionEvidence ? 3 : 1};n++){document.querySelector('#pixel-world-embedded-runtime-canvas').dispatchEvent(new WheelEvent('wheel',{deltaY:300,bubbles:true,cancelable:true}));await new Promise(r=>setTimeout(r,80));}await new Promise(r=>setTimeout(r,250));return true;})()`);
      state=await evalJson(pageStateScript());
    }
    assert(state.fixture === fixtureName && state.rendererReady && state.runtimeStatus === "ready" && !state.fatal, "fixture renderer did not become ready", state);
    if (routeMotionEvidence) {
      const links = state.renderDto.links || [];
      const assignments = links.filter((link) => link.kind === 'agent_assignment');
      const generic = links.find((link) => link.id === 'link:agent-route:loc-route');
      const zeroLength = links.find((link) => link.id === 'link:agent-zero-route:loc-zero-route');
      assert(assignments.length === 2 && assignments.every((link) => link.from && link.to && link.source_class === 'runtime_projection'), 'published assignment links missing from real Rust DTO', links);
      assert(generic?.kind === 'logistics_route' && generic.label === 'Ore logistics route' && generic.status === 'active' && generic.freshness === 'current', 'generic logistics route missing from real Rust DTO', links);
      assert(zeroLength?.kind === 'resource_flow' && zeroLength.from && zeroLength.to && zeroLength.from.x_cm === zeroLength.to.x_cm && zeroLength.from.y_cm === zeroLength.to.y_cm, 'zero-length generic route control missing from real Rust DTO', links);
      assert(!links.some((link) => link.id.includes('unknown-route') || link.id.includes('stale-route')), 'unknown/stale generic controls must fail closed in real Rust DTO', links);
    }
    assert(state.renderDto?.agents?.some((agent) => agent.id === "agent-0"), "fixture Render DTO omits agent-0", state.renderDto);
    assert(state.renderDto?.receipt_target?.agent_id === "agent-0" && state.renderDto?.receipt_target?.state === "blocked", "fixture Render DTO omits the blocked agent-0 receipt target", state.renderDto);
    const glyphKinds = Object.fromEntries((state.renderDto?.visual_hotspots || []).map((hotspot) => [hotspot.id, hotspot.kind]));
    assert(glyphKinds["recent:resource-transfer-fixture"] === "resource_transfer" && glyphKinds["recent:build-queue-fixture"] === "build_queue", "fixture Render DTO omits the two event-kind hotspots", { glyphKinds, renderDto: state.renderDto });
    assert(state.browserEnv.webglRenderer, "WebGL renderer identity unavailable", state.browserEnv);
    const statePath = writeJson(`${name}-state.json`, state); const envPath = writeJson(`${name}-browser_env.json`, state.browserEnv);
    const stateDigest = createHash("sha256").update(JSON.stringify(state)).digest("hex");
    const canvasViewport = await evalJson(bringCanvasIntoViewScript());
    assert(canvasViewport.canvas.top >= 0 && canvasViewport.canvas.bottom <= canvasViewport.viewport.height, "canvas is not fully visible before pointer dispatch", canvasViewport);
    const initialTargets = await evalJson(targetsScript());
    const initialGlyphTargets = Object.fromEntries(initialTargets.filter((target) => ["recent:resource-transfer-fixture", "recent:build-queue-fixture"].includes(target.id)).map((target) => [target.id, target]));
    assert(initialGlyphTargets["recent:resource-transfer-fixture"] && initialGlyphTargets["recent:build-queue-fixture"], "real WASM hit-target readback omits one glyph", { targets: initialTargets });
    const viewportCorrection = await evalJson(panHotspotsIntoSafeBandScript());
    const targets = viewportCorrection.after;
    const glyphTargets = Object.fromEntries(targets.filter((target) => ["recent:resource-transfer-fixture", "recent:build-queue-fixture"].includes(target.id)).map((target) => [target.id, target]));
    assert(glyphTargets["recent:resource-transfer-fixture"] && glyphTargets["recent:build-queue-fixture"], "viewport correction dropped one live glyph target", viewportCorrection);
    const decisionClearance = await evalJson(`(() => { const panel=document.querySelector('.pixel-world-decision-area'); const r=panel.getBoundingClientRect(); return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,height:r.height,scrollHeight:panel.scrollHeight,overflowY:getComputedStyle(panel).overflowY}; })()`);
    for (const target of Object.values(glyphTargets)) {
      const x=canvasViewport.canvas.left + (Number(target.canvas_x) / canvasViewport.canvas.scaleX), y=canvasViewport.canvas.top + (Number(target.canvas_y) / canvasViewport.canvas.scaleY);
      assert(x+22 <= decisionClearance.left || x-22 >= decisionClearance.right || y+22+7 <= decisionClearance.top, 'decision panel obscures event glyph', {target,decisionClearance});
    }
    assert(decisionClearance.height >= 160 && decisionClearance.overflowY === 'auto', 'decision controls lost scroll access', decisionClearance);
    writeJson(`${name}-decision-clearance.json`,decisionClearance);
    const viewportCorrectionPath = writeJson(`${name}-viewport-correction.json`, { initialTargets, viewportCorrection, decisionClearance });
    const unhoveredPng = join(outDir, `${name}-unhovered-full.png`); await runBrowser(["screenshot", "--full", unhoveredPng]);
    // The headed full-page screenshot is in the browser's CSS-pixel layout
    // even though the WebGL canvas backing store is DPR-scaled. Keep crops in
    // the same CSS projection used by the visible panel and tooltip checks.
    const toFullPageCenter = (target) => ({ x: canvasViewport.canvas.left + (Number(target.canvas_x) / canvasViewport.canvas.scaleX), y: canvasViewport.scrollY + canvasViewport.canvas.top + (Number(target.canvas_y) / canvasViewport.canvas.scaleY) });
    const transferCrop = cropGlyph(unhoveredPng, join(outDir, `${name}-resource-transfer-glyph.png`), toFullPageCenter(glyphTargets["recent:resource-transfer-fixture"]));
    const buildQueueCrop = cropGlyph(unhoveredPng, join(outDir, `${name}-build-queue-glyph.png`), toFullPageCenter(glyphTargets["recent:build-queue-fixture"]));
    const transfer = await evalJson(receiptScript("hover", "recent:resource-transfer-fixture"));
    assert(transfer.receipt.dispatched && transfer.receipt.visible && transfer.tooltip?.visible && transfer.tooltip.text.includes("Resource transfer completed"), "resource transfer pointermove did not produce its authoritative tooltip", transfer);
    assert(transfer.tooltip.rect.left >= 0 && transfer.tooltip.rect.top >= 0 && transfer.tooltip.rect.right <= transfer.viewport.width && transfer.tooltip.rect.bottom <= transfer.viewport.height, "resource transfer tooltip is not fully inside the screenshot viewport", transfer);
    const transferPng = join(outDir, `${name}-resource-transfer-full.png`); await runBrowser(["screenshot", "--full", transferPng]); const transferStats = screenshotStats(transferPng); assert(transferStats.nonBlackRatio > 0.01 && transferStats.meanBrightness > 3, "resource transfer screenshot is black", transferStats);
    const buildQueue = await evalJson(receiptScript("hover", "recent:build-queue-fixture"));
    assert(buildQueue.receipt.dispatched && buildQueue.receipt.visible && buildQueue.tooltip?.visible && buildQueue.tooltip.text.includes("Build queue updated"), "build queue pointermove did not produce its authoritative tooltip", buildQueue);
    const buildQueuePng = join(outDir, `${name}-build-queue-full.png`); await runBrowser(["screenshot", "--full", buildQueuePng]); const buildQueueStats = screenshotStats(buildQueuePng); assert(buildQueueStats.nonBlackRatio > 0.01 && buildQueueStats.meanBrightness > 3, "build queue screenshot is black", buildQueueStats);
    const cleared = await evalJson(receiptScript("leave"));
    assert(cleared.receipt.dispatched && cleared.receipt.cleared && cleared.tooltip === null, "canvas pointerleave did not clear tooltip", cleared);
    const clearedPng = join(outDir, `${name}-cleared-full.png`); await runBrowser(["screenshot", "--full", clearedPng]); const clearedStats = screenshotStats(clearedPng); assert(clearedStats.nonBlackRatio > 0.01 && clearedStats.meanBrightness > 3, "cleared screenshot is black", clearedStats);
    const screenshotDiff = screenshotDifference(transferPng, clearedPng); assert(screenshotDiff.changedPixelRatio > 0.0001 && screenshotDiff.meanChannelDelta > 0.01, "visible and cleared screenshots lack tooltip pixel difference", screenshotDiff);
    const consoleOutput = await runBrowser(["console"]); const consolePath = join(outDir, `${name}-console.log`); writeFileSync(consolePath, consoleOutput);
    assert(!/\b(?:fatal|CONTEXT_LOST_WEBGL|webgl.*error)\b/i.test(consoleOutput), "browser console reports WebGL fatal", { consolePath, consoleOutput });
    const pointerPath = writeJson(`${name}-pointer-receipt.json`, { resourceTransfer: transfer.receipt, buildQueue: buildQueue.receipt, cleared: cleared.receipt });
    summary.viewports[name] = { width, height, statePath, envPath, stateDigest, canvasViewport, viewportCorrectionPath, pointerPath, consolePath, unhoveredPng, transferPng, buildQueuePng, clearedPng, transferCrop, buildQueueCrop, transferStats, buildQueueStats, clearedStats, screenshotDiff };
    // The viewport correction intentionally moves the world for visible
    // hotspot evidence. Reload the fixture before the independent selection
    // projection check so its agent target starts from the normal camera fit.
    prepareBrowserSession(); await new Promise((resolve) => setTimeout(resolve, 500));
    await browserJson(['open', url], { timeout: 45_000 });
    await browserJson(['set', 'viewport', String(width), String(height)]);
    await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; while(Date.now()<deadline){const s=${pageStateScript()}; if(s.rendererReady && s.runtimeStatus==='ready') return true; await new Promise(r=>setTimeout(r,100));} throw new Error('renderer not ready after viewport evidence reset');})()`);
    await evalJson(bringCanvasIntoViewScript());
    const initialProjection = await evalJson(selectionGeometryScript());
    await evalJson(`(() => { document.querySelector('#pixel-world-embedded-runtime-canvas').dispatchEvent(new WheelEvent('wheel', {deltaY:-100,bubbles:true,cancelable:true})); return true; })()`);
    const zoomProjection = await evalJson(selectionGeometryScript());
    await runBrowser(['mouse','move',String(width > 640 ? 850 : 100),String(width === 320 ? 240 : 400)]);
    await runBrowser(['mouse','down']);
    await runBrowser(['mouse','move',String(width > 640 ? 890 : 140),String(width === 320 ? 260 : 420)]);
    await runBrowser(['mouse','up']);
    const panProjection = await evalJson(selectionGeometryScript());
    for (const projection of [initialProjection,zoomProjection,panProjection]) assert(projection.count === 1 && projection.width >= 44 && projection.height >= 44 && projection.error < 1.1, 'renderer selection projection diverged', projection);
    assert(zoomProjection.camera.zoom !== initialProjection.camera.zoom, 'wheel did not change renderer camera', {initialProjection,zoomProjection});
    assert(panProjection.camera.pan_x_px !== zoomProjection.camera.pan_x_px, 'drag did not change renderer camera', {zoomProjection,panProjection});
    const projectionPath = writeJson(`${name}-selection-projection.json`, {initialProjection,zoomProjection,panProjection});
    summary.viewports[name].projectionPath = projectionPath;
    if (routeMotionEvidence) {
      // Reload before the animation comparison so pan/hover do not contaminate
      // the frame comparison. Media emulation is observed from matchMedia.
      await browserJson(['set','media','dark']);
      await browserJson(['open',url]);
      await evalJson(`new Promise(resolve => setTimeout(() => resolve(true),600))`);
      await evalJson(`(async()=>{for(let n=0;n<${name === 'compact' && routeMotionEvidence ? 3 : 1};n++){document.querySelector('#pixel-world-embedded-runtime-canvas').dispatchEvent(new WheelEvent('wheel',{deltaY:300,bubbles:true,cancelable:true}));await new Promise(r=>setTimeout(r,80));}await new Promise(r=>setTimeout(r,250));return true;})()`);
      const motionViewportCorrection = await evalJson(panHotspotsIntoSafeBandScript());
      const mediaBefore = await evalJson(`matchMedia('(prefers-reduced-motion: reduce)').matches`);
      assert(mediaBefore === false,'normal-motion media preference not observed',mediaBefore);
      const motionTargets = await evalJson(targetsScript());
      const motionCanvas = await evalJson(bringCanvasIntoViewScript());
      const frameFiles = {};
      for (const mode of ['normal','reduced']) {
        if (mode === 'reduced') await browserJson(['set','media','dark','reduced-motion']);
        await evalJson(`new Promise(resolve => setTimeout(() => resolve(true),300))`);
        for (const frame of [0,1]) {
          const path=join(outDir,`${name}-${mode}-frame-${frame}.png`);
          await runBrowser(['screenshot','--full',path]);
          frameFiles[`${mode}${frame}`]=path;
          await evalJson(`new Promise(resolve => setTimeout(() => resolve(true),350))`);
        }
      }
      const mediaAfter=await evalJson(`matchMedia('(prefers-reduced-motion: reduce)').matches`);
      assert(mediaAfter === true,'reduced-motion media preference not observed',mediaAfter);
      const reducedInput=await evalJson(receiptScript('hover','recent:resource-transfer-fixture'));
      assert(reducedInput.receipt.visible,'reduced preference froze input',reducedInput);
      await evalJson(receiptScript('leave'));
      const reducedSnapshot=await evalJson(`(async()=>{const before=window.__OASIS7_PIXEL_WORLD_RENDER_DTO__().world_tick;const snapshot=window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("routes_and_events")();snapshot.time=13;window.__AW_TEST__.injectSnapshot(snapshot);await new Promise(r=>setTimeout(r,200));return {before,after:window.__OASIS7_PIXEL_WORLD_RENDER_DTO__().world_tick,reduced:matchMedia('(prefers-reduced-motion: reduce)').matches};})()`);
      assert(reducedSnapshot.reduced && reducedSnapshot.after===13,'reduced preference froze snapshot updates',reducedSnapshot);
      await browserJson(['set','media','dark']);
      const mediaRestored=await evalJson(`matchMedia('(prefers-reduced-motion: reduce)').matches`);
      assert(mediaRestored === false,'normal preference did not restore live',mediaRestored);
      for (const frame of [0,1,2]) {
        const path=join(outDir,`${name}-restored-frame-${frame}.png`);
        await runBrowser(['screenshot','--full',path]);
        frameFiles[`restored${frame}`]=path;
        await evalJson(`new Promise(resolve=>setTimeout(()=>resolve(true),350))`);
      }
      const glyphDiffs={};
      for (const id of ['recent:resource-transfer-fixture','recent:build-queue-fixture']) {
        const hit=motionTargets.find(hit=>hit.id===id);
        const center={x:motionCanvas.canvas.left+(hit.canvas_x/motionCanvas.canvas.scaleX),y:motionCanvas.scrollY+motionCanvas.canvas.top+(hit.canvas_y/motionCanvas.canvas.scaleY)};
        const files={};
        for (const [key,path] of Object.entries(frameFiles)) {
          files[key]=join(outDir,`${name}-${key}-${id.split(':')[1]}.png`);
          cropGlyph(path,files[key],center);
        }
        glyphDiffs[id]={normal:screenshotDifference(files.normal0,files.normal1),reduced:screenshotDifference(files.reduced0,files.reduced1),restored:[screenshotDifference(files.restored0,files.restored1),screenshotDifference(files.restored0,files.restored2)].sort((a,b)=>b.changedPixelRatio-a.changedPixelRatio)[0]};
        assert(glyphDiffs[id].reduced.changedPixelRatio === 0,'reduced-motion event pixels continue animating',glyphDiffs[id]);
        assert(glyphDiffs[id].restored.changedPixelRatio > 0,'normal event motion did not resume',glyphDiffs[id]);
      }
      const motionPath=writeJson(`${name}-motion-evidence.json`,{mediaBefore,mediaAfter,mediaRestored,reducedInput,reducedSnapshot,frameFiles,glyphDiffs,motionViewportCorrection,routeCount:state.renderDto.links.length,links:state.renderDto.links});
      summary.viewports[name].motionPath=motionPath;
      await browserJson(['set','media','dark']);
    }
    if (completionRun) summary.viewports[name].completion=await completionEvidence({name,url:url.replace("routes_and_events","recent_event_glyphs"),outDir,evalJson,browserJson,runBrowser,writeJson,assert});
  }
  summary.status = "passed"; summary.completedAt = new Date().toISOString(); writeJson("summary.json", summary); console.log(`pixel-world hotspot visual smoke passed: ${join(outDir, "summary.json")}`);
} catch (error) { summary.status = "failed"; summary.completedAt = new Date().toISOString(); summary.failure = { message: error instanceof Error ? error.message : String(error) }; writeJson("summary.json", summary); console.error(`pixel-world hotspot visual smoke failed: ${join(outDir, "summary.json")}`); throw error; }
finally { closeBrowser(); await new Promise((resolveClose) => server.close(resolveClose)); }
