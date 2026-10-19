import { test } from "node:test";
import assert from "node:assert/strict";
import { isolateBindgenModule } from "./pixel-world-bindgen-factory.mjs";
test("isolates abandoned initialization while preserving legacy exports", async () => {
 const source = `let wasm; export class PixelWorldBridge {} export function build_pixel_world_render_state_json() { return wasm; } async function __wbg_init(value) { wasm = await value; } export { __wbg_init as initSync }; export default __wbg_init;`;
 const result = isolateBindgenModule(source);
 const module = await import(`data:text/javascript,${encodeURIComponent(result)}`);
 const first = module.createPixelWorldModule(), second = module.createPixelWorldModule();
 let release; const pending = first.default(new Promise(resolve => release = resolve));
 await second.default("current"); release("abandoned"); await pending;
 assert.equal(second.build_pixel_world_render_state_json(), "current");
 assert.equal(first.build_pixel_world_render_state_json(), "abandoned");
 assert.equal(typeof module.PixelWorldBridge, "function");
 await module.default("legacy");
 assert.equal(module.build_pixel_world_render_state_json(), "legacy");
 assert.equal(second.build_pixel_world_render_state_json(), "current");
 assert.equal(typeof module.initSync, "function");
});
test("rejects changed output shapes", () => {
 for (const source of ["let wasm;", "import x from 'x'; let wasm; export default __wbg_init;", "let wasm; let wasm; export default __wbg_init;", "import{x}from 'x'; let wasm; export default __wbg_init;", "import*as x from 'x'; let wasm; export default __wbg_init;", "let wasm; export default other_init;", "let wasm; export let mutable = 1; export default __wbg_init;", "let wasm; export{unknown}; export default __wbg_init;"])
 assert.throws(() => isolateBindgenModule(source));
});

test("keeps import.meta.url in the emitted module's URL context", async () => {
 const source = `let wasm; export class PixelWorldBridge {} export function build_pixel_world_render_state_json() { return import.meta.url; } async function __wbg_init() {} export default __wbg_init;`;
 const url = `data:text/javascript,${encodeURIComponent(isolateBindgenModule(source))}`;
 const module = await import(url);
 assert.equal(module.createPixelWorldModule().build_pixel_world_render_state_json(), url);
 assert.equal(module.build_pixel_world_render_state_json(), url);
});

// Declaration/export syntax emitted by the repository's actual wasm-bindgen build.
test("supports the generated wasmModule/wasmInstance singleton and default alias", async () => {
 const source = `export class PixelWorldBridge {} export function build_pixel_world_render_state_json() { return [wasmModule, wasmInstance, wasm]; } let wasmModule, wasmInstance, wasm; function initSync(value) { wasmModule = value; wasmInstance = value; wasm = value; } async function __wbg_init(value) { const imports = {}; initSync(value); } export { initSync, __wbg_init as default };`;
 const module = await import(`data:text/javascript,${encodeURIComponent(isolateBindgenModule(source))}`);
 const first = module.createPixelWorldModule(), second = module.createPixelWorldModule();
 await first.default("first"); await second.default("second");
 assert.deepEqual(first.build_pixel_world_render_state_json(), ["first", "first", "first"]);
 assert.deepEqual(second.build_pixel_world_render_state_json(), ["second", "second", "second"]);
 module.initSync("legacy");
 assert.deepEqual(module.build_pixel_world_render_state_json(), ["legacy", "legacy", "legacy"]);
});
