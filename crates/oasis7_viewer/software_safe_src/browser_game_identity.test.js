import { describe, it, expect } from "vitest";
import { webcrypto } from "node:crypto";
import { createBrowserExecutorIdentity, createBrowserGameDatabase } from "./browser_game_identity.js";

const scope = { worldId: "world", genesisDigest: "genesis", accountId: "owner", agentId: "agent" };
function storage() {
  const records = new Map();
  return { records, get: async (key) => structuredClone(records.get(key)),
    update: async (key, mutate) => {
      const updated = mutate(structuredClone(records.get(key)));
      records.set(key, structuredClone(updated)); return structuredClone(updated);
    } };
}
describe("browser executor identity", () => {
  it("persists an independent nonextractable Ed25519 identity and verifies signatures", async () => {
    const database = storage(); const create = () => createBrowserExecutorIdentity({ storage: database, cryptoImpl: webcrypto });
    const first = await create().getOrCreate(scope);
    const reloaded = await create().getOrCreate(scope);
    expect(first.publicKeyHex).toMatch(/^[0-9a-f]{64}$/);
    expect(reloaded.publicKeyHex).toBe(first.publicKeyHex);
    const stored = [...database.records.values()][0];
    expect(stored.privateKey.extractable).toBe(false);
    await expect(webcrypto.subtle.exportKey("pkcs8", stored.privateKey)).rejects.toThrow();
    const message = new Uint8Array([1, 2, 3]);
    const publicKey = await webcrypto.subtle.importKey("raw", Buffer.from(first.publicKeyHex, "hex"), "Ed25519", false, ["verify"]);
    expect(await webcrypto.subtle.verify("Ed25519", publicKey, await first.sign(message), message)).toBe(true);
    expect(Object.keys(first)).toEqual(["publicKeyHex", "sign"]);
  });
  it("retains the transactional first key on concurrent creation and isolates every scope component", async () => {
    const database = storage(); const factory = createBrowserExecutorIdentity({ storage: database, cryptoImpl: webcrypto });
    const identities = await Promise.all([factory.getOrCreate(scope), factory.getOrCreate(scope)]);
    expect(identities[0].publicKeyHex).toBe(identities[1].publicKeyHex);
    for (const field of Object.keys(scope)) {
      expect((await factory.getOrCreate({ ...scope, [field]: "different" })).publicKeyHex).not.toBe(identities[0].publicKeyHex);
    }
  });
  it("fails closed on corruption, storage absence and unsupported crypto without delegation", async () => {
    const database = storage(); const factory = createBrowserExecutorIdentity({ storage: database, cryptoImpl: webcrypto });
    await factory.getOrCreate(scope);
    [...database.records.values()][0].publicKeyHex = "ff".repeat(32);
    await expect(factory.getOrCreate(scope)).rejects.toMatchObject({ code: "executor_identity_corrupt" });
    await expect(createBrowserGameDatabase({}).get("key")).rejects.toMatchObject({ code: "indexeddb_unavailable" });
    await expect(createBrowserExecutorIdentity({ storage: storage(), cryptoImpl: {} }).getOrCreate(scope))
      .rejects.toMatchObject({ code: "webcrypto_unavailable" });
    await expect(factory.getOrCreate({ ...scope, accountId: "" })).rejects.toMatchObject({ code: "identity_scope_invalid" });
  });
});
