//! Original Runtime nonce/budget/effect-link records; no authorization grant.
use crate::{
    capability_authorization::{
        CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
    },
    incremental_field_member::*,
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
use oasis7_wasm_abi::CapabilitySubject;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapabilityRecordKindV1 {
    Nonce,
    Budget,
    EffectLink,
}
impl RuntimeCapabilityRecordKindV1 {
    pub const ALL: [Self; 3] = [Self::Nonce, Self::Budget, Self::EffectLink];
    pub fn name(self) -> &'static str {
        match self {
            Self::Nonce => "capability_nonce_records",
            Self::Budget => "capability_budget_accounts",
            Self::EffectLink => "capability_effect_receipt_links",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Nonce | Self::Budget => StateNamespaceV1::Authorization,
            Self::EffectLink => StateNamespaceV1::Effects,
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.runtime-capability-execution.v1\0");
        h.update(self.name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub(super) fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: self.name(),
            shape: FieldShape::Map,
            destination: FieldDestination::State(self.namespace()),
        }
    }
    pub(super) fn columns(self) -> &'static [RuntimeCapabilityColumnV1] {
        use RuntimeCapabilityColumnV1 as C;
        match self {
            Self::Nonce => &[
                C::RequestHash,
                C::OutcomeHash,
                C::CommittedReceiptId,
                C::State,
            ],
            Self::Budget => &[
                C::SubjectKind,
                C::SubjectAgentId,
                C::SubjectOwnerBinding,
                C::SubjectGeneration,
                C::SubjectModuleId,
                C::SubjectModuleVersion,
                C::SubjectInstanceId,
                C::SubjectSystemId,
                C::SubjectEpoch,
                C::GrantId,
                C::RemainingUnits,
                C::ReservedUnits,
                C::SpentUnits,
            ],
            Self::EffectLink => &[C::AuthorizationReceiptId],
        }
    }
    pub(super) fn leaf_count(self) -> u64 {
        self.columns().len() as u64 + 2
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapabilityColumnV1 {
    RequestHash,
    OutcomeHash,
    CommittedReceiptId,
    State,
    SubjectKind,
    SubjectAgentId,
    SubjectOwnerBinding,
    SubjectGeneration,
    SubjectModuleId,
    SubjectModuleVersion,
    SubjectInstanceId,
    SubjectSystemId,
    SubjectEpoch,
    GrantId,
    RemainingUnits,
    ReservedUnits,
    SpentUnits,
    AuthorizationReceiptId,
}
impl RuntimeCapabilityColumnV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::RequestHash => "request_hash",
            Self::OutcomeHash => "outcome_hash",
            Self::CommittedReceiptId => "committed_receipt_id",
            Self::State => "state",
            Self::SubjectKind => "subject.kind",
            Self::SubjectAgentId => "subject.agent_id",
            Self::SubjectOwnerBinding => "subject.owner_binding",
            Self::SubjectGeneration => "subject.generation",
            Self::SubjectModuleId => "subject.module_id",
            Self::SubjectModuleVersion => "subject.module_version",
            Self::SubjectInstanceId => "subject.instance_id",
            Self::SubjectSystemId => "subject.system_id",
            Self::SubjectEpoch => "subject.epoch",
            Self::GrantId => "grant_id",
            Self::RemainingUnits => "remaining_units",
            Self::ReservedUnits => "reserved_units",
            Self::SpentUnits => "spent_units",
            Self::AuthorizationReceiptId => "authorization_receipt_id",
        }
    }
    pub fn kind(self) -> RuntimeCapabilityRecordKindV1 {
        use RuntimeCapabilityRecordKindV1 as K;
        match self {
            Self::RequestHash | Self::OutcomeHash | Self::CommittedReceiptId | Self::State => {
                K::Nonce
            }
            Self::AuthorizationReceiptId => K::EffectLink,
            _ => K::Budget,
        }
    }
    pub(super) fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::RemainingUnits | Self::ReservedUnits | Self::SpentUnits => {
                MemberPayloadCodecV1::I64
            }
            Self::SubjectGeneration | Self::SubjectEpoch => MemberPayloadCodecV1::U64,
            _ => MemberPayloadCodecV1::Text,
        }
    }
    pub(super) fn nullable(self) -> bool {
        matches!(
            self,
            Self::CommittedReceiptId
                | Self::SubjectAgentId
                | Self::SubjectOwnerBinding
                | Self::SubjectGeneration
                | Self::SubjectModuleId
                | Self::SubjectModuleVersion
                | Self::SubjectInstanceId
                | Self::SubjectSystemId
                | Self::SubjectEpoch
        )
    }
    pub(super) fn subject_mask(self) -> u8 {
        match self {
            Self::SubjectAgentId | Self::SubjectOwnerBinding | Self::SubjectGeneration => 1,
            Self::SubjectModuleId | Self::SubjectModuleVersion | Self::SubjectInstanceId => 2,
            Self::SubjectSystemId | Self::SubjectEpoch => 4,
            _ => 0,
        }
    }
}
#[derive(Clone, Copy)]
pub struct RuntimeCapabilityRecordKeyV1<'a> {
    pub kind: RuntimeCapabilityRecordKindV1,
    pub id: &'a str,
}
#[derive(Clone, Copy)]
pub enum RuntimeCapabilityRecordRefV1<'a> {
    Nonce(&'a CapabilityAuthorizationNonceRecord),
    Budget(&'a CapabilityBudgetAccount),
    EffectLink(&'a CapabilityEffectReceiptLink),
}
impl<'a> RuntimeCapabilityRecordRefV1<'a> {
    pub fn kind(self) -> RuntimeCapabilityRecordKindV1 {
        match self {
            Self::Nonce(_) => RuntimeCapabilityRecordKindV1::Nonce,
            Self::Budget(_) => RuntimeCapabilityRecordKindV1::Budget,
            Self::EffectLink(_) => RuntimeCapabilityRecordKindV1::EffectLink,
        }
    }
    /// Borrow only the selected primitive; visiting references never scans Text bytes.
    pub fn column_value(
        self,
        column: RuntimeCapabilityColumnV1,
    ) -> Result<RuntimeCapabilityValueRefV1<'a>, ValueTreeError> {
        if column.kind() != self.kind() {
            return Err(ValueTreeError::Invalid("Runtime capability source kind"));
        }
        let mut selected = None;
        visit_runtime_capability_record_v1(self, |cell, payload| -> Result<(), ValueTreeError> {
            if cell == Some(column) {
                selected = Some(match payload {
                    MemberPayloadRefV1::Unit => RuntimeCapabilityValueRefV1::Unit,
                    MemberPayloadRefV1::U64(n) => RuntimeCapabilityValueRefV1::Unsigned(n),
                    MemberPayloadRefV1::I64(n) => RuntimeCapabilityValueRefV1::Signed(n),
                    MemberPayloadRefV1::Text(s) => RuntimeCapabilityValueRefV1::Text(s),
                    _ => return Err(ValueTreeError::Invalid("Runtime capability source payload")),
                });
            }
            Ok(())
        })?;
        selected.ok_or(ValueTreeError::Invalid("Runtime capability source column"))
    }
}
pub struct RuntimeCapabilityRecordsRefV1<'a> {
    pub nonces: &'a BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    pub budgets: &'a BTreeMap<String, CapabilityBudgetAccount>,
    pub effect_links: &'a BTreeMap<String, CapabilityEffectReceiptLink>,
}
/// Every original field is borrowed explicitly. Nonce state stays an original
/// String; only the original Rust CapabilitySubject variants have closed labels.
pub fn visit_runtime_capability_record_v1<'a, E>(
    source: RuntimeCapabilityRecordRefV1<'a>,
    mut emit: impl FnMut(Option<RuntimeCapabilityColumnV1>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    use MemberPayloadRefV1 as P;
    use RuntimeCapabilityColumnV1 as C;
    emit(None, P::Unit)?;
    match source {
        RuntimeCapabilityRecordRefV1::Nonce(value) => {
            let CapabilityAuthorizationNonceRecord {
                request_hash,
                outcome_hash,
                committed_receipt_id,
                state,
            } = value;
            emit(Some(C::RequestHash), P::Text(request_hash))?;
            emit(Some(C::OutcomeHash), P::Text(outcome_hash))?;
            emit(
                Some(C::CommittedReceiptId),
                committed_receipt_id.as_deref().map_or(P::Unit, P::Text),
            )?;
            emit(Some(C::State), P::Text(state))?;
        }
        RuntimeCapabilityRecordRefV1::EffectLink(value) => {
            let CapabilityEffectReceiptLink {
                authorization_receipt_id,
            } = value;
            emit(
                Some(C::AuthorizationReceiptId),
                P::Text(authorization_receipt_id),
            )?;
        }
        RuntimeCapabilityRecordRefV1::Budget(value) => {
            let CapabilityBudgetAccount {
                subject,
                grant_id,
                remaining_units,
                reserved_units,
                spent_units,
            } = value;
            visit_runtime_capability_subject_v1(subject, &mut emit)?;
            emit(Some(C::GrantId), P::Text(grant_id))?;
            for (column, n) in [
                (C::RemainingUnits, *remaining_units),
                (C::ReservedUnits, *reserved_units),
                (C::SpentUnits, *spent_units),
            ] {
                emit(Some(column), P::I64(n))?;
            }
        }
    }
    Ok(())
}
fn write_cell<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    column: Option<RuntimeCapabilityColumnV1>,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let valid = match (column, payload) {
        (None, MemberPayloadRefV1::Unit) => true,
        (Some(c), MemberPayloadRefV1::Unit) => c.kind() == kind && c.nullable(),
        (Some(c), MemberPayloadRefV1::Text(_)) => {
            c.kind() == kind && c.codec() == MemberPayloadCodecV1::Text
        }
        (Some(c), MemberPayloadRefV1::U64(_)) => {
            c.kind() == kind && c.codec() == MemberPayloadCodecV1::U64
        }
        (Some(c), MemberPayloadRefV1::I64(_)) => {
            c.kind() == kind && c.codec() == MemberPayloadCodecV1::I64
        }
        _ => false,
    };
    if !valid {
        return Err(MemberRecordError::Invalid(
            "Runtime capability column/profile",
        ));
    }
    match column {
        None => write_member_record_v1(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            payload,
            ctx,
        ),
        Some(c) => write_member_record_v1(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &((), c.name()),
            payload,
            ctx,
        ),
    }
}
#[path = "incremental_runtime_capability_records_genesis.rs"]
mod genesis;
pub use genesis::{RuntimeCapabilityRecordsWorkV1, prepare_runtime_capability_records_genesis_v1};

