import { waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { renderPixelWorldHost, runtimeMock, sampleSnapshot, useTestRustRenderState } from "./pixel_world_host_test_support.jsx";

afterEach(() => vi.unstubAllGlobals());

it("refreshes telemetry and tick readout without deriving identical geometry; changed geometry still updates", async () => {
  const sockets = [];
  class MockWebSocket {
    static OPEN = 1;
    constructor() { this.readyState = 0; this.listeners = new Map(); sockets.push(this); }
    addEventListener(type, listener) { this.listeners.set(type, [...(this.listeners.get(type) || []), listener]); }
    send() {}
    close() { this.readyState = 3; }
    receive(message) { for (const listener of this.listeners.get("message") || []) listener({ data: JSON.stringify(message) }); }
  }
  vi.stubGlobal("WebSocket", MockWebSocket);
  useTestRustRenderState();
  const derive = runtimeMock.deriveRenderState = vi.fn(runtimeMock.deriveRenderState);
  const { core } = await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=1&locale=en", "en", {initializeCore:true});
  await waitFor(() => expect(runtimeMock.mountCalls).toBe(1));
  await waitFor(() => expect(document.querySelector('[data-world-tick]')).toBeInTheDocument());
  const notify = vi.fn();
  const unsubscribe = core.subscribeRenderHook(notify);
  const beforeDerive = derive.mock.calls.length;
  const beforeUpdate = runtimeMock.updateCalls;
  const socket = sockets.at(-1);
  for (let time = 50; time < 60; time += 1) {
    socket.receive({ type: "metrics", time, metrics: { decision_trace_count: time, total_ticks: time } });
    socket.receive({ type: "decision_trace", trace: { agent_id: "agent-0", time, decision: { type: "wait" } } });
  }
  expect(core.state.metrics.total_ticks).toBe(59);
  expect(core.state.recentDecisionTraces[0].time).toBe(59);
  expect(core.state.tick).toBe(59);
  expect(notify).toHaveBeenCalledTimes(20);
  expect(derive.mock.calls.length).toBe(beforeDerive);
  expect(runtimeMock.updateCalls).toBe(beforeUpdate);

  const previousSnapshot = core.state.snapshot;
  core.state.snapshot.time = 70;
  core.requestRender();
  expect(core.state.snapshot).toBe(previousSnapshot);
  await waitFor(() => expect(document.querySelector('[data-world-tick="70"]')).toBeInTheDocument());
  expect(derive.mock.calls.length).toBe(beforeDerive);
  expect(runtimeMock.updateCalls).toBe(beforeUpdate);
  socket.receive({ type: "event", event: { time: 71, kind: "agent_waited", agent_id: "agent-0" } });
  // A burst produces one latest display; mutation is detected without relying
  // on snapshot identity, and the event reducer still receives its event.
  for (let x = 1; x <= 20; x += 1) {
    core.state.snapshot.model.locations["loc-0"].pos.x_cm = 5000000 + x;
    core.requestRender();
  }
  await waitFor(() => expect(runtimeMock.updateCalls).toBeGreaterThan(beforeUpdate));
  expect(derive.mock.calls.length).toBe(beforeDerive + 1);
  expect(derive.mock.calls.at(-1)[0].lists.locations[0].pos.x_cm).toBe(5000020);
  expect(core.state.recentEvents.length).toBeGreaterThan(0);
  const lastUpdate = runtimeMock.updateCalls;
  core.state.auth.boundAgentId = null;
  core.requestRender();
  // A changed authorization scope clears the old map instead of deriving it.
  expect(document.querySelector('[data-world-tick]')).not.toBeInTheDocument();
  socket.receive({ type: "snapshot", snapshot: { ...sampleSnapshot(), time: 72 } });
  expect(runtimeMock.updateCalls).toBe(lastUpdate);
  expect(document.querySelector('[data-world-tick="72"]')).not.toBeInTheDocument();
  unsubscribe();
});
