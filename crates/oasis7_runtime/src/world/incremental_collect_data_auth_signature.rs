//! Signature-only source for the selected signed request. This is deliberately
//! not claim binding, nonce permission, admission, settlement or a tick permit.
use crate::world::{
    incremental_action_execution::SelectedPendingActionV1, incremental_body::BodyMutationError,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
pub(in crate::world) struct CollectDataSignatureWorkV1 {
    pub checks_remaining: u64,
    pub bytes_remaining: u64,
}
/// Immutable borrowed request fields. The requested collector is not part of
/// the legacy signing payload; a genuine Current claim binding is still due.
pub(in crate::world) struct CollectDataAuthFieldsV1<'a> {
    pub collector: &'a str,
    pub electricity_cost: i64,
    pub data_amount: i64,
    pub player: &'a str,
    pub public_key: &'a str,
    pub nonce: u64,
    pub signature: &'a str,
}
pub(in crate::world) struct VerifiedCollectDataSignatureV1<'a> {
    selected: &'a SelectedPendingActionV1,
    outcome: &'a ReservedValue<RootRulePhaseOutcomeV1>,
    fields: CollectDataAuthFieldsV1<'a>,
    identity: PreparedGuard,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl VerifiedCollectDataSignatureV1<'_> {
    pub(in crate::world) fn fields(&self) -> &CollectDataAuthFieldsV1<'_> {
        &self.fields
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.contribute(|tx| {
            tx.validate_transaction_identity(&self.identity)?;
            tx.validate_guard(&tx.guard(), live)?;
            selected.validate(tx)?;
            if !std::ptr::eq(self.selected, selected)
                || !std::ptr::eq(self.outcome, outcome)
                || !self.memory.same_pool(memory)
                || !tx.uses_budget(memory)
                || !outcome.uses_budget(memory)
            {
                return Err(BodyMutationError::Invalid(
                    "authenticated collect signature source binding",
                ));
            }
            Ok(())
        })
    }
}
pub(in crate::world) enum SelectedCollectDataSignatureV1<'a> {
    Verified(VerifiedCollectDataSignatureV1<'a>),
    Rejected(ReservedValue<crate::RejectReason>),
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn verify_selected_collect_data_signature_v1<'a, M: MerkleNodeReader>(
    selected: &'a SelectedPendingActionV1,
    outcome: &'a ReservedValue<RootRulePhaseOutcomeV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    work: &mut CollectDataSignatureWorkV1,
    live: &ExecutionFence,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<SelectedCollectDataSignatureV1<'a>, BodyMutationError> {
    tx.contribute(|tx| {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        selected.validate(tx)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !tx.uses_budget(memory) || !outcome.uses_budget(memory) {
            return Err(BodyMutationError::Invalid(
                "authenticated collect signature pool",
            ));
        }
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "signature verifier requires non-denied Rule",
            ));
        }
        let action = outcome
            .override_action_for_selected_v1(selected, tx)?
            .unwrap_or(&selected.envelope().action);
        let crate::Action::CollectDataAuthenticated {
            collector_agent_id,
            electricity_cost,
            data_amount,
            player_id,
            public_key,
            nonce,
            signature,
        } = action
        else {
            return Err(BodyMutationError::Invalid(
                "authenticated collect signature action",
            ));
        };
        let text = collector_agent_id
            .len()
            .checked_add(player_id.len())
            .and_then(|n| n.checked_add(public_key.len()))
            .and_then(|n| n.checked_add(signature.len()))
            .ok_or(ValueTreeError::ResourceLimited(
                "authenticated collect signature text",
            ))?;
        work.checks_remaining =
            work.checks_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "authenticated collect signature checks",
                ))?;
        work.bytes_remaining = work
            .bytes_remaining
            .checked_sub(u64::try_from(text).map_err(|_| {
                ValueTreeError::ResourceLimited("authenticated collect signature bytes")
            })?)
            .ok_or(ValueTreeError::ResourceLimited(
                "authenticated collect signature bytes",
            ))?;
        // The existing verifier allocates hex buffers, the signed CBOR payload and
        // bounded error text. Pay before entering it, including all input lengths.
        let bytes = text
            .checked_mul(8)
            .and_then(|n| n.checked_add(8192))
            .and_then(|n| n.checked_add(std::mem::size_of::<VerifiedCollectDataSignatureV1>()))
            .ok_or(ValueTreeError::ResourceLimited(
                "authenticated collect signature allocation",
            ))?;
        let fee = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let verified = oasis7_proto::collect_data_auth::verify_authorization(
            oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
            *electricity_cost,
            *data_amount,
            player_id,
            public_key,
            *nonce,
            signature,
        );
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        match verified {
            Err(error) => Ok(SelectedCollectDataSignatureV1::Rejected(fee.retain(
                crate::RejectReason::RuleDenied {
                    notes: vec![format!(
                        "authenticated collect_data signature invalid: {error}"
                    )],
                },
            ))),
            Ok(()) => Ok(SelectedCollectDataSignatureV1::Verified(
                VerifiedCollectDataSignatureV1 {
                    selected,
                    outcome,
                    fields: CollectDataAuthFieldsV1 {
                        collector: collector_agent_id,
                        electricity_cost: *electricity_cost,
                        data_amount: *data_amount,
                        player: player_id,
                        public_key,
                        nonce: *nonce,
                        signature,
                    },
                    identity: tx.guard(),
                    memory: memory.clone(),
                    _fee: fee,
                },
            )),
        }
    })
}
