import { serveStaticFile } from "./viewer-static-files.mjs";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const browserLaunchArgs = process.env.AGENT_BROWSER_HEADED === "1" ? ["--headed"] : [];
const viewerRoot = resolve(scriptDir, "..");
const artifactRoot = resolve(viewerRoot, ".viewer-test-dist");
const repoRoot = resolve(viewerRoot, "../..");
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const outDir = resolve(repoRoot, "output/playwright/pixel-world-module-visual", runId);
const session = `pixel-world-module-visual-${process.pid}`;
const browser = process.env.AGENT_BROWSER_BIN || "agent-browser";
const summary = { status: "running", startedAt: new Date().toISOString(), viewports: {} };
mkdirSync(outDir, { recursive: true });

function fail(message, details) { throw new Error(`${message}${details ? `\n${JSON.stringify(details, null, 2)}` : ""}`); }
function assert(condition, message, details) { if (!condition) fail(message, details); }
function writeJson(name, value) { const path = join(outDir, name); writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`); return path; }
function screenshotBitmap(path) {
  const bmpPath = path.replace(/\.png$/, ".bmp");
  const converted = spawnSync("sips", ["-s", "format", "bmp", path, "--out", bmpPath], { encoding: "utf8" });
  if (converted.status !== 0) fail("could not inspect metadata screenshot pixels", { path, stderr: converted.stderr });
  const bytes = readFileSync(bmpPath);
  const offset = bytes.readUInt32LE(10);
  const width = bytes.readInt32LE(18);
  const height = Math.abs(bytes.readInt32LE(22));
  const bitCount = bytes.readUInt16LE(28);
  return { bytes, offset, width, height, bitCount, stride: Math.ceil((width * bitCount) / 32) * 4 };
}
function screenshotDifference(leftPath, rightPath) {
  const left = screenshotBitmap(leftPath);
  const right = screenshotBitmap(rightPath);
  assert(left.width === right.width && left.height === right.height && left.bitCount === right.bitCount, "metadata screenshot formats differ", { left, right });
  let changedPixelCount = 0;
  let totalDelta = 0;
  const total = left.width * left.height;
  for (let y = 0; y < left.height; y += 1) {
    for (let x = 0; x < left.width; x += 1) {
      const leftOffset = left.offset + (y * left.stride) + (x * (left.bitCount / 8));
      const rightOffset = right.offset + (y * right.stride) + (x * (right.bitCount / 8));
      let pixelDelta = 0;
      for (let channel = 0; channel < 3; channel += 1) {
        pixelDelta += Math.abs(left.bytes[leftOffset + channel] - right.bytes[rightOffset + channel]);
      }
      totalDelta += pixelDelta;
      if (pixelDelta > 18) changedPixelCount += 1;
    }
  }
  return {
    changedPixelCount,
    changedPixelRatio: Number((changedPixelCount / Math.max(1, total)).toFixed(6)),
    meanChannelDelta: Number((totalDelta / Math.max(1, total * 3)).toFixed(4)),
  };
}
function contentType(pathname) { return ({ ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".wasm": "application/wasm", ".css": "text/css; charset=utf-8" })[extname(pathname)] || "application/octet-stream"; }
function serveFile(request, response) {
  serveStaticFile(request, response, artifactRoot, contentType);
}
const browserLifecycle = createOwnedSessionLifecycle({ command: browser, session, prefixArgs: browserLaunchArgs });
const closeBrowser = browserLifecycle.close;
const prepareBrowserSession = browserLifecycle.prepare;
function runBrowser(args, input) { return new Promise((resolveRun, rejectRun) => { const child = spawn(browser, ["--session", session, ...browserLaunchArgs, ...args], { stdio: ["pipe", "pipe", "pipe"] }); let stdout = ""; let stderr = ""; child.stdout.setEncoding("utf8"); child.stderr.setEncoding("utf8"); child.stdout.on("data", (chunk) => { stdout += chunk; }); child.stderr.on("data", (chunk) => { stderr += chunk; }); child.on("error", rejectRun); child.on("close", (code) => code === 0 ? resolveRun(stdout) : rejectRun(new Error(`${args.join(" ")} failed\n${stdout}\n${stderr}`))); if (input !== undefined) child.stdin.end(input); }); }
async function browserJson(args, input) { const result = JSON.parse(await runBrowser(["--json", ...args], input)); if (!result.success) fail(result.error || "browser JSON failure"); return result.data; }
async function evalJson(script) { const data = await browserJson(["eval", script]); return typeof data.result === "string" ? JSON.parse(data.result) : data.result; }
async function clickVisible(selector) {
  await browserJson(["scrollintoview", selector]);
  const target = await evalJson(`(() => { const node = document.querySelector(${JSON.stringify(selector)}); if (!node) throw new Error("missing visible target"); const rect = node.getBoundingClientRect(); const x = rect.left + rect.width / 2; const y = rect.top + rect.height / 2; const hit = document.elementFromPoint(x, y); const style = getComputedStyle(node); return JSON.stringify({ visible: rect.width > 0 && rect.height > 0 && rect.top >= 0 && rect.bottom <= innerHeight && rect.left >= 0 && rect.right <= innerWidth && style.display !== "none" && style.visibility !== "hidden", targetTop: hit === node || node.contains(hit), rect: { x: rect.x, y: rect.y, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height }, hash: location.hash, active: document.activeElement?.id || null, hit: hit?.outerHTML?.slice(0, 200) || null }); })()`);
  assert(target.visible && target.targetTop, `target is not the visible top hit: ${selector}`, target);
  const x = Math.round((target.rect.x + target.rect.right) / 2);
  const y = Math.round((target.rect.y + target.rect.bottom) / 2);
  await browserJson(["mouse", "move", String(x), String(y)]);
  await browserJson(["mouse", "down"]);
  await browserJson(["mouse", "up"]);
  return { selector, target };
}
function pageStateScript() { return String.raw`(() => { const state = window.__AW_TEST__?.getState?.() || {}; const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); const gl = canvas?.getContext('webgl2'); const debug = gl?.getExtension('WEBGL_debug_renderer_info'); const dto = window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.() || null; const moduleMarkers = [...document.querySelectorAll('[data-pixel-world-module-marker="true"]')].map((marker) => ({ id: marker.dataset.moduleId || null, kind: marker.dataset.moduleKind || null, selected: marker.dataset.selected === 'true', rendererTarget: marker.dataset.rendererTarget === 'true', ariaLabel: marker.getAttribute('aria-label') })); const eventLocators = [...document.querySelectorAll('[data-world-feed-module-locate]')].map((button) => ({ id: button.dataset.worldFeedModuleLocate || null, label: button.textContent.trim(), ariaLabel: button.getAttribute('aria-label'), tagName: button.tagName, tabIndex: button.tabIndex })); const detailsPanel = document.querySelector('#viewer-details-panel'); const detailsStyle = detailsPanel ? getComputedStyle(detailsPanel) : null; const detailsRect = detailsPanel?.getBoundingClientRect(); const detailsVisible = Boolean(detailsPanel && !detailsPanel.hidden && detailsPanel.getAttribute('aria-hidden') !== 'true' && detailsPanel.getAttribute('inert') === null && detailsStyle?.display !== 'none' && detailsStyle?.visibility !== 'hidden' && Number(detailsStyle?.opacity || '1') > 0 && detailsRect?.width > 0 && detailsRect?.height > 0 && detailsRect.right > 0 && detailsRect.bottom > 0 && detailsRect.left < innerWidth && detailsRect.top < innerHeight); const detailsActive = Boolean(detailsPanel && detailsPanel.getAttribute('data-viewer-route-panel') === 'command' && detailsPanel.getAttribute('data-viewer-surface') === 'command' && (window.location.hash === '#viewer-details-panel' || document.activeElement === detailsPanel)); const moduleDetails = detailsPanel?.querySelector('[data-viewer-module-details="true"]'); const moduleDetailsStyle = moduleDetails ? getComputedStyle(moduleDetails) : null; const moduleDetailsRect = moduleDetails?.getBoundingClientRect(); const moduleDetailsVisible = Boolean(moduleDetails && !moduleDetails.hidden && moduleDetailsStyle?.display !== 'none' && moduleDetailsStyle?.visibility !== 'hidden' && Number(moduleDetailsStyle?.opacity || '1') > 0 && moduleDetailsRect?.width > 0 && moduleDetailsRect?.height > 0 && moduleDetailsRect.right > 0 && moduleDetailsRect.bottom > 0 && moduleDetailsRect.left < innerWidth && moduleDetailsRect.top < innerHeight); const detailsText = detailsPanel?.textContent?.trim() || ''; const rect = canvas?.getBoundingClientRect(); return JSON.stringify({ renderMode: state.renderMode, runtimeStatus: state.pixelWorldRuntimeStatus, runtimeSource: state.pixelWorldRuntimeSource, runtimeModuleUrl: state.pixelWorldRuntimeModuleUrl || null, camera: state.pixelWorldCamera || null, selectedKind: state.selectedKind || null, selectedId: state.selectedId || null, fatal: state.pixelWorldFatal || state.lastError || null, canvas: Boolean(canvas), canvasRect: rect ? { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height } : null, webgl2: Boolean(gl), browserEnv: { userAgent: navigator.userAgent, devicePixelRatio: window.devicePixelRatio, webglRenderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : null, webglVendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : null }, fixture: document.querySelector('.pixel-world-host')?.dataset.visualFixture || null, renderLocale: dto?.locale || null, modules: (dto?.module_visual_entities || []).map(({ id, kind, pos }) => ({ id, kind, pos })), moduleMetadata: (dto?.module_visual_entities || []).map(({ id, module_id, kind, label, anchor, pos }) => ({ id, module_id, kind, label, anchor, pos })), links: dto?.links || [], visualHotspots: dto?.visual_hotspots || [], moduleMarkers, eventLocators, detailsText, detailsVisible, detailsActive, detailsRect: detailsRect ? { left: detailsRect.left, top: detailsRect.top, right: detailsRect.right, bottom: detailsRect.bottom, width: detailsRect.width, height: detailsRect.height } : null, moduleDetailsVisible, moduleDetailsRect: moduleDetailsRect ? { left: moduleDetailsRect.left, top: moduleDetailsRect.top, right: moduleDetailsRect.right, bottom: moduleDetailsRect.bottom, width: moduleDetailsRect.width, height: moduleDetailsRect.height } : null, control: Boolean(window.__OASIS7_MODULE_VISUAL_FIXTURE_CONTROL__) }); })()`; }
function canvasDimensionsScript() { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!canvas) throw new Error('pixel world canvas unavailable'); const rect = canvas.getBoundingClientRect(); return JSON.stringify({ cssWidth: rect.width, cssHeight: rect.height, bitmapWidth: canvas.width, bitmapHeight: canvas.height }); })()`; }
function scrollCanvasIntoViewScript() { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!canvas) throw new Error('pixel world canvas unavailable'); canvas.scrollIntoView({ block: 'center', inline: 'center' }); return JSON.stringify(true); })()`; }
function zoomCanvasScript(deltaY, repeats) { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!canvas) throw new Error('pixel world canvas unavailable'); const rect = canvas.getBoundingClientRect(); const clientX = rect.left + (rect.width / 2); const clientY = rect.top + (rect.height / 2); for (let index = 0; index < ${repeats}; index += 1) canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: ${deltaY}, clientX, clientY, bubbles: true, cancelable: true })); return JSON.stringify(true); })()`; }
function panCanvasScript(direction = 1, ratio = 0.4, minimum = 230, vertical = 0) { return String.raw`(() => { const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas'); if (!canvas) throw new Error('pixel world canvas unavailable'); canvas.setPointerCapture = () => {}; canvas.releasePointerCapture = () => {}; const rect = canvas.getBoundingClientRect(); const pointerId = 77; const startX = rect.left + (rect.width / 2); const startY = rect.top + (rect.height / 2); const endX = startX + (${direction} * Math.max(rect.width * ${ratio}, ${minimum})); const endY = startY + ${vertical}; canvas.dispatchEvent(new PointerEvent('pointerdown', { pointerId, clientX: startX, clientY: startY, button: 0, buttons: 1, bubbles: true })); canvas.dispatchEvent(new PointerEvent('pointermove', { pointerId, clientX: endX, clientY: endY, buttons: 1, bubbles: true })); canvas.dispatchEvent(new PointerEvent('pointerup', { pointerId, clientX: endX, clientY: endY, button: 0, bubbles: true })); return JSON.stringify(true); })()`; }
function clickModuleScript(id) { return String.raw`(() => { const expectedId = ${JSON.stringify(id)}; const marker = [...document.querySelectorAll('[data-pixel-world-module-marker="true"]')].find((candidate) => candidate.dataset.moduleId === expectedId && candidate.dataset.rendererTarget === 'true'); if (!marker) throw new Error(JSON.stringify({ message: 'actual-WASM module target unavailable', expectedId, markers: [...document.querySelectorAll('[data-pixel-world-module-marker="true"]')].map((candidate) => ({ id: candidate.dataset.moduleId, rendererTarget: candidate.dataset.rendererTarget, ariaLabel: candidate.getAttribute('aria-label') })) })); const keyboardReachable = marker instanceof HTMLButtonElement && marker.tabIndex >= 0; if (!keyboardReachable) throw new Error(JSON.stringify({ message: 'module target is not keyboard reachable', id: expectedId, tagName: marker.tagName, tabIndex: marker.tabIndex })); marker.focus(); const focusedBeforeClick = document.activeElement === marker; marker.click(); return JSON.stringify({ id: expectedId, ariaLabel: marker.getAttribute('aria-label'), keyboardReachable, focused: focusedBeforeClick }); })()`; }
function metadataSnapshotScript(stage) {
  return String.raw`(() => {
    const fixtures = window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__;
    const inject = window.__AW_TEST__?.injectSnapshot;
    if (typeof fixtures?.get('module_visual_entities') !== 'function' || typeof inject !== 'function') {
      throw new Error('metadata fixture injection API unavailable');
    }
    const snapshot = structuredClone(fixtures.get("module_visual_entities")());
    const baseline = ${JSON.stringify(stage === 'baseline')};
    const metadataStage = ${JSON.stringify(stage === 'metadata')};
    const explicitRouteLabel = ${JSON.stringify(stage === 'baseline' || stage === 'metadata')};
    const zhFallback = ${JSON.stringify(stage === 'metadata' || stage === 'fallback-zh')};
    const relation = {
      kind: 'logistics_route',
      status: 'active',
      source_class: 'runtime_projection',
      freshness: 'current',
    };
    snapshot.locale = zhFallback ? 'zh' : 'en';
    const localeButton = document.querySelector('[data-locale="' + snapshot.locale + '"]');
    if (!localeButton) throw new Error('locale switch unavailable for ' + snapshot.locale);
    localeButton.click();
    snapshot.model.agents['agent-0'].relation = {
      ...relation,
      ...(explicitRouteLabel ? { label: baseline ? 'Initial logistics route' : 'Updated logistics route' } : {}),
    };
    snapshot.model.module_visual_entities['module-absolute'] = {
      entity_id: 'module-absolute',
      module_id: 'fixture-module',
      kind: baseline ? 'beacon' : 'relay',
      label: baseline ? 'Beacon marker' : 'Beacon marker updated',
      anchor: { type: 'absolute', data: { pos: { x_cm: 1850000, y_cm: 3600000, z_cm: 0 } } },
    };
    snapshot.model.module_visual_entities['module-relay'] = {
      entity_id: 'module-relay',
      module_id: 'fixture-module',
      kind: baseline ? 'relay' : 'beacon',
      label: baseline ? 'Relay marker' : 'Relay marker updated',
      anchor: { type: 'absolute', data: { pos: { x_cm: 1850000, y_cm: 3600000, z_cm: 0 } } },
    };
    snapshot.model.module_visual_entities['module-agent'] = {
      entity_id: 'module-agent',
      module_id: 'fixture-module',
      kind: baseline ? 'future_module_kind' : 'future_module_kind_updated',
      label: baseline ? 'Unknown marker' : 'Unknown marker updated',
      anchor: { type: 'agent', data: { agent_id: 'agent-0' } },
    };
    snapshot.player_gameplay.goal_title = baseline ? 'Initial objective' : '更新目标';
    snapshot.player_gameplay.blocker_kind = baseline ? 'material_shortage' : metadataStage ? 'power_shortage' : 'updated_blocker';
    snapshot.player_gameplay.blocker_label = baseline ? 'Initial blocker' : 'Updated blocker';
    inject(snapshot, { returnState: false });
    return JSON.stringify({ stage: ${JSON.stringify(stage)}, locale: snapshot.locale, explicitRouteLabel });
  })()`;
}
function injectModuleEventFeedScript() { return String.raw`(() => { const inject = window.__AW_TEST__?.injectWorldFeedForTest; if (typeof inject !== 'function') throw new Error('world-feed test injection API unavailable'); const state = inject({ schema_version: 'world_feed/v1', world_id: 'fixture-world', reorg_epoch: 0, cursor: '102', status: 'ready', snapshot_reload_required: false, events: [{ event_seq: 102, kind: 'module_visual_entity_updated', summary: 'Module Agent updated', detail: 'module event fixture', module_visual_entity_id: 'module-agent' }] }); return JSON.stringify({ status: state.status, stale: state.stale, events: state.events }); })()`; }
function clickModuleLocatorScript(id) { return String.raw`(() => { const expectedId = ${JSON.stringify(id)}; const locator = document.querySelector('[data-world-feed-module-locate="' + expectedId + '"]'); if (!locator) throw new Error(JSON.stringify({ message: 'module event locator unavailable', expectedId, locators: [...document.querySelectorAll('[data-world-feed-module-locate]')].map((candidate) => ({ id: candidate.dataset.worldFeedModuleLocate, label: candidate.textContent.trim() })) })); const keyboardReachable = locator instanceof HTMLButtonElement && locator.tabIndex >= 0; if (!keyboardReachable) throw new Error(JSON.stringify({ message: 'module event locator is not keyboard reachable', expectedId, tagName: locator.tagName, tabIndex: locator.tabIndex })); locator.focus(); const focusedBeforeClick = document.activeElement === locator; locator.click(); return JSON.stringify({ id: expectedId, ariaLabel: locator.getAttribute('aria-label'), keyboardReachable, focused: focusedBeforeClick }); })()`; }

