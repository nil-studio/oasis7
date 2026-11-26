export function shellLiteViewportProbeScript() {
  return String.raw`
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for Shell Lite viewport probe condition");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const textOf = (selector, root = document) => root.querySelector(selector)?.textContent.trim() || null;
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          right: Math.round(rect.right),
          bottom: Math.round(rect.bottom),
        };
      };
      const receiptOf = () => {
        const receipt = document.querySelector(".pixel-world-action-receipt");
        if (!receipt) {
          return null;
        }
        return {
          present: receipt.dataset.receiptPresent || null,
          state: receipt.dataset.receiptState || null,
          confidence: receipt.dataset.receiptConfidence || null,
          label: textOf(".pixel-world-action-receipt__label", receipt),
          title: textOf(".pixel-world-action-receipt__title", receipt),
          rect: rectOf(receipt),
        };
      };

      await waitFor(() => state().pixelWorldRuntimeStatus === "ready");
      const commandStrip = await waitFor(() => document.querySelector('[data-viewer-overlay="next-move"]'));
      const directChildren = Array.from(commandStrip.children);
      const directRegions = directChildren.filter((element) => element.matches("[data-shell-region]"));
      const primary = commandStrip.querySelector('[data-shell-region="next-move-primary"]');
      const supporting = commandStrip.querySelector('[data-shell-region="supporting-context"]');
      const primaryControls = primary ? Array.from(primary.querySelectorAll("a,button")) : [];
      const supportingControls = supporting ? Array.from(supporting.querySelectorAll("a,button")) : [];
      const allControls = Array.from(commandStrip.querySelectorAll("a,button"));
      const selectedAgent = supporting?.querySelector("[data-selected-agent-label]");
      const receiptElement = document.querySelector(".pixel-world-action-receipt");
      const feedElement = document.querySelector('[data-viewer-overlay="feed"]');
      const readoutElement = document.querySelector(".pixel-world-readout");
      const selectedMarkerElement = document.querySelector(".pixel-world-entity--canvas-hit-target[data-selected='true']");
      const viewport = {
        width: window.innerWidth,
        height: window.innerHeight,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
      };
      const primaryRect = primary ? rectOf(primary) : null;
      const supportingRect = supporting ? rectOf(supporting) : null;
      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        shellLite: {
          directRegionCount: directRegions.length,
          directRegionNames: directRegions.map((element) => element.dataset.shellRegion || null),
          directChildCount: directChildren.length,
          primaryIndex: directRegions.indexOf(primary),
          supportingIndex: directRegions.indexOf(supporting),
          primaryCtaCount: primaryControls.length,
          primaryCtaLabels: primaryControls.map((element) => element.textContent.trim()),
          supportingControlCount: supportingControls.length,
          allControlCount: allControls.length,
          primaryRect,
          supportingRect,
          primaryOpacity: primary ? Number.parseFloat(getComputedStyle(primary).opacity) : null,
          supportingOpacity: supporting ? Number.parseFloat(getComputedStyle(supporting).opacity) : null,
          primaryDominates: Boolean(primaryRect && supportingRect && primaryRect.width >= supportingRect.width),
          objectiveText: textOf(".pixel-world-shell-context-group--objective", supporting),
          leverageText: textOf(".pixel-world-shell-context-group--leverage", supporting),
          selectedAgentLabel: selectedAgent?.dataset.selectedAgentLabel || null,
          selectedAgentText: selectedAgent?.textContent.trim() || null,
          selectedAgentReadable: Boolean(selectedAgent?.dataset.selectedAgentLabel && !/^agent-[a-z0-9_-]+$/i.test(selectedAgent.dataset.selectedAgentLabel)),
          receipt: receiptOf(),
          supportingReceiptGapPx: supporting && receiptElement ? Math.round(receiptElement.getBoundingClientRect().top - supporting.getBoundingClientRect().bottom) : null,
          receiptDistinct: Boolean(receiptElement && receiptElement.parentElement !== commandStrip && !receiptElement.closest('[data-viewer-overlay="next-move"]')),
          feedRect: feedElement ? rectOf(feedElement) : null,
          readoutRect: readoutElement ? rectOf(readoutElement) : null,
          readoutDisplay: readoutElement ? getComputedStyle(readoutElement).display : null,
          selectedMarkerRect: selectedMarkerElement ? rectOf(selectedMarkerElement) : null,
          commandStripRect: rectOf(commandStrip),
          documentLocale: document.documentElement.lang || null,
          viewport,
          horizontalOverflowPx: Math.max(0, viewport.scrollWidth - viewport.clientWidth),
        },
      });
    })()
  `;
}

