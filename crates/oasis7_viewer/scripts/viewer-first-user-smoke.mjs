#!/usr/bin/env node
import { loadPlaywright } from "../../../scripts/playwright-module-loader.mjs";
import { mkdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

const { chromium } = loadPlaywright(import.meta.url);

function usage() {
  console.log(`Usage: node crates/oasis7_viewer/scripts/viewer-first-user-smoke.mjs --url <url> --out-dir <path> [options]

Options:
  --url <url>             Viewer URL to open.
  --out-dir <path>        Artifact directory.
  --chat-message <text>   Smoke chat message.
  --timeout-ms <ms>       Overall readiness timeout (default: 45000).
  --chrome-path <path>    Use a system Chrome/Chromium executable.
  --headed                Run headed instead of headless.
  --help                  Show help.`);
}

function parseArgs(argv) {
  const options = {
    url: "",
    outDir: "",
    chatMessage: "本地启动 smoke：请确认我可以给 Agent 发消息。",
    timeoutMs: 45000,
    chromePath: process.env.OASIS7_PLAYWRIGHT_CHROME_PATH || "",
    headed: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case "--url":
        options.url = argv[++index] || "";
        break;
      case "--out-dir":
        options.outDir = argv[++index] || "";
        break;
      case "--chat-message":
        options.chatMessage = argv[++index] || "";
        break;
      case "--timeout-ms":
        options.timeoutMs = Number(argv[++index] || "0");
        break;
      case "--chrome-path":
        options.chromePath = argv[++index] || "";
        break;
      case "--headed":
        options.headed = true;
        break;
      case "--help":
      case "-h":
        usage();
        process.exit(0);
        break;
      default:
        throw new Error(`unknown option: ${arg}`);
    }
  }
  if (!options.url) throw new Error("--url is required");
  if (!options.outDir) throw new Error("--out-dir is required");
  if (!options.chatMessage) throw new Error("--chat-message cannot be empty");
  if (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0) {
    throw new Error("--timeout-ms must be a positive number");
  }
  return options;
}

function viewerUrl(rawUrl) {
  const url = new URL(rawUrl);
  url.searchParams.set("test_api", "1");
  url.searchParams.set("render_mode", "viewer");
  return url.toString();
}

async function getState(page) {
  return await page.evaluate(() => window.__AW_TEST__?.getState?.() ?? null);
}

async function installWebSocketRecorder(page) {
  await page.addInitScript(() => {
    const NativeWebSocket = window.WebSocket;
    window.__OASIS7_SMOKE_WS_SENT__ = [];
    window.__OASIS7_SMOKE_WS_RECEIVED__ = [];
    window.WebSocket = class extends NativeWebSocket {
      constructor(...args) {
        super(...args);
        this.addEventListener("message", (event) => {
          try {
            window.__OASIS7_SMOKE_WS_RECEIVED__.push(JSON.parse(event.data));
          } catch (_) {
          }
        });
      }

      send(data) {
        try {
          window.__OASIS7_SMOKE_WS_SENT__.push(JSON.parse(data));
        } catch (_) {
        }
        return super.send(data);
      }
    };
    window.WebSocket.CONNECTING = NativeWebSocket.CONNECTING;
    window.WebSocket.OPEN = NativeWebSocket.OPEN;
    window.WebSocket.CLOSING = NativeWebSocket.CLOSING;
    window.WebSocket.CLOSED = NativeWebSocket.CLOSED;
  });
}

async function getRecordedWebSocketTraffic(page) {
  return await page.evaluate(() => ({
    sent: window.__OASIS7_SMOKE_WS_SENT__ || [],
    received: window.__OASIS7_SMOKE_WS_RECEIVED__ || [],
  }));
}

async function writeJson(path, payload) {
  await writeFile(path, JSON.stringify(payload, null, 2), "utf8");
}

async function writeBestEffortScreenshot(page, path) {
  try {
    await page.screenshot({ path, fullPage: true, timeout: 10000 });
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error?.message || String(error) };
  }
}

function gameplayEntityCount(state, key) {
  return Number(
    state?.gameplaySummary?.entityCounts?.[key]
      ?? state?.gameplaySummary?.[`${key.slice(0, -1)}Count`]
      ?? state?.gameplaySummary?.[`${key}Count`]
      ?? 0,
  );
}

