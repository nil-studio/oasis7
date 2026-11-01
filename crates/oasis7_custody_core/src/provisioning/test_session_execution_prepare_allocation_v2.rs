//! Account-owned initial fixed SDK stage. Never accepts caller Cognition/body.
use super::super::test_session::{SessionEventKindV2, append_session_event_v2};
use super::*;
use oasis7_client_api::world_service::{self as ws, task_context::TaskContextPrepareV1};
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
/// Rust service RNG output only; canonical values remain permanently occupied
/// once this allocation commits, including unknown downstream attempts.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareAllocationV2 {
    pub operation_id: String,
    pub client_nonce: String,
    pub observation_nonce: String,
    pub gateway_nonce: String,
    pub factory_ingress_id: String,
}
/// Actual same-transaction Current admission, not a deserializable RPC flag.
/// The shared Account issuer's fixed Execution child consumes this opaque;
/// exposing its public payload alone does not confer signing/use authority.
pub struct ExecutionPrepareSigningAuthorityV2 {
    bundle: ExecutionSigningBundleV2,
    paid_key: String,
    paid_signed_attempt_digest: String,
    admitted_at: UnixMillis,
    source_observed_at: UnixMillis,
    source_expires_at: UnixMillis,
    expires_at: UnixMillis,
    credential_generation: DecimalU64,
    executor_scope: crate::role_key_slot::RoleKeySlotScopeV1,
    executor_key: PublicRoleKey,
}
impl ExecutionPrepareSigningAuthorityV2 {
    pub fn bundle(&self) -> &ExecutionSigningBundleV2 {
        &self.bundle
    }
    pub fn paid_key(&self) -> &str {
        &self.paid_key
    }
    pub fn paid_signed_attempt_digest(&self) -> &str {
        &self.paid_signed_attempt_digest
    }
    pub fn admitted_at(&self) -> UnixMillis {
        self.admitted_at
    }
    pub fn source_observed_at(&self) -> UnixMillis {
        self.source_observed_at
    }
    pub fn source_expires_at(&self) -> UnixMillis {
        self.source_expires_at
    }
    pub fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
    pub fn credential_generation(&self) -> DecimalU64 {
        self.credential_generation
    }
    pub fn executor_scope(&self) -> &crate::role_key_slot::RoleKeySlotScopeV1 {
        &self.executor_scope
    }
    pub fn executor_key(&self) -> &PublicRoleKey {
        &self.executor_key
    }
}
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn nonce(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl SessionExecutionBookV2 {
    pub(super) fn prepare_bundle(
        &self,
        s: &TestSessionAuthorityV2,
        allocation: &MoveStageAllocationV2,
        mut observation: ws::ReadWorldViewRequest,
    ) -> Result<ExecutionSigningBundleV2> {
        let (_, scope) = s.header()?;
        observation.scope_id = format!("agent:{}", scope.task_fence.agent_id);
        observation.observation_nonce = Some(allocation.gateway_nonce.clone());
        observation.deadline_unix_ms = Some(self.execution_deadline.get());
        observation.fixed_commit = None;
        observation.validate().map_err(|_| invalid())?;
        Ok(ExecutionSigningBundleV2 {
            api_version: 2,
            session_id: self.session_id.clone(),
            action_request_id: self.action_request_id.clone(),
            stage_operation_id: allocation.operation_id.clone(),
            allocation_sequence: allocation.allocation_sequence,
            source_operation_id: allocation.source_operation_id.clone(),
            source_nonce: s
                .sources
                .get(&allocation.source_operation_id)
                .ok_or_else(invalid)?
                .source
                .verify_nonce
                .clone(),
            approved_input_digest: self.approved_input_digest.clone(),
            world: self.world.clone(),
            execution_deadline: self.execution_deadline,
            observation,
            fence: scope.task_fence.clone(),
            action: Box::new(ExecutionSigningActionV2::Prepare {
                original: Box::new(TaskContextPrepareV1 {
                    authority_mode: Some(
                        oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1,
                    ),
                    schema_revision:
                        oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(Some(
                            oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1,
                        )),
                    fence: scope.task_fence.clone(),
                    client_operation_id: allocation.operation_id.clone(),
                    client_nonce: allocation.client_nonce.clone().ok_or_else(invalid)?,
                }),
                observation_nonce: allocation.observation_nonce.clone().ok_or_else(invalid)?,
            }),
        })
    }
}
impl AccountProvisionApprovalState {
    pub fn allocate_test_session_v2_execution_prepare(
        &mut self,
        session_id: &str,
        paid_key: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionPrepareSigningAuthorityV2> {
        let s = self.test_session_v2_local(session_id, SessionPurposeV1::Execution, now)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let SessionPurposeTransportV1::ExecutionFactoryCallbackV2(paid) =
            s.purpose_transports.get(paid_key).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        paid.validate_factory_profile(s)?;
        if paid.kind == ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2 {
            let r = paid.private_current.as_ref().ok_or_else(invalid)?;
            let result = r.result.as_ref().ok_or_else(invalid)?;
            if r.completed_at.is_none_or(|at| at.get() > now.get())
                || now.get() >= r.expires_at.get()
                || current.observed_at != r.claimed_at
                || result.private_sid_generation != current.private_sid_generation
                || result.session_ref != current.lifetime.lifetime.status.session_ref
                || result.lifetime_digest
                    != current.lifetime.lifetime.digest().map_err(|_| invalid())?
                || r.source_operation_id != current.source_operation_id
            {
                return Err(invalid());
            }
        }
        let intent = &paid.intent;
        let allocation = &intent.allocation;
        if intent.action_request_id != b.action_request_id
            || intent.approved_input_digest != b.approved_input_digest
            || intent.source_operation_id != b.source_operation_id
            || intent.ledger_key(s, &paid.signed_attempt_digest)? != paid_key
            || paid.claimed_at.get() > now.get()
        {
            return Err(invalid());
        }
        if b.lifetime_admitted_at.is_none()
            || b.lifetime_admitted_at
                .is_some_and(|at| at.get() > now.get())
            || b.archived_lifetime.as_ref() != Some(current.lifetime)
            || current.source_operation_id != b.source_operation_id
            || current.source_ticket != 5
            || !b.stages.is_empty()
            || b.last_allocation_sequence != intent.allocation_sequence
            || intent.allocation_sequence.0 != 1
            || s.roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .credential
                .as_ref()
                .ok_or_else(invalid)?
                .generation
                != b.credential_generation
            || !id(&allocation.operation_id)
            || !id(&allocation.factory_ingress_id)
            || !nonce(&allocation.client_nonce)
            || !nonce(&allocation.observation_nonce)
            || !nonce(&allocation.gateway_nonce)
        {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_v2_execution_lifetime_archive_checks(
            session_id,
            current.lifetime,
            current.native_scope,
        )?)?;
        verifier.verify(&self.test_session_v2_live_scope(
            session_id,
            SessionPurposeV1::Execution,
            current,
            now,
        )?)?;
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let sequence = intent.allocation_sequence.0;
        let saved_allocation = MoveStageAllocationV2 {
            operation_id: allocation.operation_id.clone(),
            allocation_sequence: DecimalU64(sequence),
            source_operation_id: b.source_operation_id.clone(),
            source_digest: digest(source)?,
            source_observed_at: UnixMillis::new(
                source
                    .source
                    .verification
                    .as_ref()
                    .ok_or_else(invalid)?
                    .verification
                    .observed_at
                    .0,
            ),
            source_expires_at: UnixMillis::new(source.source.expires_at.0),
            allocated_at: paid.claimed_at,
            client_nonce: Some(allocation.client_nonce.clone()),
            observation_nonce: Some(allocation.observation_nonce.clone()),
            gateway_nonce: allocation.gateway_nonce.clone(),
            predecessor_digest: None,
            factory_ingress_id: paid_key.into(),
        };
        let bundle = b.prepare_bundle(
            s,
            &saved_allocation,
            self.test_session_v2_world_read(&s.input)?,
        )?;
        let candidate_digest = bundle.digest()?;
        let executor_scope = crate::role_key_slot::RoleKeySlotScopeV1::from_task(
            self.role_key_tasks.get("executor").ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?;
        let executor_key = self
            .provision
            .role_key(Role::Executor)
            .ok_or_else(invalid)?
            .clone();
        let (_, scope) = s.header()?;
        if executor_scope.context.identity.role != Role::Executor
            || executor_scope.provision_id != self.draft.entry.provision_id
            || executor_scope.context.identity.account_id != self.draft.entry.custody_account_id
            || executor_scope.context.identity.environment != self.draft.entry.environment
            || executor_key.key_id != executor_scope.context.identity.key_id
            || executor_key.key_version != executor_scope.context.identity.key_version
            || executor_key.key_id != scope.executor_key_ref
            || scope
                .task_fence
                .executor_binding
                .as_ref()
                .is_none_or(|v| v.executor_public_key != executor_key.public_key)
        {
            return Err(invalid());
        }
        // A new service permit cannot outlive *any* actual admitted component.
        // No Source or Native observation is retimestamped to this SQL time.
        let expires_at = UnixMillis::new(
            [
                // New funded private observations retain their original C3
                // intersection. Legacy factory1 has no such observation.
                if paid.kind == ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2 {
                    paid.private_current
                        .as_ref()
                        .ok_or_else(invalid)?
                        .expires_at
                        .get()
                } else {
                    u64::MAX
                },
                b.execution_deadline.get(),
                saved_allocation.source_expires_at.get(),
                current.lifetime.lifetime.status.expires_at.0,
                current.provision.approval.handoff_until.0,
                current.provision.approval.source.session_expires_at.0,
                current.provision.approval.source.session_idle_expires_at.0,
                saved_allocation
                    .source_observed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
                current
                    .lifetime
                    .lifetime
                    .status
                    .observed_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
                current
                    .observed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
                s.lease_until.ok_or_else(invalid)?.get(),
            ]
            .into_iter()
            .min()
            .ok_or_else(invalid)?,
        );
        if now.get() >= expires_at.get() {
            return Err(invalid());
        }
        let credential_generation = b.credential_generation;
        let request_id = b.action_request_id.clone();
        let paid_signed_attempt_digest = paid.signed_attempt_digest.clone();
        let source_observed_at = saved_allocation.source_observed_at;
        let source_expires_at = saved_allocation.source_expires_at;
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(session_id)?;
        // First SQL already charged this same physical Account ingress. Do not
        // charge it again or release its durable intent on a second-SQL error.
        let book = saved.execution_book.as_mut().ok_or_else(invalid)?;
        book.last_allocation_sequence = DecimalU64(sequence);
        book.stages.insert(
            MoveExecutionStageV2::Prepare,
            MoveStageOutboxV2 {
                allocation: saved_allocation,
                candidate_digest,
                original: None,
                signing_admitted_at: now,
                service_record: None,
                native_prepare: None,
                send_claim: None,
                lookup_claims: BTreeMap::new(),
                archived_fact_cbor_hex: None,
                admitted_at: None,
            },
        );
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionPrepareAllocated { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(ExecutionPrepareSigningAuthorityV2 {
            bundle,
            paid_key: paid_key.into(),
            paid_signed_attempt_digest,
            admitted_at: now,
            source_observed_at,
            source_expires_at,
            expires_at,
            credential_generation,
            executor_scope,
            executor_key,
        })
    }
}
