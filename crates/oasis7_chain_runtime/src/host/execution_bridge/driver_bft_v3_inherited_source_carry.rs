//! Original Source input verification under an authenticated first Active carry.
//! This does not admit Fresh11/12 or reinterpret an original Source key.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input, BftPrivateRetentionLifecycleV1 as Lifecycle,
    BftPrivateRetentionVerificationContextV1 as Context,
};
impl ActualBftPrivateSourceCreationVerifierV1 {
    pub(crate) fn with_active_admission_verifier(
        mut self,
        verifier: Arc<super::super::successor_verifier::ActualBftSuccessorWindowDriverVerifierV1>,
    ) -> Result<Self, String> {
        verifier.verify_active_admission_configuration(&self.records, &self.store)?;
        self.active_admission_verifier = Some(verifier);
        Ok(self)
    }
    pub(super) fn verify_inherited_source_carry(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &Input,
        context: &Context<'_>,
        purpose: oasis7_distfs::protected_bft_v3::BftPrivateRetentionSigningPurposeV1,
        deadline: std::time::Instant,
    ) -> Result<(), String> {
        successor_archive::live(deadline)?;
        let verifier = self
            .active_admission_verifier
            .as_ref()
            .ok_or("Active carry requires explicitly configured independent archive verifier")?;
        // Original kind validation accepts only Source9/10. It does not enable
        // a new domain or classify local selection from original kind.
        node_execution::validate_source_admission_slot_kind(input)?;
        let ledger = context
            .ledger()
            .ok_or("Active carry current original ledger missing")?;
        if ledger.map.source != context.attestor().source
            || ledger.map.source != input.source
            || entry.lifecycle != Lifecycle::Ready
            || ledger.map.entries.iter().find(|e| e.key == input.key) != Some(entry)
            || entry.key != input.key
            || entry.fact != input.fact().map_err(|e| e.to_string())?
        {
            return Err("Active carry differs from current exact Ready original".into());
        }
        let reference =
            InputRef::from_entry(&ledger.map.source, entry).map_err(|e| e.to_string())?;
        reference.validate_input(input).map_err(|e| e.to_string())?;
        let active = context.active_parent().map_err(|e| e.to_string())?;
        if active.binding_digest() != binding.digest().map_err(|e| e.to_string())?
            || active.provider_public_key() != context.attestor().provider_public_key
            || !context.signer().state.history.is_empty()
            || active.base_schedule().height() != input.key.height
            || context
                .current_schedule()
                .map_err(|e| e.to_string())?
                .height()
                != input.key.height
        {
            return Err("Source carry is not the exact first authenticated Active parent".into());
        }
        let local_selection = if purpose
            == oasis7_distfs::protected_bft_v3::BftPrivateRetentionSigningPurposeV1::LocalProposal
        {
            let index = ledger
                .selection_index
                .as_ref()
                .ok_or("Active Local proposal lacks protected current selection")?;
            if !index
                .selections
                .iter()
                .any(|selected| selected.key == reference.key && selected.reference == reference)
            {
                return Err("Active Local proposal is not the exact selected original".into());
            }
            Some((index, node_execution::source_selection_identity(input)?))
        } else {
            None
        };
        let historical = context
            .original_ready(binding, &entry.key)
            .map_err(|e| e.to_string())?;
        let original = historical
            .reservation()
            .inputs
            .get(&reference.admission_digest)
            .ok_or("Active carry first reservation original missing")?;
        reference
            .validate_input(original)
            .map_err(|e| e.to_string())?;
        if original.as_ref() != input {
            return Err("Active carry current bytes differ from original reservation".into());
        }
        // Keep the old Source successor guard: it validates the ORIGINAL
        // Reserve State, not the current Active State. Its exact old inventory
        // is also the only price authority, never today's shared refs.
        let parent = self.verify_historical_input_parent(binding, input, &historical, deadline)?;
        verifier.verify_inherited_source_carry_parent(
            binding,
            context,
            &active,
            &reference,
            &parent,
            local_selection
                .as_ref()
                .map(|(index, identity)| (*index, identity)),
            deadline,
        )?;
        node_execution::verify_source_admission_ready(&self.records, &self.store, input)?;
        successor_archive::live(deadline)
    }
}
