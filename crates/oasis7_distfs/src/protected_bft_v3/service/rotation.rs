use super::*;
use crate::protected_bft_v3::rotation::{
    BFT_FINALITY_ROTATION_MAX_PROOFS_V3, BftFinalityRotationProofArchiveV3,
    BftFinalityRotationProofRecordV3, selected_key, validate_fresh, verify_proof,
};
use oasis7_agent_api::world_action::FinalityKeyRotationProofV1;

impl BftSignerServiceV3 {
    /// A completed auxiliary proof is persisted before release. It never reserves
    /// or clears a proposal/vote slot and cannot authorize a governance action.
    pub(super) fn sign_finality_rotation_proof(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let BftSignerCommandV3::SignFinalityRotationProof {
            expected_revision,
            expected_state_digest,
            body,
            role,
        } = &request.body.command
        else {
            return Err(invalid("BFT V3 expected typed finality proof command"));
        };
        body.validate().map_err(invalid)?;
        let preimage = body.signing_bytes(*role).map_err(invalid)?;
        let state = &snapshot.state;
        if let Some(original) = state.finality_rotation_proofs.as_ref().and_then(|archive| {
            archive.records.iter().find(|record| {
                record.body.rotation_id == body.rotation_id && record.proof.role == *role
            })
        }) {
            if expected_revision > &state.revision
                || original.body != **body
                || original.public_key_hex != selected_key(body, *role)
            {
                return Err(invalid(
                    "BFT V3 original rotation ID/role was reused with different body/key",
                ));
            }
            // Recovery uses the original result even after the old key exited.
            // Lease/floor/antirollback checks already ran for this fresh request.
            return self.state_reply(
                snapshot.clone(),
                request,
                request_digest,
                BftSignerReplyKindV3::FinalityRotationProofSigned {
                    proof: Box::new(original.proof.clone()),
                },
            );
        }
        if state.revision != *expected_revision || state.state_digest != *expected_state_digest {
            return Err(invalid("BFT V3 finality proof checkpoint stale"));
        }
        if state
            .finality_rotation_proofs
            .as_ref()
            .is_some_and(|archive| archive.records.len() >= BFT_FINALITY_ROTATION_MAX_PROOFS_V3)
        {
            return Err(invalid(
                "BFT V3 completed finality proof capacity exhausted",
            ));
        }
        let verified = runtime
            .verified
            .as_ref()
            .filter(|verified| verified.state == *state)
            .ok_or_else(|| {
                invalid("BFT V3 original finality proof snapshot differs from verified cache")
            })?;
        validate_fresh(&self.binding, state, &verified.schedule, body)?;
        let public = selected_key(body, *role);
        let mut candidate = state.candidate();
        let archive = candidate.finality_rotation_proofs.get_or_insert_with(|| {
            BftFinalityRotationProofArchiveV3 {
                schema_version: 1,
                records: Vec::new(),
            }
        });
        archive.records.push(BftFinalityRotationProofRecordV3 {
            body: body.as_ref().clone(),
            public_key_hex: public.into(),
            proof: FinalityKeyRotationProofV1 {
                schema_version: 1,
                role: *role,
                signature_hex: "0".repeat(128),
            },
            recorded_revision: state
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("BFT V3 finality proof revision overflow"))?,
        });
        crate::protected_bft_v3::rotation::validate_shared_archives(
            &candidate.finality_rotation_proofs,
            &candidate.bootstrap_finality_rotation_proofs,
        )?;
        let key = self
            .validator_keys
            .get(public)
            .ok_or_else(|| invalid("BFT V3 finality proof key was not provisioned"))?;
        if key.public_key_hex() != public {
            return Err(invalid("BFT V3 finality proof key handle identity changed"));
        }
        let proof = FinalityKeyRotationProofV1 {
            schema_version: 1,
            role: *role,
            signature_hex: hex::encode(key.sign_preimage(&preimage)?),
        };
        verify_proof(body, &proof, *role)?;
        candidate
            .finality_rotation_proofs
            .as_mut()
            .and_then(|archive| archive.records.last_mut())
            .ok_or_else(|| invalid("BFT V3 prepared proof record missing"))?
            .proof = proof.clone();
        self.commit_candidate(
            runtime,
            credential,
            token_digest,
            state,
            candidate,
            None,
            request,
            request_digest,
            BftSignerReplyKindV3::FinalityRotationProofSigned {
                proof: Box::new(proof),
            },
        )
    }
}
