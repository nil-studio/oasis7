import { execFileSync, spawn } from "node:child_process";
import { accessSync, constants, existsSync, mkdirSync, rmSync, statSync } from "node:fs";
import { delimiter, join, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { createOwnedSessionLifecycle } from "./agent-browser-visual-runner-lifecycle.mjs";
import { DRAFT_TEXT, helloAck, promptSnapshot } from "./browser_functional_regression_fixture_server.mjs";

function assert(condition, message, details = undefined) {
  if (!condition) {
    const suffix = details === undefined ? "" : ` | ${JSON.stringify(details)}`;
    throw new Error(`${message}${suffix}`);
  }
}

export function createBrowserDriver({ sessionPrefix, headed, outDir }) {
  const binary = resolveBrowserBinary(process.env.AGENT_BROWSER_BIN || "agent-browser");
  // agent-browser derives a Unix socket path from the calling worktree; the
  // deep task worktree path can exceed the platform socket-name budget, so
  // keep the run-scoped socket directory at a short root path. Prefer the
  // short /tmp root when it exists; macOS TMPDIR is far too long.
  const driverId = ++nextDriverId;
  const shortRoot = existsSync("/tmp") ? "/tmp" : (process.env.TMPDIR || tmpdir());
  const ownsSocketDir = !process.env.AGENT_BROWSER_SOCKET_DIR;
  const socketDir = process.env.AGENT_BROWSER_SOCKET_DIR || join(shortRoot, `o7-bfr-${process.pid}`, String(driverId));
  mkdirSync(socketDir, { recursive: true });
  const sessionSuffix = `${process.pid}-${driverId}`;
  const cleanPrefix = String(sessionPrefix).replace(/[^A-Za-z0-9._-]/g, "-").slice(0, Math.max(8, 39 - sessionSuffix.length));
  const session = `${cleanPrefix}-${sessionSuffix}`;
  let closedViaAsync = false;
  const cleanupOwnedSocketDir = () => {
    if (ownsSocketDir) {
      try { rmSync(socketDir, { recursive: true, force: true }); } catch {}
    }
  };
  const lifecycle = createOwnedSessionLifecycle({
    command: binary,
    session,
    spawnSyncImpl: (command, args, options = {}) => {
      if (closedViaAsync) return { status: 0 };
      const result = spawnSync(command, args, {
        ...options,
        env: { ...process.env, AGENT_BROWSER_SOCKET_DIR: socketDir, ...(options.env || {}) },
      });
      cleanupOwnedSocketDir();
      return result;
    },
  });
  const baseArgs = ["--session", session, ...(headed ? ["--headed"] : []), "--allowed-domains", "127.0.0.1"];

  // These must stay async: the fixture HTTP server runs in this process, so a
  // blocking spawn while the browser opens the page would deadlock the page
  // fetch.
  function runRaw(rawArgs, options = {}) {
    return new Promise((resolveRun, rejectRun) => {
      const child = spawn(binary, rawArgs, {
        env: { ...process.env, AGENT_BROWSER_SOCKET_DIR: socketDir, ...(options.env || {}) },
        stdio: ["pipe", "pipe", "pipe"],
      });
      let stdout = "";
      let stderr = "";
      const timer = setTimeout(() => {
        child.kill("SIGTERM");
        rejectRun(new Error(`agent-browser ${rawArgs[0]} timed out after ${options.timeout || 30_000}ms`));
      }, options.timeout || 30_000);
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", (chunk) => { stdout += chunk; });
      child.stderr.on("data", (chunk) => { stderr += chunk; });
      child.on("error", (error) => { clearTimeout(timer); rejectRun(error); });
      child.on("close", (code) => {
        clearTimeout(timer);
        resolveRun({ status: code, stdout, stderr });
      });
      if (options.input !== undefined) child.stdin.end(options.input);
      else child.stdin.end();
    });
  }

  function run(args, options = {}) {
    return runRaw([...baseArgs, ...args], options);
  }

  async function runJson(args, options = {}) {
    const result = await run([...args, "--json"], options);
    if (result.status !== 0) throw new Error(`agent-browser ${args.join(" ")} failed (${result.status}): ${result.stderr || result.stdout}`);
    const parsed = JSON.parse(result.stdout);
    if (!parsed.success) throw new Error(`agent-browser ${args.join(" ")} failed: ${parsed.error}`);
    return parsed.data;
  }

  async function evalJson(source, options = {}) {
    const data = await runJson(["eval", "--stdin"], { ...options, input: source });
    const value = data.result;
    return typeof value === "string" ? JSON.parse(value) : value;
  }

  async function screenshot(path) {
    mkdirSync(dirname(path), { recursive: true });
    await runJson(["screenshot", path], { timeout: 30_000 });
    return path;
  }

  async function issueAsyncClose() {
    if (!closedViaAsync) {
      try {
        await runRaw(["--session", session, "close"], { timeout: 10_000 });
      } catch {
        // Cleanup must not mask the original scenario failure.
      }
      closedViaAsync = true;
      lifecycle.close();
    }
  }

  async function closeAsync() {
    await issueAsyncClose();
    cleanupOwnedSocketDir();
  }

  async function closeWithVerification(timeoutMs = 4000) {
    await issueAsyncClose();
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        const listed = await runRaw(["session", "list", "--json"], { timeout: 5000 });
        const sessions = JSON.parse(listed.stdout || "{}")?.data?.sessions || [];
        if (!sessions.includes(session)) {
          cleanupOwnedSocketDir();
          return { closed: true, session };
        }
      } catch (_) {
        // Keep polling; a transient parse failure is not proof of cleanup.
      }
      await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
    }
    return { closed: false, session, error: "owned browser session was still listed after close" };
  }

  function close() {
    lifecycle.close();
  }

  // Each case owns a unique pid-scoped session, so there is no same-name
  // session to clear before opening. Pre-open close races the agent-browser
  // daemon socket on 0.37.x and produces "Failed to connect"; keep cleanup on
  // exit and skip the unnecessary pre-open close.
  return { binary, session, run, runJson, evalJson, screenshot, close, closeAsync, closeWithVerification, prepare() {} };
}

