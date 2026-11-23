import { availableParallelism } from "node:os";
import { resolve } from "node:path";
import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

const NATIVE_REALM_TESTS = [
  "software_safe_src/browser_*.test.js",
  "software_safe_src/main.test.jsx",
];

export default defineConfig({
  plugins: [solid()],
  resolve: { alias: [
    { find: /^.*\/main\.jsx$/, replacement: resolve(import.meta.dirname, "test/viewer-app-entry.jsx") },
    { find: /^.*\/viewer_runtime_config_module\.js$/, replacement: resolve(import.meta.dirname, "test/viewer-runtime-config.js") },
  ] },
  define: { "__OASIS7_VISUAL_TEST__": "true" },
  test: {
    environment: "jsdom",
    // vi.resetModules() reloads viewer state between scenarios. Keep Solid in
    // Node's module cache so fresh components share the renderer's owner graph.
    server: { deps: { external: ["solid-js"] } },
    globals: true,
    maxWorkers: Math.max(1, Math.min(4, availableParallelism())),
    // Reuse compiled bytecode while every file keeps a fresh DOM and module graph.
    // Vitest disables this cache in workers when collecting V8 coverage.
    env: {
      NODE_COMPILE_CACHE: process.env.NODE_COMPILE_CACHE
        ?? resolve(import.meta.dirname, "node_modules/.cache/vitest-node-compile"),
    },
    setupFiles: ["./test/setup.js"],
    environmentOptions: {
      jsdom: {
        url: "http://127.0.0.1:4173/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en",
      },
    },
    projects: [
      {
        extends: true,
        test: {
          name: "viewer-native-realm",
          pool: "forks",
          isolate: true,
          include: NATIVE_REALM_TESTS,
        },
      },
      {
        extends: true,
        test: {
          name: "viewer-vm-jsdom",
          pool: "vmThreads",
          include: [
            "software_safe_src/**/*.test.js",
            "software_safe_src/**/*.test.jsx",
          ],
          exclude: NATIVE_REALM_TESTS,
        },
      },
    ],
  },
});
