import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

function cssRules(source, selectorPattern) {
  const rules = [];
  const rulePattern = /([^{}]+)\{([^{}]*)\}/g;
  let match;
  while ((match = rulePattern.exec(source)) !== null) {
    if (selectorPattern.test(match[1])) {
      rules.push({ selector: match[1].trim(), declarations: match[2] });
    }
  }
  return rules;
}

function hasDeclaration(rule, property, valuePattern) {
  const declarationPattern = new RegExp(`(?:^|[;\\s])${property}\\s*:\\s*([^;}]*)`, "i");
  const match = rule?.declarations?.match(declarationPattern);
  return Boolean(match && (!valuePattern || valuePattern.test(match[1])));
}

function numericDeclaration(rule, property) {
  const declarationPattern = new RegExp(`(?:^|[;\\s])${property}\\s*:\\s*([0-9]+)`, "i");
  return Number(rule?.declarations?.match(declarationPattern)?.[1]);
}

function findRule(source, selectorPattern) {
  return cssRules(source, selectorPattern)[0] || null;
}

async function readViewerHtml() {
  const [viewerHtml, terminalShellCss] = await Promise.all([
    readFile("viewer.html", "utf8"),
    readFile("viewer_terminal_shell.css", "utf8"),
  ]);
  return { viewerHtml, terminalShellCss };
}

