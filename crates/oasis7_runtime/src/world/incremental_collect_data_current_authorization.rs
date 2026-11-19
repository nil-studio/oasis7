//! Current claim and replay checks for a genuinely signed selected request.
//! This owner confers no resource/result/post or durable-completion qualification.
use super::auth_signature::*;
use super::*;
use oasis7_formal_protocol::incremental_state::{StateNamespaceV1, StateScanBudget};
use oasis7_world_state::{
    incremental_authenticated_nonces::{
        AuthenticatedNonceChangeV1, AuthenticatedNonceV1, change_authenticated_nonce_overlay_v1,
        read_authenticated_nonce_overlay_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_collection_controls::WorldCollectionControlsQualificationV1,
    incremental_world_native::{
        StarterBindingIndexV1, StarterBindingIndexWorkV1, WorldNativeQualificationWorkV1,
        observe_current_starter_binding_v1,
    },
};

pub(in crate::world) struct CollectDataAuthorizationWorkV1 {
    pub signature: CollectDataSignatureWorkV1,
    pub claims: StarterBindingIndexWorkV1,
    pub native: WorldNativeQualificationWorkV1,
    pub scan: StateScanBudget,
}
pub(in crate::world) enum SelectedCollectDataAuthorizationV1<'a> {
    Authorized(CurrentCollectDataAuthorizationV1<'a>),
    Rejected(ReservedValue<crate::RejectReason>),
}
/// Unforgeable request-bound replay source. A business sink must validate this
/// before any publication, then consume it once for the exact nonce CAS.
pub(in crate::world) struct CurrentCollectDataAuthorizationV1<'a> {
    signature: VerifiedCollectDataSignatureV1<'a>,
    expected: AuthenticatedNonceV1,
    identity: PreparedGuard,
    _fee: MemoryReservation,
}
impl CurrentCollectDataAuthorizationV1<'_> {
    pub(in crate::world) fn fields(&self) -> &CollectDataAuthFieldsV1<'_> {
        self.signature.fields()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<
            super::super::incremental_module_invocation::RootRulePhaseOutcomeV1,
        >,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.identity, live)?;
            self.signature.validate(selected, outcome, tx, memory, live)
        })
    }
    pub(in crate::world) fn consume_nonce<M, R, E, C, W, CW>(
        self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<
            super::super::incremental_module_invocation::RootRulePhaseOutcomeV1,
        >,
        context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: ValueNodeWriter,
        CW: FnMut() -> bool,
    {
        let result = (|| {
            self.validate(
                selected,
                outcome,
                context.transaction,
                context.values.memory,
                live,
            )?;
            if (context.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if let Some(error) = (context.take_state_error)() {
                return Err(error.into());
            }
            let fields = self.fields();
            let guard = context.transaction.guard();
            change_authenticated_nonce_overlay_v1(
                AuthenticatedNonceChangeV1 {
                    player: fields.player,
                    public_key: fields.public_key,
                    expected: self.expected,
                    after: Some(fields.nonce),
                    create_player: true,
                },
                context,
                write,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            if (context.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if let Some(error) = (context.take_state_error)() {
                return Err(error.into());
            }
            context
                .transaction
                .validate_guard(&context.transaction.guard(), live)?;
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = context
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
fn reject(
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    bytes: usize,
    note: impl FnOnce() -> String,
) -> Result<ReservedValue<crate::RejectReason>, BodyMutationError> {
    let fee = memory
        .reserve(
            bytes
                .checked_add(512)
                .ok_or(ValueTreeError::ResourceLimited("collect auth rejection"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(crate::RejectReason::RuleDenied {
        notes: vec![note()],
    }))
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn authorize_selected_collect_data_v1<'a, M, P, R, E, C>(
    selected: &'a SelectedPendingActionV1,
    outcome: &'a ReservedValue<super::super::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    index: &ReservedValue<StarterBindingIndexV1>,
    controls: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CollectDataAuthorizationWorkV1,
    live: &ExecutionFence,
) -> Result<SelectedCollectDataAuthorizationV1<'a>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        let memory = read.values.memory;
        let signature = match verify_selected_collect_data_signature_v1(
            selected,
            outcome,
            tx,
            memory,
            &mut work.signature,
            live,
            &mut read.cancelled,
        )? {
            SelectedCollectDataSignatureV1::Rejected(reason) => {
                return Ok(SelectedCollectDataAuthorizationV1::Rejected(reason));
            }
            SelectedCollectDataSignatureV1::Verified(proof) => proof,
        };
        if read.root != roots.state_root(StateNamespaceV1::Authorization)
            || controls.allocation_class() != MemoryClass::Ordinary
            || !controls.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "authenticated collect Current controls",
            ));
        }
        controls.validate_binding(roots)?;
        let fields = signature.fields();
        let guard = tx.guard();
        let claims = observe_current_starter_binding_v1(
            fields.player,
            fields.public_key,
            index,
            roots,
            tx,
            read,
            &mut work.scan,
            &mut work.native,
            &mut work.claims,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        claims.validate(tx, memory, live).map_err(overlay_error)?;
        if claims.matching_claims() != 1 || claims.unique_agent_id() != Some(fields.collector) {
            let count = claims.matching_claims();
            let reason = reject(
                memory,
                fields
                    .collector
                    .len()
                    .checked_add(256)
                    .ok_or(ValueTreeError::ResourceLimited("collect auth note"))?,
                || {
                    format!(
                        "authenticated collect_data requires exactly one starter OC player/key binding for collector {}; found {}",
                        fields.collector, count
                    )
                },
            )?;
            return Ok(SelectedCollectDataAuthorizationV1::Rejected(reason));
        }
        let expected = {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: tx,
                values: ValueReadContext {
                    source: read.values.source,
                    memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            read_authenticated_nonce_overlay_v1(fields.player, fields.public_key, &mut overlay)
                .map_err(overlay_error)?
        };
        let last = match expected {
            AuthenticatedNonceV1::Value(n) => n,
            _ => 0,
        };
        if (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        if fields.nonce == 0 || fields.nonce <= last {
            let reason = reject(memory, 256, || {
                format!(
                    "authenticated collect_data nonce replay: expected nonce > {last}, received {}",
                    fields.nonce
                )
            })?;
            return Ok(SelectedCollectDataAuthorizationV1::Rejected(reason));
        }
        let fee = memory
            .reserve(
                std::mem::size_of::<CurrentCollectDataAuthorizationV1<'_>>() + 256,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(SelectedCollectDataAuthorizationV1::Authorized(
            CurrentCollectDataAuthorizationV1 {
                signature,
                expected,
                identity: tx.guard(),
                _fee: fee,
            },
        ))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
