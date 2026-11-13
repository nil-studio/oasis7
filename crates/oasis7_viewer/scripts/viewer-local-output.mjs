import { lstatSync, realpathSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const viewerRoot = resolve(import.meta.dirname, "..");

// The local entry owns this directory and supplies it after clearing inherited
// OASIS7 overrides. Keep one layout for producers and their browser consumers.
export function localViewerOutputRoot(env = process.env) {
  if (env.OASIS7_LOCAL_CHECK !== "1") return null;
  const raw = env.OASIS7_VIEWER_LOCAL_OUTPUT_ROOT;
  if (!raw || !isAbsolute(raw)) throw new Error("local Viewer checks require an absolute owned output directory");
  const root = resolve(raw);
  if (realpathSync(root) !== root || !lstatSync(root).isDirectory()) {
    throw new Error("local Viewer output directory must be a canonical directory without symlinks");
  }
  return root;
}

export function localViewerPath(...parts) {
  const root = localViewerOutputRoot();
  if (!root) throw new Error("local Viewer output requested outside a local check");
  const path = resolve(root, ...parts);
  if (!relative(root, path) || relative(root, path).startsWith(`..${sep}`) || relative(root, path) === "..") {
    throw new Error("local Viewer output must stay inside its owned directory");
  }
  for (let current = path; current !== root; current = dirname(current)) {
    try {
      if (lstatSync(current).isSymbolicLink()) throw new Error(`local Viewer output contains a symlink: ${current}`);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  return path;
}

export function viewerBuildPaths(profile = "release") {
  if (!["release", "visual-test"].includes(profile)) throw new Error("unknown Viewer build profile");
  const local = localViewerOutputRoot() !== null;
  const visualTest = profile === "visual-test";
  return {
    local,
    bundleDir: local ? localViewerPath("bundles", profile)
      : resolve(viewerRoot, visualTest ? ".software-safe-test-build" : ".software-safe-build"),
    distDir: local ? localViewerPath("dist", profile)
      : resolve(viewerRoot, visualTest ? ".viewer-test-dist" : "dist"),
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [kind, profile] = process.argv.slice(2);
  if (kind === "bundle" || kind === "dist") {
    console.log(viewerBuildPaths(profile)[`${kind}Dir`]);
  } else if (kind === "report" && ["browser-functional", "performance"].includes(profile)) {
    console.log(localViewerPath("reports", profile));
  } else {
    throw new Error("usage: viewer-local-output.mjs bundle|dist release|visual-test | report browser-functional|performance");
  }
}
