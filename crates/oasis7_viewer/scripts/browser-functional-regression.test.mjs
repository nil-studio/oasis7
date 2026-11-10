import assert from "node:assert/strict";
import { test } from "node:test";
import vm from "node:vm";

import {
  deliveryProbeSource,
  resolveBrowserBinary,
  selectCases,
  selectViewports,
} from "./browser-functional-regression.mjs";

test("case selection accepts the declared P01-P03 matrix", () => {
  const selected = selectCases(["p01-loading", "p02-success", "p03-control-lost"]);
  assert.deepEqual(selected, ["p01-loading", "p02-success", "p03-control-lost"]);
});

test("empty case selection fails instead of silently reporting pass", () => {
  assert.throws(() => selectCases([]), /at least one case/);
  assert.throws(() => selectCases(["", "  "]), /at least one case/);
});

test("unknown case ids fail closed", () => {
  assert.throws(() => selectCases(["p01-loading", "p99-not-real"]), /unknown case id/);
});

test("duplicate case ids fail instead of inflating the expected matrix", () => {
  assert.throws(() => selectCases(["p01-loading", "p01-loading"]), /duplicate case selection/);
});

test("duplicate viewports fail instead of inflating the expected matrix", () => {
  assert.deepEqual(selectViewports(["desktop", "narrow"]), ["desktop", "narrow"]);
  assert.throws(() => selectViewports(["desktop", "desktop"]), /duplicate viewport selection/);
  assert.throws(() => selectViewports(["desktop", "wide"]), /unknown viewport/);
});

test("delivery probe waits for the page delivery promise before serializing", async () => {
  let settle = null;
  const pending = new Promise((resolve) => { settle = resolve; });
  const context = { JSON, String, window: { __BFR__: { deliver: () => pending } } };
  const probe = vm.runInNewContext(deliveryProbeSource({ type: "snapshot" }, 300), context);
  let outcome = null;
  probe.then((value) => { outcome = JSON.parse(value); });
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(outcome, null, "probe serialized before the page delivery settled");
  settle(1);
  await probe;
  assert.deepEqual(outcome, { ok: true, delivered: 1 });
});

test("delivery probe surfaces a deadline rejection as a failed settlement", async () => {
  const context = {
    JSON,
    String,
    window: { __BFR__: { deliver: () => Promise.reject(new Error("no open fixture websocket")) } },
  };
  const outcome = JSON.parse(await vm.runInNewContext(deliveryProbeSource({ type: "snapshot" }, 300), context));
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /no open fixture websocket/);
});

test("missing browser automation binary fails closed", () => {
  assert.throws(() => resolveBrowserBinary("/nonexistent/agent-browser-for-test"), /required browser automation binary/);
  assert.throws(() => resolveBrowserBinary(""), /required/);
});
