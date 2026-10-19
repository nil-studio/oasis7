import { createPixelWorldModule } from "./pixel_world_bridge_bindgen.js";

export const PIXEL_WORLD_RUNTIME_SOURCE = "wasm_bindgen_runtime";
const DRAG_CLICK_SUPPRESSION_THRESHOLD_PX = 4;
const HOTSPOT_TEST_READBACK_CONTRACT = "oasis7_hotspot_pointer_evidence_v1";
const AMBIENT_FRAME_INTERVAL_MS = 1000 / 12;
const LOCATION_TEST_READBACK_CONTRACT = "oasis7_location_frame_evidence_v1";

let runtimeInitPromise = null;
let activeModule = null;
const DOWNLOAD_BUDGET_MS = 30000;
const PREPARE_BUDGET_MS = 10000;

function timeoutError(stage) {
  const error = new Error(`pixel world ${stage} timed out`);
  error.code = `pixel_world_${stage}_timeout`;
  return error;
}

function cumulativeBudget(milliseconds, stage) {
  let remaining = milliseconds;
  return async (operation, onTimeout) => {
    if (remaining <= 0) throw timeoutError(stage);
    const allowance = remaining;
    const started = performance.now();
    let timer;
    const expired = new Promise((_, reject) => {
      timer = setTimeout(() => { onTimeout?.(); reject(timeoutError(stage)); }, allowance);
    });
    try {
      const result = await Promise.race([Promise.resolve().then(operation), expired]);
      // A synchronous wasm start may prevent timer dispatch until it returns.
      if (performance.now() - started >= allowance) {
        onTimeout?.();
        throw timeoutError(stage);
      }
      return result;
    } catch (error) {
      if (performance.now() - started >= allowance) {
        onTimeout?.();
        throw timeoutError(stage);
      }
      throw error;
    } finally {
      remaining -= performance.now() - started;
      clearTimeout(timer);
    }
  };
}
let resourceStage = "descriptor";
let resourceStageStarted = performance.now();
const resourceStageDurations = {};
const resourceStageListeners = new Set();
function publishResourceStage(stage) {
  const now = performance.now();
  if (stage === "descriptor") {
    for (const key of Object.keys(resourceStageDurations)) delete resourceStageDurations[key];
  } else {
    resourceStageDurations[resourceStage] = (resourceStageDurations[resourceStage] || 0) + now - resourceStageStarted;
  }
  resourceStageStarted = now;
  resourceStage = stage;
  for (const listener of resourceStageListeners) listener(stage);
}

const PIXEL_WORLD_WASM_PAYLOAD_NAME = "pixel_world_bridge_bindgen_bg.wasm";
const PIXEL_WORLD_OPTIONAL_PAYLOAD_MANIFEST_NAME = "optional-payloads.json";
const PIXEL_WORLD_OPTIONAL_PAYLOAD_MISSING_CODE = "pixel_world_optional_payload_missing";
const PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE = "pixel_world_optional_payload_integrity_failed";
const PIXEL_WORLD_OPTIONAL_PAYLOAD_FETCH_CODE = "pixel_world_optional_payload_fetch_failed";

function legacyPixelWorldWasmUrl(moduleUrl = import.meta.url) {
  return new URL(PIXEL_WORLD_WASM_PAYLOAD_NAME, moduleUrl);
}

function optionalPayloadManifestUrl(moduleUrl = import.meta.url) {
  // The generated bridge lives at pixel-world-bridge/webgl2/. The manifest is
  // written at the primary viewer-dist root by copy-viewer-web-dist.sh.
  return new URL(`../../${PIXEL_WORLD_OPTIONAL_PAYLOAD_MANIFEST_NAME}`, moduleUrl);
}

function configuredOptionalPayloadBaseUrl(manifestUrl) {
  const configured = globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__;
  if (typeof configured === "string" && configured.trim()) {
    return new URL(configured, manifestUrl);
  }
  if (typeof window !== "undefined" && window.location) {
    const queryBase = new URL(window.location.href).searchParams.get("optional_payload_base");
    if (queryBase) {
      return new URL(queryBase, manifestUrl);
    }
  }
  return manifestUrl;
}

