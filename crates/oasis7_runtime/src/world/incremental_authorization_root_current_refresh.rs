//! Selected normal source refresh. Expected DTOs are untrusted DATA until
//! the fixed Current join succeeds; no caller root or callback can replace it.
use super::*;
use oasis7_world_state::incremental_runtime_capability_records::{
    RuntimeCapabilityRecordKeyV1, RuntimeCapabilityRecordKindV1,
};
use oasis7_world_state::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord,
    CapabilityBudgetAccount, CapabilityEffectReceiptLink, CapabilityInvocationContext,
};
pub(in crate::world) enum AuthorizationProjectionRefreshV1<'a> {
    Grant(&'a serde_json::Value),
    Audit(&'a CapabilityAuthorizationAuditReceipt),
    Invocation(&'a CapabilityInvocationContext),
    Nonce(&'a CapabilityAuthorizationNonceRecord),
    Budget(&'a CapabilityBudgetAccount),
    EffectLink(&'a CapabilityEffectReceiptLink),
}
impl CurrentAuthorizationProjectionV1<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn refresh_selected_record<M, R, E, C, V, ER, CR>(
        &mut self,
        mut outcome: OwnedProjectedOutcomeV1,
        key: &str,
        after: AuthorizationProjectionRefreshV1<'_>,
        qualified: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
        roots: &RootSetV1,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        parent: &mut WorldFieldReadContextV1<'_, M, V, ER, CR>,
        work: &mut AuthorizationRootSourceReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<OwnedProjectedOutcomeV1, warm::WarmError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        V: ValueNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
    {
        let result = (|| {
            self.validate(overlay.transaction, common)?;
            outcome.validate(overlay.transaction, common, live)?;
            if !std::sync::Arc::ptr_eq(&self.stage_identity, &outcome.stage_identity) {
                return Err(warm::WarmError::from(invalid(
                    "authorization refresh stage identity",
                )));
            }
            read::metadata(roots, common)?;
            qualified
                .validate_binding(roots)
                .map_err(SourceError::from)?;
            if !qualified.uses_budget(common.memory())
                || qualified.allocation_class() != MemoryClass::Ordinary
            {
                return Err(warm::WarmError::from(invalid(
                    "authorization refresh qualified pool/class",
                )));
            }
            let scratch = self
                .data
                .limits
                .max_depth
                .checked_add(1)
                .and_then(|n| {
                    n.checked_mul(
                        std::mem::size_of::<String>() + 40 + 2 * std::mem::align_of::<String>(),
                    )
                })
                .ok_or_else(limited)?;
            let _scratch = common
                .memory()
                .reserve(scratch, MemoryClass::Ordinary)
                .map_err(SourceError::from)?;
            let (field, raw) = match &after {
                AuthorizationProjectionRefreshV1::Grant(v) => (
                    "grants",
                    wire::encode(v, self.data.limits, common, MemoryClass::Ordinary)?,
                ),
                AuthorizationProjectionRefreshV1::Audit(v) => (
                    "receipts",
                    wire::encode(v, self.data.limits, common, MemoryClass::Ordinary)?,
                ),
                AuthorizationProjectionRefreshV1::Invocation(v) => (
                    "invocation_contexts",
                    wire::encode(v, self.data.limits, common, MemoryClass::Ordinary)?,
                ),
                AuthorizationProjectionRefreshV1::Nonce(v) => (
                    "nonce_records",
                    wire::encode(v, self.data.limits, common, MemoryClass::Ordinary)?,
                ),
                AuthorizationProjectionRefreshV1::Budget(v) => (
                    "budget_accounts",
                    wire::encode(v, self.data.limits, common, MemoryClass::Ordinary)?,
                ),
                AuthorizationProjectionRefreshV1::EffectLink(v) => (
                    "effect_receipt_links",
                    wire::encode(v, self.data.limits, common, MemoryClass::Ordinary)?,
                ),
            };
            let node = cbor::parse(&raw, self.data.limits, common, MemoryClass::Ordinary)?;
            match &after {
                AuthorizationProjectionRefreshV1::Grant(_) => sources::grant(
                    qualified.grants(),
                    roots,
                    key,
                    &node,
                    self.data.limits,
                    overlay,
                    parent,
                    work,
                    common,
                    live,
                )?,
                AuthorizationProjectionRefreshV1::Audit(_) => sources::audit(
                    qualified.authorization_records(),
                    roots,
                    key,
                    &node,
                    self.data.limits,
                    overlay,
                    parent,
                    work,
                    common,
                    live,
                )?,
                AuthorizationProjectionRefreshV1::Invocation(_) => sources::invocation(
                    qualified.authorization_records(),
                    roots,
                    key,
                    &node,
                    self.data.limits,
                    overlay,
                    parent,
                    work,
                    common,
                    live,
                )?,
                _ => {
                    let kind = match &after {
                        AuthorizationProjectionRefreshV1::Nonce(_) => {
                            RuntimeCapabilityRecordKindV1::Nonce
                        }
                        AuthorizationProjectionRefreshV1::Budget(_) => {
                            RuntimeCapabilityRecordKindV1::Budget
                        }
                        AuthorizationProjectionRefreshV1::EffectLink(_) => {
                            RuntimeCapabilityRecordKindV1::EffectLink
                        }
                        _ => {
                            return Err(warm::WarmError::from(invalid(
                                "authorization fixed refresh source",
                            )));
                        }
                    };
                    sources::capability(
                        qualified.capabilities(),
                        roots,
                        RuntimeCapabilityRecordKeyV1 { kind, id: key },
                        &node,
                        self.data.limits,
                        overlay,
                        parent,
                        work,
                        common,
                        live,
                    )?;
                }
            }
            self.register_record(
                field,
                key,
                overlay.transaction,
                &mut work.pending_entries,
                common,
            )?;
            let path = [
                AuthorizationProjectionPathV1::Field(field),
                AuthorizationProjectionPathV1::Field(key),
            ];
            let root = patch(
                &self.data.root,
                &path,
                Some(node),
                self.data.limits,
                common,
                MemoryClass::Ordinary,
            )?;
            common.consume_steps(0)?;
            self.data.root = root;
            outcome.guard = overlay.transaction.guard();
            Ok(outcome)
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
