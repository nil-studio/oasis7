// Generated from actual Rust handler DTOs. Do not edit.
export type Role = "identity" | "owner" | "executor" | "recovery-fence";
export type ErrorCode = "not_found" | "not_ready" | "method_not_allowed";
export type ApprovalContextV1 = {
  "audience": string;
  "custody_account_id": string;
  "environment": string;
  "policy_version": DecimalU64;
  "security_version": DecimalU64;
  "service_epoch": DecimalU64;
};
export type ApprovalPhaseV1 = "pending" | "completed" | "expired" | "exhausted";
export type CustodyApprovalProofV1 = {
  "approval_id": string;
  "approval_request_id": string;
  "context": ApprovalContextV1;
  "expires_at": DecimalU64;
  "identity_instance_epoch": DecimalU64;
  "identity_namespace": string;
  "intent_digest": string;
  "issued_at": DecimalU64;
  "key_id": string;
  "manager_account_id": string;
  "manager_policy_version": DecimalU64;
  "manager_security_version": DecimalU64;
  "manager_session_ref": string;
  "nonce": string;
  "origin_credential_id": string;
  "origin_credential_version": DecimalU64;
  "passkey_credential_id": string;
  "passkey_credential_version": DecimalU64;
  "version": number;
};
export type DecimalU64 = string;
export type SignedCustodyApprovalV1 = {
  "proof": CustodyApprovalProofV1;
  "signature": string;
};
export type ApprovedActionV1 = ({
  "position_cm": Array<DecimalI64>;
  "tool": "move";
} | {
  "amount": DecimalI64;
  "distance_km": DecimalI64;
  "from": MaterialLedgerId;
  "material": string;
  "priority": MaterialTransitPriority;
  "routing": ApprovedRoutingV1;
  "to": MaterialLedgerId;
  "tool": "transfer";
} | {
  "factory": string;
  "factory_kind": string;
  "site": string;
  "tool": "build";
} | {
  "batches": number;
  "factory": string;
  "recipe": string;
  "tool": "schedule";
});
export type ApprovedClientBindingV1 = ({
  "client_key": string;
  "profile": "native-broker-custody-v1";
} | {
  "client_key": string;
  "connector_key": string;
  "profile": "dot-mcp-custody-v1";
});
export type ApprovedRoutingV1 = ({
  "kind": "direct";
} | {
  "kind": "routes";
  "route_ids": Array<string>;
});
export type ApprovedWorldV1 = {
  "branch_id": string;
  "genesis_digest": string;
  "reorg_epoch": DecimalU64;
  "service_public_key": string;
  "world_id": string;
};
export type ControlBudgetV1 = {
  "identity_transport": DecimalU64;
  "requests": DecimalU64;
  "transport_hops": number;
  "transport_per_hop": DecimalU64;
};
export type CustodyIntentV1 = ({
  "bootstrap_config_version": DecimalU64;
  "deadline": DecimalU64;
  "identity_route": string;
  "kind": "provision";
  "login_request_id": string;
  "native_identity_namespace": string;
  "provision_id": string;
  "register_request_id": string;
  "transport_limit": DecimalU64;
  "world_identity": ApprovedWorldV1;
  "world_route": string;
} | {
  "blocked_operation_ref": string;
  "deadline": DecimalU64;
  "kind": "managed_login";
  "login_key_ref": string;
  "native_account_id": string;
  "native_identity_namespace": string;
} | {
  "actions": Array<ApprovedActionV1>;
  "allowed_resource_refs": Array<string>;
  "business_limit": DecimalU64;
  "cleanup_budget": PurposeBudgetV1;
  "cleanup_until": DecimalU64;
  "client_binding": ApprovedClientBindingV1;
  "control_budget": ControlBudgetV1;
  "executor_key_ref": string;
  "kind": "test_session";
  "native_account_id": string;
  "read_budget": PurposeBudgetV1;
  "read_until": DecimalU64;
  "request_limit": DecimalU64;
  "runtime_ref": string;
  "session_id": string;
  "skill_ref": string;
  "source_ref": string;
  "task_fence": GameTaskFenceV1;
  "termination_plan_digest": string;
  "transport_limit": DecimalU64;
  "world_identity": ApprovedWorldV1;
  "write_deadline": DecimalU64;
});
export type DecimalI64 = string;
export type GameExecutorBindingV1 = {
  "delegation_generation": DecimalU64;
  "executor_epoch": DecimalU64;
  "executor_id": string;
  "executor_public_key": string;
};
export type GameGoalReferenceV1 = {
  "activation_committed_height": DecimalU64;
  "goal_ref": string;
  "goal_revision": DecimalU64;
  "prepare_operation_id": string;
  "publish_operation_id": string;
};
export type GameTaskFenceV1 = {
  "active_goal_reference"?: (GameGoalReferenceV1 | null);
  "active_goal_revision": DecimalU64;
  "agent_id": string;
  "branch_id": string;
  "executor_binding"?: (GameExecutorBindingV1 | null);
  "executor_epoch": DecimalU64;
  "genesis_digest": string;
  "latest_goal_revision": DecimalU64;
  "owner_binding": string;
  "owner_generation": DecimalU64;
  "owner_public_key": string;
  "reorg_epoch": DecimalU64;
  "task_id": string;
  "task_revision": DecimalU64;
  "task_status": GameTaskStatusV1;
  "world_id": string;
};
export type GameTaskStatusV1 = "suspended" | "active";
export type MaterialLedgerId = string;
export type MaterialTransitPriority = "urgent" | "standard";
export type PurposeBudgetV1 = {
  "business": DecimalU64;
  "requests": DecimalU64;
  "transport": DecimalU64;
};
export type VerificationPurposeV1 = "consume_approval" | "inspect_origin";
export type ApprovalVerificationV1 = {
  "approval_request_id": string;
  "audience": string;
  "credential_version": DecimalU64;
  "identity_instance_epoch": DecimalU64;
  "identity_namespace": string;
  "intent_digest": string;
  "key_id": string;
  "manager_policy_version": DecimalU64;
  "manager_security_version": DecimalU64;
  "observed_at": DecimalU64;
  "origin_credential_version": DecimalU64;
  "origin_state": OriginStateV1;
  "proof_digest": string;
  "proof_expires_at": DecimalU64;
  "proof_state": ProofStateV1;
  "purpose": VerificationPurposeV1;
  "source_session_expires_at": DecimalU64;
  "source_session_idle_expires_at": DecimalU64;
  "verify_nonce": string;
  "version": number;
};
export type OriginStateV1 = "active" | "revoked" | "unavailable";
export type ProofStateV1 = "active" | "expired";
export type ServiceMetadata = {
  "api_profile": string;
  "execution_enabled": boolean;
  "role": Role;
  "schema_revision": string;
};
export type ErrorResponse = {
  "code": ErrorCode;
  "schema_revision": string;
};
export type ApprovalCompleteRequestV1 = {
  "approval_id": string;
  "ceremony_id": string;
  "intent_digest": string;
  "operation_id": string;
  "response": unknown;
};
export type ApprovalCompleteResponseV1 = {
  "api_version": number;
  "approval": SignedCustodyApprovalV1;
  "phase": ApprovalPhaseV1;
  "proof_bytes": string;
};
export type ApprovalOptionsRequestV1 = {
  "approval_request_id": string;
  "context": ApprovalContextV1;
  "intent": CustodyIntentV1;
};
export type ApprovalOptionsResponseV1 = {
  "api_version": number;
  "approval_id": string;
  "audience": string;
  "ceremony_expires_at": DecimalU64;
  "ceremony_id": string;
  "intent_digest": string;
  "operation_id": string;
  "options"?: unknown;
  "phase": ApprovalPhaseV1;
};
export type ApprovalVerifyRequestV1 = {
  "approval": SignedCustodyApprovalV1;
  "expected_audience": string;
  "expected_intent_digest": string;
  "proof_digest": string;
  "purpose": VerificationPurposeV1;
  "verify_nonce": string;
};
export type CustodySessionStatusV1 = {
  "account_id": string;
  "api_version": number;
  "credential_ref": string;
  "credential_version": DecimalU64;
  "expires_at": DecimalU64;
  "identity_instance_epoch": DecimalU64;
  "identity_namespace": string;
  "idle_expires_at": DecimalU64;
  "policy_version": DecimalU64;
  "security_version": DecimalU64;
  "server_time": DecimalU64;
  "session_ref": string;
};
export type SignedApprovalVerificationV1 = {
  "signature": string;
  "verification": ApprovalVerificationV1;
};