#[path = "incremental_runtime_capability_records_read.rs"]
mod read;
pub use read::{
    RuntimeCapabilityCellRequestV1, RuntimeCapabilityValueV1,
    open_qualified_runtime_capability_record_v1, read_qualified_runtime_capability_cell_v1,
};
#[path = "incremental_runtime_capability_records_closure.rs"]
mod closure;
pub use closure::RuntimeCapabilityRecordQualificationV1;

#[path = "incremental_runtime_capability_records_family.rs"]
mod family;
pub use family::{RuntimeCapabilityFamilyQualificationV1, qualify_runtime_capability_family_v1};

#[path = "incremental_runtime_capability_records_point.rs"]
mod point;
pub use point::{
    RuntimeCapabilityPointChangeV1, RuntimeCapabilityPointRequestV1, RuntimeCapabilityValueRefV1,
    change_qualified_runtime_capability_cell_v1, read_qualified_runtime_capability_overlay_v1,
};

#[path = "incremental_runtime_capability_records_subject.rs"]
mod subject;
pub use subject::{
    RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1, RuntimeCapabilitySubjectChangeV1,
    RuntimeCapabilitySubjectRequestV1, change_qualified_runtime_capability_subject_v1,
    read_qualified_runtime_capability_subject_overlay_v1, visit_runtime_capability_subject_v1,
};

#[path = "incremental_runtime_capability_records_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::{
    RuntimeCapabilityCreateV1, RuntimeCapabilityDeleteV1,
    create_qualified_runtime_capability_record_v1, delete_qualified_runtime_capability_record_v1,
};

#[path = "incremental_runtime_capability_records_receiver.rs"]
mod receiver;
pub use receiver::accept_runtime_capability_transition_v1;

#[path = "incremental_runtime_capability_records_pages.rs"]
mod pages;
pub use pages::*;
