import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const runtimeState = vi.hoisted(() => ({
  mountImpl: null,
  instances: [],
}));

const originalRequestAnimationFrame = globalThis.requestAnimationFrame;
const originalCancelAnimationFrame = globalThis.cancelAnimationFrame;
const originalMatchMedia = globalThis.matchMedia;

function animationHarness() {
  const callbacks = new Map();
  let nextFrameId = 1;
  globalThis.requestAnimationFrame = vi.fn((callback) => {
    const frameId = nextFrameId;
    nextFrameId += 1;
    callbacks.set(frameId, callback);
    return frameId;
  });
  globalThis.cancelAnimationFrame = vi.fn((frameId) => callbacks.delete(frameId));
  globalThis.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
  return {
    runNext(timestamp) {
      const next = callbacks.entries().next().value;
      if (!next) return false;
      const [frameId, callback] = next;
      callbacks.delete(frameId);
      callback(timestamp);
      return true;
    },
  };
}

const INTEGRITY_PAYLOAD_BYTES = Uint8Array.from([1, 2, 3, 4]);
const INTEGRITY_PAYLOAD_SHA256 = "9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a";

function digestBufferFromHex(hex) {
  return Uint8Array.from(hex.match(/.{2}/g).map((pair) => Number.parseInt(pair, 16))).buffer;
}

function installIntegrityCrypto({ digestHex = INTEGRITY_PAYLOAD_SHA256, digestError = null } = {}) {
  const originalCrypto = globalThis.crypto;
  const digest = vi.fn(async () => {
    if (digestError) throw digestError;
    return digestBufferFromHex(digestHex);
  });
  Object.defineProperty(globalThis, "crypto", {
    configurable: true,
    value: { subtle: { digest } },
  });
  return {
    digest,
    restore() {
      if (originalCrypto === undefined) {
        delete globalThis.crypto;
      } else {
        Object.defineProperty(globalThis, "crypto", {
          configurable: true,
          value: originalCrypto,
        });
      }
    },
  };
}

function integrityManifest(overrides = {}) {
  return {
    "pixel_world_bridge_bindgen_bg.wasm": {
      available: true,
      path: "viewer-optional-payload/pixel_world_bridge_bindgen_bg.wasm",
      size_bytes: INTEGRITY_PAYLOAD_BYTES.byteLength,
      sha256: INTEGRITY_PAYLOAD_SHA256,
      ...overrides,
    },
  };
}

vi.mock("./pixel_world_bridge_bindgen.js", () => {
  class MockPixelWorldBridge {
    constructor(onEvent, onFatal) {
      this.onEvent = onEvent;
      this.onFatal = onFatal;
      this.pointer_down = vi.fn();
      this.pointer_move = vi.fn();
      this.pointer_up = vi.fn();
      this.wheel = vi.fn();
      this.click = vi.fn();
      this.tick = vi.fn();
      this.set_reduced_motion = vi.fn();
      this.unmount = vi.fn(() => ({ status: "detached" }));
      this.update = vi.fn(() => ({ status: "ready" }));
      this.mount = vi.fn((canvas, renderState) => {
        if (runtimeState.mountImpl) {
          return runtimeState.mountImpl(this, canvas, renderState);
        }
        return { status: "ready" };
      });
      runtimeState.instances.push(this);
    }
  }

  return {
    __esModule: true,
    default: vi.fn(async () => undefined),
    PixelWorldBridge: MockPixelWorldBridge,
    build_pixel_world_render_state_json: vi.fn(),
    createPixelWorldModule: vi.fn(() => ({ default: vi.fn(async () => undefined), PixelWorldBridge: MockPixelWorldBridge, build_pixel_world_render_state_json: vi.fn() })),
  };
});

