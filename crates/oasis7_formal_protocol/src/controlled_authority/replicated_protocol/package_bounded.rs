//! Fixed same-Core adapter for the original declared IWS closure kernel.
use super::*;
use crate::controlled_authority::formal::{
    FormalError, bounded_cbor_data::PaidCborDataV1, bounded_context::FormalBoundedContextV1,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
struct Paid<'a, 'c> {
    ctx: &'a mut FormalBoundedContextV1<'c>,
    closure: Option<MemoryReservation>,
}
enum Failure {
    Resource(FormalError),
    Protocol(ProtocolError),
    Native(crate::native_environment::NativeCodecRefusal),
}
impl From<ProtocolError> for Failure {
    fn from(e: ProtocolError) -> Self {
        Self::Protocol(e)
    }
}
impl From<FormalError> for Failure {
    fn from(e: FormalError) -> Self {
        Self::Resource(e)
    }
}
pub enum BoundedIwsValidationErrorV1 {
    Resource(FormalError),
    Protocol(PaidIwsValidationRefusalV1),
}
pub struct PaidIwsValidationRefusalV1 {
    source: Failure,
    _fee: MemoryReservation,
}
impl std::fmt::Display for PaidIwsValidationRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Failure::Protocol(e) => e.fmt(f),
            Failure::Native(note) => write!(f, "Invalid({:?})", note.message()),
            Failure::Resource(e) => e.fmt(f),
        }
    }
}
impl std::fmt::Debug for PaidIwsValidationRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaidIwsValidationRefusalV1(original paid refusal)")
    }
}
impl std::fmt::Debug for BoundedIwsValidationErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(e) => f.debug_tuple("Resource").field(e).finish(),
            Self::Protocol(e) => f.debug_tuple("Protocol").field(e).finish(),
        }
    }
}
impl std::fmt::Display for BoundedIwsValidationErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(e) => e.fmt(f),
            Self::Protocol(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for BoundedIwsValidationErrorV1 {}
impl PaidIwsValidationRefusalV1 {
    pub fn uses_budget(&self, memory: &oasis7_proto::memory_budget::MemoryBudget) -> bool {
        self._fee.uses_budget(memory)
    }
}
impl PackageValidationOps for Paid<'_, '_> {
    type Error = Failure;
    type Native = PaidCborDataV1<crate::native_environment::NativeExecutionEnvironmentV1>;
    fn work(&mut self, n: usize) -> Result<(), Failure> {
        self.ctx.charge(1, n as u64).map_err(Into::into)
    }
    fn retention(&mut self, raw: &[u8]) -> Result<(), Failure> {
        let memory = self.ctx.memory().clone();
        let mut first = None;
        let result = crate::incremental_retention::RetentionLedgerManifestV1::decode_paid_with_work(
            raw,
            &memory,
            &mut |s, b| {
                self.ctx.charge(s, b).map_err(|e| {
                    if first.is_none() {
                        first = Some(e)
                    }
                    crate::incremental_execution::ExecutionError::Aborted
                })
            },
        );
        if let Some(error) = first {
            return Err(Failure::Resource(error));
        }
        result.map(|_| ()).map_err(|error| match error {
            crate::incremental_execution::ExecutionError::Memory(_) => Failure::Resource(
                FormalError::Invalid("retention descriptor memory admission".into()),
            ),
            _ => Failure::Protocol(invalid("retention ledger descriptor codec")),
        })
    }
    fn before_closure(&mut self, r: &ClosedRecord) -> Result<(), Failure> {
        let mut refs = r.roots.len();
        for object in &r.objects {
            self.work(0)?;
            refs = refs
                .checked_add(object.references.len())
                .ok_or_else(|| FormalError::Invalid("IWS reference graph overflow".into()))?;
        }
        // Map<&str,&Object> and Set<&str> each have at most n+1 nodes: a
        // missing reached key stops immediately. Queue visits every real reference;
        // doubled slots cover Vec geometric growth, without copying its Strings.
        let node = 11 * (std::mem::size_of::<&str>() + std::mem::size_of::<&ArtifactObject>())
            + 14 * std::mem::size_of::<usize>()
            + 128;
        let bytes = (r.objects.len() + 1)
            .checked_mul(2 * node)
            .and_then(|n| {
                refs.checked_mul(2 * std::mem::size_of::<&str>())
                    .and_then(|q| n.checked_add(q))
            })
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| FormalError::Invalid("IWS closure graph layout".into()))?;
        self.work(bytes)?;
        self.closure = Some(
            self.ctx
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(|_| FormalError::Invalid("IWS closure graph admission".into()))?,
        );
        Ok(())
    }
    fn native(&mut self, b: &[u8]) -> Result<Self::Native, Failure> {
        match crate::native_environment::decode_native_environment_bounded(b, self.ctx) {
            Ok(v) => Ok(v),
            Err(crate::native_environment::NativeBoundedError::Resource(e)) => {
                Err(Failure::Resource(e))
            }
            Err(crate::native_environment::NativeBoundedError::Codec(note)) => {
                Err(Failure::Native(note))
            }
        }
    }
    fn hash(&mut self, b: &[u8]) -> Result<blake3::Hash, Failure> {
        let mut h = blake3::Hasher::new();
        for chunk in b.chunks(8192) {
            self.work(chunk.len())?;
            h.update(chunk);
        }
        self.work(64)?;
        Ok(h.finalize())
    }
}
impl ClosedRecord {
    /// Declared closure only: no execution correctness, finality or durability is
    /// inferred. Native parsing/validation and object predicates are the legacy
    /// kernel, with actual graph admission and continuing work at each operation.
    pub fn validate_iws_bounded(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BoundedIwsValidationErrorV1> {
        ctx.consume_steps(1)
            .map_err(BoundedIwsValidationErrorV1::Resource)?;
        // Original package predicate errors are fixed literals. Dynamic codec
        // refusals retain their original independently paid native note below.
        let fee = ctx
            .memory()
            .reserve(
                4096 + std::mem::size_of::<PaidIwsValidationRefusalV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| {
                BoundedIwsValidationErrorV1::Resource(FormalError::Invalid(
                    "IWS refusal admission".into(),
                ))
            })?;
        let result = self.validate_iws_with(&mut Paid { ctx, closure: None });
        match result {
            Err(Failure::Resource(e)) => Err(BoundedIwsValidationErrorV1::Resource(e)),
            Err(source) => Err(BoundedIwsValidationErrorV1::Protocol(
                PaidIwsValidationRefusalV1 { source, _fee: fee },
            )),
            Ok(()) => {
                ctx.consume_steps(0)
                    .map_err(BoundedIwsValidationErrorV1::Resource)?;
                Ok(())
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
    use oasis7_proto::memory_budget::MemoryBudget;
    fn fixture() -> ClosedRecord {
        let (desc, _) =
            crate::controlled_authority::formal::test_fixtures::incremental::native_fixture();
        with_native(desc.encode().unwrap())
    }
    fn with_native(bytes: Vec<u8>) -> ClosedRecord {
        let input = b"original-input".to_vec();
        let a = blake3::hash(&input).to_hex().to_string();
        let n = blake3::hash(&bytes).to_hex().to_string();
        let roles = [
            ArtifactRole::Input,
            ArtifactRole::IncrementalGenesisConfig,
            ArtifactRole::IncrementalRootSet,
            ArtifactRole::IncrementalCommitBody,
            ArtifactRole::IncrementalHistoryPrefix,
            ArtifactRole::ExecutionManifest,
            ArtifactRole::Wasm,
            ArtifactRole::Rules,
            ArtifactRole::NativeExecutionEnvironment,
        ];
        let roots = roles
            .into_iter()
            .map(|r| {
                (
                    r,
                    if r == ArtifactRole::NativeExecutionEnvironment {
                        n.clone()
                    } else {
                        a.clone()
                    },
                )
            })
            .collect();
        let mut objects = vec![
            ArtifactObject {
                content_hash: a.clone(),
                bytes: input,
                references: vec![],
            },
            ArtifactObject {
                content_hash: n,
                bytes,
                references: vec![],
            },
        ];
        objects.sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
        ClosedRecord {
            payload_digest: a,
            before_state_root: "01".repeat(32),
            after_state_root: "02".repeat(32),
            roots,
            objects,
        }
    }
    fn check(record: &ClosedRecord) {
        let old = record.validate_iws().map_err(|e| e.to_string());
        let memory = MemoryBudget::new(128 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
        let mut stop = || false;
        let result = record
            .validate_iws_bounded(&mut FormalBoundedContextV1::new(&memory, &work, &mut stop));
        match (&result, &old) {
            (Ok(()), Ok(())) => assert_eq!(memory.used(), 0),
            (Err(BoundedIwsValidationErrorV1::Protocol(note)), Err(old)) => {
                assert_eq!(note.to_string(), *old);
                assert!(memory.used() > 0);
                assert!(note.uses_budget(&memory));
            }
            _ => panic!("original closure result differs: {result:?} {old:?}"),
        };
        drop(result);
        assert_eq!(memory.used(), 0);
    }
    #[test]
    fn declared_iws_roles_native_codec_hash_and_graph_keep_original_order() {
        let original = fixture();
        check(&original);
        let mut wrong = original.clone();
        wrong.payload_digest = "bad".into();
        wrong.objects.clear();
        check(&wrong);
        let mut wrong = original.clone();
        wrong.objects[0].bytes.push(0);
        check(&wrong);
        let mut wrong = original.clone();
        let input = wrong
            .objects
            .iter_mut()
            .find(|o| o.content_hash == wrong.payload_digest)
            .unwrap();
        input.references = vec!["ff".repeat(32)];
        check(&wrong);
        let mut wrong = original;
        wrong.objects.push(ArtifactObject {
            content_hash: blake3::hash(b"orphan").to_hex().to_string(),
            bytes: b"orphan".to_vec(),
            references: vec![],
        });
        wrong
            .objects
            .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
        check(&wrong);
    }
    #[test]
    fn invalid_native_wire_refusal_keeps_its_actual_paid_error_until_drop() {
        check(&with_native(vec![0xff]));
        let (desc, _) =
            crate::controlled_authority::formal::test_fixtures::incremental::native_fixture();
        let wire = desc.encode().unwrap();
        assert_eq!(wire[0] >> 5, 5);
        let mut indefinite = vec![0xbf];
        indefinite.extend_from_slice(&wire[1..]);
        indefinite.push(0xff);
        check(&with_native(indefinite));
        let mut invalid = desc;
        invalid.components[0].content_digest = "bad".into();
        check(&with_native(serde_cbor::to_vec(&invalid).unwrap()));
    }
    #[test]
    fn cancellation_remains_resource_and_never_becomes_protocol_refusal() {
        let record = fixture();
        let memory = MemoryBudget::new(128 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
        let mut calls = 0;
        let mut stop = || {
            calls += 1;
            true
        };
        let result = record
            .validate_iws_bounded(&mut FormalBoundedContextV1::new(&memory, &work, &mut stop));
        assert!(matches!(
            result,
            Err(BoundedIwsValidationErrorV1::Resource(_))
        ));
        assert_eq!(calls, 1);
        drop(result);
        assert_eq!(memory.used(), 0);
    }
}
