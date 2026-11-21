import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const suite = "software_safe_src/control_proof_panel.test.jsx";
const snapshots = {
  OASIS7_AGENCY_BACKEND_SNAPSHOT: "restored.json",
  OASIS7_AGENCY_STALE_BACKEND_SNAPSHOT: "stale.json",
};
const expected = [
  ["actual restored Rust owner snapshot bridge",
    "preserves durable correction and canonical receipt identity through feedback and the rendered panel"],
  ["actual stale-replacement owner snapshot bridge",
    "keeps the earliest prepared identity separate from the committed replacement and its receipt"],
];

export function verifyAgencySnapshotReceipt(report, expectedFile) {
  assert.equal(report.success, true, "Viewer bridge suite failed");
  assert.equal(report.numFailedTests, 0);
  assert.equal(report.numPassedTests, expected.length);
  assert.equal(report.testResults.length, 1);
  assert.equal(report.testResults[0].name, expectedFile);
  const results = report.testResults[0].assertionResults;
  for (const [group, title] of expected) {
    const matches = results.filter(
      (result) => result.title === title && result.ancestorTitles.includes(group),
    );
    assert.equal(matches.length, 1, `expected one Viewer bridge test: ${title}`);
    assert.equal(matches[0].status, "passed", `Viewer bridge test did not pass: ${title}`);
  }
}

export function runAgencySnapshotBridge(directory) {
  assert.ok(directory, "expected the fresh Rust snapshot directory");
  const viewerRoot = fileURLToPath(new URL("../", import.meta.url));
  const reportPath = resolve(directory, "vitest.json");
  const env = { ...process.env };
  for (const [variable, filename] of Object.entries(snapshots)) {
    const path = resolve(directory, filename);
    const snapshot = JSON.parse(readFileSync(path, "utf8"));
    assert.ok(snapshot?.player_gameplay?.primary_intent?.agency_read_model, `missing agency snapshot: ${filename}`);
    env[variable] = path;
  }
  execFileSync(process.execPath, [
    resolve(viewerRoot, "node_modules/vitest/vitest.mjs"), "run", suite,
    "--testNamePattern", expected.map(([group]) => group).join("|"),
    "--reporter", "json", "--outputFile", reportPath,
  ], { cwd: viewerRoot, env, stdio: "inherit" });
  verifyAgencySnapshotReceipt(JSON.parse(readFileSync(reportPath, "utf8")), resolve(viewerRoot, suite));
  console.log("Viewer Rust snapshot bridge: restored and stale-replacement tests passed (2/2)");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runAgencySnapshotBridge(process.argv[2]);
}