function summarizeState(state) {
  const agents = state?.snapshot?.model?.agents ?? {};
  const locations = state?.snapshot?.model?.locations ?? {};
  const runtimeState = state?.snapshot?.model?.state ?? state?.snapshot?.state ?? {};
  const starterOcClaims = runtimeState.starter_oc_claims
    ?? runtimeState.starterOcClaims
    ?? state?.snapshot?.model?.starter_oc_claims
    ?? state?.snapshot?.model?.starterOcClaims
    ?? state?.snapshot?.starter_oc_claims
    ?? state?.snapshot?.starterOcClaims
    ?? {};
  const auth = state?.auth ?? {
    available: state?.authReady ?? null,
    playerId: state?.authPlayerId ?? null,
    registrationStatus: state?.authRegistrationStatus ?? null,
    runtimeStatus: state?.authRuntimeStatus ?? null,
    boundAgentId: state?.authBoundAgentId ?? null,
    recoveryErrorCode: state?.authRecoveryErrorCode ?? null,
    recoveryErrorMessage: state?.authRecoveryErrorMessage ?? null,
  };
  return {
    connectionStatus: state?.connectionStatus ?? null,
    lastError: state?.lastError ?? null,
    auth,
    agentCount: Object.keys(agents).length || gameplayEntityCount(state, "agents"),
    locationCount: Object.keys(locations).length || gameplayEntityCount(state, "locations"),
    blockerKind: state?.snapshot?.player_gameplay?.blocker_kind ?? null,
    blockerDetail: state?.snapshot?.player_gameplay?.blocker_detail ?? null,
    starterOcClaimed: Object.keys(starterOcClaims).length > 0,
    lastChatFeedback: state?.lastChatFeedback ?? null,
  };
}

async function selectBoundAgent(page) {
  const result = await page.evaluate(() => {
    const api = window.__AW_TEST__;
    const state = api?.getState?.();
    const boundAgentId = state?.auth?.boundAgentId ?? state?.authBoundAgentId;
    if (!boundAgentId) {
      return { ok: false, reason: "bound agent missing" };
    }
    const selectedId = state?.selectedId;
    const selectedKind = state?.selectedKind;
    if (selectedKind === "agent" && selectedId === boundAgentId) {
      return { ok: true, kind: "agent", id: boundAgentId, alreadySelected: true };
    }
    return api?.select?.({ kind: "agent", id: boundAgentId }) ?? { ok: false, reason: "select api missing" };
  });
  if (!result?.ok) {
    throw new Error(`failed to select bound agent: ${result?.reason || JSON.stringify(result)}`);
  }
  await page.evaluate(() => {
    const details = document.querySelector("#viewer-details-panel");
    details?.scrollIntoView?.({ block: "start", inline: "nearest" });
    window.location.hash = "viewer-details-panel";
  });
  await page.waitForFunction(() => {
    const state = window.__AW_TEST__?.getState?.();
    const boundAgentId = state?.auth?.boundAgentId ?? state?.authBoundAgentId;
    return state?.selectedKind === "agent" && state?.selectedId === boundAgentId;
  }, null, { timeout: 5000 });
  return result;
}

async function clickClaimFirstAgent(page) {
  const testIdButton = page.getByTestId("viewer-playthrough-action-claim-first-agent").first();
  if (await testIdButton.count()) {
    const disabled = await testIdButton.evaluate((button) => button.disabled).catch(() => true);
    if (!disabled) {
      await testIdButton.click();
      return { clicked: true, selector: "data-testid" };
    }
    return { clicked: false, selector: "data-testid", disabled: true };
  }

  const roleButton = page.getByRole("button", { name: /^(Claim First Agent|认领第一个 Agent)$/ }).first();
  if (await roleButton.count()) {
    const disabled = await roleButton.evaluate((button) => button.disabled).catch(() => true);
    if (!disabled) {
      await roleButton.click();
      return { clicked: true, selector: "role" };
    }
    return { clicked: false, selector: "role", disabled: true };
  }

  return { clicked: false, selector: "missing" };
}

