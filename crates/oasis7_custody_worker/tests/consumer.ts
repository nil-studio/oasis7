// Typecheck the actual HTTP consumer against the Rust-generated declarations.
import type { ServiceMetadata, ErrorResponse, ApprovalOptionsRequestV1,
  ApprovalCompleteRequestV1, ApprovalCompleteResponseV1, ApprovalVerifyRequestV1,
  SignedApprovalVerificationV1, CustodySessionStatusV1, ApprovedActionV1 } from '../contract/http';
export function stateText(value: ServiceMetadata): string {
  return `${value.role}: ${value.execution_enabled ? 'enabled' : 'not ready'}`;
}
export function errorText(value: ErrorResponse): string { return value.code; }
const example: ServiceMetadata = { role: 'owner', execution_enabled: false,
  schema_revision: 'custody-http-v1', api_profile: 'custody-foundation-v1' };
stateText(example);
// @ts-expect-error protocol keys are not public metadata
stateText({ ...example, private_key: 'never' });
// @ts-expect-error unknown roles are rejected
stateText({ ...example, role: 'admin' });

// Native management consumers use the same Rust declarations as the handlers.
export function loginApproval(deadline: string): ApprovalOptionsRequestV1 {
  return { approval_request_id: 'persisted-request-one', context: {
    audience: 'account-authority:test', environment: 'test', custody_account_id: 'custody-one',
    policy_version: '1', security_version: '2', service_epoch: '3',
  }, intent: { kind: 'managed_login', native_account_id: 'native-one', login_key_ref: 'login-one',
    native_identity_namespace: 'identity:test', blocked_operation_ref: 'blocked-one', deadline } };
}
export function completion(id: string, intent_digest: string, assertion: unknown): ApprovalCompleteRequestV1 {
  return { approval_id: id, ceremony_id: id, operation_id: id, intent_digest, response: assertion };
}
export function inspectOrigin(complete: ApprovalCompleteResponseV1, digestFromRust: string,
  verify_nonce: string): ApprovalVerifyRequestV1 {
  return { approval: complete.approval, proof_digest: digestFromRust,
    expected_audience: complete.approval.proof.context.audience,
    expected_intent_digest: complete.approval.proof.intent_digest,
    purpose: 'inspect_origin', verify_nonce };
}
export function observedSource(value: SignedApprovalVerificationV1): string {
  return `${value.verification.origin_state}: ${value.verification.source_session_expires_at}`;
}
export function sessionExpiry(value: CustodySessionStatusV1): string { return value.expires_at; }
const plan = loginApproval('9007199254740993');
// @ts-expect-error lossless u64 fields never accept JavaScript Number
const numericDeadline: ApprovalOptionsRequestV1 = { ...plan, intent: { ...plan.intent, deadline: 9007199254740993 } };
// @ts-expect-error no raw signing intent exists
const rawIntent: ApprovalOptionsRequestV1 = { ...plan, intent: { kind: 'raw_sign', bytes: 'never' } };
// @ts-expect-error coordinates use lossless signed decimal strings
const numericCoordinate: ApprovedActionV1 = { tool: 'move', position_cm: ['1', '2', 3] };
// @ts-expect-error privileged account routes cannot be injected into an assertion envelope
const privilegedComplete: ApprovalCompleteRequestV1 = { approval_id: 'a', ceremony_id: 'a', operation_id: 'a', intent_digest: 'd', response: {}, owner_url: 'https://evil.invalid' };
