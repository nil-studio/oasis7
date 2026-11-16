//! Chain-private, paid classification of genuine Frozen/Node committed input.
//! The plan is DATA, never applied input, bootstrap authority or acceptance.
use crate::{
    iws_normal_input::PaidVerifiedNormalTickInputV1,
    iws_normal_input_decode::{
        self as decode, NormalInputDecodeErrorV1, NormalInputDecodeLimitsV1,
        PaidNodeReplicatedInputV1,
    },
};
use oasis7::consensus_action_payload::{ConsensusActionAuthEnvelope, ConsensusActionPayloadBody};
use oasis7_consensus::{
    node_consensus_action::NodeConsensusAction,
    replicated_execution_input::{self as replicated, NodeReplicatedExecutionInputV1},
};
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::{self as cbor, PaidCborDataV1},
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use std::{borrow::Borrow, fmt};
type ServiceRequest =
    oasis7::world_service::SubmitIntentRequest<oasis7::world_service::WorldServicePayloadV1>;
type CleanupRequest = oasis7::world_service::task_cleanup_v2::TaskCleanupSubmitV2;
pub(crate) enum NormalServiceIntentDataV1 {
    LegacyV1(Box<ServiceRequest>),
    CleanupV2(Box<CleanupRequest>),
}
enum RowData {
    Runtime {
        action: oasis7_runtime::Action,
        origin: Option<oasis7_runtime::CommittedRecipeOrigin>,
        authorization: Option<oasis7_runtime::GovernanceAuthorizationV1>,
    },
    Simulator {
        action: oasis7_simulator::Action,
        submitter: oasis7_simulator::ActionSubmitter,
    },
    Service {
        action_id: u64,
        request: NormalServiceIntentDataV1,
    },
}
struct PaidRow {
    data: RowData,
    _fee: MemoryReservation,
}
pub(crate) struct PaidNormalInputPlanV1<'a, 's> {
    input: &'a PaidVerifiedNormalTickInputV1<'s>,
    rows: Vec<PaidRow>,
    bootstrap: Option<PaidCborDataV1<Vec<oasis7_runtime::ProviderBackedBootstrapAuthorityV1>>>,
    core: WorkIdentityV1,
    _groups: MemoryReservation,
}
pub(crate) struct PaidNormalInputPlanRefusalV1<'a, 's> {
    input: &'a PaidVerifiedNormalTickInputV1<'s>,
    note: String,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl PaidNormalInputPlanRefusalV1<'_, '_> {
    pub(crate) fn message(&self) -> &str {
        &self.note
    }
    pub(crate) fn validate_context(
        &self,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), NormalInputDecodeErrorV1> {
        self.input.validate_context(c)?;
        c.consume_steps(1)?;
        if self.core != c.work().identity() || !self._fee.uses_budget(c.memory()) {
            return Err(NormalInputDecodeErrorV1::Invalid(
                "normal plan refusal context",
            ));
        }
        Ok(())
    }
}
impl PaidNormalInputPlanV1<'_, '_> {
    pub(crate) fn validate_context(
        &self,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), NormalInputDecodeErrorV1> {
        self.input.validate_context(c)?;
        c.consume_steps(self.rows.len() as u64 + 1)?;
        if self.core != c.work().identity()
            || !self._groups.uses_budget(c.memory())
            || self.rows.iter().any(|r| !r._fee.uses_budget(c.memory()))
        {
            return Err(NormalInputDecodeErrorV1::Invalid(
                "normal classified plan context",
            ));
        }
        if let Some(b) = &self.bootstrap {
            b.validate_context(c)?
        }
        Ok(())
    }
    pub(crate) fn input(&self) -> &PaidVerifiedNormalTickInputV1<'_> {
        self.input
    }
    pub(crate) fn runtime_actions(
        &self,
    ) -> impl Iterator<
        Item = (
            &oasis7_runtime::Action,
            Option<&oasis7_runtime::CommittedRecipeOrigin>,
            Option<&oasis7_runtime::GovernanceAuthorizationV1>,
        ),
    > {
        self.rows.iter().filter_map(|r| match &r.data {
            RowData::Runtime {
                action,
                origin,
                authorization,
            } => Some((action, origin.as_ref(), authorization.as_ref())),
            _ => None,
        })
    }
    pub(crate) fn simulator_actions(
        &self,
    ) -> impl Iterator<
        Item = (
            &oasis7_simulator::Action,
            &oasis7_simulator::ActionSubmitter,
        ),
    > {
        self.rows.iter().filter_map(|r| match &r.data {
            RowData::Simulator { action, submitter } => Some((action, submitter)),
            _ => None,
        })
    }
    pub(crate) fn service_intents(
        &self,
    ) -> impl Iterator<Item = (u64, &NormalServiceIntentDataV1)> {
        self.rows.iter().filter_map(|r| match &r.data {
            RowData::Service { action_id, request } => Some((*action_id, request)),
            _ => None,
        })
    }
    pub(crate) fn bootstrap(
        &self,
    ) -> Option<&[oasis7_runtime::ProviderBackedBootstrapAuthorityV1]> {
        self.bootstrap.as_ref().map(|b| {
            Borrow::<Vec<oasis7_runtime::ProviderBackedBootstrapAuthorityV1>>::borrow(b).as_slice()
        })
    }
}
fn limit() -> NormalInputDecodeErrorV1 {
    NormalInputDecodeErrorV1::Limit("normal input plan admission")
}
fn note<'a, 's>(
    input: &'a PaidVerifiedNormalTickInputV1<'s>,
    args: fmt::Arguments<'_>,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidNormalInputPlanRefusalV1<'a, 's>, NormalInputDecodeErrorV1> {
    struct Count(usize);
    impl fmt::Write for Count {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.0 = self.0.checked_add(s.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }
    let mut count = Count(0);
    fmt::write(&mut count, args).map_err(|_| limit())?;
    c.charge(1, count.0 as u64)?;
    let fee = c.memory().reserve(
        count
            .0
            .checked_add(std::mem::size_of::<PaidNormalInputPlanRefusalV1<'_, '_>>())
            .ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let mut text = String::with_capacity(count.0);
    fmt::write(&mut text, args).map_err(|_| limit())?;
    if text.capacity() != count.0 {
        return Err(limit());
    }
    Ok(PaidNormalInputPlanRefusalV1 {
        input,
        note: text,
        core: c.work().identity(),
        _fee: fee,
    })
}
struct ReservedValidation<'a, 'b, 'ctx> {
    common: &'a mut FormalBoundedContextV1<'ctx>,
    limits: &'b NormalInputDecodeLimitsV1,
    scope: &'a mut MemoryReservation,
}
impl replicated::ReplicatedExecutionInputValidationOpsV1 for ReservedValidation<'_, '_, '_> {
    type Owner = PaidNodeReplicatedInputV1;
    type Fatal = NormalInputDecodeErrorV1;
    fn before_action(&mut self, a: &NodeConsensusAction) -> Result<(), Self::Fatal> {
        self.common.charge(1, a.submitter_player_id.len() as u64)?;
        let mut f = self.common.memory().reserve(
            a.submitter_player_id
                .len()
                .checked_add(512)
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        )?;
        self.scope
            .merge(&mut f)
            .map_err(NormalInputDecodeErrorV1::Invalid)
    }
    fn decode_action(
        &mut self,
        a: &NodeConsensusAction,
    ) -> Result<Result<Option<Self::Owner>, String>, Self::Fatal> {
        replicated::decode_replicated_execution_input_action_with(a, |raw| {
            decode::decode_paid_replicated_input_codec_v1(raw, self.limits, self.common)
        })
    }
}
/// Both passes borrow the real verified input. Reserved validation of the
/// ENTIRE list precedes the first ordinary payload, preserving original error
/// order even if an earlier ordinary payload is malformed.
pub(crate) fn prepare_normal_input_plan_v1<'a, 's>(
    input: &'a PaidVerifiedNormalTickInputV1<'s>,
    limits: &NormalInputDecodeLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    Result<PaidNormalInputPlanV1<'a, 's>, PaidNormalInputPlanRefusalV1<'a, 's>>,
    NormalInputDecodeErrorV1,
> {
    input.validate_context(common)?;
    let context = input.data().context();
    let count = context.committed_actions.len();
    common.consume_steps(1)?;
    let bytes = count
        .checked_mul(std::mem::size_of::<PaidRow>())
        .and_then(|n| n.checked_add(std::mem::size_of::<PaidNormalInputPlanV1<'_, '_>>() + 4096))
        .ok_or_else(limit)?;
    let mut groups = common.memory().reserve(bytes, MemoryClass::Ordinary)?;
    let validation = replicated::validate_replicated_execution_input_actions_with(
        &context.committed_actions,
        context.height,
        &mut ReservedValidation {
            common,
            limits,
            scope: &mut groups,
        },
    )?;
    common.consume_steps(0)?;
    if let Err(error) = validation {
        return Ok(Err(note(
            input,
            format_args!(
                "execution driver replicated execution input validation failed at height {}: {}",
                context.height, error
            ),
            common,
        )?));
    }
    let mut rows = Vec::with_capacity(count);
    if rows.capacity() != count {
        return Err(limit());
    }
    let mut bootstrap = None;
    for action in &context.committed_actions {
        common.charge(1, action.submitter_player_id.len() as u64)?;
        let decoded = replicated::decode_replicated_execution_input_action_with(action, |raw| {
            decode::decode_paid_replicated_input_codec_v1(raw, limits, common)
        })?;
        common.consume_steps(0)?;
        let decoded = match decoded {
            Ok(v) => v,
            Err(error) => {
                return Ok(Err(note(
                    input,
                    format_args!(
                        "execution driver decode replicated execution input failed action_id={} err={error}",
                        action.action_id
                    ),
                    common,
                )?));
            }
        };
        if let Some(node) = decoded {
            let node: &NodeReplicatedExecutionInputV1 = node.borrow();
            if node.target_height != context.height {
                return Ok(Err(note(
                    input,
                    format_args!(
                        "execution driver replicated execution input height mismatch: action_height={} context_height={}",
                        node.target_height, context.height
                    ),
                    common,
                )?));
            }
            if bootstrap.is_some() {
                return Ok(Err(note(
                    input,
                    format_args!(
                        "execution driver received duplicate replicated ProviderBacked bootstrap inputs at height {}",
                        context.height
                    ),
                    common,
                )?));
            }
            common.charge(1, node.kind.len() as u64)?;
            if node.kind != replicated::PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND {
                return Ok(Err(note(
                    input,
                    format_args!("unsupported replicated execution input kind {}", node.kind),
                    common,
                )?));
            }
            if node.target_height == 0 {
                return Ok(Err(note(
                    input,
                    format_args!(
                        "ProviderBacked authority bootstrap input must be bound to a commit height"
                    ),
                    common,
                )?));
            }
            let authority = oasis7_runtime::decode_provider_bootstrap_data_v1(
                &node.payload_cbor,
                limits,
                common,
            )?;
            let authority = match authority {
                Ok(v) => v,
                Err(error) => {
                    return Ok(Err(note(
                        input,
                        format_args!(
                            "decode ProviderBacked authority bootstrap replicated input failed: {}",
                            error.message()
                        ),
                        common,
                    )?));
                }
            };
            if Borrow::<Vec<oasis7_runtime::ProviderBackedBootstrapAuthorityV1>>::borrow(&authority)
                .is_empty()
            {
                return Ok(Err(note(
                    input,
                    format_args!("ProviderBacked authority bootstrap input cannot be empty"),
                    common,
                )?));
            }
            bootstrap = Some(authority);
            continue;
        }
        let envelope = decode::decode_normal_payload_data_v1(&action.payload_cbor, limits, common)?;
        if let Err(error) = envelope.envelope() {
            let refusal = if envelope.is_metadata_refusal() {
                note(input, format_args!("{error}"), common)?
            } else {
                note(
                    input,
                    format_args!(
                        "execution driver decode committed action failed action_id={} err={error}",
                        action.action_id
                    ),
                    common,
                )?
            };
            return Ok(Err(refusal));
        }
        let layout =
            oasis7::consensus_action_payload::consensus_payload_owned_layout_v1(&mut || {
                common.consume_steps(1)
            })?;
        let mut rowfee = cbor::reserve_closed_cbor_output_v1(
            &action.payload_cbor,
            layout.into(),
            limits,
            common,
        )?;
        // The only additional output fields are copied from this genuine Node row.
        let origin_bytes = action
            .submitter_player_id
            .len()
            .checked_add(action.payload_hash.len())
            .and_then(|n| n.checked_add(context.action_root.len()))
            .and_then(|n| {
                n.checked_add(std::mem::size_of::<oasis7_runtime::CommittedRecipeOrigin>())
            })
            .ok_or_else(limit)?;
        common.charge(1, origin_bytes as u64)?;
        let mut extra = common
            .memory()
            .reserve(origin_bytes, MemoryClass::Ordinary)?;
        rowfee
            .merge(&mut extra)
            .map_err(NormalInputDecodeErrorV1::Invalid)?;
        let data =
            envelope
                .with_envelope_data(common, |envelope, common| {
                    let authorization = match envelope.auth {
                        Some(ConsensusActionAuthEnvelope::GovernanceAction(proof)) => Some(proof),
                        _ => None,
                    };
                    match envelope.body {
                        ConsensusActionPayloadBody::WorldCleanupV2 { request } => {
                            common.charge(1, action.payload_cbor.len() as u64)?;
                            request.validate().map_err(ClassifyError::Codec)?;
                            Ok(RowData::Service {
                                action_id: action.action_id,
                                request: NormalServiceIntentDataV1::CleanupV2(Box::new(request)),
                            })
                        }
                        ConsensusActionPayloadBody::WorldServiceIntent { request } => {
                            Ok(RowData::Service {
                                action_id: action.action_id,
                                request: NormalServiceIntentDataV1::LegacyV1(Box::new(request)),
                            })
                        }
                        ConsensusActionPayloadBody::SimulatorAction { action, submitter } => {
                            Ok(RowData::Simulator { action, submitter })
                        }
                        ConsensusActionPayloadBody::RuntimeAction { action: decoded } => {
                            if let oasis7_runtime::Action::WorldServiceIntent { request } = decoded
                            {
                                let converted = cbor::decode_closed_json_value_data_v1::<
                                    ServiceRequest,
                                >(
                                    request, layout.into(), limits, common
                                )?;
                                match converted {
                                    Err(e) => {
                                        let n = e.message().len();
                                        common.charge(1, n as u64)?;
                                        let note = format!(
                                            "invalid registered world-service intent: {}",
                                            e.message()
                                        );
                                        Err(ClassifyError::Codec(note))
                                    }
                                    Ok(v) => v
                                        .with_data(common, |request, _| {
                                            Ok(RowData::Service {
                                                action_id: action.action_id,
                                                request: NormalServiceIntentDataV1::LegacyV1(
                                                    Box::new(request),
                                                ),
                                            })
                                        })
                                        .map_err(ClassifyError::Fatal)?,
                                }
                            } else {
                                let origin =
                                    envelope.gameplay_submission_origin.map(|submission| {
                                        oasis7_runtime::CommittedRecipeOrigin {
                                            submission,
                                            consensus_action_id: action.action_id,
                                            consensus_submitter_player_id: action
                                                .submitter_player_id
                                                .clone(),
                                            action_payload_hash: action.payload_hash.clone(),
                                            committed_height: context.height,
                                            action_root: context.action_root.clone(),
                                        }
                                    });
                                Ok(RowData::Runtime {
                                    action: decoded,
                                    origin,
                                    authorization,
                                })
                            }
                        }
                    }
                })
                .map_err(|error| match error {
                    decode::NormalPayloadConsumeErrorV1::Resource(e) => e,
                    decode::NormalPayloadConsumeErrorV1::Context { error, .. } => error,
                    decode::NormalPayloadConsumeErrorV1::Codec(_) => {
                        NormalInputDecodeErrorV1::Invalid("classified envelope changed")
                    }
                })?;
        common.consume_steps(0)?;
        let data = match data {
            Ok(v) => v,
            Err(ClassifyError::Fatal(e)) => return Err(e),
            Err(ClassifyError::Codec(e)) => {
                return Ok(Err(note(input, format_args!("{e}"), common)?));
            }
        };
        rows.push(PaidRow { data, _fee: rowfee });
    }
    common.consume_steps(0)?;
    Ok(Ok(PaidNormalInputPlanV1 {
        input,
        rows,
        bootstrap,
        core: common.work().identity(),
        _groups: groups,
    }))
}
enum ClassifyError {
    Codec(String),
    Fatal(NormalInputDecodeErrorV1),
}
impl From<NormalInputDecodeErrorV1> for ClassifyError {
    fn from(e: NormalInputDecodeErrorV1) -> Self {
        Self::Fatal(e)
    }
}
impl From<oasis7_formal_protocol::controlled_authority::formal::FormalError> for ClassifyError {
    fn from(e: oasis7_formal_protocol::controlled_authority::formal::FormalError) -> Self {
        Self::Fatal(e.into())
    }
}
#[cfg(test)]
#[path = "iws_normal_input_plan_tests.rs"]
mod tests;