function rendererTargetGeometryScript() {
  return String.raw`(() => {
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    if (!canvas) throw new Error('pixel world canvas unavailable');
    const canvasRect = canvas.getBoundingClientRect();
    const read = (selector, idKey) => [...document.querySelectorAll(selector)].map((node) => {
      const rect = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      return {
        id: node.dataset[idKey] || null,
        tagName: node.tagName,
        tabIndex: node.tabIndex,
        keyboardReachable: node instanceof HTMLButtonElement && node.tabIndex >= 0,
        visible: rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight && style.display !== 'none' && style.visibility !== 'hidden',
        rect: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height },
        center: { x: rect.left + (rect.width / 2) - canvasRect.left, y: rect.top + (rect.height / 2) - canvasRect.top },
        clientCenter: { x: rect.left + (rect.width / 2), y: rect.top + (rect.height / 2) },
      };
    });
    return JSON.stringify({
      canvas: {
        rect: { left: canvasRect.left, top: canvasRect.top, right: canvasRect.right, bottom: canvasRect.bottom, width: canvasRect.width, height: canvasRect.height },
        bitmap: { width: canvas.width, height: canvas.height },
        bitmapScale: { x: canvas.width / canvasRect.width, y: canvas.height / canvasRect.height },
      },
      agents: read('[data-pixel-world-agent-marker="true"][data-renderer-target="true"]', 'agentId'),
      locations: read('[data-pixel-world-location-marker="true"][data-renderer-target="true"]', 'locationId'),
      modules: read('[data-pixel-world-module-marker="true"][data-renderer-target="true"]', 'moduleId'),
    });
  })()`;
}

