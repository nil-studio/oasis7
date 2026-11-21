import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const scriptPath = new URL("../../../scripts/viewer-gameplay-attraction-ui-click-playthrough.sh", import.meta.url);
const source = readFileSync(scriptPath, "utf8");

assert.match(source, /find_testid_click\(\)/, "UI-click runner must click player-visible controls by test id");
assert.match(source, /find_selector_click\(\)/, "UI-click runner must click player-visible controls by selector when a target id is required");
assert.match(source, /data-select-kind=\\"agent\\".*data-select-id=\\"\$\{AGENT_ID\}\\"/, "runner must click the requested target-agent control");
assert.match(source, /viewer-playthrough-action-step/, "runner must click the visible step control");
assert.match(source, /viewer-playthrough-action-request-snapshot/, "runner must click the visible refresh-snapshot control");
assert.match(source, /viewer-playthrough-action-recommended/, "runner must click the visible recommended-action control");
assert.match(source, /viewer-gameplay-details/, "runner must assert the visible gameplay proof details surface");
assert.doesNotMatch(
  source,
  /__AW_TEST__\.(sendControl|sendGameplayAction|runSteps)\s*\(/,
  "__AW_TEST__ may be used for assertions/evidence only, not for gameplay progression in the UI-click runner",
);

const automationSource = readFileSync(
  new URL("../../../scripts/verify-gameplay-attraction-automation.sh", import.meta.url),
  "utf8",
);
assert.match(
  automationSource,
  /live_browser_30m_ui_click_playthrough/,
  "live automation tier must include the actual UI-click playthrough command",
);

console.log("TASK-GAME-076 UI-click playthrough guard passed");
