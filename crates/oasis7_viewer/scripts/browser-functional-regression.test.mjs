import assert from "node:assert/strict";
import { test } from "node:test";

import {
  resolveBrowserBinary,
  selectCases,
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

test("missing browser automation binary fails closed", () => {
  assert.throws(() => resolveBrowserBinary("/nonexistent/agent-browser-for-test"), /required browser automation binary/);
  assert.throws(() => resolveBrowserBinary(""), /required/);
});
