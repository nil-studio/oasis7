import { readExpandedCss } from "./test_support/read_expanded_css.js";
import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

describe("Viewer warning feedback styling", () => {
  it("gives feedback-summary--warn a warm visible treatment without changing neutral feedback", async () => {
    const source = await readExpandedCss("viewer_base.css");
    const warningRule = source.match(/\.feedback-summary--warn\s*\{([^}]*)\}/)?.[1] || "";
    const neutralRule = source.match(/\.feedback-summary\s*\{([^}]*)\}/)?.[1] || "";

    expect(warningRule).toMatch(/border\s*:/);
    expect(warningRule).toMatch(/background\s*:/);
    expect(warningRule).toMatch(/color\s*:/);
    expect(warningRule).toMatch(/var\(--warn\)|223\s*,\s*182\s*,\s*107/);
    expect(neutralRule).not.toMatch(/var\(--warn\)|223\s*,\s*182\s*,\s*107/);
  });
});