function chromeExecutablePath(requestedPath) {
  if (requestedPath && existsSync(requestedPath)) return requestedPath;
  const macChrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  if (existsSync(macChrome)) return macChrome;
  return undefined;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  options.url = viewerUrl(options.url);
  options.outDir = resolve(options.outDir);
  await mkdir(options.outDir, { recursive: true });

  const browser = await chromium.launch({
    headless: !options.headed,
    executablePath: chromeExecutablePath(options.chromePath),
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await installWebSocketRecorder(page);
  const consoleLines = [];
  page.on("console", (message) => {
    consoleLines.push(`[${message.type()}] ${message.text()}`);
  });
  page.on("pageerror", (error) => {
    consoleLines.push(`[pageerror] ${error.stack || error.message}`);
  });

  let finalState = null;
  let sendResult = null;
  let websocketTraffic = { sent: [], received: [] };
  try {
    await page.goto(options.url, { waitUntil: "domcontentloaded", timeout: options.timeoutMs });
    await page.waitForFunction(() => typeof window.__AW_TEST__ === "object", null, { timeout: options.timeoutMs });
    await page.waitForFunction(() => window.__AW_TEST__?.getState?.()?.connectionStatus === "connected", null, { timeout: options.timeoutMs });

    await page.waitForFunction(() => {
      const state = window.__AW_TEST__?.getState?.();
      if (state?.auth?.boundAgentId ?? state?.authBoundAgentId) {
        return true;
      }
      const testIdButton = document.querySelector('[data-testid="viewer-playthrough-action-claim-first-agent"]');
      if (testIdButton && !testIdButton.disabled) {
        return true;
      }
      return Array.from(document.querySelectorAll("button")).some((button) => (
        /^(Claim First Agent|认领第一个 Agent)$/.test(button.textContent?.trim() || "")
        && !button.disabled
      ));
    }, null, { timeout: options.timeoutMs });
    const claimFirstAgentResult = await clickClaimFirstAgent(page);
    await writeJson(join(options.outDir, "claim_first_agent_result.json"), claimFirstAgentResult);

    await page.waitForFunction(() => {
      const state = window.__AW_TEST__?.getState?.();
      const agents = state?.snapshot?.model?.agents ?? {};
      const agentCount = Object.keys(agents).length
        || Number(state?.gameplaySummary?.entityCounts?.agents ?? state?.gameplaySummary?.agentCount ?? 0);
      const registrationStatus = state?.auth?.registrationStatus ?? state?.authRegistrationStatus;
      const boundAgentId = state?.auth?.boundAgentId ?? state?.authBoundAgentId;
      const bodyText = document.body?.innerText ?? "";
      const emptyEntityVisible = bodyText.includes("当前快照里没有行动体")
        || bodyText.includes("runtime_snapshot_empty_entities");
      return (agentCount >= 1 || !emptyEntityVisible)
        && registrationStatus === "registered"
        && Boolean(boundAgentId);
    }, null, { timeout: options.timeoutMs });

    await writeJson(join(options.outDir, "ready_state.json"), await getState(page));

    const selectResult = await selectBoundAgent(page);
    await writeJson(join(options.outDir, "select_result.json"), selectResult);

    await page.waitForFunction(() => !/pixel_world_render_state_unavailable/.test(document.body?.innerText ?? ""), null, {
      timeout: options.timeoutMs,
    });

    const claimStarterOcButton = page.getByTestId("viewer-playthrough-action-claim-starter-oc").first();
    if (await claimStarterOcButton.count()) {
      await page.waitForFunction(() => {
        const button = document.querySelector('[data-testid="viewer-playthrough-action-claim-starter-oc"]');
        return button && !button.disabled;
      }, null, { timeout: options.timeoutMs });
      await claimStarterOcButton.click();
      await page.waitForFunction(() => {
        const state = window.__AW_TEST__?.getState?.();
        const boundAgentId = state?.auth?.boundAgentId ?? state?.authBoundAgentId;
        const snapshot = state?.snapshot ?? {};
        const model = snapshot.model ?? {};
        const runtimeState = model.state ?? snapshot.state ?? {};
        const claims = runtimeState.starter_oc_claims
          || runtimeState.starterOcClaims
          || model.starter_oc_claims
          || model.starterOcClaims
          || snapshot.starter_oc_claims
          || snapshot.starterOcClaims
          || {};
        const balances = runtimeState.main_token_balances
          || runtimeState.mainTokenBalances
          || model.main_token_balances
          || model.mainTokenBalances
          || snapshot.main_token_balances
          || snapshot.mainTokenBalances
          || {};
        const balance = balances[boundAgentId] || null;
        const liquidBalance = Number(
          balance?.liquid_balance
            ?? balance?.liquidBalance
            ?? balance?.liquid
            ?? balance?.balance
            ?? 0,
        );
        return Boolean(boundAgentId && claims[boundAgentId])
          || (Number.isFinite(liquidBalance) && liquidBalance > 0)
          || Boolean(document.querySelector("#agent-chat-message") && !document.querySelector("#agent-chat-message").disabled);
      }, null, { timeout: options.timeoutMs });
    }

    await page.waitForSelector("#agent-chat-message", { state: "visible", timeout: options.timeoutMs });
    await page.waitForSelector('[data-chat-send="1"]', { state: "visible", timeout: options.timeoutMs });
    await page.waitForFunction(() => {
      const input = document.querySelector("#agent-chat-message");
      const button = document.querySelector('[data-chat-send="1"]');
      return input && !input.disabled && button && !button.disabled;
    }, null, { timeout: options.timeoutMs });

    sendResult = await page.evaluate((message) => {
      const state = window.__AW_TEST__?.getState?.();
      const agentId = state?.auth?.boundAgentId ?? state?.authBoundAgentId ?? state?.selectedId;
      return window.__AW_TEST__?.sendAgentChat?.(agentId, message);
    }, options.chatMessage);
    if (!sendResult?.ok && !sendResult?.feedback?.accepted) {
      throw new Error(`sendAgentChat rejected before send: ${sendResult?.reason || JSON.stringify(sendResult)}`);
    }
    await writeJson(join(options.outDir, "send_result.json"), sendResult);
    await page.waitForFunction(() => (
      (window.__OASIS7_SMOKE_WS_SENT__ || []).some((message) => message?.type === "agent_chat")
    ), null, { timeout: options.timeoutMs });
    await page.waitForFunction(() => (
      (window.__OASIS7_SMOKE_WS_RECEIVED__ || []).some((message) => (
        message?.type === "agent_chat_ack" || message?.type === "agent_chat_error"
      ))
    ), null, { timeout: options.timeoutMs });

    finalState = await getState(page);
    websocketTraffic = await getRecordedWebSocketTraffic(page);
    const chatError = websocketTraffic.received.find((message) => message?.type === "agent_chat_error");
    if (chatError) {
      throw new Error(`agent chat failed: ${chatError.error_code || chatError.code || JSON.stringify(chatError)}`);
    }
    if (!websocketTraffic.received.some((message) => message?.type === "agent_chat_ack")) {
      throw new Error("agent chat did not receive ack");
    }
    await writeJson(join(options.outDir, "websocket_traffic.json"), websocketTraffic);
    await writeJson(join(options.outDir, "final_state.json"), finalState);
    const screenshotResult = await writeBestEffortScreenshot(page, join(options.outDir, "viewer-first-user-smoke.png"));
    if (!screenshotResult.ok) {
      await writeJson(join(options.outDir, "screenshot_warning.json"), screenshotResult);
    }
  } catch (error) {
    finalState = await getState(page).catch(() => null);
    await writeJson(join(options.outDir, "failure_state.json"), {
      error: error.message,
      summary: summarizeState(finalState),
      state: finalState,
      websocketTraffic: await getRecordedWebSocketTraffic(page).catch(() => null),
    });
    await writeBestEffortScreenshot(page, join(options.outDir, "failure.png"));
    throw error;
  } finally {
    await writeFile(join(options.outDir, "browser-console.log"), consoleLines.join("\n") + "\n", "utf8");
    await browser.close();
  }

  const summary = {
    ok: true,
    url: options.url,
    artifactPath: options.outDir,
    chatMessage: options.chatMessage,
    agentChatSent: websocketTraffic.sent.some((message) => message?.type === "agent_chat"),
    agentChatCompleted: websocketTraffic.received.some((message) => message?.type === "agent_chat_ack"),
    sendResult,
    state: summarizeState(finalState),
  };
  await writeJson(join(options.outDir, "viewer-first-user-smoke-summary.json"), summary);
  console.log(`ok: viewer first-user smoke passed; agent=${summary.state.auth?.boundAgentId ?? "<none>"}`);
  console.log(`artifacts=${options.outDir}`);
}

main().catch((error) => {
  console.error(`error: ${error.message}`);
  process.exit(1);
});
