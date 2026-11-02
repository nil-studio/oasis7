//! Borrowed Agent GEN lowering with accepted-history Domain provenance.
//! Outputs remain provisional; this does not qualify a new head or new Event.
use super::incremental_body::{BodyMutationContext, BodyMutationError};
use super::{World, incremental_agent_genesis::AgentGenesisContext};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1 as Ns, incremental_value::ValueNodeWriter,
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::incremental_domain_event_proof::{
    DomainEventProofContextV1, MailboxDomainProofRequestV1, VerifiedDomainEventOccurrenceV1,
    verified_mailbox_domain_original_with_cancel_v1,
};

pub struct VerifiedAgentGenesisContextV1<'a, 'budget, W, C, F, L, V> {
    pub mutation: BodyMutationContext<'a, W, C>,
    pub emit: F,
    /// Must return a private witness of an actual accepted historical original.
    /// The ordinal remains a locator, not proof that the payload matches.
    pub mailbox_original: L,
    pub validation: &'a mut DomainEventProofContextV1<'budget, V>,
    pub expected_head_record: [u8; 32],
    pub expected_roots: [u8; 32],
    pub max_scalar_bytes: u64,
}
impl World {
    /// Use the original GEN visitor/encoders with a concrete provenance consumer.
    /// This is useful for lowering borrowed state whose mailbox originals are
    /// already accepted. A same-transaction newly prepared Event cannot supply
    /// this witness. Neither success nor an emitted prefix activates a World.
    pub fn write_iws_agent_genesis_fields_verified<
        'event,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(Ns, &[u8], [u8; 32]) -> Result<(), BodyMutationError>,
        L: FnMut(
            u64,
        )
            -> Result<ReservedValue<VerifiedDomainEventOccurrenceV1<'event>>, BodyMutationError>,
        V: FnMut() -> bool,
    >(
        &self,
        agent: &str,
        context: &mut VerifiedAgentGenesisContextV1<'_, '_, W, C, F, L, V>,
    ) -> Result<u64, BodyMutationError> {
        let VerifiedAgentGenesisContextV1 {
            mutation,
            emit,
            mailbox_original,
            validation,
            expected_head_record,
            expected_roots,
            max_scalar_bytes,
        } = context;
        if !validation.uses_budget(mutation.memory) {
            validation.invalidate();
            return Err(BodyMutationError::Invalid("verified Agent GEN proof pool"));
        }
        validation
            .check_cancelled()
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        // The original GEN invokes its writer and mailbox resolver sequentially.
        // Both need these same two mutable cancellation channels; the cells stay
        // private and no callback can retain or reenter either shared borrow.
        let cancelled = std::cell::RefCell::new(&mut mutation.cancelled);
        let validation = std::cell::RefCell::new(validation);
        let mut lowered = AgentGenesisContext {
            mutation: BodyMutationContext {
                sink: &mut *mutation.sink,
                memory: mutation.memory,
                io: &mut *mutation.io,
                cancelled: || {
                    (*cancelled.borrow_mut())()
                        || validation.borrow_mut().check_cancelled().is_err()
                },
            },
            emit,
            mailbox_original: |ordinal, domain: &oasis7_world_state::DomainEvent| {
                let proof = mailbox_original(ordinal)?;
                verified_mailbox_domain_original_with_cancel_v1(
                    &MailboxDomainProofRequestV1 {
                        domain,
                        expected_original: proof.occurrence().location().original,
                        proof: &proof,
                        expected_head_record: *expected_head_record,
                        expected_roots: *expected_roots,
                    },
                    **validation.borrow_mut(),
                    &mut || (*cancelled.borrow_mut())(),
                )
                .map_err(|e| BodyMutationError::Original(Box::new(e)))
            },
            max_scalar_bytes: *max_scalar_bytes,
        };
        let result = self.write_iws_agent_genesis_fields(agent, &mut lowered);
        if result.is_err() {
            validation.borrow_mut().invalidate();
            return result;
        }
        validation
            .borrow_mut()
            .check_cancelled()
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        result
    }
}

#[cfg(test)]
#[path = "incremental_agent_verified_genesis_tests.rs"]
mod tests;