export function resolveBrowserBinary(candidate) {
  if (typeof candidate !== "string" || !candidate) throw new Error("agent-browser binary is required");
  const paths = candidate.includes("/") || candidate.includes(sep)
    ? [resolve(candidate)]
    : (process.env.PATH ?? "/usr/bin:/bin").split(delimiter).map(directory => resolve(directory, candidate));
  for (const path of paths) {
    try {
      if (!statSync(path).isFile()) continue;
      accessSync(path, constants.X_OK);
      return path;
    } catch {
      // A missing or unusable PATH entry does not hide a later executable.
    }
  }
  throw new Error(`required browser automation binary is unavailable: ${candidate}`);
}

export async function waitForObservation(readObservation, label, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await readObservation();
    if (last.value) return last.state;
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 50));
  }
  throw new Error(`condition not reached: ${label}\n${JSON.stringify(last?.state ?? null).slice(0, 1200)}`);
}

const STATE_READER = "window.__AW_TEST__?.getState?.() || {}";

// Compare serialized state in Node so request IDs and other observed data
// remain closure values, never browser program text.
export function waitForState(driver, predicate, label, timeoutMs = 8000) {
  return waitForObservation(async () => {
    const state = await driver.evalJson(`JSON.stringify(${STATE_READER})`);
    return { value: predicate(state), state };
  }, label, timeoutMs);
}

// Only repository-owned, static predicates that need DOM/window access or
// pre-serialization checks belong here. Dynamic values must use waitForState
// instead of source interpolation.
export function waitForBrowserState(driver, predicateSource, label, timeoutMs = 8000) {
  return waitForObservation(
    () => driver.evalJson(`(() => { const state = ${STATE_READER}; return JSON.stringify({ value: (${predicateSource})(state), state }); })()`),
    label,
    timeoutMs,
  );
}

