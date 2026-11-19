//! Dedicated plain Meta DTO mutation. Child of the original typed codec.
//! No generic detach/mutable-T API and no action qualification is introduced.
use super::*;
use crate::MetaProgressState;
use crate::incremental_world_native::WorldNativeRecordV1;
/// Caller-owned continuous work, shared across preparation and source rechecks.
pub struct MetaProgressMutationWorkV1 {
    pub steps_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub max_entries: usize,
    pub max_text_bytes: usize,
}
impl MetaProgressMutationWorkV1 {
    pub(crate) fn visit(
        &mut self,
        bytes: usize,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("meta mutation steps"))?;
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
            .ok_or(ValueTreeError::ResourceLimited("meta comparison bytes"))?;
        Ok(())
    }
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("meta owned layout")
}
fn add(total: &mut usize, n: usize) -> Result<(), ValueTreeError> {
    *total = total.checked_add(n).ok_or_else(limited)?;
    Ok(())
}
fn mul(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_mul(b).ok_or_else(limited)
}
fn vector_peak(len: usize, capacity: usize, extra: usize) -> Result<usize, ValueTreeError> {
    // Rust Vec amortized growth is bounded by max(old capacity, 2*required,
    // minimum nonzero capacity). Two such buffers cover the transient old+new
    // allocation during any intermediate realloc (including successive pushes),
    // while decoder charges remain retained for the original allocation.
    let required = len.checked_add(extra).ok_or_else(limited)?;
    mul(
        mul(capacity.max(mul(required, 2)?).max(8), 2)?,
        std::mem::size_of::<String>(),
    )
}
enum MetaValue {
    Existing(RecoveredTypedRecordV1<WorldNativeRecordV1<MetaProgressState>>),
    New(MetaProgressState),
}
/// Owns both original decoder allocations and all charged mutation growth.
pub(crate) struct PreparedMetaProgressValueV1 {
    data: MetaValue,
    _growth: MemoryReservation,
}
impl PreparedMetaProgressValueV1 {
    pub(crate) fn value(&self) -> &MetaProgressState {
        match &self.data {
            MetaValue::Existing(v) => v.value().value(),
            MetaValue::New(v) => v,
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_meta_progress_owned_v1(
    record: Option<RecoveredTypedRecordV1<WorldNativeRecordV1<MetaProgressState>>>,
    target: &str,
    track: &str,
    points: i64,
    achievement: Option<&str>,
    now: u64,
    memory: &MemoryBudget,
    work: &mut MetaProgressMutationWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedMetaProgressValueV1, ValueTreeError> {
    work.visit(0, &mut cancelled)?;
    for text in [Some(target), Some(track), achievement]
        .into_iter()
        .flatten()
    {
        if text.len() > work.max_text_bytes {
            return Err(limited());
        }
        work.visit(text.len(), &mut cancelled)?;
    }
    // The same finite text cap applies to every persisted string, including
    // generated achievements. Check the actual next score before any mutation:
    // a record written at this cap must remain readable under the same cap.
    let next = if let Some(source) = &record {
        let progress = source.value().value();
        for key in progress.track_points.keys() {
            work.visit(key.len().max(track.len()), &mut cancelled)?;
        }
        progress
            .track_points
            .get(track)
            .copied()
            .unwrap_or(0)
            .saturating_add(points)
    } else {
        points
    };
    for (tier, threshold) in [("bronze", 20), ("silver", 50), ("gold", 100)] {
        work.visit(0, &mut cancelled)?;
        if next >= threshold {
            let generated = track
                .len()
                .checked_add(tier.len())
                .and_then(|n| n.checked_add(6))
                .ok_or_else(limited)?;
            if generated > work.max_text_bytes {
                return Err(ValueTreeError::ResourceLimited(
                    "meta generated identifier bytes",
                ));
            }
        }
    }
    let mut bytes = std::mem::size_of::<PreparedMetaProgressValueV1>();
    let mut entries = 0usize;
    let max_entries = work.max_entries;
    let mut count = |n: usize| -> Result<(), ValueTreeError> {
        entries = entries.checked_add(n).ok_or_else(limited)?;
        if entries > max_entries {
            return Err(limited());
        }
        Ok(())
    };
    if let Some(source) = &record {
        let value = source.value().value();
        work.visit(value.agent_id.len(), &mut cancelled)?;
        if value.agent_id.len() > work.max_text_bytes {
            return Err(limited());
        }
        count(value.track_points.len())?;
        count(value.unlocked_tiers.len())?;
        count(value.achievements.len())?;
        for key in value.track_points.keys() {
            if key.len() > work.max_text_bytes {
                return Err(limited());
            }
            work.visit(key.len(), &mut cancelled)?;
        }
        for (key, tiers) in &value.unlocked_tiers {
            if key.len() > work.max_text_bytes {
                return Err(limited());
            }
            work.visit(key.len(), &mut cancelled)?;
            count(tiers.len())?;
            for tier in tiers {
                if tier.len() > work.max_text_bytes {
                    return Err(limited());
                }
                work.visit(tier.len(), &mut cancelled)?;
            }
        }
        for id in &value.achievements {
            if id.len() > work.max_text_bytes {
                return Err(limited());
            }
            work.visit(id.len(), &mut cancelled)?;
        }
        // Same closed BTreeMap output model as incremental_typed_record_values:
        // per entry sizeof(K)+sizeof(V)+128. Charge the whole post-insert map,
        // rather than assuming a single insertion cannot split ancestor nodes.
        add(
            &mut bytes,
            mul(
                value
                    .track_points
                    .len()
                    .checked_add(1)
                    .ok_or_else(limited)?,
                std::mem::size_of::<String>() + std::mem::size_of::<i64>() + 128,
            )?,
        )?;
        add(
            &mut bytes,
            mul(
                value
                    .unlocked_tiers
                    .len()
                    .checked_add(1)
                    .ok_or_else(limited)?,
                std::mem::size_of::<String>() + std::mem::size_of::<Vec<String>>() + 128,
            )?,
        )?;
        add(
            &mut bytes,
            vector_peak(value.achievements.len(), value.achievements.capacity(), 4)?,
        )?;
        let tiers = value.unlocked_tiers.get(track);
        add(
            &mut bytes,
            vector_peak(tiers.map_or(0, Vec::len), tiers.map_or(0, Vec::capacity), 3)?,
        )?;
    } else {
        add(&mut bytes, target.len())?;
        add(
            &mut bytes,
            std::mem::size_of::<String>() + std::mem::size_of::<i64>() + 128,
        )?;
        add(
            &mut bytes,
            std::mem::size_of::<String>() + std::mem::size_of::<Vec<String>>() + 128,
        )?;
        add(&mut bytes, vector_peak(0, 0, 4)?)?;
        add(&mut bytes, vector_peak(0, 0, 3)?)?;
    }
    count(9)?;
    // One track insertion plus up to three occupied-or-new tier map entries;
    // each entry() argument owns its temporary String even on an occupied key.
    add(&mut bytes, mul(track.len(), 4)?)?;
    add(&mut bytes, achievement.map_or(0, str::len))?;
    for tier in ["bronze", "silver", "gold"] {
        let len = track
            .len()
            .checked_add(tier.len())
            .and_then(|n| n.checked_add(6))
            .ok_or_else(limited)?;
        // format!("tier.{track}.{tier}") has six literal bytes: fmt starts
        // with at most 12 bytes, then String grows geometrically. Pay both
        // live buffers through realloc plus the resulting owned String capacity.
        add(&mut bytes, mul(mul(len, 2)?.max(12), 2)?)?;
        add(&mut bytes, tier.len())?;
    }
    let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
    work.visit(0, &mut cancelled)?;
    let mut data = match record {
        Some(value) => MetaValue::Existing(value),
        None => MetaValue::New(MetaProgressState {
            agent_id: target.to_owned(),
            track_points: BTreeMap::new(),
            total_points: 0,
            achievements: Vec::new(),
            unlocked_tiers: BTreeMap::new(),
            last_granted_at: now,
        }),
    };
    match &mut data {
        MetaValue::Existing(v) => {
            v.value
                .apply_prepaid_meta_progress_v1(track, points, achievement, now, |n| {
                    work.visit(n, &mut cancelled)
                })?
        }
        MetaValue::New(v) => {
            crate::world::governance_meta_publication::apply_meta_progress_delta_v1(
                v,
                track,
                points,
                achievement,
                now,
                |n| work.visit(n, &mut cancelled),
            )?
        }
    }
    work.visit(0, &mut cancelled)?;
    Ok(PreparedMetaProgressValueV1 { data, _growth: fee })
}
