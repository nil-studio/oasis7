//! Fixed operational admission for the original native checkpoint kernel.
//! The archive supplies only paid bytes. No caller can replace predicates or
//! implement the private decoder root set.
use super::super::{
    bounded_cbor_data::{self as cbor, CborDataLayoutV1, CborDataLimitsV1},
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;

struct Phase {
    _graph: MemoryReservation,
    _node: MemoryReservation,
    next: Option<Box<Phase>>,
}
struct Retention {
    phases: Option<Box<Phase>>,
    shell: Option<MemoryReservation>,
}
impl Retention {
    fn push(
        &mut self,
        graph: MemoryReservation,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        let node = common
            .memory()
            .reserve(size_of::<Phase>(), MemoryClass::Ordinary)
            .map_err(|_| invalid("native provenance phase admission"))?;
        self.phases = Some(Box::new(Phase {
            _graph: graph,
            _node: node,
            next: self.phases.take(),
        }));
        Ok(())
    }
    fn bytes(&self) -> usize {
        let mut total = self.shell.as_ref().map_or(0, MemoryReservation::bytes);
        let mut current = self.phases.as_deref();
        while let Some(p) = current {
            total = total
                .saturating_add(p._graph.bytes())
                .saturating_add(p._node.bytes());
            current = p.next.as_deref();
        }
        total
    }
}
// Closed reachable graph: SignedHot -> NativeHot -> optional base Record,
// seal/durable binding; HistoryNode -> Vec<HistoryLink>; ArchivedEntry ->
// QualifiedProof -> Decision/Record/Frozen/receipts. Their only dynamic typed
// containers are String, Vec<HistoryLink> and BTreeMap<ArtifactRole, String>.
// before() additionally covers the borrowed authority/policy/provider tuple,
// whose only extra container is the policy's Vec<String>. Embedded structs and
// Option fields belong to the enclosing root, not to every CBOR field/value.
// The shared engine separately pays serde Content, strings, collection growth
// and malformed length hints. None of these closed types has a Box, a set or
// a default that allocates. No external type can extend ClosedNative.
fn layout() -> CborDataLayoutV1 {
    let envelope = size_of::<SignedHot>()
        + size_of::<NativeHistoryNodeV1>()
        + size_of::<ArchivedEntry>()
        + size_of::<NativeCheckpointVerdictV1>();
    CborDataLayoutV1 {
        root: envelope,
        sequence_element: size_of::<NativeHistoryLinkV1>().max(size_of::<String>()),
        map_pair: size_of::<(
            crate::controlled_authority::replicated_protocol::ArtifactRole,
            String,
        )>(),
        set_element: 0,
        box_pointee: 0,
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 16,
        missing: None,
    }
}
fn cbor_error(error: cbor::BoundedCborDataErrorV1) -> FormalError {
    match error {
        cbor::BoundedCborDataErrorV1::Common(e) => e,
        other => invalid(other.to_string()),
    }
}
fn admit(
    raw: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
    retention: &mut Retention,
) -> Result<(), FormalError> {
    let limits = CborDataLimitsV1 {
        max_wire_bytes: LIMIT,
        max_graph_bytes: usize::MAX,
        max_depth: 128,
    };
    // Four simultaneous graph owners: decoded wire root; the original native
    // kernel's base-parent/head clones; restored qualified proof; restored
    // predecessor. The original graph stays paid through both restore outputs.
    for _ in 0..4 {
        let graph = cbor::reserve_closed_cbor_output_v1(raw, layout(), &limits, common)
            .map_err(cbor_error)?;
        retention.push(graph, common)?;
    }
    let bytes = u64::try_from(raw.len()).map_err(|_| invalid("native provenance byte layout"))?;
    common.charge(
        bytes
            .checked_mul(16)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("native provenance visit layout"))?,
        bytes
            .checked_mul(16)
            .ok_or_else(|| invalid("native provenance copy layout"))?,
    )
}
struct Ops<'a, 'ctx, F> {
    common: &'a mut FormalBoundedContextV1<'ctx>,
    load: F,
    retention: Retention,
}
impl<F> NativeReadOps for Ops<'_, '_, F>
where
    F: FnMut(&str, &mut FormalBoundedContextV1<'_>) -> Result<ReservedValue<Vec<u8>>, FormalError>,
{
    fn decode<T: ClosedNative>(&mut self, raw: &[u8]) -> Result<T, FormalError> {
        admit(raw, self.common, &mut self.retention)?;
        serde_cbor::from_slice(raw).map_err(|e| invalid(e.to_string()))
    }
    fn object<T: ClosedNative>(&mut self, id: &str) -> Result<T, FormalError> {
        self.common.charge(1, id.len() as u64)?;
        crypto::hash_hex(id)?;
        let bytes = (self.load)(id, self.common)?;
        if !bytes.uses_budget(self.common.memory()) {
            return Err(invalid("native provenance foreign object pool"));
        }
        // Original order: load, bound/hash, decoder. Hash is streamed, with
        // payment immediately before each actual chunk, and no copied bytes.
        let matches = if bytes.len() > LIMIT {
            false
        } else {
            let mut hash = blake3::Hasher::new();
            for chunk in bytes.chunks(8192) {
                self.common.charge(1, chunk.len() as u64)?;
                hash.update(chunk);
            }
            hash.finalize().to_hex().as_str() == id
        };
        if !matches {
            return Err(invalid("native provenance referenced CAS checksum/bound"));
        }
        self.decode(&bytes)
    }
    fn before<T: Serialize>(&mut self, value: &T) -> Result<(), FormalError> {
        // Same CBOR serializer as original hashes/signatures. This payment
        // covers the subsequent original serializer traversals, policy clones,
        // equality scans and both Ed25519 checks; it supplies no verdict.
        let bytes = super::super::context_wire::encode_paid(
            value,
            LIMIT,
            self.common,
            "formal encoding exceeds byte bound",
            "native provenance",
        )?;
        admit(&bytes, self.common, &mut self.retention)
    }
}
/// Actual native verdict and all decoder/clone admissions. This owner grants
/// neither namespace continuity nor an accepted storage source by itself.
pub struct PaidNativeCheckpointVerdictV1 {
    value: NativeCheckpointVerdictV1,
    identity: WorkIdentityV1,
    retention: Retention,
}
impl PaidNativeCheckpointVerdictV1 {
    pub fn value(&self) -> &NativeCheckpointVerdictV1 {
        &self.value
    }
    pub fn reserved_bytes(&self) -> usize {
        self.retention.bytes()
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.retention
            .shell
            .as_ref()
            .is_some_and(|fee| fee.uses_budget(memory))
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        if self.identity != common.work().identity() || !self.uses_budget(common.memory()) {
            return Err(invalid("native provenance foreign context"));
        }
        Ok(())
    }
}
impl std::fmt::Debug for PaidNativeCheckpointVerdictV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidNativeCheckpointVerdictV1")
            .finish_non_exhaustive()
    }
}
pub struct PaidNativeCheckpointErrorV1 {
    error: FormalError,
    _retention: Retention,
}
impl PaidNativeCheckpointErrorV1 {
    pub fn error(&self) -> &FormalError {
        &self.error
    }
}
impl std::fmt::Debug for PaidNativeCheckpointErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
impl std::fmt::Display for PaidNativeCheckpointErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for PaidNativeCheckpointErrorV1 {}
impl NativeCheckpointVerdictV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn verify_bounded(
        signed_checkpoint: &[u8],
        checkpoint_authority: &VerifiedAuthorityV1,
        endpoint_public_key: &str,
        expected_checkpoint: &fencing::EndpointCheckpointBindingV1,
        provider: &fencing::MonotonicProviderEvidenceV1,
        target: &FormalHeadRefV1,
        minimum: &FormalHeadRefV1,
        authority_id: &str,
        load: impl FnMut(
            &str,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<ReservedValue<Vec<u8>>, FormalError>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PaidNativeCheckpointVerdictV1, PaidNativeCheckpointErrorV1> {
        // Reserving the output shell is the first fallible allocation. Its
        // admission failure needs no decoder graph retention.
        let shell = match common.memory().reserve(
            size_of::<PaidNativeCheckpointVerdictV1>(),
            MemoryClass::Ordinary,
        ) {
            Ok(fee) => fee,
            Err(_) => {
                return Err(PaidNativeCheckpointErrorV1 {
                    error: invalid("native provenance verdict admission"),
                    _retention: Retention {
                        phases: None,
                        shell: None,
                    },
                });
            }
        };
        let identity = common.work().identity();
        let mut ops = Ops {
            common,
            load,
            retention: Retention {
                phases: None,
                shell: Some(shell),
            },
        };
        let result = Self::verify_with_ops(
            signed_checkpoint,
            checkpoint_authority,
            endpoint_public_key,
            expected_checkpoint,
            provider,
            target,
            minimum,
            authority_id,
            &mut ops,
        );
        match result {
            Ok(value) => Ok(PaidNativeCheckpointVerdictV1 {
                value,
                identity,
                retention: ops.retention,
            }),
            Err(error) => Err(PaidNativeCheckpointErrorV1 {
                error,
                _retention: ops.retention,
            }),
        }
    }
}
