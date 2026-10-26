import { beforeEach, expect, it, vi } from "vitest";
import { quoteDependencyFingerprint } from "./viewer_quote_dependencies.js";

beforeEach(() => { vi.resetModules(); window.localStorage.clear(); window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&hosted_bootstrap=0"); });
function installQuote(state) {
  state.refineQuotePreflight = { value: "current" };
  state.refineQuoteRequest = { status: "received", dependencyFingerprint: quoteDependencyFingerprint(state, "refine") };
}
it("coordinates auth and owner changes without a mounted UI and preserves pending originals", async () => {
  const core = await import("./legacy_core.js");
  await core.initializeSoftwareSafeCore();
  const original = { auth: { nonce: 42, signature: "original" } };
  core.state.lastGameplayActionFeedback = { stage: "unknown", originalRequest: original };
  const savedOriginal = core.state.lastGameplayActionFeedback.originalRequest;
  installQuote(core.state);
  core.requestRender(); core.requestRender();
  expect(core.state.refineQuotePreflight).toEqual({ value: "current" });
  core.state.auth = { ...core.state.auth, bindingEpoch: 7 };
  core.requestRender();
  expect(core.state.refineQuotePreflight).toBeNull();
  installQuote(core.state);
  core.state.canonicalAgentOwnerView = { canonical_authority: { branch_id: "replacement" } };
  core.requestRender();
  expect(core.state.refineQuotePreflight).toBeNull();
  expect(core.state.lastGameplayActionFeedback.originalRequest).toBe(savedOriginal);
}, 30000);
it("coordinates binding adoption after the snapshot's early invalidation", async () => {
  const core = await import("./legacy_core.js");
  await core.initializeSoftwareSafeCore();
  core.state.auth = { ...core.state.auth, playerId: "player", boundAgentId: "old", available: false };
  const snapshot = { time: 1, model: { agents: {}, locations: {}, agent_player_bindings: { replacement: "player" } } };
  core.state.snapshot = structuredClone(snapshot);
  installQuote(core.state);
  core.injectSnapshot(snapshot, { returnState: false });
  expect(core.state.auth.boundAgentId).toBe("replacement");
  expect(core.state.refineQuotePreflight).toBeNull();
}, 30000);
