import { expect, it, vi } from "vitest";
import { createViewerApplication } from "./viewer_application.js";
import { coordinateDependentQuotes, quoteDependencyFingerprint } from "./viewer_quote_dependencies.js";

function fixture() {
  const state = { auth: { playerId: "player", bindingEpoch: 1 }, snapshot: {},
    lastGameplayActionFeedback: { stage: "unknown", originalRequest: { auth: { nonce: 7 } } } };
  state.refineQuotePreflight = { value: "quote" };
  state.refineQuoteRequest = { status: "received", dependencyFingerprint: quoteDependencyFingerprint(state, "refine") };
  return state;
}
it("publishes without coordinating domain writes, then coordinates before a compatibility update", () => {
  const state = fixture();
  const coordinateState = vi.fn(() => coordinateDependentQuotes(state));
  const app = createViewerApplication({ coordinateState, bootstrap: async () => {}, onBootstrapError: vi.fn() });
  state.auth.bindingEpoch = 2;
  Object.freeze(state.auth);
  Object.freeze(state.lastGameplayActionFeedback.originalRequest.auth);
  const before = structuredClone(state), seen = [];
  const detach = app.attachView(value => seen.push([value, state.refineQuoteRequest.status]));
  app.publish("scope");
  expect(coordinateState).not.toHaveBeenCalled();
  expect(state).toEqual(before);
  expect(seen).toEqual([["scope", "received"]]);
  app.coordinateAndPublish("update");
  expect(seen[1]).toEqual(["update", "idle"]);
  expect(state.refineQuotePreflight).toBeNull();
  detach();
  app.coordinateAndPublish();
  expect(seen).toHaveLength(2);
  expect(state.lastGameplayActionFeedback).toEqual(before.lastGameplayActionFeedback);
});

it("shares pending and successful start across view detach without restarting", async () => {
  let complete;
  const bootstrap = vi.fn(() => new Promise(resolve => { complete = resolve; }));
  const app = createViewerApplication({ coordinateState: vi.fn(), bootstrap, onBootstrapError: vi.fn() });
  const first = app.start();
  expect(app.start()).toBe(first);
  app.attachView(() => {})();
  complete("ready");
  await expect(first).resolves.toBe("ready");
  expect(app.start()).toBe(first);
  expect(bootstrap).toHaveBeenCalledTimes(1);
});

it("caches a failed start and synchronously publishes failure after its owner updates", async () => {
  const failure = new Error("bootstrap failed"), calls = [];
  const bootstrap = vi.fn(async () => { throw failure; });
  const app = createViewerApplication({ bootstrap, onBootstrapError: error => calls.push(error), coordinateState: () => calls.push("coordinate") });
  app.setView(() => calls.push("primary"));
  app.attachView(() => calls.push("subscriber"));
  const first = app.start();
  await expect(first).rejects.toBe(failure);
  expect(app.start()).toBe(first);
  await expect(app.start()).rejects.toBe(failure);
  expect(bootstrap).toHaveBeenCalledTimes(1);
  expect(calls).toEqual([failure, "coordinate", "primary", "subscriber"]);
});
