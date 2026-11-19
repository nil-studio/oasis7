//! The original release-profile policy over sealed real Legacy/Current points.
use super::*;
use crate::world::event_processing::action_to_event_economy::ensure_profile_field_whitelist;
use crate::world::incremental_normal_action_factory_schedule::{
    FactoryScheduleReadErrorV1, with_module_lifecycle_read_io_v1,
};
use crate::world::module_actions::MODULE_RELEASE_PROFILE_CHANGE_LIMIT;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueReadContext};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use std::collections::BTreeSet;
enum ProfileError<E> {
    Note(String),
    Source(E),
}
fn note<E>(value: String) -> ProfileError<E> {
    ProfileError::Note(value)
}
mod sealed {
    pub trait Sealed {}
}
trait Facts: sealed::Sealed {
    type Error;
    fn reserve<T: serde::Serialize>(&mut self, value: &T) -> Result<(), Self::Error>;
    fn product(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn recipe(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn factory(&mut self, id: &str) -> Result<bool, Self::Error>;
}
fn validate<S: Facts>(
    source: &mut S,
    changes: &crate::ModuleProfileChanges,
) -> Result<(), ProfileError<S::Error>> {
    if changes.is_empty() {
        return Ok(());
    }

    let total_changes = changes
        .product_profiles
        .len()
        .saturating_add(changes.recipe_profiles.len())
        .saturating_add(changes.factory_profiles.len());
    if total_changes > MODULE_RELEASE_PROFILE_CHANGE_LIMIT {
        return Err(note(format!(
            "profile changes exceed limit {} (got {})",
            MODULE_RELEASE_PROFILE_CHANGE_LIMIT, total_changes
        )));
    }

    let product_fields = [
        "product_id",
        "role_tag",
        "maintenance_sink",
        "tradable",
        "unlock_stage",
    ];
    let recipe_fields = [
        "recipe_id",
        "bottleneck_tags",
        "stage_gate",
        "preferred_factory_tags",
        "substitution_rules",
    ];
    let factory_fields = ["factory_id", "tier", "recipe_slots", "tags"];

    let mut product_ids = BTreeSet::new();
    for profile in &changes.product_profiles {
        source.reserve(profile).map_err(ProfileError::Source)?;
        if profile.product_id.trim().is_empty() {
            return Err(note(
                "product profile product_id cannot be empty".to_string(),
            ));
        }
        if profile.role_tag.trim().is_empty() {
            return Err(note(format!(
                "product profile role_tag cannot be empty: {}",
                profile.product_id
            )));
        }
        ensure_profile_field_whitelist(profile, product_fields.as_slice(), "product profile")
            .map_err(note)?;
        if !product_ids.insert(profile.product_id.clone()) {
            return Err(note(format!(
                "duplicate product profile_id {}",
                profile.product_id
            )));
        }
        if source
            .product(profile.product_id.as_str())
            .map_err(ProfileError::Source)?
        {
            return Err(note(format!(
                "product profile_id already exists in state {} (module release overwrite is forbidden)",
                profile.product_id
            )));
        }
    }

    let mut recipe_ids = BTreeSet::new();
    for profile in &changes.recipe_profiles {
        source.reserve(profile).map_err(ProfileError::Source)?;
        profile
            .validate_substitution_rules()
            .map_err(|error| note(format!("invalid recipe substitution profile: {error}")))?;
        if profile.recipe_id.trim().is_empty() {
            return Err(note("recipe profile recipe_id cannot be empty".to_string()));
        }
        ensure_profile_field_whitelist(profile, recipe_fields.as_slice(), "recipe profile")
            .map_err(note)?;
        if !recipe_ids.insert(profile.recipe_id.clone()) {
            return Err(note(format!(
                "duplicate recipe profile_id {}",
                profile.recipe_id
            )));
        }
        if source
            .recipe(profile.recipe_id.as_str())
            .map_err(ProfileError::Source)?
        {
            return Err(note(format!(
                "recipe profile_id already exists in state {} (module release overwrite is forbidden)",
                profile.recipe_id
            )));
        }
    }

    let mut factory_ids = BTreeSet::new();
    for profile in &changes.factory_profiles {
        source.reserve(profile).map_err(ProfileError::Source)?;
        if profile.factory_id.trim().is_empty() {
            return Err(note(
                "factory profile factory_id cannot be empty".to_string(),
            ));
        }
        if profile.tier == 0 {
            return Err(note(format!(
                "factory profile tier must be >= 1: {}",
                profile.factory_id
            )));
        }
        if profile.recipe_slots == 0 {
            return Err(note(format!(
                "factory profile recipe_slots must be > 0: {}",
                profile.factory_id
            )));
        }
        ensure_profile_field_whitelist(profile, factory_fields.as_slice(), "factory profile")
            .map_err(note)?;
        if !factory_ids.insert(profile.factory_id.clone()) {
            return Err(note(format!(
                "duplicate factory profile_id {}",
                profile.factory_id
            )));
        }
        if source
            .factory(profile.factory_id.as_str())
            .map_err(ProfileError::Source)?
        {
            return Err(note(format!(
                "factory profile_id already exists in state {} (module release overwrite is forbidden)",
                profile.factory_id
            )));
        }
    }

    Ok(())
}
struct Legacy<'a>(&'a crate::World);
impl sealed::Sealed for Legacy<'_> {}
impl Facts for Legacy<'_> {
    type Error = std::convert::Infallible;
    fn reserve<T: serde::Serialize>(&mut self, _: &T) -> Result<(), Self::Error> {
        Ok(())
    }
    fn product(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.0.state.product_profiles.contains_key(id))
    }
    fn recipe(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.0.state.recipe_profiles.contains_key(id))
    }
    fn factory(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.0.state.factory_profiles.contains_key(id))
    }
}
pub(in crate::world) fn validate_legacy_release_profiles_v1(
    world: &crate::World,
    changes: &crate::ModuleProfileChanges,
) -> Result<(), String> {
    match validate(&mut Legacy(world), changes) {
        Ok(()) => Ok(()),
        Err(ProfileError::Note(note)) => Err(note),
        Err(ProfileError::Source(never)) => match never {},
    }
}

