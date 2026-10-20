import { describe, expect, it } from "vitest";
import { createViewerAuthSurfaceModule } from "./viewer_auth_surface_module.js";

function createAuthSurfaceModule(hostedAccess) {
  return createViewerAuthSurfaceModule({
    getSearchParams: () => new URLSearchParams({ hosted_access: hostedAccess }),
    localeText: (_locale, zh, en) => en || zh,
    state: {
      auth: { available: false, error: null },
      hostedAccess: null,
      uiLocale: "en",
    },
    windowRef: { location: { href: "http://127.0.0.1/viewer", hostname: "127.0.0.1" } },
  });
}

describe("viewer auth surface module", () => {
  it("rejects JSON arrays passed as hosted access configuration", () => {
    const module = createAuthSurfaceModule("[]");

    expect(module.resolveHostedAccessHint()).toBe(null);
  });
});


describe("canonical chat readiness", () => {
  function fixture() {
    const view = { agent_id: "a", player_id: "p", public_key: "k", world_id: "w", reorg_epoch: 1,
      authority_scope: "player_agent_chat", canonical_authority: { branch_id: "b", agent_identity_generation: 2 } };
    const state = { auth: { available: true, registrationStatus: "registered", boundAgentId: "a", playerId: "p", publicKey: "k" },
      viewerProtocol: { negotiated: true, capabilities: ["canonical_agent_chat_v1"] },
      worldFeed: { status: "ready", worldId: "w", reorgEpoch: 1, stale: false }, canonicalAgentOwnerView: view };
    const module = createViewerAuthSurfaceModule({ state, getSearchParams: () => new URLSearchParams(),
      localeText: (_l, zh, en) => en || zh, windowRef: { location: { href: "http://localhost/", hostname: "localhost" } } });
    return { state, capability: () => module.buildSemanticCapability("agent_chat") };
  }
  it("waits for an authenticated owner view instead of declaring an inevitably rejected chat ready", () => {
    const { state, capability } = fixture();
    expect(capability().enabled).toBe(true);
    state.canonicalAgentOwnerView = null;
    expect(capability()).toMatchObject({ enabled: false, code: "canonical_owner_not_ready" });
  });
  it.each(["registering", "stale", "branch", "generation"])("blocks %s authority", (kind) => {
    const { state, capability } = fixture();
    if (kind === "registering") state.auth.registrationStatus = "registering";
    if (kind === "stale") state.worldFeed.stale = true;
    if (kind === "branch") state.canonicalAgentOwnerView.canonical_authority.branch_id = "";
    if (kind === "generation") state.canonicalAgentOwnerView.canonical_authority.agent_identity_generation = 0;
    expect(capability().enabled).toBe(false);
  });
});
