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
export type ClientPairingBindingV1 = {
  "api_version": number;
  "client_key_id": string;
  "client_public_key": string;
  "connector_public_key"?: (string | null);
  "descriptor_digest": string;
  "enrollment_digest": string;
  "enrollment_id": string;
  "host_instance_id": string;
  "target": PairingInviteTargetRefV1;
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
  "plan_digest": string;
  "provision_id": string;
  "purpose": string;
  "register_request_id": string;
  "transport_limit": DecimalU64;
  "world_identity": ApprovedWorldV1;
  "world_route": string;
} | {
  "bootstrap_config_version": DecimalU64;
  "deadline": DecimalU64;
  "executor_key_ref": string;
  "kind": "owner_setup";
  "owner_key_ref": string;
  "participant_receipt_digest": string;
  "provision_id": string;
  "request_limit": DecimalU64;
  "setup_id": string;
  "setup_plan_digest": string;
  "transport_limit": DecimalU64;
  "world_identity": ApprovedWorldV1;
  "write_limit": DecimalU64;
} | {
  "kind": "owner_setup_reconcile";
  "plan": OwnerSetupReconcilePlanV1;
} | {
  "blocked_operation_ref": string;
  "deadline": DecimalU64;
  "kind": "managed_login";
  "login_key_ref": string;
  "login_request_id": string;
  "native_account_id": string;
  "native_identity_namespace": string;
  "transport_limit": DecimalU64;
} | {
  "actions": Array<ApprovedActionV1>;
  "allowed_resource_refs": Array<string>;
  "business_limit": DecimalU64;
  "cleanup_budget": PurposeBudgetV1;
  "cleanup_until": DecimalU64;
  "client_binding": ApprovedClientBindingV1;
  "client_pairing"?: (ClientPairingBindingV1 | null);
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
export type OwnerSetupReconcilePlanV1 = {
  "api_version": number;
  "attempt_limit": number;
  "correlation_digest": string;
  "deadline": DecimalU64;
  "executor_key_ref": string;
  "original_deadline": DecimalU64;
  "original_digest": string;
  "owner_key_ref": string;
  "provision_id": string;
  "reconcile_id": string;
  "setup_id": string;
  "setup_plan_digest": string;
  "stage": OwnerSetupStageV1;
  "transport_limit": DecimalU64;
  "world_identity": ApprovedWorldV1;
};
export type OwnerSetupStageV1 = "create_task" | "prepare_goal" | "publish_goal" | "delegation" | "bind" | "final_observation";
export type PairingInviteTargetRefV1 = {
  "custody_account_id": string;
  "invite_digest": string;
  "invite_id": string;
  "provision_id": string;
};
export type PurposeBudgetV1 = {
  "business": DecimalU64;
  "requests": DecimalU64;
  "transport": DecimalU64;
};
export type VerificationPurposeV1 = "consume_approval" | "inspect_origin";
export type TestSessionPurposeV1 = "execute" | "read" | "cleanup";
export type HostDeploymentDescriptorV1 = {
  "api_version": number;
  "build_digest": string;
  "client_public_key": string;
  "environment": string;
  "host_instance_id": string;
  "https_origin": string;
  "isolation_acceptance_refs": Array<string>;
  "isolation_mechanism": HostIsolationMechanismV1;
  "restricted_runtime_identity": string;
  "skill_manifest_digest": string;
  "tool_manifest_digest": string;
  "tool_profile": string;
  "transport_profile": HostTransportProfileV1;
};
export type HostIsolationMechanismV1 = "separate_os_principal" | "enforced_sandbox" | "remote_closed_tool_host";
export type HostTransportProfileV1 = "native-broker-custody-v1" | "dot-mcp-custody-v1";
export type ClientEnrollmentProofStatusV1 = "proved";
export type ManagedLoginStageV1 = "approved" | "native_session_ready" | "explicit_login_required";
export type ApprovalAdmissionInput = {
  "approval": SignedCustodyApprovalV1;
  "operation_id": string;
  "request": ApprovalOptionsRequestV1;
};
export type ApprovalOptionsRequestV1 = {
  "approval_request_id": string;
  "context": ApprovalContextV1;
  "intent": CustodyIntentV1;
};
export type ManagerClosureStatus = "open" | "pending" | "closed";
export type GoalContentV1 = {
  "goal_text": string;
  "soft_constraints": Array<string>;
};
export type OwnerSetupPlanV1 = {
  "api_version": number;
  "bind_operation_id": string;
  "bootstrap_config_version": DecimalU64;
  "create_operation_id": string;
  "deadline": DecimalU64;
  "delegation_generation": DecimalU64;
  "delegation_nonce": DecimalU64;
  "delegation_operation_id": string;
  "executor_id": string;
  "executor_key_ref": string;
  "executor_public_key": string;
  "goal_content": GoalContentV1;
  "owner_key_ref": string;
  "participant_receipt_digest": string;
  "prepare_nonce": string;
  "prepare_operation_id": string;
  "provision_id": string;
  "publish_operation_id": string;
  "request_limit": DecimalU64;
  "resource_refs": Array<string>;
  "setup_id": string;
  "transport_limit": DecimalU64;
  "world_identity": ApprovedWorldV1;
  "write_limit": DecimalU64;
};
export type PublicRoleKey = {
  "key_id": string;
  "key_version": DecimalU64;
  "public_key": string;
};
export type OwnerSetupBlockedCodeV1 = "rejected" | "expired_pending" | "history_unavailable_pending" | "outcome_unknown" | "awaiting_canonical_result";
export type OwnerSetupBlockedOperationV1 = {
  "code": OwnerSetupBlockedCodeV1;
  "operation_ref": string;
  "stage": OwnerSetupStageV1;
};
export type ProvisionStage = "draft" | "approved" | "role_keys_ready" | "native_registered" | "native_session_ready" | "participant_bound" | "ready_for_setup" | "prepared";
export type ProvisionPlan = {
  "environment": string;
  "expires_at": DecimalU64;
  "identity_route": string;
  "purpose": string;
  "transport_limit": DecimalU64;
  "world_identity": string;
  "world_route": string;
};
export type DirectoryEntry = {
  "bootstrap_config_version": DecimalU64;
  "client_request_id": string;
  "custody_account_id": string;
  "environment": string;
  "manager": ManagerPrincipal;
  "plan_digest": string;
  "provision_id": string;
};
export type DraftPhase = "draft_pending" | "draft_initialized" | "frozen";
export type ManagerPrincipal = {
  "identity_namespace": string;
  "native_account_id": string;
};
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
export type SourceClosureAckV1 = {
  "closed_at": DecimalU64;
  "closure_digest": string;
  "environment": string;
  "event_digest": string;
  "event_id": string;
  "identity_instance_epoch": DecimalU64;
  "identity_namespace": string;
  "inventory_digest": string;
  "key_id": string;
  "manager_account_id": string;
  "owner_audience": string;
  "status": SourceClosureStateV1;
  "version": number;
};
export type SourceClosureStateV1 = "closed";
export type AccountSecurityStateV1 = {
  "policy_version": DecimalU64;
  "security_version": DecimalU64;
  "status": string;
};
export type CredentialSecurityStateV1 = {
  "state": string;
  "version": DecimalU64;
};
export type RevokedSourceV1 = ({
  "current": AccountSecurityStateV1;
  "kind": "account";
  "previous": AccountSecurityStateV1;
} | {
  "credential_ref": string;
  "current": CredentialSecurityStateV1;
  "kind": "credential";
  "previous": CredentialSecurityStateV1;
} | {
  "current": SessionSecurityStateV1;
  "kind": "session";
  "previous": SessionSecurityStateV1;
  "session_ref": string;
});
export type SessionSecurityStateV1 = {
  "auth_method": string;
  "expires_at": DecimalU64;
  "identity_instance_epoch": DecimalU64;
  "idle_expires_at": DecimalU64;
  "issued_security_version": DecimalU64;
  "origin_credential_ref": string;
  "origin_credential_version": DecimalU64;
  "revoked_at"?: (DecimalU64 | null);
};
export type SignedSourceRevocationEventV1 = {
  "event": SourceRevocationEventV1;
  "signature": string;
};
export type SourceRevocationEventV1 = {
  "environment": string;
  "event_id": string;
  "identity_instance_epoch": DecimalU64;
  "identity_namespace": string;
  "key_id": string;
  "manager_account_id": string;
  "manager_security_version": DecimalU64;
  "observed_at": DecimalU64;
  "owner_audience": string;
  "source": RevokedSourceV1;
  "version": number;
};
export type TestSessionPlanV1 = {
  "actions": Array<ApprovedActionV1>;
  "allowed_resource_refs": Array<string>;
  "api_version": number;
  "basis_digest": string;
  "business_limit": DecimalU64;
  "cleanup_budget": PurposeBudgetV1;
  "cleanup_until": DecimalU64;
  "control_budget": ControlBudgetV1;
  "enrollment_id": string;
  "read_budget": PurposeBudgetV1;
  "read_until": DecimalU64;
  "request_id": string;
  "request_limit": DecimalU64;
  "transport_limit": DecimalU64;
  "write_deadline": DecimalU64;
};
export type DelegationMetadataV1 = {
  "agent_id": string;
  "agent_identity_generation": DecimalU64;
  "branch_id": string;
  "delegate_public_key": string;
  "generation": DecimalU64;
  "nonce": DecimalU64;
  "owner_binding": string;
  "revoked": boolean;
  "world_id": string;
};
export type TerminationNoncePolicyV1 = "account_allocated_high_water_v1";
export type TerminationPlanV1 = {
  "api_version": number;
  "cleanup_until": DecimalU64;
  "custody_account_id": string;
  "executor_key_ref": string;
  "initial_fence": GameTaskFenceV1;
  "nonce_policy": TerminationNoncePolicyV1;
  "original_delegation": DelegationMetadataV1;
  "owner_key_ref": string;
  "prepared_result_digest": string;
  "provision_id": string;
  "request_limit": DecimalU64;
  "revoke_operation_id": string;
  "session_id": string;
  "setup_id": string;
  "stop_operation_id": string;
  "transport_limit": DecimalU64;
  "trigger": TerminationTriggerV1;
  "world": ApprovedWorldV1;
};
export type TerminationTriggerV1 = "closing_only_v1";
export type SessionPurposeV1 = "execution" | "reconcile_read" | "termination_plan";
export type TestSessionClosingReasonV1 = "finished" | "aborted" | "deadline" | "lease_expired" | "budget_exhausted" | "revoked";
export type TestSessionPhaseV1 = "pending_approval" | "consumed_pending_preflight" | "active" | "closing" | "blocked";
export type TestSessionPurposeProgressV1 = {
  "business_remaining": DecimalU64;
  "deadline": DecimalU64;
  "purpose": SessionPurposeV1;
  "requests_remaining": DecimalU64;
  "revoked": boolean;
  "transport_remaining": DecimalU64;
};
export type TestSessionTerminationStatusV1 = "not_allocated" | "original_saved" | "unknown" | "committed" | "rejected";
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
export type ClientCredentialClaimRequestV1 = {
  "api_version": number;
  "enrollment_id": string;
  "expected_generation"?: (DecimalU64 | null);
  "purposes": Array<TestSessionPurposeV1>;
  "replace": boolean;
  "request_id": string;
  "test_session_id": string;
};
export type ClientEnrollmentCreateRequestV1 = {
  "api_version": number;
  "descriptor": HostDeploymentDescriptorV1;
  "request_id": string;
  "target": PairingInviteTargetRefV1;
};
export type ClientEnrollmentCreateResponseV1 = {
  "api_version": number;
  "challenge_expires_at": DecimalU64;
  "descriptor_digest": string;
  "enrollment_id": string;
  "expires_at": DecimalU64;
  "key_id": string;
  "server_nonce": string;
};
export type ClientEnrollmentProofRequestV1 = {
  "api_version": number;
  "challenge_expires_at": DecimalU64;
  "client_public_key": string;
  "descriptor_digest": string;
  "enrollment_id": string;
  "request_id": string;
  "server_nonce": string;
  "target": PairingInviteTargetRefV1;
};
export type ClientEnrollmentProofResponseV1 = {
  "api_version": number;
  "enrollment_id": string;
  "status": ClientEnrollmentProofStatusV1;
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
export type ManagedLoginApprovalViewV1 = {
  "api_version": number;
  "custody_account_id": string;
  "operation_id": string;
  "provision_id": string;
  "stage": ManagedLoginStageV1;
};
export type ManagedLoginApproveRequestV1 = {
  "api_version": number;
  "input": ApprovalAdmissionInput;
};
export type ManagerClosureViewV1 = {
  "api_version": number;
  "associated_accounts": DecimalU64;
  "pending_accounts": Array<string>;
  "pending_operations": Array<string>;
  "status": ManagerClosureStatus;
};
export type ManagerFreezeRequestV1 = {
  "api_version": number;
  "operation_id": string;
};
export type OwnerSetupApproveRequestV1 = {
  "api_version": number;
  "input": ApprovalAdmissionInput;
  "plan": OwnerSetupPlanV1;
};
export type OwnerSetupBasisRequestV1 = {
  "api_version": number;
};
export type OwnerSetupBasisV1 = {
  "api_version": number;
  "bootstrap_config_version": DecimalU64;
  "custody_account_id": string;
  "executor_key": PublicRoleKey;
  "identity_instance_epoch": DecimalU64;
  "native_account_id": string;
  "native_identity_namespace": string;
  "owner_key": PublicRoleKey;
  "participant_receipt_digest": string;
  "provision_id": string;
  "security_epoch": DecimalU64;
  "service_epoch": DecimalU64;
  "world_identity": ApprovedWorldV1;
};
export type OwnerSetupDriveRequestV1 = {
  "api_version": number;
  "custody_account_id": string;
  "provision_id": string;
  "setup_id": string;
};
export type OwnerSetupPrepareRequestV1 = {
  "api_version": number;
  "approval_request_id": string;
  "plan": OwnerSetupPlanV1;
};
export type OwnerSetupPrepareResponseV1 = {
  "approval_request_id": string;
  "context": ApprovalContextV1;
  "intent": CustodyIntentV1;
};
export type OwnerSetupProgressV1 = {
  "api_version": number;
  "blocked_operations": Array<OwnerSetupBlockedOperationV1>;
  "custody_account_id": string;
  "pending_operations": Array<string>;
  "provision_id": string;
  "setup_id": string;
  "stage": ProvisionStage;
  "transport_remaining": DecimalU64;
};
export type OwnerSetupReconcileApproveRequestV1 = {
  "api_version": number;
  "input": ApprovalAdmissionInput;
  "plan": OwnerSetupReconcilePlanV1;
};
export type OwnerSetupReconcileAttemptRequestV1 = {
  "api_version": number;
  "attempt_id": string;
  "reconcile_id": string;
};
export type OwnerSetupReconcileBasisRequestV1 = {
  "api_version": number;
  "setup_id": string;
  "stage": OwnerSetupStageV1;
};
export type OwnerSetupReconcileBasisV1 = {
  "api_version": number;
  "correlation_digest": string;
  "custody_account_id": string;
  "executor_key_ref": string;
  "original_deadline": DecimalU64;
  "original_digest": string;
  "original_transport_remaining": DecimalU64;
  "owner_key_ref": string;
  "provision_id": string;
  "security_epoch": DecimalU64;
  "service_epoch": DecimalU64;
  "setup_id": string;
  "setup_plan_digest": string;
  "stage": OwnerSetupStageV1;
  "world_identity": ApprovedWorldV1;
};
export type OwnerSetupReconcilePrepareRequestV1 = {
  "api_version": number;
  "approval_request_id": string;
  "plan": OwnerSetupReconcilePlanV1;
};
export type OwnerSetupReconcilePrepareResponseV1 = {
  "approval_request_id": string;
  "context": ApprovalContextV1;
  "intent": CustodyIntentV1;
};
export type OwnerSetupReconcileProgressV1 = {
  "api_version": number;
  "completed_attempts": Array<string>;
  "custody_account_id": string;
  "original_transport_remaining": DecimalU64;
  "pending_attempts": Array<string>;
  "provision_id": string;
  "reconcile_id": string;
  "setup_id": string;
  "stage": OwnerSetupStageV1;
  "transport_remaining": DecimalU64;
};
export type PairingInviteCreateRequestV1 = {
  "api_version": number;
  "request_id": string;
  "tool_profile": string;
  "transport_profile": HostTransportProfileV1;
};
export type PairingInviteOriginalV1 = {
  "api_version": number;
  "custody_account_id": string;
  "domain": string;
  "environment": string;
  "expires_at": DecimalU64;
  "invite_id": string;
  "issued_at": DecimalU64;
  "provision_id": string;
  "service_epoch": DecimalU64;
  "tool_profile": string;
  "transport_profile": HostTransportProfileV1;
};
export type ProvisionApprovalViewV1 = {
  "api_version": number;
  "custody_account_id": string;
  "operation_id": string;
  "provision_id": string;
  "stage": ProvisionStage;
};
export type ProvisionApproveRequestV1 = {
  "api_version": number;
  "input": ApprovalAdmissionInput;
};
export type ProvisionDraftRequestV1 = {
  "api_version": number;
  "client_request_id": string;
  "plan": ProvisionPlan;
};
export type ProvisionDraftViewV1 = {
  "api_version": number;
  "entry": DirectoryEntry;
  "phase": DraftPhase;
  "plan": ProvisionPlan;
  "security_epoch": DecimalU64;
  "world_identity": ApprovedWorldV1;
};
export type ProvisionDriveRequestV1 = {
  "api_version": number;
  "operation_id": string;
};
export type ProvisionProgressViewV1 = {
  "api_version": number;
  "custody_account_id": string;
  "operation_id": string;
  "pending_operations": Array<string>;
  "provision_id": string;
  "stage": ProvisionStage;
  "transport_remaining": DecimalU64;
};
export type SignedApprovalVerificationV1 = {
  "signature": string;
  "verification": ApprovalVerificationV1;
};
export type SignedSourceClosureAckV1 = {
  "ack": SourceClosureAckV1;
  "signature": string;
};
export type SourceRevocationAckResultV1 = {
  "api_version": number;
  "event_digest": string;
  "event_id": string;
  "status": SourceClosureStateV1;
};
export type SourceRevocationsPollRequestV1 = {
  "api_version": number;
  "limit": number;
};
export type SourceRevocationsPollResponseV1 = {
  "api_version": number;
  "events": Array<SignedSourceRevocationEventV1>;
};
export type TestSessionApproveRequestV1 = {
  "api_version": number;
  "input": ApprovalAdmissionInput;
  "plan": TestSessionPlanV1;
};
export type TestSessionBasisRequestV1 = {
  "api_version": number;
  "cleanup_until": DecimalU64;
  "enrollment_id": string;
  "request_id": string;
};
export type TestSessionBasisV1 = {
  "allowed_resource_refs": Array<string>;
  "api_version": number;
  "authority": ApprovalContextV1;
  "client_pairing": ClientPairingBindingV1;
  "custody_account_id": string;
  "executor_key": PublicRoleKey;
  "native_account_id": string;
  "owner_key": PublicRoleKey;
  "provision_id": string;
  "request_id": string;
  "runtime_ref": string;
  "session_id": string;
  "skill_ref": string;
  "source_ref": string;
  "termination": TerminationPlanV1;
};
export type TestSessionContinueRequestV1 = {
  "api_version": number;
  "session_id": string;
};
export type TestSessionPrepareRequestV1 = {
  "api_version": number;
  "approval_request_id": string;
  "plan": TestSessionPlanV1;
};
export type TestSessionPrepareResponseV1 = {
  "approval_request_id": string;
  "context": ApprovalContextV1;
  "intent": CustodyIntentV1;
};
export type TestSessionProgressV1 = {
  "api_version": number;
  "cleanup_until": DecimalU64;
  "closing_reason"?: (TestSessionClosingReasonV1 | null);
  "control_connector_http_remaining"?: (DecimalU64 | null);
  "control_host_http_remaining": DecimalU64;
  "control_requests_remaining": DecimalU64;
  "custody_account_id": string;
  "identity_control_http_remaining": DecimalU64;
  "lease_until"?: (DecimalU64 | null);
  "pending_operations": Array<string>;
  "provision_id": string;
  "purposes": Array<TestSessionPurposeProgressV1>;
  "read_until": DecimalU64;
  "revoke_status": TestSessionTerminationStatusV1;
  "session_id": string;
  "stage": TestSessionPhaseV1;
  "stop_status": TestSessionTerminationStatusV1;
  "write_closed": boolean;
  "write_deadline": DecimalU64;
};
