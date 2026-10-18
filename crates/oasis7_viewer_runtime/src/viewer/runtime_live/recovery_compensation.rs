use super::recovery_receipt::{recovery_error, rollback_runtime_error};
use super::*;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

impl ViewerRuntimeLiveServer {
    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn verify_compensation_operator(
        &self,
        authority: &crate::viewer::protocol::RollbackOperatorAuthorization,
        payload: &[u8],
    ) -> Result<(), AuthoritativeRecoveryError> {
        let now_ms = super::recovery_receipt::current_unix_time_ms();
        const FUTURE_SKEW_MS: u64 = 30_000;
        if authority.nonce.trim().is_empty()
            || authority.expires_at_ms < authority.issued_at_ms
            || authority.expires_at_ms < now_ms
            || authority.issued_at_ms > now_ms.saturating_add(FUTURE_SKEW_MS)
        {
            return Err(recovery_error(
                "rollback_operator_authorization_expired",
                "operator authorization is expired, invalid, or issued too far in the future",
                None,
                None,
                None,
            ));
        }
        if self
            .consumed_rollback_operator_nonces
            .contains(authority.nonce.as_str())
        {
            return Err(recovery_error(
                "rollback_operator_authorization_replayed",
                "operator authorization nonce was already consumed",
                None,
                None,
                None,
            ));
        }
        let snapshot = self.world.snapshot();
        let record = snapshot
            .rollback_authority_registry
            .get(authority.authority_id.as_str())
            .filter(|record| {
                record.active && record.role == crate::runtime::RollbackAuthorityRole::Governance
            })
            .ok_or_else(|| {
                recovery_error(
                    "rollback_compensation_authorization_invalid",
                    "active governance rollback authority required",
                    None,
                    None,
                    None,
                )
            })?;
        if authority.signature_scheme != "ed25519" {
            return Err(recovery_error(
                "rollback_compensation_authorization_invalid",
                "ed25519 signature required",
                None,
                None,
                None,
            ));
        }
        let key_bytes: [u8; 32] = hex::decode(record.public_key_hex.as_str())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                recovery_error(
                    "rollback_compensation_authorization_invalid",
                    "invalid authority public key",
                    None,
                    None,
                    None,
                )
            })?;
        let signature_bytes: [u8; 64] = hex::decode(authority.signature_hex.as_str())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                recovery_error(
                    "rollback_compensation_authorization_invalid",
                    "invalid operator signature",
                    None,
                    None,
                    None,
                )
            })?;
        VerifyingKey::from_bytes(&key_bytes)
            .and_then(|key| key.verify(payload, &Signature::from_bytes(&signature_bytes)))
            .map_err(|_| {
                recovery_error(
                    "rollback_compensation_authorization_invalid",
                    "operator signature verification failed",
                    None,
                    None,
                    None,
                )
            })
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn transition_rollback_compensation(
        &mut self,
        request: crate::viewer::protocol::RollbackCompensationTransitionRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let payload = request.canonical_signing_payload().map_err(|err| {
            recovery_error(
                "rollback_compensation_authorization_invalid",
                err.to_string(),
                None,
                None,
                None,
            )
        })?;
        self.verify_compensation_operator(&request.authorization, payload.as_slice())?;
        let source = crate::runtime::RollbackSourceEventIdentity {
            source_batch_id: request.source_batch_id,
            source_event_id: request.source_event_id,
        };
        let next = match request.next_state {
            crate::viewer::protocol::PlayerCompensationState::PendingAuthorization => {
                crate::runtime::RollbackCompensationState::PendingAuthorization
            }
            crate::viewer::protocol::PlayerCompensationState::Authorized => {
                crate::runtime::RollbackCompensationState::Authorized
            }
            crate::viewer::protocol::PlayerCompensationState::InProgress => {
                crate::runtime::RollbackCompensationState::InProgress
            }
            crate::viewer::protocol::PlayerCompensationState::Completed => {
                crate::runtime::RollbackCompensationState::Completed
            }
            crate::viewer::protocol::PlayerCompensationState::Rejected => {
                crate::runtime::RollbackCompensationState::Rejected
            }
        };
        let previous = self.world.clone();
        let previous_nonces = self.consumed_rollback_operator_nonces.clone();
        self.consumed_rollback_operator_nonces
            .insert(request.authorization.nonce.clone());
        if let Err(err) = self.world.transition_rollback_compensation_case(
            request.authorization_nonce.as_str(),
            &source,
            next,
        ) {
            self.world = previous;
            self.consumed_rollback_operator_nonces = previous_nonces;
            return Err(rollback_runtime_error(err, source.source_batch_id.clone()));
        }
        let ack = match self.get_rollback_receipt(request.authorization_nonce) {
            Ok(ack) => ack,
            Err(error) => {
                self.world = previous;
                self.consumed_rollback_operator_nonces = previous_nonces;
                return Err(error);
            }
        };
        if let Err(error) = self.persist_current_recovery_generation(&ack) {
            self.world = previous;
            self.consumed_rollback_operator_nonces = previous_nonces;
            return Err(error);
        }
        Ok(ack)
    }

    #[expect(
        clippy::result_large_err,
        reason = "Recovery protocol errors preserve the stable typed error envelope"
    )]
    pub(super) fn resolve_rollback_attribution(
        &mut self,
        request: crate::viewer::protocol::RollbackAttributionResolutionRequest,
    ) -> Result<AuthoritativeRecoveryAck<u64>, AuthoritativeRecoveryError> {
        let payload = request.canonical_signing_payload().map_err(|err| {
            recovery_error(
                "rollback_attribution_authorization_invalid",
                err.to_string(),
                None,
                None,
                None,
            )
        })?;
        self.verify_compensation_operator(&request.authorization, payload.as_slice())?;
        let source = crate::runtime::RollbackSourceEventIdentity {
            source_batch_id: request.source_batch_id,
            source_event_id: request.source_event_id,
        };
        let case_digest = hex::encode(Sha256::digest(format!(
            "oasis7:rollback-compensation-public:v1:{}:{}",
            source.source_batch_id, source.source_event_id
        )));
        let previous = self.world.clone();
        let previous_nonces = self.consumed_rollback_operator_nonces.clone();
        let previous_readiness = self.rollback_readiness.clone();
        self.consumed_rollback_operator_nonces
            .insert(request.authorization.nonce.clone());
        let mutation = match request.resolution {
            crate::viewer::protocol::RollbackAttributionResolution::Player { player_id } => {
                self.world.resolve_rollback_action_attribution(
                    request.authorization_nonce.as_str(),
                    &source,
                    player_id,
                    crate::runtime::RollbackCompensationCaseRef {
                        owner_id: "player_support".to_string(),
                        ticket_id: format!("rollback-case-{}", &case_digest[..16]),
                        state: crate::runtime::RollbackCompensationState::PendingAuthorization,
                    },
                )
            }
            crate::viewer::protocol::RollbackAttributionResolution::SystemAuthored => {
                self.world.resolve_rollback_action_as_system_authored(
                    request.authorization_nonce.as_str(),
                    &source,
                )
            }
        };
        let changed = match mutation {
            Ok(changed) => changed,
            Err(err) => {
                self.world = previous;
                self.consumed_rollback_operator_nonces = previous_nonces;
                return Err(rollback_runtime_error(err, source.source_batch_id.clone()));
            }
        };
        if !changed {
            self.consumed_rollback_operator_nonces = previous_nonces;
            return self.get_rollback_receipt(request.authorization_nonce);
        }
        let ack = match self.reevaluate_rollback_readiness(request.authorization_nonce, None) {
            Ok(ack) => ack,
            Err(error) => {
                self.world = previous;
                self.consumed_rollback_operator_nonces = previous_nonces;
                self.rollback_readiness = previous_readiness;
                return Err(error);
            }
        };
        Ok(ack)
    }
}
