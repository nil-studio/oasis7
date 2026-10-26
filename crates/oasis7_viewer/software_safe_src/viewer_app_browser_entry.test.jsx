import { bindLocalTestAgent, bindFirstSnapshotAgentForTest, bindUnboundLocalPlayerForTest } from "./test_support/viewer_player_auth_fixture.js";
import { screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, vi, it } from "vitest";
import { buildTaskGame076ScenarioSnapshot } from "./gameplay_attraction_scenario.js";
vi.mock("./pixel_world_host.jsx", () => ({ PixelWorldHost: () => <div /> }));
const entryFixture = vi.hoisted(() => ({ config: undefined, prepare: vi.fn(), inspect: vi.fn(), session: vi.fn(), model: vi.fn() }));
vi.mock("./viewer_runtime_config_module.js", async (original) => {
  const module = await original();
  return { ...module, viewerRuntimeConfig: () => ({ ...module.viewerRuntimeConfig(), browserGameClient: entryFixture.config }) };
});
vi.mock("./browser_game_protocol.js", () => ({ loadBrowserGameProtocol: async () => ({}) }));
vi.mock("./browser_owner_entry.js", () => ({ createBrowserOwnerEntry: () => ({ inspect: entryFixture.inspect, prepare: entryFixture.prepare }) }));
vi.mock("./browser_game_session.js", () => ({ createBrowserGameSession: entryFixture.session }));
vi.mock("./browser_model_provider.js", async (original) => ({ ...await original(), probeBrowserModel: entryFixture.model, callBrowserModel: entryFixture.model }));
function viewerUrl() { return "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en"; }
let activeCleanup = null;
const TEST_ED25519_PKCS8_PREFIX = new Uint8Array([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
  0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);
function createTestCrypto() {
  const privateBytes = new Uint8Array(32).fill(7);
  const publicBytes = new Uint8Array(32).fill(9);
  return {
    getRandomValues(bytes) { return bytes.fill(42); },
    subtle: {
      async generateKey() {
        return {
          privateKey: { kind: "test-ed25519-private" },
          publicKey: { kind: "test-ed25519-public" },
        };
      },
      async exportKey(format) {
        if (format === "pkcs8") {
          const out = new Uint8Array(TEST_ED25519_PKCS8_PREFIX.length + privateBytes.length);
          out.set(TEST_ED25519_PKCS8_PREFIX, 0);
          out.set(privateBytes, TEST_ED25519_PKCS8_PREFIX.length);
          return out.buffer;
        }
        if (format === "raw") {
          return publicBytes.buffer.slice(0);
        }
        throw new Error(`unsupported test key export: ${format}`);
      },
      async verify() { return true; },
      async importKey() {
        return { kind: "test-ed25519-imported" };
      },
      async sign() {
        return new Uint8Array(64).fill(11).buffer;
      },
    },
  };
}
function sampleSnapshot(overrides = {}) {
  const base = buildTaskGame076ScenarioSnapshot();
  return {
    ...base,
    ...overrides,
    config: {
      ...base.config,
      ...(overrides.config || {}),
    },
    model: {
      ...base.model,
      agent_player_bindings: {
        "agent-0": "local-test-player-bound",
      },
      agent_player_public_key_bindings: {
        "agent-0": "abcdef0123456789abcdef0123456789".repeat(2),
      },
      ...(overrides.model || {}),
    },
    player_gameplay: {
      ...base.player_gameplay,
      ...(overrides.player_gameplay || {}),
    },
  };
}
async function renderViewerApp({
  snapshot = sampleSnapshot(),
  selection = null,
  search = viewerUrl(),
  setupCore = null,
  setupAfterMount = null,
  autoBindSnapshotAgent = true,
  starterOcOnboardingComplete = true,
} = {}) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", search);
  window.localStorage.clear();
  document.body.innerHTML = "";
  const core = await import("./legacy_core.js");
  const main = await import("./main.jsx");
  const { mountViewerApp } = main;
  const appRoot = document.createElement("div");
  appRoot.id = "app";
  document.body.appendChild(appRoot);
  await core.initializeSoftwareSafeCore();
  core.setViewerLocale("en");
  if (snapshot) {
    core.injectSnapshot(snapshot);
    if (autoBindSnapshotAgent) {
      bindFirstSnapshotAgentForTest(core, snapshot);
    }
  }
  if (selection) {
    core.applySelection(selection);
  }
  if (setupCore) {
    setupCore(core);
  }
  if (autoBindSnapshotAgent) {
    bindFirstSnapshotAgentForTest(core, core.state.snapshot);
  }
  if (setupAfterMount) {
    setupAfterMount(core);
  }
  if (starterOcOnboardingComplete) {
    main.__markStarterOcOnboardingCompleteForTest(core.state.auth.boundAgentId);
  }
  if (/^[0-9a-f]{64}$/i.test(core.state.auth.privateKey || "")) {
    await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
  }
  const dispose = mountViewerApp(appRoot);
  const cleanup = () => {
    dispose();
    if (activeCleanup === cleanup) {
      activeCleanup = null;
    }
  };
  activeCleanup = cleanup;
  return {
    core,
    cleanup,
    container: appRoot,
  };
}
beforeEach(() => {
  vi.restoreAllMocks();
  // Legacy fixtures explicitly advertise v2 absence; outages must never permit fallback.
  vi.spyOn(globalThis, "fetch").mockResolvedValue({ ok: false, status: 404, json: async () => ({ ok: false, error_code: "not_found" }) });
  window.history.replaceState({}, "", viewerUrl());
  window.localStorage.clear();
  Object.defineProperty(window, "crypto", {
    configurable: true,
    value: createTestCrypto(),
  });
  document.body.innerHTML = "";
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  activeCleanup?.();
  activeCleanup = null;
  document.body.innerHTML = "";
});

