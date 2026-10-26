import { fireEvent, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildTaskGame076ScenarioSnapshot } from "./gameplay_attraction_scenario.js";

vi.mock("./pixel_world_host.jsx", () => ({
  PixelWorldHost: (props) => (
    <div data-testid="pixel-world-host">
      {`pixel-world-host:${typeof props.locale === "function" ? props.locale() : props.locale}`}
    </div>
  ),
}));

function viewerUrl() {
  return "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en";
}

let activeCleanup = null;
const HEAVY_UI_TEST_TIMEOUT_MS = 60000;
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
        "agent-0": "abcdef0123456789abcdef0123456789",
      },
      ...(overrides.model || {}),
    },
    player_gameplay: {
      ...base.player_gameplay,
      ...(overrides.player_gameplay || {}),
    },
  };
}

function bindLocalTestAgent(core, agentId = "agent-0", playerId = "local-test-player-bound") {
  core.state.auth = {
    ...core.state.auth,
    available: true,
    playerId,
    publicKey: "abcdef0123456789abcdef0123456789",
    privateKey: "private-key-must-stay-hidden",
    source: "local_test_api_ephemeral",
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: agentId,
  };
}

function bindFirstSnapshotAgentForTest(core, snapshot) {
  const agentId = Object.keys(snapshot?.model?.agents || {})[0];
  const playerId = snapshot?.model?.agent_player_bindings?.[agentId];
  if (!agentId || !playerId) {
    return;
  }
  bindLocalTestAgent(core, agentId, playerId);
}

async function renderViewerApp({
  snapshot = sampleSnapshot(),
  selection = null,
  search = viewerUrl(),
  setupAfterMount = null,
} = {}) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", search);
  window.localStorage.clear();
  document.body.innerHTML = "";

  const core = await import("./legacy_core.js");
  const main = await import("./main.jsx");
  const appRoot = document.createElement("div");
  appRoot.id = "app";
  document.body.appendChild(appRoot);

  await core.initializeSoftwareSafeCore();
  core.setViewerLocale("en");
  if (snapshot) {
    core.injectSnapshot(snapshot);
    bindFirstSnapshotAgentForTest(core, snapshot);
  }
  if (selection) {
    core.applySelection(selection);
  }
  if (snapshot) {
    bindFirstSnapshotAgentForTest(core, core.state.snapshot);
  }
  if (setupAfterMount) {
    setupAfterMount(core);
  }
  main.__markStarterOcOnboardingCompleteForTest(core.state.auth.boundAgentId);

  const dispose = main.mountViewerApp(appRoot);
  const cleanup = () => {
    dispose();
    if (activeCleanup === cleanup) {
      activeCleanup = null;
    }
  };
  activeCleanup = cleanup;
  return { core, container: appRoot };
}

beforeEach(() => {
  vi.restoreAllMocks();
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
  vi.unstubAllGlobals();
  vi.useRealTimers();
  activeCleanup?.();
  activeCleanup = null;
  document.body.innerHTML = "";
});