export function createScenarioTrace() {
  const entries = [];
  return {
    entries,
    push(kind, detail = {}) {
      entries.push({ at: new Date().toISOString(), kind, ...detail });
      return entries.length;
    },
    jsonl() {
      return `${entries.map((entry) => JSON.stringify(entry)).join("\n")}\n`;
    },
  };
}

export function deliveryProbeSource(message, timeoutMs = 5000) {
  return `window.__BFR__.deliver(${JSON.stringify(message)}, ${timeoutMs}).then(
    (delivered) => JSON.stringify({ ok: true, delivered }),
    (error) => JSON.stringify({ ok: false, error: String(error && error.message ? error.message : error) })
  )`;
}

export async function deliver(driver, message, trace = null) {
  trace?.push("deliver", { type: message.type, status: message?.ack?.status || message?.error?.status || null });
  const result = await driver.evalJson(deliveryProbeSource(message));
  if (!result.ok) {
    throw new Error(`fixture delivery barrier did not settle before its deadline for ${message.type}: ${result.error || "unknown error"}`);
  }
  trace?.push("deliver-settled", { type: message.type, delivered: result.delivered });
  return result;
}

export async function waitOutbound(driver, fromIndex, spec, timeoutMs = 8000, trace = null) {
  const result = await driver.evalJson(`window.__BFR__.nextOutbound(${fromIndex}, ${JSON.stringify(spec)}, ${timeoutMs}).then((value) => JSON.stringify(value))`);
  if (!result.ok) throw new Error(`expected outbound ${JSON.stringify(spec)} was not observed: ${JSON.stringify(result)}`);
  trace?.push("outbound-request", { spec, cursor: result.cursor, requestId: result.entry?.message?.command?.request?.request_id || null });
  return result;
}

export async function countOutbound(driver, spec) {
  return driver.evalJson(`JSON.stringify({ count: window.__BFR__.countOutbound(${JSON.stringify(spec)}), total: window.__BFR__.outboundCount() })`);
}

export async function socketInfo(driver) {
  return driver.evalJson("JSON.stringify(window.__BFR__.socketInfo())");
}

export async function currentSocketId(driver) {
  return driver.evalJson("JSON.stringify(window.__BFR__.currentSocketId())");
}

export async function deliverToSocket(driver, socketId, message, trace = null) {
  trace?.push("deliver-old-socket", { socketId, type: message.type });
  const result = await driver.evalJson(`JSON.stringify(window.__BFR__.deliverToSocket(${socketId}, ${JSON.stringify(message)}))`);
  assert(result.ok === true, "fixture could not deliver a late message to the requested socket", result);
  return result;
}

export async function armSigningGate(driver) {
  const armed = await driver.evalJson("JSON.stringify(window.__BFR__.armSigningGate())");
  assert(armed.installed === true, "deferred signer gate was not installed in the visual-test page", armed);
  return armed;
}

export async function waitForSigning(driver, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await driver.evalJson("JSON.stringify(window.__BFR__.signingState())");
    if (last.entered === true && last.deferred === true) return last;
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 25));
  }
  throw new Error(`request did not enter the deferred signing stage\n${JSON.stringify(last)}`);
}

export async function releaseSigning(driver, trace = null) {
  const released = await driver.evalJson("window.__BFR__.releaseSigning().then((value) => JSON.stringify(value))");
  trace?.push("release-signing", released);
  assert(released.released === true, "deferred signing release did not produce a real signature", released);
  return released;
}

export async function expirePendingPromptAck(driver, trace = null) {
  const result = await driver.evalJson(`JSON.stringify((() => {
    const value = window.__AW_TEST__.expirePendingPromptControlAckTimeoutForTest();
    return { result: value, stage: window.__AW_TEST__.getState().lastPromptFeedback?.stage || null };
  })())`);
  trace?.push("expire-prompt-ack", result);
  assert(result.result === true, "test hook could not terminate the pending prompt request", result);
  return result;
}

