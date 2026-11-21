import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const scriptPath = new URL("../../../scripts/viewer-gameplay-attraction-ui-click-playthrough.sh", import.meta.url);

function runBeat(failingCommand = "", recommendedDisabled = "false") {
  const driver = String.raw`
source "$1"
failing_command=$2
recommended_disabled=$3
session=recorded-session
AGENT_ID=agent-42
BEAT_TIMEOUT_MS=999
ab_log=/dev/null

ab_cmd() {
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" >&3
  if [[ "$2" == "$failing_command" ]]; then return 7; fi
}
ab_eval() {
  printf 'recommendation-check\n' >&3
  printf '%s\n' "$recommended_disabled"
}
log_note() { :; }
sleep_ms() { :; }
capture_state() { printf 'state.json\n'; }
wait_for_js_true() { printf 'assertion\t%s\t%s\n' "$@" >&3; }
record_beat() { printf 'record\t%s\t%s\t%s\n' "$5" "$6" "$7" >&3; }

exec 3>&1
if run_click_beat 00m beat click \
  'agent:agent-42,testid:viewer-playthrough-action-step,maybe:viewer-playthrough-action-recommended,viewer-playthrough-action-request-snapshot' \
  assertion 'read-only-assertion'; then
  printf 'result\tpass\n'
else
  printf 'result\tfail\n'
fi
`;
  const result = spawnSync("bash", ["-c", driver, "test-runner", fileURLToPath(scriptPath), failingCommand, recommendedDisabled], { encoding: "utf8" });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trimEnd().split("\n");
}

const agent = '[data-select-kind="agent"][data-select-id="agent-42"]';
const step = '[data-testid="viewer-playthrough-action-step"]';
const recommended = '[data-testid="viewer-playthrough-action-recommended"]';
const snapshot = '[data-testid="viewer-playthrough-action-request-snapshot"]';
const scroll = selector => `recorded-session\tscrollintoview\t${selector}`;
const click = selector => `recorded-session\tclick\t${selector}`;
const pass = ["assertion\tread-only-assertion\t999", "record\tpass\t\tstate.json", "result\tpass"];
const fail = ["record\tfail\tui click failed\tstate.json", "result\tfail"];

test("UI beat dispatches actual control clicks before asserting state", () => {
  assert.deepEqual(runBeat(), [
    scroll(agent), click(agent), scroll(step), click(step), "recommendation-check",
    scroll(recommended), click(recommended), scroll(snapshot), click(snapshot), ...pass,
  ]);
});

test("disabled recommendation is observed but not clicked", () => {
  assert.deepEqual(runBeat("", "true"), [
    scroll(agent), click(agent), scroll(step), click(step), "recommendation-check",
    scroll(snapshot), click(snapshot), ...pass,
  ]);
});

test("failed browser commands prevent later targets and success assertions", () => {
  assert.deepEqual(runBeat("scrollintoview"), [scroll(agent), ...fail]);
  assert.deepEqual(runBeat("click"), [scroll(agent), click(agent), ...fail]);
});

test("live tier keeps the visible gameplay evidence and does not drive it via test API", () => {
  const source = readFileSync(scriptPath, "utf8");
  assert.match(source, /viewer-gameplay-details/);
  assert.doesNotMatch(source, /__AW_TEST__\.(sendControl|sendGameplayAction|runSteps)\s*\(/);
  const automation = readFileSync(new URL("../../../scripts/verify-gameplay-attraction-automation.sh", import.meta.url), "utf8");
  assert.match(automation, /live_browser_30m_ui_click_playthrough/);
});