describe("focused viewer UI contracts", () => {
  it.each(["available", "blocked"])("renders canonical recipe %s without inheriting legacy recovery or sponsorship", async (status) => {
    const { container } = await renderViewerApp({snapshot: sampleSnapshot({player_gameplay: {
      ...sampleSnapshot().player_gameplay,
      major_power_dependency_status: "forced",
      repair_available: true,
      recovery_path_detail: "legacy repair is available after a snapshot refresh",
      gameplay_capability: {goal_id: "current_industrial_capability", recovery: [
        {kind: "current_recipe_run", status,
          reason_code: status === "available" ? "canonical_recipe_quote_available" : "canonical_recipe_admission_rejected",
          action: {ScheduleRecipe: {factory_id: "factory-0"}}},
        {kind: "repair", status: "unknown", reason_code: "unsupported_recovery_action", action: null},
      ]},
    }})});
    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getAllByText(new RegExp(`Run the current recipe: ${status}`)).length).toBeGreaterThan(0);
    expect(within(stagePanel).getAllByText(/Repair: unknown/).length).toBeGreaterThan(0);
    expect(within(stagePanel).queryByText("forced")).not.toBeInTheDocument();
    expect(within(stagePanel).queryByText("legacy repair is available after a snapshot refresh")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps visible Prompt Overrides contained inside the Advanced disclosure", async () => {
    const { core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
      },
    });

    core.togglePromptOverridesVisible();
    await waitFor(() => {
      expect(screen.getByLabelText("System Prompt Override")).toBeInTheDocument();
    });

    const advancedDetails = screen.getByText("Advanced Prompt Settings").closest("details");
    expect(advancedDetails).toBeTruthy();
    expect(advancedDetails).toContainElement(screen.getByLabelText("System Prompt Override"));
    expect(advancedDetails).toContainElement(screen.getByRole("button", { name: "Preview Prompt" }));
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("gates prompt actions on the negotiated capability and current epochs", async () => {
    const { core, container } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
        core.state.viewerProtocol = {
          negotiated: true,
          version: 2,
          capabilities: ["prompt_control_result_v1"],
          authorityEpoch: "authority-test-1",
        };
      },
    });

    core.togglePromptOverridesVisible();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Preview Prompt" })).toBeDisabled();
      expect(container.querySelector('[data-prompt-readiness="blocked"]')).toHaveTextContent("prompt control is waiting for a current registered player session");
    });

    core.state.auth.sessionEpoch = 12;
    core.state.auth.bindingEpoch = 9;
    core.state.auth.authorityEpoch = "authority-test-1";
    core.requestRender();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Preview Prompt" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "Apply Prompt" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "Rollback Prompt" })).toBeEnabled();
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("fails chat explicitly when the current world authority is unavailable", async () => {
    const { core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
        core.state.worldFeed = {
          ...core.state.worldFeed,
          status: "unavailable",
          stale: true,
          unavailableReason: "source_unavailable",
        };
      },
    });

    expect(core.sendAgentChat("agent-0", "hello while world authority is unavailable"))
      .toEqual(expect.objectContaining({ ok: true }));
    await waitFor(() => {
      expect(core.state.lastChatFeedback.stage).toBe("error");
      expect(core.state.lastChatFeedback.accepted).toBe(false);
      expect(core.state.lastChatFeedback.reason)
        .toBe("Error: agent_chat requires the current runtime world authority");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("shows safe control-loss recovery for a stale chat refusal", async () => {
    const { core, container } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
        core.state.auth.controlLostAgentId = "agent-0";
        core.state.auth.runtimeStatus = "control_lost";
        core.state.auth.sessionEpoch = null;
        core.state.auth.bindingEpoch = null;
        core.state.lastChatFeedback = {
          id: "chat-control-lost",
          kind: "chat",
          action: "agent_chat",
          agentId: "agent-0",
          accepted: false,
          ok: false,
          stage: "error",
          reason: "control_lost",
          response: {
            status: "blocked",
            value_visibility: "hidden",
            reason_code: "control_lost",
            next_step: "reauthenticate_and_refresh_binding",
            message: "Agent control was lost; re-authenticate and refresh the current binding before retrying.",
            binding_epoch: 99,
            provider_trace: "trace-secret",
          },
        };
      },
    });

    await waitFor(() => {
      expect(screen.getByTestId("control-loss-recovery")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Send Chat" })).toBeDisabled();
    });
    expect(container).toHaveTextContent("Control lost");
    expect(container).toHaveTextContent("Re-authenticate and refresh the current binding");
    expect(container).not.toHaveTextContent("trace-secret");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps hidden prompt-control metadata out of the result card and offers binding recovery", async () => {
    const { core, container } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
        core.state.viewerProtocol = {
          negotiated: true,
          version: 2,
          capabilities: ["prompt_control_result_v1"],
          authorityEpoch: "authority-test-1",
        };
        core.state.auth.sessionEpoch = 12;
        core.state.auth.bindingEpoch = 9;
        core.state.auth.authorityEpoch = "authority-test-1";
        core.state.lastPromptFeedback = {
          id: "hidden-result",
          kind: "prompt",
          action: "prompt_control_apply",
          agentId: "agent-0",
          accepted: false,
          ok: false,
          stage: "blocked",
          response: {
            status: "blocked",
            value_visibility: "hidden",
            reason_code: "control_lost",
            next_step: "reauthenticate_and_refresh_binding",
            player_id: "player-secret",
            binding_epoch: 12,
            version: 9,
            digest: "digest-secret",
            system_prompt: "latest secret prompt",
          },
        };
      },
    });

    core.togglePromptOverridesVisible();
    await waitFor(() => {
      expect(screen.getByTestId("prompt-recovery-cta")).toBeInTheDocument();
    });
    const advancedDetails = screen.getByText("Advanced Prompt Settings").closest("details");
    expect(advancedDetails).toHaveTextContent("Re-authenticate and refresh the current binding");
    expect(advancedDetails).not.toHaveTextContent("player-secret");
    expect(advancedDetails).not.toHaveTextContent("digest-secret");
    expect(advancedDetails).not.toHaveTextContent("latest secret prompt");
    expect(container.querySelector('[data-feedback-kind="prompt"][data-prompt-value-visibility="hidden"]')).toBeTruthy();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps email-free test login hidden by default and consumes an issuer grant when opted in", async () => {
    const { core } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&hosted_test_login=1`,
      setupAfterMount(core) {
        core.state.hostedAccess = { deployment_mode: "hosted_public_join", action_matrix: [] };
        core.state.auth.available = false;
        vi.stubGlobal("fetch", vi.fn(async (route, options) => {
          expect(route).toBe("/api/public/hosted-account/test-login");
          expect(JSON.parse(options.body)).toEqual({
            public_key: "0909090909090909090909090909090909090909090909090909090909090909",
          });
          return {
            ok: true,
            async json() {
              return {
                ok: true,
                deployment_mode: "hosted_public_join",
                grant: {
                  player_id: "hosted-player-test-login",
                  device_session_id: "hosted-device-session-test-login",
                  issued_at_unix_ms: 123,
                  auth_mode: "browser_local_ephemeral_ed25519",
                  release_token: "a".repeat(64),
                  registration_grant: "issuer-signed-registration-grant",
                },
              };
            },
          };
        }));
      },
    });

    expect(screen.queryByRole("button", { name: /email-free test login/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /email-free test login/i }));
    await waitFor(() => {
      expect(core.state.auth.available).toBe(true);
      expect(core.state.auth.source).toBe("hosted_test_login");
      expect(core.state.auth.playerId).toBe("hosted-player-test-login");
      expect(core.state.auth.registrationGrant).toBeUndefined();
      expect(window.localStorage.getItem(`oasis7_hosted_player_session:${core.state.wsUrl}`) || "").not.toContain("privateKey");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not expose the email-free test login without its explicit URL opt-in", async () => {
    await renderViewerApp({
      snapshot: null,
      setupAfterMount(core) {
        core.state.hostedAccess = { deployment_mode: "hosted_public_join", action_matrix: [] };
        core.state.auth.available = false;
      },
    });
    expect(screen.queryByRole("button", { name: /email-free test login/i })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses readable entity identity and semantic selection state in Targets", async () => {
    const base = sampleSnapshot();
    const snapshot = sampleSnapshot({
      model: {
        ...base.model,
        agents: {
          ...base.model.agents,
          "agent-0": {
            ...base.model.agents["agent-0"],
            name: "Surveyor Seven",
          },
        },
        locations: {
          ...base.model.locations,
          "loc-0": {
            ...base.model.locations["loc-0"],
            name: "Assembly Nexus",
          },
        },
      },
    });
    const { container } = await renderViewerApp({
      snapshot,
      selection: { kind: "agent", id: "agent-0" },
    });

    const targetsPanel = container.querySelector("#viewer-targets-panel");
    const agentButton = within(targetsPanel).getByTestId("viewer-playthrough-select-agent");
    const locationButton = within(targetsPanel).getByTestId("viewer-select-location-loc-0");
    expect(within(agentButton).getByText("Surveyor Seven")).toBeInTheDocument();
    expect(within(locationButton).getByText("Assembly Nexus")).toBeInTheDocument();
    expect(agentButton).toHaveAttribute("aria-pressed", "true");
    expect(locationButton).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(locationButton);
    await waitFor(() => {
      expect(locationButton).toHaveAttribute("aria-pressed", "true");
      expect(agentButton).toHaveAttribute("aria-pressed", "false");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps the selected Agent identity in the narrow Command context", async () => {
    const base = sampleSnapshot();
    const snapshot = sampleSnapshot({
      model: {
        ...base.model,
        agents: {
          ...base.model.agents,
          "agent-0": { ...base.model.agents["agent-0"], name: "Surveyor Seven" },
        },
      },
    });
    const { container } = await renderViewerApp({
      snapshot,
      selection: { kind: "agent", id: "agent-0" },
    });

    const commandSurface = container.querySelector("#viewer-details-panel .command-surface");
    const context = commandSurface.querySelector(".command-surface__target-row");
    expect(context).toHaveTextContent("Surveyor Seven");
    expect(context).toHaveTextContent(/Chat Ready|Chat Limited/);
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
