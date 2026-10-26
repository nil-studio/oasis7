import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import Ajv from 'ajv/dist/2020.js';
import { declarations, schemaType } from '../scripts/schema-to-typescript.mjs';
const contract = JSON.parse(readFileSync(new URL('../contract/http.schema.json', import.meta.url), 'utf8'));
const ajv = new Ajv({ strict: true, allowUnionTypes: true });
for (const format of ['uint8', 'uint32', 'int32']) ajv.addFormat(format, { type: 'number', validate: Number.isInteger });
const validators = Object.fromEntries(Object.entries(contract.native).map(([name, schema]) => [name, ajv.compile(schema)]));
const copy = value => structuredClone(value);

test('actual Rust native HTTP fixtures satisfy their derived schemas', () => {
  for (const [name, validate] of Object.entries(validators)) {
    for (const fixture of contract.fixtures[name]) assert.ok(validate(fixture), `${name}: ${JSON.stringify(validate.errors)}`);
  }
});
test('canonical decimal strings preserve the exact u64 boundary without Number', () => {
  const schema = contract.native.CustodySessionStatusV1;
  const validate = validators.CustodySessionStatusV1;
  const fixture = contract.fixtures.CustodySessionStatusV1[0];
  for (const value of ['0', '9007199254740993', '18446744073709551614', '18446744073709551615'])
    assert.ok(validate({ ...fixture, expires_at: value }), value);
  for (const value of [0, 9007199254740992, '', '01', '+1', '-0', '-1', '1.0', '1e3',
    '18446744073709551616', '99999999999999999999', '184467440737095516150'])
    assert.equal(validate({ ...fixture, expires_at: value }), false, String(value));
  assert.equal(schema.$defs.DecimalU64.type, 'string');
  for (const { value, accepted_by_rust } of contract.decimal_fixtures)
    assert.equal(validate({ ...fixture, expires_at: value }), accepted_by_rust, JSON.stringify(value));
  // Exercise every decimal prefix at the upper boundary, not just its final digit.
  const max = 18446744073709551615n;
  for (let index = 0; index < 20; index++) {
    const digits = String(max).split('');
    for (let digit = 0; digit <= 9; digit++) {
      digits[index] = String(digit); const value = digits.join('');
      assert.equal(validate({ ...fixture, expires_at: value }), value[0] !== '0' && BigInt(value) <= max, value);
    }
  }
});
test('native requests reject extra fields, omitted budgets and invalid discriminants', () => {
  for (const [name, fixture] of Object.entries(contract.fixtures).filter(([name]) => name in validators)) {
    assert.equal(validators[name]({ ...fixture[0], unexpected_authority: true }), false, name);
  }
  const request = copy(contract.fixtures.ApprovalOptionsRequestV1[0]);
  request.intent.kind = 'raw_sign';
  assert.equal(validators.ApprovalOptionsRequestV1(request), false);
  const plan = copy(contract.fixtures.ApprovalOptionsRequestV1.find(v => v.intent.kind === 'test_session'));
  for (const budget of ['read_budget', 'cleanup_budget', 'control_budget']) {
    const invalid = copy(plan); delete invalid.intent[budget];
    assert.equal(validators.ApprovalOptionsRequestV1(invalid), false, budget);
  }
  for (const value of ['-9223372036854775809', '9223372036854775808', '-0', '01', 1]) {
    const invalid = copy(plan); invalid.intent.actions[0].position_cm[0] = value;
    assert.equal(validators.ApprovalOptionsRequestV1(invalid), false, String(value));
  }
  const verify = copy(contract.fixtures.ApprovalVerifyRequestV1[0]); verify.purpose = 'login';
  assert.equal(validators.ApprovalVerifyRequestV1(verify), false);
});
test('TypeScript converter handles closed unions and rejects unknown constructs', () => {
  assert.equal(schemaType({ anyOf: [{ type: 'string' }, { type: 'null' }] }), '(string | null)');
  assert.equal(schemaType({ type: 'array', prefixItems: [{ type: 'string' }, { type: 'integer' }], minItems: 2, maxItems: 2 }), '[string, number]');
  assert.equal(schemaType(true), 'unknown');
  for (const schema of [{ allOf: [{ type: 'string' }] }, { type: 'string', not: { const: 'x' } },
    { type: 'object', additionalProperties: true }, { $ref: 'https://evil.invalid/schema' },
    { type: 'array' }, { type: 'array', prefixItems: [], minItems: 1 },
    { type: 'integer', format: 'uint64' },
    { oneOf: [] }, { type: 'object', properties: {}, additionalProperties: false, patternProperties: {} }])
    assert.throws(() => schemaType(schema));
  assert.throws(() => declarations({ Root: { $ref: '#/$defs/Missing' } }), /Unresolved/);
});
