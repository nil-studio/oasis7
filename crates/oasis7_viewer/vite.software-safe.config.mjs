import { resolve } from "node:path";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { viewerBuildPaths } from "./scripts/viewer-local-output.mjs";

export default defineConfig(({ mode }) => {
const visualTest = mode === "visual-test";
return {
  define: { "__OASIS7_VISUAL_TEST__": JSON.stringify(visualTest) },
  plugins: [solid(), {
    name: "release-excludes-visual-fixtures",
    enforce: "pre",
    resolveId(source) {
      if (!visualTest && /_visual_test_data\.js$/.test(source)) return "\0release-quote-fixture-disabled";
      if (!visualTest && /pixel_world_visual_fixture(?:_data)?\.js$/.test(source)) return "\0release-visual-fixture-disabled";
    },
    load(id) {
      if (id === "\0release-quote-fixture-disabled") return "export const VISUAL_FIXTURE_NAME = null; export const visualFixtureQuote = null;";
      if (id === "\0release-visual-fixture-disabled") return `
        export const pixelWorldTestApiEnabled = () => false;
        export const installPixelWorldVisualFixtureHook = () => null;
        export const installPixelWorldRenderDtoProbe = () => {};
        export const pixelWorldSelectedBlockerVisualFixture = () => null;
      `;
    },
  }],
  build: {
    target: "es2020",
    emptyOutDir: true,
    minify: false,
    sourcemap: false,
    outDir: viewerBuildPaths(visualTest ? "visual-test" : "release").bundleDir,
    lib: {
      entry: resolve(import.meta.dirname, visualTest ? "software_safe_src/main.visual-test.jsx" : "software_safe_src/main.jsx"),
      formats: ["es"],
      fileName: () => "viewer"
    },
    rolldownOptions: {
      output: {
        codeSplitting: false,
        entryFileNames: "viewer.js"
      }
    }
  }
};
});
