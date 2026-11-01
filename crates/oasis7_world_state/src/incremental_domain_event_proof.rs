//! Concrete borrowed WorldEvent semantics over an authenticated Event original.
//! This proves the Domain envelope and original bytes, not execution authority.
use crate::{DomainEvent, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::incremental_material::VerifiedEventFactOccurrenceV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError,
    CanonicalCborWorkV1, stream_canonical_cbor_v1,
};
use std::io::{self, Write};

struct Allocator<'a> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
}
impl CanonicalCborAllocator for Allocator<'_> {
    type Charge = MemoryReservation;
    fn reserve(&self, bytes: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.memory
            .reserve(bytes, self.class)
            .map_err(|_| CanonicalCborStreamError::ResourceLimited("Domain canonical memory"))
    }
}

/// One operation's continuous allowances. Every failed verification poisons the
/// context; constructing a second witness cannot reset work within this context.
pub struct DomainEventProofContextV1<'a, C> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    work: Option<CanonicalCborWorkV1>,
    cancelled: C,
    poisoned: bool,
}
impl<'a, C: FnMut() -> bool> DomainEventProofContextV1<'a, C> {
    pub fn new(
        memory: &'a MemoryBudget,
        class: MemoryClass,
        work: CanonicalCborWorkV1,
        cancelled: C,
    ) -> Self {
        Self {
            memory,
            class,
            work: Some(work),
            cancelled,
            poisoned: false,
        }
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.memory.same_pool(memory)
    }
    /// A caller composing this verification with writes must check its final
    /// boundary too. Cancellation permanently invalidates this operation.
    pub fn check_cancelled(&mut self) -> Result<(), CanonicalCborStreamError> {
        if self.poisoned {
            return Err(CanonicalCborStreamError::Poisoned);
        }
        if (self.cancelled)() {
            self.poisoned = true;
            return Err(CanonicalCborStreamError::Cancelled);
        }
        Ok(())
    }
    pub fn invalidate(&mut self) {
        self.poisoned = true;
    }
}

pub struct DomainEventProofRequestV1<'a> {
    pub event: &'a WorldEvent,
    pub occurrence: &'a ReservedValue<VerifiedEventFactOccurrenceV1>,
    pub expected_head_record: [u8; 32],
    pub expected_roots: [u8; 32],
}

/// Holds shared immutable borrows of both verified provenance and the exact
/// concrete event. Neither the event nor its original charge can detach while
/// this witness is live. Construction requires the original canonical hash.
pub struct VerifiedDomainEventOccurrenceV1<'a> {
    event: &'a WorldEvent,
    occurrence: &'a ReservedValue<VerifiedEventFactOccurrenceV1>,
}
impl VerifiedDomainEventOccurrenceV1<'_> {
    pub fn event(&self) -> &WorldEvent {
        self.event
    }
    pub fn domain(&self) -> &DomainEvent {
        match &self.event.body {
            WorldEventBody::Domain(domain) => domain,
            _ => unreachable!("private constructor checks Domain"),
        }
    }
    pub fn occurrence(&self) -> &VerifiedEventFactOccurrenceV1 {
        self.occurrence
    }
}

struct HashSink(blake3::Hasher);
impl Write for HashSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The only input serializer here is the concrete repository WorldEvent type.
/// Arbitrary callbacks or caller-supplied digests cannot mint Domain semantics.
/// The input DTO remains caller owned; only canonical keys and bounded serializer
/// metadata allocate. Long values feed BLAKE3 directly without an encoded Vec.
pub fn verify_domain_event_occurrence_v1<'a, C: FnMut() -> bool>(
    request: &DomainEventProofRequestV1<'a>,
    context: &mut DomainEventProofContextV1<'_, C>,
) -> Result<ReservedValue<VerifiedDomainEventOccurrenceV1<'a>>, CanonicalCborStreamError> {
    use CanonicalCborStreamError as Error;
    if context.poisoned {
        return Err(Error::Poisoned);
    }
    context.poisoned = true;
    if (context.cancelled)() {
        return Err(Error::Cancelled);
    }
    if !request.occurrence.uses_budget(context.memory)
        || request.occurrence.reserved_bytes()
            < std::mem::size_of::<VerifiedEventFactOccurrenceV1>()
    {
        return Err(Error::Invalid("Domain provenance pool/charge"));
    }
    if request.occurrence.head_record_id() != request.expected_head_record
        || request.occurrence.roots_id() != request.expected_roots
    {
        return Err(Error::Invalid("Domain provenance head/roots"));
    }
    if !matches!(&request.event.body, WorldEventBody::Domain(_)) {
        return Err(Error::Invalid("Event original is not Domain"));
    }
    let allocator = Allocator {
        memory: context.memory,
        class: context.class,
    };
    let _scratch = allocator.reserve(8 << 10)?;
    let charge = allocator.reserve(std::mem::size_of::<VerifiedDomainEventOccurrenceV1<'a>>())?;
    let work = context.work.take().ok_or(Error::Poisoned)?;
    let mut encoding = CanonicalCborStreamContextV1::new(&allocator, work, &mut context.cancelled);
    let mut sink = HashSink(blake3::Hasher::new());
    let result = stream_canonical_cbor_v1(request.event, &mut encoding, &mut sink);
    context.work = Some(encoding.work);
    let summary = result?;
    if summary.encoded_bytes != request.occurrence.original_bytes()
        || sink.0.finalize().as_bytes() != &request.occurrence.location().original
    {
        return Err(Error::Invalid("Domain canonical original mismatch"));
    }
    if (context.cancelled)() {
        return Err(Error::Cancelled);
    }
    let witness = charge.retain(VerifiedDomainEventOccurrenceV1 {
        event: request.event,
        occurrence: request.occurrence,
    });
    context.poisoned = false;
    Ok(witness)
}

