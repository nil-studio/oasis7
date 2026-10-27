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

test('TypeScript converter reuses exact root definitions without dropping validation', () => {
  const shape = { type: 'object', properties: { value: { type: 'string', minLength: 2, maxLength: 8 } }, required: ['value'], additionalProperties: false };
  const roots = () => ({
    Shared: { $schema: 'https://json-schema.org/draft/2020-12/schema', title: 'Public root', description: 'Root annotation', ...copy(shape),
      $defs: { OnlyInRoot: { type: 'string', minLength: 3 } } },
    Wrapper: { type: 'object', properties: { request: { $ref: '#/$defs/Shared' } }, required: ['request'], additionalProperties: false,
      $defs: { Shared: { additionalProperties: false, required: ['value'], properties: copy(shape.properties), type: 'object', title: 'Nested definition' } } },
  });
  const ts = declarations(roots());
  assert.equal((ts.match(/export type Shared =/g) ?? []).length, 1);
  assert.match(ts, /export type OnlyInRoot = string/);
  assert.match(ts, /"request": Shared/);
  // These render the same TS but have different runtime constraints/shape.
  for (const mutate of [
    value => { value.Shared.properties.value.minLength = 1; },
    value => { value.Shared.properties.value.pattern = '^safe$'; },
    value => { value.Shared.required = []; },
    value => { value.Shared.properties.extra = { type: 'string' }; },
  ]) {
    const value = roots(); mutate(value);
    assert.throws(() => declarations(value), /collides|Conflicting/);
  }
  const unsupported = roots();
  unsupported.Shared.$defs.Unused = { allOf: [{ type: 'string' }] };
  assert.throws(() => declarations(unsupported), /Unsupported/);
  const unresolved = roots();
  unresolved.Shared.$defs.Unused = { $ref: '#/$defs/Missing' };
  assert.throws(() => declarations(unresolved), /Unresolved/);
  const nestedUnsupported = roots();
  nestedUnsupported.Wrapper.$defs.Shared.$defs = { UnusedNested: { allOf: [{ type: 'string' }] } };
  assert.throws(() => declarations(nestedUnsupported), /Unsupported/);
  const nestedUnresolved = roots();
  nestedUnresolved.Wrapper.$defs.Shared.$defs = { UnusedNested: { $ref: '#/$defs/Missing' } };
  assert.throws(() => declarations(nestedUnresolved), /Unresolved/);
  const conflict = roots();
  conflict.Shared.$defs.Shared = { ...copy(shape), required: [] };
  assert.throws(() => declarations(conflict), /Conflicting/);
});

test('public pairing and TestSession Rust fixtures remain closed metadata contracts', () => {
  const names = ['PairingInviteCreateRequestV1', 'PairingInviteOriginalV1', 'PairingInviteTargetRefV1',
    'HostDeploymentDescriptorV1', 'ClientEnrollmentCreateRequestV1', 'ClientEnrollmentCreateResponseV1',
    'ClientEnrollmentProofRequestV1', 'ClientEnrollmentProofResponseV1', 'ClientCredentialClaimRequestV1',
    'ClientPairingBindingV1', 'TestSessionBasisRequestV1', 'TestSessionBasisV1', 'TestSessionPrepareRequestV1',
    'TestSessionPrepareResponseV1', 'TestSessionApproveRequestV1', 'TestSessionContinueRequestV1', 'TestSessionProgressV1'];
  for (const name of names) {
    assert.ok(validators[name], `${name} must come from the generated Rust contract`);
    assert.ok(contract.fixtures[name].length > 0, `${name} must consume actual Rust fixtures`);
    for (const original of contract.fixtures[name]) {
      const roundtrip = JSON.parse(JSON.stringify(original));
      assert.ok(validators[name](roundtrip), `${name}: ${JSON.stringify(validators[name].errors)}`);
      assert.deepEqual(roundtrip, original);
      assert.equal(validators[name]({ ...roundtrip, verified_authority: true }), false, name);
    }
  }
  const enrollment = copy(contract.fixtures.ClientEnrollmentCreateRequestV1[0]);
  delete enrollment.target;
  assert.equal(validators.ClientEnrollmentCreateRequestV1(enrollment), false, 'a directed invite target is mandatory');
  const proof = copy(contract.fixtures.ClientEnrollmentProofResponseV1[0]);
  proof.status = 'approved';
  assert.equal(validators.ClientEnrollmentProofResponseV1(proof), false, 'proof status never asserts Host approval');
  const prepare = copy(contract.fixtures.TestSessionPrepareRequestV1[0]);
  delete prepare.plan.control_budget;
  assert.equal(validators.TestSessionPrepareRequestV1(prepare), false, 'preflight control budget is explicit');
});

test('custody delegation metadata preserves all three u64 decimal fields through JSON', () => {
  // This is the metadata codec, not a cryptographic grant or semantic fence validator.
  const validate = validators.TestSessionBasisV1;
  const original = contract.fixtures.TestSessionBasisV1[0];
  const fields = ['agent_identity_generation', 'generation', 'nonce'];
  for (const field of fields) {
    assert.equal(typeof original.termination.original_delegation[field], 'string');
    for (const value of ['0', '9007199254740993', '18446744073709551614', '18446744073709551615']) {
      const fixture = copy(original);
      fixture.termination.original_delegation[field] = value;
      const roundtrip = JSON.parse(JSON.stringify(fixture));
      assert.equal(roundtrip.termination.original_delegation[field], value);
      assert.ok(validate(roundtrip), `${field}=${value}: ${JSON.stringify(validate.errors)}`);
    }
    for (const value of [1, 9007199254740992, '01', '+1', '-1', '1.0', '1e3', '18446744073709551616']) {
      const fixture = copy(original);
      fixture.termination.original_delegation[field] = value;
      assert.equal(validate(fixture), false, `${field} must reject ${String(value)}`);
    }
    for (const { value, accepted_by_rust } of contract.decimal_fixtures) {
      const fixture = copy(original);
      fixture.termination.original_delegation[field] = value;
      assert.equal(validate(fixture), accepted_by_rust, `${field}: ${JSON.stringify(value)}`);
    }
  }
  const forged = copy(original);
  forged.termination.original_delegation.verified = true;
  assert.equal(validate(forged), false, 'an extra delegation authority claim is never metadata');
});

test('safe TestSession progress rejects proofs, SID selectors, credentials and private goals', () => {
  const validate = validators.TestSessionProgressV1;
  const forbidden = ['proof', 'approval', 'source', 'private_sid', 'session_ref', 'private_sid_generation',
    'raw_token', 'credential_history', 'goal_content', 'authority', 'permit'];
  for (const fixture of contract.fixtures.TestSessionProgressV1) {
    assert.ok(validate(fixture));
    for (const name of forbidden) {
      assert.equal(Object.hasOwn(fixture, name), false);
      assert.equal(validate({ ...copy(fixture), [name]: 'not-public' }), false, name);
    }
    for (const purpose of fixture.purposes) {
      for (const name of ['raw_token', 'proof', 'goal_content']) {
        const forged = copy(fixture);
        forged.purposes[fixture.purposes.indexOf(purpose)][name] = 'not-public';
        assert.equal(validate(forged), false, `purpose.${name}`);
      }
    }
  }
  assert.ok(contract.fixtures.TestSessionProgressV1.some(value => value.stage === 'active'));
  assert.ok(contract.fixtures.TestSessionProgressV1.some(value => value.stage === 'closing'));
});