describe("pixel world wasm runtime bridge", () => {
  beforeEach(() => { vi.spyOn(WebAssembly, "compile").mockResolvedValue({}); vi.stubGlobal("fetch", vi.fn(async () => ({ok:true, arrayBuffer:async () => new ArrayBuffer(0)}))); });
  afterEach(() => {
    runtimeState.mountImpl = null;
    runtimeState.instances.length = 0;
    globalThis.requestAnimationFrame = originalRequestAnimationFrame;
    globalThis.cancelAnimationFrame = originalCancelAnimationFrame;
    globalThis.matchMedia = originalMatchMedia;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  function pointerEvent(type, options = {}) {
    const event = new MouseEvent(type, {
      bubbles: true,
      cancelable: true,
      clientX: options.clientX ?? 0,
      clientY: options.clientY ?? 0,
    });
    Object.defineProperty(event, "pointerId", {
      configurable: true,
      value: options.pointerId ?? 1,
    });
    return event;
  }

  it("maps CSS click coordinates into the canvas backing coordinate space", async () => {
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const onEvent = vi.fn();
    const bridge = await createPixelWorldBridge({ onEvent });
    const canvas = document.createElement("canvas");
    canvas.width = 960;
    canvas.height = 540;
    canvas.getBoundingClientRect = () => ({
      left: 10,
      top: 20,
      width: 480,
      height: 270,
      right: 490,
      bottom: 290,
      x: 10,
      y: 20,
      toJSON() {
        return this;
      },
    });
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();

    runtimeState.mountImpl = (instance) => {
      instance.click.mockImplementation((x, y) => {
        instance.onEvent?.({
          type: "select_entity",
          selection: {
            kind: "agent",
            id: `${x},${y}`,
          },
        });
        return { status: "ready" };
      });
      return { status: "ready" };
    };

    bridge.mount(canvas, { selection: null });
    canvas.dispatchEvent(new MouseEvent("click", {
      clientX: 130,
      clientY: 95,
      bubbles: true,
    }));

    expect(runtimeState.instances).toHaveLength(1);
    expect(runtimeState.instances[0].click).toHaveBeenCalledWith(240, 150);
    expect(onEvent).toHaveBeenCalledWith({
      type: "select_entity",
      selection: {
        kind: "agent",
        id: "240,150",
      },
    });
  });

  it("resolves an available optional WASM payload from the served manifest route", async () => {
    const originalFetch = globalThis.fetch;
    const originalBaseUrl = globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__;
    const fetchMock = vi.fn(async () => ({
      status: 200,
      ok: true,
      json: async () => ({
        "pixel_world_bridge_bindgen_bg.wasm": {
          available: true,
          path: "pixel_world_bridge_bindgen_bg.wasm",
          size_bytes: 4,
          sha256: "9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a",
        },
      }),
    }));
    globalThis.fetch = fetchMock;
    globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__ = "/optional-payload/";
    try {
      const { resolvePixelWorldWasmUrlForTest } = await import("./pixel_world_runtime_module_wasm.js");
      const resolved = await resolvePixelWorldWasmUrlForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      });
      expect(resolved.href).toBe("https://example.test/optional-payload/pixel_world_bridge_bindgen_bg.wasm");
      expect(fetchMock).toHaveBeenCalledWith(
        new URL("https://example.test/viewer/optional-payloads.json"),
        { cache: "no-store" },
      );
    } finally {
      globalThis.fetch = originalFetch;
      if (originalBaseUrl === undefined) {
        delete globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__;
      } else {
        globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__ = originalBaseUrl;
      }
    }
  });

  it("resolves the published split-delivery payload beside the archive web root", async () => {
    const originalFetch = globalThis.fetch;
    const originalBaseUrl = globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__;
    const fetchMock = vi.fn(async () => ({
      status: 200,
      ok: true,
      json: async () => ({
        "pixel_world_bridge_bindgen_bg.wasm": {
          available: true,
          path: "viewer-optional-payload/pixel_world_bridge_bindgen_bg.wasm",
          size_bytes: 4,
          sha256: "9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a",
        },
      }),
    }));
    globalThis.fetch = fetchMock;
    delete globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__;
    try {
      const { resolvePixelWorldWasmUrlForTest } = await import("./pixel_world_runtime_module_wasm.js");
      const resolved = await resolvePixelWorldWasmUrlForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      });
      expect(resolved.href).toBe("https://example.test/viewer/viewer-optional-payload/pixel_world_bridge_bindgen_bg.wasm");
      expect(fetchMock).toHaveBeenCalledWith(
        new URL("https://example.test/viewer/optional-payloads.json"),
        { cache: "no-store" },
      );
    } finally {
      globalThis.fetch = originalFetch;
      if (originalBaseUrl === undefined) {
        delete globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__;
      } else {
        globalThis.__OASIS7_VIEWER_OPTIONAL_PAYLOAD_BASE_URL__ = originalBaseUrl;
      }
    }
  });

  it("fetches and verifies the split payload before wasm-bindgen initialization", async () => {
    const originalFetch = globalThis.fetch;
    const cryptoHarness = installIntegrityCrypto();
    const fetchMock = vi.fn(async (url) => {
      if (String(url).endsWith("optional-payloads.json")) {
        return {
          status: 200,
          ok: true,
          json: async () => integrityManifest(),
        };
      }
      return {
        status: 200,
        ok: true,
        arrayBuffer: async () => INTEGRITY_PAYLOAD_BYTES.buffer,
      };
    });
    globalThis.fetch = fetchMock;
    try {
      const { loadPixelWorldWasmInputForTest } = await import("./pixel_world_runtime_module_wasm.js");
      const input = await loadPixelWorldWasmInputForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      });
      expect(input).toEqual(INTEGRITY_PAYLOAD_BYTES);
      expect(cryptoHarness.digest).toHaveBeenCalledWith("SHA-256", INTEGRITY_PAYLOAD_BYTES);
      expect(fetchMock).toHaveBeenNthCalledWith(
        2,
        new URL("https://example.test/viewer/viewer-optional-payload/pixel_world_bridge_bindgen_bg.wasm"),
        { cache: "no-store", signal: expect.any(AbortSignal) },
      );
    } finally {
      globalThis.fetch = originalFetch;
      cryptoHarness.restore();
    }
  });

  it("uses HTTP cache only for hash-addressed payloads and reloads corrupt cached bytes once", async () => {
    const previousFetch = globalThis.fetch;
    const crypto = installIntegrityCrypto();
    const attempts = [];
    const hashedPath = `viewer-optional-payload/${INTEGRITY_PAYLOAD_SHA256}.wasm`;
    globalThis.fetch = vi.fn(async (url, options) => {
      if (String(url).endsWith("optional-payloads.json")) return {status:200,ok:true,json:async()=>integrityManifest({path:hashedPath})};
      attempts.push(options.cache);
      return {status:200,ok:true,arrayBuffer:async()=>attempts.length === 1 ? new Uint8Array([0]).buffer : INTEGRITY_PAYLOAD_BYTES.buffer};
    });
    try {
      const {loadPixelWorldWasmInputForTest} = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({moduleUrl:"https://example.test/pixel-world-bridge/webgl2/pixel_world_bridge.js"})).resolves.toEqual(INTEGRITY_PAYLOAD_BYTES);
      expect(attempts).toEqual(["default", "reload"]);
    } finally { globalThis.fetch = previousFetch; crypto.restore(); }
  });

  it("rejects an incompatible descriptor before fetching or initializing its payload", async () => {
    const previousFetch = globalThis.fetch;
    globalThis.fetch = vi.fn(async () => ({status:200,ok:true,json:async()=>integrityManifest({render_input_schema_version:2})}));
    try {
      const {loadPixelWorldWasmInputForTest} = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({moduleUrl:"https://example.test/pixel-world-bridge/webgl2/pixel_world_bridge.js"})).rejects.toThrow(/unsupported_render_input_schema/);
      expect(globalThis.fetch).toHaveBeenCalledTimes(1);
    } finally { globalThis.fetch = previousFetch; }
  });

  it("rejects an available manifest that omits integrity metadata", async () => {
    const originalFetch = globalThis.fetch;
    const cryptoHarness = installIntegrityCrypto();
    globalThis.fetch = vi.fn(async () => ({
      status: 200,
      ok: true,
      json: async () => integrityManifest({ size_bytes: undefined, sha256: undefined }),
    }));
    try {
      const { loadPixelWorldWasmInputForTest, PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE } = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      })).rejects.toMatchObject({
        code: PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE,
        message: expect.stringContaining("invalid_size_bytes"),
      });
      expect(cryptoHarness.digest).not.toHaveBeenCalled();
    } finally {
      globalThis.fetch = originalFetch;
      cryptoHarness.restore();
    }
  });

  it("rejects a split payload whose byte length differs from the manifest", async () => {
    const originalFetch = globalThis.fetch;
    const cryptoHarness = installIntegrityCrypto();
    const fetchMock = vi.fn(async (url) => String(url).endsWith("optional-payloads.json")
      ? { status: 200, ok: true, json: async () => integrityManifest({ size_bytes: 5 }) }
      : { status: 200, ok: true, arrayBuffer: async () => INTEGRITY_PAYLOAD_BYTES.buffer });
    globalThis.fetch = fetchMock;
    try {
      const { loadPixelWorldWasmInputForTest, PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE } = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      })).rejects.toMatchObject({
        code: PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE,
        message: expect.stringContaining("size_mismatch"),
      });
      expect(cryptoHarness.digest).not.toHaveBeenCalled();
    } finally {
      globalThis.fetch = originalFetch;
      cryptoHarness.restore();
    }
  });

  it("rejects a split payload whose SHA-256 differs from the manifest", async () => {
    const originalFetch = globalThis.fetch;
    const cryptoHarness = installIntegrityCrypto({ digestHex: INTEGRITY_PAYLOAD_SHA256 });
    const fetchMock = vi.fn(async (url) => String(url).endsWith("optional-payloads.json")
      ? { status: 200, ok: true, json: async () => integrityManifest({ sha256: "0".repeat(64) }) }
      : { status: 200, ok: true, arrayBuffer: async () => INTEGRITY_PAYLOAD_BYTES.buffer });
    globalThis.fetch = fetchMock;
    try {
      const { loadPixelWorldWasmInputForTest, PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE } = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      })).rejects.toMatchObject({
        code: PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE,
        message: expect.stringContaining("sha256_mismatch"),
      });
    } finally {
      globalThis.fetch = originalFetch;
      cryptoHarness.restore();
    }
  });

  it("surfaces deterministic typed errors for payload fetch and crypto failures", async () => {
    const originalFetch = globalThis.fetch;
    const cryptoHarness = installIntegrityCrypto({ digestError: new Error("subtle unavailable") });
    const fetchMock = vi.fn(async (url) => {
      if (String(url).endsWith("optional-payloads.json")) {
        return { status: 200, ok: true, json: async () => integrityManifest() };
      }
      throw new Error("payload connection reset");
    });
    globalThis.fetch = fetchMock;
    try {
      const { loadPixelWorldWasmInputForTest, PIXEL_WORLD_OPTIONAL_PAYLOAD_FETCH_CODE } = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      })).rejects.toMatchObject({
        code: PIXEL_WORLD_OPTIONAL_PAYLOAD_FETCH_CODE,
        message: expect.stringContaining("payload connection reset"),
      });

      globalThis.fetch = vi.fn(async (url) => String(url).endsWith("optional-payloads.json")
        ? { status: 200, ok: true, json: async () => integrityManifest() }
        : { status: 200, ok: true, arrayBuffer: async () => INTEGRITY_PAYLOAD_BYTES.buffer });
      const { PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE } = await import("./pixel_world_runtime_module_wasm.js");
      await expect(loadPixelWorldWasmInputForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      })).rejects.toMatchObject({
        code: PIXEL_WORLD_OPTIONAL_PAYLOAD_INTEGRITY_CODE,
        message: expect.stringContaining("digest_failed"),
      });
    } finally {
      globalThis.fetch = originalFetch;
      cryptoHarness.restore();
    }
  });

  it("returns a deterministic missing-payload error instead of probing a stale adjacent WASM", async () => {
    const originalFetch = globalThis.fetch;
    const fetchMock = vi.fn(async () => ({
      status: 200,
      ok: true,
      json: async () => ({
        "pixel_world_bridge_bindgen_bg.wasm": {
          available: false,
          reason: "source_missing",
        },
      }),
    }));
    globalThis.fetch = fetchMock;
    try {
      const { resolvePixelWorldWasmUrlForTest, PIXEL_WORLD_OPTIONAL_PAYLOAD_MISSING_CODE } = await import("./pixel_world_runtime_module_wasm.js");
      await expect(resolvePixelWorldWasmUrlForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      })).rejects.toMatchObject({
        code: PIXEL_WORLD_OPTIONAL_PAYLOAD_MISSING_CODE,
        message: expect.stringContaining("source_missing"),
      });
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("keeps the adjacent WASM URL for development when no manifest is served", async () => {
    const originalFetch = globalThis.fetch;
    const fetchMock = vi.fn(async () => ({ status: 404, ok: false }));
    globalThis.fetch = fetchMock;
    try {
      const { resolvePixelWorldWasmUrlForTest } = await import("./pixel_world_runtime_module_wasm.js");
      const resolved = await resolvePixelWorldWasmUrlForTest({
        moduleUrl: "https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge.js",
      });
      expect(resolved.href).toBe("https://example.test/viewer/pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm");
      expect(fetchMock).toHaveBeenCalledWith(
        new URL("https://example.test/viewer/optional-payloads.json"),
        { cache: "no-store" },
      );
    } finally {
      globalThis.fetch = originalFetch;
    }
  });

  it("clears DOM-origin hover on canvas leave even when the runtime has no hover transition", async () => {
    animationHarness();
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const onEvent = vi.fn();
    const bridge = await createPixelWorldBridge({ onEvent });
    const canvas = document.createElement('canvas');
    bridge.mount(canvas, {});
    canvas.dispatchEvent(pointerEvent('pointerleave'));
    expect(onEvent).toHaveBeenCalledWith({ type: 'hover_entity', selection: null });
    bridge.unmount();
  });

  it("caps WASM ambient ticks at 12Hz while leaving pointer input immediate", async () => {
    const animation = animationHarness();
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const bridge = await createPixelWorldBridge();
    const canvas = document.createElement("canvas");
    canvas.width = 480;
    canvas.height = 270;
    canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 480, height: 270 });
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();

    bridge.mount(canvas, { selection: null });
    for (let timestamp = 0; timestamp <= 1_000; timestamp += 16) animation.runNext(timestamp);

    expect(runtimeState.instances).toHaveLength(1);
    expect(runtimeState.instances[0].tick.mock.calls.length).toBeGreaterThan(0);
    expect(runtimeState.instances[0].tick.mock.calls.length).toBeLessThanOrEqual(12);
    canvas.dispatchEvent(pointerEvent("pointerdown", { clientX: 10, clientY: 10 }));
    expect(runtimeState.instances[0].pointer_down).toHaveBeenCalledWith(10, 10, 1);
    bridge.unmount();
  });

  it("reacts to reduced-motion preference changes and removes its media listener on unmount", async () => {
    const animation = animationHarness();
    let mediaChange = null;
    const mediaQuery = {
      matches: false,
      addEventListener: vi.fn((event, callback) => { if (event === "change") mediaChange = callback; }),
      removeEventListener: vi.fn(),
    };
    globalThis.matchMedia = vi.fn(() => mediaQuery);
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const bridge = await createPixelWorldBridge();
    const canvas = document.createElement("canvas");
    canvas.width = 480;
    canvas.height = 270;
    canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 480, height: 270 });

    bridge.mount(canvas, { selection: null });
    expect(mediaQuery.addEventListener).toHaveBeenCalledWith("change", expect.any(Function));
    mediaChange?.({ matches: true });
    expect(runtimeState.instances[0].set_reduced_motion).toHaveBeenLastCalledWith(true);
    expect(runtimeState.instances[0].tick).toHaveBeenCalledTimes(1);
    expect(animation.runNext(100)).toBe(false);
    mediaQuery.matches = false;
    mediaChange?.({ matches: false });
    expect(runtimeState.instances[0].set_reduced_motion).toHaveBeenLastCalledWith(false);
    bridge.unmount();
    expect(mediaQuery.removeEventListener).toHaveBeenCalledWith("change", expect.any(Function));
  });

  it("suppresses the synthetic click that follows a drag gesture", async () => {
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const bridge = await createPixelWorldBridge();
    const canvas = document.createElement("canvas");
    canvas.width = 480;
    canvas.height = 270;
    canvas.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 480,
      height: 270,
      right: 480,
      bottom: 270,
      x: 0,
      y: 0,
      toJSON() {
        return this;
      },
    });
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();

    bridge.mount(canvas, { selection: null });
    canvas.dispatchEvent(pointerEvent("pointerdown", { clientX: 10, clientY: 10 }));
    canvas.dispatchEvent(pointerEvent("pointermove", { clientX: 48, clientY: 10 }));
    canvas.dispatchEvent(pointerEvent("pointerup", { clientX: 48, clientY: 10 }));
    canvas.dispatchEvent(new MouseEvent("click", {
      bubbles: true,
      clientX: 48,
      clientY: 10,
    }));

    expect(runtimeState.instances).toHaveLength(1);
    expect(runtimeState.instances[0].click).not.toHaveBeenCalled();
  });

  it("does not suppress a later click after a cancelled drag", async () => {
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const bridge = await createPixelWorldBridge();
    const canvas = document.createElement("canvas");
    canvas.width = 480;
    canvas.height = 270;
    canvas.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 480,
      height: 270,
      right: 480,
      bottom: 270,
      x: 0,
      y: 0,
      toJSON() {
        return this;
      },
    });
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();

    bridge.mount(canvas, { selection: null });
    canvas.dispatchEvent(pointerEvent("pointerdown", { clientX: 10, clientY: 10 }));
    canvas.dispatchEvent(pointerEvent("pointermove", { clientX: 48, clientY: 10 }));
    canvas.dispatchEvent(pointerEvent("pointercancel", { clientX: 48, clientY: 10 }));
    canvas.dispatchEvent(new MouseEvent("click", {
      bubbles: true,
      clientX: 96,
      clientY: 30,
    }));

    expect(runtimeState.instances).toHaveLength(1);
    expect(runtimeState.instances[0].click).toHaveBeenCalledWith(96, 30);
  });

  it("keeps click selection active when pointer jitter stays below the drag threshold", async () => {
    const { createPixelWorldBridge } = await import("./pixel_world_runtime_module_wasm.js");
    const bridge = await createPixelWorldBridge();
    const canvas = document.createElement("canvas");
    canvas.width = 480;
    canvas.height = 270;
    canvas.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 480,
      height: 270,
      right: 480,
      bottom: 270,
      x: 0,
      y: 0,
      toJSON() {
        return this;
      },
    });
    canvas.setPointerCapture = vi.fn();
    canvas.releasePointerCapture = vi.fn();

    bridge.mount(canvas, { selection: null });
    canvas.dispatchEvent(pointerEvent("pointerdown", { clientX: 10, clientY: 10 }));
    canvas.dispatchEvent(pointerEvent("pointermove", { clientX: 12, clientY: 11 }));
    canvas.dispatchEvent(pointerEvent("pointerup", { clientX: 12, clientY: 11 }));
    canvas.dispatchEvent(new MouseEvent("click", {
      bubbles: true,
      clientX: 12,
      clientY: 11,
    }));

    expect(runtimeState.instances).toHaveLength(1);
    expect(runtimeState.instances[0].click).toHaveBeenCalledWith(12, 11);
  });
  it("aborts a stalled response body after the download budget", async () => {
    vi.useFakeTimers();
    const { loadPixelWorldWasmInputForTest } = await import("./pixel_world_runtime_module_wasm.js");
    let signal;
    globalThis.fetch = vi.fn(async (_url, options) => {
      signal = options.signal;
      return {ok:true, arrayBuffer:() => new Promise(() => {})};
    });
    const failed = expect(loadPixelWorldWasmInputForTest()).rejects.toMatchObject({code:"pixel_world_download_timeout"});
    await vi.advanceTimersByTimeAsync(30000);
    await failed;
    expect(signal.aborted).toBe(true);
  });

  it.each([{digestMs:8000, compileMs:3000}, {digestMs:11000, compileMs:0}])("uses one hash/compile/init budget and ignores late work (%j)", async ({digestMs, compileMs}) => {
    vi.useFakeTimers();
    const cryptoHarness = installIntegrityCrypto();
    cryptoHarness.digest.mockImplementation(() => new Promise(resolve => setTimeout(() => resolve(digestBufferFromHex(INTEGRITY_PAYLOAD_SHA256)), digestMs)));
    globalThis.fetch = vi.fn(async url => String(url).endsWith("optional-payloads.json")
      ? {ok:true, json:async () => integrityManifest()} : {ok:true, arrayBuffer:async () => INTEGRITY_PAYLOAD_BYTES.buffer});
    const {loadPixelWorldWasmInputForTest} = await import("./pixel_world_runtime_module_wasm.js");
    WebAssembly.compile.mockImplementation(() => new Promise(resolve => setTimeout(() => resolve({}), compileMs)));
    const initialize = vi.fn();
    try {
      const failed = expect(loadPixelWorldWasmInputForTest({moduleUrl:"https://example.test/pixel-world-bridge/webgl2/bridge.js", initialize})).rejects.toMatchObject({code:"pixel_world_prepare_timeout"});
      await vi.advanceTimersByTimeAsync(10000); await failed;
      await vi.advanceTimersByTimeAsync(2000);
      expect(initialize).not.toHaveBeenCalled();
    } finally { cryptoHarness.restore(); }
  });

  it("isolates timed-out initialization, retries explicitly and reuses the successful single flight", async () => {
    vi.resetModules();
    vi.useFakeTimers();
    const bindings = await import("./pixel_world_bridge_bindgen.js");
    let finishOld;
    const oldModule = {default:vi.fn(() => new Promise(resolve => {finishOld = resolve;})), PixelWorldBridge:bindings.PixelWorldBridge, build_pixel_world_render_state_json:vi.fn(() => "old")};
    const currentModule = {default:vi.fn(async () => {}), PixelWorldBridge:bindings.PixelWorldBridge, build_pixel_world_render_state_json:vi.fn(() => "current")};
    bindings.createPixelWorldModule.mockReturnValueOnce(oldModule).mockReturnValueOnce(currentModule);
    const {createPixelWorldBridge, derivePixelWorldRenderState} = await import("./pixel_world_runtime_module_wasm.js");
    const failed = expect(createPixelWorldBridge()).rejects.toMatchObject({code:"pixel_world_prepare_timeout"});
    await vi.advanceTimersByTimeAsync(10000); await failed;
    const first = createPixelWorldBridge(), second = createPixelWorldBridge();
    await Promise.all([first, second]);
    expect(currentModule.default).toHaveBeenCalledTimes(1);
    finishOld(); await Promise.resolve(); await Promise.resolve();
    expect(derivePixelWorldRenderState({schema_version:1})).toBe("current");
    await createPixelWorldBridge();
    expect(currentModule.default).toHaveBeenCalledTimes(1);
  });

  it.each([
    {firstDownloadMs:25000, reloadMs:6000, timesOut:true},
    {firstDownloadMs:0, reloadMs:15000, timesOut:false},
  ])("shares download allowance across cache repair without charging it to preparation (%j)", async ({firstDownloadMs, reloadMs, timesOut}) => {
    vi.useFakeTimers();
    const cryptoHarness = installIntegrityCrypto();
    cryptoHarness.digest.mockImplementationOnce(() => new Promise(resolve => setTimeout(() => resolve(digestBufferFromHex("00".repeat(32))), 1000)));
    let downloads = 0;
    const signals = [];
    globalThis.fetch = vi.fn(async (url, options) => {
      if (String(url).endsWith("optional-payloads.json")) return {ok:true, json:async () => integrityManifest({path:`viewer-optional-payload/${INTEGRITY_PAYLOAD_SHA256}.wasm`})};
      const delay = downloads++ ? reloadMs : firstDownloadMs;
      signals.push(options.signal);
      return {ok:true, arrayBuffer:() => new Promise(resolve => setTimeout(() => resolve(INTEGRITY_PAYLOAD_BYTES.buffer), delay))};
    });
    const {loadPixelWorldWasmInputForTest} = await import("./pixel_world_runtime_module_wasm.js");
    const initialize = vi.fn(() => new Promise(resolve => setTimeout(() => resolve("ready"), 2000)));
    try {
      const operation = loadPixelWorldWasmInputForTest({moduleUrl:"https://example.test/pixel-world-bridge/webgl2/bridge.js", initialize});
      const checked = timesOut ? expect(operation).rejects.toMatchObject({code:"pixel_world_download_timeout"}) : expect(operation).resolves.toBe("ready");
      await vi.advanceTimersByTimeAsync(40000); await checked;
      expect(downloads).toBe(2);
      if (timesOut) { expect(signals[1].aborted).toBe(true); expect(initialize).not.toHaveBeenCalled(); }
      else expect(initialize).toHaveBeenCalledTimes(1);
    } finally { cryptoHarness.restore(); }
  });

  it("aborts a stalled manifest body within the shared download budget", async () => {
    vi.useFakeTimers();
    let signal;
    globalThis.fetch = vi.fn(async (_url, options) => {
      signal = options.signal;
      return {ok:true, json:() => new Promise(() => {})};
    });
    const {loadPixelWorldWasmInputForTest} = await import("./pixel_world_runtime_module_wasm.js");
    const failed = expect(loadPixelWorldWasmInputForTest({moduleUrl:"https://example.test/pixel-world-bridge/webgl2/bridge.js"})).rejects.toMatchObject({code:"pixel_world_download_timeout"});
    await vi.advanceTimersByTimeAsync(30000); await failed;
    expect(signal.aborted).toBe(true);
  });

  it("rejects synchronous initialization overruns without relying on timer dispatch", async () => {
    const cryptoHarness = installIntegrityCrypto();
    let elapsed = 0;
    vi.spyOn(performance, "now").mockImplementation(() => elapsed);
    globalThis.fetch = vi.fn(async url => String(url).endsWith("optional-payloads.json")
      ? {ok:true, json:async () => integrityManifest()} : {ok:true, arrayBuffer:async () => INTEGRITY_PAYLOAD_BYTES.buffer});
    const {loadPixelWorldWasmInputForTest} = await import("./pixel_world_runtime_module_wasm.js");
    try {
      await expect(loadPixelWorldWasmInputForTest({moduleUrl:"https://example.test/pixel-world-bridge/webgl2/bridge.js", initialize:() => {elapsed += 11000;}})).rejects.toMatchObject({code:"pixel_world_prepare_timeout"});
    } finally { cryptoHarness.restore(); }
  });

});
