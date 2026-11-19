import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BrowserCanonicalEntry } from "./browser_canonical_entry.jsx";
afterEach(cleanup);
const config = { endpoint: "/api/public/hosted-account/v2", servicePublicKey: "service", world: { world_id: "world", genesis_digest: "genesis" } };
const account = (id) => ({ api_version: 2, ok: true, authentication: { state: "authenticated" }, account: { account_id: id } });
function setup(overrides = {}) {
 const accountClient = { session: vi.fn(async () => account("account")) };
 const identityStore = { getOrCreate: vi.fn(async (scope) => ({ publicKeyHex: `owner-${scope.accountId}`, sign: vi.fn(async () => new Uint8Array(64)) })) };
 const inspect = vi.fn(async () => ({ state: "needs_task", agentId: "canonical-agent" })), prepare = vi.fn();
 const createOwnerEntry = vi.fn(() => ({ inspect, prepare, dispose: vi.fn() }));
 render(() => <BrowserCanonicalEntry locale="en" config={config} accountClient={accountClient} identityStore={identityStore}
  surfaceProps={{ protocolLoader: async () => ({}), createOwnerEntry }} {...overrides} />);
 return { accountClient, identityStore, inspect, prepare, createOwnerEntry };
}
describe("ordinary canonical account entry", () => {
 it("opens from the signed-in account without a legacy player, world join or selected Agent", async () => {
  const f = setup();
  expect(await screen.findByRole("button", { name: "Prepare player, Agent and Task" })).toBeVisible();
  expect(f.identityStore.getOrCreate).toHaveBeenCalledWith({ worldId: "world", genesisDigest: "genesis", accountId: "account" });
  expect(f.createOwnerEntry.mock.calls[0][0]).toMatchObject({ accountId: "account", ownerSigner: { publicKeyHex: "owner-account" } });
  expect(f.inspect).toHaveBeenCalledTimes(1); expect(f.prepare).not.toHaveBeenCalled();
 });
 it("keeps the scope tied to the authenticated account and never reuses another account's legacy key", async () => {
  const f = setup({ legacyAccountId: "different-account", legacyOwnerSigner: { publicKey: "wrong-owner", sign: vi.fn() } });
  await screen.findByRole("button", { name: "Prepare player, Agent and Task" });
  expect(f.createOwnerEntry.mock.calls[0][0].ownerSigner.publicKeyHex).toBe("owner-account");
  f.accountClient.session.mockResolvedValue(account("second-account"));
  fireEvent.click(screen.getByRole("button", { name: "Verify account and player identity" }));
  await waitFor(() => expect(f.identityStore.getOrCreate).toHaveBeenLastCalledWith({ worldId: "world", genesisDigest: "genesis", accountId: "second-account" }));
  await waitFor(() => expect(f.createOwnerEntry.mock.calls.at(-1)[0].ownerSigner.publicKeyHex).toBe("owner-second-account"));
 });
 it("requires authenticated session evidence before creating or installing an owner key", async () => {
  const accountClient = { session: vi.fn(async () => ({ api_version: 2, ok: true, account: { account_id: "account" }, authentication: { state: "expired" } })) };
  const f = setup({ accountClient });
  expect(await screen.findByText("Sign in through the account panel, then use the button above to continue.")).toBeVisible();
  expect(f.identityStore.getOrCreate).not.toHaveBeenCalled(); expect(f.createOwnerEntry).not.toHaveBeenCalled();
 });
});