export async function promptFeedbackState(driver) {
  return driver.evalJson(`(() => {
    const feedback = window.__AW_TEST__.getState().lastPromptFeedback;
    return JSON.stringify(feedback ? {
      id: feedback.id,
      action: feedback.action,
      stage: feedback.stage,
      requestId: feedback.requestId ?? null,
      requestAuthNonce: feedback.requestAuthNonce ?? null,
      agentId: feedback.agentId ?? null,
      effect: feedback.effect ?? null,
    } : null);
  })()`);
}

export async function promptResultState(driver) {
  return driver.evalJson(`(() => {
    const state = window.__AW_TEST__.getState();
    return JSON.stringify({
      selectedPromptVersion: state.selectedPromptVersion ?? null,
      promptDraftVersion: state.selectedPromptVersion ?? null,
      draftSystemPrompt: document.querySelector("#prompt-system")?.value ?? null,
      lastPromptFeedback: state.lastPromptFeedback ? {
        action: state.lastPromptFeedback.action,
        stage: state.lastPromptFeedback.stage,
        requestId: state.lastPromptFeedback.requestId ?? null,
      } : null,
    });
  })()`);
}

export async function requestSnapshotCount(driver) {
  return driver.evalJson(`JSON.stringify({
    count: window.__BFR__.countOutbound({ type: "request_snapshot" }),
    total: window.__BFR__.outboundCount(),
  })`);
}

export async function connectionGeneration(driver) {
  return driver.evalJson("window.__AW_TEST__.connectionGenerationForTest()");
}

export async function waitForStableSockets(driver, minimum, { stableMs = 900, timeoutMs = 15000 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  let signature = null;
  let stableSince = 0;
  while (Date.now() < deadline) {
    last = await socketInfo(driver);
    const outbound = await countOutbound(driver, {});
    const next = JSON.stringify({ sockets: last, outbound: outbound.total });
    if ((last || []).length >= minimum && next === signature) {
      if (Date.now() - stableSince >= stableMs) return last;
    } else {
      signature = next;
      stableSince = Date.now();
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));
  }
  throw new Error(`fixture socket set did not stabilize with ${minimum} sockets\n${JSON.stringify(last)}`);
}

export async function beginRollbackRequest(driver, trace = null) {
  await driver.runJson(["fill", "#prompt-rollback-version", "999"]);
  trace?.push("ui-input", { selector: "#prompt-rollback-version", value: 999 });
  const before = await countOutbound(driver, {});
  await scrollIntoViewport(driver, '[data-prompt-action="rollback"]', trace);
  await driver.runJson(["click", '[data-prompt-action="rollback"]']);
  trace?.push("ui-click", { selector: "[data-prompt-action=rollback]" });
  const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "rollback" }, 8000, trace);
  await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "sent" && state.lastPromptFeedback?.action === "prompt_rollback", "rollback request sent");
  return sent.entry.message.command.request;
}

export function promptControlError(request, message = "late result for an already terminated request") {
  return {
    type: "prompt_control_error",
    request_auth_nonce: request.auth.nonce,
    error: {
      request_id: request.request_id,
      agent_id: request.agent_id,
      status: "blocked",
      reason_code: "control_lost",
      value_visibility: "hidden",
      next_step: "reauthenticate_and_refresh_binding",
      message,
    },
  };
}

export function promptControlApplied(request, version = 4, override = DRAFT_TEXT) {
  return {
    type: "prompt_control_ack",
    request_auth_nonce: request.auth.nonce,
    ack: {
      request_id: request.request_id,
      agent_id: request.agent_id,
      operation: "apply",
      status: "applied",
      version,
      applied_fields: ["system_prompt_override"],
      applied_scope: "current_runtime",
      persistence_scope: "none",
      sync_scope: "none",
      system_prompt_override: override,
    },
  };
}

