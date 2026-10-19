import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { WorldFeedSurface } from "./world_feed_integration.jsx";
import {
  HEAVY_UI_TEST_TIMEOUT_MS,
  buildTestRustRenderState,
  cleanupMountedHost,
  renderPixelWorldHost,
  runtimeMock,
  sampleSnapshot,
} from "./pixel_world_host_test_support.jsx";

describe("pixel world host production refresh", () => {
  it("clears hit targets after ready context loss and retries only on explicit request", async () => {
    runtimeMock.deriveRenderState = vi.fn(buildTestRustRenderState);
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?connect=0&locale=en", "en", {injectionSearch:"?test_api=1&connect=0&locale=en"});
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("ready"));
    const canvas = document.getElementById("pixel-world-embedded-runtime-canvas");
    const mounted = runtimeMock.mountCalls;
    fireEvent(canvas, new Event("webglcontextlost", {cancelable:true}));
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable"));
    expect(document.querySelector(".pixel-world-canvas__selection")).not.toBeInTheDocument();
    expect(runtimeMock.mountCalls).toBe(mounted);
    runtimeMock.mountError = new Error("still unavailable");
    fireEvent.click(screen.getByRole("button", {name:/retry renderer/i}));
    await waitFor(() => expect(runtimeMock.mountCalls).toBe(mounted + 1));
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable"));
    expect(document.querySelector(".pixel-world-action-receipt")).toBeInTheDocument();
    runtimeMock.mountError = null;
    fireEvent.click(screen.getByRole("button", {name:/retry renderer/i}));
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("ready"));
    expect(runtimeMock.mountCalls).toBe(mounted + 2);
  }, HEAVY_UI_TEST_TIMEOUT_MS);
  it("clears private graphics and retained text on account change and rejects late callbacks", async () => {
    runtimeMock.deriveRenderState = vi.fn(buildTestRustRenderState);
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?connect=0&locale=en", "en", {injectionSearch:"?test_api=1&connect=0&locale=en"});
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("ready"));
    core.state.lastGameplayActionFeedback = {kind:"gameplay_action",agentId:"private-agent-A",effect:"private owner A result",stage:"completed"};
    core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback);
    const oldEvents = runtimeMock.onEvent;
    core.state.auth.playerId = "different-player";
    core.requestRender();
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable"));
    expect(document.querySelector(".pixel-world-command-strip")).not.toBeInTheDocument();
    expect(document.querySelector(".pixel-world-action-receipt")).not.toBeInTheDocument();
    expect(core.state.lastGameplayActionFeedback).not.toBeNull();
    expect(core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback)).toBeNull();
    const selected = core.state.selectedId;
    oldEvents({type:"select_entity",selection:{kind:"agent",id:"late-private"}});
    expect(core.state.selectedId).toBe(selected);
    fireEvent.click(screen.getByRole("button", {name:/retry renderer/i}));
    expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable");
  }, HEAVY_UI_TEST_TIMEOUT_MS);
  it("retains authorized receipt text when Rust derivation fails during a ready update", async () => {
    runtimeMock.deriveRenderState = vi.fn(buildTestRustRenderState);
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?connect=0&locale=en", "en", {injectionSearch:"?test_api=1&connect=0&locale=en"});
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("ready"));
    runtimeMock.deriveRenderState.mockImplementation(() => {throw new Error("derivation interrupted");});
    core.state.snapshot.model.locations["loc-0"].pos.x_cm += 1;
    core.requestRender();
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable"));
    expect(document.querySelector(".pixel-world-action-receipt")).toBeInTheDocument();
    expect(document.querySelector(".pixel-world-canvas__selection")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);
  it.each([
    ["test_api=false", "?test_api=false&connect=0&locale=en"],
    ["test_api omitted", "?connect=0&locale=en"],
  ])("refreshes selection DTO and camera state for World Feed Locate with %s", async (_mode, productionSearch) => {
    const snapshot = sampleSnapshot();
    snapshot.model.module_visual_entities = {
      "module-relay": {
        entity_id: "module-relay",
        module_id: "module-7",
        kind: "relay",
        label: "Relay Seven",
        anchor: {
          type: "absolute",
          data: { pos: { x_cm: 7_100_000, y_cm: 1_200_000, z_cm: 80 } },
        },
      },
    };
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      moduleVisualEntities: [{
        id: "module-relay",
        moduleId: "module-7",
        kind: "relay",
        label: "Relay Seven",
        anchor: snapshot.model.module_visual_entities["module-relay"].anchor,
        pos: { x_cm: 7_100_000, y_cm: 1_200_000, z_cm: 80 },
      }],
      selection: input.selectedKind && input.selectedId
        ? { kind: input.selectedKind, id: input.selectedId }
        : { kind: "agent", id: "agent-0" },
    }));

    const { core } = await renderPixelWorldHost(
      snapshot,
      productionSearch,
      "en",
      { injectionSearch: "?test_api=1&connect=0&locale=en" },
    );
    await waitFor(() => {
      expect(core.state.pixelWorldRuntimeStatus).toBe("ready");
      expect(document.querySelector(".pixel-world-canvas__selection")).toHaveTextContent("Agent 0");
    });
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(resolve))));

    const detailsPanel = document.createElement("section");
    detailsPanel.id = "viewer-details-panel";
    detailsPanel.tabIndex = -1;
    document.body.appendChild(detailsPanel);
    const appRenderHook = vi.fn();
    core.setRenderHook(appRenderHook);
    render(() => (
      <WorldFeedSurface
        core={core}
        locale={() => "en"}
        tr={(_locale, _zh, en) => en}
      />
    ));
    Object.assign(core.state.worldFeed, {
      status: "ready",
      events: [{
        event_seq: 101,
        kind: "ModuleVisualEntityUpserted",
        module_visual_entity_id: "module-relay",
      }],
    });
    const locateModule = await screen.findByRole("button", { name: /locate module relay seven/i });
    const updatesBeforeSelection = runtimeMock.updateCalls;
    fireEvent.click(locateModule);

    await waitFor(() => {
      expect(core.state.selectedKind).toBe("module_visual");
      expect(core.state.selectedId).toBe("module-relay");
      expect(document.querySelector(".pixel-world-canvas__selection")).toHaveTextContent("Relay Seven");
      expect(runtimeMock.updateCalls).toBe(updatesBeforeSelection + 1);
    });
    expect(appRenderHook).toHaveBeenCalledTimes(1);
    const latestRenderInput = runtimeMock.deriveRenderState.mock.calls.at(-1)[0];
    expect(latestRenderInput.selectedKind).toBe("module_visual");
    expect(latestRenderInput.selectedId).toBe("module-relay");
    expect(window.location.hash).toBe("#viewer-details-panel");
    expect(document.activeElement).toBe(detailsPanel);

    const camera = { zoom: 1.5, pan_x_px: 120, pan_y_px: -48 };
    runtimeMock.onEvent({ type: "camera_state_changed", camera });
    await waitFor(() => expect(core.state.pixelWorldCamera).toEqual(camera));

    const updatesBeforeUnmount = runtimeMock.updateCalls;
    cleanupMountedHost();
    core.requestRender();
    expect(runtimeMock.updateCalls).toBe(updatesBeforeUnmount);
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
