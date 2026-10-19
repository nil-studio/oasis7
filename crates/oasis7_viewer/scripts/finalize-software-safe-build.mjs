import { verifyPixelWorldProjection } from "./verify-pixel-world-projection.mjs";
import { isolateBindgenModule } from "./pixel-world-bindgen-factory.mjs";
import { spawn } from "node:child_process";
import { access, copyFile, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { writeViewerCompatAliases } from "./viewer-compat-aliases.mjs";

const scriptsDir = fileURLToPath(new URL(".", import.meta.url));
const viewerRoot = resolve(scriptsDir, "..");
const workspaceRoot = resolve(viewerRoot, "..", "..");
const profile = process.argv[2] || "release";
if (!["release", "visual-test"].includes(profile)) throw new Error("unknown viewer build profile");
const visualTest = profile === "visual-test";
const tempOutDir = resolve(viewerRoot, visualTest ? ".software-safe-test-build" : ".software-safe-build");
const softwareSafeSrcDir = resolve(viewerRoot, "software_safe_src");
const viewerDistDir = resolve(viewerRoot, visualTest ? ".viewer-test-dist" : "dist");
const canonicalHtmlPath = resolve(viewerRoot, "viewer.html");
const builtBundlePath = resolve(tempOutDir, "viewer.js");
const finalCanonicalBundlePath = visualTest ? null : resolve(viewerRoot, "viewer.js");
const finalDistBundlePath = resolve(viewerDistDir, "viewer.js");
const staleRootCompatHtmlPath = resolve(viewerRoot, "software_safe.html");
const staleRootCompatBundlePath = resolve(viewerRoot, "software_safe.js");
const staleRootCompatClaimEvidencePath = resolve(viewerRoot, "software_safe_first_agent_claim_evidence.html");
const pixelWorldRuntimeDir = resolve(viewerDistDir, "pixel-world-bridge");
const pixelWorldRuntimeSelectorSourcePath = resolve(softwareSafeSrcDir, "pixel_world_runtime_module_selector.js");
const pixelWorldRuntimeModulePath = resolve(pixelWorldRuntimeDir, "pixel_world_bridge.js");
const canonicalBundleBanner = "// Generated canonical Viewer bundle; source truth lives in ./software_safe_src/.\n";
const pixelWorldCompiledWasmPath = resolve(
  workspaceRoot,
  "target",
  "wasm32-unknown-unknown",
  "release",
  "pixel_world_bridge.wasm",
);
const cargoHomeDir = process.env.CARGO_HOME
  ? resolve(process.env.CARGO_HOME)
  : resolve(process.env.HOME || workspaceRoot, ".cargo");
const cargoBinDir = resolve(cargoHomeDir, "bin");
const wasmBindgenCliPath = resolve(cargoBinDir, "wasm-bindgen");
const cargoLockPath = resolve(workspaceRoot, "Cargo.lock");

function runChecked(command, args, options = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      cwd: options.cwd || workspaceRoot,
      stdio: "inherit",
      env: options.env || process.env,
    });
    child.on("error", rejectPromise);
    child.on("exit", (code) => {
      if (code === 0) {
        resolvePromise();
        return;
      }
      rejectPromise(new Error(`${command} ${args.join(" ")} exited with code ${code}`));
    });
  });
}

async function findWasmBindgenVersion() {
  const lockContents = await readFile(cargoLockPath, "utf8");
  const match = lockContents.match(/\[\[package\]\]\s+name = "wasm-bindgen"\s+version = "([^"]+)"/m);
  if (!match?.[1]) {
    throw new Error(`failed to resolve wasm-bindgen version from ${cargoLockPath}`);
  }
  return match[1];
}

async function wasmBindgenVersionMatches(commandPath, expectedVersion) {
  if (!commandPath) {
    return false;
  }
  try {
    await access(commandPath);
  } catch {
    return false;
  }

  return await new Promise((resolvePromise) => {
    const child = spawn(commandPath, ["--version"], {
      cwd: workspaceRoot,
      stdio: ["ignore", "pipe", "ignore"],
      env: process.env,
    });
    let stdout = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.on("error", () => resolvePromise(false));
    child.on("exit", (code) => {
      resolvePromise(code === 0 && stdout.trim() === `wasm-bindgen ${expectedVersion}`);
    });
  });
}

