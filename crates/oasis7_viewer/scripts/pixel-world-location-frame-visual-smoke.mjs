import { serveStaticFile } from "./viewer-static-files.mjs";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";
import {
  locationFrameCropGeometry,
  measureLocationCornerFrame,
  readScreenshotBitmap,
} from "./pixel-world-location-frame-pixels.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const browserLaunchArgs = process.env.AGENT_BROWSER_HEADED === "1" ? ["--headed"] : [];
const viewerRoot = resolve(scriptDir, "..");
const artifactRoot = resolve(viewerRoot, ".viewer-test-dist");
const repoRoot = resolve(viewerRoot, "../..");
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = resolve(repoRoot, "output/playwright/pixel-world-location-frame-visual", runId);
const agentBrowserBin = process.env.AGENT_BROWSER_BIN || "agent-browser";
const session = `pixel-world-location-frame-visual-${process.pid}`;
const summary = { status: "running", fixtureName: "recent_event_glyphs", startedAt: new Date().toISOString(), viewports: {} };
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
    child.on("error", (error) => { clearTimeout(timer); rejectRun(error); }); child.on("close", (code) => { clearTimeout(timer); code === 0 ? resolveRun(stdout) : rejectRun(new Error(`agent-browser failed: ${args.join(" ")}\n${stdout}\n${stderr}`)); }); if (options.input !== undefined) child.stdin.end(options.input);
  });
}
async function browserJson(args, options) { const output = await runBrowser(["--json", ...args], options); const parsed = JSON.parse(output); if (!parsed.success) fail(parsed.error || "agent-browser JSON failure"); return parsed.data; }
async function evalJson(source) { const data = await browserJson(["eval", source], { timeout: 60_000 }); return typeof data.result === "string" ? JSON.parse(data.result) : data.result; }
function screenshotBitmap(path) { const bmpPath = path.replace(/\.png$/, ".bmp"); const converted = spawnSync("sips", ["-s", "format", "bmp", path, "--out", bmpPath], { encoding: "utf8" }); if (converted.status !== 0) fail("could not inspect screenshot pixels", { path, stderr: converted.stderr }); return readScreenshotBitmap(readFileSync(bmpPath)); }
function screenshotStats(source) { const bitmap = typeof source === "string" ? screenshotBitmap(source) : source; let brightness = 0; let nonBlack = 0; const count = bitmap.width * bitmap.height; for (let y = 0; y < bitmap.height; y += 1) for (let x = 0; x < bitmap.width; x += 1) { const value = bitmap.pixelAt(x, y).reduce((sum, channel) => sum + channel, 0); brightness += value; if (value > 24) nonBlack += 1; } return { meanBrightness: Number((brightness / Math.max(1, count)).toFixed(2)), nonBlackRatio: Number((nonBlack / Math.max(1, count)).toFixed(4)) }; }
function cropTarget(sourcePath, outputPath, crop) { const cropped = spawnSync("sips", ["--cropToHeightWidth", String(crop.height), String(crop.width), "--cropOffset", String(crop.top), String(crop.left), sourcePath, "--out", outputPath], { encoding: "utf8" }); if (cropped.status !== 0) fail("could not crop location-frame evidence", { sourcePath, outputPath, crop, stderr: cropped.stderr }); return outputPath; }
function stateScript() { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); const state = window.__AW_TEST__?.getState?.() || {}; const gl = canvas?.getContext('webgl2') || canvas?.getContext('webgl'); const debug = gl?.getExtension('WEBGL_debug_renderer_info'); const renderDto = window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.() || null; return { rendererReady: document.querySelector('.pixel-world-canvas')?.dataset.rendererReady === 'true', runtimeStatus: state.pixelWorldRuntimeStatus, runtimeSource: state.pixelWorldRuntimeSource, camera: state.pixelWorldCamera || null, fatal: state.pixelWorldFatal || state.lastError || null, fixture: document.querySelector('.pixel-world-host')?.dataset.visualFixture || null, locationLabels: (renderDto?.locations || []).map(({ id, label }) => ({ id, label })), renderDto, browserEnv: { userAgent: navigator.userAgent, devicePixelRatio: window.devicePixelRatio, webglRenderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : null, webglVendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : null } }; })()`; }
function scrollCanvasIntoViewScript() { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!canvas) throw new Error('pixel world canvas unavailable'); canvas.scrollIntoView({ block: 'center', inline: 'center' }); return JSON.stringify(true); })()`; }
function zoomCanvasScript(deltaY, repeats) { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!canvas) throw new Error('pixel world canvas unavailable'); const rect = canvas.getBoundingClientRect(); const clientX = rect.left + (rect.width / 2); const clientY = rect.top + (rect.height / 2); for (let index = 0; index < ${repeats}; index += 1) canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: ${deltaY}, clientX, clientY, bubbles: true, cancelable: true })); return JSON.stringify(true); })()`; }
function locationFrameScript() { return String.raw`(async () => { const probe = window.__OASIS7_PIXEL_WORLD_HOTSPOT_POINTER_PROBE__; const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!probe || !canvas) throw new Error('location target probe or canvas unavailable'); canvas.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'instant' }); await new Promise(resolve => requestAnimationFrame(resolve)); const read = () => probe.locationTargets().find((target) => target.id === 'loc-1') || null; const before = read(); if (!before) throw new Error('loc-1 target unavailable before pan'); const rect = canvas.getBoundingClientRect(); const desired = { x: rect.width / 2, y: rect.height / 2 }; const start = { x: rect.width / 2, y: rect.height / 2 }; const delta = { x: (desired.x - Number(before.canvas_x)) / 2, y: (desired.y - Number(before.canvas_y)) / 2 }; const pointerId = 917; const capture = canvas.setPointerCapture; const release = canvas.releasePointerCapture; canvas.setPointerCapture = () => {}; canvas.releasePointerCapture = () => {}; try { canvas.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientX: rect.left + start.x, clientY: rect.top + start.y, pointerId, buttons: 1 })); canvas.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: rect.left + start.x + delta.x, clientY: rect.top + start.y + delta.y, pointerId, buttons: 1 })); canvas.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, clientX: rect.left + start.x + delta.x, clientY: rect.top + start.y + delta.y, pointerId })); } finally { canvas.setPointerCapture = capture; canvas.releasePointerCapture = release; } let after = before; const deadline = Date.now() + 3000; while (Date.now() < deadline) { await new Promise(resolve => requestAnimationFrame(resolve)); after = read(); if (after && (Math.abs(Number(after.canvas_x) - Number(before.canvas_x)) > 1 || Math.abs(Number(after.canvas_y) - Number(before.canvas_y)) > 1)) break; } if (!after) throw new Error('loc-1 target unavailable after pan'); const safeMargin = Math.min(Number(after.canvas_x), Number(after.canvas_y), rect.width - Number(after.canvas_x), rect.height - Number(after.canvas_y)); return JSON.stringify({ scrollY: window.scrollY, viewport: { width: window.innerWidth, height: window.innerHeight }, canvas: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height }, before, after, drag: { start, delta }, safeMargin }); })()`; }
function locationCaptureScript() {
  return String.raw`(async () => {
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    const after = window.__OASIS7_PIXEL_WORLD_HOTSPOT_POINTER_PROBE__?.locationTargets()
      .find(target => target.id === 'loc-1');
    if (!canvas || !after) throw new Error('location target unavailable at capture');
    const rect = canvas.getBoundingClientRect();
    return JSON.stringify({
      viewport: { width: window.innerWidth, height: window.innerHeight },
      canvas: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
      after, devicePixelRatio: window.devicePixelRatio,
      reducedMotion: matchMedia('(prefers-reduced-motion: reduce)').matches,
    });
  })()`;
}

ensureBrowser();
const server = createServer(serveFile);
try {
  await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
  const address = server.address(); const url = `http://127.0.0.1:${address.port}/viewer.html?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=recent_event_glyphs`;
  summary.url = url; prepareBrowserSession(); await new Promise((resolve) => setTimeout(resolve, 500)); await browserJson(["open", url], { timeout: 45_000 });
  await evalJson(`(async()=>{for(let n=0;n<1200;n++){if(window.__OASIS7_VIEWER_FIXTURE_READY__) return true; await new Promise(r=>setTimeout(r,50));} throw new Error("test fixture never became ready");})()`);
  await browserJson(["set", "media", "dark", "reduced-motion"]);
  for (const [name, width, height] of [["desktop", 1440, 900], ["narrow", 390, 844]]) {
    await browserJson(["set", "viewport", String(width), String(height)]);
    const state = await evalJson(String.raw`(async()=>{const deadline=Date.now()+15000; while(Date.now()<deadline){const s=${stateScript()}; if(s.rendererReady && s.runtimeStatus==='ready') return JSON.stringify(s); await new Promise(r=>setTimeout(r,100));} throw new Error('renderer not ready');})()`);
    const location = state.renderDto?.locations?.find((entry) => entry.id === "loc-1");
    assert(state.fixture === "recent_event_glyphs" && state.rendererReady && state.runtimeStatus === "ready" && !state.fatal, "fixture renderer did not become ready", state);
    assert(state.locationLabels?.some(({ id, label }) => id === "loc-1" && typeof label === "string" && label.trim()), "fixture DTO has no location label for loc-1", state.locationLabels);
    assert(state.browserEnv.webglRenderer, "WebGL renderer identity unavailable", state.browserEnv);
    assert(location?.marker_role === "primary_marker", "loc-1 is not a primary marker", { location, selection: state.renderDto?.selection });
    assert(!(state.renderDto?.selection?.kind === "location" && state.renderDto.selection.id === "loc-1"), "loc-1 must remain unselected", { location, selection: state.renderDto?.selection });
    const statePath = writeJson(`${name}-state.json`, state); const envPath = writeJson(`${name}-browser_env.json`, state.browserEnv);
    await evalJson(scrollCanvasIntoViewScript());
    await evalJson(zoomCanvasScript(-120, 12));
    const zoomedState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=${stateScript()}; if(state.camera?.zoom >= 1.75) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify(state));})()`);
    assert(zoomedState.camera?.zoom >= 1.75, "camera did not reach label zoom threshold", zoomedState);
    const zoomedStatePath = writeJson(`${name}-zoomed-state.json`, zoomedState);
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const frame = await evalJson(locationFrameScript());
    assert(frame.canvas.top >= 0 && frame.canvas.bottom <= frame.viewport.height, "canvas is not fully visible before screenshot", frame);
    assert(frame.safeMargin >= 16, "loc-1 target is inside the 16px safe margin", frame);
    assert(frame.after.canvas_x >= 32 && frame.after.canvas_y >= 32 && frame.after.canvas_x <= frame.canvas.width - 32 && frame.after.canvas_y <= frame.canvas.height - 32, "loc-1 64px crop would leave canvas", frame);
    const fullPng = join(outDir, `${name}-loc-1-full.png`); await runBrowser(["screenshot", "--full", fullPng]); const stats = screenshotStats(fullPng);
    assert(stats.nonBlackRatio > 0.01 && stats.meanBrightness > 3, "location-frame screenshot is black", stats);
    await evalJson(scrollCanvasIntoViewScript());
    const captureFrame = await evalJson(locationCaptureScript());
    assert(captureFrame.reducedMotion, "location-frame capture must freeze animation", captureFrame);
    const viewportPng = join(outDir, `${name}-loc-1-viewport.png`); await runBrowser(["screenshot", viewportPng]);
    const crop = locationFrameCropGeometry(screenshotBitmap(viewportPng), captureFrame);
    const cropPng = cropTarget(viewportPng, join(outDir, `${name}-loc-1-crop-64.png`), crop);
    const cropBitmap = screenshotBitmap(cropPng);
    assert(cropBitmap.width === crop.width && cropBitmap.height === crop.height, "location-frame crop dimensions changed", crop);
    const cropStats = screenshotStats(cropBitmap); assert(cropStats.nonBlackRatio > 0.01 && cropStats.meanBrightness > 3, "location-frame crop is black", cropStats);
    // Reduced motion fixes animation_ms at zero. The loc-1 fixture retains its
    // deterministic id phase (5), so the expectation is independent of ECS readback.
    const bodySizeCss = Number(location.size_hint_px) * (1 + 0.08 * Math.sin(5));
    const cornerFrame = measureLocationCornerFrame(cropBitmap, crop, bodySizeCss);
    const cornerFramePath = writeJson(`${name}-loc-1-corner-frame.json`, { captureFrame, crop, ...cornerFrame });
    assert(cornerFrame.passed, "location-frame crop must show both mint L corners and leave the other corners open", { cropPng, cornerFramePath, ...cornerFrame });
    const consoleOutput = await runBrowser(["console"]); const consolePath = join(outDir, `${name}-console.log`); writeFileSync(consolePath, consoleOutput);
    assert(!/\b(?:fatal|CONTEXT_LOST_WEBGL|webgl.*error)\b/i.test(consoleOutput), "browser console reports WebGL fatal", { consolePath, consoleOutput });
    const framePath = writeJson(`${name}-loc-1-frame.json`, frame);
    summary.viewports[name] = { width, height, statePath, zoomedStatePath, envPath, framePath, consolePath, fullPng, viewportPng, cropPng, cornerFramePath, captureFrame, crop, cornerFrame, safeMargin: frame.safeMargin, stats, cropStats };
  }
  summary.status = "passed"; summary.completedAt = new Date().toISOString(); writeJson("summary.json", summary); console.log(`pixel-world location-frame visual smoke passed: ${join(outDir, "summary.json")}`);
} catch (error) {
  summary.status = "failed"; summary.completedAt = new Date().toISOString(); summary.failure = { message: error instanceof Error ? error.message : String(error) }; writeJson("summary.json", summary); console.error(`pixel-world location-frame visual smoke failed: ${join(outDir, "summary.json")}`); throw error;
} finally { closeBrowser(); await new Promise((resolveClose) => server.close(resolveClose)); }
