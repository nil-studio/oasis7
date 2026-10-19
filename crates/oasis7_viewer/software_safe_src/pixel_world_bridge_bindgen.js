export default async function initPixelWorldBridgeModule() {
  throw new Error("pixel_world_bridge_bindgen.js is only generated in dist/pixel-world-bridge for real wasm runtime builds");
}

export class PixelWorldBridge {
  constructor() {
    throw new Error("PixelWorldBridge bindgen stub should not be instantiated outside tests or finalized dist builds");
  }
}

export function build_pixel_world_render_state() {
  throw new Error("build_pixel_world_render_state bindgen stub is unavailable in software_safe_src");
}

export function build_pixel_world_render_state_json() {
  throw new Error("build_pixel_world_render_state_json bindgen stub is unavailable in software_safe_src");
}
