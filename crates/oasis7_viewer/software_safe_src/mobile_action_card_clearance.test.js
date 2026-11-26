import { readExpandedCssSync } from "./test_support/read_expanded_css.js";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("mobile gameplay action card clearance", () => {
  const readCombinedStyles = () => readExpandedCssSync("viewer_base.css");

  it("keeps targeted action cards below the sticky mobile jump rail", () => {
    const viewerHtml = readCombinedStyles();

    for (const html of [viewerHtml]) {
      expect(html).toMatch(
        /\.event-card--action\s*\{[^}]*scroll-margin-top:\s*var\(--mobile-action-card-clearance,\s*0\);/,
      );
      expect(html).toMatch(
        /@media \(max-width: 1240px\)[\s\S]*?--mobile-action-card-clearance:\s*84px;/,
      );
    }
  });

  it("removes nonessential interactive motion when players prefer reduced motion", () => {
    const viewerHtml = readCombinedStyles();

    for (const html of [viewerHtml]) {
      const reducedMotionStart = html.lastIndexOf("@media (prefers-reduced-motion: reduce)");
      const primaryActionTransition = html.indexOf(
        "transition:",
        html.indexOf(".pixel-world-command-cell--next {"),
      );
      const helpPanelTransition = html.indexOf(
        "transition:",
        html.indexOf(".inline-help-tip__panel {"),
      );

      expect(html).toMatch(
        /@media \(prefers-reduced-motion: reduce\)[\s\S]*?button,[\s\S]*?\.pixel-world-command-cell--next,[\s\S]*?\.inline-help-tip__panel\s*\{\s*transition:\s*none;/,
      );
      expect(html).toMatch(
        /@media \(prefers-reduced-motion: reduce\)[\s\S]*?button:not\(:disabled\):hover,[\s\S]*?\.pixel-world-command-cell--next:active\s*\{\s*transform:\s*none;/,
      );
      expect(reducedMotionStart).toBeGreaterThan(-1);
      expect(primaryActionTransition).toBeGreaterThan(-1);
      expect(helpPanelTransition).toBeGreaterThan(-1);
      expect(reducedMotionStart).toBeGreaterThan(primaryActionTransition);
      expect(reducedMotionStart).toBeGreaterThan(helpPanelTransition);
    }
  });
});