pub struct MailboxDomainProofRequestV1<'a, 'event> {
    pub domain: &'a DomainEvent,
    pub proof: &'a ReservedValue<VerifiedDomainEventOccurrenceV1<'event>>,
    pub expected_original: [u8; 32],
    pub expected_head_record: [u8; 32],
    pub expected_roots: [u8; 32],
}

/// Match a mailbox's actual Domain value and authenticated original reference.
/// The private proof authenticates the complete envelope; a Domain-shaped
/// callback result alone cannot supply this original. Both borrowed Domain
/// encodings consume the same operation context without materializing either.
pub fn verified_mailbox_domain_original_v1<C: FnMut() -> bool>(
    request: &MailboxDomainProofRequestV1<'_, '_>,
    context: &mut DomainEventProofContextV1<'_, C>,
) -> Result<[u8; 32], CanonicalCborStreamError> {
    verified_mailbox_domain_original_with_cancel_v1(request, context, &mut || false)
}

/// Composition with a mutation's independent cancellation channel. Both are
/// checked throughout key discovery, borrowed value streaming and finalization.
pub fn verified_mailbox_domain_original_with_cancel_v1<C: FnMut() -> bool, D: FnMut() -> bool>(
    request: &MailboxDomainProofRequestV1<'_, '_>,
    context: &mut DomainEventProofContextV1<'_, C>,
    cancelled: &mut D,
) -> Result<[u8; 32], CanonicalCborStreamError> {
    use CanonicalCborStreamError as Error;
    if context.poisoned {
        return Err(Error::Poisoned);
    }
    context.poisoned = true;
    if (context.cancelled)() || cancelled() {
        return Err(Error::Cancelled);
    }
    let proof = request.proof;
    if !proof.uses_budget(context.memory)
        || !proof.occurrence.uses_budget(context.memory)
        || proof.reserved_bytes() < std::mem::size_of::<VerifiedDomainEventOccurrenceV1<'_>>()
        || proof.occurrence.reserved_bytes() < std::mem::size_of::<VerifiedEventFactOccurrenceV1>()
    {
        return Err(Error::Invalid("mailbox Domain proof pool/charge"));
    }
    if proof.occurrence.head_record_id() != request.expected_head_record
        || proof.occurrence.roots_id() != request.expected_roots
        || proof.occurrence.location().original != request.expected_original
    {
        return Err(Error::Invalid("mailbox Domain proof identity"));
    }
    let allocator = Allocator {
        memory: context.memory,
        class: context.class,
    };
    let _scratch = allocator.reserve(8 << 10)?;
    let work = context.work.take().ok_or(Error::Poisoned)?;
    let mut combined = || (context.cancelled)() || cancelled();
    let mut encoding = CanonicalCborStreamContextV1::new(&allocator, work, &mut combined);
    let mut actual = HashSink(blake3::Hasher::new());
    let mut expected = HashSink(blake3::Hasher::new());
    let result = (|| {
        let a = stream_canonical_cbor_v1(request.domain, &mut encoding, &mut actual)?;
        let b = stream_canonical_cbor_v1(proof.domain(), &mut encoding, &mut expected)?;
        Ok::<_, Error>((a, b))
    })();
    context.work = Some(encoding.work);
    let (a, b) = result?;
    if a.encoded_bytes != b.encoded_bytes || actual.0.finalize() != expected.0.finalize() {
        return Err(Error::Invalid("mailbox Domain content mismatch"));
    }
    if (context.cancelled)() || cancelled() {
        return Err(Error::Cancelled);
    }
    context.poisoned = false;
    Ok(request.expected_original)
}

#[cfg(test)]
#[path = "incremental_domain_event_proof_tests.rs"]
mod tests;

#[cfg(any(test, feature = "test-support"))]
#[path = "incremental_domain_event_proof_fixtures.rs"]
pub mod test_support;
