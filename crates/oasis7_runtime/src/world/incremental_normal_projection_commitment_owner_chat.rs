//! Fixed actual-column canonical owner chat consumer. All result records pass
//! the original complete SDK and retained index checks; reader ownership still
//! comes from the original current identity/owner/namespace predicates.
use super::aux_facts::CurrentProjectionAuxFactsV1;
use super::*;
use crate::service_admission::{
    CanonicalIntentResultV1,
    agent_chat::readonly_rules::{
        self as rules, CanonicalOwnerChatReadonlyFactsV1, ChatProtocolDataV1,
    },
};
use crate::world::incremental_normal_projection_retained_reader as retained;
use crate::world::industrial_control::readonly_runtime_owner::{
    current_agent_identity_readonly_v1, owner_public_key_readonly_v1,
};
use oasis7_agent_api::RuntimeBindingV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::PaidCborDataV1;
use oasis7_proto::{
    player_auth::{PaidVerifiedPlayerAuthDataV1, PlayerAuthWorkErrorV1, VerifiedPlayerAuth},
    viewer::{
        AgentChatRequest, CanonicalAgentChatAuthorityV1, CanonicalAgentChatViewV1,
        CanonicalAgentOwnerFenceV1, PlayerAuthProof,
    },
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::{borrow::Borrow, collections::BTreeMap, mem::size_of, sync::Arc};

impl<'a, 'c> CurrentProjectionReadFactsV1<'a, 'c> {
    pub(super) fn with_agent_intents_data_v1(
        mut self,
        source: &'a OriginalProjectionRowsDataV1,
    ) -> Result<Self, BodyMutationError> {
        self.check()?;
        source.validate(&mut self.common.borrow_mut())?;
        self.agent_intents = Some(source);
        Ok(self)
    }
    pub(super) fn with_agent_intent_data_v1<T>(
        &self,
        agent: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AgentIntentV2>) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        self.check()?;
        let rows = self
            .agent_intents
            .ok_or_else(|| invalid("actual projection Agent intent source missing"))?;
        let key = self.key(agent)?;
        let row = {
            let mut common = self.common.borrow_mut();
            rows.row_data_v1(&key, &mut common)?
        };
        let present = self.owner_agent_present_data(agent)?;
        let result = match row {
            None if present => return Err(invalid("actual Agent intent cohort row missing")),
            Some(_) if !present => {
                return Err(invalid("Agent intent cohort contains absent Agent"));
            }
            None => visit(None),
            Some(row) => fact_decode::with_optional_agent_intent_data_v1(
                self.raw(row)?,
                &self.common,
                |value| visit(value.as_ref()),
            ),
        }?;
        self.check()?;
        Ok(result)
    }
}

