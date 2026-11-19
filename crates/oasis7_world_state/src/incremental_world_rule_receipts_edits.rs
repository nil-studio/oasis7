//! Closed original branch edits. Whole records are encoded only on explicit
//! insertion/replacement; ordinary progress edits touch selected scalar leaves.
use super::*;
use crate::incremental_world_rule_composite_family::CompositeBranchCodecV1;
use crate::incremental_world_rule_composite_overlay::prepare_semantic_path_key_v1;
use oasis7_proto::memory_budget::ReservedValue;
use std::collections::BTreeMap;
pub enum WorldRuleRequestEditV1<'a> {
    Count {
        expected: [u8; 32],
        after: u64,
    },
    Put {
        key: &'a str,
        expected: Option<[u8; 32]>,
        receipt: &'a crate::world_rules::WorldRuleReceiptV1,
    },
    Remove {
        key: &'a str,
        expected: [u8; 32],
    },
}
pub enum AgentOrderEditV1<'a> {
    Count {
        expected: [u8; 32],
        after: u64,
    },
    Put {
        key: &'a str,
        expected: Option<[u8; 32]>,
        order: &'a crate::world_rules::AgentIndustrialOrderV1,
    },
    Remove {
        key: &'a str,
        expected: [u8; 32],
    },
    Phase {
        key: &'a str,
        expected: [u8; 32],
        after: crate::world_rules::AgentIndustrialPhaseV1,
    },
    CompletedWorkTicks {
        key: &'a str,
        expected: [u8; 32],
        after: u32,
    },
    StartedAt {
        key: &'a str,
        expected: [u8; 32],
        after: Option<u64>,
    },
    ProducedAt {
        key: &'a str,
        expected: [u8; 32],
        after: Option<u64>,
    },
    BlockedAt {
        key: &'a str,
        expected: [u8; 32],
        after: Option<u64>,
    },
    DeliveredAt {
        key: &'a str,
        expected: [u8; 32],
        after: Option<u64>,
    },
    LastProgressTick {
        key: &'a str,
        expected: [u8; 32],
        after: Option<u64>,
    },
    TerminalReason {
        key: &'a str,
        expected: [u8; 32],
        after: Option<&'a str>,
    },
    RefundPending {
        key: &'a str,
        expected: [u8; 32],
        after: bool,
    },
    RefundedAt {
        key: &'a str,
        expected: [u8; 32],
        after: Option<u64>,
    },
}
pub(super) struct PreparedEdit<'a> {
    pub(super) keys: Vec<ReservedValue<Vec<u8>>>,
    pub(super) expected: Option<[u8; 32]>,
    pub(super) after: Option<Encoded<'a>>,
}
pub(super) trait OwnerEditV1 {
    type Branch: CompositeBranchCodecV1;
    fn prepare<'a, W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &'a self,
        profile: &TypedRecordProfileV1,
        write: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut WorldRuleCompositeWriteWorkV1,
    ) -> Result<PreparedEdit<'a>, MemberRecordError>;
}
enum Key<'a> {
    Field(&'static str),
    Entry(&'a str),
}
fn path<'a, W: StatePageWriter, C: FnMut() -> bool>(
    keys: &[Key<'_>],
    expected: Option<[u8; 32]>,
    after: Option<Encoded<'a>>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PreparedEdit<'a>, MemberRecordError> {
    // The enclosing owner batch reserves all Vec/Encoded metadata before this
    // allocation; each variable semantic key retains its own admitted buffer.
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        out.push(match key {
            Key::Field(name) => prepare_semantic_path_key_v1(*name, write)?,
            Key::Entry(id) => prepare_semantic_path_key_v1(&("entry", *id), write)?,
        });
    }
    Ok(PreparedEdit {
        keys: out,
        expected,
        after,
    })
}
impl OwnerEditV1 for WorldRuleRequestEditV1<'_> {
    type Branch = BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>;
    fn prepare<'a, W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &'a self,
        profile: &TypedRecordProfileV1,
        write: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut WorldRuleCompositeWriteWorkV1,
    ) -> Result<PreparedEdit<'a>, MemberRecordError> {
        match self {
            Self::Count { expected, after } => path(
                &[Key::Field("_length")],
                Some(*expected),
                Some(Encoded::Scalar(MemberPayloadRefV1::U64(*after))),
                write,
            ),
            Self::Put {
                key,
                expected,
                receipt,
            } => {
                let value = receipt.encode(profile, write, work)?;
                path(&[Key::Entry(key)], *expected, Some(value), write)
            }
            Self::Remove { key, expected } => {
                path(&[Key::Entry(key)], Some(*expected), None, write)
            }
        }
    }
}
impl OwnerEditV1 for AgentOrderEditV1<'_> {
    type Branch = BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>;
    fn prepare<'a, W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &'a self,
        profile: &TypedRecordProfileV1,
        write: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut WorldRuleCompositeWriteWorkV1,
    ) -> Result<PreparedEdit<'a>, MemberRecordError> {
        let (key, expected, field, after) = match self {
            Self::Count { expected, after } => {
                return path(
                    &[Key::Field("_length")],
                    Some(*expected),
                    Some(Encoded::Scalar(MemberPayloadRefV1::U64(*after))),
                    write,
                );
            }
            Self::Put {
                key,
                expected,
                order,
            } => {
                let value = order.encode(profile, write, work)?;
                return path(&[Key::Entry(key)], *expected, Some(value), write);
            }
            Self::Remove { key, expected } => {
                return path(&[Key::Entry(key)], Some(*expected), None, write);
            }
            Self::Phase {
                key,
                expected,
                after,
            } => {
                use crate::world_rules::AgentIndustrialPhaseV1 as P;
                let tag = match after {
                    P::Reserved => 0,
                    P::Producing => 1,
                    P::ProductionBlocked => 2,
                    P::AwaitingDelivery => 3,
                    P::Delivered => 4,
                    P::Cancelled => 5,
                    P::Disposed => 6,
                };
                return path(
                    &[Key::Entry(key), Key::Field("phase"), Key::Field("tag")],
                    Some(*expected),
                    Some(Encoded::Scalar(MemberPayloadRefV1::U64(tag))),
                    write,
                );
            }
            Self::CompletedWorkTicks {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "completed_work_ticks",
                MemberPayloadRefV1::U64(u64::from(*after)),
            ),
            Self::StartedAt {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "started_at",
                match after {
                    Some(v) => MemberPayloadRefV1::U64(*v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
            Self::ProducedAt {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "produced_at",
                match after {
                    Some(v) => MemberPayloadRefV1::U64(*v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
            Self::BlockedAt {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "blocked_at",
                match after {
                    Some(v) => MemberPayloadRefV1::U64(*v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
            Self::DeliveredAt {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "delivered_at",
                match after {
                    Some(v) => MemberPayloadRefV1::U64(*v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
            Self::LastProgressTick {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "last_progress_tick",
                match after {
                    Some(v) => MemberPayloadRefV1::U64(*v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
            Self::TerminalReason {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "terminal_reason",
                match after {
                    Some(v) => MemberPayloadRefV1::Text(v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
            Self::RefundPending {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "refund_pending",
                MemberPayloadRefV1::Bool(*after),
            ),
            Self::RefundedAt {
                key,
                expected,
                after,
            } => (
                *key,
                *expected,
                "refunded_at",
                match after {
                    Some(v) => MemberPayloadRefV1::U64(*v),
                    None => MemberPayloadRefV1::Unit,
                },
            ),
        };
        path(
            &[Key::Entry(key), Key::Field(field)],
            Some(expected),
            Some(Encoded::Scalar(after)),
            write,
        )
    }
}
