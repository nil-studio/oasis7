import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const IMPORT_RE = /^\s*@import\s+(?:url\(\s*)?["'](\.\/[^"']+\.css)["']\s*\)?\s*;\s*$/;

function toFsPath(target) {
  return target instanceof URL ? fileURLToPath(target) : resolve(String(target));
}

export function readExpandedCssSync(target, seen = new Set()) {
  const fullPath = toFsPath(target);
  if (seen.has(fullPath)) {
    return "";
  }
  seen.add(fullPath);
  const dir = dirname(fullPath);
  const raw = readFileSync(fullPath, "utf8");
  return raw
    .split(/\r?\n/)
    .map((line) => {
      const match = line.match(IMPORT_RE);
      if (!match) {
        return line;
      }
      return readExpandedCssSync(resolve(dir, match[1]), seen);
    })
    .join("\n");
}

export async function readExpandedCss(target) {
  return readExpandedCssSync(target);
}
