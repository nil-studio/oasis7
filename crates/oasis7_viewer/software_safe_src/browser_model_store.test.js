import { describe, expect, it } from "vitest";
import { createBrowserModelStore } from "./browser_model_store.js";

function environment() {
  const records = new Map();
  const storage = { getItem: (key) => records.get(key) ?? null, setItem: (key, value) => records.set(key, value) };
  let tail = Promise.resolve();
  const locks = { request: (_name, _options, action) => {
    const next = tail.then(action); tail = next.catch(() => {}); return next;
  } };
  const create = (accountId = "player/a") => createBrowserModelStore({ accountId, storage, locks });
  return { records, storage, locks, create };
}
const reserve = (store, revision, input = 100, output = 100) => store.reserve({ revision,
  maxInputTokens: input, maxOutputTokens: output });

describe("browser model account store", () => {
  it("persists only locally and exposes no credential in snapshots or notifications", async () => {
    const { create } = environment(); const store = create();
    let notice;
    store.subscribe((value) => { notice = value; });
    const saved = await store.save("canary-secret", 0);
    expect(JSON.stringify(saved)).not.toContain("canary");
    expect(notice).toBeUndefined();
    expect(await create().credential(saved.revision)).toBe("canary-secret");
    expect(create("player%2Fa").snapshot().hasKey).toBe(false);
    expect(create("other-player").snapshot().hasKey).toBe(false);
  });
  it("serializes competing tabs and rejects drafts across forget tombstones", async () => {
    const { create } = environment(); const first = create(); const second = create();
    const results = await Promise.allSettled([first.save("one", 0), second.save("two", 0)]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    expect(results.find((r) => r.status === "rejected").reason.code).toBe("revision_conflict");
    await first.forget(1);
    expect(second.snapshot()).toMatchObject({ revision: 2, hasKey: false, stopped: true });
    await expect(second.save("stale", 1)).rejects.toMatchObject({ code: "revision_conflict" });
  });
  it("invalidates old sends and requires explicit probe rearming after stop", async () => {
    const { create } = environment(); const first = create(); const second = create();
    await first.save("key", 0); await reserve(first, 1);
    await second.stop(1);
    await expect(first.credential(1)).rejects.toMatchObject({ code: "revision_conflict" });
    await expect(reserve(first, 2)).rejects.toMatchObject({ code: "stopped" });
    const resumed = await first.beginProbe(2);
    expect(resumed.callsReserved).toBe(1);
    expect(await first.credential(3)).toBe("key");
    await first.clear();
    expect(second.snapshot()).toMatchObject({ revision: 4, hasKey: false, callsReserved: 1 });
  });
  it("retains unknown and malformed usage; refunds only validated actual tokens once", async () => {
    const { create } = environment(); const store = create(); await store.save("key", 0);
    const reservation = await reserve(store, 1, 500, 500);
    for (const usage of [null, {}, { inputTokens: -1, outputTokens: 0 },
      { inputTokens: 1.5, outputTokens: 0 }, { inputTokens: 1001, outputTokens: 0 },
      { inputTokens: 1, outputTokens: NaN }]) {
      await store.settle(reservation, usage);
      expect(store.snapshot().tokensReserved).toBe(1000);
    }
    await store.stop();
    await store.settle(reservation, { inputTokens: 100, outputTokens: 50 });
    expect(store.snapshot()).toMatchObject({ tokensReserved: 150, callsReserved: 1 });
    await expect(store.settle(reservation, { inputTokens: 0, outputTokens: 0 }))
      .rejects.toMatchObject({ code: "reservation_invalid" });
  });
  it("enforces aggregate token and physical request limits across tabs and edits", async () => {
    const { create } = environment(); const store = create(); await store.save("key", 0);
    const requests = await Promise.allSettled([reserve(store, 1, 1000, 500), reserve(create(), 1, 1000, 500)]);
    expect(requests.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    expect(store.snapshot().tokensReserved).toBe(1500);
    await store.save("replacement", 1);
    await reserve(store, 2, 1, 1); await reserve(store, 2, 1, 1); await reserve(store, 2, 1, 1);
    await expect(reserve(store, 2, 1, 1)).rejects.toMatchObject({ code: "budget_exhausted" });
    await store.forget();
    expect(store.snapshot().callsReserved).toBe(4);
  });
  it("fails closed on missing locks, broken storage, corrupt schemas and failed writes", async () => {
    const { create, storage } = environment();
    const noLocks = createBrowserModelStore({ accountId: "a", storage, locks: {} });
    await expect(noLocks.save("key", 0)).rejects.toMatchObject({ code: "locks_unavailable" });
    const store = create(); await store.save("key", 0);
    storage.setItem = () => { throw new Error("canary-secret"); };
    await expect(store.forget()).rejects.toMatchObject({ code: "storage_write_failed" });
    await expect(store.credential(1)).rejects.toMatchObject({ code: "stopped" });
    storage.getItem = () => "invalid";
    expect(() => store.snapshot()).toThrow("storage_corrupt");
    storage.getItem = () => '{"schema":2}';
    expect(() => store.snapshot()).toThrow("storage_schema_unsupported");
    storage.getItem = () => { throw new Error("canary-secret"); };
    expect(() => store.snapshot()).toThrow("storage_unavailable");
  });
});
