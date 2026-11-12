//! Sealed actual Current source for the sole economic-contract predicate kernel.
use super::*;
use crate::world::event_processing::{alliance_fixed::AllianceFactsV1, contract_fixed::sealed};
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_authorization_mapsets::{
        AuthorizationMapSetFieldV1, AuthorizationMembershipV1,
        read_authorization_membership_overlay_v1,
    },
    incremental_world_native::recover_gameplay_policy_overlay_v1,
    incremental_world_number_maps::{
        WorldNumberMapFieldV1, WorldNumberMapKeyRefV1, WorldNumberV1,
        read_world_number_map_overlay_v1,
    },
};
pub(super) struct CurrentContractFactsV1<
    'a,
    'tx,
    'state,
    'values,
    'read,
    'core,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
> {
    pub source: facts::CurrentAllianceFactsV1<
        'a,
        'tx,
        'state,
        'values,
        'read,
        'core,
        M,
        P,
        R,
        V,
        E,
        C,
        PE,
        PC,
    >,
    pub scalar: &'a mut crate::world::incremental_action_contract_accept::ContractAcceptWorkV1,
    pub policy_scan: &'a mut StateScanBudget,
    pub policy: &'a mut TypedRecordReadWorkV1,
    pub contract_cells: &'a mut u64,
    pub binding:
        &'a mut crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    pub artifacts: &'a ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    pub now: u64,
}
impl<M, P, R, V, E, C, PE, PC> sealed::Sealed
    for CurrentContractFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
}
impl<M, P, R, V, E, C, PE, PC>
    CurrentContractFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn keep(&mut self, slot: usize, n: usize) -> Result<(), AllianceEvaluationErrorV1> {
        if let Some(fee) = &self.source.fees[slot] {
            if !fee.uses_budget(self.source.common.memory()) || fee.reserved_bytes() < n {
                return Err(limited().into());
            }
        } else {
            self.source.fees[slot] = Some(
                self.source
                    .common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)
                    .map_err(BodyMutationError::from)?
                    .retain(()),
            );
        }
        Ok(())
    }
    fn with_policy<T>(
        &mut self,
        consume: impl FnOnce(
            &oasis7_world_state::GameplayPolicyState,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, AllianceEvaluationErrorV1> {
        self.source.charge(0)?;
        let mut read = WorldFieldReadContextV1 {
            root: self.source.roots.state_root(StateNamespaceV1::Business),
            state: self.source.read.state,
            values: ValueReadContext {
                source: self.source.read.values.source,
                memory: self.source.read.values.memory,
                class: self.source.read.values.class,
                io: &mut *self.source.read.values.io,
            },
            key_work: &mut *self.source.read.key_work,
            semantic_bytes: self.source.read.semantic_bytes,
            record_bytes: self.source.read.record_bytes,
            take_state_error: &mut self.source.read.take_state_error,
            cancelled: &mut self.source.read.cancelled,
        };
        let policy = recover_gameplay_policy_overlay_v1(
            self.source.context.transaction,
            self.source.live,
            &mut read,
            self.policy_scan,
            self.policy,
        )
        .map_err(overlay)?;
        consume(policy.value().value(), self.source.common).map_err(Into::into)
    }
    fn number(
        &mut self,
        field: WorldNumberMapFieldV1,
        id: &str,
    ) -> Result<Option<WorldNumberV1>, AllianceEvaluationErrorV1> {
        self.source.charge(id.len())?;
        read_world_number_map_overlay_v1(
            field,
            WorldNumberMapKeyRefV1::Text(id),
            self.source.context,
        )
        .map_err(overlay)
        .map_err(Into::into)
    }
    fn unsigned(
        &mut self,
        field: WorldNumberMapFieldV1,
        id: &str,
    ) -> Result<Option<u64>, AllianceEvaluationErrorV1> {
        match self.number(field, id)? {
            None => Ok(None),
            Some(WorldNumberV1::Unsigned(n)) => Ok(Some(n)),
            _ => Err(bad().into()),
        }
    }
    fn signed(
        &mut self,
        field: WorldNumberMapFieldV1,
        id: &str,
    ) -> Result<Option<i64>, AllianceEvaluationErrorV1> {
        match self.number(field, id)? {
            None => Ok(None),
            Some(WorldNumberV1::Signed(n)) => Ok(Some(n)),
            _ => Err(bad().into()),
        }
    }
    fn open(
        &mut self,
        id: &str,
    ) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, AllianceEvaluationErrorV1>
    {
        self.source.charge(id.len())?;
        let guard = self.source.context.transaction.guard();
        open_current_diplomacy_contract_record_v1(
            id,
            self.source.diplomacy,
            self.source.roots,
            self.source.context.transaction,
            self.source.read,
            self.policy_scan,
            self.contract_cells,
            self.scalar.max_text_bytes,
            &guard,
            self.source.live,
        )
        .map_err(overlay)
        .map_err(Into::into)
    }
}
impl<'source, M, P, R, V, E, C, PE, PC> ContractFactsV1<'source>
    for CurrentContractFactsV1<'_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = AllianceEvaluationErrorV1;
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.source.charge(n)
    }
    fn reserve_outputs(&mut self, action: &crate::Action) -> Result<(), Self::Error> {
        let texts: &[&str] = match action {
            crate::Action::OpenEconomicContract {
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                description,
                ..
            } => &[
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                description,
            ],
            crate::Action::SettleEconomicContract {
                operator_agent_id,
                contract_id,
                notes,
                ..
            } => &[operator_agent_id, contract_id, notes],
            crate::Action::AcceptEconomicContract {
                accepter_agent_id,
                contract_id,
            } => &[accepter_agent_id, contract_id],
            _ => return Err(bad().into()),
        };
        let mut bytes = 0usize;
        for text in texts {
            self.charge(text.len())?;
            bytes = bytes.checked_add(text.len()).ok_or_else(limited)?;
        }
        // The original notes interpolate at most the two party IDs plus the chosen
        // contract ID and two i64 values (20 decimal bytes each). Reserve all fixed
        // diagnostic templates once, allowing Display's two-times String growth.
        const NOTES: &[&str] = &[
            "service contracts unavailable: collateral/evidence/remedy not implemented",
            "economic contract requires distinct parties",
            "contract_id cannot be empty",
            "economic contract already exists: ",
            "economic contract expires_at must be in the future",
            "economic contract settlement_amount must be > 0",
            "economic contract reputation_stake must be >= 0",
            "economic contract description cannot be empty",
            "economic contract blocked by gameplay policy",
            "economic contract creator open limit reached",
            "economic contract not found: ",
            "economic contract is not open",
            "economic contract accepter does not match counterparty",
            "economic contract expired",
            "economic contract not accepted",
            "economic contract settlement requires atomic exchange success",
            "economic contract settlement operator must be a party",
            "economic contract pair cooldown active",
            "economic contract data permission missing",
            "atomic exchange settlement succeeded",
        ];
        let fixed = NOTES
            .iter()
            .try_fold(2 * 20usize, |n, s| n.checked_add(s.len()))
            .ok_or_else(limited)?;
        let total = bytes
            .checked_add(fixed)
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| {
                n.checked_add(
                    size_of::<crate::WorldEventBody>()
                        + size_of::<crate::RejectReason>()
                        + size_of::<Vec<String>>()
                        + 8 * size_of::<String>(),
                )
            })
            .ok_or_else(limited)?;
        self.keep(0, total)
    }
    fn reserve_pair_key(&mut self, a: &str, b: &str) -> Result<(), Self::Error> {
        let payload = a
            .len()
            .checked_add(b.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(limited)?;
        let capacity = payload
            .max(8)
            .checked_next_power_of_two()
            .ok_or_else(limited)?;
        let n = capacity
            .checked_add(size_of::<String>())
            .ok_or_else(limited)?;
        self.charge(payload)?;
        self.keep(1, n)
    }
    fn now(&self) -> u64 {
        self.now
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.source.agent_exists(id)
    }
    fn contract_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.open(id)?.is_some())
    }
    fn contract(&mut self, id: &str) -> Result<Option<ContractRecordV1<'source>>, Self::Error> {
        let Some(proof) = self.open(id)? else {
            return Ok(None);
        };
        let common = &mut *self.source.common;
        let record=crate::world::incremental_action_contract_accept::read_current_contract_body_with_work_v1(id,&proof,self.source.read,self.scalar,&mut|n|charge(common,n))?;
        let mut payload = 0usize;
        for text in [
            &record.contract_id,
            &record.creator_agent_id,
            &record.counterparty_agent_id,
            &record.description,
        ] {
            payload = payload.checked_add(text.len()).ok_or_else(limited)?;
        }
        if let Some(notes) = &record.settlement_notes {
            payload = payload.checked_add(notes.len()).ok_or_else(limited)?;
        }
        self.keep(
            3,
            payload
                .checked_mul(2)
                .and_then(|n| n.checked_add(4 * size_of::<String>()))
                .ok_or_else(limited)?,
        )?;
        Ok(Some(ContractRecordV1::Current(record)))
    }
    fn blocked_parties(&mut self, a: &str, b: &str) -> Result<bool, Self::Error> {
        self.with_policy(|policy, common| {
            for id in &policy.blocked_agents {
                charge(
                    common,
                    id.len()
                        .checked_add(a.len())
                        .and_then(|n| n.checked_add(b.len()))
                        .ok_or_else(limited)?,
                )?;
                if id == a || id == b {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }
    fn active_contract_count(&mut self, creator: &str) -> Result<usize, Self::Error> {
        self.charge(creator.len())?;
        let guard = self.source.context.transaction.guard();
        let count = observe_current_creator_contract_count_v1(
            creator,
            self.source.diplomacy,
            self.source.roots,
            self.source.context.transaction,
            self.source.read,
            self.source.index,
            self.policy_scan,
            self.contract_cells,
            self.scalar.max_text_bytes,
            &guard,
            self.source.live,
        )
        .map_err(overlay)?;
        count
            .validate(
                self.source.context.transaction,
                self.source.common.memory(),
                self.source.live,
            )
            .map_err(overlay)?;
        Ok(count.count())
    }
    fn max_open_contracts_per_agent(&mut self) -> Result<u16, Self::Error> {
        self.with_policy(|p, common| {
            charge(common, size_of::<u16>())?;
            Ok(p.max_open_contracts_per_agent)
        })
    }
    fn pair_last_settled_at(&mut self, id: &str) -> Result<Option<u64>, Self::Error> {
        self.unsigned(WorldNumberMapFieldV1::ContractPairLastSuccess, id)
    }
    fn data_access_grant(&mut self, a: &str, b: &str) -> Result<bool, Self::Error> {
        self.charge(a.len().checked_add(b.len()).ok_or_else(limited)?)?;
        Ok(read_authorization_membership_overlay_v1(
            AuthorizationMapSetFieldV1::DataAccessPermissions,
            a,
            b,
            self.source.context,
        )
        .map_err(overlay)?
            == AuthorizationMembershipV1::Present)
    }
    fn electricity_tax_bps(&mut self) -> Result<u16, Self::Error> {
        self.with_policy(|p, common| {
            charge(common, size_of::<u16>())?;
            Ok(p.electricity_tax_bps)
        })
    }
    fn power_trade_fee_bps(&mut self) -> Result<u16, Self::Error> {
        self.with_policy(|p, common| {
            charge(common, size_of::<u16>())?;
            Ok(p.power_trade_fee_bps)
        })
    }
    fn data_tax_bps(&mut self) -> Result<u16, Self::Error> {
        self.with_policy(|p, common| {
            charge(common, size_of::<u16>())?;
            Ok(p.data_tax_bps)
        })
    }
    fn resource(
        &mut self,
        id: &str,
        kind: oasis7_agent_api::world_action::ResourceKind,
    ) -> Result<i64, Self::Error> {
        self.charge(id.len())?;
        let guard = self.source.context.transaction.guard();
        let ordinal = match kind {
            oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
            oasis7_agent_api::world_action::ResourceKind::Data => 1,
        };
        match read_agent_subtree_scalar_overlay_v1(
            AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: id,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal,
                },
                qualification: self.source.agents,
            },
            self.source.context,
            64,
            &mut std::io::sink(),
            &guard,
            self.source.live,
        )
        .map_err(overlay)?
        {
            None => Ok(0),
            Some(AgentSubtreeValueV1::I64(n)) => Ok(n),
            _ => Err(bad().into()),
        }
    }
    fn reward_window_started_at(&mut self, id: &str) -> Result<Option<u64>, Self::Error> {
        self.unsigned(WorldNumberMapFieldV1::ReputationWindowStarted, id)
    }
    fn reward_window_accumulated(&mut self, id: &str) -> Result<Option<i64>, Self::Error> {
        self.signed(WorldNumberMapFieldV1::ReputationWindowAccumulated, id)
    }
    fn world_binding(&mut self) -> Result<Option<String>, Self::Error> {
        self.keep(2, size_of::<Option<String>>() + 64)?;
        self.charge(64)?;
        let guard = self.source.context.transaction.guard();
        let value =
            crate::world::incremental_industrial_eligibility::read_current_world_binding_v1(
                self.artifacts,
                self.source.context,
                self.source.read,
                self.binding,
                &guard,
                self.source.live,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(ContractBindingErrorV1(e))))?;
        Ok(value.as_ref().map(|text| text.to_owned()))
    }
}

#[derive(Debug)]
struct ContractBindingErrorV1(
    crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1,
);
impl std::fmt::Display for ContractBindingErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for ContractBindingErrorV1 {}
