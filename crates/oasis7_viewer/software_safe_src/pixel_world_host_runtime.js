import { untrack } from "solid-js";
import * as core from "./legacy_core.js";
import { probePixelWorldWebgl2Surface } from "./pixel_world_runtime_loader.js";
import { installPixelWorldHotspotPointerProbe } from "./pixel_world_hotspot_probe.js";
import { applyPixelWorldRendererRoute } from "./pixel_world_renderer_route.js";
import { pixelWorldTestApiEnabled } from "./pixel_world_visual_fixture.js";

const PIXEL_WORLD_RUNTIME_CANVAS_ID = "pixel-world-embedded-runtime-canvas";

async function waitForRuntimeCanvasAttachment(canvas) {
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (canvas?.isConnected && document.getElementById(PIXEL_WORLD_RUNTIME_CANVAS_ID) === canvas) {
      return true;
    }
    await new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }
  return false;
}

export function createPixelWorldHostRuntimeController({
  adapter,
  spatial,
  scopeGuard,
  worldViewState,
  renderInput,
  rendererRoute,
  visualFixtureName,
  focusController,
  rendererStatus,
  setRendererStatus,
  rendererFatal,
  setRendererFatal,
  runtimeSource,
  setRuntimeSource,
  cameraState,
  setCameraState,
  setRetainedSurface,
  setRustRenderState,
  setWorldRevision,
  setCoreRevision,
  hoverSelection,
  hoveredHotspot,
}) {
  let mountedCanvas = null;
  let removeContextListener = () => {};
  let scopeReadPending = null;
  let hostActive = true;
  let rendererAttemptGeneration = 0;
  let rendererUpdatePending = false;
  let rendererUpdateFrame = null;

  const rendererAttemptIsCurrent = (canvas, generation) => (
    generation === rendererAttemptGeneration && mountedCanvas === canvas
  );

  function bumpGeneration() {
    rendererAttemptGeneration += 1;
    return rendererAttemptGeneration;
  }

  function attachContextListener(canvas) {
    removeContextListener();
    const lost = (event) => {
      event.preventDefault();
      adapter().simulateFatal({
        code: "pixel_world_context_lost",
        message: "World graphics context lost. Reload the page to rebuild graphics.",
      });
    };
    let start = null;
    const stopFollow = spatial.pauseFollow;
    const down = (event) => { start = { id: event.pointerId, x: event.clientX, y: event.clientY }; };
    const move = (event) => {
      if (start && start.id === event.pointerId && Math.hypot(event.clientX - start.x, event.clientY - start.y) > 4) {
        start = null;
        stopFollow();
      }
    };
    const up = () => { start = null; };
    const events = [["webglcontextlost", lost], ["pointerdown", down], ["pointermove", move], ["pointerup", up], ["pointercancel", up], ["wheel", stopFollow]];
    for (const [type, listener] of events) canvas.addEventListener(type, listener);
    removeContextListener = () => {
      for (const [type, listener] of events) canvas.removeEventListener(type, listener);
    };
  }

  function applyRendererUpdate() {
    if (rendererStatus() === "unavailable") return;
    const generation = rendererAttemptGeneration;
    const presentedScope = spatial.scope();
    const presentedView = worldViewState().view;
    const input = renderInput();
    const result = adapter().update(input);
    if (generation !== rendererAttemptGeneration) return;
    if (result?.fatal) setRendererFatal(result.fatal);
    if (result?.renderState) setRetainedSurface(null);
    setRustRenderState(result?.renderState || null);
    spatial.present(presentedView, result?.status === "ready" && !result?.fatal ? result?.renderState : null, presentedScope);
    setRendererStatus(result?.status || "ready");
    setRuntimeSource(result?.runtimeSource || adapter().runtimeSource());
    core.updatePixelWorldRuntimeMeta({
      runtimeStatus: result?.status || "ready",
      runtimeSource: result?.runtimeSource || adapter().runtimeSource(),
      runtimeModuleUrl: result?.runtimeModuleUrl || adapter().runtimeModuleUrl(),
      camera: cameraState(),
      fatal: result?.fatal || rendererFatal(),
    });
  }

  function scheduleRendererUpdate() {
    if (rendererUpdateFrame !== null) return;
    const generation = rendererAttemptGeneration;
    rendererUpdateFrame = requestAnimationFrame(() => {
      rendererUpdateFrame = null;
      if (!hostActive || generation !== rendererAttemptGeneration || scopeGuard.check().blocked) return;
      setWorldRevision((revision) => revision + 1);
      if (rendererStatus() === "ready") applyRendererUpdate();
      else rendererUpdatePending = true;
    });
  }

  async function setReadyMode(canvas = mountedCanvas, generation = rendererAttemptGeneration) {
    if (scopeGuard.check().blocked || !rendererAttemptIsCurrent(canvas, generation)) return;
    if (!canvas) {
      const fatal = { code: "pixel_world_renderer_mount_missing_canvas", message: "pixel world canvas is not mounted yet" };
      setRendererFatal(fatal); setRendererStatus("unavailable"); setRuntimeSource("detached");
      setRustRenderState(null); spatial.present(null, null);
      core.updatePixelWorldRuntimeMeta({ runtimeStatus: "unavailable", runtimeSource: "detached", runtimeModuleUrl: null, camera: null, fatal });
      return;
    }
    setRendererFatal(null); setRendererStatus("booting"); setRuntimeSource("loading");
    const attached = await waitForRuntimeCanvasAttachment(canvas);
    if (!rendererAttemptIsCurrent(canvas, generation)) return;
    if (!attached) {
      const fatal = { code: "pixel_world_renderer_canvas_detached", message: "pixel world runtime canvas never became queryable in document" };
      setRendererFatal(fatal); setRendererStatus("unavailable"); setRuntimeSource("detached");
      setRustRenderState(null); spatial.present(null, null);
      core.updatePixelWorldRuntimeMeta({ runtimeStatus: "unavailable", runtimeSource: "detached", runtimeModuleUrl: null, camera: cameraState(), fatal });
      return;
    }
    const surfaceFatal = probePixelWorldWebgl2Surface(canvas);
    if (surfaceFatal) { adapter().simulateFatal(surfaceFatal); return; }
    const presentedScope = spatial.scope();
    const presentedView = worldViewState().view;
    const result = await adapter().mount(canvas, renderInput());
    if (!rendererAttemptIsCurrent(canvas, generation)) return;
    if (result?.fatal) setRendererFatal(result.fatal);
    if (result?.renderState) setRetainedSurface(null);
    setRustRenderState(result?.renderState || null);
    spatial.present(presentedView, result?.status === "ready" && !result?.fatal ? result?.renderState : null, presentedScope);
    setRendererStatus(result?.status || "ready");
    setRuntimeSource(result?.runtimeSource || adapter().runtimeSource());
    core.updatePixelWorldRuntimeMeta({
      runtimeStatus: result?.status || "ready",
      runtimeSource: result?.runtimeSource || adapter().runtimeSource(),
      runtimeModuleUrl: result?.runtimeModuleUrl || adapter().runtimeModuleUrl(),
      camera: cameraState(),
      fatal: result?.fatal || null,
    });
    if (rendererUpdatePending) {
      rendererUpdatePending = false;
      applyRendererUpdate();
    }
  }

  const rendererNeedsReload = () => rendererFatal()?.code === "pixel_world_context_lost";

  function requestReadyMode() {
    if (rendererNeedsReload()) { window.location.reload(); return; }
    if (scopeGuard.check().blocked) { scopeReadPending = null; core.requestRender(); return; }
    const canvas = mountedCanvas?.isConnected ? mountedCanvas : null;
    mountedCanvas = canvas;
    if (rendererUpdateFrame !== null) cancelAnimationFrame(rendererUpdateFrame);
    rendererUpdateFrame = null;
    const generation = ++rendererAttemptGeneration;
    setRendererFatal(null); setRendererStatus("booting"); setRuntimeSource("loading");
    if (canvas) void setReadyMode(canvas, generation);
  }

  function onCanvasMount(canvas) {
    if (mountedCanvas === canvas) return;
    mountedCanvas = canvas;
    attachContextListener(canvas);
    if (rendererUpdateFrame !== null) cancelAnimationFrame(rendererUpdateFrame);
    rendererUpdateFrame = null;
    const generation = ++rendererAttemptGeneration;
    if (untrack(rendererStatus) !== "ready") void setReadyMode(canvas, generation);
  }

  function mountRuntimeLifecycle(registerCleanup) {
    applyPixelWorldRendererRoute(rendererRoute, core.updatePixelWorldRuntimeMeta);
    const handleKeyDown = (event) => focusController.handleKeyDown(event);
    window.addEventListener("keydown", handleKeyDown);
    registerCleanup(() => window.removeEventListener("keydown", handleKeyDown));
    const unsubscribeRenderHook = core.subscribeRenderHook?.((invalidation) => {
      const scope = scopeGuard.check();
      if (scope.scope !== spatial.scope()) {
        if (scope.changed) { setRendererStatus("unavailable"); setRustRenderState(null); spatial.present(null, null); }
        spatial.resetScope();
      }
      if (scope.changed) {
        core.state.selectedKind = null; core.state.selectedId = null; core.state.selectedObject = null;
        adapter().simulateFatal({ code: "pixel_world_scope_changed", message: "Display authorization changed. Waiting for a new authorized snapshot." });
        setRetainedSurface(null); setCameraState(null);
      }
      setCoreRevision((revision) => revision + 1);
      if (scope.blocked) {
        if (core.state.viewerProtocol?.capabilities?.includes("world_view") && scopeReadPending !== scope.scope) {
          scopeReadPending = scope.scope;
          const expected = scope.scope;
          void core.requestScopeSnapshot().then((snapshot) => {
            if (!hostActive || !scopeGuard.authorize(expected, scope.generation)) return;
            scopeReadPending = null;
            core.acceptScopeSnapshot(snapshot);
            requestReadyMode();
          }).catch(() => {});
        }
        return;
      }
      spatial.refresh(core.state.snapshot);
      if (invalidation?.pixelWorldChanged === false) return;
      scheduleRendererUpdate();
    });
    registerCleanup(unsubscribeRenderHook);
    if (pixelWorldTestApiEnabled()) {
      registerCleanup(installPixelWorldHotspotPointerProbe({
        fixtureName: visualFixtureName,
        getCanvas: () => mountedCanvas,
        getRendererStatus: rendererStatus,
        getHotspotHitTargets: () => adapter().hotspotTestHitTargets(),
        getLocationHitTargets: () => adapter().locationTestHitTargets(),
        getHoverSelection: hoverSelection,
        getHoveredHotspot: hoveredHotspot,
      }));
    }
  }

  function dispose() {
    hostActive = false;
    if (rendererUpdateFrame !== null) cancelAnimationFrame(rendererUpdateFrame);
    rendererUpdateFrame = null;
    rendererAttemptGeneration += 1;
    removeContextListener();
    adapter().unmount();
    core.updatePixelWorldRuntimeMeta({ runtimeStatus: "detached", runtimeSource: "detached", runtimeModuleUrl: null, camera: null, fatal: null });
  }

  function scheduleIfMountedReady() {
    if (mountedCanvas && rendererStatus() === "ready") scheduleRendererUpdate();
  }

  return {
    bumpGeneration,
    rendererNeedsReload,
    requestReadyMode,
    onCanvasMount,
    mountRuntimeLifecycle,
    dispose,
    scheduleIfMountedReady,
  };
}
