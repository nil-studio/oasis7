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
    native_identity_namespace: 'identity:test', blocked_operation_ref: 'blocked-one',
    login_request_id:'managed-login-one', transport_limit:'64', deadline } };
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

// New public Host pairing and TestSession management use only generated DTOs.
import type { PairingInviteCreateRequestV1, PairingInviteOriginalV1, PairingInviteTargetRefV1,
  HostDeploymentDescriptorV1, ClientEnrollmentCreateRequestV1, ClientEnrollmentCreateResponseV1,
  ClientEnrollmentProofRequestV1, ClientEnrollmentProofResponseV1, TestSessionBasisRequestV1,
  TestSessionBasisV1, TestSessionPrepareRequestV1, TestSessionPrepareResponseV1,
  TestSessionApproveRequestV1, TestSessionContinueRequestV1, TestSessionProgressV1 } from '../contract/http';
export function inviteRequest(request_id: string): PairingInviteCreateRequestV1 {
  return { api_version: 1, request_id, transport_profile: 'native-broker-custody-v1', tool_profile: 'custody-native-v1' };
}
export function directedInvite(invite: PairingInviteOriginalV1, digestFromRust: string): PairingInviteTargetRefV1 {
  return { custody_account_id: invite.custody_account_id, provision_id: invite.provision_id,
    invite_id: invite.invite_id, invite_digest: digestFromRust };
}
export function enrollmentRequest(target: PairingInviteTargetRefV1, descriptor: HostDeploymentDescriptorV1): ClientEnrollmentCreateRequestV1 {
  return { api_version: 1, request_id: 'enrollment-create', target, descriptor };
}
export function enrollmentProof(created: ClientEnrollmentCreateResponseV1,
  original: ClientEnrollmentCreateRequestV1): ClientEnrollmentProofRequestV1 {
  return { api_version: 1, enrollment_id: created.enrollment_id, request_id: 'enrollment-proof',
    target: original.target, client_public_key: original.descriptor.client_public_key,
    server_nonce: created.server_nonce, challenge_expires_at: created.challenge_expires_at,
    descriptor_digest: created.descriptor_digest };
}
export function provedStatus(value: ClientEnrollmentProofResponseV1): 'proved' { return value.status; }
export function sessionBasis(created: ClientEnrollmentCreateResponseV1, cleanup_until: string): TestSessionBasisRequestV1 {
  return { api_version: 1, request_id: 'session-plan', enrollment_id: created.enrollment_id, cleanup_until };
}
export function prepareSession(plan: TestSessionPrepareRequestV1['plan']): TestSessionPrepareRequestV1 {
  return { api_version: 1, approval_request_id: 'fresh-session-approval', plan };
}
export function approveSession(request: TestSessionPrepareResponseV1, complete: ApprovalCompleteResponseV1,
  plan: TestSessionPrepareRequestV1['plan']): TestSessionApproveRequestV1 {
  if (request.intent.kind !== 'test_session') throw new Error('wrong public approval intent');
  return { api_version: 1, input: { operation_id: request.intent.session_id, request, approval: complete.approval }, plan };
}
export function continueSession(progress: TestSessionProgressV1): TestSessionContinueRequestV1 {
  return { api_version: 1, session_id: progress.session_id };
}
export function safeSessionText(progress: TestSessionProgressV1): string {
  return `${progress.stage}: ${progress.control_host_http_remaining}; Stop ${progress.stop_status}, Revoke ${progress.revoke_status}`;
}
export function delegationHighWater(value: TestSessionBasisV1['termination']['original_delegation']): [string, string, string] {
  const wide: typeof value = { ...value, agent_identity_generation: '9007199254740993',
    generation: '18446744073709551615', nonce: '18446744073709551615' };
  // @ts-expect-error custody metadata identity generation cannot round through Number
  const numericIdentity: typeof value = { ...value, agent_identity_generation: 9007199254740993 };
  // @ts-expect-error custody metadata delegation generation is also a decimal string
  const numericGeneration: typeof value = { ...value, generation: 1 };
  // @ts-expect-error custody metadata nonce remains lossless independently
  const numericNonce: typeof value = { ...value, nonce: 1 };
  return [wide.agent_identity_generation, wide.generation, wide.nonce];
}
export function rejectProgressSecrets(progress: TestSessionProgressV1): void {
  // @ts-expect-error a safe progress response has no signed approval proof
  progress.proof;
  // @ts-expect-error a private native SID selector is never a progress field
  progress.private_sid;
  // @ts-expect-error public progress never carries a bearer credential
  progress.raw_token;
  // @ts-expect-error private goal content does not cross this response boundary
  progress.goal_content;
}
// @ts-expect-error proving possession does not assert deployment approval
const approvedEnrollment: ClientEnrollmentProofResponseV1 = { api_version: 1, enrollment_id: 'one', status: 'approved' };
// @ts-expect-error metadata-only basis requests cannot inject verified caller authority
const forgedSessionBasis: TestSessionBasisRequestV1 = { api_version: 1, request_id: 'r', enrollment_id: 'e', cleanup_until: '9007199254740993', verified: true };