export async function remountState(driver) {
  return driver.evalJson(`(() => {
    const api = window.__BFR_REMOUNT__;
    return JSON.stringify({
      available: Boolean(api),
      mountCount: api ? api.mountCount() : null,
      rootChildren: document.getElementById("app")?.childElementCount ?? null,
      viewerShells: document.querySelectorAll("#app .viewer-shell").length,
      promptPanels: document.querySelectorAll(".command-surface__prompt-panel").length,
      applyControls: document.querySelectorAll('[data-prompt-action="apply"]').length,
      detailsPanel: Boolean(document.getElementById("viewer-details-panel")),
    });
  })()`);
}

export async function disposeViewer(driver, trace = null) {
  const result = await driver.evalJson(`JSON.stringify((() => {
    const api = window.__BFR_REMOUNT__;
    if (!api) return { available: false };
    api.dispose();
    return { available: true, mountCount: api.mountCount() };
  })())`);
  trace?.push("dispose-viewer", result);
  assert(result.available === true, "visual-test page did not expose the real viewer disposer", result);
  return result;
}

export async function mountViewer(driver, trace = null) {
  const count = await driver.evalJson("JSON.stringify((() => window.__BFR_REMOUNT__.mount())())");
  trace?.push("mount-viewer", { mountCount: count });
  return count;
}

export async function ensurePromptEditorVisible(driver, trace = null) {
  const summarySelector = "#viewer-details-panel details.command-surface__advanced-details > summary";
  await scrollIntoViewport(driver, summarySelector, trace);
  const open = await driver.evalJson(`document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true`);
  if (!open) {
    await driver.runJson(["click", summarySelector]);
    trace?.push("ui-click", { selector: summarySelector });
    await waitForBrowserState(driver, `() => document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true`, "advanced prompt details open", 8000);
  }
  const visible = await driver.evalJson(`Boolean(document.querySelector(".command-surface__prompt-panel"))`);
  if (!visible) {
    await scrollIntoViewport(driver, "[data-prompt-visibility-toggle=\"1\"]", trace);
    await driver.runJson(["click", "[data-prompt-visibility-toggle=\"1\"]"]);
    trace?.push("ui-click", { selector: "[data-prompt-visibility-toggle]" });
  }
  await waitForBrowserState(driver, `() => Boolean(document.querySelector('.command-surface__prompt-panel')) && document.querySelector('[data-prompt-action="apply"]')?.disabled === false`, "prompt apply control enabled", 8000);
}

export async function openAdvancedPromptDetails(driver, trace = null) {
  const summarySelector = "#viewer-details-panel details.command-surface__advanced-details > summary";
  await driver.runJson(["scrollintoview", summarySelector]);
  await waitForBrowserState(
    driver,
    `() => {
      const entry = document.querySelector("#viewer-details-panel details.command-surface__advanced-details > summary");
      if (!entry) return false;
      const rect = entry.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    }`,
    "advanced prompt summary visible",
    8000,
  );
  const initial = await driver.evalJson(`(() => JSON.stringify({ open: document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true }))()`);
  assert(initial.open === false, "advanced prompt settings were already open before the player clicked the entry", initial);
  await driver.runJson(["click", summarySelector]);
  trace?.push("ui-click", { selector: summarySelector });
  await waitForBrowserState(
    driver,
    `() => document.querySelector("#viewer-details-panel details.command-surface__advanced-details")?.open === true`,
    "advanced prompt details open",
    8000,
  );
  trace?.push("ui-state", { selector: summarySelector, open: true });
}

export async function openPromptEditor(driver, trace = null) {
  await openAdvancedPromptDetails(driver, trace);
  await driver.runJson(["scrollintoview", "[data-prompt-visibility-toggle=\"1\"]"]);
  await driver.runJson(["click", "[data-prompt-visibility-toggle=\"1\"]"]);
  trace?.push("ui-click", { selector: "[data-prompt-visibility-toggle]" });
  await waitForBrowserState(driver, `() => Boolean(document.querySelector(".command-surface__prompt-panel")) && document.querySelector('[data-prompt-action="apply"]')?.disabled === false`, "prompt apply control enabled", 8000);
  await driver.runJson(["scrollintoview", "[data-prompt-action=\"apply\"]"]);
}

