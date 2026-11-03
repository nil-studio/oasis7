//! Original monetary configuration and record profiles; storage does not grant issuance authority.
use crate::{
    incremental_field_member::*,
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    main_token::*,
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[path = "incremental_main_token_policy_records_schema.rs"]
mod schema;
pub use schema::*;
const FORMAT: &str = "main_token_policy_record_closure_v1";
#[derive(Debug, Clone, Copy)]
pub enum MainTokenPolicyRecordRefV1<'a> {
    Config(&'a MainTokenConfig),
    Bucket(&'a MainTokenGenesisAllocationBucketState),
    Issuance(&'a MainTokenEpochIssuanceRecord),
    Grant(&'a RestrictedStarterClaimGrantState),
    Scheduled(&'a MainTokenScheduledPolicyUpdate),
}
impl MainTokenPolicyRecordRefV1<'_> {
    pub fn kind(self) -> MainTokenPolicyKindV1 {
        match self {
            Self::Config(_) => MainTokenPolicyKindV1::Config,
            Self::Bucket(_) => MainTokenPolicyKindV1::Bucket,
            Self::Issuance(_) => MainTokenPolicyKindV1::Issuance,
            Self::Grant(_) => MainTokenPolicyKindV1::Grant,
            Self::Scheduled(_) => MainTokenPolicyKindV1::Scheduled,
        }
    }
}
/// One exhaustive borrowed config projection, consumed by global and scheduled records.
pub fn visit_main_token_config_v1<'a, E>(
    value: &'a MainTokenConfig,
    mut emit: impl FnMut(MainTokenConfigColumnV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let MainTokenConfig {
        symbol,
        decimals,
        initial_supply,
        max_supply,
        inflation_policy,
        issuance_split,
        burn_policy,
    } = value;
    let MainTokenInflationPolicy {
        base_rate_bps,
        min_rate_bps,
        max_rate_bps,
        target_stake_ratio_bps,
        stake_feedback_gain_bps,
        epochs_per_year,
    } = inflation_policy;
    let MainTokenIssuanceSplitPolicy {
        staking_reward_bps,
        node_service_reward_bps,
        ecosystem_pool_bps,
        security_reserve_bps,
    } = issuance_split;
    let MainTokenBurnPolicy {
        gas_base_fee_burn_bps,
        slash_burn_bps,
        module_fee_burn_bps,
    } = burn_policy;
    emit(
        MainTokenConfigColumnV1::Symbol,
        MemberPayloadRefV1::Text(symbol),
    )?;
    emit(
        MainTokenConfigColumnV1::Decimals,
        MemberPayloadRefV1::U64(u64::from(*decimals)),
    )?;
    emit(
        MainTokenConfigColumnV1::InitialSupply,
        MemberPayloadRefV1::U64(*initial_supply),
    )?;
    emit(
        MainTokenConfigColumnV1::MaxSupply,
        match max_supply {
            None => MemberPayloadRefV1::Unit,
            Some(v) => MemberPayloadRefV1::U64(*v),
        },
    )?;
    emit(
        MainTokenConfigColumnV1::BaseRateBps,
        MemberPayloadRefV1::U64(u64::from(*base_rate_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::MinRateBps,
        MemberPayloadRefV1::U64(u64::from(*min_rate_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::MaxRateBps,
        MemberPayloadRefV1::U64(u64::from(*max_rate_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::TargetStakeRatioBps,
        MemberPayloadRefV1::U64(u64::from(*target_stake_ratio_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::StakeFeedbackGainBps,
        MemberPayloadRefV1::U64(u64::from(*stake_feedback_gain_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::EpochsPerYear,
        MemberPayloadRefV1::U64(u64::from(*epochs_per_year)),
    )?;
    emit(
        MainTokenConfigColumnV1::StakingRewardBps,
        MemberPayloadRefV1::U64(u64::from(*staking_reward_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::NodeServiceRewardBps,
        MemberPayloadRefV1::U64(u64::from(*node_service_reward_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::EcosystemPoolBps,
        MemberPayloadRefV1::U64(u64::from(*ecosystem_pool_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::SecurityReserveBps,
        MemberPayloadRefV1::U64(u64::from(*security_reserve_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::GasBaseFeeBurnBps,
        MemberPayloadRefV1::U64(u64::from(*gas_base_fee_burn_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::SlashBurnBps,
        MemberPayloadRefV1::U64(u64::from(*slash_burn_bps)),
    )?;
    emit(
        MainTokenConfigColumnV1::ModuleFeeBurnBps,
        MemberPayloadRefV1::U64(u64::from(*module_fee_burn_bps)),
    )?;
    Ok(())
}
/// Unit means None only on a declared Option column; Some(empty/0) uses its primitive codec.
pub fn visit_main_token_policy_record_v1<'a, E: From<MemberRecordError>>(
    source: MainTokenPolicyRecordRefV1<'a>,
    mut emit: impl FnMut(MainTokenPolicyCellV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    emit(MainTokenPolicyCellV1::Record, MemberPayloadRefV1::Unit)?;
    match source {
        MainTokenPolicyRecordRefV1::Config(value) => {
            visit_main_token_config_v1(value, |c, v| emit(MainTokenPolicyCellV1::Config(c), v))?
        }
        MainTokenPolicyRecordRefV1::Bucket(MainTokenGenesisAllocationBucketState {
            bucket_id,
            ratio_bps,
            recipient,
            cliff_epochs,
            linear_unlock_epochs,
            start_epoch,
            allocated_amount,
            claimed_amount,
        }) => {
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::BucketId),
                MemberPayloadRefV1::Text(bucket_id),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::RatioBps),
                MemberPayloadRefV1::U64(u64::from(*ratio_bps)),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::Recipient),
                MemberPayloadRefV1::Text(recipient),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::CliffEpochs),
                MemberPayloadRefV1::U64(*cliff_epochs),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::LinearUnlockEpochs),
                MemberPayloadRefV1::U64(*linear_unlock_epochs),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::StartEpoch),
                MemberPayloadRefV1::U64(*start_epoch),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::AllocatedAmount),
                MemberPayloadRefV1::U64(*allocated_amount),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::ClaimedAmount),
                MemberPayloadRefV1::U64(*claimed_amount),
            )?;
        }
        MainTokenPolicyRecordRefV1::Issuance(MainTokenEpochIssuanceRecord {
            epoch_index,
            inflation_rate_bps,
            issued_amount,
            staking_reward_amount,
            node_service_reward_amount,
            ecosystem_pool_amount,
            security_reserve_amount,
        }) => {
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::EpochIndex),
                MemberPayloadRefV1::U64(*epoch_index),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::InflationRateBps),
                MemberPayloadRefV1::U64(u64::from(*inflation_rate_bps)),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::IssuedAmount),
                MemberPayloadRefV1::U64(*issued_amount),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::StakingRewardAmount),
                MemberPayloadRefV1::U64(*staking_reward_amount),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::NodeServiceRewardAmount),
                MemberPayloadRefV1::U64(*node_service_reward_amount),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::EcosystemPoolAmount),
                MemberPayloadRefV1::U64(*ecosystem_pool_amount),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::SecurityReserveAmount),
                MemberPayloadRefV1::U64(*security_reserve_amount),
            )?;
        }
        MainTokenPolicyRecordRefV1::Grant(RestrictedStarterClaimGrantState {
            beneficiary_account_id,
            issuer_id,
            issuance_reason,
            spend_scope,
            source_treasury_bucket_id,
            issued_amount,
            issued_at_epoch,
            expires_at_epoch,
            status,
            status_updated_at_epoch,
            status_reason,
        }) => {
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::BeneficiaryAccountId),
                MemberPayloadRefV1::Text(beneficiary_account_id),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::IssuerId),
                MemberPayloadRefV1::Text(issuer_id),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::IssuanceReason),
                MemberPayloadRefV1::Text(issuance_reason),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::SpendScope),
                MemberPayloadRefV1::Text(spend_scope),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::SourceTreasuryBucketId),
                MemberPayloadRefV1::Text(source_treasury_bucket_id),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::IssuedAmount),
                MemberPayloadRefV1::U64(*issued_amount),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::IssuedAtEpoch),
                MemberPayloadRefV1::U64(*issued_at_epoch),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::ExpiresAtEpoch),
                MemberPayloadRefV1::U64(*expires_at_epoch),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::Status),
                MemberPayloadRefV1::Text(match status {
                    RestrictedStarterClaimGrantStatus::Issued => "issued",
                    RestrictedStarterClaimGrantStatus::Expired => "expired",
                    RestrictedStarterClaimGrantStatus::Revoked => "revoked",
                }),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::StatusUpdatedAtEpoch),
                match status_updated_at_epoch {
                    None => MemberPayloadRefV1::Unit,
                    Some(v) => MemberPayloadRefV1::U64(*v),
                },
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::StatusReason),
                match status_reason {
                    None => MemberPayloadRefV1::Unit,
                    Some(v) => MemberPayloadRefV1::Text(v),
                },
            )?;
        }
        MainTokenPolicyRecordRefV1::Scheduled(MainTokenScheduledPolicyUpdate {
            proposal_id,
            effective_epoch,
            next_config,
        }) => {
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::ProposalId),
                MemberPayloadRefV1::U64(*proposal_id),
            )?;
            emit(
                MainTokenPolicyCellV1::Column(MainTokenPolicyColumnV1::EffectiveEpoch),
                MemberPayloadRefV1::U64(*effective_epoch),
            )?;
            visit_main_token_config_v1(next_config, |c, v| {
                emit(MainTokenPolicyCellV1::NextConfig(c), v)
            })?;
        }
    }
    Ok(())
}
macro_rules! with_policy_key {
    ($key:expr,$encoded:ident=>$body:expr) => {
        match $key {
            MainTokenPolicyKeyV1::Config => {
                let $encoded = &();
                $body
            }
            MainTokenPolicyKeyV1::Bucket(id) | MainTokenPolicyKeyV1::Grant(id) => {
                let $encoded = id;
                $body
            }
            MainTokenPolicyKeyV1::Issuance(n) | MainTokenPolicyKeyV1::Scheduled(n) => {
                let $encoded = &n;
                $body
            }
        }
    };
}
macro_rules! with_policy_cell_key {
    ($cell:expr,$encoded:ident=>$body:expr) => {
        match $cell {
            MainTokenPolicyCellV1::Record => {
                let $encoded = &();
                $body
            }
            MainTokenPolicyCellV1::Column(c) => {
                let $encoded = &((), c.name());
                $body
            }
            MainTokenPolicyCellV1::Config(c) => match c.group() {
                None => {
                    let $encoded = &((), c.name());
                    $body
                }
                Some(group) => {
                    let $encoded = &(((), group), c.name());
                    $body
                }
            },
            MainTokenPolicyCellV1::NextConfig(c) => match c.group() {
                None => {
                    let $encoded = &(((), "next_config"), c.name());
                    $body
                }
                Some(group) => {
                    let $encoded = &((((), "next_config"), group), c.name());
                    $body
                }
            },
        }
    };
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    kind: MainTokenPolicyKindV1,
    cell: MainTokenPolicyCellV1,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if !cell.accepts(kind, payload) {
        return Err(MemberRecordError::Invalid("main token policy cell domain"));
    }
    with_policy_cell_key!(cell,key=>write_member_record_v1(kind.descriptor()?,WorldFieldRecordKindV1::Entry,key,payload,ctx))
}
#[path = "incremental_main_token_policy_records_genesis.rs"]
mod genesis;
pub use genesis::{MainTokenPolicyWorkV1, prepare_main_token_policy_genesis_v1};

