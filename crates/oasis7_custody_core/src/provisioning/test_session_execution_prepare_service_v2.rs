//! Exact typed service proofs persisted with the initial stage, never a grant.
use super::super::test_session::{SessionEventKindV2, append_session_event_v2};
use super::*;
use crate::native_cleanup_v2::{AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2};
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
pub(super) fn service_shape(
    record: &ExecutionPrepareServiceRecordV2,
    bundle: &ExecutionSigningBundleV2,
    scope: &crate::role_key_slot::RoleKeySlotScopeV1,
    public: &PublicRoleKey,
    admitted: UnixMillis,
) -> Result<()> {
    if record.recorded_at != admitted {
        return Err(invalid());
    }
    let init = &record.permits.initialize;
    let sign = &record.permits.sign;
    for (proof, operation) in [
        (init, ExecutionPrepareSignerOperationV2::InitializeSigner),
        (sign, ExecutionPrepareSignerOperationV2::SignPrepare),
    ] {
        if proof.permit.issued_at != admitted
            || proof.signature_hex.len() != 128
            || !proof
                .signature_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        proof.permit.validate_for(
            bundle,
            ExecutionPrepareRecipientV2 {
                scope,
                key: public,
                operation,
            },
            admitted,
        )?;
    }
    let mut expected = sign.permit.clone();
    expected.operation = ExecutionPrepareSignerOperationV2::InitializeSigner;
    expected.recipient_route =
        super::signer_permit_v2::EXECUTION_PREPARE_SIGNER_INITIALIZE_ROUTE_V2.into();
    if init.permit != expected {
        return Err(invalid());
    }
    Ok(())
}
impl AccountProvisionApprovalState {
    /// Runtime configuration is supplied only by the fixed private issuer child.
    /// Historical wire data cannot recreate a signing/dispatch opaque token.
    pub fn record_test_session_v2_prepare_service_permits(
        &mut self,
        id: &str,
        permits: SignedExecutionPreparePermitsV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<Option<ExecutionPrepareDispatchV2>> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let stage = b
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?;
        if !matches!(s.purpose_transports.get(&stage.allocation.factory_ingress_id), Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(p)) if p.current_read.is_some())
        {
            return Err(invalid());
        }
        let scope = crate::role_key_slot::RoleKeySlotScopeV1::from_task(
            self.role_key_tasks.get("executor").ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?;
        let public = self
            .provision
            .role_key(Role::Executor)
            .ok_or_else(invalid)?;
        let bundle = b.prepare_bundle(
            s,
            &stage.allocation,
            self.test_session_v2_world_read(&s.input)?,
        )?;
        let port_ids = [
            super::rkv_ports_v2::port_id(
                s,
                &permits.initialize.permit.physical_ingress_key,
                &permits.initialize,
                ExecutionPrepareRkvHopV2::AccountToExecutorService,
            )?,
            super::rkv_ports_v2::port_id(
                s,
                &permits.initialize.permit.physical_ingress_key,
                &permits.initialize,
                ExecutionPrepareRkvHopV2::ExecutorServiceToActor,
            )?,
            super::rkv_ports_v2::port_id(
                s,
                &permits.sign.permit.physical_ingress_key,
                &permits.sign,
                ExecutionPrepareRkvHopV2::AccountToExecutorService,
            )?,
            super::rkv_ports_v2::port_id(
                s,
                &permits.sign.permit.physical_ingress_key,
                &permits.sign,
                ExecutionPrepareRkvHopV2::ExecutorServiceToActor,
            )?,
        ];
        let record = ExecutionPrepareServiceRecordV2 {
            port_ids,
            permits,
            recorded_at: now,
        };
        service_shape(&record, &bundle, &scope, public, stage.signing_admitted_at)?;
        for (proof, operation) in [
            (
                &record.permits.initialize,
                ExecutionPrepareSignerOperationV2::InitializeSigner,
            ),
            (
                &record.permits.sign,
                ExecutionPrepareSignerOperationV2::SignPrepare,
            ),
        ] {
            proof.verify(
                pin,
                floor,
                &bundle,
                ExecutionPrepareRecipientV2 {
                    scope: &scope,
                    key: public,
                    operation,
                },
                now,
            )?;
        }
        if let Some(old) = &stage.service_record {
            return if old == &record {
                Ok(None)
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        for key in &record.port_ids {
            if s.purpose_transports.contains_key(key)
                || s.purpose_http_admissions.contains_key(key)
                || s.sources.contains_key(key)
                || s.source_funding.contains_key(key)
                || s.control_transports.contains_key(key)
                || s.client_http_admissions.contains_key(key)
                || s.termination_state.transport_id_in_use(key)
                || b.transport_id_in_use(key)
            {
                return Err(ProvisionApprovalError::Conflict);
            }
        }
        self.test_session_v2_local(id, SessionPurposeV1::Execution, now)?;
        let dispatch = ExecutionPrepareDispatchV2::new(&record, bundle.clone());
        let request = b.action_request_id.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(4)
            .map_err(|_| invalid())?;
        for (operation_index, proof) in [&record.permits.initialize, &record.permits.sign]
            .into_iter()
            .enumerate()
        {
            for (hop_index, hop) in [
                ExecutionPrepareRkvHopV2::AccountToExecutorService,
                ExecutionPrepareRkvHopV2::ExecutorServiceToActor,
            ]
            .into_iter()
            .enumerate()
            {
                let key = &record.port_ids[operation_index * 2 + hop_index];
                s.purpose_transports.insert(
                    key.clone(),
                    SessionPurposeTransportV1::ExecutionPrepareRkvV2(Box::new(
                        ExecutionPrepareRkvTransportV2 {
                            hop,
                            operation: proof.permit.operation,
                            factory_key: proof.permit.physical_ingress_key.clone(),
                            proof_digest: digest(&(
                                "oasis7.custody.execution-signer.permit.v2",
                                proof,
                            ))?,
                            claimed_at: now,
                        },
                    )),
                );
            }
        }
        s.execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .stages
            .get_mut(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?
            .service_record = Some(record);
        append_session_event_v2(
            s,
            SessionEventKindV2::ExecutionPreparePermitsRecorded {
                request_id: request,
            },
            now,
        )?;
        if s.roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && s.closing.is_none()
        {
            s.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(Some(dispatch))
    }
    /// Public proof data only; the receiving RKV must independently verify its
    /// configured issuer pin/floor, exact slot, fresh Current and physical once.
    pub fn test_session_v2_prepare_service_permits(
        &self,
        id: &str,
    ) -> Option<&SignedExecutionPreparePermitsV2> {
        self.test_session_v2_record(id)
            .ok()?
            .execution_book
            .as_ref()?
            .prepare_service_record()
            .map(|r| &r.permits)
    }
}
