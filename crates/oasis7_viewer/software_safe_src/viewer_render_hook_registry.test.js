import { expect, it } from "vitest";
import { createViewerRenderHookRegistry } from "./viewer_render_hook_registry.js";

it("copies subscribers after primary, then preserves the copied iteration during unsubscribe", () => {
  const hooks = createViewerRenderHookRegistry(), calls = [];
  let removeSecond;
  hooks.subscribe(value => { calls.push(`first:${value}`); removeSecond(); hooks.subscribe(() => calls.push("late")); });
  removeSecond = hooks.subscribe(value => calls.push(`second:${value}`));
  hooks.set(value => { calls.push(`primary:${value}`); hooks.subscribe(() => calls.push("from-primary")); });
  hooks.invoke("scope");
  expect(calls).toEqual(["primary:scope", "first:scope", "second:scope", "from-primary"]);
});

it("permits synchronous reentry without queuing", () => {
  const hooks = createViewerRenderHookRegistry(), calls = [];
  hooks.set(value => { calls.push(`primary:${value}`); if (value === 1) hooks.invoke(2); });
  hooks.subscribe(value => calls.push(`subscriber:${value}`));
  hooks.invoke(1);
  expect(calls).toEqual(["primary:1", "primary:2", "subscriber:2", "subscriber:1"]);
});

it("propagates primary and subscriber errors and stops that invocation", () => {
  const hooks = createViewerRenderHookRegistry(), calls = [], failure = new Error("callback failed");
  hooks.subscribe(() => { calls.push("first"); throw failure; });
  hooks.subscribe(() => calls.push("second"));
  hooks.set(() => { throw failure; });
  expect(() => hooks.invoke()).toThrow(failure);
  expect(calls).toEqual([]);
  hooks.set(null);
  expect(() => hooks.invoke()).toThrow(failure);
  expect(calls).toEqual(["first"]);
});

it("uses noops for missing views and supports idempotent detach", () => {
  const hooks = createViewerRenderHookRegistry(), calls = [];
  hooks.invoke();
  const callback = () => calls.push("view");
  const detach = hooks.subscribe(callback);
  hooks.subscribe(callback);
  hooks.invoke();
  detach(); detach(); hooks.subscribe(null)();
  hooks.invoke();
  expect(calls).toEqual(["view"]);
});