#[cfg(test)]
#[path = "incremental_main_token_policy_records_tests.rs"]
mod tests;

#[path = "incremental_main_token_policy_records_read.rs"]
mod read;
pub use read::{
    MainTokenPolicyCellRequestV1, MainTokenPolicyValueV1, inspect_main_token_policy_record_root_v1,
    read_qualified_main_token_policy_cell_v1,
};
#[path = "incremental_main_token_policy_records_closure.rs"]
mod closure;
pub use closure::{MainTokenPolicyRecordQualificationV1, qualify_main_token_policy_body_v1};

#[path = "incremental_main_token_policy_records_family.rs"]
mod family;
pub use family::{
    MainTokenPolicyFamilyQualificationV1, open_qualified_main_token_policy_record_v1,
    qualify_main_token_policy_family_v1,
};

#[path = "incremental_main_token_policy_records_point.rs"]
mod point;
pub use point::{
    MainTokenPolicyCellChangeV1, MainTokenPolicyValueRefV1,
    change_qualified_main_token_policy_cell_v1, read_qualified_main_token_policy_cell_overlay_v1,
};
#[path = "incremental_main_token_policy_records_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    MainTokenPolicyRecordCreateV1, MainTokenPolicyRecordDeleteV1,
    create_qualified_main_token_policy_record_v1, delete_qualified_main_token_policy_record_v1,
};
#[path = "incremental_main_token_policy_records_receiver.rs"]
mod receiver;
pub use receiver::accept_main_token_policy_transition_v1;
#[path = "incremental_main_token_policy_records_pages.rs"]
mod pages;
pub use pages::{
    MainTokenPolicyFamilyPagesV1, MainTokenPolicyPagesWorkV1,
    mark_main_token_policy_family_pages_v1, transfer_main_token_policy_family_pages_v1,
};
