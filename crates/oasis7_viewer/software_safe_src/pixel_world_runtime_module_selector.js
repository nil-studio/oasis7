let runtimeModulePromise = null;
let runtimeModule = null;

function resolveBackendModuleUrl() {
  return new URL("webgl2/pixel_world_bridge.js", import.meta.url).href;
}

async function loadRuntimeModule() {
  if (!runtimeModulePromise) {
    runtimeModulePromise = import(/* @vite-ignore */ resolveBackendModuleUrl()).then((module) => {
      runtimeModule = module;
      return module;
    });
  }
  return runtimeModulePromise;
}

export const PIXEL_WORLD_RUNTIME_SOURCE = "wasm_bindgen_runtime";

export async function createPixelWorldBridge(options = {}) {
  const module = await loadRuntimeModule();
  return module.createPixelWorldBridge(options);
}

export function derivePixelWorldRenderState(input) {
  if (!runtimeModule?.derivePixelWorldRenderState) {
    throw new Error("pixel world bridge backend module is not initialized");
  }
  return runtimeModule.derivePixelWorldRenderState(input);
}

export function readPixelWorldDisplayMetrics() {
  return runtimeModule?.readPixelWorldDisplayMetrics?.() || {status:"uninitialized"};
}

export default async function initPixelWorldBridgeSelector() {
  await loadRuntimeModule();
}

export function resolveBackendModuleUrlForTest() {
  return resolveBackendModuleUrl();
}

export function __resetPixelWorldRuntimeModuleForTest() {
  runtimeModulePromise = null;
  runtimeModule = null;
}

export function readPixelWorldLinearMemoryCapacity() {
  return runtimeModule?.readPixelWorldLinearMemoryCapacity?.() || {status:"uninitialized"};
}
