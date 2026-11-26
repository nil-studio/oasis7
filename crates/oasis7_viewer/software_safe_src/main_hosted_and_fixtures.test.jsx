import { fireEvent, screen, waitFor, within } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import {
  HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
} from "./software_safe_constants.js";
vi.mock("./pixel_world_host.jsx", () => ({
  PixelWorldHost: (props) => (
    <div data-testid="pixel-world-host">
      {`pixel-world-host:${typeof props.locale === "function" ? props.locale() : props.locale}`}
    </div>
  ),
}));
import {
  HEAVY_UI_TEST_TIMEOUT_MS,
  clearActiveCleanup,
  createTestCrypto,
  elementPrecedes,
  installMainTestLifecycleHooks,
  installMockWebSocket,
  renderViewerApp,
  renderViewerAppThroughAutoMount,
  sampleHostedPublicJoinAccess,
  sampleSnapshot,
  setActiveCleanup,
  setupConnectedSemanticCore,
  viewerUrl,
} from "./test_support/main_test_harness.jsx";
installMainTestLifecycleHooks();

describe("viewer hosted auth gate and visual fixtures", () => {
  it("shows a cookie-authenticated account waiting for world entry without granting control", async () => {
    let account = { account_id: "synthetic-cookie-account", display_name: "Synthetic Cookie Player" };
    globalThis.fetch.mockImplementation(async url => {
      const result = String(url).endsWith("/capabilities") ? { ok: true, csrf_token: "synthetic-csrf", passkey: { available: true } }
        : String(url).endsWith("/session") ? { ok: true, account }
        : String(url).endsWith("/credentials") ? { ok: true, credentials: [] }
        : String(url).endsWith("/sessions") ? { ok: true, sessions: [] }
        : String(url).endsWith("/logout") ? (account = null, { ok: true })
        : { ok: true };
      return { ok: true, status: 200, json: async () => result };
    });
    const { core } = await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = { available: false, playerId: null, publicKey: null, boundAgentId: null, source: "guest_only", registrationStatus: "guest", runtimeStatus: "guest" };
      },
    });
    const dialog = await screen.findByRole("dialog", { name: "Enter the World to Play" });
    expect(within(dialog).getByText("Signed in · world entry pending")).toBeInTheDocument();
    expect(within(dialog).queryByText("Not signed in")).not.toBeInTheDocument();
    expect(core.state.auth.available).toBe(false);
    expect(core.state.auth.boundAgentId).toBeNull();
    expect(globalThis.fetch.mock.calls.some(([url]) => String(url).endsWith("/world/join"))).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "退出登录" }));
    expect(await screen.findByRole("dialog", { name: "Sign In to Play" })).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("surfaces hosted recovery and preview strong-auth truth without not-implemented drift", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth.error = "session_revoked";
        core.state.auth.revokeReason = "qa-kick";
        core.state.auth.revokedBy = "qa";
        core.state.hostedLogin.handle = "player@example.com";
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(await screen.findByRole("dialog", { name: "Sign In to Play" })).toBeInTheDocument();
    expect((await screen.findAllByRole("button", { name: "Request Login Code" })).length).toBeGreaterThan(0);
    expect(screen.getAllByLabelText("Email").length).toBeGreaterThan(0);
    expect(screen.getByText("upgrade_after_player_session")).toBeInTheDocument();
    expect(
      screen.getAllByText(
        "The runtime or operator revoked this browser session by qa. Reason: qa-kick. You need to re-login to the hosted account and acquire a fresh player session before gameplay, chat, or prompt actions can continue.",
      ).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText("not_implemented")).not.toBeInTheDocument();
    expect(screen.queryByText(/not implemented yet/i)).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not show the hosted login gate after player session registration", async () => {
    await renderViewerApp({
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "hosted-player-1",
          publicKey: "oc:pk:test-player",
          privateKey: "ed25519-secret",
          releaseToken: "hosted-release-1",
          source: "hosted_browser_storage",
          registrationStatus: "registered",
          runtimeStatus: "registered",
        };
      },
    });

    expect(screen.queryByRole("dialog", { name: "Sign In to Play" })).not.toBeInTheDocument();
  });

  it("surfaces hosted login retry-after guidance when OTP resend is throttled", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.hostedLogin.handle = "player@example.com";
        core.state.hostedLogin.error = "a login code was just sent for this email; retry in 21 seconds (retry in 21s)";
        core.state.hostedLogin.retryAfterSeconds = 21;
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(
      (await screen.findAllByText("a login code was just sent for this email; retry in 21 seconds (retry in 21s)")).length,
    ).toBeGreaterThan(0);
    expect((await screen.findAllByText("retry_after=21s")).length).toBeGreaterThan(0);
  });

  it("clears hosted login retry guidance immediately when the player edits the email", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.hostedLogin.handle = "player@example.com";
        core.state.hostedLogin.error = "a login code was just sent for this email; retry in 21 seconds (retry in 21s)";
        core.state.hostedLogin.retryAfterSeconds = 21;
      },
    });

    expect((await screen.findAllByText("retry_after=21s")).length).toBeGreaterThan(0);

    fireEvent.input(document.getElementById("gate-hosted-login-handle"), {
      target: { value: "next-player@example.com" },
    });

    expect(screen.queryByText("retry_after=21s")).not.toBeInTheDocument();
    expect(screen.queryByText(/retry in 21 seconds/i)).not.toBeInTheDocument();
  });

  it("keeps keyboard focus inside the hosted login gate while it is modal", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.hostedLogin.handle = "player@example.com";
      },
    });

    const dialog = await screen.findByRole("dialog", { name: "Sign In to Play" });
    const emailInput = await within(dialog).findByLabelText("Email");
    const requestButton = await within(dialog).findByRole("button", { name: "Request Login Code" });

    await waitFor(() => {
      expect(document.activeElement).toBe(emailInput);
    });

    requestButton.focus();
    const tabEvent = new KeyboardEvent("keydown", {
      key: "Tab",
      bubbles: true,
      cancelable: true,
    });
    dialog.dispatchEvent(tabEvent);

    expect(tabEvent.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(emailInput);
  });

  it("marks hosted backend reauth as available once a browser player session is registered", async () => {
    await renderViewerApp({
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "hosted-player-1",
          publicKey: "oc:pk:test-player",
          privateKey: "ed25519-secret",
          releaseToken: "hosted-release-1",
          source: "hosted_browser_storage",
          registrationStatus: "registered",
          runtimeStatus: "registered",
        };
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(screen.getAllByRole("button", { name: "Release Player Session" }).length).toBeGreaterThan(0);
    expect(screen.getByText("active_hosted_session")).toBeInTheDocument();
    expect(screen.getByText("preview_backend_reauth_available")).toBeInTheDocument();
    expect(
      screen.getByText(
        "hosted preview backend reauth is available after the browser device-session-backed player_session has completed runtime registration for prompt_control",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("page reload will reuse the hosted device session, mint a fresh browser session key, and attempt reconnect_sync first"),
    ).toBeInTheDocument();
    const assetLane = screen.getByText("Asset / Governance Lane").closest("section");
    expect(assetLane).toBeTruthy();
    expect(
      within(assetLane).getAllByText(/main_token_transfer remains blocked until a higher-trust hosted strong-auth lane exists/i).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText("not_implemented")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps hosted backend reauth pending until runtime registration finishes", async () => {
    await renderViewerApp({
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "hosted-player-2",
          publicKey: "oc:pk:test-player-2",
          privateKey: "ed25519-secret-2",
          releaseToken: "hosted-release-2",
          source: "hosted_browser_storage",
          registrationStatus: "issued",
          runtimeStatus: "issued",
        };
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(screen.getAllByText("issued_pending_register").length).toBeGreaterThan(0);
    expect(screen.queryByText("preview_backend_reauth_available")).not.toBeInTheDocument();
    expect(
      screen.getByText(
        "hosted preview backend reauth stays pending until the browser device-session-backed player_session finishes runtime registration",
      ),
    ).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("installs visual fixture states for screenshot parity without production semantics", async () => {
    const states = [
      "shell_selected_blocker",
      "agent_chat_history",
      "gameplay_diagnostics_expanded",
      "factory_production_failure_disposition",
      "hosted_login_gate",
      "empty_world_recovery",
      "major_world_event_crisis",
    ];

    for (const fixtureName of states) {
      const { cleanup, container } = await renderViewerApp({
        snapshot: null,
        search: `${viewerUrl()}&viewer_visual_fixture=${fixtureName}`,
      });

      expect(window.__OASIS7_VIEWER_VISUAL_FIXTURES__).toBeTruthy();
      expect(container).toHaveAttribute("data-viewer-visual-fixture", fixtureName);
      expect(document.body).toHaveAttribute("data-viewer-visual-fixture", fixtureName);

      if (fixtureName === "factory_production_failure_disposition") {
        const card = within(container).getByTestId("viewer-factory-production-failure-disposition");
        expect(within(card).getByText("Production result failed validation")).toBeInTheDocument();
        expect(card).toHaveAttribute("role", "status");
        expect(card).toHaveAttribute("aria-live", "polite");
        expect(card).toHaveTextContent("Factory: factory.target · Recipe: recipe.smelter.iron_ingot");
        expect(card).toHaveTextContent("Consumed inputs");
        expect(card).toHaveTextContent("iron_ore × 3");
        expect(card).toHaveTextContent("Consumed power");
        expect(card).toHaveTextContent("Refresh gameplay snapshot");
        expect(card).toHaveTextContent("Recovery action: request_snapshot");
        expect(card).toHaveTextContent("Next recheck: next committed snapshot");
        expect(card.textContent).not.toMatch(
          /product_validation_rejected|consumed_lost|inspect_product_validation_and_reschedule/,
        );
        await waitFor(() => expect(container.querySelector("#viewer-gameplay-details")).toHaveAttribute("open"));
        expect(elementPrecedes(
          card,
          within(container.querySelector("#viewer-gameplay-details")).getByText("Control Proof").closest(".event-card"),
        )).toBe(true);
      }

      cleanup();
    }
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps visual fixture query parameters inert without test_api", async () => {
    const { container } = await renderViewerAppThroughAutoMount({
      snapshot: null,
      search: "/software_safe.html?connect=0&hosted_bootstrap=0&locale=en&viewer_visual_fixture=gameplay_diagnostics_expanded",
    });

    expect(window.__OASIS7_VIEWER_VISUAL_FIXTURES__).toBeUndefined();
    expect(container).not.toHaveAttribute("data-viewer-visual-fixture");
    expect(document.body).not.toHaveAttribute("data-viewer-visual-fixture");
    expect(container.querySelector("#viewer-gameplay-details")).not.toHaveAttribute("open");
    expect(container.querySelector("#viewer-gameplay-details")).toHaveProperty("open", false);
    expect(container.querySelector("#viewer-diagnostics-panel")).not.toHaveAttribute("open");
    expect(elementPrecedes(
      container.querySelector(".stage-hero"),
      container.querySelector("#viewer-gameplay-details"),
    )).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders the agent chat fixture with history and collapsed prompt controls", async () => {
    const { container } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=agent_chat_history`,
    });

    const detailsPanel = container.querySelector("#viewer-details-panel");
    const commandSurface = detailsPanel.querySelector(".command-surface");
    expect(commandSurface).toHaveAttribute("data-command-agent", "agent-0");
    expect(commandSurface).toHaveAttribute("data-command-chat-history", "3");
    const diagnosticStrip = commandSurface.querySelector(".command-surface__diagnostic-strip");
    expect(diagnosticStrip).toBeTruthy();
    expect(within(diagnosticStrip).getByText("prompt=enabled")).toHaveClass("badge--good");
    expect(within(diagnosticStrip).getByText("chat=enabled")).toHaveClass("badge--good");
    expect(within(diagnosticStrip).getByText(/mainToken=/)).toHaveClass("badge--good");
    expect(commandSurface.querySelector(".command-surface__chat-panel")).toBeTruthy();
    expect(elementPrecedes(
      commandSurface.querySelector(".command-surface__chat-panel"),
      commandSurface.querySelector(".command-surface__advanced-panel"),
    )).toBe(true);
    expect(within(detailsPanel).getByText("Chat Ready")).toBeInTheDocument();
    expect(within(detailsPanel).getByText("Awaiting material recovery before the smelter can proceed.")).toBeInTheDocument();
    expect(within(detailsPanel).getByText("Hold position and confirm the blocker.")).toBeInTheDocument();
    expect(commandSurface.querySelectorAll(".event-card--chat-player").length).toBeGreaterThan(0);
    expect(commandSurface.querySelectorAll(".event-card--chat-agent").length).toBeGreaterThan(0);
    expect(within(detailsPanel).getByText("state=hidden_by_default")).toBeInTheDocument();
    expect(within(detailsPanel).queryByLabelText("System Prompt Override")).not.toBeInTheDocument();
    expect(within(detailsPanel).getAllByText(/no transfer form/i).length).toBeGreaterThan(0);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("opens gameplay details and diagnostics for the diagnostics visual fixture", async () => {
    const { container } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=gameplay_diagnostics_expanded`,
    });

    await waitFor(() => {
      expect(container.querySelector("#viewer-gameplay-details")).toHaveAttribute("open");
      expect(container.querySelector("#viewer-diagnostics-panel")).toHaveAttribute("open");
    });
    expect(within(container.querySelector("#viewer-gameplay-details")).getByText("Formal Gameplay Summary")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-gameplay-details")).getByText("Capability Economics")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-diagnostics-panel")).getByText("Execution Lanes")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-diagnostics-panel")).getByText("state sync")).toBeInTheDocument();
    expect(elementPrecedes(
      container.querySelector("#viewer-gameplay-details"),
      container.querySelector(".stage-hero"),
    )).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("forces a hosted login gate fixture with reserved retry/error guidance", async () => {
    await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=hosted_login_gate`,
    });

    const dialog = await screen.findByRole("dialog", { name: "Sign In to Play" });
    expect(dialog).toHaveAttribute("data-viewer-fixture-state", "hosted_login_gate");
    expect(await within(dialog).findByDisplayValue("player@example.com")).toBeInTheDocument();
    expect(within(dialog).getByText("challenge=fixture-challenge")).toBeInTheDocument();
    expect(within(dialog).getByText("Enter the latest verification code to continue.")).toBeInTheDocument();
    expect(within(dialog).getByText("retry_after=18s")).toBeInTheDocument();
    expect(screen.queryByText(/wallet/i)).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders empty-world recovery as intentional recovery instead of selected-agent acceptance", async () => {
    const { container } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=empty_world_recovery`,
    });

    const state = window.__AW_TEST__.getState();
    expect(state.selectedKind).toBe(null);
    expect(state.selectedId).toBe(null);
    expect(state.gameplaySummary.blockerKind).toBe("runtime_snapshot_empty_entities");
    expect(within(container.querySelector("#viewer-targets-panel")).getByText("No agents in current snapshot.")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-stage-panel")).getByText("Recover World Snapshot")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-stage-panel")).getAllByText("No published entities yet").length).toBeGreaterThan(0);
    expect(within(container.querySelector("#viewer-stage-panel")).getAllByText(/Request a fresh snapshot|Reload the authoritative snapshot|first published entity/i).length).toBeGreaterThan(0);
    expect(within(container.querySelector("#viewer-stage-panel")).getAllByText("Request snapshot").length).toBeGreaterThan(0);
    expect(within(container.querySelector("#viewer-details-panel")).getByText("Claim Your First Agent")).toBeInTheDocument();
    expect(container.querySelector("[data-callout-kind='empty_world_recovery']")).toBeTruthy();
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
