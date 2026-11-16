import assert from "node:assert/strict";
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, relative, resolve, sep } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

import {
  createFixtureServer,
  describeArtifacts,
  deliveryProbeSource,
  fileFingerprint,
  resolveBrowserBinary,
  selectCases,
  selectViewports,
  waitForBrowserState,
  waitForState,
} from "./browser-functional-regression.mjs";

test("state polling keeps observed request IDs out of browser program text", async () => {
  const requestId = '"\\` ${globalThis.compromised = true}\n</script>\u2028\u2029';
  const expected = { lastPromptFeedback: { stage: "applied", requestId }, nested: [null, false, 12] };
  const states = [{ lastPromptFeedback: { stage: "accepted", requestId } }, expected];
  const sources = [];
  const context = {
    window: { __AW_TEST__: { getState: () => states.shift() } },
    compromised: false,
  };
  const driver = { async evalJson(source) {
    sources.push(source);
    return JSON.parse(vm.runInNewContext(source, context));
  } };
  const result = await waitForState(driver,
    state => state.lastPromptFeedback?.stage === "applied" && state.lastPromptFeedback?.requestId === requestId,
    "matching request applied",
  );
  assert.deepEqual(result, expected);
  assert.equal(sources.length, 2);
  assert.equal(sources[0], "JSON.stringify(window.__AW_TEST__?.getState?.() || {})");
  assert.equal(sources[1], sources[0]);
  assert.equal(sources[0].includes(requestId), false);
  assert.equal(sources[0].includes("=>"), false);
  assert.equal(context.compromised, false);
});

test("state polling preserves the empty state fallback", async () => {
  const driver = { async evalJson(source) {
    return JSON.parse(vm.runInNewContext(source, { window: {} }));
  } };
  assert.deepEqual(await waitForState(driver, state => Object.keys(state).length === 0, "empty state"), {});
});

test("polling timeout retains a descriptive label and bounded last state, not predicate code", async t => {
  const state = { requestId: "observed-request", detail: "x".repeat(2000) };
  let now = 0;
  t.mock.method(Date, "now", () => now);
  let reads = 0;
  const driver = { async evalJson() { reads += 1; now = 2; return state; } };
  const predicate = function neverReady() { return false; };
  await assert.rejects(waitForState(driver, predicate, "request applied", 1), error => {
    assert.equal(error.message, `condition not reached: request applied\n${JSON.stringify(state).slice(0, 1200)}`);
    assert.equal(error.message.includes("neverReady"), false);
    assert.equal(error.message.includes("return false"), false);
    return true;
  });
  assert.equal(reads, 1);
});

test("browser-only polling keeps DOM and state observations in the same browser evaluation", async () => {
  let reads = 0;
  const context = {
    window: { __AW_TEST__: { getState: () => ({ selectedId: null, observation: ++reads }) } },
    document: { body: { get innerText() { return reads > 1 ? "No agents in current snapshot." : "Loading"; } } },
  };
  const driver = { async evalJson(source) { return JSON.parse(vm.runInNewContext(source, context)); } };
  const result = await waitForBrowserState(driver,
    '(state) => state.selectedId === null && document.body.innerText.includes("No agents in current snapshot.")',
    "empty snapshot displayed",
  );
  assert.deepEqual(result, { selectedId: null, observation: 2 });
  assert.equal(reads, 2);
});

test("browser-side missing-epoch checks reject non-finite numbers before JSON converts them to null", async () => {
  for (const epoch of [NaN, Infinity, -Infinity]) {
    let reads = 0;
    const context = { window: { __AW_TEST__: { getState() {
      reads += 1;
      return reads === 1 ? { authSessionEpoch: epoch, authBindingEpoch: epoch } : { authSessionEpoch: null };
    } } } };
    const driver = { async evalJson(source) { return JSON.parse(vm.runInNewContext(source, context)); } };
    const result = await waitForBrowserState(driver,
      "(state) => state.authSessionEpoch == null && state.authBindingEpoch == null",
      "missing auth epochs",
    );
    assert.equal(reads, 2, "a non-finite epoch is not a missing epoch");
    assert.deepEqual(result, { authSessionEpoch: null });
  }
});

