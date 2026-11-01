//! Strong registration receipt consumption, using the existing activation checks.
//! The resulting three Roots remain externally inert until Native Active is admitted.
use super::*;
use crate::native_cleanup_v2::{
    NativeFloorEvidenceV2, NativeReadAndRegisterReplyV2, NativeReadAndRegisterRequestV2,
    native_registration_nonce_v2,
};
impl AccountProvisionApprovalState {
    /// Historical exact original, not permission to dispatch/retry it. The actual
    /// Identity driver obtains it only after the once signature completion.
    pub fn test_session_v2_machine_read_original(
        &self,
        id: &str,
    ) -> Result<NativeReadAndRegisterRequestV2> {
        let s = self.test_session_v2_record(id)?;
        let machine = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        let pending = machine.signed.as_ref().ok_or_else(invalid)?;
        let request = NativeReadAndRegisterRequestV2 {
            api_version: 2,
            pending: pending.compact_reference().map_err(|_| invalid())?,
            owner_read: s.preflight_read.as_deref().ok_or_else(invalid)?.clone(),
            registration_nonce: native_registration_nonce_v2(pending).map_err(|_| invalid())?,
        };
        request.validate().map_err(|_| invalid())?;
        Ok(request)
    }
    pub fn activate_test_session_v2_registered_machine(
        &mut self,
        id: &str,
        reply: NativeReadAndRegisterReplyV2,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionAuthorityV2> {
        let s = self.test_session_v2_record(id)?;
        let machine = s.native_machine_registration.as_ref().ok_or_else(invalid)?;
        if machine.registered.is_some() || s.approved_at.is_some() || !s.roots.is_empty() {
            return Err(invalid());
        }
        let pending = machine.signed.as_ref().ok_or_else(invalid)?;
        let original = self.test_session_v2_machine_read_original(id)?;
        reply
            .verify_world(&original, pending)
            .map_err(|_| invalid())?;
        let native = current.approval.native_scope;
        reply
            .registration_ack
            .verify_native(
                pending,
                native.native_root_key_id,
                native.native_root_public_key,
                &s.input.approval.proof.identity_namespace,
                s.input.approval.proof.identity_instance_epoch.0,
                now.get(),
            )
            .map_err(|_| invalid())?;
        if current.world.request != original.owner_read
            || oasis7_wasm_abi::encode_canonical_cbor(&current.world.response)
                .map_err(|_| invalid())?
                != oasis7_wasm_abi::encode_canonical_cbor(&reply.world_response)
                    .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        for check in self.activate_test_session_v2_checks(id, current, now)? {
            verifier.verify(&check)?;
        }
        let world_response_cbor =
            oasis7_wasm_abi::encode_canonical_cbor(&reply.world_response).map_err(|_| invalid())?;
        if serde_json::to_vec(&reply).map_err(|_| invalid())?.len() > 65_536 {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.test_sessions_v2
            .get_mut(id)
            .ok_or_else(invalid)?
            .native_machine_registration
            .as_mut()
            .ok_or_else(invalid)?
            .registered = Some(
            super::machine_registration_v2::NativeMachineRegisteredFactV2 {
                ack: reply.registration_ack.clone(),
                world_response_cbor,
            },
        );
        // The same reducer, not a parallel set of copied V1 Roots. Its public
        // wrapper's checks were performed above before mutating this candidate.
        next.activate_test_session_v2_reducer(id, current, now)?;
        next.record_test_session_v2_native_floor(
            id,
            NativeFloorEvidenceV2::Registration(Box::new(reply.registration_ack)),
            native,
        )?;
        next.validate_persistence()?;
        let result = next.test_sessions_v2.get(id).ok_or_else(invalid)?.clone();
        *self = next;
        Ok(result)
    }
}