function optionalPayloadMissingError(reason = "source_missing") {
  const error = new Error(`optional pixel world WASM payload is unavailable: ${reason}`);
  error.code = PIXEL_WORLD_OPTIONAL_PAYLOAD_MISSING_CODE;
  return error;
}

function optionalPayloadIntegrityError(reason = "integrity_mismatch") {
  const error = new Error(`optional pixel world WASM payload integrity verification failed: ${reason}`);
  error.code = PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE;
  return error;
}

function optionalPayloadFetchError(reason = "payload_fetch_failed") {
  const error = new Error(`optional pixel world WASM payload fetch failed: ${reason}`);
  error.code = PIXEL_WORLD_OPTIONAL_PAYLOAD_FETCH_CODE;
  return error;
}

function normalizeOptionalPayloadIntegrity(payload) {
  const sizeBytes = payload?.size_bytes;
  const sha256 = typeof payload?.sha256 === "string" ? payload.sha256.trim().toLowerCase() : "";
  if (!Number.isSafeInteger(sizeBytes) || sizeBytes < 0) {
    throw optionalPayloadIntegrityError("invalid_size_bytes");
  }
  if (!/^[0-9a-f]{64}$/.test(sha256)) {
    throw optionalPayloadIntegrityError("invalid_sha256");
  }
  return { sizeBytes, sha256 };
}

async function resolvePixelWorldWasmDescriptor({ moduleUrl = import.meta.url, signal } = {}) {
  const fallbackUrl = legacyPixelWorldWasmUrl(moduleUrl);
  // Node/jsdom unit tests and source-tree development do not serve a JSON
  // manifest. Keep the legacy adjacent-WASM path in those environments.
  if (typeof window === "undefined" || typeof fetch !== "function") {
    return { url: fallbackUrl, integrity: null };
  }

  const manifestUrl = optionalPayloadManifestUrl(moduleUrl);
  if (manifestUrl.protocol !== "http:" && manifestUrl.protocol !== "https:") {
    return { url: fallbackUrl, integrity: null };
  }
  let response;
  try {
    response = await fetch(manifestUrl, { cache: "no-store", ...(signal ? {signal} : {}) });
  } catch (error) {
    throw optionalPayloadMissingError(error instanceof Error ? error.message : "manifest_fetch_failed");
  }
  if (response.status === 404) {
    return { url: fallbackUrl, integrity: null };
  }
  if (!response.ok) {
    throw optionalPayloadMissingError(`manifest_http_${response.status}`);
  }

  let manifest;
  try {
    manifest = await response.json();
  } catch (error) {
    throw optionalPayloadMissingError(error instanceof Error ? error.message : "manifest_invalid_json");
  }
  const payload = manifest?.[PIXEL_WORLD_WASM_PAYLOAD_NAME];
  if (payload?.available !== true || typeof payload.path !== "string" || !payload.path.trim()) {
    throw optionalPayloadMissingError(payload?.reason || "source_missing");
  }

  if (payload.render_input_schema_version !== undefined && payload.render_input_schema_version !== 1) {
    throw optionalPayloadIntegrityError("unsupported_render_input_schema");
  }

  let url;
  try {
    url = new URL(payload.path, configuredOptionalPayloadBaseUrl(manifestUrl));
  } catch {
    throw optionalPayloadMissingError("invalid_payload_path");
  }
  return {
    url,
    integrity: normalizeOptionalPayloadIntegrity(payload),
  };
}

async function resolvePixelWorldWasmUrl({ moduleUrl = import.meta.url } = {}) {
  const descriptor = await resolvePixelWorldWasmDescriptor({ moduleUrl });
  return descriptor.url;
}

function bytesToHex(bytes) {
  return Array.from(new Uint8Array(bytes), (value) => value.toString(16).padStart(2, "0")).join("");
}

async function downloadPayload(url, cache, downloadBudget) {
  const controller = new AbortController();
  publishResourceStage("download");
  return downloadBudget(async () => {
    try {
      const response = await fetch(url, { cache, signal: controller.signal });
      if (!response?.ok) throw optionalPayloadFetchError(`http_${response?.status ?? "unknown"}`);
      return new Uint8Array(await response.arrayBuffer());
    } catch (error) {
      if (error?.code) throw error;
      throw optionalPayloadFetchError(error instanceof Error ? error.message : "network_error");
    }
  }, () => controller.abort());
}