async function actualCanvasClick(kind, id, evidenceName) {
  const selector = kind === 'agent'
    ? '[data-pixel-world-agent-marker="true"][data-renderer-target="true"]'
    : kind === 'location'
      ? '[data-pixel-world-location-marker="true"][data-renderer-target="true"]'
      : '[data-pixel-world-module-marker="true"][data-renderer-target="true"]';
  const idKey = kind === 'agent' ? 'agentId' : kind === 'location' ? 'locationId' : 'moduleId';
  const target = await evalJson(String.raw`(() => {
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    const marker = [...document.querySelectorAll(${JSON.stringify(selector)})].find((candidate) => candidate.dataset[${JSON.stringify(idKey)}] === ${JSON.stringify(id)});
    if (!canvas || !marker) throw new Error(JSON.stringify({ message: 'renderer target unavailable for native canvas click', kind: ${JSON.stringify(kind)}, id: ${JSON.stringify(id)} }));
    const canvasRect = canvas.getBoundingClientRect();
    const markerRect = marker.getBoundingClientRect();
    const center = { x: markerRect.left + markerRect.width / 2 - canvasRect.left, y: markerRect.top + markerRect.height / 2 - canvasRect.top };
    const clientCenter = { x: markerRect.left + markerRect.width / 2, y: markerRect.top + markerRect.height / 2 };
    const style = getComputedStyle(marker);
    return JSON.stringify({
      id: ${JSON.stringify(id)}, kind: ${JSON.stringify(kind)},
      keyboardReachable: marker instanceof HTMLButtonElement && marker.tabIndex >= 0,
      visible: markerRect.width > 0 && markerRect.height > 0 && markerRect.left >= 0 && markerRect.right <= innerWidth && markerRect.top >= 0 && markerRect.bottom <= innerHeight && style.display !== 'none' && style.visibility !== 'hidden',
      markerRect: { left: markerRect.left, top: markerRect.top, right: markerRect.right, bottom: markerRect.bottom, width: markerRect.width, height: markerRect.height },
      canvasRect: { left: canvasRect.left, top: canvasRect.top, right: canvasRect.right, bottom: canvasRect.bottom, width: canvasRect.width, height: canvasRect.height },
      bitmap: { width: canvas.width, height: canvas.height },
      bitmapScale: { x: canvas.width / canvasRect.width, y: canvas.height / canvasRect.height },
      center, clientCenter,
    });
  })()`);
  assert(target.keyboardReachable && target.visible, 'renderer target is not visible and keyboard aligned before native canvas click', target);
  assert(target.center.x >= 0 && target.center.x <= target.canvasRect.width && target.center.y >= 0 && target.center.y <= target.canvasRect.height, 'renderer target center is outside the canvas CSS bounds', target);

  await evalJson(String.raw`(() => {
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    window.__OASIS7_NATIVE_CANVAS_POINTER_TRACE__ = [];
    if (!window.__OASIS7_NATIVE_CANVAS_POINTER_TRACE_BOUND__) {
      for (const type of ['pointermove', 'pointerdown', 'pointerup', 'click']) {
        canvas.addEventListener(type, (event) => window.__OASIS7_NATIVE_CANVAS_POINTER_TRACE__.push({
          type,
          clientX: event.clientX,
          clientY: event.clientY,
          pointerId: event.pointerId ?? null,
          buttons: event.buttons ?? null,
          target: event.target?.id || event.target?.tagName || null,
        }), true);
      }
      window.__OASIS7_NATIVE_CANVAS_POINTER_TRACE_BOUND__ = true;
    }
    return JSON.stringify(true);
  })()`);
  const canvasHit = await evalJson(String.raw`(() => {
    const canvas = document.querySelector('#pixel-world-embedded-runtime-canvas');
    const nodes = [...document.querySelectorAll('[data-renderer-target="true"]')];
    nodes.forEach((node) => { node.dataset.qaPreviousPointerEvents = node.style.pointerEvents; node.style.pointerEvents = 'none'; });
    const hit = document.elementFromPoint(${target.clientCenter.x}, ${target.clientCenter.y});
    return JSON.stringify({ isCanvas: hit === canvas, hit: hit?.outerHTML?.slice(0, 240) || null });
  })()`);
  assert(canvasHit.isCanvas, 'native click coordinate did not reach the Bevy canvas after DOM target suppression', { target, canvasHit });
  try {
    await browserJson(['mouse', 'move', String(Math.round(target.clientCenter.x)), String(Math.round(target.clientCenter.y))]);
    await browserJson(['mouse', 'down']);
    await browserJson(['mouse', 'up']);
  } finally {
    await evalJson(String.raw`(() => { for (const node of document.querySelectorAll('[data-renderer-target="true"]')) { node.style.pointerEvents = node.dataset.qaPreviousPointerEvents || ''; delete node.dataset.qaPreviousPointerEvents; } return JSON.stringify(true); })()`);
  }
  const pointerTrace = await evalJson(String.raw`JSON.stringify(window.__OASIS7_NATIVE_CANVAS_POINTER_TRACE__ || [])`);
  const receipt = { ...target, inputPath: 'agent-browser mouse -> actual Bevy canvas', canvasHit, pointerTrace };
  if (evidenceName) writeJson(`${evidenceName}-native-canvas-${kind}-${id}.json`, receipt);
  return receipt;
}