export async function typeDraft(driver, text, trace = null) {
  await driver.runJson(["fill", "#prompt-system", text]);
  trace?.push("ui-input", { selector: "#prompt-system", length: text.length });
}

export async function scrollIntoViewport(driver, selector, trace = null) {
  // The viewer scrolls inside its own panels, so a window-level scroll probe can
  // stay off-screen after a reconnect re-render. Scroll the owning container and
  // require a stable in-viewport rect before a real click.
  const deadline = Date.now() + 8000;
  let probe = null;
  while (Date.now() < deadline) {
    probe = await driver.evalJson(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element) return JSON.stringify({ found: false });
      let rect = element.getBoundingClientRect();
      for (let node = element.parentElement; node && node !== document.documentElement; node = node.parentElement) {
        const style = getComputedStyle(node);
        if (/(auto|scroll)/.test(style.overflowY)) {
          node.scrollTop = Math.max(0, rect.top - node.clientHeight / 2 + rect.height / 2 + node.scrollTop);
          rect = element.getBoundingClientRect();
        }
      }
      if (rect.top < 0 || rect.bottom > window.innerHeight) {
        const scroller = document.scrollingElement || document.documentElement;
        scroller.scrollTop += rect.top - window.innerHeight / 2 + rect.height / 2;
        rect = element.getBoundingClientRect();
      }
      return JSON.stringify({
        found: true,
        inView: rect.width > 0 && rect.height > 0 && rect.top >= 0 && rect.left >= 0
          && rect.bottom <= window.innerHeight && rect.right <= window.innerWidth,
        top: rect.top,
        bottom: rect.bottom,
        innerHeight: window.innerHeight,
      });
    })()`);
    if (probe?.found && probe.inView) {
      trace?.push("ui-scroll", { selector, top: probe.top });
      return;
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 80));
  }
  throw new Error(`could not scroll ${selector} into the viewport\n${JSON.stringify(probe)}`);
}

export async function clickApply(driver, trace = null) {
  await scrollIntoViewport(driver, "[data-prompt-action=\"apply\"]", trace);
  await driver.runJson(["click", "[data-prompt-action=\"apply\"]"]);
  trace?.push("ui-click", { selector: "[data-prompt-action=apply]" });
}

// ---- case execution -----------------------------------------------------

export function caseUrl(port, { hash = "", extra = "" } = {}) {
  return `http://127.0.0.1:${port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en${extra}${hash}`;
}

export async function bootstrapPromptScenario(driver, phaseLog, trace = null) {
  await waitOutbound(driver, 0, { type: "hello_v2" }, 8000, trace);
  phaseLog.push("hello_v2 outbound observed");
  await deliver(driver, helloAck, trace);
  const reconnect = await waitOutbound(driver, 0, { type: "authoritative_recovery", mode: "reconnect_sync" }, 8000, trace);
  phaseLog.push("reconnect_sync outbound observed");
  const identity = await driver.evalJson("JSON.stringify(window.__BFR__.identity())");
  assert(identity.playerId && identity.publicKey, "local test session identity was not issued", identity);
  await deliver(driver, {
    type: "authoritative_recovery_ack",
    ack: {
      status: "catch_up_ready",
      player_id: identity.playerId,
      session_pubkey: identity.publicKey,
      agent_id: "agent-0",
      session_epoch: 12,
      binding_epoch: 9,
    },
  }, trace);
  await waitForState(driver, (state) => state.authRegistrationStatus === "registered" && state.authBoundAgentId === "agent-0" && state.authBindingEpoch === 9 && state.authSessionEpoch === 12, "initial fixture auth binding registered");
  phaseLog.push("player session and Agent binding registered through authoritative recovery ack");
  await deliver(driver, { type: "snapshot", snapshot: promptSnapshot() }, trace);
  await waitForState(driver, (state) => state.selectedKind === "agent" && state.selectedId === "agent-0", "fixture agent selected");
  phaseLog.push("canonical snapshot adopted and agent-0 selected");
  return { reconnect, identity };
}
