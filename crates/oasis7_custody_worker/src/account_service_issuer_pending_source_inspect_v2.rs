//! Two fixed Source purposes use the original sole Account signer. The
//! separate bounded public pin pair does not widen the ordinary16 inventory.
use super::*;
use oasis7_custody_core::approval_admission::ApprovalSignatureVerifier;
use oasis7_custody_core::{
    execution_pending_source_inspect_v2::*,
    provisioning::{
        AccountProvisionApprovalState, PendingSourceDispatchTargetV2, PendingSourceDispatchV2,
    },
    time_units::UnixMillis,
};
impl AccountServiceIssuer {
    fn source_inspect_pair(&self, env: &Env) -> Result<[AccountMachineIssuerPinV2; 2]> {
        let pair = crate::execution_source_inspect_pins_v2::load(env)?;
        let public = &self.pins.first().ok_or_else(denied)?.public_key;
        let purposes = [
            AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
            AccountMachinePurposeV2::ExecutionPendingSourceNativeLeafV2,
        ];
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        for (pin, purpose) in pair.iter().zip(purposes) {
            if pin.public_key != *public {
                return Err(denied());
            }
            self.issuer
                .verify_pin(
                    pin,
                    &floor,
                    purpose,
                    pin.not_before.0,
                    pin.not_after.0,
                    pin.not_before.0,
                )
                .map_err(|_| denied())?;
        }
        Ok(pair)
    }
    pub(crate) fn pending_source_receiver_name_v2(&self) -> Result<String> {
        source_inspect_receiver_name_v2(&self.issuer).map_err(|_| denied())
    }
    pub(crate) fn pending_source_dispatch_v2(
        &self,
        env: &Env,
        state: &mut AccountProvisionApprovalState,
        current: &mut crate::owner_provision::native::PendingDirectoryFenceObservationV2,
        object_id: String,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PendingSourceDispatchV2> {
        current.check_source_current(env, state, now.get())?;
        let target = PendingSourceDispatchTargetV2 {
            object_id,
            handoff_until: current.source_until(),
        };
        let first = current.take_source_first()?;
        let [pin, _] = self.source_inspect_pair(env)?;
        let authority = state
            .pending_source_dispatch_authority_v2(
                first,
                self.issuer.clone(),
                target,
                &self.native_scope(),
                now,
                verifier,
            )
            .map_err(|_| denied())?;
        let b = authority.binding();
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        self.issuer
            .verify_pin(
                &pin,
                &floor,
                AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
                b.issued_at.0,
                b.expires_at.0,
                now.get(),
            )
            .map_err(|_| denied())?;
        let bytes = Zeroizing::new(authority.signing_bytes().map_err(|_| denied())?);
        let signature = hex::encode(self.sign_bytes(&bytes));
        state
            .record_pending_source_dispatch_v2(authority, signature, pin, floor, now)
            .map_err(|_| denied())
    }
    pub(crate) fn pending_source_native_leaf_v2(
        &self,
        env: &Env,
        state: &mut AccountProvisionApprovalState,
        proof: &SignedPendingSourceInspectDispatchV2,
        now: UnixMillis,
    ) -> Result<SignedPendingSourceNativeLeafV2> {
        if proof.binding.issuer != self.issuer {
            return Err(denied());
        }
        let [dispatch_pin, native_pin] = self.source_inspect_pair(env)?;
        let floor = AccountMachineIssuerFloorV2 {
            issuer_epoch: self.issuer.issuer_epoch,
            config_generation: self.issuer.config_generation,
        };
        let authority = state
            .pending_source_native_leaf_authority_v2(proof, &dispatch_pin, &floor, now)
            .map_err(|_| denied())?;
        let c = authority.claim();
        self.issuer
            .verify_pin(
                &native_pin,
                &floor,
                AccountMachinePurposeV2::ExecutionPendingSourceNativeLeafV2,
                c.claimed_at.0,
                c.expires_at.0,
                now.get(),
            )
            .map_err(|_| denied())?;
        let bytes = Zeroizing::new(authority.signing_bytes().map_err(|_| denied())?);
        let signature = hex::encode(self.sign_bytes(&bytes));
        state
            .record_pending_source_native_leaf_v2(authority, signature, native_pin, floor, now)
            .map_err(|_| denied())
    }
}
