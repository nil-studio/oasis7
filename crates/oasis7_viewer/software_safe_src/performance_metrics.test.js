import { describe, expect, it } from "vitest";
import {
  buildViewerPerformanceMarkdown,
  evaluateViewerPerformance,
  summarizeViewerPerformance,
} from "./performance_metrics.js";

describe("viewer performance metrics", () => {
  it("summarizes healthy samples with fps and dom readiness", () => {
    const summary = summarizeViewerPerformance({
      runId: "healthy",
      profile: "smoke",
      durationMs: 1800,
      sampleDurationMs: 1800,
      frameIntervals: Array.from({ length: 110 }, (_, index) => (index % 12 === 0 ? 18 : 16)),
      longTasks: [],
      domReadiness: {
        domContentLoadedMs: 240,
        loadEventMs: 320,
        domInteractiveMs: 200,
      },
      interactionLatencies: [42, 51, 65, 58],
      dom: { nodeCount: 240, panelCount: 3, interactiveElementCount: 18, renderedCanvasCount: 1, fallbackShellCount: 0 },
      thresholds: {
        minFrameSamples: 100,
        minFps: 50,
        maxFrameP95Ms: 24,
        maxFrameP99Ms: 24,
        maxLongTaskCount: 0,
        maxLongTaskTotalMs: 0,
        maxDomContentLoadedMs: 1000,
        maxLoadEventMs: 1000,
        maxInteractionP95Ms: 100,
      },
    });

    expect(summary.status).toBe("pass");
    expect(summary.metrics.frameSamples).toBe(110);
    expect(summary.metrics.fpsAvg).toBeGreaterThan(55);
    expect(summary.metrics.frameP95Ms).toBe(18);
    expect(summary.metrics.domContentLoadedMs).toBe(240);
    expect(summary.metrics.interactionP95Ms).toBe(65);
    expect(summary.metrics.renderedCanvasCount).toBe(1);
    expect(summary.metrics.fallbackShellCount).toBe(0);
  });

  it("does not treat missing interaction measurements as zero-latency success", () => {
    const summary = summarizeViewerPerformance({
      frameIntervals: Array(120).fill(16), sampleDurationMs: 2000,
      interactionLatencies: [null, undefined, "", " ", false, Number.NaN],
      domReadiness: { domContentLoadedMs: 100, loadEventMs: 120 },
      dom: { renderedCanvasCount: 1, fallbackShellCount: 0 },
      finalState: { pixelWorldRuntimeStatus: "ready", pixelWorldRuntimeSource: "wasm_bindgen_runtime" },
    });
    expect(summary.metrics.interactionSamples).toBe(0);
    expect(summary.metrics.interactionP95Ms).toBeNull();
    expect(summary.gates.find(gate => gate.id === "interaction_p95_ms").status).toBe("skip");
    expect(summary.gates.find(gate => gate.id === "interaction_samples").status).toBe("fail");
    expect(summary.status).toBe("fail");
  });

  it("fails laggy samples against fps, frame, long task, and interaction gates", () => {
    const summary = summarizeViewerPerformance({
      runId: "laggy",
      profile: "smoke",
      durationMs: 1200,
      sampleDurationMs: 1200,
      frameIntervals: [16, 18, 52, 64, 80, 90, 110, 24, 18, 16],
      longTasks: [{ startTime: 12, duration: 180 }],
      domReadiness: {
        domContentLoadedMs: 4200,
        loadEventMs: 5100,
      },
      interactionLatencies: [40, 120, 280],
      thresholds: {
        minFrameSamples: 5,
        minFps: 20,
        maxFrameP95Ms: 50,
        maxFrameP99Ms: 80,
        maxLongTaskCount: 0,
        maxLongTaskTotalMs: 0,
        maxDomContentLoadedMs: 1000,
        maxLoadEventMs: 1000,
        maxInteractionP95Ms: 150,
      },
    });
    const failed = evaluateViewerPerformance(summary).gates
      .filter((gate) => gate.status === "fail")
      .map((gate) => gate.id);

    expect(summary.status).toBe("fail");
    expect(failed).toContain("frame_p95_ms");
    expect(failed).toContain("long_task_count");
    expect(failed).toContain("long_task_total_ms");
    expect(failed).toContain("dom_content_loaded_ms");
    expect(failed).toContain("load_event_ms");
    expect(failed).toContain("interaction_p95_ms");
  });

  it("keeps nearest-rank percentiles stable for unsorted and invalid samples", () => {
    const summary = summarizeViewerPerformance({
      runId: "percentile-stability",
      durationMs: 1000,
      sampleDurationMs: 1000,
      frameIntervals: [30, "10", null, 20, Number.NaN, -4, 40, 50],
      interactionLatencies: [120, "40", undefined, 80, 160, Number.POSITIVE_INFINITY],
      thresholds: {
        minFrameSamples: 1,
        minFps: 1,
        maxFrameP95Ms: 100,
        maxFrameP99Ms: 100,
        maxLongTaskCount: 0,
        maxLongTaskTotalMs: 0,
        maxDomContentLoadedMs: 1000,
        maxLoadEventMs: 1000,
        maxInteractionP95Ms: 200,
      },
    });

    expect(summary.metrics.frameSamples).toBe(5);
    expect(summary.metrics.frameP50Ms).toBe(30);
    expect(summary.metrics.frameP95Ms).toBe(50);
    expect(summary.metrics.frameP99Ms).toBe(50);
    expect(summary.metrics.interactionSamples).toBe(4);
    expect(summary.metrics.interactionP50Ms).toBe(80);
    expect(summary.metrics.interactionP95Ms).toBe(160);
  });

  it("renders markdown with the expanded metric rows", () => {
    const summary = summarizeViewerPerformance({
      runId: "markdown",
      url: "http://127.0.0.1/viewer",
      scenario: { name: "dense" },
      sampleDurationMs: 1600,
      frameIntervals: Array.from({ length: 90 }, () => 16),
      domReadiness: { domContentLoadedMs: 180, loadEventMs: 240 },
      interactionLatencies: [30, 35, 40],
      dom: { renderedCanvasCount: 1, fallbackShellCount: 0 },
      finalState: {
        pixelWorldRuntimeStatus: "ready",
        pixelWorldRuntimeSource: "wasm_bindgen_runtime",
      },
      thresholds: { minFrameSamples: 90 },
    });

    const markdown = buildViewerPerformanceMarkdown(summary);

    expect(markdown).toContain("# Viewer Performance Probe");
    expect(markdown).toContain("| frame_p95_ms |");
    expect(markdown).toContain("DOMContentLoaded(ms)");
    expect(markdown).toContain("Pixel-world runtime: `ready` / `wasm_bindgen_runtime`");
    expect(markdown).toContain("rendered canvas `1`, fallback shell `0`");
    expect(markdown).toContain("http://127.0.0.1/viewer");
  });

  it("fails when the probe finishes unavailable or without the wasm runtime", () => {
    const summary = summarizeViewerPerformance({
      runId: "renderer-regressed",
      profile: "smoke",
      durationMs: 1800,
      sampleDurationMs: 1800,
      frameIntervals: Array.from({ length: 110 }, (_, index) => (index % 12 === 0 ? 18 : 16)),
      longTasks: [],
      domReadiness: {
        domContentLoadedMs: 240,
        loadEventMs: 320,
        domInteractiveMs: 200,
      },
      interactionLatencies: [42, 51, 65, 58],
      dom: { nodeCount: 240, panelCount: 3, interactiveElementCount: 18, renderedCanvasCount: 0, fallbackShellCount: 1 },
      finalState: {
        pixelWorldRuntimeStatus: "unavailable",
        pixelWorldRuntimeSource: "detached",
      },
      thresholds: {
        minFrameSamples: 100,
        minFps: 50,
        maxFrameP95Ms: 24,
        maxFrameP99Ms: 24,
        maxLongTaskCount: 0,
        maxLongTaskTotalMs: 0,
        maxDomContentLoadedMs: 1000,
        maxLoadEventMs: 1000,
        maxInteractionP95Ms: 100,
      },
    });

    const failed = evaluateViewerPerformance(summary).gates
      .filter((gate) => gate.status === "fail")
      .map((gate) => gate.id);

    expect(summary.status).toBe("fail");
    expect(failed).toContain("pixel_world_runtime_status");
    expect(failed).toContain("pixel_world_runtime_source");
    expect(failed).toContain("rendered_canvas_count");
    expect(failed).toContain("fallback_shell_count");
  });

  it("keeps the probe artifact schema stable", () => {
    const summary = summarizeViewerPerformance({
      runId: "schema",
      profile: "smoke",
      url: "http://127.0.0.1/viewer",
      sampleDurationMs: 1600,
      frameIntervals: Array.from({ length: 90 }, () => 16),
      domReadiness: { domContentLoadedMs: 180, loadEventMs: 240 },
    });
    summary.artifacts = {
      summaryJson: "/tmp/summary.json",
      summaryMarkdown: "/tmp/summary.md",
      screenshot: "/tmp/viewer-performance.png",
    };
    summary.scenario = {
      name: "dense",
      agents: 288,
      locations: 96,
    };

    expect(summary.schemaVersion).toBe(3);
    expect(summary).toEqual(expect.objectContaining({
      runId: "schema",
      profile: "smoke",
      url: "http://127.0.0.1/viewer",
      status: expect.any(String),
      metrics: expect.any(Object),
      thresholds: expect.any(Object),
      domReadiness: expect.objectContaining({
        domContentLoadedMs: 180,
        loadEventMs: 240,
      }),
      gates: expect.any(Array),
      artifacts: expect.objectContaining({
        summaryJson: expect.stringContaining("summary.json"),
        summaryMarkdown: expect.stringContaining("summary.md"),
        screenshot: expect.stringContaining("viewer-performance.png"),
      }),
      scenario: {
        name: "dense",
        agents: 288,
        locations: 96,
      },
    }));
  });

  it("reports CPU samples, hardware eligibility, and repeated idle/dense baselines in schema v3", () => {
    const summary = summarizeViewerPerformance({
      runId: "cpu-and-baselines",
      sampleDurationMs: 2_000,
      frameIntervals: Array.from({ length: 120 }, () => 16),
      cpuSamples: [
        { processTreeCpuPct: 18.5 },
        { processTreeCpuPct: 24.5 },
        { processTreeCpuPct: 32.0 },
      ],
      renderer: {
        rendererClass: "hardware",
        renderer: "ANGLE (Apple, Apple M-series, Metal)",
      },
      baselines: {
        idle: [
          { runId: "idle-1", frameP95Ms: 16, processTreeCpuPctAvg: 4.2 },
          { runId: "idle-2", frameP95Ms: 17, processTreeCpuPctAvg: 4.8 },
        ],
        dense: [
          { runId: "dense-1", frameP95Ms: 19, processTreeCpuPctAvg: 22.4 },
          { runId: "dense-2", frameP95Ms: 20, processTreeCpuPctAvg: 24.6 },
        ],
      },
    });

    expect(summary.schemaVersion).toBe(3);
    expect(summary.rendererEligibility).toEqual({
      eligible: true,
      hardwareAccelerated: true,
      softwareRenderer: false,
      renderer: "ANGLE (Apple, Apple M-series, Metal)",
    });
    expect(summary.cpu).toEqual({
      sampleCount: 3,
      processTreeCpuPctAvg: 25,
      processTreeCpuPctP95: 32,
    });
    expect(summary.baselines.idle).toHaveLength(2);
    expect(summary.baselines.dense).toHaveLength(2);
  });

  it("marks software renderer observations ineligible instead of accepting them as a performance baseline", () => {
    const summary = summarizeViewerPerformance({
      runId: "software-renderer",
      renderer: {
        rendererClass: "software",
        renderer: "ANGLE (Google, SwiftShader)",
      },
      baselines: { idle: [{ runId: "idle-1" }, { runId: "idle-2" }], dense: [{ runId: "dense-1" }, { runId: "dense-2" }] },
    });

    expect(summary.rendererEligibility).toEqual({
      eligible: false,
      hardwareAccelerated: false,
      softwareRenderer: true,
      renderer: "ANGLE (Google, SwiftShader)",
    });
  });
});
