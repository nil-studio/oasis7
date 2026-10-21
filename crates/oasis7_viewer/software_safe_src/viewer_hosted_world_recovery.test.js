import { webcrypto } from "node:crypto";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createHostedWorldRecovery } from "./viewer_hosted_world_recovery.js";
let keys, keyStore, held;
const candidate = { accountId: "account-a", attempt: "original-attempt", keypair: { publicKey: "public", privateKey: "synthetic-private-key" } };
beforeEach(() => {
  vi.stubGlobal("crypto", webcrypto);
  vi.stubGlobal("indexedDB", {});
  keys = new Map(); held = new Set();
  keyStore = vi.fn(async (id, operation, value) => {
    if (operation === "get") return keys.get(id);
    if (operation === "put") keys.set(id, value);
    if (operation === "delete") keys.delete(id);
  });
  vi.stubGlobal("navigator", { locks: { request: async (id, options, callback) => {
    if (held.has(id)) return callback(null);
    held.add(id);
    try { return await callback({ name: id }); } finally { held.delete(id); }
  } } });
  sessionStorage.clear();
});
afterEach(async () => { dispatchEvent(new Event("pagehide")); await Promise.resolve(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
it("recovers the original key and attempt after reload without persisting plaintext", async () => {
  const first = createHostedWorldRecovery({ keyStore });
  await first.save(candidate);
  expect(sessionStorage.getItem("oasis7.hosted-world-recovery.v1")).not.toContain(candidate.keypair.privateKey);
  expect([...keys.values()][0].extractable).toBe(false);
  await expect(crypto.subtle.exportKey("raw", [...keys.values()][0])).rejects.toThrow();
  dispatchEvent(new Event("pagehide")); await vi.waitFor(() => expect(held.size).toBe(0));
  const reloaded = createHostedWorldRecovery({ keyStore });
  expect(await reloaded.load("account-a")).toEqual(candidate);
  await reloaded.clear();
  expect(sessionStorage.length).toBe(0); expect(keys.size).toBe(0);
});
it("denies a cloned tab while the original controller holds its lock", async () => {
  await createHostedWorldRecovery({ keyStore }).save(candidate);
  await expect(createHostedWorldRecovery({ keyStore }).load("account-a")).rejects.toThrow("context_in_use");
});
it("does not recover another account or damaged ciphertext", async () => {
  const recovery = createHostedWorldRecovery({ keyStore });
  await recovery.save(candidate);
  expect(await recovery.load("account-b")).toBeNull();
  expect(keys.size).toBe(0);
  await recovery.save(candidate);
  const capsule = JSON.parse(sessionStorage.getItem("oasis7.hosted-world-recovery.v1"));
  capsule.accountId = "account-b";
  sessionStorage.setItem("oasis7.hosted-world-recovery.v1", JSON.stringify(capsule));
  await expect(recovery.load("account-b")).rejects.toThrow("world_recovery_unavailable");
});
it("leaves control in memory when required browser APIs are unavailable", async () => {
  vi.stubGlobal("indexedDB", undefined);
  const recovery = createHostedWorldRecovery({ keyStore });
  expect(await recovery.load("account-a")).toBeNull();
  expect(await recovery.save(candidate)).toBe(false);
  expect(keyStore).not.toHaveBeenCalled(); expect(sessionStorage.length).toBe(0);
});

it("retains exclusive custody while a page is in the back-forward cache", async () => {
  await createHostedWorldRecovery({ keyStore }).save(candidate);
  const event = new Event("pagehide"); Object.defineProperty(event, "persisted", { value: true });
  dispatchEvent(event);
  await expect(createHostedWorldRecovery({ keyStore }).load("account-a")).rejects.toThrow("context_in_use");
});
it("fails closed if browser storage refuses a saved recovery context", async () => {
  const recovery = createHostedWorldRecovery({ keyStore });
  await recovery.save(candidate);
  vi.spyOn(Storage.prototype, "getItem").mockImplementationOnce(() => { throw new Error("storage denied"); });
  await expect(recovery.load("account-a")).rejects.toThrow("storage denied");
});

it("serializes logout behind pending saves without retaining a capsule or encryption key", async () => {
  let resume; const paused = new Promise(resolve => { resume = resolve; });
  const slowStore = async (...args) => { if (args[1] === "put") await paused; return keyStore(...args); };
  const recovery = createHostedWorldRecovery({ keyStore: slowStore });
  const saving = recovery.save(candidate);
  await vi.waitFor(() => expect(held.size).toBe(1));
  const clearing = recovery.clear();
  resume();
  await Promise.all([saving, clearing]);
  expect(sessionStorage.length).toBe(0); expect(keys.size).toBe(0);
  await recovery.save({ ...candidate, accountId: "new-account" });
  expect(await recovery.load("new-account")).toMatchObject({ accountId: "new-account" });
});

it("an account-switch save queues after cleanup instead of being deleted by old cleanup", async () => {
  const recovery = createHostedWorldRecovery({ keyStore });
  await recovery.save(candidate);
  const clearing = recovery.clear();
  const saving = recovery.save({ ...candidate, accountId: "new-account" });
  await Promise.all([clearing, saving]);
  expect(await recovery.load("new-account")).toMatchObject({ accountId: "new-account" });
});

it("does not publish a pending save after pagehide releases exclusive custody", async () => {
  let resume; const paused = new Promise(resolve => { resume = resolve; });
  const slowStore = async (...args) => { if (args[1] === "put") await paused; return keyStore(...args); };
  const recovery = createHostedWorldRecovery({ keyStore: slowStore });
  const saving = recovery.save(candidate);
  await vi.waitFor(() => expect(held.size).toBe(1));
  dispatchEvent(new Event("pagehide"));
  resume();
  expect(await saving).toBe(false);
  expect(sessionStorage.length).toBe(0);
});

it("encrypts the original signed gameplay request and recovers its unchanged nonce", async () => {
  const recovery = createHostedWorldRecovery({ keyStore });
  const gameplayRequest = { action_id: "claim_first_agent", target_agent_id: "synthetic-target", auth: { nonce: 4321, signature: "synthetic-request-proof" } };
  await recovery.save({ ...candidate, gameplayRequest });
  const stored = sessionStorage.getItem("oasis7.hosted-world-recovery.v1");
  expect(stored).not.toContain("synthetic-request-proof");
  expect(stored).not.toContain("claim_first_agent");
  expect((await recovery.load("account-a")).gameplayRequest).toEqual(gameplayRequest);
});

it("explicit logout removes corrupted recovery metadata so it cannot block the next account", async () => {
  const recovery = createHostedWorldRecovery({ keyStore });
  await recovery.save(candidate);
  sessionStorage.setItem("oasis7.hosted-world-recovery.v1", "{corrupt");
  await recovery.clear();
  expect(sessionStorage.length).toBe(0);
  expect(keys.size).toBe(0);
  await recovery.save({ ...candidate, accountId: "new-account" });
  expect(await recovery.load("new-account")).toMatchObject({ accountId: "new-account" });
});