async function resolveWasmBindgenCommand() {
  const wasmBindgenVersion = await findWasmBindgenVersion();
  const cachedRoot = resolve(
    process.env.XDG_CACHE_HOME || resolve(process.env.HOME || workspaceRoot, ".cache"),
    "oasis7",
    "wasm-bindgen-cli",
    wasmBindgenVersion,
  );
  const cachedCliPath = resolve(cachedRoot, "bin", "wasm-bindgen");

  if (await wasmBindgenVersionMatches(cachedCliPath, wasmBindgenVersion)) {
    return cachedCliPath;
  }
  if (await wasmBindgenVersionMatches(process.env.WASM_BINDGEN_BIN, wasmBindgenVersion)) {
    return process.env.WASM_BINDGEN_BIN;
  }
  if (await wasmBindgenVersionMatches(wasmBindgenCliPath, wasmBindgenVersion)) {
    return wasmBindgenCliPath;
  }
  throw new Error(
    `missing wasm-bindgen ${wasmBindgenVersion}; run ./scripts/ensure-wasm-bindgen-cli.sh --print-bin and retry the viewer build`,
  );
}

async function listFilesRecursively(dirPath) {
  const entries = await readdir(dirPath, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const entryPath = resolve(dirPath, entry.name);
    if (entry.isDirectory()) {
      files.push(...await listFilesRecursively(entryPath));
      continue;
    }
    if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

async function canonicalBundleContents() {
  const bundle = await readFile(builtBundlePath, "utf8");
  return bundle.startsWith(canonicalBundleBanner)
    ? bundle
    : `${canonicalBundleBanner}${bundle}`;
}

async function buildPixelWorldRuntimeVariant({ featureName, backendDirName }) {
  await runChecked("env", [
    "-u",
    "RUSTC_WRAPPER",
    "cargo",
    "build",
    "-p",
    "pixel_world_bridge",
    "--target",
    "wasm32-unknown-unknown",
    "--release",
    "--no-default-features",
    "--features",
    featureName,
  ]);
  await access(pixelWorldCompiledWasmPath);
  const outputDir = resolve(pixelWorldRuntimeDir, backendDirName);
  await mkdir(outputDir, { recursive: true });
  const runtimeModuleSourcePath = resolve(softwareSafeSrcDir, "pixel_world_runtime_module_wasm.js");
  await copyFile(runtimeModuleSourcePath, resolve(outputDir, "pixel_world_bridge.js"));
  const wasmBindgenCommand = await resolveWasmBindgenCommand();
  await runChecked(wasmBindgenCommand, [
    "--target",
    "web",
    ...(!visualTest ? ["--remove-name-section", "--remove-producers-section"] : []),
    "--out-dir",
    outputDir,
    "--out-name",
    "pixel_world_bridge_bindgen",
    pixelWorldCompiledWasmPath,
  ]);
  const bindingsPath = resolve(outputDir, "pixel_world_bridge_bindgen.js");
  const bindings = await readFile(bindingsPath, "utf8");
  await writeFile(bindingsPath, isolateBindgenModule(bindings), "utf8");
}

await access(canonicalHtmlPath);
await access(builtBundlePath);
const emittedFiles = (await listFilesRecursively(tempOutDir))
  .map((filePath) => relative(tempOutDir, filePath))
  .sort();
if (emittedFiles.length !== 1 || emittedFiles[0] !== "viewer.js") {
  throw new Error(`unexpected viewer canonical bundle outputs: ${emittedFiles.join(", ") || "(none)"}`);
}
const canonicalBundle = await canonicalBundleContents();
if (!visualTest) await writeFile(finalCanonicalBundlePath, canonicalBundle, "utf8");
await mkdir(viewerDistDir, { recursive: true });
await copyFile(canonicalHtmlPath, resolve(viewerDistDir, "viewer.html"));
await writeFile(finalDistBundlePath, canonicalBundle, "utf8");
await writeViewerCompatAliases(viewerRoot, viewerDistDir);
if (visualTest) await Promise.all(["viewer_terminal_shell.css", "favicon.ico", "viewer_first_agent_claim_evidence.html"].map(name => copyFile(resolve(viewerRoot, name), resolve(viewerDistDir, name))));
if (!visualTest) await Promise.all([
  rm(staleRootCompatHtmlPath, { force: true }),
  rm(staleRootCompatBundlePath, { force: true }),
  rm(staleRootCompatClaimEvidencePath, { force: true }),
]);
await rm(pixelWorldRuntimeDir, { recursive: true, force: true });
await mkdir(pixelWorldRuntimeDir, { recursive: true });
await copyFile(pixelWorldRuntimeSelectorSourcePath, pixelWorldRuntimeModulePath);
await buildPixelWorldRuntimeVariant({
  featureName: "webgl2_runtime",
  backendDirName: "webgl2",
});
const projectionReports = await verifyPixelWorldProjection({
  bindingsPath: resolve(pixelWorldRuntimeDir, "webgl2", "pixel_world_bridge_bindgen.js"),
  wasmPath: resolve(pixelWorldRuntimeDir, "webgl2", "pixel_world_bridge_bindgen_bg.wasm"),
});
console.log(`pixel world projection ABI verified: ${JSON.stringify(projectionReports)}`);
await rm(tempOutDir, { recursive: true, force: true });

console.log(`software_safe build finalized: ${finalCanonicalBundlePath}`);