describe("CommandSurface browser owner entry", () => {
  afterEach(() => { entryFixture.config = undefined; });
  async function mountEntry(enabled = true, selected = false, projection = null) {
    entryFixture.config = enabled ? { servicePublicKey: "test-service" } : undefined;
    entryFixture.inspect.mockReset().mockResolvedValue({ state: "needs_participant" });
    entryFixture.prepare.mockReset(); entryFixture.session.mockReset(); entryFixture.model.mockReset();
    return renderViewerApp({ autoBindSnapshotAgent: false,
      snapshot: selected ? sampleSnapshot() : { ...sampleSnapshot(), model: { ...sampleSnapshot().model, agents: {}, agent_player_bindings: {} } },
      selection: selected ? { kind: "agent", id: "agent-0" } : null,
      setupCore(core) {
        if (selected) bindLocalTestAgent(core); else bindUnboundLocalPlayerForTest(core);
        core.state.auth.hostedAccountId = "first-entry-account";
        core.state.auth.registrationStatus = selected ? "registered" : "registered_unbound";
        core.state.auth.runtimeStatus = selected ? "registered" : "registered_unbound";
        if (projection) {
          // A read-only pixel-world selection may precede the runtime projection.
          core.state.selectedKind = projection;
          core.state.selectedId = "participant-readonly";
          core.state.selectedObject = { id: "participant-readonly", entity_id: "participant-readonly", module_id: "test.module" };
        }
      },
    });
  }
  it("exposes Prepare to an installed owner without a selected or controllable Agent", async () => {
    const { core } = await mountEntry();
    expect(core.state.auth.boundAgentId).toBeNull();
    expect(document.querySelector("[data-command-agent]")).toBeNull();
    expect((await import("./viewer_auth_session_module.js")).authSigningIdentity(core.state.auth)).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Prepare player, Agent and Task" })).toBeVisible();
    await waitFor(() => expect(entryFixture.inspect).toHaveBeenCalledTimes(1));
    expect(entryFixture.prepare).not.toHaveBeenCalled(); expect(entryFixture.session).not.toHaveBeenCalled(); expect(entryFixture.model).not.toHaveBeenCalled();
  }, 60000);
  it.each(["agent", "module_visual"])("keeps owner entry visible with a read-only %s projection selected", async (kind) => {
    const { core } = await mountEntry(true, false, kind);
    expect(core.state.selectedKind).toBe(kind);
    expect(Object.keys(core.state.snapshot.model.agents)).toHaveLength(0);
    expect(core.state.auth.boundAgentId).toBeNull();
    if (kind === "agent") expect(core.isAgentVisibleToCurrentSession(core.state.selectedId)).toBe(false);
    expect(document.querySelector("[data-command-agent]")).toBeNull();
    expect(await screen.findAllByRole("button", { name: "Prepare player, Agent and Task" })).toHaveLength(1);
    const entrySurface = screen.getByRole("region", { name: "Browser BYOK Agent" });
    core.applySelection({ kind: "location", id: "loc-0" });
    expect(screen.getByRole("region", { name: "Browser BYOK Agent" })).toBe(entrySurface);
    await waitFor(() => expect(entryFixture.inspect).toHaveBeenCalledTimes(1));
    expect(entryFixture.prepare).not.toHaveBeenCalled(); expect(entryFixture.session).not.toHaveBeenCalled(); expect(entryFixture.model).not.toHaveBeenCalled();
  }, 60000);
  it("does not mount entry when deployment configuration is off", async () => {
    await mountEntry(false, true);
    expect(screen.queryByRole("region", { name: "Browser BYOK Agent" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Prepare player, Agent and Task" })).toBeNull();
    expect(entryFixture.inspect).not.toHaveBeenCalled(); expect(entryFixture.prepare).not.toHaveBeenCalled();
  }, 60000);
  it("mounts one entry with an existing Agent and performs no automatic writes", async () => {
    await mountEntry(true, true);
    expect(await screen.findAllByRole("button", { name: "Prepare player, Agent and Task" })).toHaveLength(1);
    await waitFor(() => expect(entryFixture.inspect).toHaveBeenCalledTimes(1));
    expect(entryFixture.prepare).not.toHaveBeenCalled(); expect(entryFixture.session).not.toHaveBeenCalled(); expect(entryFixture.model).not.toHaveBeenCalled();
  }, 60000);
});
