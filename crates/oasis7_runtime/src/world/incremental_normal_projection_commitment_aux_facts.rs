//! Fixed original auxiliary row access. The constructor is private to the
//! formatter's actual-column pipeline; this object grants no read permission.
use super::*;
use crate::CapabilityAgentIdentity;
use crate::world::industrial_control::readonly_runtime_owner::RuntimeOwnerReadonlyFactsV1;
use oasis7_agent_api::game::{GameIdentityNamespaceV1, GameParticipantReceiptV1};
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1, ProjectionColumnErrorV1, ProjectionColumnRefV1,
};
use oasis7_world_state::service_admission::participant::ParticipantEntryPolicyV1;
use oasis7_world_state::world_rules::IndustrialOwnerReadonlyFactsV1;
fn graph_error(e: ProjectionColumnErrorV1<BodyMutationError>) -> BodyMutationError {
    match e {
        ProjectionColumnErrorV1::Work(e) => e,
        ProjectionColumnErrorV1::Memory(e) => ValueTreeError::Memory(e).into(),
        ProjectionColumnErrorV1::Invalid(e) => invalid(e),
    }
}
pub(super) struct CurrentProjectionAuxFactsV1<'f, 'a, 'c> {
    base: &'f CurrentProjectionReadFactsV1<'a, 'c>,
    aux: &'f ProjectionColumnDataV1,
}
impl<'f, 'a, 'c> CurrentProjectionAuxFactsV1<'f, 'a, 'c> {
    pub(super) fn new(
        base: &'f CurrentProjectionReadFactsV1<'a, 'c>,
        aux: &'f ProjectionColumnDataV1,
    ) -> Result<Self, BodyMutationError> {
        base.check()?;
        if !aux.uses_budget(base.common.borrow().memory()) {
            return Err(invalid("Current auxiliary facts pool"));
        }
        Ok(Self { base, aux })
    }
    fn path(&self, path: &[&str]) -> Result<ProjectionColumnRefV1<'f>, BodyMutationError> {
        self.base.check()?;
        let memory = self.base.common.borrow().memory().clone();
        let mut work = |steps, bytes| {
            self.base
                .common
                .borrow_mut()
                .charge(steps, bytes)
                .map_err(paid)
        };
        let mut row = self.aux.root_data();
        for key in path {
            row = row
                .object_member_data(key, &memory, &mut work)
                .map_err(graph_error)?
                .ok_or_else(|| invalid("Current auxiliary fact field missing"))?;
        }
        Ok(row)
    }
    fn optional_member(
        &self,
        parent: &ProjectionColumnRefV1<'f>,
        key: &str,
    ) -> Result<Option<ProjectionColumnRefV1<'f>>, BodyMutationError> {
        let memory = self.base.common.borrow().memory().clone();
        parent
            .object_member_data(key, &memory, &mut |steps, bytes| {
                self.base
                    .common
                    .borrow_mut()
                    .charge(steps, bytes)
                    .map_err(paid)
            })
            .map_err(graph_error)
    }
    fn raw(
        &self,
        row: &ProjectionColumnRefV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        super::super::shared_json::capture_shared_v1(
            row,
            &self.base.common,
            &RefCell::new(None),
            usize::MAX,
        )
    }
}
impl IndustrialOwnerReadonlyFactsV1 for CurrentProjectionAuxFactsV1<'_, '_, '_> {
    type Error = BodyMutationError;
    fn owner_work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        self.base.work(s, b)
    }
    fn owner_before_owned(&self, b: usize) -> Result<(), Self::Error> {
        self.base.before_owned(b)
    }
    fn owner_agent_present(&self, id: &str) -> Result<bool, Self::Error> {
        self.base.owner_agent_present(id)
    }
    fn owner_visit_orders(
        &self,
        v: impl FnMut(
            &oasis7_world_state::world_rules::AgentIndustrialOrderV1,
        ) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.base.owner_visit_orders(v)
    }
    fn owner_with_claim<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&oasis7_world_state::AgentClaimState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.base.owner_with_claim(id, v)
    }
    fn owner_with_starter<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&oasis7_world_state::StarterOcClaimState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.base.owner_with_starter(id, v)
    }
    fn owner_acquisition_sequence(&self, id: &str) -> Result<Option<u64>, Self::Error> {
        self.base.owner_acquisition_sequence(id)
    }
}
impl RuntimeOwnerReadonlyFactsV1 for CurrentProjectionAuxFactsV1<'_, '_, '_> {
    fn runtime_visit_participants(
        &self,
        mut visit: impl FnMut(&GameParticipantReceiptV1) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        let revocation = self.path(&["capability_revocation_state"])?;
        let Some(participants) = self.optional_member(&revocation, "participants")? else {
            return Ok(());
        };
        let rows = self
            .optional_member(&participants, "bindings")?
            .ok_or_else(|| invalid("Current participant bindings missing"))?;
        let memory = self.base.common.borrow().memory().clone();
        let _ = rows.object_members_control_data_v1(
            &memory,
            &mut |s, b| self.base.common.borrow_mut().charge(s, b).map_err(paid),
            &mut |_, row, _| {
                let raw = self.raw(&row).map_err(ProjectionColumnErrorV1::Work)?;
                fact_decode::with_participant_receipt_data_v1(&raw, &self.base.common, |receipt| {
                    visit(receipt)
                })
                .map_err(ProjectionColumnErrorV1::Work)
            },
        )
        .map_err(graph_error)?;
        Ok(())
    }
    fn runtime_with_participant_policy<T>(
        &self,
        visit: impl FnOnce(Option<&ParticipantEntryPolicyV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let revocation = self.path(&["capability_revocation_state"])?;
        let Some(participants) = self.optional_member(&revocation, "participants")? else {
            return visit(None);
        };
        let row = self
            .optional_member(&participants, "policy")?
            .ok_or_else(|| invalid("Current participant policy missing"))?;
        let raw = self.raw(&row)?;
        fact_decode::with_participant_policy_data_v1(&raw, &self.base.common, |value| {
            visit(value.as_ref())
        })
    }
    fn runtime_with_identity<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&CapabilityAgentIdentity>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let rows = self.path(&["capability_revocation_state", "agent_identities"])?;
        match self.optional_member(&rows, id)? {
            None => visit(None),
            Some(row) => {
                let raw = self.raw(&row)?;
                fact_decode::with_runtime_identity_data_v1(&raw, &self.base.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn runtime_manufactured_sequence(&self, id: &str) -> Result<Option<u64>, Self::Error> {
        let rows = self.path(&["capability_revocation_state"])?;
        let Some(rows) =
            self.optional_member(&rows, "manufactured_identity_acquisition_sequences")?
        else {
            return Ok(None);
        };
        match self.optional_member(&rows, id)? {
            None => Ok(None),
            Some(row) => {
                let raw = self.raw(&row)?;
                fact_decode::with_u64_data_v1(&raw, &self.base.common, |v| Ok(Some(*v)))
            }
        }
    }
    fn runtime_manifest<T>(
        &self,
        visit: impl FnOnce(&str, &str, Option<&str>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let world = self.path(&["chain_resource_manifest", "world_id"])?;
        let chain = self.path(&["chain_resource_manifest", "chain_id"])?;
        let manifest = self.path(&["chain_resource_manifest"])?;
        let genesis = self.optional_member(&manifest, "genesis_ref")?;
        let world = world
            .text_data()
            .ok_or_else(|| invalid("Current world manifest world ID"))?;
        let chain = chain
            .text_data()
            .ok_or_else(|| invalid("Current world manifest chain ID"))?;
        match genesis {
            None => visit(world, chain, None),
            Some(genesis) => {
                let raw = self.raw(&genesis)?;
                fact_decode::with_option_string_data_v1(&raw, &self.base.common, |g| {
                    visit(world, chain, g.as_deref())
                })
            }
        }
    }
    fn runtime_visit_namespaces(
        &self,
        mut visit: impl FnMut(&GameIdentityNamespaceV1) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        if self
            .runtime_with_participant_policy(|policy| match policy {
                Some(policy) => visit(&policy.identity_namespace),
                None => Ok(ControlFlow::Continue(())),
            })?
            .is_break()
        {
            return Ok(());
        }
        let revocation = self.path(&["capability_revocation_state"])?;
        let Some(participants) = self.optional_member(&revocation, "participants")? else {
            return Ok(());
        };
        for (field, request) in [("policy_history", false), ("devlocal_policy_history", true)] {
            self.owner_work(1, field.len())?;
            let Some(rows) = self.optional_member(&participants, field)? else {
                continue;
            };
            let memory = self.base.common.borrow().memory().clone();
            let stopped = rows
                .array_elements_data_v1(
                    &memory,
                    &mut |s, b| self.base.common.borrow_mut().charge(s, b).map_err(paid),
                    &mut |_, mut row, work| {
                        if request {
                            row = row.object_member_data("request", &memory, work)?.ok_or(
                                ProjectionColumnErrorV1::Invalid(
                                    "participant policy authorization request",
                                ),
                            )?;
                        }
                        let policy = row.object_member_data("policy", &memory, work)?.ok_or(
                            ProjectionColumnErrorV1::Invalid(
                                "participant policy authorization policy",
                            ),
                        )?;
                        let namespace = policy
                            .object_member_data("identity_namespace", &memory, work)?
                            .ok_or(ProjectionColumnErrorV1::Invalid(
                                "participant policy namespace",
                            ))?;
                        let raw = self
                            .raw(&namespace)
                            .map_err(ProjectionColumnErrorV1::Work)?;
                        fact_decode::with_identity_namespace_data_v1(
                            &raw,
                            &self.base.common,
                            |value| visit(value),
                        )
                        .map_err(ProjectionColumnErrorV1::Work)
                    },
                )
                .map_err(graph_error)?;
            if stopped.is_break() {
                return Ok(());
            }
        }
        Ok(())
    }
}

impl CurrentProjectionAuxFactsV1<'_, '_, '_> {
    /// Every retained hot service result is validated in original String-key
    /// order before task/chat filtering. Only the fixed auxiliary graph minted
    /// by the actual Current pipeline reaches this reader.
    pub(super) fn visit_hot_service_results_data_v1(
        &self,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        mut visit: impl FnMut(
            &str,
            oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::PaidCborDataV1<
                crate::service_admission::CanonicalIntentResultV1,
            >,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.base.check()?;
        let revocation = self.path(&["capability_revocation_state"])?;
        let Some(rows) = self.optional_member(&revocation, "world_service_results")? else {
            return Ok(());
        };
        let memory = self.base.common.borrow().memory().clone();
        let _ = rows.object_members_control_data_v1(&memory,&mut|s,b|self.base.common.borrow_mut().charge(s,b).map_err(paid),&mut|key,row,_|{
   let raw=self.raw(&row).map_err(ProjectionColumnErrorV1::Work)?;
   let value={let mut common=self.base.common.borrow_mut();crate::world::incremental_normal_projection_retained_reader::decode_hot_service_result_json_data_v1(key,&raw,&mut common,canonical).map_err(ProjectionColumnErrorV1::Work)?};
   // Release the Common RefMut before callbacks can read more raw facts.
   visit(key,value).map_err(ProjectionColumnErrorV1::Work)
  }).map_err(graph_error)?;
        self.base.check()?;
        Ok(())
    }
}

impl CurrentProjectionAuxFactsV1<'_, '_, '_> {
    /// Construct only the original retained query DATA from this actual identity,
    /// owner and manifest. The callback cannot retain references beyond the paid
    /// fixed fact source. Missing current identity is the original filter branch.
    pub(super) fn with_task_lookup_data_v1<T>(
        &self,
        agent: &str,
        binding: Option<&oasis7_agent_api::RuntimeBindingV1>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        visit: impl FnOnce(
            Option<&oasis7_world_state::formal_retained_history::GameTaskControlLookupV1>,
            &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        use crate::world::industrial_control::readonly_runtime_owner::{
            current_agent_identity_readonly_v1, owner_public_key_readonly_v1,
        };
        let Some(identity) = current_agent_identity_readonly_v1(self, agent)? else {
            return visit(None, canonical);
        };
        let binding = binding
            .ok_or_else(|| invalid("actual retained task runtime binding source unavailable"))?;
        let owner = owner_public_key_readonly_v1(self, agent)?
            .map_err(|note| aux_task_business(self, note))?;
        let encoded = {
            let mut common = self.base.common.borrow_mut();
            crate::world::incremental_normal_projection_retained_reader::canonical_task_owner_digest_data_v1(&owner,&mut common,canonical)?
        };
        self.runtime_manifest(|_, _, genesis| {
            let genesis = genesis
                .ok_or_else(|| aux_task_business(self, "retained Game task genesis unavailable"))?;
            self.owner_before_owned(std::mem::size_of::<
                oasis7_world_state::formal_retained_history::GameTaskControlLookupV1,
            >())?;
            let lookup = oasis7_world_state::formal_retained_history::GameTaskControlLookupV1 {
                agent_id: aux_task_copy(self, agent)?,
                owner_binding: aux_task_copy(self, &identity.owner_binding)?,
                owner_generation: identity.generation,
                owner_public_key_digest: aux_task_copy(self, encoded.as_str())?,
                world_id: aux_task_copy(self, &binding.world_id)?,
                genesis_digest: aux_task_copy(self, genesis)?,
                branch_id: aux_task_copy(self, &binding.branch_id)?,
                reorg_epoch: binding.reorg_epoch,
            };
            visit(Some(&lookup), canonical)
        })
    }
}
fn aux_task_copy<S: IndustrialOwnerReadonlyFactsV1<Error = BodyMutationError>>(
    source: &S,
    text: &str,
) -> Result<String, BodyMutationError> {
    source.owner_before_owned(std::mem::size_of::<String>() + text.len())?;
    let mut bytes = Vec::with_capacity(text.len());
    if bytes.capacity() != text.len() {
        return Err(invalid("task lookup string capacity"));
    }
    for part in text.as_bytes().chunks(8192) {
        source.owner_work(1, part.len())?;
        bytes.extend_from_slice(part)
    }
    String::from_utf8(bytes).map_err(paid)
}
#[derive(Debug)]
struct AuxTaskBusinessFailure {
    note: &'static str,
    _fee: MemoryReservation,
}
impl std::fmt::Display for AuxTaskBusinessFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.note)
    }
}
impl std::error::Error for AuxTaskBusinessFailure {}
fn aux_task_business(
    source: &CurrentProjectionAuxFactsV1<'_, '_, '_>,
    note: &'static str,
) -> BodyMutationError {
    let mut common = source.base.common.borrow_mut();
    if let Err(error) = common.charge(1, std::mem::size_of::<AuxTaskBusinessFailure>() as u64) {
        return paid(error);
    }
    match common.memory().reserve(
        std::mem::size_of::<AuxTaskBusinessFailure>(),
        MemoryClass::Ordinary,
    ) {
        Ok(fee) => paid(AuxTaskBusinessFailure { note, _fee: fee }),
        Err(error) => ValueTreeError::Memory(error).into(),
    }
}

impl CurrentProjectionAuxFactsV1<'_, '_, '_> {
    pub(super) fn with_task_terminal_index_data_v2<T>(
        &self,
        visit: impl FnOnce(
            Option<&oasis7_world_state::TaskTerminalIndexV2>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        self.base.check()?;
        let revocation = self.path(&["capability_revocation_state"])?;
        let Some(row) = self.optional_member(&revocation, "task_terminals_v2")? else {
            return visit(None);
        };
        let raw = self.raw(&row)?;
        let value = {
            let mut common = self.base.common.borrow_mut();
            crate::world::incremental_normal_projection_retained_reader::decode_current_task_terminal_index_data_v2(&raw,&mut common)?
        };
        let result = visit(Some(std::borrow::Borrow::borrow(&value)))?;
        self.base.check()?;
        Ok(result)
    }
}

impl CurrentProjectionAuxFactsV1<'_, '_, '_> {
    pub(super) fn with_signer_floor_data_v1<T>(
        &self,
        agent: &str,
        visit: impl FnOnce(
            Option<&crate::service_admission::AgentSignerDelegationChangeV1>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        self.base.check()?;
        let parent = self.path(&["capability_revocation_state"])?;
        let Some(rows) = self.optional_member(&parent, "agent_signer_delegations")? else {
            return visit(None);
        };
        match self.optional_member(&rows, agent)? {
            None => visit(None),
            Some(row) => {
                let raw = self.raw(&row)?;
                fact_decode::with_signer_delegation_data_v1(&raw, &self.base.common, |v| {
                    visit(Some(v))
                })
            }
        }
    }
}
