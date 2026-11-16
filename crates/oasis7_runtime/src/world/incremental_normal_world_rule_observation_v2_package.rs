//! V2 observation DATA original: stream twice without a whole encoded buffer.
//! Coverage authority is established by the fixed registrar, not this encoder.
use super::*;
use oasis7_world_state::world_rules::EmergencyCurrentObservationV2;
use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap};

const MAX_ORIGINAL_BYTES: u64 = 64 << 20;
#[derive(Serialize)]
struct Package<'a> {
    domain: &'static str,
    region_id: &'a str,
    observation: &'a EmergencyCurrentObservationV2,
    interval_coverage: &'a BTreeMap<String, i64>,
}
pub(in crate::world) struct PersistedObservationOriginalDataV2 {
    id: [u8; 32],
    cut_object: [u8; 32],
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    guard: PreparedGuard,
    locator: ReservedValue<String>,
    _fee: ReservedValue<()>,
}
impl PersistedObservationOriginalDataV2 {
    pub(in crate::world) fn validate_source<M: MerkleNodeReader>(
        &self,
        source: &PersistedObservationCurrentSourceV2,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ObservationCurrentSourceErrorV2> {
        source.validate_capture(parent, tx, common, live)?;
        common.charge(1, 256)?;
        tx.validate_guard(&self.guard, live)?;
        if self.identity != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.locator.uses_budget(common.memory())
            || !self._fee.uses_budget(common.memory())
            || self.cut_object != *source.object_id()
        {
            return Err(invalid("V2 observation original source scope"));
        }
        Ok(())
    }
    pub(in crate::world) fn object_id(&self) -> &[u8; 32] {
        &self.id
    }
    pub(in crate::world) fn locator(&self) -> &str {
        &self.locator
    }
}
struct State {
    attempted: bool,
    complete: bool,
    first: Option<ObservationCurrentSourceErrorV2>,
    encoding_error: Option<serde_json::Error>,
}
struct Output<'a, 'b, 'c> {
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    state: &'a RefCell<State>,
    physical: Option<&'a mut dyn Write>,
    hash: blake3::Hasher,
    bytes: u64,
}
impl Write for Output<'_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for chunk in bytes.chunks(16 << 10) {
            let next = self
                .bytes
                .checked_add(chunk.len() as u64)
                .filter(|n| *n <= MAX_ORIGINAL_BYTES)
                .ok_or_else(|| {
                    self.state.borrow_mut().first = Some(limited());
                    io::Error::other("V2 observation original limit")
                })?;
            if let Err(error) = self.common.borrow_mut().charge(1, chunk.len() as u64) {
                self.state.borrow_mut().first = Some(error.into());
                return Err(io::Error::other("V2 observation original work"));
            }
            if let Some(out) = self.physical.as_deref_mut() {
                let mut remaining = chunk;
                while !remaining.is_empty() {
                    if let Err(error) = self.common.borrow_mut().consume_steps(1) {
                        self.state.borrow_mut().first = Some(error.into());
                        return Err(io::Error::other("V2 observation original work"));
                    }
                    let n = out.write(remaining)?;
                    if n == 0 || n > remaining.len() {
                        return Err(io::Error::other("V2 observation original write count"));
                    }
                    remaining = &remaining[n..];
                }
            }
            self.hash.update(chunk);
            self.bytes = next;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Stream<'a, 'b, 'c, 'd> {
    package: &'a Package<'a>,
    memory: &'a MemoryBudget,
    common: &'b RefCell<&'c mut FormalBoundedContextV1<'d>>,
    state: &'b RefCell<State>,
    expected: [u8; 32],
    bytes: u64,
}
impl ObservationJsonOriginalStreamV2 for Stream<'_, '_, '_, '_> {
    fn memory(&self) -> &MemoryBudget {
        self.memory
    }
    fn original_id(&self) -> [u8; 32] {
        self.expected
    }
    fn original_bytes(&self) -> u64 {
        self.bytes
    }
    fn write_to(
        &mut self,
        physical: &mut dyn Write,
    ) -> Result<(), ObservationOriginalStreamFailedV2> {
        {
            let mut state = self.state.borrow_mut();
            if state.attempted {
                if state.first.is_none() {
                    state.first = Some(invalid("V2 observation original stream reused"));
                }
                state.complete = false;
                return Err(ObservationOriginalStreamFailedV2);
            }
            state.attempted = true;
        }
        let mut output = Output {
            common: self.common,
            state: self.state,
            physical: Some(physical),
            hash: blake3::Hasher::new(),
            bytes: 0,
        };
        if let Err(error) = serde_json::to_writer(&mut output, self.package) {
            self.state.borrow_mut().encoding_error = Some(error);
            return Err(ObservationOriginalStreamFailedV2);
        }
        if output.bytes != self.bytes || *output.hash.finalize().as_bytes() != self.expected {
            self.state.borrow_mut().first =
                Some(invalid("V2 observation original stable encoding"));
            return Err(ObservationOriginalStreamFailedV2);
        }
        self.state.borrow_mut().complete = true;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn persist_observation_original_data_v2<
    M: MerkleNodeReader,
    S: ObservationJsonOriginalSinkV2,
>(
    region: &str,
    observation: &EmergencyCurrentObservationV2,
    intervals: &BTreeMap<String, i64>,
    source: &PersistedObservationCurrentSourceV2,
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    sink: &mut S,
) -> Result<PersistedObservationOriginalDataV2, ObservationOriginalPersistenceErrorV2<S::Error>> {
    use ObservationOriginalPersistenceErrorV2::{Sink, Source};
    let result = (|| {
        source
            .validate_capture(parent, tx, common, live)
            .map_err(Source)?;
        let memory = common.memory().clone();
        let phase = memory
            .reserve(
                size_of::<Stream<'_, '_, '_, '_>>() + size_of::<Output<'_, '_, '_>>() + 16 * 1024,
                MemoryClass::Ordinary,
            )
            .map_err(|e| Source(e.into()))?;
        if observation.obligation_ids.len() > 4096 || observation.normal_coverage.len() > 4096 {
            return Err(Source(invalid("V2 observation original obligations")));
        }
        common
            .charge(1, (region.len() + 256) as u64)
            .map_err(|e| Source(e.into()))?;
        for (a, b) in observation
            .obligation_ids
            .iter()
            .zip(observation.normal_coverage.keys())
        {
            common
                .charge(1, a.len().min(b.len()) as u64)
                .map_err(|e| Source(e.into()))?;
        }
        oasis7_world_state::world_rules::validate_current_observation_shape_v2(observation)
            .map_err(|_| Source(invalid("V2 observation original shape")))?;
        if observation.current_commitment != source.source().current_commitment()
            || observation.source_original != source.source_original()
        {
            return Err(Source(invalid("V2 observation original actual cut")));
        }
        let package = Package {
            domain: "oasis7.emergency.observation.original.v2",
            region_id: region,
            observation,
            interval_coverage: intervals,
        };
        let state = RefCell::new(State {
            attempted: false,
            complete: false,
            first: None,
            encoding_error: None,
        });
        let (expected, bytes, finished) = {
            let shared = RefCell::new(&mut *common);
            let mut measured = Output {
                common: &shared,
                state: &state,
                physical: None,
                hash: blake3::Hasher::new(),
                bytes: 0,
            };
            if let Err(error) = serde_json::to_writer(&mut measured, &package) {
                if let Some(first) = state.borrow_mut().first.take() {
                    return Err(Source(first));
                }
                return Err(Source(ObservationCurrentSourceErrorV2::Io(
                    io::Error::other(error),
                )));
            }
            let expected = *measured.hash.finalize().as_bytes();
            let bytes = measured.bytes;
            let mut stream = Stream {
                package: &package,
                memory: &memory,
                common: &shared,
                state: &state,
                expected,
                bytes,
            };
            let mut cancelled = || {
                if state.borrow().first.is_some() {
                    return true;
                }
                if let Err(error) = shared.borrow_mut().consume_steps(0) {
                    state.borrow_mut().first = Some(error.into());
                    return true;
                }
                false
            };
            (
                expected,
                bytes,
                sink.finish_json_original(&mut stream, &mut cancelled),
            )
        };
        let mut state = state.into_inner();
        if let Some(first) = state.first.take() {
            return Err(Source(first));
        }
        let actual = finished.map_err(Sink)?;
        if let Some(error) = state.encoding_error.take() {
            return Err(Source(ObservationCurrentSourceErrorV2::Io(
                io::Error::other(error),
            )));
        }
        if !state.complete || actual != expected || bytes > MAX_ORIGINAL_BYTES {
            return Err(Source(invalid("V2 observation original incomplete finish")));
        }
        source
            .validate_capture(parent, tx, common, live)
            .map_err(Source)?;
        common
            .charge(
                1,
                (64 + size_of::<PersistedObservationOriginalDataV2>()) as u64,
            )
            .map_err(|e| Source(e.into()))?;
        let fee = memory
            .reserve(
                size_of::<PersistedObservationOriginalDataV2>() + 64,
                MemoryClass::Ordinary,
            )
            .map_err(|e| Source(e.into()))?;
        let locator_fee = memory
            .reserve(size_of::<String>() + 64, MemoryClass::Ordinary)
            .map_err(|e| Source(e.into()))?;
        let mut locator = String::with_capacity(64);
        if locator.capacity() > 64 {
            return Err(Source(limited()));
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in expected {
            locator.push(HEX[(byte >> 4) as usize] as char);
            locator.push(HEX[(byte & 15) as usize] as char);
        }
        drop(phase);
        Ok(PersistedObservationOriginalDataV2 {
            id: expected,
            cut_object: *source.object_id(),
            identity: common.work().identity(),
            memory: memory.clone(),
            guard: tx.guard(),
            locator: locator_fee.retain(locator),
            _fee: fee.retain(()),
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
