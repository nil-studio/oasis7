//! Consume complete Current coverage plus the genuine persisted cut, then
//! publish the exact V2 observation original. Arbitrary DTOs cannot enter here.
use super::*;
use crate::world::incremental_normal_sources::BoundNormalTickParentV1;
use crate::world::incremental_normal_world_rule_observation_v2::{
    ObservationCurrentSourceErrorV2, ObservationJsonOriginalSinkV2,
    ObservationOriginalPersistenceErrorV2, PersistedObservationCurrentSourceV2,
    PersistedObservationOriginalDataV2, persist_observation_original_data_v2,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_world_state::world_rules::EmergencyCurrentObservationV2;
use std::collections::BTreeSet;

pub(in crate::world) enum CurrentObservationPublishErrorV2<E> {
    Coverage(IndustrialDeliveryErrorV1),
    Source(ObservationCurrentSourceErrorV2),
    Persistence(ObservationOriginalPersistenceErrorV2<E>),
}
pub(in crate::world) struct PublishedCurrentObservationV2 {
    pub(super) region: String,
    pub(super) observation: EmergencyCurrentObservationV2,
    pub(super) intervals: BTreeMap<String, i64>,
    pub(super) cut: PersistedObservationCurrentSourceV2,
    pub(super) original: PersistedObservationOriginalDataV2,
    // Keep all selected source originals and allocation charges live across
    // the registry CAS and its eventual counted lifecycle event publication.
    pub(super) _services: Vec<ReservedValue<CurrentEmergencyObservationServiceSourcesV1>>,
    pub(super) _fees: Vec<MemoryReservation>,
    pub(super) _fee_slots: MemoryReservation,
}
impl PublishedCurrentObservationV2 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ObservationCurrentSourceErrorV2> {
        self.original
            .validate_source(&self.cut, parent, tx, common, live)
    }
    pub(in crate::world) fn original_id(&self) -> &[u8; 32] {
        self.original.object_id()
    }
    pub(in crate::world) fn observation(&self) -> &EmergencyCurrentObservationV2 {
        &self.observation
    }
    pub(in crate::world) fn region(&self) -> &str {
        &self.region
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn publish_current_observation_v2<
    M: MerkleNodeReader,
    S: ObservationJsonOriginalSinkV2,
>(
    prepared: PreparedCurrentObservationCoverageV2,
    cut: PersistedObservationCurrentSourceV2,
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
    sink: &mut S,
) -> Result<PublishedCurrentObservationV2, CurrentObservationPublishErrorV2<S::Error>> {
    use CurrentObservationPublishErrorV2::{Coverage, Persistence, Source};
    {
        let mut tail = NormalTailWorkV1::new(common);
        prepared
            .validate(tx, &mut tail, now, live)
            .map_err(Coverage)?;
    }
    cut.validate_capture(parent, tx, common, live)
        .map_err(Source)?;
    let PreparedCurrentObservationCoverageV2 {
        region,
        services,
        normal,
        intervals,
        mut _fees,
        _fee_slots,
        ..
    } = prepared;
    // All fallible arithmetic, output-node and String payload charges precede
    // the corresponding clones. The two digest fields borrow actual owners.
    let mut ids = BTreeSet::new();
    for id in normal.keys() {
        common
            .charge(
                32,
                u64::try_from(id.len().checked_mul(32).ok_or_else(|| {
                    Source(ObservationCurrentSourceErrorV2::Invalid(
                        "observation ID layout",
                    ))
                })?)
                .map_err(|_| {
                    Source(ObservationCurrentSourceErrorV2::Invalid(
                        "observation ID address",
                    ))
                })?,
            )
            .map_err(|e| Source(e.into()))?;
        if _fees.len() == _fees.capacity() {
            return Err(Source(ObservationCurrentSourceErrorV2::Invalid(
                "observation fee handle bound",
            )));
        }
        _fees.push(
            common
                .memory()
                .reserve(
                    id.len().checked_add(2048).ok_or_else(|| {
                        Source(ObservationCurrentSourceErrorV2::Invalid(
                            "observation ID layout",
                        ))
                    })?,
                    MemoryClass::Ordinary,
                )
                .map_err(|e| Source(e.into()))?,
        );
        ids.insert(id.clone());
    }
    common.charge(1, 128).map_err(|e| Source(e.into()))?;
    if _fees.len() == _fees.capacity() { return Err(Source(ObservationCurrentSourceErrorV2::Invalid("observation fee handle bound"))); }
    _fees.push(
        common
            .memory()
            .reserve(
                128 + std::mem::size_of::<PublishedCurrentObservationV2>()
                    + std::mem::size_of::<EmergencyCurrentObservationV2>(),
                MemoryClass::Ordinary,
            )
            .map_err(|e| Source(e.into()))?,
    );
    let observation = EmergencyCurrentObservationV2 {
        format: 2,
        window_start: now - 60,
        current_commitment: cut.source().current_commitment().to_owned(),
        source_original: cut.source_original().to_owned(),
        obligation_ids: ids,
        normal_coverage: normal,
    };
    if observation.current_commitment.capacity() > 64 || observation.source_original.capacity() > 64
    {
        return Err(Source(ObservationCurrentSourceErrorV2::Invalid(
            "observation digest allocation",
        )));
    }
    let original = persist_observation_original_data_v2(
        &region,
        &observation,
        &intervals,
        &cut,
        parent,
        tx,
        common,
        live,
        sink,
    )
    .map_err(Persistence)?;
    Ok(PublishedCurrentObservationV2 {
        region,
        observation,
        intervals,
        cut,
        original,
        _services: services,
        _fees,
        _fee_slots,
    })
}