async function verifyPayload(bytes, integrity) {
  publishResourceStage("verify");
  if (bytes.byteLength !== integrity.sizeBytes) throw optionalPayloadIntegrityError("size_mismatch");
  if (!globalThis.crypto?.subtle?.digest) throw optionalPayloadIntegrityError("crypto_unavailable");
  let digest;
  try { digest = await globalThis.crypto.subtle.digest("SHA-256", bytes); }
  catch { throw optionalPayloadIntegrityError("digest_failed"); }
  if (bytesToHex(digest) !== integrity.sha256) throw optionalPayloadIntegrityError("sha256_mismatch");
}

async function loadPixelWorldWasmInput({ moduleUrl = import.meta.url, initialize = null } = {}) {
  publishResourceStage("descriptor");
  const downloadBudget = cumulativeBudget(DOWNLOAD_BUDGET_MS, "download");
  const prepareBudget = cumulativeBudget(PREPARE_BUDGET_MS, "prepare");
  const descriptorController = new AbortController();
  const descriptor = await downloadBudget(() => resolvePixelWorldWasmDescriptor({ moduleUrl, signal: descriptorController.signal }), () => descriptorController.abort());
  const hashed = descriptor.integrity && descriptor.url.pathname.endsWith(`/${descriptor.integrity.sha256}.wasm`);
  let bytes = await downloadPayload(descriptor.url, hashed ? "default" : "no-store", downloadBudget);
  // Verification, compilation and init share a budget separate from all downloads.
  // A late non-abortable digest/compile/init never owns the published module.
  let abandoned = false;
  const prepare = async () => {
    if (descriptor.integrity) await verifyPayload(bytes, descriptor.integrity);
    if (abandoned) throw new Error("abandoned pixel world preparation");
    if (!initialize) return bytes;
    publishResourceStage("compile");
    const compiled = await WebAssembly.compile(bytes);
    if (abandoned) throw new Error("abandoned pixel world preparation");
    publishResourceStage("prepare");
    const result = await initialize(compiled);
    return result;
  };
  try { return await prepareBudget(prepare, () => { abandoned = true; }); }
  catch (error) {
    if (!hashed || error?.code !== PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE
        || !/size_mismatch|sha256_mismatch/.test(error.message)) throw error;
    bytes = await downloadPayload(descriptor.url, "reload", downloadBudget);
    return prepareBudget(prepare, () => { abandoned = true; });
  }
}

function pixelWorldTestApiEnabled() {
  return new URLSearchParams(window.location.search || "").get("test_api") === "1";
}

function ensurePixelWorldBridgeModule() {
  if (!runtimeInitPromise) {
    // The generated factory gives each attempt private wasm-bindgen mutable
    // state. Abandoned initialization cannot overwrite a later attempt.
    const candidate = createPixelWorldModule();
    const attempt = loadPixelWorldWasmInput({ initialize: bytes => candidate.default({ module_or_path: bytes }) })
      .then(() => {
        if (runtimeInitPromise !== attempt) return;
        activeModule = candidate;
        publishResourceStage("ready");
      });
    runtimeInitPromise = attempt;
    attempt.catch(() => { if (runtimeInitPromise === attempt) { runtimeInitPromise = null; publishResourceStage("unavailable"); } });
  }
  return runtimeInitPromise;
}

function toCanvasPoint(canvas, event) {
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) {
    return null;
  }
  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  return {
    x: (event.clientX - rect.left) * scaleX,
    y: (event.clientY - rect.top) * scaleY,
  };
}