test("both polling paths propagate read failures, and state predicate failures are not retried", async () => {
  const readFailure = new Error("browser read failed");
  let reads = 0;
  const driver = { async evalJson() { reads += 1; throw readFailure; } };
  await assert.rejects(waitForState(driver, () => true, "state ready"), error => error === readFailure);
  await assert.rejects(waitForBrowserState(driver, "() => true", "DOM ready"), error => error === readFailure);
  assert.equal(reads, 2);
  const predicateFailure = new Error("bad state shape");
  let calls = 0;
  await assert.rejects(waitForState({ async evalJson() { return {}; } }, () => {
    calls += 1;
    throw predicateFailure;
  }, "state ready"), error => error === predicateFailure);
  assert.equal(calls, 1);
});

test("case selection accepts the declared P01-P07 matrix", () => {
  const selected = selectCases([
    "p01-loading",
    "p02-success",
    "p03-control-lost",
    "p04-signing-deferred",
    "p04-sent-late-error",
    "p05-duplicate-out-of-order",
    "p06-reconnect-recovery",
    "p07-unmount-remount",
  ]);
  assert.deepEqual(selected, [
    "p01-loading",
    "p02-success",
    "p03-control-lost",
    "p04-signing-deferred",
    "p04-sent-late-error",
    "p05-duplicate-out-of-order",
    "p06-reconnect-recovery",
    "p07-unmount-remount",
  ]);
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

function temporaryDirectory(t, parent = tmpdir()) {
  const root = fs.mkdtempSync(join(parent, "browser fixture "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

function replaceAfterFileCheck(t, file) {
  const identity = fs.statSync(file);
  let replaced = false;
  for (const name of ["statSync", "fstatSync"]) {
    const inspect = fs[name];
    t.mock.method(fs, name, (...args) => {
      const result = inspect(...args);
      if (!replaced && result.dev === identity.dev && result.ino === identity.ino) {
        replaced = true;
        fs.renameSync(file, file + ".old");
        fs.writeFileSync(file, "replacement has a different length");
      }
      return result;
    });
  }
  syncBuiltinESMExports();
  t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });
  return () => assert.equal(replaced, true, "the test must replace the checked pathname");
}

test("fixture HTTP reads the checked file even when its pathname is replaced", async t => {
  const viewerRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const root = temporaryDirectory(t, viewerRoot);
  const file = join(root, "module.js");
  fs.writeFileSync(file, "export const original = true;");
  const checked = replaceAfterFileCheck(t, file);
  const { server, port } = await createFixtureServer();
  try {
    const path = relative(viewerRoot, file).split(sep).map(encodeURIComponent).join("/");
    const response = await fetch(`http://127.0.0.1:${port}/${path}`);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("content-type"), "text/javascript; charset=utf-8");
    assert.equal(response.headers.get("cache-control"), "no-store");
    assert.equal(await response.text(), "export const original = true;");
    checked();
    for (const suffix of ["missing", "directory"]) {
      if (suffix === "directory") fs.mkdirSync(join(root, suffix));
      const failed = await fetch(`http://127.0.0.1:${port}/${relative(viewerRoot, root).split(sep).map(encodeURIComponent).join("/")}/${suffix}`);
      assert.equal(failed.status, 404);
      assert.equal(await failed.text(), "not found");
    }
    const malformed = await fetch(`http://127.0.0.1:${port}/%zz`);
    assert.equal(malformed.status, 400);
    const recovered = await fetch(`http://127.0.0.1:${port}/${path}`);
    assert.equal(recovered.status, 200);
    assert.equal(await recovered.text(), "replacement has a different length");
  } finally {
    await new Promise((done, reject) => server.close(error => error ? reject(error) : done()));
  }
});

for (const operation of [fileFingerprint, path => describeArtifacts([path])[0]]) {
  test(`${operation.name || "artifact description"} binds size and hash to the checked content`, t => {
    const root = temporaryDirectory(t);
    const file = join(root, "evidence.png");
    const original = Buffer.from("original evidence");
    fs.writeFileSync(file, original);
    const checked = replaceAfterFileCheck(t, file);
    const result = operation(file);
    assert.equal(result.bytes, original.length);
    assert.equal(result.sha256, createHash("sha256").update(original).digest("hex"));
    checked();
  });
}

test("fingerprints preserve missing, empty and non-file behavior", t => {
  const root = temporaryDirectory(t);
  const missing = join(root, "missing");
  assert.deepEqual(fileFingerprint(missing), { path: missing, missing: true });
  assert.throws(() => describeArtifacts([missing]));
  assert.throws(() => fileFingerprint(root));
  assert.throws(() => describeArtifacts([root]));
  const empty = join(root, "empty");
  fs.writeFileSync(empty, "");
  assert.equal(fileFingerprint(empty).sha256, createHash("sha256").update("").digest("hex"));
  assert.deepEqual(describeArtifacts([empty]), [{ path: empty, bytes: 0, sha256: null }]);
});

function executable(root, name, mode = 0o755) {
  const path = join(root, name);
  const marker = "'" + join(root, "unexpected-execution").replaceAll("'", "'\"'\"'") + "'";
  fs.writeFileSync(path, `#!/bin/sh\n: > ${marker}\nexit 23\n`, { mode });
  return path;
}

test("browser lookup treats command substitution as a literal missing name", () => {
  for (const candidate of ["$(printf sh)", "`printf sh`"]) {
    assert.throws(() => resolveBrowserBinary(candidate), /required browser automation binary/);
  }
});

test("browser lookup accepts executable paths and literal special characters without running them", t => {
  const root = temporaryDirectory(t);
  for (const name of ["browser with spaces", "$(printf sh)", "`printf sh`"]) {
    const file = executable(root, name);
    assert.equal(resolveBrowserBinary(file), file);
    assert.equal(resolveBrowserBinary(relative(process.cwd(), file)), file);
  }
  const target = executable(root, "browser-target");
  const link = join(root, "agent-browser");
  fs.symlinkSync(target, link);
  assert.equal(resolveBrowserBinary(link), link);
  assert.equal(fs.existsSync(join(root, "unexpected-execution")), false);
});

test("browser lookup rejects directories and non-executable files", t => {
  const root = temporaryDirectory(t);
  assert.throws(() => resolveBrowserBinary(root), /required browser automation binary/);
  const file = executable(root, "not-executable", 0o644);
  if (process.platform !== "win32") {
    assert.throws(() => resolveBrowserBinary(file), /required browser automation binary/);
  }
  const link = join(root, "dangling");
  fs.symlinkSync(join(root, "missing"), link);
  assert.throws(() => resolveBrowserBinary(link), /required browser automation binary/);
});

test("browser lookup follows PATH order and skips non-executable candidates", t => {
  const first = temporaryDirectory(t);
  const second = temporaryDirectory(t);
  executable(first, "agent-browser", 0o644);
  const expected = executable(second, "agent-browser");
  const previous = process.env.PATH;
  process.env.PATH = [first, second].join(delimiter);
  try {
    assert.equal(resolveBrowserBinary("agent-browser"), expected);
  } finally {
    if (previous === undefined) delete process.env.PATH;
    else process.env.PATH = previous;
  }
});

test("browser lookup distinguishes an unset PATH from empty and relative entries", { skip: process.platform === "win32" }, t => {
  const root = temporaryDirectory(t);
  const expected = executable(root, "agent-browser");
  const moduleUrl = new URL("./browser-functional-regression.mjs", import.meta.url).href;
  const probe = candidate => `import { resolveBrowserBinary } from ${JSON.stringify(moduleUrl)}; console.log(resolveBrowserBinary(${JSON.stringify(candidate)}));`;
  for (const path of ["", ".", delimiter]) {
    const resolved = execFileSync(process.execPath, ["--input-type=module", "-e", probe("agent-browser")], {
      cwd: root, env: { ...process.env, PATH: path }, encoding: "utf8",
    }).trim();
    assert.equal(resolved, expected);
  }
  const env = { ...process.env };
  delete env.PATH;
  const resolved = execFileSync(process.execPath, ["--input-type=module", "-e", probe("sh")], {
    cwd: root, env, encoding: "utf8",
  }).trim();
  assert.ok(["/usr/bin/sh", "/bin/sh"].includes(resolved));
  assert.equal(fs.existsSync(join(root, "unexpected-execution")), false);
});
