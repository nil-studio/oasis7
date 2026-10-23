import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Ajv2020 } from "ajv/dist/2020.js";
import type {
  DecimalU64, ReadInfo, ReadProjection, ReadError, ReadWorldViewRequest,
  TaskControlBootstrap, GoalReference, ExecutorBinding, Operations,
} from "../protocol_codegen/generated/types.js";

// Source locations are relative to this package, including when executed from .build.
const here = fileURLToPath(new URL("../../", import.meta.url));
const schema = JSON.parse(readFileSync(`${here}../protocol_codegen/generated/schema.json`, "utf8"));
const fixtures = JSON.parse(readFileSync(`${here}../../oasis7_viewer_runtime/tests/fixtures/signed_read_bootstrap.json`, "utf8")) as {
  cases: { name: string; path: string; status: number; body: unknown; request?: unknown }[];
};
const ajv = new Ajv2020({ strict: false, allErrors: true });
ajv.addSchema(schema, "bootstrap");
let assertions = 0;
function expectSchema(name: string, value: unknown, expected = true): void {
  const validate = ajv.compile({ $ref: `bootstrap#/$defs/${name}` });
  assert.equal(validate(value), expected, `${name}: ${JSON.stringify(value)}: ${ajv.errorsText(validate.errors)}`);
  assertions++;
}
type Operation = { method: string; path: string; responses: Record<string, object> };
const operations = schema["x-operations"] as Record<string, Operation>;
const successFixtures = ["info", "openapi", "observation", "capabilities", "task-control"];
function validateResponse(bundle: typeof schema, fixture: typeof fixtures.cases[number]): void {
  // Match literal segments and nonempty single-segment path parameters. Fixtures
  // come from real GET handlers, independently of the generated operations.
  const matches = Object.entries(bundle["x-operations"] as Record<string, Operation>).filter(([, operation]) => {
    const template = operation.path.split("/");
    const actual = fixture.path.split("/");
    return operation.method === "GET" && template.length === actual.length &&
      template.every((part, index) => /^\{[^{}]+\}$/.test(part) ? actual[index].length > 0 : part === actual[index]);
  });
  assert.equal(matches.length, 1, `${fixture.name}: GET ${fixture.path}: expected unique operation`);
  const [id, operation] = matches[0];
  const response = operation.responses[String(fixture.status)];
  assert.ok(response, `${fixture.name}: ${id}: undeclared status ${fixture.status}`);
  const validate = ajv.compile({ ...bundle, ...response });
  assert.ok(validate(fixture.body), `${fixture.name}: ${id}/${fixture.status}: ${ajv.errorsText(validate.errors)}`);
}
// Fault injection uses unchanged handler inputs and mutates only the contract.
for (const [label, mutate] of [
  ["missing 400", (s: typeof schema) => { delete s["x-operations"].getAgentObservation.responses["400"]; }],
  ["wrong path", (s: typeof schema) => { s["x-operations"].getAgentObservation.path += "/wrong"; }],
  ["wrong method", (s: typeof schema) => { s["x-operations"].getAgentObservation.method = "POST"; }],
  ["wrong error schema", (s: typeof schema) => { s["x-operations"].getAgentObservation.responses["400"] = { $ref: "#/$defs/ReadInfo" }; }],
  ["ambiguous path", (s: typeof schema) => { s["x-operations"].duplicate = structuredClone(s["x-operations"].getAgentObservation); }],
] as const) {
  const changed = structuredClone(schema);
  mutate(changed);
  assert.throws(() => fixtures.cases.forEach(fixture => validateResponse(changed, fixture)), Error, label);
}
function assertProjectionIntegersAreStrings(value: unknown): void {
  if (typeof value === "number") assert.equal(Number.isInteger(value), false, "projection integer remained numeric");
  else if (Array.isArray(value)) value.forEach(assertProjectionIntegersAreStrings);
  else if (value !== null && typeof value === "object") Object.values(value).forEach(assertProjectionIntegersAreStrings);
}
for (const fixture of fixtures.cases) {
  assert.ok(Buffer.byteLength(JSON.stringify(fixture.body)) <= 2097152, `${fixture.name}: response byte limit`);
  validateResponse(schema, fixture);
  assertions++;
  if (fixture.status === 200 && ["observation", "capabilities", "task-control"].includes(fixture.name)) {
    const projection = fixture.body as ReadProjection;
    assertProjectionIntegersAreStrings(projection.world_binding);
    assertProjectionIntegersAreStrings(projection.data);
  }
  if (fixture.request !== undefined && fixture.status === 200) expectSchema("ReadWorldViewRequest", fixture.request);
}
for (const name of successFixtures) assert.ok(fixtures.cases.some(f => f.name === name && f.status === 200));
// These functions exercise real generated types as a consumer, including per-operation responses.
const info = fixtures.cases.find(f => f.name === "info")!.body as ReadInfo;
const observation = fixtures.cases.find(f => f.name === "observation")!.body as ReadProjection;
const error = fixtures.cases.find(f => f.status !== 200)!.body as ReadError;
function consumeInfo(value: Operations["getGameReadInfo"]["responses"]["200"]): false { return value.writes_available; }
assert.equal(consumeInfo(info), false);
const tick: DecimalU64 = observation.logical_tick;
assert.equal(typeof tick, "string");
assert.equal(typeof error.retryable, "boolean");
expectSchema("ReadInfo", { ...info, future_extension: true });
expectSchema("ReadProjection", { ...observation, future_extension: true });
expectSchema("ReadError", { ...error, future_extension: true });
expectSchema("ReadInfo", { ...info, api_revision: "1" }, false);
expectSchema("ReadInfo", { ...info, writes_available: true }, false);
expectSchema("ReadProjection", { ...observation, logical_tick: 1 }, false);
// Dynamic projection JSON may contain signed integer strings, never inferred as DecimalU64.
expectSchema("ReadProjection", { ...observation, data: { signed: "-9223372036854775808" } });
const max = "18446744073709551615";
for (const value of ["0", "9007199254740991", "9007199254740992", max]) expectSchema("DecimalU64", value);
for (const value of ["18446744073709551616", "00", "01", "+1", "-1", "1\n", "1.0", "", 1, null]) expectSchema("DecimalU64", value, false);
// Independent arithmetic oracle checks the schema's range expression beyond golden endpoints.
const decimalPattern = new RegExp(schema.$defs.DecimalU64.pattern, "u");
let seed = 123456789n;
const checkInteger = (value: bigint): void => {
  assert.equal(decimalPattern.test(value.toString()), value >= 0n && value <= BigInt(max));
  assertions++;
};
for (let delta = -256n; delta <= 256n; delta++) checkInteger(BigInt(max) + delta);
for (let digits = 1n; digits <= 20n; digits++) {
  for (let delta = -2n; delta <= 2n; delta++) checkInteger(10n ** digits + delta);
}
for (let index = 0; index < 4096; index++) {
  seed = (seed * 6364136223846793005n + 1442695040888963407n) % (1n << 66n);
  checkInteger(seed);
}
const request: ReadWorldViewRequest = { contract_version: 1, world: { world_id: "world", genesis_digest: "digest" }, scope_id: "agent:1" };
// Compile-time assertions fail if generated request/discovery contracts broaden.
// @ts-expect-error This profile only permits null/omitted fixed_commit.
const historicalRequest: ReadWorldViewRequest = { ...request, fixed_commit: {} };
// @ts-expect-error Discovery remains strictly read-only.
const writableInfo: ReadInfo = { ...info, writes_available: true };
void historicalRequest;
void writableInfo;
expectSchema("ReadWorldViewRequest", request);
expectSchema("ReadWorldViewRequest", { ...request, fixed_commit: null });
expectSchema("ReadWorldViewRequest", { ...request, fixed_commit: {} }, false);
expectSchema("ReadWorldViewRequest", { ...request, unknown: true }, false);
expectSchema("ReadWorldViewRequest", { ...request, world: { ...request.world, unknown: true } }, false);
const task = fixtures.cases.find(f => f.name === "task-control")!.body as ReadProjection;
const goal: GoalReference = { goal_revision: max, goal_ref: "a".repeat(64), prepare_operation_id: "prepare", publish_operation_id: "publish", activation_committed_height: max };
const binding: ExecutorBinding = { executor_id: "executor", executor_public_key: "b".repeat(64), delegation_generation: max, executor_epoch: max };
const control = { ...task.data, active_goal_reference: goal, executor_binding: binding } as TaskControlBootstrap;
expectSchema("TaskControlBootstrap", control);
for (const field of ["reorg_epoch", "owner_generation", "task_revision", "latest_goal_revision", "active_goal_revision", "executor_epoch"]) {
  expectSchema("TaskControlBootstrap", { ...control, [field]: max });
  expectSchema("TaskControlBootstrap", { ...control, [field]: "18446744073709551616" }, false);
}
for (const [name, value, fields] of [
  ["GoalReference", goal, ["goal_revision", "activation_committed_height"]],
  ["ExecutorBinding", binding, ["delegation_generation", "executor_epoch"]],
] as const) for (const field of fields) {
  expectSchema(name, { ...value, [field]: "18446744073709551616" }, false);
  expectSchema(name, { ...value, [field]: "0" }, false);
}
const validateTaskResponse = ajv.compile({ ...schema, ...operations.getAgentTaskControl.responses["200"] as object });
assert.equal(validateTaskResponse({ ...task, data: { ...control, task_revision: "18446744073709551616" } }), false);
assert.equal(validateTaskResponse({ ...task, data: { ...control, active_goal_reference: { ...goal, goal_revision: "0" } } }), false);
assertions += 2;
for (const [key, value, fields] of [
  ["active_goal_reference", goal, ["goal_revision", "activation_committed_height"]],
  ["executor_binding", binding, ["delegation_generation", "executor_epoch"]],
] as const) for (const field of fields) {
  assert.equal(validateTaskResponse({ ...task, data: { ...control, [key]: { ...value, [field]: "18446744073709551616" } } }), false);
  assertions++;
}
expectSchema("TaskControlBootstrap", { ...control, executor_id: null });
expectSchema("TaskControlBootstrap", { ...control, schema_revision: 1 }, false);
console.log(`signed-read TS consumer: ${fixtures.cases.length} real handler fixtures; ${assertions} schema assertions passed`);
