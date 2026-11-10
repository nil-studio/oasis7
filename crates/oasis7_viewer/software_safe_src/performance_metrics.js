export const DEFAULT_VIEWER_PERF_THRESHOLDS = Object.freeze({
  minFrameSamples: 90,
  minFps: 45,
  maxFrameP95Ms: 24,
  maxFrameP99Ms: 50,
  maxLongTaskCount: 3,
  maxLongTaskTotalMs: 200,
  maxDomContentLoadedMs: 3000,
  maxLoadEventMs: 3500,
  maxInteractionP95Ms: 200,
  frameBudgetMs: 16.7,
  severeFrameBudgetMs: 50,
});

export const VIEWER_PERF_PROFILES = Object.freeze({
  smoke: {
    durationMs: 2500,
    thresholds: DEFAULT_VIEWER_PERF_THRESHOLDS,
  },
  release: {
    durationMs: 8000,
    thresholds: {
      ...DEFAULT_VIEWER_PERF_THRESHOLDS,
      minFrameSamples: 240,
      minFps: 55,
      maxFrameP95Ms: 20,
      maxFrameP99Ms: 33.3,
      maxLongTaskCount: 1,
      maxLongTaskTotalMs: 120,
      maxInteractionP95Ms: 120,
    },
  },
});

function asNumber(value, fallback = null) {
  if (value == null || typeof value === "boolean" || (typeof value === "string" && value.trim() === "")) return fallback;
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

function round(value, digits = 2) {
  if (!Number.isFinite(value)) {
    return null;
  }
  const multiplier = 10 ** digits;
  return Math.round(value * multiplier) / multiplier;
}

function percentileSorted(sortedValues, quantile) {
  if (!Array.isArray(sortedValues) || sortedValues.length === 0) {
    return null;
  }
  const ratio = Math.min(1, Math.max(0, Number(quantile) || 0));
  const index = Math.min(sortedValues.length - 1, Math.ceil(sortedValues.length * ratio) - 1);
  return sortedValues[Math.max(0, index)];
}

function average(values) {
  if (!Array.isArray(values) || values.length === 0) {
    return null;
  }
  return values.reduce((total, value) => total + value, 0) / values.length;
}

function sortedNonNegativeNumbers(values) {
  if (!Array.isArray(values) || values.length === 0) {
    return [];
  }
  const normalized = [];
  for (const value of values) {
    const number = asNumber(value, NaN);
    if (Number.isFinite(number) && number >= 0) {
      normalized.push(number);
    }
  }
  normalized.sort((left, right) => left - right);
  return normalized;
}

function normalizeRendererEligibility(renderer = {}) {
  const rendererName = typeof renderer?.renderer === "string" ? renderer.renderer : null;
  const rendererClass = String(renderer?.rendererClass || "").toLowerCase();
  const softwareRenderer = rendererClass === "software" || /swiftshader|software renderer/i.test(rendererName || "");
  const hardwareAccelerated = rendererClass === "hardware" || (Boolean(rendererName) && !softwareRenderer);
  return {
    eligible: hardwareAccelerated && !softwareRenderer,
    hardwareAccelerated,
    softwareRenderer,
    renderer: rendererName,
  };
}

function summarizeCpu(cpuSamples) {
  const values = sortedNonNegativeNumbers((cpuSamples || [])
    .map((sample) => sample?.processTreeCpuPct)
    .filter((value) => value != null));
  return {
    sampleCount: values.length,
    processTreeCpuPctAvg: round(average(values)),
    processTreeCpuPctP95: round(percentileSorted(values, 0.95)),
  };
}

function normalizeBaselines(baselines = {}) {
  const normalize = (samples) => (Array.isArray(samples) ? samples.map((sample) => ({
    runId: sample?.runId || null,
    frameP95Ms: round(asNumber(sample?.frameP95Ms)),
    processTreeCpuPctAvg: sample?.processTreeCpuPctAvg == null
      ? null
      : round(asNumber(sample.processTreeCpuPctAvg)),
  })) : []);
  return { idle: normalize(baselines.idle), dense: normalize(baselines.dense) };
}

function normalizeLongTasks(longTasks) {
  const items = [];
  let totalMs = 0;
  let maxMs = 0;
  if (!Array.isArray(longTasks)) {
    return { items, totalMs, maxMs };
  }
  for (const task of longTasks) {
    const duration = round(asNumber(task?.duration, 0));
    if (duration <= 0) {
      continue;
    }
    const item = {
      startTime: round(asNumber(task?.startTime, 0)),
      duration,
      name: String(task?.name || "longtask"),
    };
    items.push(item);
    totalMs += duration;
    maxMs = Math.max(maxMs, duration);
  }
  return { items, totalMs, maxMs };
}

function skipGate(id, label, threshold) {
  return {
    id,
    label,
    actual: null,
    threshold,
    comparator: "n/a",
    status: "skip",
  };
}

function compareGate(id, label, actual, threshold, comparator) {
  if (!Number.isFinite(actual)) {
    return skipGate(id, label, threshold);
  }
  const pass = comparator === "<=" ? actual <= threshold : actual >= threshold;
  return {
    id,
    label,
    actual: round(actual),
    threshold,
    comparator,
    status: pass ? "pass" : "fail",
  };
}

function compareExactGate(id, label, actual, expected) {
  if (actual == null) {
    return skipGate(id, label, expected);
  }
  return {
    id,
    label,
    actual,
    threshold: expected,
    comparator: "===",
    status: actual === expected ? "pass" : "fail",
  };
}

export function normalizeViewerPerfThresholds(overrides = {}) {
  const thresholds = { ...DEFAULT_VIEWER_PERF_THRESHOLDS, ...overrides };
  thresholds.frameBudgetMs = Math.max(1, asNumber(thresholds.frameBudgetMs, 16.7));
  thresholds.severeFrameBudgetMs = Math.max(
    thresholds.frameBudgetMs,
    asNumber(thresholds.severeFrameBudgetMs, 50),
  );
  thresholds.minFrameSamples = Math.max(1, Math.floor(asNumber(thresholds.minFrameSamples, 90)));
  return thresholds;
}

export function summarizeViewerPerformance({
  runId,
  url,
  profile = "smoke",
  scenario = null,
  durationMs = 0,
  sampleDurationMs = null,
  frameIntervals = [],
  longTasks = [],
  domReadiness = {},
  interactionLatencies = [],
  interactionEvidence = [],
  dom = {},
  viewport = {},
  browser = {},
  thresholds = DEFAULT_VIEWER_PERF_THRESHOLDS,
  notes = [],
  finalState = null,
  browserConsole = [],
  cpuSamples = [],
  renderer = {},
  baselines = {},
} = {}) {
  const normalizedThresholds = normalizeViewerPerfThresholds(thresholds);
  const frames = sortedNonNegativeNumbers(frameIntervals);
  const interactions = sortedNonNegativeNumbers(interactionLatencies);
  const {
    items: longTaskItems,
    totalMs: longTaskTotalMs,
    maxMs: longTaskMaxMs,
  } = normalizeLongTasks(longTasks);

  const frameSamples = frames.length;
  const meanFrameMs = average(frames);
  const sampleWindowMs = asNumber(sampleDurationMs, durationMs);
  const fpsFromWindow = Number.isFinite(sampleWindowMs) && sampleWindowMs > 0
    ? (frameSamples * 1000) / sampleWindowMs
    : null;
  const fpsFromMean = meanFrameMs && meanFrameMs > 0 ? 1000 / meanFrameMs : null;
  let slowFrames = 0;
  let severeFrames = 0;
  for (const value of frames) {
    if (value > normalizedThresholds.frameBudgetMs) {
      slowFrames += 1;
    }
    if (value > normalizedThresholds.severeFrameBudgetMs) {
      severeFrames += 1;
    }
  }
  const metrics = {
    durationMs: round(durationMs),
    sampleDurationMs: round(sampleWindowMs),
    frameSamples,
    frameAvgMs: round(meanFrameMs),
    frameP50Ms: round(percentileSorted(frames, 0.5)),
    frameP95Ms: round(percentileSorted(frames, 0.95)),
    frameP99Ms: round(percentileSorted(frames, 0.99)),
    frameMaxMs: round(frames[frames.length - 1] ?? null),
    fpsAvg: round(fpsFromWindow ?? fpsFromMean),
    slowFrameCount: slowFrames,
    slowFramePct: frameSamples ? round((slowFrames / frameSamples) * 100) : null,
    severeFrameCount: severeFrames,
    severeFramePct: frameSamples ? round((severeFrames / frameSamples) * 100) : null,
    longTaskCount: longTaskItems.length,
    longTaskTotalMs: round(longTaskTotalMs),
    longTaskMaxMs: round(longTaskMaxMs),
    domContentLoadedMs: round(asNumber(domReadiness.domContentLoadedMs)),
    loadEventMs: round(asNumber(domReadiness.loadEventMs)),
    domInteractiveMs: round(asNumber(domReadiness.domInteractiveMs)),
    responseEndMs: round(asNumber(domReadiness.responseEndMs)),
    interactionSamples: interactions.length,
    interactionP50Ms: round(percentileSorted(interactions, 0.5)),
    interactionP95Ms: round(percentileSorted(interactions, 0.95)),
    interactionMaxMs: round(interactions[interactions.length - 1] ?? null),
    domNodeCount: Math.floor(asNumber(dom.nodeCount, 0) ?? 0),
    panelCount: Math.floor(asNumber(dom.panelCount, 0) ?? 0),
    interactiveElementCount: Math.floor(asNumber(dom.interactiveElementCount, 0) ?? 0),
    renderedCanvasCount: Math.floor(asNumber(dom.renderedCanvasCount, 0) ?? 0),
    fallbackShellCount: Math.floor(asNumber(dom.fallbackShellCount, 0) ?? 0),
  };

  const result = {
    schemaVersion: 3,
    runId: runId || null,
    profile,
    scenario,
    url: url || null,
    status: "unknown",
    metrics,
    thresholds: normalizedThresholds,
    longTasks: longTaskItems,
    interactionLatencies: interactions.map((value) => round(value)),
    interactionEvidence,
    domReadiness: {
      domInteractiveMs: metrics.domInteractiveMs,
      domContentLoadedMs: metrics.domContentLoadedMs,
      loadEventMs: metrics.loadEventMs,
      responseEndMs: metrics.responseEndMs,
    },
    dom,
    viewport,
    browser,
    notes: Array.isArray(notes) ? notes : [],
    finalState,
    browserConsole,
    cpu: summarizeCpu(cpuSamples),
    cpuEvidence: {
      attempted: cpuSamples.some((sample) => sample?.attempted === true),
      observationCount: cpuSamples.length,
      unavailableReason: cpuSamples.find((sample) => sample?.unavailableReason)?.unavailableReason || null,
    },
    rendererEligibility: normalizeRendererEligibility(renderer),
    baselines: normalizeBaselines(baselines),
    gates: [],
  };
  result.gates = evaluateViewerPerformance(result).gates;
  result.status = result.gates.every((gate) => gate.status !== "fail") ? "pass" : "fail";
  return result;
}

export function evaluateViewerPerformance(summary) {
  const metrics = summary?.metrics || {};
  const thresholds = normalizeViewerPerfThresholds(summary?.thresholds || {});
  const finalState = summary?.finalState || {};
  const gates = [
    compareGate("frame_samples", "frame sample count", asNumber(metrics.frameSamples), thresholds.minFrameSamples, ">="),
    compareGate("fps_avg", "average FPS", asNumber(metrics.fpsAvg), thresholds.minFps, ">="),
    compareGate("frame_p95_ms", "p95 frame interval", asNumber(metrics.frameP95Ms), thresholds.maxFrameP95Ms, "<="),
    compareGate("frame_p99_ms", "p99 frame interval", asNumber(metrics.frameP99Ms), thresholds.maxFrameP99Ms, "<="),
    compareGate("long_task_count", "long task count", asNumber(metrics.longTaskCount), thresholds.maxLongTaskCount, "<="),
    compareGate("long_task_total_ms", "long task total time", asNumber(metrics.longTaskTotalMs), thresholds.maxLongTaskTotalMs, "<="),
    compareGate("dom_content_loaded_ms", "DOMContentLoaded", asNumber(metrics.domContentLoadedMs), thresholds.maxDomContentLoadedMs, "<="),
    compareGate("load_event_ms", "load event", asNumber(metrics.loadEventMs), thresholds.maxLoadEventMs, "<="),
    compareGate("interaction_samples", "measured interaction sample count", asNumber(metrics.interactionSamples, 0), 1, ">="),
    compareGate("interaction_p95_ms", "interaction p95", asNumber(metrics.interactionP95Ms), thresholds.maxInteractionP95Ms, "<="),
    compareExactGate("pixel_world_runtime_status", "pixel world runtime status", finalState.pixelWorldRuntimeStatus ?? null, "ready"),
    compareExactGate("pixel_world_runtime_source", "pixel world runtime source", finalState.pixelWorldRuntimeSource ?? null, "wasm_bindgen_runtime"),
    compareGate("rendered_canvas_count", "rendered canvas count", asNumber(metrics.renderedCanvasCount), 1, ">="),
    compareGate("fallback_shell_count", "fallback shell count", asNumber(metrics.fallbackShellCount), 0, "<="),
  ];
  return {
    status: gates.every((gate) => gate.status !== "fail") ? "pass" : "fail",
    gates,
  };
}

export function buildViewerPerformanceMarkdown(summary) {
  const metrics = summary.metrics || {};
  const finalState = summary.finalState || {};
  const lines = [
    "# Viewer Performance Probe",
    "",
    `- Status: \`${summary.status}\``,
    `- Run ID: \`${summary.runId || "-"}\``,
    `- Profile: \`${summary.profile || "-"}\``,
    `- Scenario: \`${summary.scenario?.name || summary.scenario || "-"}\``,
    `- URL: \`${summary.url || "-"}\``,
    "",
    "## Metrics",
    `- Frame samples: \`${metrics.frameSamples}\``,
    `- FPS avg: \`${metrics.fpsAvg}\``,
    `- Frame p95(ms): \`${metrics.frameP95Ms}\``,
    `- Frame p99(ms): \`${metrics.frameP99Ms}\``,
    `- Long tasks: \`${metrics.longTaskCount}\`, total(ms): \`${metrics.longTaskTotalMs}\`, max(ms): \`${metrics.longTaskMaxMs}\``,
    `- DOMContentLoaded(ms): \`${metrics.domContentLoadedMs}\``,
    `- Load event(ms): \`${metrics.loadEventMs}\``,
    `- Interaction p95(ms): \`${metrics.interactionP95Ms ?? "-"}\``,
    `- Process-tree CPU(%): avg \`${summary.cpu?.processTreeCpuPctAvg ?? "-"}\`, p95 \`${summary.cpu?.processTreeCpuPctP95 ?? "-"}\`, samples \`${summary.cpu?.sampleCount ?? 0}\``,
    `- Renderer eligibility: \`${summary.rendererEligibility?.eligible === true ? "eligible" : "ineligible"}\` (\`${summary.rendererEligibility?.renderer || "unknown"}\`)`,
    `- DOM nodes: \`${metrics.domNodeCount}\`, interactive elements: \`${metrics.interactiveElementCount}\``,
    `- Pixel-world runtime: \`${finalState.pixelWorldRuntimeStatus || "-"}\` / \`${finalState.pixelWorldRuntimeSource || "-"}\``,
    `- Pixel-world renderer DOM: rendered canvas \`${metrics.renderedCanvasCount}\`, fallback shell \`${metrics.fallbackShellCount}\``,
    "",
    "## Gates",
    "| Gate | Actual | Comparator | Threshold | Status |",
    "| --- | ---: | --- | ---: | --- |",
    ...(summary.gates || []).map((gate) => (
      `| ${gate.id} | ${gate.actual ?? "-"} | ${gate.comparator} | ${gate.threshold ?? "-"} | ${gate.status} |`
    )),
  ];
  if (Array.isArray(summary.notes) && summary.notes.length > 0) {
    lines.push("");
    lines.push("## Notes");
    for (const note of summary.notes) {
      lines.push(`- ${note}`);
    }
  }
  lines.push("");
  return `${lines.join("\n")}\n`;
}
