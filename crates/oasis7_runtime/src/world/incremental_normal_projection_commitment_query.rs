//! Fixed actual-source gameplay query and original Runtime recovery extension.
use super::*;
use crate::world_rules::readonly_recovery::{
    RecoveryAssemblyOpsV1, append_recipe_recovery_data_v1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::gameplay_capability::{GameplayCapabilityQuery, GameplaySubject};
use std::{cell::RefCell, mem::size_of};
#[derive(serde::Serialize)]
struct Evidence<'a, R, P, A> {
    quoted_at_tick: u64,
    rejection: &'a R,
    preview: &'a P,
    authorization: &'a A,
}
struct CurrentRecovery<'f, 'a, 's, 'c, 'w, M> {
    facts: &'f CurrentProjectionReadFactsV1<'a, 's, 'c, M>,
    state: &'f OriginalStateColumnsDataV1,
    electricity: &'f OriginalProjectionRowsDataV1,
    aux: &'f oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    metadata:
        &'f crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<
            'f,
        >,
    raw: &'f super::super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&'f ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    canonical: RefCell<&'w mut CanonicalCborWorkV1>,
    quote_fees: RefCell<Vec<CurrentProjectionFactsFeesV1>>,
    json_fees: RefCell<Vec<MemoryReservation>>,
}
impl<M: MerkleNodeReader> CurrentRecovery<'_, '_, '_, '_, '_, M> {
    fn json<T: serde::Serialize>(&self, value: &T) -> Result<serde_json::Value, BodyMutationError> {
        let raw = super::super::shared_json::capture_shared_v1(
            value,
            &self.facts.common,
            &RefCell::new(None),
            usize::MAX,
        )?;
        let (value, fee) = fact_decode::recover_json_value_parts_data_v1(&raw, &self.facts.common)?;
        let mut fees = self.json_fees.borrow_mut();
        self.facts.before_owned(
            (fees.len() + 1)
                .saturating_mul(size_of::<MemoryReservation>())
                .saturating_mul(2),
        )?;
        if fees.len() == fees.capacity() {
            fees.try_reserve_exact(1)
                .map_err(|_| invalid("query JSON fees allocation"))?
        }
        fees.push(fee);
        Ok(value)
    }
}
impl<M: MerkleNodeReader> RecoveryAssemblyOpsV1 for CurrentRecovery<'_, '_, '_, '_, '_, M> {
    type Error = BodyMutationError;
    fn text(&self, v: &str) -> Result<String, Self::Error> {
        self.facts.before_owned(size_of::<String>() + v.len())?;
        let mut bytes = Vec::with_capacity(v.len());
        if bytes.capacity() != v.len() {
            return Err(invalid("query string capacity"));
        }
        for chunk in v.as_bytes().chunks(8192) {
            self.facts.work(1, chunk.len())?;
            bytes.extend_from_slice(chunk)
        }
        String::from_utf8(bytes).map_err(paid)
    }
    fn before_push(&self, len: usize, capacity: usize) -> Result<(), Self::Error> {
        self.facts.work(1, 0)?;
        if len == capacity {
            self.facts.before_owned(
                capacity.max(4).saturating_mul(2).saturating_mul(size_of::<
                    oasis7_world_state::gameplay_capability::RecoveryAssessment,
                >()),
            )?
        }
        Ok(())
    }
    fn compare(&self, a: &str, b: &str) -> Result<bool, Self::Error> {
        self.facts.work(1, a.len() + b.len())?;
        Ok(a == b)
    }
    fn quote(
        &self,
        owner: &str,
        factory: &str,
        recipe: &str,
    ) -> Result<Option<(bool, serde_json::Value, serde_json::Value)>, Self::Error> {
        let paid = {
            let mut common = self.facts.common.borrow_mut();
            let mut canonical = self.canonical.borrow_mut();
            let mut live = self.facts.live.borrow_mut();
            super::prepare_current_builtin_recipe_quote_data_v1(
                self.state,
                self.electricity,
                self.aux,
                self.metadata,
                self.raw,
                self.binding,
                owner,
                factory,
                recipe,
                1,
                self.facts.tx,
                &mut common,
                &mut canonical,
                &mut **live,
            )?
        };
        let (result, fee) = paid.into_parts_v1();
        {
            let mut fees = self.quote_fees.borrow_mut();
            self.facts.before_owned(
                (fees.len() + 1)
                    .saturating_mul(size_of::<CurrentProjectionFactsFeesV1>())
                    .saturating_mul(2),
            )?;
            if fees.len() == fees.capacity() {
                fees.try_reserve_exact(1)
                    .map_err(|_| invalid("query quote fee slots"))?
            }
            fees.push(fee)
        }
        match result {
            Err(_) => Ok(None),
            Ok(quote) => {
                let rejected = quote.rejection.is_some();
                let action = self.json(&quote.action)?;
                let evidence = self.json(&Evidence {
                    quoted_at_tick: quote.quoted_at_tick,
                    rejection: &quote.rejection,
                    preview: &quote.preview,
                    authorization: &quote.authorization,
                })?;
                Ok(Some((rejected, action, evidence)))
            }
        }
    }
}
pub(in crate::world) struct CurrentGameplayQueryFeesV1 {
    _base: CurrentProjectionFactsFeesV1,
    _quotes: Vec<CurrentProjectionFactsFeesV1>,
    _json: Vec<MemoryReservation>,
}
pub(in crate::world) struct PaidCurrentGameplayQueryDataV1 {
    query: serde_json::Value,
    _fees: CurrentGameplayQueryFeesV1,
}
impl PaidCurrentGameplayQueryDataV1 {
    pub(in crate::world) fn into_parts_v1(self) -> (serde_json::Value, CurrentGameplayQueryFeesV1) {
        (self.query, self._fees)
    }
}
pub(in crate::world) fn prepare_current_gameplay_query_data_v1<M: MerkleNodeReader>(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    aux: &oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    raw: &super::super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    agent: Option<&str>,
    execution_actor: Option<&str>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
    live: &mut dyn FnMut() -> ExecutionFence,
) -> Result<PaidCurrentGameplayQueryDataV1, BodyMutationError> {
    let facts = CurrentProjectionReadFactsV1::for_source_data_v1(
        state,
        electricity,
        metadata,
        tx,
        common,
        live,
    )?;
    let (query, quote_fees, json_fees) = {
        let source = CurrentRecovery {
            facts: &facts,
            state,
            electricity,
            aux,
            metadata,
            raw,
            binding,
            canonical: RefCell::new(canonical),
            quote_fees: RefCell::new(Vec::new()),
            json_fees: RefCell::new(Vec::new()),
        };
        let subject = match agent {
            None => GameplaySubject::Spectator,
            Some(agent) => GameplaySubject::Personal {
                owner_agent_id: source.text(agent)?,
            },
        };
        let mut query =
            oasis7_world_state::gameplay_capability::gameplay_capability_query_readonly_v1(
                &facts,
                subject,
                "current_industrial_capability",
            )?;
        append_recipe_recovery_data_v1(
            &source,
            &mut query,
            "current_industrial_capability",
            execution_actor,
        )?;
        let query = source.json(&query)?;
        (
            query,
            source.quote_fees.into_inner(),
            source.json_fees.into_inner(),
        )
    };
    facts.check()?;
    Ok(PaidCurrentGameplayQueryDataV1 {
        query,
        _fees: CurrentGameplayQueryFeesV1 {
            _base: facts.into_fees(),
            _quotes: quote_fees,
            _json: json_fees,
        },
    })
}
