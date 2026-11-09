import fs from "node:fs";
import { createHash } from "node:crypto";
import { normalize, relative, resolve } from "node:path";

export function readRegularFile(filePath) {
  // O_NONBLOCK lets us reject a FIFO before it can wait for a writer. Keep
  // existing symlink behavior; the type check and read refer to this one fd.
  const fd = fs.openSync(filePath, fs.constants.O_RDONLY | (fs.constants.O_NONBLOCK || 0));
  try {
    if (!fs.fstatSync(fd).isFile()) throw new Error("not a regular file");
    return fs.readFileSync(fd);
  } finally {
    fs.closeSync(fd);
  }
}

export function fileFingerprint(filePath) {
  const content = readRegularFile(filePath);
  return { bytes: content.length, sha256: createHash("sha256").update(content).digest("hex") };
}

export function serveStaticFile(request, response, root, contentType, { bridgeFromDist = false } = {}) {
  const requestUrl = new URL(request.url || "/", "http://127.0.0.1");
  let rawPath;
  try {
    rawPath = decodeURIComponent(requestUrl.pathname === "/" ? "/viewer.html" : requestUrl.pathname);
  } catch {
    response.writeHead(400);
    response.end("bad request");
    return;
  }
  const normalized = normalize(rawPath).replace(/^(\.\.(\/|\\|$))+/, "");
  const bridgePath = bridgeFromDist && normalized.replaceAll("\\", "/").startsWith("/pixel-world-bridge/");
  const filePath = bridgePath ? resolve(root, "dist", `.${normalized}`) : resolve(root, `.${normalized}`);
  const relativePath = relative(root, filePath);
  if (!relativePath || relativePath.startsWith("..")) {
    response.writeHead(403);
    response.end("forbidden");
    return;
  }
  try {
    const content = readRegularFile(filePath);
    response.writeHead(200, { "Content-Type": contentType(filePath), "Cache-Control": "no-store" });
    response.end(content);
  } catch {
    response.writeHead(404);
    response.end("not found");
  }
}