export function rendererFallbackProbeScript() {
  return String.raw`
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 12000) => {
        const deadline = Date.now() + timeoutMs;
        let lastError = null;
        while (Date.now() < deadline) {
          try {
            const value = predicate();
            if (value) {
              return value;
            }
          } catch (error) {
            lastError = error;
          }
          await sleep(100);
        }
        throw lastError || new Error("timed out waiting for renderer-unavailable recovery surface");
      };
      const state = () => window.__AW_TEST__?.getState?.() || {};
      const rectOf = (element) => {
        const rect = element.getBoundingClientRect();
        return {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          right: Math.round(rect.right),
          bottom: Math.round(rect.bottom),
        };
      };
      let fallback;
      try {
        fallback = await waitFor(() => document.querySelector('[data-viewer-overlay="renderer-unavailable"][data-renderer-state="unavailable"]'));
      } catch (error) {
        const diagnostics = document.querySelector(".pixel-world-render-diagnostics");
        throw new Error([
          "renderer deferral route did not become unavailable",
          error instanceof Error ? error.message : String(error),
          JSON.stringify({
            query: window.location.search,
            runtimeStatus: state().pixelWorldRuntimeStatus ?? null,
            runtimeSource: state().pixelWorldRuntimeSource ?? null,
            fatal: state().pixelWorldFatal || null,
            rendererOverlayState: document.querySelector('[data-viewer-overlay="renderer-unavailable"]')?.dataset.rendererState || null,
            diagnosticsState: diagnostics?.dataset.rendererState || null,
            diagnosticsOpen: diagnostics?.open ?? null,
            shellLitePresent: Boolean(document.querySelector('[data-viewer-overlay="next-move"]')),
            actionReceiptPresent: Boolean(document.querySelector(".pixel-world-action-receipt")),
            canvasPresent: Boolean(document.querySelector("#pixel-world-embedded-runtime-canvas")),
          }, null, 2),
        ].join("\n"));
      }
      const diagnostics = document.querySelector(".pixel-world-render-diagnostics");
      const diagnosticText = diagnostics?.textContent.trim() || null;
      const viewport = {
        width: window.innerWidth,
        height: window.innerHeight,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
      };
      return JSON.stringify({
        runtimeStatus: state().pixelWorldRuntimeStatus,
        runtimeSource: state().pixelWorldRuntimeSource,
        fatal: state().pixelWorldFatal || null,
        fallback: {
          visible: getComputedStyle(fallback).display !== "none",
          text: fallback.textContent.trim(),
          rect: rectOf(fallback),
          retryText: fallback.querySelector("button")?.textContent.trim() || null,
          retryCount: fallback.querySelectorAll("button").length,
          rawFatalPromoted: /pixel_world_|CONTEXT_LOST_WEBGL|fatal/i.test(fallback.textContent),
        },
        diagnostics: {
          present: Boolean(diagnostics),
          open: diagnostics?.open ?? null,
          rendererState: diagnostics?.dataset.rendererState || null,
          text: diagnosticText,
          rawFatalFolded: Boolean(diagnostics && diagnostics.open === false && /pixel_world_|fatal/i.test(diagnosticText || "")),
        },
        shellLitePresent: Boolean(document.querySelector('[data-viewer-overlay="next-move"]')),
        actionReceiptPresent: Boolean(document.querySelector(".pixel-world-action-receipt")),
        canvasPresent: Boolean(document.querySelector("#pixel-world-embedded-runtime-canvas")),
        viewport,
        horizontalOverflowPx: Math.max(0, viewport.scrollWidth - viewport.clientWidth),
      });
    })()
  `;
}