struct Current<'a, 's, 't, 'v, 'm, 'p, 'b, 'mw, M, P, R, E, C, PE, PC> {
    sources: &'a ModuleLifecycleSourcesV1<'s>,
    ctx: &'a mut WorldFieldOverlayContextV1<'t, 'v, 'm, M, R, E, C>,
    parent: &'a mut WorldFieldReadContextV1<'p, P, R, PE, PC>,
    factory: &'a mut FactoryScheduleWorkV1,
    module: &'a mut ModuleLifecycleWorkV1<'mw>,
    common: &'a mut FormalBoundedContextV1<'b>,
    live: &'a ExecutionFence,
    fees: Vec<MemoryReservation>,
    slots: MemoryReservation,
}
impl<M, P, R, E, C, PE, PC> sealed::Sealed
    for Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
{
}
impl<M, P, R, E, C, PE, PC> Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn hold(&mut self, fee: MemoryReservation) -> Result<(), BodyMutationError> {
        if self.fees.len() == self.fees.capacity() {
            let cap = self.fees.capacity().checked_mul(2).ok_or_else(invalid)?;
            let bytes = cap
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .ok_or_else(invalid)?;
            pay(self.common, bytes)?;
            let next = self
                .common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            self.fees.reserve_exact(cap - self.fees.len());
            if self.fees.capacity() > cap {
                return Err(invalid());
            }
            self.slots = next;
        }
        self.fees.push(fee);
        Ok(())
    }
}
impl<M, P, R, E, C, PE, PC> Facts for Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = BodyMutationError;
    fn reserve<T: serde::Serialize>(&mut self, value: &T) -> Result<(), BodyMutationError> {
        struct Count<'a, 'b> {
            n: usize,
            common: &'a mut FormalBoundedContextV1<'b>,
            first: Option<BodyMutationError>,
        }
        impl std::io::Write for Count<'_, '_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if let Err(error) = self.common.charge(1, bytes.len() as u64) {
                    self.first = Some(BodyMutationError::Original(Box::new(error)));
                    return Err(std::io::Error::other("profile serialization budget"));
                }
                self.n = self
                    .n
                    .checked_add(bytes.len())
                    .ok_or_else(|| std::io::Error::other("profile size"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count {
            n: 0,
            common: self.common,
            first: None,
        };
        let result = serde_json::to_writer(&mut count, value);
        if let Some(error) = count.first {
            return Err(error);
        }
        result.map_err(|_| invalid())?;
        let bytes = count
            .n
            .checked_mul(128)
            .and_then(|n| n.checked_add(65536))
            .ok_or_else(invalid)?;
        pay(self.common, bytes)?;
        let fee = self
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.hold(fee)
    }
    fn product(&mut self, id: &str) -> Result<bool, BodyMutationError> {
        current_point(self, id, Point::Product)
    }
    fn recipe(&mut self, id: &str) -> Result<bool, BodyMutationError> {
        current_point(self, id, Point::Recipe)
    }
    fn factory(&mut self, id: &str) -> Result<bool, BodyMutationError> {
        current_point(self, id, Point::Factory)
    }
}
enum Point {
    Product,
    Recipe,
    Factory,
}
fn current_point<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    source: &mut Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>,
    id: &str,
    point: Point,
) -> Result<bool, BodyMutationError> {
    let sources = source.sources;
    let live = source.live;
    let result = with_module_lifecycle_read_io_v1(
        sources.families.roots,
        source.ctx,
        source.parent,
        source.factory,
        source.common,
        source.module,
        |ctx, parent, factory, _| {
            let mut read = WorldFieldReadContextV1 {
                root: sources.families.roots.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                ),
                state: parent.state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let guard = ctx.transaction.guard();
            use oasis7_world_state::incremental_world_native::*;
            let present = match point {
                Point::Product => read_current_governed_product_profile_v1(
                    id,
                    sources.factory.lifecycle.native,
                    sources.families.roots,
                    ctx.transaction,
                    &mut read,
                    &mut factory.lifecycle.native_scan,
                    &mut factory.lifecycle.native_read,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?
                .value()
                .is_some(),
                Point::Recipe => read_current_governed_recipe_profile_v1(
                    id,
                    sources.factory.lifecycle.native,
                    sources.families.roots,
                    ctx.transaction,
                    &mut read,
                    &mut factory.lifecycle.native_scan,
                    &mut factory.lifecycle.native_read,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?
                .value()
                .is_some(),
                Point::Factory => read_current_governed_factory_profile_v1(
                    id,
                    sources.factory.lifecycle.native,
                    sources.families.roots,
                    ctx.transaction,
                    &mut read,
                    &mut factory.lifecycle.native_scan,
                    &mut factory.lifecycle.native_read,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?
                .value()
                .is_some(),
            };
            Ok(present)
        },
    );
    match result {
        Ok(value) => Ok(value),
        Err(FactoryScheduleReadErrorV1::Body(error)) => Err(error),
        Err(FactoryScheduleReadErrorV1::World(_)) => Err(invalid()),
    }
}
/// Keeps the actual validation/error graph fees until the consumer transfers or
/// finishes its request/plan/rejection owner. This grants no release permission.
pub(in crate::world) struct PaidCurrentReleaseProfilesV1 {
    result: Result<(), String>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: oasis7_proto::memory_budget::MemoryBudget,
    fees: Vec<MemoryReservation>,
    slots: MemoryReservation,
    _layout: MemoryReservation,
}
impl PaidCurrentReleaseProfilesV1 {
    pub(in crate::world) fn result(&self) -> Result<(), &str> {
        self.result.as_ref().map(|_| ()).map_err(String::as_str)
    }
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core
            || !self.memory.same_pool(memory)
            || !self._layout.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn validate_current_release_profiles_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    changes: &crate::ModuleProfileChanges,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentReleaseProfilesV1, BodyMutationError> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(&tx.guard(), live)?;
        if !ctx.values.memory.same_pool(common.memory()) {
            return Err(invalid());
        }
        let bytes = std::mem::size_of::<PaidCurrentReleaseProfilesV1>() + 4096;
        pay(common, bytes)?;
        let layout = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let slots = common
            .memory()
            .reserve(
                32 * std::mem::size_of::<MemoryReservation>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let mut source = Current {
            sources,
            ctx: &mut overlay,
            parent,
            factory,
            module,
            common,
            live,
            fees: Vec::with_capacity(32),
            slots,
        };
        let result = match validate(&mut source, changes) {
            Ok(()) => Ok(()),
            Err(ProfileError::Note(note)) => Err(note),
            Err(ProfileError::Source(error)) => return Err(error),
        };
        source
            .ctx
            .transaction
            .validate_guard(&source.ctx.transaction.guard(), live)?;
        Ok(PaidCurrentReleaseProfilesV1 {
            result,
            guard: source.ctx.transaction.guard(),
            core: source.common.work().identity(),
            memory: source.common.memory().clone(),
            fees: source.fees,
            slots: source.slots,
            _layout: layout,
        })
    })
}