describe("fullscreen map shell contract", () => {
  it("makes the default and visual-fixture shell a viewport-sized map base", async () => {
    const { viewerHtml, terminalShellCss } = await readViewerHtml();

    for (const html of [`${viewerHtml}\n${terminalShellCss}`]) {
      expect(html.includes('id="app" class="viewer-shell" data-viewer-shell="player-fullscreen"')).toBe(true);
      expect(html.includes('#viewer-stage-panel[data-viewer-map-layer="base"]')).toBe(true);

      const shell = findRule(html, /\.viewer-shell(?:\[|\s|,|$)/);
      expect(shell).not.toBeNull();
      expect(hasDeclaration(shell, "position", /relative|absolute|fixed/)).toBe(true);
      expect(hasDeclaration(shell, "min-height", /100dvh|100vh/)).toBe(true);
      expect(hasDeclaration(shell, "overflow-x", /hidden|clip/)).toBe(true);
      expect(shell.declarations).not.toMatch(/display\s*:\s*grid/i);
      expect(shell.declarations).not.toMatch(/grid-template-columns\s*:/i);

      const stage = findRule(html, /#viewer-stage-panel\[data-viewer-map-layer=["']base["']\]/);
      expect(stage).not.toBeNull();
      expect(hasDeclaration(stage, "position", /absolute|fixed/)).toBe(true);
      expect(hasDeclaration(stage, "inset", /0/)).toBe(true);
      expect(hasDeclaration(stage, "min-height", /100dvh|100vh/)).toBe(true);
      expect(hasDeclaration(stage, "height", /100dvh|100vh/)).toBe(true);
    }
  });

  it("does not reintroduce a visual-fixture three-column shell exception", async () => {
    const { viewerHtml, terminalShellCss } = await readViewerHtml();
    for (const html of [`${viewerHtml}\n${terminalShellCss}`]) {
      const fixtureShellGridRules = cssRules(
        html,
        /#app\[data-viewer-visual-fixture(?:=[^\]]+)?\][^{}]*\.(?:shell|viewer-shell)/,
      ).filter((rule) => /grid-template-columns\s*:/.test(rule.declarations));
      expect(fixtureShellGridRules).toEqual([]);
      expect(html).not.toMatch(
        /#app\[data-viewer-visual-fixture(?:=[^\]]+)?\]\s+\.(?:shell|viewer-shell)\s*\{[^}]*display\s*:\s*grid/i,
      );
    }
  });

  it("does not let visual fixtures override the responsive command-band layout", async () => {
    const { viewerHtml } = await readViewerHtml();
    for (const html of [viewerHtml]) {
      const fixtureCommandRules = cssRules(
        html,
        /#app\[data-viewer-visual-fixture(?:=[^\]]+)?\][^{}]*\.pixel-world-command-strip/,
      ).filter((rule) => /grid-template-columns\s*:/.test(rule.declarations));
      expect(fixtureCommandRules).toEqual([]);
    }
  });

  it("keeps map HUD, next move, receipt, feed, and navigation in overlay layers", async () => {
    const { viewerHtml, terminalShellCss } = await readViewerHtml();
    for (const html of [`${viewerHtml}\n${terminalShellCss}`]) {
      for (const overlayName of ["world-hud", "next-move", "receipt", "feed", "navigation"]) {
        const rule = findRule(html, new RegExp(`\\[data-viewer-overlay=["']${overlayName}["']\\]`));
        expect(rule, `missing overlay rule for ${overlayName}`).not.toBeNull();
        expect(hasDeclaration(rule, "position", /absolute|fixed|sticky/), overlayName).toBe(true);
        expect(hasDeclaration(rule, "z-index", /[1-9]/), overlayName).toBe(true);
      }
    }
  });

  it("keeps Focus drawers interactive inside the pointer-transparent world HUD", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const drawerRule = findRule(
      terminalShellCss,
      /\[data-viewer-overlay=["']world-hud["']\]\s+\.pixel-world-focus-drawer/,
    );
    expect(drawerRule).not.toBeNull();
    expect(hasDeclaration(drawerRule, "pointer-events", /auto/)).toBe(true);
  });

  it("keeps Targets and Command as fixed drawers with stable route anchors", async () => {
    const { viewerHtml, terminalShellCss } = await readViewerHtml();
    for (const html of [`${viewerHtml}\n${terminalShellCss}`]) {
      expect(html.includes('data-viewer-route-panel="targets"')).toBe(true);
      expect(html.includes('data-viewer-route-panel="command"')).toBe(true);
      for (const routePanel of ["targets", "command"]) {
        const rule = findRule(html, new RegExp(`\\[data-viewer-route-panel=["']${routePanel}["']\\]`));
        expect(rule, `missing drawer rule for ${routePanel}`).not.toBeNull();
        expect(hasDeclaration(rule, "position", /fixed|absolute/), routePanel).toBe(true);
        expect(hasDeclaration(rule, "z-index", /[1-9]/), routePanel).toBe(true);
      }
    }
  });

  it("uses viewport map geometry and on-demand bottom sheets on mobile without horizontal overflow", async () => {
    const { viewerHtml, terminalShellCss } = await readViewerHtml();
    for (const html of [`${viewerHtml}\n${terminalShellCss}`]) {
      const mobileBlock = `${terminalShellCss}\n${html}`.match(/@media\s*\(max-width:\s*1240px\)[\s\S]*?(?=@media|<\/style>|$)/i)?.[0] || "";
      expect(/\.viewer-shell[^{]*\{[^}]*min-height\s*:\s*(?:100dvh|100vh)/i.test(mobileBlock)).toBe(true);
      expect(/\.pixel-world-canvas[^{]*\{[^}]*position\s*:\s*(?:absolute|fixed)/i.test(mobileBlock)).toBe(true);
      expect(/\.pixel-world-canvas[^{]*\{[^}]*inset\s*:\s*0/i.test(mobileBlock)).toBe(true);
      for (const routePanel of ["targets", "command"]) {
        const sheetRule = mobileBlock.match(
          new RegExp(`\\[data-viewer-route-panel=["']${routePanel}["']\\][^{]*\\{[^}]*\\}`),
        )?.[0] || "";
        const baseRule = findRule(html, new RegExp(`\\[data-viewer-route-panel=["']${routePanel}["']\\]`));
        const sheetDeclarations = `${baseRule?.declarations || ""} ${sheetRule}`;
        expect(/position\s*:\s*fixed/i.test(sheetDeclarations), routePanel).toBe(true);
        expect(/bottom\s*:|inset\s*:[^;]*\d/i.test(sheetDeclarations), routePanel).toBe(true);
        expect(/max-height\s*:/i.test(sheetDeclarations), routePanel).toBe(true);
      }
    }
  });

  it("keeps expanded Feed usable in short landscape viewports", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*1240px\)\s*and\s*\(max-height:\s*640px\)[\s\S]*?\[data-viewer-overlay="feed"\]\[open\][\s\S]*?top:\s*\d+px;[\s\S]*?bottom:\s*calc\([^;]+\);[\s\S]*?max-height:\s*calc\(100dvh[^;]*\)/i,
    );
  });

  it("compacts short-landscape Feed chrome while preserving its title and status", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*1240px\)\s*and\s*\(max-height:\s*640px\)[\s\S]*?\[data-viewer-overlay="feed"\]\[open\]\s+\.panel__eyebrow,[\s\S]*?\[data-viewer-overlay="feed"\]\[open\]\s+\.panel__meta-copy\s*\{[\s\S]*?display:\s*none/i,
    );
  });

  it("keeps the mobile Feed band below top chrome and outside the decision band", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*640px\)[\s\S]*?\.stack\s*>\s*\[data-viewer-overlay="feed"\]\s*\{[\s\S]*?position:\s*absolute;[\s\S]*?top:\s*132px;[\s\S]*?bottom:\s*auto;[\s\S]*?max-height:\s*min\(20dvh,\s*128px\)/i,
    );
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*640px\)[\s\S]*?\.pixel-world-decision-area\s*\{[\s\S]*?max-height:\s*calc\(100dvh\s*-\s*276px\);[\s\S]*?align-content:\s*start;/i,
    );
  });

  it("leaves a mobile gap between navigation, Cinematic, and Feed", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*640px\)[\s\S]*?\.secondary-viewer-nav\s*\{[\s\S]*?top:\s*56px;[\s\S]*?\[data-viewer-overlay="cinematic-entry"\]\s*\{[\s\S]*?top:\s*80px;/i,
    );
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*640px\)[\s\S]*?\.stack\s*>\s*\[data-viewer-overlay="feed"\]\s*\{[\s\S]*?top:\s*132px;/i,
    );
  });

  it("bounds short-landscape Next Move content inside its receipt-safe band", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(
      /@media\s*\(max-width:\s*1240px\)\s*and\s*\(max-height:\s*640px\)[\s\S]*?\[data-viewer-overlay="next-move"\][\s\S]*?height:\s*min\(42dvh,\s*calc\(100dvh\s*-\s*72px\s*-\s*min\(12dvh,\s*48px\)\s*-\s*16px\s*-\s*8px\s*-\s*96px\)\);[\s\S]*?max-height:\s*none;[\s\S]*?overflow-y:\s*auto[\s\S]*?\[data-viewer-overlay="next-move"\]\s+\[data-shell-region="next-move-primary"\][\s\S]*?overflow-y:\s*auto/i,
    );
  });

  it("reserves a meaningful Feed body at both supported short-landscape heights", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(
      /height:\s*min\(42dvh,\s*calc\(100dvh\s*-\s*72px\s*-\s*min\(12dvh,\s*48px\)\s*-\s*16px\s*-\s*8px\s*-\s*96px\)\)/i,
    );
    const feedSummaryHeight = 64;
    const feedBodyMinimum = 32;
    for (const viewportHeight of [390, 360]) {
      const receiptHeight = Math.min(viewportHeight * 0.12, 48);
      const nextMoveHeight = Math.min(
        viewportHeight * 0.42,
        viewportHeight - 72 - receiptHeight - 16 - 8 - feedSummaryHeight - feedBodyMinimum,
      );
      const feedHeight = viewportHeight - 72 - receiptHeight - 16 - 8 - nextMoveHeight;
      expect(feedHeight - feedSummaryHeight, `${viewportHeight}px`).toBeGreaterThanOrEqual(feedBodyMinimum);
    }
  });

  it("keeps hotspots above selected entity markers for pointer inspection", async () => {
    const { viewerHtml } = await readViewerHtml();
    for (const html of [viewerHtml]) {
      const hotspot = findRule(html, /\.pixel-world-hotspot(?:\s|$)/);
      expect(numericDeclaration(hotspot, "border-radius")).toBe(0);
      const close = findRule(html, /\.pixel-world-canvas__hotspot-tooltip-close(?:\s|$)/);
      expect(numericDeclaration(close, "min-width")).toBe(44);
      expect(numericDeclaration(close, "min-height")).toBe(44);
      expect(html).not.toMatch(/\.pixel-world-hotspot\s*\{[^}]*box-shadow:\s*0/);
      expect(numericDeclaration(findRule(html, /\.pixel-world-hotspot__glyph(?:\s|$)/), "border-radius")).toBe(999);
      const selectedEntity = findRule(html, /\.pixel-world-entity\[data-selected="true"\],/);
      expect(numericDeclaration(hotspot, "z-index")).toBeGreaterThan(numericDeclaration(selectedEntity, "z-index"));
    }
  });

  it("keeps the desktop primary rail visible in the default fullscreen Player shell", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const navigationRule = findRule(terminalShellCss, /\[data-viewer-overlay=["']navigation["']\]/);
    expect(navigationRule).not.toBeNull();
    expect(hasDeclaration(navigationRule, "display", /flex|block|inline-flex/)).toBe(true);
  });

  it("keeps Objective and Player Leverage visible in the default fullscreen Player HUD", async () => {
    const { terminalShellCss } = await readViewerHtml();

    for (const cellSelector of [
      /\.pixel-world-command-cell--objective/,
      /\.pixel-world-command-cell--leverage/,
    ]) {
      const hiddenRules = cssRules(terminalShellCss, cellSelector)
        .filter((rule) => hasDeclaration(rule, "display", /none/));
      expect(hiddenRules, `HUD cell is hidden: ${cellSelector}`).toEqual([]);
    }
  });

  it("keeps Agent Context decision fields compact, readable, and non-disclosed on narrow screens", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const decisionRule = findRule(
      terminalShellCss,
      /\.agent-context-lite__decision\s+\.agent-context-lite__gameplay/,
    );
    expect(decisionRule).not.toBeNull();
    expect(hasDeclaration(decisionRule, "grid-template-columns", /repeat\(2\s*,\s*minmax\(0\s*,\s*1fr\)/i)).toBe(true);

    const fieldRule = findRule(terminalShellCss, /\.agent-context-lite__field/);
    expect(fieldRule).not.toBeNull();
    expect(hasDeclaration(fieldRule, "min-width", /0/)).toBe(true);
    expect(hasDeclaration(fieldRule, "overflow-wrap", /anywhere/)).toBe(true);

    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(
      /\.agent-context-lite__decision\s+\.agent-context-lite__gameplay\s*\{[^}]*grid-template-columns\s*:\s*minmax\(0\s*,\s*1fr\)/i,
    );
    expect(mobileBlock).toMatch(
      /\.agent-context-lite__truth\s+\.agent-activity__field\s*\{[^}]*display\s*:\s*none/i,
    );
    expect(mobileBlock).not.toMatch(
      /\.agent-context-lite__truth\s+\.agent-activity__state\s*\{[^}]*display\s*:\s*none/i,
    );
    expect(mobileBlock).toMatch(
      /\.agent-context-lite__decision\s+\.agent-context-lite__gameplay\s*\{[^}]*gap\s*:\s*5px/i,
    );
    expect(mobileBlock).not.toMatch(
      /\.agent-context-lite__field(?:[^{}]*)\{[^}]*display\s*:\s*none/i,
    );
    expect(mobileBlock).not.toMatch(
      /\.agent-context-lite__decision(?:[^{}]*)\{[^}]*display\s*:\s*none/i,
    );
  });

  it("keeps the compact authoritative world readout visible outside Cinematic View", async () => {
    const { viewerHtml, terminalShellCss } = await readViewerHtml();
    expect(viewerHtml).toMatch(/data-viewer-shell="player-fullscreen"/);
    const readoutRule = findRule(
      terminalShellCss,
      /\[data-viewer-shell=["']player-fullscreen["']\][^{}]*\.pixel-world-readout/,
    );
    expect(readoutRule, "default Player shell must place the compact world readout").not.toBeNull();
    expect(hasDeclaration(readoutRule, "display", /flex/)).toBe(true);

    const cinematicRule = findRule(
      terminalShellCss,
      /\[data-viewer-shell=["']player-fullscreen["']\][^{}]*\.pixel-world-host--focus[^{}]*\.pixel-world-readout/,
    );
    expect(cinematicRule, "Cinematic View must hide the world readout").not.toBeNull();
    expect(hasDeclaration(cinematicRule, "display", /none/)).toBe(true);
  });

  it("keeps an independent Cinematic View entry visible in the fullscreen Player shell", async () => {
    const [hostSource, terminalShellCss] = await Promise.all([
      readFile("software_safe_src/pixel_world_host.jsx", "utf8"),
      readFile("viewer_terminal_shell.css", "utf8"),
    ]);
    expect(hostSource).toContain('data-viewer-overlay="cinematic-entry"');
    const entryRule = findRule(
      terminalShellCss,
      /\[data-viewer-overlay=["']cinematic-entry["']\]/,
    );
    expect(entryRule, "fullscreen Player must position the Cinematic View entry independently").not.toBeNull();
    expect(hasDeclaration(entryRule, "display", /flex|block|inline-flex/)).toBe(true);
    expect(hasDeclaration(entryRule, "pointer-events", /auto/)).toBe(true);
    expect(terminalShellCss).not.toMatch(
      /\.pixel-world-host__summary[^{}]*\[data-viewer-overlay=["']cinematic-entry["'][^{}]*\{[^}]*display\s*:\s*none/i,
    );
  });

  it("reserves the More safe area for desktop readout and Cinematic controls", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const readoutRule = findRule(
      terminalShellCss,
      /\[data-viewer-overlay=["']world-hud["']\]\s+\.pixel-world-canvas__selection\s*$/,
    );
    expect(readoutRule).not.toBeNull();
    expect(hasDeclaration(readoutRule, "right", /280px/)).toBe(true);

    const focusHudRule = findRule(terminalShellCss, /\.pixel-world-host--focus\s+\.pixel-world-focus-hud/);
    expect(focusHudRule).not.toBeNull();
    expect(hasDeclaration(focusHudRule, "right", /96px/)).toBe(true);
  });

  it("narrows the mobile Cinematic objective away from More without changing shell overflow", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(
      /\.pixel-world-host--focus\s+\.pixel-world-focus-hud__cell--prompt\s*\{[^}]*margin-right:\s*80px/i,
    );
    expect(terminalShellCss).toMatch(/\.viewer-shell\s*\{[^}]*overflow-x:\s*hidden/i);
  });

  it("keeps the mobile Cinematic entry and compact world readout in separate safe-area bands", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    const entryTop = Number(mobileBlock.match(/\[data-viewer-overlay=["']cinematic-entry["']\][^{]*\{[^}]*top:\s*(\d+)px/i)?.[1]);
    const readoutRule = findRule(mobileBlock, /\[data-viewer-shell=["']player-fullscreen["']\]\s+\.pixel-world-readout/);
    const readoutTop = Number(readoutRule?.declarations.match(/top\s*:\s*(\d+)px/i)?.[1]);
    expect(Number.isFinite(entryTop)).toBe(true);
    expect(Number.isFinite(readoutTop) || hasDeclaration(readoutRule, "position", /static/)).toBe(true);
    if (Number.isFinite(readoutTop)) expect(readoutTop).toBeGreaterThanOrEqual(entryTop + 44);
  });

  it("does not leave the low-priority mobile readout under the Feed overlay", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    const readoutRule = findRule(
      mobileBlock,
      /\[data-viewer-shell=["']player-fullscreen["']\]\s+\.pixel-world-readout/,
    );
    const feedRule = findRule(mobileBlock, /\[data-viewer-overlay=["']feed["']\]/);
    expect(readoutRule, "mobile Player readout must have an explicit safe-area policy").not.toBeNull();
    expect(feedRule, "mobile Feed must have an explicit safe-area policy").not.toBeNull();

    const readoutIsHidden = hasDeclaration(readoutRule, "display", /none/);
    const readoutIsStatic = hasDeclaration(readoutRule, "position", /static/);
    const readoutTop = Number(readoutRule?.declarations.match(/top\s*:\s*(\d+)px/i)?.[1]);
    const feedTop = Number(feedRule?.declarations.match(/top\s*:\s*(\d+)px/i)?.[1]);
    const bandsAreSeparated = Number.isFinite(readoutTop)
      && Number.isFinite(feedTop)
      && feedTop >= readoutTop + 44;

    expect(
      readoutIsHidden || readoutIsStatic || bandsAreSeparated,
      "mobile Feed and the low-priority world readout must be hidden or occupy disjoint vertical bands",
    ).toBe(true);
  });

  it("moves the selected mobile marker into the ordinary Shell safe area", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(/@media\s*\(max-width:\s*640px\)[\s\S]*?\[data-viewer-overlay=["']next-move["']\]\s*\{[^}]*overflow-y\s*:\s*auto/i);
    const safeAreaSource = await readFile("software_safe_src/pixel_world_mobile_safe_area.js", "utf8");
    expect(safeAreaSource).toContain("commandTop - SAFE_AREA_GAP_PX - markerBottom");
    expect(safeAreaSource).toContain("feedBottom + SAFE_AREA_GAP_PX - markerTop");
  });

  it("keeps mobile selection, Feed, legend, and status surfaces in distinct presentation bands", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(
      /\[data-viewer-overlay=["']world-hud["']\]\s+\.pixel-world-canvas__selection\s*\{[^}]*top:\s*264px/i,
    );
    expect(mobileBlock).toMatch(
      /\.pixel-world-decision-area\s+\.pixel-world-canvas__legend\s*\{[^}]*margin:\s*0\s+0\s+0\s+auto/i,
    );
    expect(mobileBlock).toMatch(
      /\[data-viewer-shell=["']player-fullscreen["']\]\s+\.pixel-world-readout\s*\{[^}]*position:\s*static[^}]*display:\s*flex/i,
    );
    expect(terminalShellCss).toMatch(/\.pixel-world-canvas__sparse-guidance\s*\{/i);
  });

  it("uses severity width and lifecycle line styles as truthful crisis shape cues", async () => {
    const { terminalShellCss } = await readViewerHtml();
    expect(terminalShellCss).toMatch(/\.world-feed__event--severity-4\s*\{[^}]*border-left-color/i);
    expect(terminalShellCss).toMatch(/\.world-feed__event--lifecycle-resolved\s*\{[^}]*border-left-style:\s*dashed/i);
    expect(terminalShellCss).toMatch(/\.world-feed__event--lifecycle-timed_out\s*\{[^}]*border-left-style:\s*dotted/i);
  });

  it("keeps the narrow Command context row sticky while the route panel scrolls independently", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*1240px\)[\s\S]*?(?=@media\s*\(max-width:\s*640px\))/i)?.[0] || "";
    expect(terminalShellCss).toMatch(/overflow-y:\s*auto/i);
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s+\.command-surface__target-row\s*\{[^}]*position:\s*sticky/i,
    );
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s+\.command-surface__target-row\s*\{[^}]*top:\s*var\(--command-route-chrome-height/i,
    );
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s*\{[^}]*max-height:\s*min\(80dvh,\s*660px\)/i,
    );
  });

  it("keeps the Command route chrome and truthful context available at scroll bottom", async () => {
    const [{ terminalShellCss }, mainSource] = await Promise.all([
      readViewerHtml(),
      readFile("software_safe_src/viewer_app.jsx", "utf8"),
    ]);
    expect(mainSource).toMatch(
      /id="viewer-details-panel"[\s\S]*?class="panel__header panel__header--stack command-route-chrome"[\s\S]*?href="#viewer-stage-panel"/i,
    );

    const chromeRule = findRule(
      terminalShellCss,
      /#viewer-details-panel\s*>\s*\.command-route-chrome/,
    );
    expect(chromeRule, "Command route chrome must stay visible while the drawer scrolls").not.toBeNull();
    expect(hasDeclaration(chromeRule, "position", /sticky/)).toBe(true);
    expect(hasDeclaration(chromeRule, "top", /0/)).toBe(true);
    expect(hasDeclaration(chromeRule, "z-index", /[1-9]/)).toBe(true);
    expect(hasDeclaration(chromeRule, "background", /var\(--panel-bg/)).toBe(true);

    const responsiveChrome = terminalShellCss.match(
      /@media\s*\(max-width:\s*1240px\)[\s\S]*?(?=@media|$)/i,
    )?.[0] || "";
    expect(responsiveChrome).toMatch(
      /#viewer-details-panel\s+\.command-surface__target-row\s*\{[^}]*top:\s*var\(--command-route-chrome-height/i,
    );
    expect(responsiveChrome).toMatch(
      /#viewer-details-panel\s+\.command-surface__target-row\s*\{[^}]*position:\s*sticky/i,
    );
    expect(mainSource).toMatch(
      /class="badge-row command-surface__target-row"[^>]*data-command-continuity="target"/i,
    );
    expect(mainSource).toMatch(
      /data-command-continuity="target"[\s\S]*?data-command-continuity="summary"[\s\S]*?Freshness[\s\S]*?Objective/i,
    );
    expect(mainSource).toMatch(
      /<AgentContextLite\s+model=\{selectedAgentContextModel\(\)\}/,
    );
  });

  it("keeps Hosted prompt rollback reachable in the true phone-width Command drawer", async () => {
    const [{ terminalShellCss }, mainSource] = await Promise.all([
      readViewerHtml(),
      readFile("software_safe_src/viewer_app.jsx", "utf8"),
    ]);
    expect(mainSource).toMatch(/class="command-surface__prompt-panel"/);
    expect(mainSource).toMatch(/class="toolbar command-surface__rollback-actions"/);
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s+\.command-surface__prompt-panel\s+textarea\s*\{[^}]*min-height:\s*56px/i,
    );
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s+\.command-surface__rollback-actions\s*\{[^}]*display:\s*grid[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)\s+auto/i,
    );
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s+\.command-surface__rollback-actions\s+\.field--inline-flex\s*\{[^}]*min-width:\s*0/i,
    );
    expect(mobileBlock).toMatch(
      /#viewer-details-panel\s+\.command-surface__rollback-actions\s+\[data-prompt-action="rollback"\]:focus\s*\{[^}]*outline:\s*3px\s+solid\s+var\(--color-focus-ring\)/i,
    );
  });

  it("places unavailable fallback copy below navigation and keeps diagnostics folded", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const fallbackRule = findRule(terminalShellCss, /\[data-viewer-overlay=["']renderer-unavailable["']\]/);
    expect(fallbackRule).not.toBeNull();
    expect(hasDeclaration(fallbackRule, "top", /72px/)).toBe(true);
    expect(hasDeclaration(fallbackRule, "right", /96px/)).toBe(true);
    expect(hasDeclaration(fallbackRule, "white-space", /normal/)).toBe(true);
    expect(hasDeclaration(fallbackRule, "overflow-wrap", /anywhere/)).toBe(true);

    const diagnosticsRule = findRule(
      terminalShellCss,
      /\.pixel-world-render-diagnostics\[data-renderer-state=["']unavailable["']\]/,
    );
    expect(diagnosticsRule).not.toBeNull();
    expect(hasDeclaration(diagnosticsRule, "display", /block/)).toBe(true);
  });

  it("moves unavailable fallback copy and diagnostics below the mobile navigation", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(/\[data-viewer-overlay=["']feed["']\][^{]*\{[^}]*top:\s*104px/i);
    expect(mobileBlock).toMatch(/\[data-viewer-overlay=["']renderer-unavailable["']\][^{]*\{[^}]*top:\s*calc\(132px \+ min\(20dvh, 128px\) \+ 8px\)/i);
    expect(mobileBlock).toMatch(/\.pixel-world-render-diagnostics\[data-renderer-state=["']unavailable["']\][^{]*\{[^}]*top:\s*calc\(132px \+ min\(20dvh, 128px\) \+ 120px\)/i);
  });

  it("keeps a mobile More route for secondary Diagnostics without a narrow-screen hide rule", async () => {
    const [navigationSource, terminalShellCss] = await Promise.all([
      readFile("software_safe_src/viewer_navigation.jsx", "utf8"),
      readFile("viewer_terminal_shell.css", "utf8"),
    ]);
    expect(navigationSource).toMatch(/More/);
    expect(navigationSource).toMatch(/Diagnostics/);
    expect(terminalShellCss).not.toMatch(
      /@media\s*\(max-width:\s*640px\)[\s\S]*?\.secondary-viewer-nav\s*\{[^}]*display:\s*none/i,
    );
  });

  it("reserves tablet rail space for the secondary More control", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const tabletBlock = terminalShellCss.match(/@media\s*\(max-width:\s*1240px\)[\s\S]*?(?=@media\s*\(max-width:\s*640px\))/i)?.[0] || "";
    expect(tabletBlock).toMatch(/\[data-viewer-overlay=["']navigation["']\][^{]*\{[^}]*right\s*:\s*(?:9[0-9]|[1-9]\d{2,})px/i);
  });

  it("keeps all three mobile HUD meanings visible without a scroll-only third card", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(/\[data-viewer-overlay=["']next-move["']\][^{]*\{[^}]*grid-template-columns\s*:\s*repeat\(2/i);
    expect(mobileBlock).toMatch(/\.pixel-world-command-cell--next[^{]*\{[^}]*grid-column\s*:\s*1\s*\/\s*-1/i);
    expect(mobileBlock).not.toMatch(/\[data-viewer-overlay=["']next-move["']\][^{]*\{[^}]*overflow\s*:\s*auto/i);
  });

  it("keeps an expanded World Feed inside the safe area above Next Move and Action Receipt", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const tabletBlock = terminalShellCss.match(/@media\s*\(max-width:\s*1240px\)[\s\S]*?(?=@media\s*\(max-width:\s*640px\))/i)?.[0] || "";
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(tabletBlock).toMatch(/\[data-viewer-overlay=["']feed["']\]\[open\][^{]*\{[^}]*position\s*:\s*fixed/i);
    expect(tabletBlock).toMatch(/\[data-viewer-overlay=["']feed["']\]\[open\][^{]*\{[^}]*bottom\s*:\s*300px/i);
    expect(tabletBlock).toMatch(/\[data-viewer-overlay=["']feed["']\]\[open\][^{]*\{[^}]*overflow-y\s*:\s*auto/i);
    expect(mobileBlock).toMatch(/\[data-viewer-overlay=["']feed["']\]\[open\][^{]*\{[^}]*top\s*:\s*160px/i);
    expect(mobileBlock).toMatch(/\[data-viewer-overlay=["']feed["']\]\[open\][^{]*\{[^}]*bottom\s*:\s*calc\(144px\s*\+\s*min\(38dvh,\s*280px\)\s*\+\s*8px\)/i);
  });

  it("moves the selected chip when normal AppShell places World Summary between Host and Feed", async () => {
    const [{ terminalShellCss }, mainSource] = await Promise.all([
      readViewerHtml(),
      readFile("software_safe_src/viewer_app.jsx", "utf8"),
    ]);
    expect(mainSource).toMatch(/<PixelWorldHost[\s\S]*<WorldSummaryPanel[\s\S]*<WorldFeedSurface/);
    const tabletBlock = terminalShellCss.match(/@media\s*\(max-width:\s*1240px\)[\s\S]*?(?=@media\s*\(max-width:\s*640px\))/i)?.[0] || "";
    expect(tabletBlock).toMatch(/\.stack:has\(>\s*\[data-viewer-overlay=["']feed["']\]:not\(\[open\]\)\)\s*>\s*\[data-viewer-overlay=["']world-hud["']\][^{]*\{[^}]*top:\s*var\(--world-selection-feedback-top,\s*160px\)/i);
  });

  it("allows long Next Move and blocker copy to wrap and remain scrollable on mobile", async () => {
    const { terminalShellCss } = await readViewerHtml();
    const mobileBlock = terminalShellCss.match(/@media\s*\(max-width:\s*640px\)[\s\S]*$/i)?.[0] || "";
    expect(mobileBlock).toMatch(/\.pixel-world-command-cell--next\s+\.pixel-world-command-cell__detail[^{]*\{[^}]*display\s*:\s*block/i);
    expect(mobileBlock).toMatch(/\.pixel-world-command-cell--next\s+\.pixel-world-command-cell__detail[^{]*\{[^}]*overflow-y\s*:\s*auto/i);
    expect(mobileBlock).toMatch(/\.pixel-world-command-cell--next\s+\.pixel-world-command-cell__value[^{]*\{[^}]*display\s*:\s*block/i);
    expect(mobileBlock).toMatch(/\.pixel-world-command-cell--next\s+\.pixel-world-command-cell__value[^{]*\{[^}]*-webkit-line-clamp\s*:\s*unset/i);
    expect(mobileBlock).toMatch(/\.pixel-world-command-cell--next\s+\.pixel-world-command-cell__value[^{]*\{[^}]*overflow-wrap\s*:\s*anywhere/i);
  });
});

describe("headed visual smoke serving contract", () => {
  it("serves viewer CSS with text/css so fullscreen geometry is applied in the browser", async () => {
    const smokeSource = await readFile("scripts/pixel-world-fragment-visual-smoke.mjs", "utf8");
    expect(/case\s+["']\.css["']\s*:\s*return\s+["']text\/css(?:;\s*charset=utf-8)?["']/i.test(smokeSource)).toBe(true);
  });
});

describe("hotspot overlay contract", () => {
  it("renders hotspot explanations in a top-level overlay above expanded Feed", async () => {
    const [hotspotSource, hostSource, terminalShellCss] = await Promise.all([
      readFile("software_safe_src/pixel_world_hotspot.jsx", "utf8"),
      readFile("software_safe_src/pixel_world_host.jsx", "utf8"),
      readFile("viewer_terminal_shell.css", "utf8"),
    ]);
    expect(hostSource).toMatch(/PixelWorldHotspotTooltip/);
    expect(hotspotSource).toMatch(/<Portal>/);
    expect(hotspotSource).toMatch(/data-hotspot-tooltip/);

    const tooltipRule = findRule(terminalShellCss, /\.pixel-world-canvas__hotspot-tooltip/);
    expect(tooltipRule).not.toBeNull();
    expect(hasDeclaration(tooltipRule, "position", /fixed/)).toBe(true);
    expect(numericDeclaration(tooltipRule, "z-index")).toBeGreaterThan(60);
  });
});

describe("fullscreen map fixture composition", () => {
  it("retains fullscreen map and overlay anchors through fixture composition", async () => {
    const mainSource = await readFile("software_safe_src/viewer_app.jsx", "utf8");
    const pixelWorldHostSource = await readFile("software_safe_src/pixel_world_host.jsx", "utf8");
    const worldFeedSource = await readFile("software_safe_src/world_feed_panel.jsx", "utf8");
    const viewerNavigationSource = await readFile("software_safe_src/viewer_navigation.jsx", "utf8");

    expect(mainSource.includes('data-viewer-map-layer="base"')).toBe(true);
    expect(/data-viewer-overlay=.*navigation/.test(viewerNavigationSource)).toBe(true);
    expect(mainSource.includes('data-viewer-route-panel="targets"')).toBe(true);
    expect(mainSource.includes('data-viewer-route-panel="command"')).toBe(true);
    expect(pixelWorldHostSource.includes('data-viewer-overlay="world-hud"')).toBe(true);
    expect(pixelWorldHostSource.includes('data-viewer-overlay="next-move"')).toBe(true);
    expect(pixelWorldHostSource.includes('data-viewer-overlay="receipt"')).toBe(true);
    expect(worldFeedSource.includes('data-viewer-overlay="feed"')).toBe(true);
    expect(mainSource.includes("data-viewer-visual-fixture")).toBe(true);
  });
});
