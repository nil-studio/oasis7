import { resolve } from "node:path";
import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

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
    maxWorkers: 1,
    setupFiles: ["./test/setup.js"],
    include: [
      "software_safe_src/**/*.test.js",
      "software_safe_src/**/*.test.jsx",
    ],
    environmentOptions: {
      jsdom: {
        url: "http://127.0.0.1:4173/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en",
      },
    },
  },
});