enum ResultValue {
    Hot(PaidCborDataV1<CanonicalIntentResultV1>),
    Cold(retained::PaidDecodedRetainedServiceDataV1),
}
struct ResultData(Arc<ResultValue>);
impl Borrow<CanonicalIntentResultV1> for ResultData {
    fn borrow(&self) -> &CanonicalIntentResultV1 {
        match &*self.0 {
            ResultValue::Hot(v) => v.borrow(),
            ResultValue::Cold(v) => v.result(),
        }
    }
}
pub(super) struct CurrentVerifiedPlayerDataV1(PaidVerifiedPlayerAuthDataV1<MemoryReservation>);
impl Borrow<VerifiedPlayerAuth> for CurrentVerifiedPlayerDataV1 {
    fn borrow(&self) -> &VerifiedPlayerAuth {
        self.0.verified()
    }
}
struct CurrentChatFactsV1<'f, 'a, 'c, 'b, 'r, 'w, R> {
    facts: &'f CurrentProjectionReadFactsV1<'a, 'c>,
    aux: CurrentProjectionAuxFactsV1<'f, 'a, 'c>,
    binding: Option<&'b ReservedValue<RuntimeBindingV1>>,
    reader: RefCell<&'r mut R>,
    canonical: RefCell<&'w mut CanonicalCborWorkV1>,
}
impl<R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>>
    CurrentChatFactsV1<'_, '_, '_, '_, '_, '_, R>
{
    fn meter<T>(
        &self,
        run: impl FnOnce(
            &crate::world::normal_bootstrap_current::hash::BootstrapCanonicalMeterV1<'_, '_, '_, '_>,
        ) -> T,
    ) -> Result<T, BodyMutationError> {
        let mut common = self.facts.common.borrow_mut();
        let mut canonical = self.canonical.borrow_mut();
        crate::world::normal_bootstrap_current::hash::with_current_meter_v1(
            &mut common,
            &mut canonical,
            run,
        )
    }
    fn text(&self, v: &str) -> Result<String, BodyMutationError> {
        self.facts.before_owned(size_of::<String>() + v.len())?;
        let mut out = Vec::with_capacity(v.len());
        if out.capacity() != v.len() {
            return Err(invalid("owner chat exact text capacity"));
        }
        for part in v.as_bytes().chunks(8192) {
            self.facts.work(1, part.len())?;
            out.extend_from_slice(part)
        }
        String::from_utf8(out).map_err(paid)
    }
    fn map_work<V>(&self, map: &BTreeMap<String, V>, key: &str) -> Result<(), BodyMutationError> {
        let levels = (map.len() + 1).ilog2() as usize + 2;
        let comparisons = 11usize
            .checked_mul(levels)
            .ok_or_else(|| invalid("owner chat tree work layout"))?;
        self.facts.work(
            comparisons as u64,
            key.len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(comparisons))
                .ok_or_else(|| invalid("owner chat tree byte work"))?,
        )
    }
    fn insert<V>(
        &self,
        map: &mut BTreeMap<String, V>,
        key: String,
        value: V,
    ) -> Result<(), BodyMutationError> {
        self.map_work(map, &key)?;
        let levels = (map.len() + 1).ilog2() as usize + 2;
        let n = size_of::<(String, V)>()
            .checked_mul(32)
            .and_then(|n| n.checked_add(1024))
            .and_then(|n| n.checked_mul(levels))
            .ok_or_else(|| invalid("owner chat map slots layout"))?;
        self.facts.before_owned(n)?;
        map.insert(key, value);
        Ok(())
    }
    fn result(&self, value: ResultValue) -> Result<ResultData, BodyMutationError> {
        self.facts
            .before_owned(size_of::<ResultValue>() + 2 * size_of::<usize>())?;
        Ok(ResultData(Arc::new(value)))
    }
    fn digest(
        &self,
        value: &crate::service_admission::RequestKey,
    ) -> Result<String, BodyMutationError> {
        let digest = self
            .meter(|meter| {
                oasis7_client_api::world_service::request_digest_with_work_v1(
                    "request-key",
                    value,
                    meter,
                )
            })?
            .map_err(paid)?;
        self.text(digest.as_str())
    }
    fn copy_binding(&self, v: &RuntimeBindingV1) -> Result<RuntimeBindingV1, BodyMutationError> {
        use oasis7_agent_api::Digest32;
        self.facts.before_owned(size_of::<RuntimeBindingV1>())?;
        Ok(RuntimeBindingV1 {
            world_id: self.text(&v.world_id)?,
            branch_id: self.text(&v.branch_id)?,
            finality_epoch: v.finality_epoch,
            finality_block_hash: v
                .finality_block_hash
                .as_ref()
                .map(|d| self.text(&d.0).map(Digest32))
                .transpose()?,
            finality_status: self.text(&v.finality_status)?,
            base_tick: v.base_tick,
            base_world_hash: Digest32(self.text(&v.base_world_hash.0)?),
            reorg_epoch: v.reorg_epoch,
            runtime_manifest_hash: Digest32(self.text(&v.runtime_manifest_hash.0)?),
        })
    }
    fn owner_fence(
        &self,
        view: &CanonicalAgentChatViewV1,
    ) -> Result<CanonicalAgentOwnerFenceV1, BodyMutationError> {
        self.facts
            .before_owned(size_of::<CanonicalAgentOwnerFenceV1>())?;
        Ok(CanonicalAgentOwnerFenceV1 {
            agent_id: self.text(&view.agent_id)?,
            player_id: self.text(&view.player_id)?,
            public_key: self.text(&view.public_key)?,
            world_id: self.text(&view.world_id)?,
            reorg_epoch: view.reorg_epoch,
            canonical_authority: CanonicalAgentChatAuthorityV1 {
                branch_id: self.text(&view.canonical_authority.branch_id)?,
                agent_identity_generation: view.canonical_authority.agent_identity_generation,
            },
            current_intent_id: view
                .current_intent_id
                .as_ref()
                .map(|v| self.text(v))
                .transpose()?,
        })
    }
}
impl<R> rules::sealed::Sealed for CurrentChatFactsV1<'_, '_, '_, '_, '_, '_, R> {}
impl<R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>>
    CanonicalOwnerChatReadonlyFactsV1 for CurrentChatFactsV1<'_, '_, '_, '_, '_, '_, R>
{
    type Error = BodyMutationError;
    type Charge = MemoryReservation;
    type Verified = CurrentVerifiedPlayerDataV1;
    fn chat_work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        self.facts.work(s, b)
    }
    fn chat_before_owned(&self, n: usize) -> Result<(), Self::Error> {
        self.facts.before_owned(n)
    }
    fn chat_refusal(
        &self,
        note: &'static str,
    ) -> Result<ChatProtocolDataV1<Self::Charge>, Self::Error> {
        let mut common = self.facts.common.borrow_mut();
        let n = size_of::<ChatProtocolDataV1<MemoryReservation>>() + note.len();
        common.charge(1, n as u64).map_err(paid)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(ChatProtocolDataV1::from_admitted_data_v1(
            note.to_owned(),
            Some(fee),
        ))
    }
    fn chat_identity(
        &self,
        id: &str,
    ) -> Result<Option<crate::CapabilityAgentIdentity>, Self::Error> {
        current_agent_identity_readonly_v1(&self.aux, id)
    }
    fn chat_owner_key(
        &self,
        id: &str,
    ) -> Result<Result<String, ChatProtocolDataV1<Self::Charge>>, Self::Error> {
        match owner_public_key_readonly_v1(&self.aux, id)? {
            Ok(value) => Ok(Ok(value)),
            Err(note) => Ok(Err(self.chat_refusal(note)?)),
        }
    }
    fn chat_binding(&self) -> Result<RuntimeBindingV1, Self::Error> {
        self.copy_binding(
            self.binding
                .ok_or_else(|| invalid("actual enabled owner chat runtime binding missing"))?,
        )
    }
    fn chat_with_intent<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AgentIntentV2>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.facts.with_agent_intent_data_v1(id, visit)
    }
    fn chat_verify(
        &self,
        request: &AgentChatRequest,
        proof: &PlayerAuthProof,
    ) -> Result<Result<Self::Verified, ChatProtocolDataV1<Self::Charge>>, Self::Error> {
        match self.meter(|meter| {
            oasis7_proto::player_auth::verify_agent_chat_auth_proof_with_work_v1(
                request, proof, meter,
            )
        })? {
            Ok(value) => Ok(Ok(CurrentVerifiedPlayerDataV1(value))),
            Err(PlayerAuthWorkErrorV1::Protocol(refusal)) => {
                Ok(Err(ChatProtocolDataV1::from_player_refusal_v1(refusal)))
            }
            Err(PlayerAuthWorkErrorV1::Work(error)) => Err(paid(error)),
        }
    }
    fn chat_visit_goal_results(
        &self,
        agent: &str,
        intent: &str,
        mut visit: impl FnMut(&CanonicalIntentResultV1) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.facts.check()?;
        self.facts
            .before_owned(size_of::<BTreeMap<String, ResultData>>())?;
        let mut results = BTreeMap::new();
        {
            let mut canonical = self.canonical.borrow_mut();
            self.aux
                .visit_hot_service_results_data_v1(&mut canonical, |key, value| {
                    let value = self.result(ResultValue::Hot(value))?;
                    self.insert(&mut results, self.text(key)?, value)?;
                    Ok(ControlFlow::Continue(()))
                })?;
        }
        let root = self.facts.retained_root_data()?;
        if let Some(root) = root.as_ref() {
            let identity = match self.chat_identity(agent)? {
                Some(value) => value,
                None => {
                    return Err(paid(
                        self.chat_refusal("retained chat owner identity unavailable")?,
                    ));
                }
            };
            let binding = self.chat_binding()?;
            let key = self.chat_owner_key(agent)?.map_err(paid)?;
            let digest = {
                let mut common = self.facts.common.borrow_mut();
                let mut canonical = self.canonical.borrow_mut();
                retained::canonical_task_owner_digest_data_v1(&key, &mut common, &mut canonical)?
            };
            self.facts.before_owned(size_of::<
                oasis7_world_state::formal_retained_history::AgentChatIntentLookupV1,
            >())?;
            let lookup = oasis7_world_state::formal_retained_history::AgentChatIntentLookupV1 {
                agent_id: self.text(agent)?,
                intent_id: self.text(intent)?,
                owner_public_key_digest: self.text(digest.as_str())?,
                player_id: self.text(&identity.owner_binding)?,
                world_id: binding.world_id,
                branch_id: binding.branch_id,
                identity_generation: identity.generation,
                reorg_epoch: binding.reorg_epoch,
                authority_scope: self.text("player_agent_chat")?,
            };
            let cold = {
                let mut common = self.facts.common.borrow_mut();
                let mut canonical = self.canonical.borrow_mut();
                let mut reader = self.reader.borrow_mut();
                retained::read_chat_results_data_v1(
                    root,
                    &lookup,
                    &mut **reader,
                    &mut common,
                    &mut canonical,
                )?
            };
            for value in cold.into_values_data_v1() {
                self.facts.work(1, 0)?;
                let value = self.result(ResultValue::Cold(value))?;
                let key = self.digest(
                    &Borrow::<CanonicalIntentResultV1>::borrow(&value)
                        .request
                        .correlation
                        .key,
                )?;
                self.map_work(&results, &key)?;
                if let Some(previous) = results.get(&key) {
                    let equal = {
                        let mut common = self.facts.common.borrow_mut();
                        let mut canonical = self.canonical.borrow_mut();
                        retained::equal_canonical_service_result_data_v1(
                            previous.borrow(),
                            value.borrow(),
                            &mut common,
                            &mut canonical,
                        )?
                    };
                    if !equal {
                        return Err(paid(
                            self.chat_refusal("hot/cold canonical result conflict")?,
                        ));
                    }
                } else {
                    self.insert(&mut results, key, value)?;
                }
            }
        }
        for value in results.into_values() {
            self.facts.work(1, 0)?;
            if visit(value.borrow())?.is_break() {
                break;
            }
        }
        self.facts.check()?;
        Ok(())
    }
}
pub(in crate::world) struct PaidCurrentOwnerChatProjectionDataV1 {
    owner: Option<CanonicalAgentOwnerFenceV1>,
    view: Option<CanonicalAgentChatViewV1>,
    _fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentOwnerChatProjectionDataV1 {
    pub(in crate::world) fn into_parts_v1(
        self,
    ) -> (
        Option<CanonicalAgentOwnerFenceV1>,
        Option<CanonicalAgentChatViewV1>,
        CurrentProjectionFactsFeesV1,
    ) {
        (self.owner, self.view, self._fees)
    }
}
/// Actual-source owner/fence data. The authenticated formatter performs the
/// original owner-key check before emitting the private chat component.
pub(in crate::world) fn prepare_current_owner_chat_projection_data_v1<
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    intents: &OriginalProjectionRowsDataV1,
    auxiliary:&oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    raw_state: &super::super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<RuntimeBindingV1>>,
    agent: &str,
    retained: &mut R,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<PaidCurrentOwnerChatProjectionDataV1, BodyMutationError> {
    origin.validate_metadata(metadata, common)?;
    raw_state.validate_context(common)?;
    let expected = metadata.runtime_binding_for_state_hash_bounded_v1(
        raw_state.as_str(),
        common,
        canonical,
    )?;
    match (expected.as_ref(), binding) {
        (Some(expected), Some(actual)) => {
            if !actual.uses_budget(common.memory()) {
                return Err(invalid("owner chat binding pool"));
            }
            for value in [&**expected, &**actual] {
                for text in [
                    &value.world_id,
                    &value.branch_id,
                    &value.finality_status,
                    &value.base_world_hash.0,
                    &value.runtime_manifest_hash.0,
                ] {
                    for part in text.as_bytes().chunks(8192) {
                        common.charge(1, part.len() as u64).map_err(paid)?;
                    }
                }
                if let Some(hash) = &value.finality_block_hash {
                    for part in hash.0.as_bytes().chunks(8192) {
                        common.charge(1, part.len() as u64).map_err(paid)?;
                    }
                }
                common.consume_steps(9).map_err(paid)?;
            }
            if **expected != **actual {
                return Err(invalid("owner chat binding source mismatch"));
            }
        }
        (None, None) => {}
        _ => return Err(invalid("owner chat binding source mismatch")),
    }
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?
    .with_agent_intents_data_v1(intents)?;
    let (owner, view) = {
        let source = CurrentChatFactsV1 {
            facts: &facts,
            aux: CurrentProjectionAuxFactsV1::new(&facts, auxiliary)?,
            binding,
            reader: RefCell::new(retained),
            canonical: RefCell::new(canonical),
        };
        let view = rules::owner_view_with_facts_v1(&source, agent)?.map_err(paid)?;
        let owner = view
            .as_ref()
            .map(|view| source.owner_fence(view))
            .transpose()?;
        (owner, view)
    };
    facts.check()?;
    facts.before_owned(size_of::<PaidCurrentOwnerChatProjectionDataV1>())?;
    Ok(PaidCurrentOwnerChatProjectionDataV1 {
        owner,
        view,
        _fees: facts.into_fees(),
    })
}

/// Fixed original authenticated owner-key predicate DATA. This is not read or
/// execution authority; callers still authenticate the audience separately.
pub(in crate::world) struct PaidCurrentOwnerKeyMatchDataV1 {
    matches: bool,
    _fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentOwnerKeyMatchDataV1 {
    pub(in crate::world) fn matches_data_v1(&self) -> bool {
        self.matches
    }
}
pub(in crate::world) fn prepare_current_owner_key_match_data_v1(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    auxiliary:&oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    agent: &str,
    reader_key: &str,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidCurrentOwnerKeyMatchDataV1, BodyMutationError> {
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    let matches = {
        let aux = CurrentProjectionAuxFactsV1::new(&facts, auxiliary)?;
        let key = match owner_public_key_readonly_v1(&aux, agent)? {
            Ok(key) => key,
            Err(note) => {
                let mut common = facts.common.borrow_mut();
                let n = size_of::<ChatProtocolDataV1<MemoryReservation>>() + note.len();
                common.charge(1, n as u64).map_err(paid)?;
                let fee = common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                return Err(paid(ChatProtocolDataV1::from_admitted_data_v1(
                    note.to_owned(),
                    Some(fee),
                )));
            }
        };
        facts.work(1, 0)?;
        let mut matches = key.len() == reader_key.len();
        if matches {
            for (a, b) in key
                .as_bytes()
                .chunks(8192)
                .zip(reader_key.as_bytes().chunks(8192))
            {
                facts.work(1, a.len() + b.len())?;
                if a != b {
                    matches = false;
                    break;
                }
            }
        }
        matches
    };
    facts.check()?;
    facts.before_owned(size_of::<PaidCurrentOwnerKeyMatchDataV1>())?;
    Ok(PaidCurrentOwnerKeyMatchDataV1 {
        matches,
        _fees: facts.into_fees(),
    })
}
