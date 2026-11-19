//! Transaction-local changed-path DATA update for the actual unique
//! OutcomeRecorded producer. No whole Authority branch is recovered here.
use super::*;
#[derive(Debug)]
pub(in crate::world) enum WarmError {
    Source(SourceError),
    Body(crate::world::incremental_body::BodyMutationError),
}
impl From<SourceError> for WarmError {
    fn from(e: SourceError) -> Self {
        Self::Source(e)
    }
}
impl From<AuthorizationRootCacheErrorV1> for WarmError {
    fn from(e: AuthorizationRootCacheErrorV1) -> Self {
        Self::Source(e.into())
    }
}
impl From<FormalError> for WarmError {
    fn from(e: FormalError) -> Self {
        Self::Source(e.into())
    }
}
/// Consumes the unique original publication proof before any source read.
/// Its genuine original event/receipt owners retain their fees throughout the
/// post-read handoff. This alone is not the full seven-source hash capability.
pub(in crate::world) struct OwnedProjectedOutcomeV1 {
    pub(super) stage_identity:std::sync::Arc<ReservedValue<()>>,
    publication:crate::world::incremental_safe_alternative_matching::PreparedSafeAlternativeOutcomeRecordedV1,
    pub(super) guard:PreparedGuard,
    core:oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    fee:ReservedValue<()>,
}
impl OwnedProjectedOutcomeV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), WarmError> {
        common.consume_steps(1)?;
        tx.validate_guard(&self.guard, live)
            .map_err(SourceError::Execution)?;
        self.publication
            .receipt()
            .validate_transaction(tx)
            .map_err(SourceError::from)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.stage_identity.uses_budget(common.memory())
        {
            return Err(WarmError::from(invalid(
                "authorization projected outcome handoff",
            )));
        }
        Ok(())
    }
    pub(in crate::world) fn authorization_event<'a, M: MerkleNodeReader>(
        &'a self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<&'a crate::CapabilityAuthorizationEvent, WarmError> {
        self.validate(tx, common, live)?;
        Ok(self.publication.authorization_event())
    }
}
impl CurrentAuthorizationProjectionV1<'_> {
    /// Actual publication proof is checked before source IO. The only written
    /// cache path is selected from its genuine event and the actual Current
    /// reader's lineage, never an independent caller key or replacement JSON.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn refresh_safe_alternative_outcome<M, R, E, C, P, V, ER, CR>(
        &mut self,
        publication:crate::world::incremental_safe_alternative_matching::PreparedSafeAlternativeOutcomeRecordedV1,
        qualified: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
        current: &RuntimeRevocationCurrentV1,
        roots: &RootSetV1,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
        work: &mut AuthorizationRootSourceReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<OwnedProjectedOutcomeV1, WarmError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        P: MerkleNodeReader,
        V: ValueNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
    {
        let result = (|| {
            self.validate(overlay.transaction, common)?;
            publication
                .validate(overlay.transaction, common, live)
                .map_err(WarmError::Body)?;
            let crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
                key,
                action_id,
                ..
            } = publication.authorization_event()
            else {
                return Err(WarmError::from(invalid(
                    "authorization fixed OutcomeRecorded event",
                )));
            };
            let receipt = publication.receipt();
            receipt
                .validate_transaction(overlay.transaction)
                .map_err(SourceError::from)?;
            if receipt.before_body() != &*self.revocation_body
                || receipt.after_body() != current.body()
                || receipt.lineage() != key
            {
                return Err(WarmError::from(invalid(
                    "authorization fixed path body receipt chain",
                )));
            }
            let guard = overlay.transaction.guard();
            let actual=read::paid(roots,overlay,parent,work,common,|overlay,parent,work| {
                oasis7_world_state::incremental_runtime_revocation::read_current_safe_alternative_for_action_v1(
                    *action_id,qualified.revocation(),current,overlay,parent,&mut work.scan,&mut work.typed,&guard,live).map_err(SourceError::from)
            })?.ok_or_else(||SourceError::from(invalid("authorization actual outcome record missing")))?;
            actual
                .validate(overlay.transaction, live)
                .map_err(SourceError::from)?;
            if actual.lineage() != key {
                return Err(WarmError::from(invalid(
                    "authorization outcome actual lineage",
                )));
            }
            let after = self.data.replace_serialized(
                &[
                    AuthorizationProjectionPathV1::Field("revocation"),
                    AuthorizationProjectionPathV1::Field("safe_alternatives"),
                    AuthorizationProjectionPathV1::Field(actual.lineage()),
                ],
                actual.record(),
                common,
            )?;
            self.register_revocation(overlay.transaction, &mut work.pending_entries, common)?;
            let revocation_body = paid_body(publication.receipt().after_body(), common)?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<OwnedProjectedOutcomeV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(SourceError::from)?
                .retain(());
            let guard = overlay.transaction.guard();
            overlay
                .transaction
                .validate_guard(&guard, live)
                .map_err(SourceError::Execution)?;
            common.consume_steps(0)?;
            self.data = after;
            self.revocation_body = revocation_body;
            Ok(OwnedProjectedOutcomeV1 {
                stage_identity: self.stage_identity.clone(),
                publication,
                guard,
                core: common.work().identity(),
                fee,
            })
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                overlay.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
