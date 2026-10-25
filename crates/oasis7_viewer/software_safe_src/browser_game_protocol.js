// Independent protocol WASM, never the rendering bridge or page-global helpers.
let loaded;
export function loadBrowserGameProtocol() {
  if (!loaded) loaded = (async () => {
    const url = new URL("./browser-game-client/oasis7_browser_game_client.js", import.meta.url).href;
    const bridge = await import(/* @vite-ignore */ url);
    await bridge.default({ module_or_path: new URL("./browser-game-client/oasis7_browser_game_client_bg.wasm", import.meta.url) });
    if (["world_signing_bytes", "world_request_digest", "world_verify_response", "world_verify_gateway_response", "world_validate", "world_build"].some((name) => typeof bridge[name] !== "function")) throw new Error("game_protocol_unavailable");
    return Object.freeze({
      signingBytes: bridge.world_signing_bytes,
      requestDigest: bridge.world_request_digest,
      verifyResponse: bridge.world_verify_response,
      verifyGatewayResponse: bridge.world_verify_gateway_response,
      validate: bridge.world_validate,
      build: bridge.world_build,
    });
  })().catch(() => { loaded = null; throw new Error("game_protocol_unavailable"); });
  return loaded;
}