function waitForSelectionScript(kind, id) {
  return String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.selectedKind === ${JSON.stringify(kind)} && state.selectedId === ${JSON.stringify(id)}) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'native canvas click did not reach the expected selection', expected:{kind:${JSON.stringify(kind)},id:${JSON.stringify(id)}}, state}));})()`;
}

if (spawnSync(browser, ["--version"], { stdio: "ignore" }).status !== 0) fail(`missing ${browser}`);
const server = createServer(serveFile);
try {
  await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
  const address = server.address();
  const url = `http://127.0.0.1:${address.port}/viewer.html?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=module_visual_entities`;
  summary.url = url;
  prepareBrowserSession(); await new Promise((resolve) => setTimeout(resolve, 500));
  await browserJson(["open", url]);
  await evalJson(`(async()=>{for(let n=0;n<1200;n++){if(window.__OASIS7_VIEWER_FIXTURE_READY__) return true; await new Promise(r=>setTimeout(r,50));} throw new Error("test fixture never became ready");})()`);
  for (const [name, width, height] of [["desktop", 1440, 900], ["narrow", 390, 844]]) {
    if (name !== "desktop") await browserJson(["open", url]);
    await browserJson(["set", "viewport", String(width), String(height)]);
    const state = await evalJson(String.raw`(async()=>{const deadline=Date.now()+15000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.runtimeStatus === 'ready' && state.canvas) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,100));} throw new Error(JSON.stringify(state));})()`);
    assert(state.fixture === "module_visual_entities" && state.control && !state.fatal, "module fixture was not ready", state);
    assert(state.renderMode === "viewer", "module smoke did not use the viewer render mode", state);
    assert(state.runtimeSource === "wasm_bindgen_runtime" && /pixel_world_bridge\.js(?:$|[?#])/.test(String(state.runtimeModuleUrl || "")), "module smoke did not load the actual bindgen WASM bridge", state);
    assert(state.webgl2 && state.browserEnv?.webglRenderer, "WebGL2 renderer identity unavailable", state);
    const expectedInitialModules = [
      { id: "module-absolute", kind: "beacon", pos: { x_cm: 1_850_000, y_cm: 3_600_000, z_cm: 0 } },
      { id: "module-agent", kind: "future_module_kind", pos: { x_cm: 2_900_000, y_cm: 3_450_000, z_cm: 0 } },
      { id: "module-relay", kind: "relay", pos: { x_cm: 1_850_000, y_cm: 3_600_000, z_cm: 0 } },
    ];
    assert(JSON.stringify(state.modules) === JSON.stringify(expectedInitialModules), "module kind and anchor evidence is not present in stable order", { state, expectedInitialModules });
    assert(state.modules[0].pos.x_cm === state.modules[2].pos.x_cm && state.modules[0].pos.y_cm === state.modules[2].pos.y_cm, "beacon and relay must remain co-anchored for the renderer smoke", state.modules);
    const initialMetadata = Object.fromEntries(state.moduleMetadata.map((module) => [module.id, module]));
    assert(initialMetadata["module-absolute"]?.module_id === "fixture-module" && initialMetadata["module-absolute"]?.label === "Beacon marker" && initialMetadata["module-absolute"]?.anchor?.type === "absolute", "absolute module identity metadata is missing from the real Render DTO", initialMetadata["module-absolute"]);
    assert(initialMetadata["module-agent"]?.module_id === "fixture-module" && initialMetadata["module-agent"]?.label === "Unknown marker" && initialMetadata["module-agent"]?.anchor?.type === "agent", "agent-anchored module identity metadata is missing from the real Render DTO", initialMetadata["module-agent"]);
    assert(initialMetadata["module-relay"]?.module_id === "fixture-module" && initialMetadata["module-relay"]?.anchor?.type === "absolute", "co-anchored relay identity metadata is missing from the real Render DTO", initialMetadata["module-relay"]);

    await evalJson(metadataSnapshotScript("baseline"));
    const metadataBaselineState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.renderLocale === 'en' && state.links.some((link)=>link.label === 'Initial logistics route') && state.visualHotspots.some((hotspot)=>hotspot.label === 'Initial objective') && state.visualHotspots.some((hotspot)=>hotspot.label === 'Missing Material')) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'metadata baseline did not reach the real Render DTO',state}));})()`);
    await evalJson(String.raw`new Promise((resolve)=>setTimeout(resolve,250))`);
    const metadataBaselineStableState = await evalJson(String.raw`(async()=>{await new Promise((resolve)=>setTimeout(resolve,2000)); const deadline=Date.now()+10000; let previous=null; let stableSamples=0; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); const camera=JSON.stringify(state.camera); stableSamples=camera===previous?stableSamples+1:0; if(stableSamples>=2) return JSON.stringify(state); previous=camera; await new Promise((resolve)=>setTimeout(resolve,250));} throw new Error(JSON.stringify({message:'renderer camera did not settle before metadata baseline',state}));})()`);
    const metadataBaselineCanvasPng = join(outDir, `${name}-metadata-baseline-canvas.png`);
    await browserJson(["screenshot", "#pixel-world-embedded-runtime-canvas", metadataBaselineCanvasPng]);

    await evalJson(metadataSnapshotScript("metadata"));
    const metadataUpdatedState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.renderLocale === 'zh' && state.links.some((link)=>link.label === 'Updated logistics route') && state.visualHotspots.some((hotspot)=>hotspot.label === '更新目标') && state.visualHotspots.some((hotspot)=>hotspot.label === '缺电') && state.moduleMarkers.some((marker)=>marker.id === 'module-absolute' && marker.kind === 'relay')) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'same-geometry metadata update did not reach the real Render DTO and renderer targets',state}));})()`);
    await evalJson(String.raw`new Promise((resolve)=>setTimeout(resolve,250))`);
    const metadataUpdatedStableState = await evalJson(pageStateScript());
    assert(JSON.stringify(metadataUpdatedStableState.camera) === JSON.stringify(metadataBaselineStableState.camera), "same-geometry metadata update changed the renderer camera", { before: metadataBaselineStableState.camera, after: metadataUpdatedStableState.camera });
    assert(JSON.stringify(metadataUpdatedStableState.moduleMetadata.map(({ id, pos }) => ({ id, pos }))) === JSON.stringify(metadataBaselineStableState.moduleMetadata.map(({ id, pos }) => ({ id, pos }))), "module metadata update changed anchor geometry", { before: metadataBaselineStableState.moduleMetadata, after: metadataUpdatedStableState.moduleMetadata });
    assert(JSON.stringify(metadataUpdatedStableState.links.map(({ id, from, to }) => ({ id, from, to }))) === JSON.stringify(metadataBaselineStableState.links.map(({ id, from, to }) => ({ id, from, to }))), "route metadata update changed endpoint geometry", { before: metadataBaselineStableState.links, after: metadataUpdatedStableState.links });
    assert(JSON.stringify(metadataUpdatedStableState.visualHotspots.map(({ id, pos }) => ({ id, pos }))) === JSON.stringify(metadataBaselineStableState.visualHotspots.map(({ id, pos }) => ({ id, pos }))), "hotspot metadata update changed anchor geometry", { before: metadataBaselineStableState.visualHotspots, after: metadataUpdatedStableState.visualHotspots });
    assert(metadataUpdatedStableState.moduleMarkers.some((marker) => marker.id === "module-absolute" && marker.kind === "relay" && /Beacon marker updated/i.test(marker.ariaLabel || "")), "module identity metadata did not update in the renderer target", metadataUpdatedStableState.moduleMarkers);
    const metadataUpdatedCanvasPng = join(outDir, `${name}-metadata-updated-canvas.png`);
    await browserJson(["screenshot", "#pixel-world-embedded-runtime-canvas", metadataUpdatedCanvasPng]);
    const metadataCanvasDelta = screenshotDifference(metadataBaselineCanvasPng, metadataUpdatedCanvasPng);
    assert(metadataCanvasDelta.changedPixelCount >= 48, "same-geometry metadata update did not change rendered canvas pixels", metadataCanvasDelta);

    await evalJson(metadataSnapshotScript("fallback-zh"));
    const localeZhState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.renderLocale === 'zh' && state.links.some((link)=>link.kind === 'logistics_route' && !link.label)) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'locale fallback baseline did not reach the real Render DTO',state}));})()`);
    await evalJson(String.raw`new Promise((resolve)=>setTimeout(resolve,250))`);
    const localeZhStableState = await evalJson(pageStateScript());
    const localeZhCanvasPng = join(outDir, `${name}-metadata-locale-zh-canvas.png`);
    await browserJson(["screenshot", "#pixel-world-embedded-runtime-canvas", localeZhCanvasPng]);
    await evalJson(metadataSnapshotScript("fallback-en"));
    const localeEnState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.renderLocale === 'en' && state.links.some((link)=>link.kind === 'logistics_route' && !link.label)) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'locale fallback update did not reach the real Render DTO',state}));})()`);
    await evalJson(String.raw`new Promise((resolve)=>setTimeout(resolve,250))`);
    const localeEnStableState = await evalJson(pageStateScript());
    assert(JSON.stringify(localeEnStableState.camera) === JSON.stringify(localeZhStableState.camera), "locale-only fallback update changed the renderer camera", { before: localeZhStableState.camera, after: localeEnStableState.camera });
    const localeEnCanvasPng = join(outDir, `${name}-metadata-locale-en-canvas.png`);
    await browserJson(["screenshot", "#pixel-world-embedded-runtime-canvas", localeEnCanvasPng]);
    const localeCanvasDelta = screenshotDifference(localeZhCanvasPng, localeEnCanvasPng);
    assert(localeCanvasDelta.changedPixelCount >= 16, "locale-only fallback update did not change rendered canvas pixels", localeCanvasDelta);
    const metadataEvidence = {
      baseline: metadataBaselineStableState,
      updated: metadataUpdatedStableState,
      updatedCanvasDelta: metadataCanvasDelta,
      localeZh: localeZhStableState,
      localeEn: localeEnStableState,
      localeCanvasDelta,
      canvas: { baseline: metadataBaselineCanvasPng, updated: metadataUpdatedCanvasPng, localeZh: localeZhCanvasPng, localeEn: localeEnCanvasPng },
    };

    const targetById = (geometry, collection, id) => geometry[collection].find((target) => target.id === id);
    const targetSeparation = (geometry, parentCollection, parentId, moduleId, label) => {
      const parent = targetById(geometry, parentCollection, parentId);
      const module = targetById(geometry, "modules", moduleId);
      assert(parent && module, `${label} parent/module DOM targets are missing`, { geometry, parentCollection, parentId, moduleId });
      assert(parent.visible && module.visible && parent.keyboardReachable && module.keyboardReachable, `${label} parent/module DOM targets are not visible and keyboard reachable`, { parent, module });
      const delta = { x: Number((module.center.x - parent.center.x).toFixed(3)), y: Number((module.center.y - parent.center.y).toFixed(3)) };
      assert(Math.abs(delta.x) >= 44 || Math.abs(delta.y) >= 44, `${label} module target does not clear the 44px parent target`, { parent, module, delta });
      return { parent, module, delta, bitmapScale: geometry.canvas.bitmapScale };
    };
    const closeNativeDetails = async () => {
      const current = await evalJson(pageStateScript());
      if (!current.detailsActive) return current;
      await clickVisible("#viewer-details-panel .panel__route-close");
      return evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(!state.detailsActive && window.location.hash === '#viewer-stage-panel') return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'native canvas proof could not return to World after closing details',state}));})()`);
    };

    // The transparent DOM targets are evidence for keyboard/touch alignment. For
    // selection proof, suppress only those targets and use real browser mouse
    // input at their centers so the event reaches the actual Bevy canvas.
    const initialTargetGeometry = await evalJson(rendererTargetGeometryScript());
    const initialTargetGeometryPath = writeJson(`${name}-native-canvas-preflight.json`, initialTargetGeometry);
    const nativeTargetOverlayPng = join(outDir, `${name}-native-canvas-target-overlay.png`);
    await evalJson(String.raw`(() => {
      for (const node of document.querySelectorAll('[data-renderer-target="true"]')) {
        node.dataset.qaPreviousOutline = node.style.outline;
        node.style.outline = '2px dashed rgba(255, 70, 70, 0.95)';
      }
      return JSON.stringify(true);
    })()`);
    try {
      await runBrowser(["screenshot", "--full", nativeTargetOverlayPng]);
    } finally {
      await evalJson(String.raw`(() => { for (const node of document.querySelectorAll('[data-renderer-target="true"]')) { node.style.outline = node.dataset.qaPreviousOutline || ''; delete node.dataset.qaPreviousOutline; } return JSON.stringify(true); })()`);
    }
    const agentPair = targetSeparation(initialTargetGeometry, "agents", "agent-0", "module-agent", "Agent co-anchor");
    const moduleAgentClick = await actualCanvasClick("module_visual", "module-agent", name);
    const moduleAgentState = await evalJson(waitForSelectionScript("module_visual", "module-agent"));
    assert(moduleAgentState.selectedKind === "module_visual" && moduleAgentState.selectedId === "module-agent", "native canvas click at displaced Agent module did not select module_visual", moduleAgentState);
    await closeNativeDetails();
    const agentParentClick = await actualCanvasClick("agent", "agent-0", name);
    const agentParentState = await evalJson(waitForSelectionScript("agent", "agent-0"));
    assert(agentParentState.selectedKind === "agent" && agentParentState.selectedId === "agent-0", "native canvas click at Agent parent P did not select the parent Agent", agentParentState);
    await closeNativeDetails();

    const absoluteModuleClick = await actualCanvasClick("module_visual", "module-absolute", name);
    const absoluteModuleState = await evalJson(waitForSelectionScript("module_visual", "module-absolute"));
    await closeNativeDetails();
    const relayModuleClick = await actualCanvasClick("module_visual", "module-relay", name);
    const relayModuleState = await evalJson(waitForSelectionScript("module_visual", "module-relay"));
    assert(absoluteModuleState.selectedId === "module-absolute" && relayModuleState.selectedId === "module-relay" && absoluteModuleState.selectedId !== relayModuleState.selectedId, "native canvas clicks did not distinguish multiple displaced modules", { absoluteModuleState, relayModuleState });
    await closeNativeDetails();

    const locationFixtureUpdate = await evalJson(String.raw`(() => {
      const control = window.__OASIS7_MODULE_VISUAL_FIXTURE_CONTROL__;
      if (!control?.update) throw new Error('module fixture update control unavailable for location co-anchor');
      return JSON.stringify(control.update({
        "module-absolute": { entity_id: "module-absolute", module_id: "fixture-module", kind: "beacon", label: "Beacon marker", anchor: { type: "absolute", data: { pos: { x_cm: 1850000, y_cm: 3600000, z_cm: 0 } } } },
        "module-agent": { entity_id: "module-agent", module_id: "fixture-module", kind: "future_module_kind", label: "Unknown marker", anchor: { type: "agent", data: { agent_id: "agent-0" } } },
        "module-location": { entity_id: "module-location", module_id: "fixture-module", kind: "artifact", label: "Location artifact", anchor: { type: "location", data: { location_id: "loc-0" } } },
        "module-relay": { entity_id: "module-relay", module_id: "fixture-module", kind: "relay", label: "Relay marker", anchor: { type: "absolute", data: { pos: { x_cm: 1850000, y_cm: 3600000, z_cm: 0 } } } },
      }));
    })()`);
    assert(locationFixtureUpdate === true, "module fixture location co-anchor update failed", locationFixtureUpdate);
    const locationFixtureState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.modules.length === 4 && state.moduleMetadata.some((module)=>module.id === 'module-location' && module.anchor?.type === 'location')) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'location co-anchor fixture did not reach the real Render DTO',state}));})()`);
    const locationCameraBeforePan = await evalJson(pageStateScript());
    await evalJson(panCanvasScript(-1));
    const locationCameraAfterPan = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.camera?.pan_x_px < ${locationCameraBeforePan.camera?.pan_x_px ?? 0}) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'location co-anchor camera reframe did not move the actual renderer camera',before:${JSON.stringify(locationCameraBeforePan)},state}));})()`);
    if (name === "narrow") {
      // Reframe the actual canvas target below the focus-entry UI before native input.
      await evalJson(panCanvasScript(0, 0, 0, 150));
      await evalJson(String.raw`new Promise((resolve)=>setTimeout(resolve,1000))`);
    }
    const locationTargetGeometry = await evalJson(rendererTargetGeometryScript());
    const locationTargetGeometryPath = writeJson(`${name}-location-native-canvas-preflight.json`, locationTargetGeometry);
    const locationPair = targetSeparation(locationTargetGeometry, "locations", "loc-0", "module-location", "Location co-anchor");
    const locationModuleClick = await actualCanvasClick("module_visual", "module-location", name);
    const locationModuleState = await evalJson(waitForSelectionScript("module_visual", "module-location"));
    assert(locationModuleState.selectedKind === "module_visual" && locationModuleState.selectedId === "module-location", "native canvas click at displaced Location module did not select module_visual", locationModuleState);
    await closeNativeDetails();
    const locationParentClick = await actualCanvasClick("location", "loc-0", name);
    const locationParentState = await evalJson(waitForSelectionScript("location", "loc-0"));
    assert(locationParentState.selectedKind === "location" && locationParentState.selectedId === "loc-0", "native canvas click at Location parent P did not select the parent Location", locationParentState);
    await closeNativeDetails();
    const nativeHitEvidencePath = writeJson(`${name}-native-canvas-hit-evidence.json`, {
      input: "agent-browser mouse",
      route: "actual Bevy canvas",
      initialTargetGeometry,
      locationTargetGeometry,
      initialTargetGeometryPath,
      locationTargetGeometryPath,
      nativeTargetOverlayPng,
      agentPair,
      locationPair,
      locationCameraBeforePan,
      locationCameraAfterPan,
      clicks: { moduleAgentClick, agentParentClick, locationModuleClick, locationParentClick, absoluteModuleClick, relayModuleClick },
      states: { moduleAgentState, agentParentState, locationFixtureState, locationModuleState, locationParentState, absoluteModuleState, relayModuleState },
    });
    const nativeHitProofPng = join(outDir, `${name}-native-canvas-hit-proof.png`);
    await runBrowser(["screenshot", "--full", nativeHitProofPng]);

    const feedInjection = await evalJson(injectModuleEventFeedScript());
    assert(feedInjection.status === "ready" && feedInjection.stale === false && feedInjection.events?.some((event) => event.module_visual_entity_id === "module-agent"), "module event fixture did not enter the authoritative World Feed state", feedInjection);
    const moduleClick = await evalJson(clickModuleScript("module-absolute"));
    assert(moduleClick.id === "module-absolute" && moduleClick.keyboardReachable, "module marker click did not dispatch through a keyboard-reachable renderer target", moduleClick);
    const selectedModuleState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.selectedKind === 'module_visual' && state.selectedId === 'module-absolute' && state.moduleMarkers.some((marker)=>marker.id === 'module-absolute' && marker.selected)) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'module selection did not reach the viewer state',state}));})()`);
    assert(selectedModuleState.detailsVisible && selectedModuleState.detailsActive && selectedModuleState.moduleDetailsVisible, "selected module did not expose the visible active Command details panel", selectedModuleState);
    assert(/Module Details/i.test(selectedModuleState.detailsText) && selectedModuleState.detailsText.includes("fixture-module") && /beacon/i.test(selectedModuleState.detailsText) && selectedModuleState.detailsText.includes("Beacon marker") && /absolute/i.test(selectedModuleState.detailsText), "selected module details omitted module/kind/label/anchor evidence", selectedModuleState);
    const moduleDetailsStatePath = writeJson(`${name}-module-details-state.json`, selectedModuleState);
    const moduleDetailsPng = join(outDir, `${name}-module-details.png`); await runBrowser(["screenshot", "--full", moduleDetailsPng]);

    const preEventState = await evalJson(pageStateScript());
    let worldState = preEventState;
    if (preEventState.detailsVisible && preEventState.detailsActive) {
      await clickVisible("#viewer-details-panel .panel__route-close");
      worldState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(window.location.hash === '#viewer-stage-panel' && document.activeElement?.id === 'viewer-stage-panel') return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'module detail route did not return to World after visible Back interaction',state,hash:window.location.hash,activeElement:document.activeElement?.id || null}));})()`);
    } else {
      assert(!preEventState.detailsVisible && !preEventState.detailsActive && preEventState.selectedId === 'module-absolute' && (!preEventState.hash || preEventState.hash === '#viewer-stage-panel'), "module route was neither an active visible Command panel nor an already-returned World state", preEventState);
    }
    assert(!worldState.detailsActive && worldState.selectedId === 'module-absolute', "World route did not preserve the selected module", worldState);
    await clickVisible("#viewer-world-feed > summary");
    const feedReadyState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.eventLocators.some((locator)=>locator.id === 'module-agent')) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'module event locator did not render',state}));})()`);

    const locateTarget = await evalJson(String.raw`(() => { const locator = document.querySelector('[data-world-feed-module-locate="module-agent"]'); if (!locator) throw new Error('module event locator unavailable'); const rect = locator.getBoundingClientRect(); const style = getComputedStyle(locator); return JSON.stringify({ id: locator.dataset.worldFeedModuleLocate, keyboardReachable: locator instanceof HTMLButtonElement && locator.tabIndex >= 0, visible: rect.width > 0 && rect.height > 0 && rect.top >= 0 && rect.bottom <= innerHeight && style.display !== 'none' && style.visibility !== 'hidden', rect: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom } }); })()`);
    assert(locateTarget.id === "module-agent" && locateTarget.keyboardReachable && locateTarget.visible, "module event locator was not a visible keyboard-reachable control", locateTarget);
    const locateCameraBeforeClick = await evalJson(pageStateScript());
    const locateClick = await clickVisible('[data-world-feed-module-locate="module-agent"]');
    const locatedModuleState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.selectedKind === 'module_visual' && state.selectedId === 'module-agent' && state.moduleMarkers.some((marker)=>marker.id === 'module-agent' && marker.selected) && /Module Details/i.test(state.detailsText)) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'module event locator did not select its module',state}));})()`);
    assert(locatedModuleState.detailsVisible && locatedModuleState.detailsActive && locatedModuleState.moduleDetailsVisible, "located module did not expose the visible active Command details panel", locatedModuleState);
    assert(/future_module_kind/i.test(locatedModuleState.detailsText) && locatedModuleState.detailsText.includes("Unknown marker") && /agent\s*[·:]\s*agent-0/i.test(locatedModuleState.detailsText), "module event locate did not expose the target module details", locatedModuleState);
    await evalJson(String.raw`new Promise((resolve)=>setTimeout(resolve,250))`);
    const locatedAfterFocusState = await evalJson(pageStateScript());
    assert(locatedAfterFocusState.camera && locateCameraBeforeClick.camera && (locatedAfterFocusState.camera.pan_x_px !== locateCameraBeforeClick.camera.pan_x_px || locatedAfterFocusState.camera.pan_y_px !== locateCameraBeforeClick.camera.pan_y_px), "module event locate did not move the actual renderer camera to the target", { before: locateCameraBeforeClick.camera, after: locatedAfterFocusState.camera, locatedModuleState });
    const moduleLocateStatePath = writeJson(`${name}-module-event-locate-state.json`, locatedAfterFocusState);
    const moduleLocatePng = join(outDir, `${name}-module-event-locate.png`); await runBrowser(["screenshot", "--full", moduleLocatePng]);

    const staleUpdate = await evalJson(String.raw`(() => { const control = window.__OASIS7_MODULE_VISUAL_FIXTURE_CONTROL__; if (!control?.update) throw new Error('module fixture update control unavailable'); return JSON.stringify(control.update({"module-absolute":{"entity_id":"module-absolute","module_id":"fixture-module","kind":"beacon","label":"Beacon marker","anchor":{"type":"absolute","data":{"pos":{"x_cm":1850000,"y_cm":3600000,"z_cm":0}}}},"module-relay":{"entity_id":"module-relay","module_id":"fixture-module","kind":"relay","label":"Relay marker","anchor":{"type":"absolute","data":{"pos":{"x_cm":1850000,"y_cm":3600000,"z_cm":0}}}}})); })()`);
    assert(staleUpdate === true, "module fixture stale-target update control failed", staleUpdate);
    const staleState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.modules.length === 2 && !state.modules.some((module)=>module.id === 'module-agent') && state.selectedKind === null && state.selectedId === null && !state.moduleMarkers.some((marker)=>marker.id === 'module-agent') && !state.eventLocators.some((locator)=>locator.id === 'module-agent')) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'stale module selection or event locator remained after removal',state}));})()`);
    assert(!/Module Details|module-agent/i.test(staleState.detailsText), "stale module details remained visible after the target was removed", staleState);
    const staleStatePath = writeJson(`${name}-module-stale-state.json`, staleState);
    const stalePng = join(outDir, `${name}-module-stale.png`); await runBrowser(["screenshot", "--full", stalePng]);
    await evalJson(zoomCanvasScript(-120, 12));
    const closeState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.camera?.zoom >= 1.75) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify(state));})()`);
    await evalJson(panCanvasScript());
    const closePanState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.camera?.pan_x_px > ${closeState.camera.pan_x_px}) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify(state));})()`);
    await evalJson(scrollCanvasIntoViewScript());
    const closeViewportPng = join(outDir, `${name}-close-labels-viewport.png`); await runBrowser(["screenshot", closeViewportPng]);
    await evalJson(zoomCanvasScript(120, 16));
    const overviewState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.camera?.zoom < 1.75) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify(state));})()`);
    await evalJson(scrollCanvasIntoViewScript());
    const overviewViewportPng = join(outDir, `${name}-overview-glyphs-viewport.png`); await runBrowser(["screenshot", overviewViewportPng]);
    const beforePng = join(outDir, `${name}-before.png`); await runBrowser(["screenshot", "--full", beforePng]);
    const beforeCanvasDimensions = await evalJson(canvasDimensionsScript());
    assert(beforeCanvasDimensions.cssWidth > 0 && beforeCanvasDimensions.cssHeight > 0, "canvas has no visible element dimensions", beforeCanvasDimensions);
    const beforeCanvasPng = join(outDir, `${name}-before-canvas.png`); await runBrowser(["screenshot", "#pixel-world-embedded-runtime-canvas", beforeCanvasPng]);
    await evalJson(scrollCanvasIntoViewScript());
    const beforeViewportPng = join(outDir, `${name}-before-viewport.png`); await runBrowser(["screenshot", beforeViewportPng]);
    const cleared = await evalJson(String.raw`(() => { window.__OASIS7_MODULE_VISUAL_FIXTURE_CONTROL__.clear(); return JSON.stringify(true); })()`);
    assert(cleared === true, "module fixture clear control failed");
    const clearedState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; while(Date.now()<deadline){const state=JSON.parse(${pageStateScript()}); if(state.modules.length === 0) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error('module markers remained after clear');})()`);
    const clearedPng = join(outDir, `${name}-cleared.png`); await runBrowser(["screenshot", "--full", clearedPng]);
    const clearedCanvasDimensions = await evalJson(canvasDimensionsScript());
    const clearedCanvasPng = join(outDir, `${name}-cleared-canvas.png`); await runBrowser(["screenshot", "#pixel-world-embedded-runtime-canvas", clearedCanvasPng]);
    await evalJson(scrollCanvasIntoViewScript());
    const clearedViewportPng = join(outDir, `${name}-cleared-viewport.png`); await runBrowser(["screenshot", clearedViewportPng]);
    const updated = await evalJson(String.raw`(() => { window.__OASIS7_MODULE_VISUAL_FIXTURE_CONTROL__.update({"module-update":{"entity_id":"module-update","module_id":"fixture-module","kind":"future_module_kind","anchor":{"type":"absolute","data":{"pos":{"x_cm":1850000,"y_cm":3600000,"z_cm":0}}}}}); return JSON.stringify(true); })()`);
    assert(updated === true, "module fixture update control failed");
    const updatedState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; while(Date.now()<deadline){const state=JSON.parse(${pageStateScript()}); if(JSON.stringify(state.modules) === JSON.stringify([{ id:"module-update", kind:"future_module_kind", pos:{ x_cm:1850000, y_cm:3600000, z_cm:0 } }])) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error('unknown fallback update was not rendered or stale markers remained');})()`);
    const fallbackClick = await evalJson(clickModuleScript("module-update"));
    assert(fallbackClick.id === "module-update" && fallbackClick.keyboardReachable, "unknown-kind fallback module marker was not keyboard reachable", fallbackClick);
    const fallbackState = await evalJson(String.raw`(async()=>{const deadline=Date.now()+5000; let state; while(Date.now()<deadline){state=JSON.parse(${pageStateScript()}); if(state.selectedKind === 'module_visual' && state.selectedId === 'module-update' && /future_module_kind:module-update/i.test(state.detailsText)) return JSON.stringify(state); await new Promise((resolve)=>setTimeout(resolve,50));} throw new Error(JSON.stringify({message:'unknown module fallback label was not exposed in details',state}));})()`);
    const fallbackStatePath = writeJson(`${name}-module-fallback-state.json`, fallbackState);
    const updatedPng = join(outDir, `${name}-updated.png`); await runBrowser(["screenshot", "--full", updatedPng]);
    const updatedCanvasDimensions = await evalJson(canvasDimensionsScript());
    const updatedCanvasPng = join(outDir, `${name}-updated-canvas.png`); await runBrowser(["screenshot", "#pixel-world-embedded-runtime-canvas", updatedCanvasPng]);
    await evalJson(scrollCanvasIntoViewScript());
    const updatedViewportPng = join(outDir, `${name}-updated-viewport.png`); await runBrowser(["screenshot", updatedViewportPng]);
    const consolePath = join(outDir, `${name}-console.log`); const consoleOutput = await runBrowser(["console"]); writeFileSync(consolePath, consoleOutput);
    assert(!/\b(?:fatal|CONTEXT_LOST_WEBGL|webgl.*error)\b/i.test(consoleOutput), "browser console reports a renderer fatal", { consolePath, consoleOutput });
    summary.viewports[name] = { width, height, state, metadataEvidence, nativeHitEvidencePath, nativeHitProofPng, feedReadyState, selectedModuleState, locatedAfterFocusState, staleState, updatedState, fallbackState, closeState, closePanState, overviewState, closeViewportPng, overviewViewportPng, moduleDetailsStatePath, moduleDetailsPng, moduleLocateStatePath, moduleLocatePng, staleStatePath, stalePng, fallbackStatePath, beforePng, beforeCanvasPng, beforeViewportPng, clearedPng, clearedCanvasPng, clearedViewportPng, updatedPng, updatedCanvasPng, updatedViewportPng, canvasDimensions: { before: beforeCanvasDimensions, cleared: clearedCanvasDimensions, updated: updatedCanvasDimensions }, consolePath };
  }
  summary.status = "passed";
} catch (error) { summary.status = "failed"; summary.failure = { message: error instanceof Error ? error.message : String(error) }; throw error; }
finally { summary.completedAt = new Date().toISOString(); writeJson("summary.json", summary); closeBrowser(); await new Promise((resolveClose) => server.close(resolveClose)); }
