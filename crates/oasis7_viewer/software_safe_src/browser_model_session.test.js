import { afterEach, expect, it, vi } from "vitest";
import { createBrowserModelSession } from "./browser_model_session.js";
const sessions = [];
const locks = { request: async (_name, _options, action) => action() };
const documentRef = () => Object.assign(new EventTarget(), { visibilityState: "visible" });
function setup(fetchImpl, extra = {}) {
  const data = new Map();
  const storage = { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => data.set(key, value) };
  const doc = documentRef(), win = new EventTarget();
  const session = createBrowserModelSession({ accountId: "account-a", storage, locks, fetchImpl, documentRef: doc, windowRef: win, navigatorRef: { onLine: true }, ...extra });
  sessions.push(session);
  return { session, storage, doc, win };
}
const reply = (message, reason) => new Response(JSON.stringify({ model: "deepseek-flash", choices: [{ finish_reason: reason, message }], usage: { prompt_tokens: 20, completion_tokens: 4, total_tokens: 24 } }), { headers: { "Content-Type": "application/json" } });
afterEach(() => { for (const session of sessions.splice(0)) session.dispose(); });

it("only probes after explicit consent path, preserves Key on Stop and removes on logout", async () => {
  const fetchImpl = vi.fn()
    .mockImplementationOnce(async () => reply({ role: "assistant", content: null, tool_calls: [{ type: "function", id: "fixture-1", function: { name: "read_fixture", arguments: "{}" } }] }, "tool_calls"))
    .mockImplementationOnce(async () => reply({ role: "assistant", content: "OK" }, "stop"));
  const { session, storage } = setup(fetchImpl);
  await session.save("canary-session-secret", 0);
  expect(fetchImpl).not.toHaveBeenCalled();
  expect(await session.probe(session.snapshot().revision)).toMatchObject({ tool_round_trip: true });
  expect(fetchImpl).toHaveBeenCalledTimes(2);
  expect(session.snapshot()).toMatchObject({ checked: true, callsReserved: 2, tokensReserved: 48 });
  for (const [endpoint, request] of fetchImpl.mock.calls) {
    expect(endpoint).toBe("https://api.deepseek.com/chat/completions");
    expect(request.body).not.toContain("canary-session-secret");
  }
  await session.stop();
  expect(session.snapshot()).toMatchObject({ hasKey: true, stopped: true, checked: false });
  const reopened = createBrowserModelSession({ accountId: "account-a", storage, locks }); sessions.push(reopened);
  expect(reopened.snapshot()).toMatchObject({ hasKey: true, checked: false, callsReserved: 2 });
  await reopened.logout();
  expect(reopened.snapshot().hasKey).toBe(false);
  expect(JSON.stringify(reopened.snapshot())).not.toContain("secret");
});

it("hidden pages abort and cannot send a second request from a late response", async () => {
  let resolve;
  const fetchImpl = vi.fn(() => new Promise((done) => { resolve = done; }));
  const { session, doc } = setup(fetchImpl);
  await session.save("canary", 0);
  const result = session.probe(session.snapshot().revision).catch((failure) => failure.code);
  await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
  doc.visibilityState = "hidden"; doc.dispatchEvent(new Event("visibilitychange"));
  resolve(reply({ role: "assistant", content: null, tool_calls: [{ type: "function", id: "late", function: { name: "read_fixture", arguments: "{}" } }] }, "tool_calls"));
  expect(await result).toBeTruthy();
  expect(fetchImpl).toHaveBeenCalledTimes(1);
  expect(session.snapshot().checked).toBe(false);
});

it("cross-tab notifications invalidate a check even when no response has arrived", async () => {
  let resolve;
  const fetchImpl = vi.fn(() => new Promise((done) => { resolve = done; }));
  const { session, win } = setup(fetchImpl);
  await session.save("canary", 0);
  const result = session.probe(session.snapshot().revision).catch((failure) => failure.code);
  await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
  win.dispatchEvent(Object.assign(new Event("storage"), { key: null }));
  resolve(reply({ role: "assistant", content: "OK" }, "stop"));
  await result;
  expect(session.snapshot().checked).toBe(false);
  expect(fetchImpl).toHaveBeenCalledTimes(1);
});

it("does not continue a paid check after a visible page wakes from a long pause", async () => {
  let clock = 1000, resolve;
  const fetchImpl = vi.fn(() => new Promise((done) => { resolve = done; }));
  const { session } = setup(fetchImpl, { now: () => clock, monotonicNow: () => clock });
  await session.save("canary", 0);
  const result = session.probe(session.snapshot().revision).catch((failure) => failure.code);
  await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
  clock += 60000;
  resolve(reply({ role: "assistant", content: null, tool_calls: [{ type: "function", id: "wake", function: { name: "read_fixture", arguments: "{}" } }] }, "tool_calls"));
  // A second request here would hang; the pre-send gate must reject first.
  expect(await Promise.race([result, new Promise((done) => setTimeout(() => done("continued"), 200))])).toBe("stopped");
  expect(fetchImpl).toHaveBeenCalledTimes(1);
  expect(session.snapshot().checked).toBe(false);
});

it.each(["freeze", "resume"])("invalidates a visible-page check on %s", async (eventName) => {
  let resolve;
  const fetchImpl = vi.fn(() => new Promise((done) => { resolve = done; }));
  const { session, doc } = setup(fetchImpl);
  await session.save("canary", 0);
  const result = session.probe(session.snapshot().revision).catch((failure) => failure.code);
  await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalledTimes(1));
  doc.dispatchEvent(new Event(eventName));
  resolve(reply({ role: "assistant", content: "OK" }, "stop"));
  await result;
  expect(fetchImpl).toHaveBeenCalledTimes(1);
  expect(session.snapshot().checked).toBe(false);
});
