import { readExpandedCss } from "./test_support/read_expanded_css.js";
import { fireEvent, screen, within } from "@solidjs/testing-library";
import { readFile } from "node:fs/promises";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./pixel_world_host.jsx", () => ({ PixelWorldHost: () => <div data-testid="pixel-world-host" /> }));

let dispose = null;

beforeEach(async () => {
  vi.resetModules();
  window.history.replaceState({}, "", "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en");
  document.body.innerHTML = "";
  const core = await import("./legacy_core.js");
  const { mountViewerApp } = await import("./main.jsx");
  await core.initializeSoftwareSafeCore();
  const root = document.createElement("div");
  document.body.appendChild(root);
  dispose = mountViewerApp(root);
});

afterEach(() => {
  dispose?.();
  dispose = null;
  document.body.innerHTML = "";
});

describe("mobile jump rail", () => {
  it("keeps the terminal rail to World, Targets, and Command while demoting Diagnostics and removing Quote as a peer route", () => {
    const rail = screen.getByRole("navigation", { name: /primary entry section navigation/i });
    expect(Array.from(rail.querySelectorAll("a")).map((link) => link.textContent?.trim())).toEqual([
      "World",
      "Targets",
      "Command",
    ]);
    expect(within(rail).queryByRole("link", { name: "Diagnostics" })).not.toBeInTheDocument();
    expect(within(rail).queryByRole("link", { name: "Quote" })).not.toBeInTheDocument();

    const diagnosticsPanel = document.querySelector("#viewer-diagnostics-panel");
    expect(diagnosticsPanel).toBeTruthy();
    expect(diagnosticsPanel.closest("nav")).not.toBe(rail);
  });

  it("uses the canonical Chinese Command label 指令", async () => {
    const core = await import("./legacy_core.js");
    core.setSoftwareSafeLocale("zh");

    const rail = screen.getByRole("navigation", { name: /主入口分区导航/ });
    expect(within(rail).getByRole("link", { name: "世界" })).toBeInTheDocument();
    expect(within(rail).getByRole("link", { name: "目标" })).toBeInTheDocument();
    expect(within(rail).getByRole("link", { name: "指令" })).toBeInTheDocument();
    expect(within(rail).queryByRole("link", { name: "指挥" })).not.toBeInTheDocument();
  });

  it("starts Player Gameplay Details collapsed so the stage remains the first read", () => {
    const gameplayDetails = document.querySelector("#viewer-gameplay-details");
    expect(gameplayDetails).toBeTruthy();
    expect(gameplayDetails).not.toHaveAttribute("open");
    expect(gameplayDetails).toHaveProperty("open", false);
  });

  it("wraps rather than horizontally scrolling its quiet secondary links", async () => {
    const baseCss = await readExpandedCss("viewer_base.css");
    expect(baseCss).toMatch(/\.mobile-rail\s*\{[\s\S]*?position: sticky;[\s\S]*?top: 10px;[\s\S]*?flex-wrap: wrap; overflow: visible;/);
  });

  it("keeps World, Targets, and Command as primary links and excludes secondary peers", () => {
    const nav = screen.getByRole("navigation", { name: /primary entry section navigation/i });
    expect(Array.from(nav.querySelectorAll("a")).map((link) => link.textContent?.trim())).toEqual(["World", "Targets", "Command"]);
    expect(within(nav).getByRole("link", { name: "World" })).toHaveAttribute("href", "#viewer-stage-panel");
    expect(within(nav).getByRole("link", { name: "Targets" })).toHaveAttribute("href", "#viewer-targets-panel");
    expect(within(nav).getByRole("link", { name: "Command" })).toHaveAttribute("href", "#viewer-details-panel");
    expect(within(nav).queryByRole("link", { name: "Diagnostics" })).not.toBeInTheDocument();
    expect(within(nav).queryByRole("link", { name: "Quote" })).not.toBeInTheDocument();

    const secondaryNav = screen.getByRole("navigation", { name: /secondary viewer navigation/i });
    expect(within(secondaryNav).getByRole("button", { name: "More" })).toHaveAttribute(
      "aria-controls",
      "viewer-diagnostics-panel",
    );
  });

  it("marks Targets and Command as focusable contextual route panels with Back to World controls", () => {
    for (const [id, route] of [
      ["viewer-targets-panel", "targets"],
      ["viewer-details-panel", "command"],
    ]) {
      const panel = document.querySelector(`#${id}`);
      expect(panel).toBeTruthy();
      expect(panel).toHaveAttribute("data-viewer-route-panel", route);
      expect(panel).toHaveAttribute("tabindex", "-1");

      const backToWorld = within(panel).getByRole("link", { name: /(?:close|back to world)/i });
      expect(backToWorld).toHaveAttribute("href", "#viewer-stage-panel");
    }
  });

  it("moves keyboard focus to each canonical route anchor when its primary nav link is activated", () => {
    const nav = screen.getByRole("navigation", { name: /primary entry section navigation/i });
    for (const [name, id] of [
      ["World", "viewer-stage-panel"],
      ["Targets", "viewer-targets-panel"],
      ["Command", "viewer-details-panel"],
    ]) {
      const link = within(nav).getByRole("link", { name });
      const panel = document.getElementById(id);
      expect(panel).toBeTruthy();
      fireEvent.click(link);
      expect(document.activeElement).toBe(panel);
    }
  });

  it("opens and focuses the closed Diagnostics disclosure from secondary navigation", () => {
    const diagnosticsPanel = document.querySelector("#viewer-diagnostics-panel");
    const moreButton = within(
      screen.getByRole("navigation", { name: /secondary viewer navigation/i }),
    ).getByRole("button", { name: "More" });
    expect(diagnosticsPanel).toHaveProperty("open", false);

    fireEvent.click(moreButton);

    expect(window.location.hash).toBe("#viewer-diagnostics-panel");
    expect(diagnosticsPanel).toHaveProperty("open", true);
    expect(document.activeElement).toBe(diagnosticsPanel.querySelector("summary"));
  });

  it("closes a Targets or Command route on Escape, returns to World, and ignores IME Escape", () => {
    const nav = screen.getByRole("navigation", { name: /primary entry section navigation/i });
    const stagePanel = document.querySelector("#viewer-stage-panel");
    const targetsPanel = document.querySelector("#viewer-targets-panel");
    const commandPanel = document.querySelector("#viewer-details-panel");

    fireEvent.click(within(nav).getByRole("link", { name: "Targets" }));
    expect(document.activeElement).toBe(targetsPanel);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(window.location.hash).toBe("#viewer-stage-panel");
    expect(document.activeElement).toBe(stagePanel);

    fireEvent.click(within(nav).getByRole("link", { name: "Command" }));
    expect(document.activeElement).toBe(commandPanel);
    fireEvent.keyDown(window, { key: "Escape", isComposing: true });
    expect(window.location.hash).toBe("#viewer-details-panel");
    expect(document.activeElement).toBe(commandPanel);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(window.location.hash).toBe("#viewer-stage-panel");
    expect(document.activeElement).toBe(stagePanel);
  });

  it("makes mobile More own an on-demand Diagnostics disclosure and restores stage focus on Escape", () => {
    const secondaryNav = screen.getByRole("navigation", { name: /secondary viewer navigation/i });
    const more = within(secondaryNav).getByRole("button", { name: "More" });
    expect(more).toBeInTheDocument();
    expect(within(secondaryNav).queryByRole("link", { name: "Diagnostics" })).not.toBeInTheDocument();

    fireEvent.click(more);
    const diagnostics = document.querySelector("#viewer-diagnostics-panel");
    expect(diagnostics).toHaveProperty("open", true);
    expect(window.location.hash).toBe("#viewer-diagnostics-panel");
    expect(document.activeElement).toBe(diagnostics.querySelector("summary"));

    fireEvent.keyDown(window, { key: "Escape", isComposing: true });
    expect(window.location.hash).toBe("#viewer-diagnostics-panel");
    expect(diagnostics).toHaveProperty("open", true);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(window.location.hash).toBe("#viewer-stage-panel");
    expect(diagnostics).toHaveProperty("open", false);
    expect(document.activeElement).toBe(document.querySelector("#viewer-stage-panel"));
  });

  it("opens nested Diagnostics with its Gameplay Details ancestor and closes both on Escape", () => {
    const secondaryNav = screen.getByRole("navigation", { name: /secondary viewer navigation/i });
    const more = within(secondaryNav).getByRole("button", { name: "More" });
    const gameplayDetails = document.querySelector("#viewer-gameplay-details");
    const diagnostics = document.querySelector("#viewer-diagnostics-panel");
    const stagePanel = document.querySelector("#viewer-stage-panel");

    expect(gameplayDetails).toHaveProperty("open", false);
    expect(diagnostics).toHaveProperty("open", false);

    fireEvent.click(more);

    expect(window.location.hash).toBe("#viewer-diagnostics-panel");
    expect(gameplayDetails).toHaveProperty("open", true);
    expect(diagnostics).toHaveProperty("open", true);

    fireEvent.keyDown(window, { key: "Escape" });

    expect(window.location.hash).toBe("#viewer-stage-panel");
    expect(gameplayDetails).toHaveProperty("open", false);
    expect(diagnostics).toHaveProperty("open", false);
    expect(document.activeElement).toBe(stagePanel);
  });

  it("defines a stage-first single-column shell and on-demand drawer CSS without horizontal overflow", async () => {
    const css = await readExpandedCss("viewer_terminal_shell.css");

    expect(css).toMatch(/\.shell\s*\{[^}]*display:\s*grid/s);
    expect(css).toMatch(/\.shell\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)/s);
    for (const id of ["viewer-targets-panel", "viewer-details-panel"]) {
      expect(css).toMatch(new RegExp(`#${id}[^{}]*\\{[^}]*display:\\s*none`, "s"));
      expect(css).toMatch(new RegExp(`#${id}[^{}]*(?::target|\\[data-route-open(?:=\\\"true\\\")?\\])`, "s"));
    }
    expect(css).toMatch(/(?:#viewer-targets-panel|#viewer-details-panel)[^{]*\{[^}]*position:\s*fixed/s);
    expect(css).toMatch(/(?:#viewer-targets-panel|#viewer-details-panel)[^{]*\{[^}]*z-index:\s*\d+/s);
    expect(css).toMatch(/@media[^{}]*(?:max-width|max-inline-size)[^{}]*\{[\s\S]*#viewer-(?:targets|details)-panel/s);
    expect(css).toMatch(/max-width:\s*min\(/s);
    expect(css).toMatch(/overflow-x:\s*hidden/s);
  });

  it("protects primary and secondary rail links from bounded long-label overflow", async () => {
    const css = await readExpandedCss("viewer_terminal_shell.css");
    for (const selector of ["mobile-rail__link", "secondary-viewer-nav__link"]) {
      expect(css).toMatch(new RegExp(
        `\\.${selector}[^{}]*\\{[\\s\\S]*?min-width:\\s*0;[\\s\\S]*?(?:overflow-wrap:\\s*anywhere|word-break:\\s*break-word)`,
        "s",
      ));
    }
  });
});
