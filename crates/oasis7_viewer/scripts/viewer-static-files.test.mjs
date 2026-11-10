import assert from "node:assert/strict";
import fs from "node:fs";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileFingerprint, readRegularFile, serveStaticFile } from "./viewer-static-files.mjs";

function fixture(t) {
  const root = fs.mkdtempSync(join(tmpdir(), "viewer files "));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const file = join(root, "viewer.html");
  fs.writeFileSync(file, "original viewer content");
  return { root, file };
}

for (const operation of [readRegularFile, fileFingerprint]) {
  test(`${operation.name} retains the opened file when its pathname is replaced`, t => {
    const { file } = fixture(t);
    const original = fs.readFileSync(file);
    let replaced = false;
    for (const name of ["statSync", "fstatSync"]) {
      const inspect = fs[name];
      t.mock.method(fs, name, (...args) => {
        const result = inspect(...args);
        if (!replaced) {
          replaced = true;
          fs.renameSync(file, file + ".old");
          fs.writeFileSync(file, "replacement");
        }
        return result;
      });
    }
    const expected = operation === readRegularFile ? original : {
      bytes: original.length,
      sha256: createHash("sha256").update(original).digest("hex"),
    };
    assert.deepEqual(operation(file), expected);
    assert.equal(replaced, true, "the replacement must occur during the file check");
    assert.equal(fs.readFileSync(file, "utf8"), "replacement");
  });
}

test("regular-file reads close their descriptor on success and on failures", t => {
  const { root, file } = fixture(t);
  const opened = [];
  const open = fs.openSync;
  t.mock.method(fs, "openSync", (...args) => {
    const fd = open(...args);
    opened.push(fd);
    return fd;
  });
  assert.equal(readRegularFile(file).toString(), "original viewer content");
  assert.throws(() => readRegularFile(root));
  assert.throws(() => readRegularFile(join(root, "missing")), { code: "ENOENT" });
  const read = t.mock.method(fs, "readFileSync", () => { throw new Error("read failed"); });
  assert.throws(() => readRegularFile(file), /read failed/);
  read.mock.restore();
  assert.ok(opened.length >= 2);
  for (const fd of opened) assert.throws(() => fs.fstatSync(fd), { code: "EBADF" });
});

test("static HTTP responses retain content, type, missing-file and containment behavior", async t => {
  const { root } = fixture(t);
  fs.mkdirSync(join(root, "directory"));
  fs.writeFileSync(join(root, "module.js"), "export const value = 7;");
  const server = createServer((request, response) => serveStaticFile(
    request, response, root, file => file.endsWith(".js") ? "text/javascript" : "text/html"));
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())));
  const origin = `http://127.0.0.1:${server.address().port}`;
  for (const [url, status, body, type] of [
    ["/", 200, "original viewer content", "text/html"],
    ["/module.js", 200, "export const value = 7;", "text/javascript"],
    ["/directory", 404, "not found"],
    ["/missing", 404, "not found"],
    ["/..private", 403, "forbidden"],
    ["/%zz", 400, "bad request"],
  ]) {
    const response = await fetch(origin + url);
    assert.equal(response.status, status, url);
    assert.equal(await response.text(), body, url);
    if (type) {
      assert.equal(response.headers.get("content-type"), type);
      assert.equal(response.headers.get("cache-control"), "no-store");
    }
  }
  const escaped = await fetch(origin + "/%2e%2e%5coutside");
  assert.ok([403, 404].includes(escaped.status), "platform path normalization must not permit escaping the root");
});

test("fragment server retains its dist bridge mapping without changing other servers", async t => {
  const { root } = fixture(t);
  const bridge = join(root, "dist", "pixel-world-bridge");
  fs.mkdirSync(bridge, { recursive: true });
  fs.writeFileSync(join(bridge, "bridge.wasm"), Buffer.from([0, 97, 115, 109]));
  for (const bridgeFromDist of [false, true]) {
    const server = createServer((request, response) => serveStaticFile(
      request, response, root, () => "application/wasm", { bridgeFromDist }));
    await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
    try {
      const response = await fetch(`http://127.0.0.1:${server.address().port}/pixel-world-bridge/bridge.wasm`);
      assert.equal(response.status, bridgeFromDist ? 200 : 404);
      if (bridgeFromDist) {
        assert.equal(response.headers.get("content-type"), "application/wasm");
        assert.deepEqual(Buffer.from(await response.arrayBuffer()), Buffer.from([0, 97, 115, 109]));
      } else {
        assert.equal(await response.text(), "not found");
      }
    } finally {
      await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    }
  }
});