export async function createPixelWorldBridge({ onEvent, onFatal } = {}) {
  const progress = stage => onEvent?.({type:"resource_stage", stage, durations_ms: {...resourceStageDurations}});
  resourceStageListeners.add(progress);
  progress(resourceStage);
  try { await ensurePixelWorldBridgeModule(); }
  finally { resourceStageListeners.delete(progress); }

  let mountedCanvas = null;
  let animationFrameId = null;
  let removeCanvasListeners = () => {};
  let dragState = null;
  let suppressNextClick = false;
  let lastAmbientTickMs = Number.NEGATIVE_INFINITY;
  let reducedMotion = false;
  let documentVisible = true;
  let canvasVisible = true;
  let visibilityObserver = null;
  let removeLifecycleListeners = () => {};
  let animationGeneration = 0;

  const runtime = new activeModule.PixelWorldBridge(
    (event) => {
      if (mountedCanvas && event?.type === "hover_entity") {
        mountedCanvas.style.cursor = event.selection ? "pointer" : "grab";
      }
      onEvent?.(event);
    },
    (fatal) => {
      onFatal?.(fatal);
    },
  );

  function stopAnimationLoop() {
    animationGeneration += 1;
    if (animationFrameId !== null) {
      cancelAnimationFrame(animationFrameId);
      animationFrameId = null;
    }
  }

  function canTick() {
    return documentVisible && canvasVisible;
  }

  function syncRuntime() {
    if (!canTick()) return;
    runtime.tick(performance.now());
  }

  function startAnimationLoop() {
    stopAnimationLoop();
    reducedMotion = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches === true;
    runtime.set_reduced_motion(reducedMotion);
    if (reducedMotion || !canTick()) return;
    const generation = animationGeneration;
    const tick = (animationMs) => {
      animationFrameId = null;
      if (generation !== animationGeneration || !canTick()) return;
      try {
        if (animationMs - lastAmbientTickMs >= AMBIENT_FRAME_INTERVAL_MS) {
          runtime.tick(animationMs);
          lastAmbientTickMs = animationMs;
        }
        if (generation === animationGeneration && canTick()) animationFrameId = requestAnimationFrame(tick);
      } catch (error) {
        stopAnimationLoop();
        onFatal?.({
          code: "pixel_world_renderer_fatal",
          message: error instanceof Error ? error.message : String(error || "renderer fatal"),
        });
      }
    };
    animationFrameId = requestAnimationFrame(tick);
  }

  function attachLifecycleHooks(canvas) {
    removeLifecycleListeners();
    documentVisible = document.hidden !== true;
    canvasVisible = true;
    const mediaQuery = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)");
    const onMotionChange = (event) => {
      reducedMotion = event.matches === true;
      runtime.set_reduced_motion(reducedMotion);
      if (reducedMotion) stopAnimationLoop(); else resume();
    };
    mediaQuery?.addEventListener?.("change", onMotionChange);
    const resume = () => {
      if (!canTick()) return;
      try {
        syncRuntime();
        startAnimationLoop();
      } catch (error) {
        onFatal?.({ code: "pixel_world_renderer_fatal", message: error instanceof Error ? error.message : String(error) });
      }
    };
    const onVisibilityChange = () => {
      documentVisible = document.hidden !== true;
      if (documentVisible) resume(); else stopAnimationLoop();
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    const disposers = [
      () => document.removeEventListener("visibilitychange", onVisibilityChange),
      () => mediaQuery?.removeEventListener?.("change", onMotionChange),
    ];
    if (globalThis.IntersectionObserver) {
      visibilityObserver = new IntersectionObserver((entries) => {
        canvasVisible = entries.some((entry) => entry.isIntersecting);
        if (canvasVisible) resume(); else stopAnimationLoop();
      });
      visibilityObserver.observe(canvas);
      disposers.push(() => visibilityObserver?.disconnect());
    }
    removeLifecycleListeners = () => {
      for (const dispose of disposers.splice(0)) dispose();
      visibilityObserver = null;
    };
  }

  function cleanupCanvasListeners() {
    removeCanvasListeners();
    removeCanvasListeners = () => {};
    dragState = null;
    if (mountedCanvas) {
      mountedCanvas.style.cursor = "default";
    }
  }

  function attachCanvasListeners(canvas) {
    cleanupCanvasListeners();
    const disposers = [];

    const onPointerDown = (event) => {
      const point = toCanvasPoint(canvas, event);
      if (!point) {
        return;
      }
      dragState = {
        pointerId: event.pointerId,
        startX: point.x,
        startY: point.y,
        moved: false,
      };
      suppressNextClick = false;
      canvas.style.cursor = "grabbing";
      canvas.setPointerCapture?.(event.pointerId);
      runtime.pointer_down(point.x, point.y, event.pointerId);
      syncRuntime();
    };

    const onPointerMove = (event) => {
      const point = toCanvasPoint(canvas, event);
      if (!point) {
        return;
      }
      if (dragState && dragState.pointerId === event.pointerId) {
        const deltaX = point.x - dragState.startX;
        const deltaY = point.y - dragState.startY;
        dragState.moved = Math.hypot(deltaX, deltaY) > DRAG_CLICK_SUPPRESSION_THRESHOLD_PX;
      }
      runtime.pointer_move(point.x, point.y, false, event.pointerId);
      syncRuntime();
    };

    const onPointerLeave = (event) => {
      canvas.style.cursor = "default";
      runtime.pointer_move(0, 0, true, event.pointerId ?? -1);
      syncRuntime();
      // Accessible DOM controls can set hover without changing Rust's hover
      // key. Leaving the canvas clears that presentation state as well.
      onEvent?.({ type: "hover_entity", selection: null });
    };

    const onPointerUp = (event) => {
      runtime.pointer_up(event.pointerId);
      syncRuntime();
      canvas.releasePointerCapture?.(event.pointerId);
      canvas.style.cursor = "grab";
      // A cancelled pointer sequence cannot produce the compatibility click
      // emitted after a completed drag, so it must not suppress a later user click.
      suppressNextClick = event.type === "pointerup" && dragState?.moved === true;
      dragState = null;
    };

    const onWheel = (event) => {
      event.preventDefault();
      runtime.wheel(event.deltaY);
      syncRuntime();
    };

    const onClick = (event) => {
      const point = toCanvasPoint(canvas, event);
      if (!point) {
        return;
      }
      if (suppressNextClick) {
        suppressNextClick = false;
        return;
      }
      runtime.click(point.x, point.y);
      syncRuntime();
    };

    const bind = (name, handler, options) => {
      canvas.addEventListener(name, handler, options);
      disposers.push(() => canvas.removeEventListener(name, handler, options));
    };

    bind("pointerdown", onPointerDown);
    bind("pointermove", onPointerMove);
    bind("pointerleave", onPointerLeave);
    bind("pointerup", onPointerUp);
    bind("pointercancel", onPointerUp);
    bind("wheel", onWheel, { passive: false });
    bind("click", onClick);

    canvas.style.cursor = "grab";
    removeCanvasListeners = () => {
      for (const dispose of disposers.splice(0)) {
        dispose();
      }
      suppressNextClick = false;
    };
  }

  return {
    mount(canvas, renderState) {
      mountedCanvas = canvas;
      const result = runtime.mount(canvas, renderState);
      if (result?.status === "ready") {
        attachCanvasListeners(canvas);
        attachLifecycleHooks(canvas);
        syncRuntime();
        lastAmbientTickMs = performance.now();
        startAnimationLoop();
      } else {
        stopAnimationLoop();
        cleanupCanvasListeners();
        mountedCanvas = null;
      }
      return result;
    },
    update(renderState) {
      const result = runtime.update(renderState);
      syncRuntime();
      return result;
    },
    unmount() {
      stopAnimationLoop();
      removeLifecycleListeners();
      cleanupCanvasListeners();
      const result = runtime.unmount();
      mountedCanvas = null;
      return result;
    },
    hotspotTestHitTargets() {
      if (!pixelWorldTestApiEnabled()) {
        return [];
      }
      return runtime.hotspot_test_hit_targets(HOTSPOT_TEST_READBACK_CONTRACT) || [];
    },
    locationTestHitTargets() {
      if (!pixelWorldTestApiEnabled()) {
        return [];
      }
      return runtime.location_test_hit_targets(LOCATION_TEST_READBACK_CONTRACT) || [];
    },
  };
}

export function derivePixelWorldRenderState(input) {
  if (!activeModule) {
    throw new Error("pixel world bridge module is not initialized");
  }
  if (input?.schema_version !== 1) throw new Error("unsupported pixel world render input version");
  return activeModule.build_pixel_world_render_state_json(JSON.stringify(input));
}

export {
  PIXEL_WORLD_OPTIONAL_PAYLOAD_MANIFEST_NAME,
  PIXEL_WORLD_OPTIONAL_PAYLOAD_MISSING_CODE,
  PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE,
  PIXEL_WORLD_OPTIONAL_PAYLOAD_FETCH_CODE,
  PIXEL_WORLD_WASM_PAYLOAD_NAME,
  loadPixelWorldWasmInput as loadPixelWorldWasmInputForTest,
  resolvePixelWorldWasmUrl as resolvePixelWorldWasmUrlForTest,
};
