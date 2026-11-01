//! Same dedicated context KEK; disjoint V2 scope AAD. No first-use key creation.
use super::*;
use oasis7_custody_core::{approval::ApprovedWorldV1, context_body_v2::*};
impl ContextBodyCrypto {
    fn check_fullsigned(&self, scope: &ContextBodyScopeV2) -> Result<()> {
        scope.validate().map_err(|_| rejected())?;
        let now = js_sys::Date::now();
        if scope.identity_scope.context.identity.environment != self.environment
            || !now.is_finite()
            || now < 0.0
            || now.fract() != 0.0
            || now > 9_007_199_254_740_991.0
            || now as u64 >= scope.write_deadline.0
        {
            return Err(rejected());
        }
        Ok(())
    }
    /// Invoke once only after paid/current factory admission. Save the actual
    /// envelope digest/wire length and once claim before outbound immutable PUT.
    pub async fn seal_fullsigned_v2(
        &self,
        body: &ValidatedFullSignedContextBodyV2,
    ) -> Result<ContextBodyRecordV2> {
        self.check_fullsigned(body.scope())?;
        let context = body.scope().envelope_context().map_err(|_| rejected())?;
        let envelope = envelope_crypto::seal(
            &context,
            self.wrapping_version,
            &self.kek,
            body.canonical_bytes(),
        )
        .await
        .map_err(|_| rejected())?;
        self.check_fullsigned(body.scope())?;
        let record = ContextBodyRecordV2 {
            scope: body.scope().clone(),
            envelope,
        };
        record.validate().map_err(|_| rejected())?;
        Ok(record)
    }
    /// Recovery reads the original record only. This method never re-seals,
    /// PUTs, grants use, or emits plaintext to Account.
    pub async fn open_fullsigned_v2(
        &self,
        reference: &AttestedSlotRefV2,
        record: &ContextBodyRecordV2,
        trusted_world: &ApprovedWorldV1,
    ) -> Result<ValidatedFullSignedContextBodyV2> {
        let scope = &reference.scope;
        self.check_fullsigned(scope)?;
        reference
            .validate_provenance(trusted_world)
            .map_err(|_| rejected())?;
        record.validate_for(scope).map_err(|_| rejected())?;
        if record.envelope.wrapping_key_version != self.wrapping_version
            || record.envelope_digest().map_err(|_| rejected())? != reference.envelope_digest
            || record.wire_bytes().map_err(|_| rejected())?.len() as u64
                != reference.envelope_wire_bytes.0
        {
            return Err(rejected());
        }
        let context = scope.envelope_context().map_err(|_| rejected())?;
        let bytes = envelope_crypto::open(&record.envelope, &context, &self.kek)
            .await
            .map_err(|_| rejected())?;
        self.check_fullsigned(scope)?;
        validate_recovered_fullsigned_storage_v2(reference, trusted_world, bytes.as_bytes())
            .map_err(|_| rejected())
    }
}

/// Identity-local pending original. Account gets only `reference()`. This is
/// not a dispatch permit; a real durable Account once claim is still required.
pub struct SealedFullSignedSlotV2 {
    record: ContextBodyRecordV2,
    reference: AttestedSlotRefV2,
}
impl SealedFullSignedSlotV2 {
    pub fn reference(&self) -> &AttestedSlotRefV2 {
        &self.reference
    }
    /// Identity storage caller only. Never serialized into an Account RPC.
    pub fn original_record(&self) -> &ContextBodyRecordV2 {
        &self.record
    }
}
impl ContextBodyCrypto {
    pub async fn prepare_fullsigned_slot_v2(
        &self,
        scope: ContextBodyScopeV2,
        world: &ApprovedWorldV1,
        q:&oasis7_client_api::world_service::task_context_projection::TaskContextPrepareAttestationRequestV2,
        delivery:&oasis7_client_api::world_service::task_context_projection::WorldContextAttestationV2<
            oasis7_client_api::world_service::task_context_projection::TaskContextPreparedDeliveryV2>,
    ) -> Result<SealedFullSignedSlotV2> {
        use oasis7_custody_core::approval_admission::{
            fullsigned_prepared_storage_checks_v2, validate_fullsigned_prepared_checked_v2,
        };
        self.check_fullsigned(&scope)?;
        let checks = fullsigned_prepared_storage_checks_v2(&scope, world, q, delivery)
            .map_err(|_| rejected())?;
        let mut verified = crate::context_body_signatures::verify(checks).await?;
        self.check_fullsigned(&scope)?;
        let body =
            validate_fullsigned_prepared_checked_v2(scope, world, q, delivery, &mut verified)
                .map_err(|_| rejected())?;
        let record = self.seal_fullsigned_v2(&body).await?;
        let reference = body
            .reference(
                AttestedContextEvidenceV2::Prepared {
                    request: Box::new(q.clone()),
                    projection: Box::new(delivery.payload.projection.clone()),
                },
                &record,
                world,
            )
            .map_err(|_| rejected())?;
        Ok(SealedFullSignedSlotV2 { record, reference })
    }
    pub async fn capture_fullsigned_slot_v2(
        &self,
        scope: ContextBodyScopeV2,
        world: &ApprovedWorldV1,
        prior: &AttestedSlotRefV2,
        q:&oasis7_client_api::world_service::task_context_projection::TaskContextGetAttestationRequestV2,
        delivery:&oasis7_client_api::world_service::task_context_projection::WorldContextAttestationV2<
            oasis7_client_api::world_service::task_context_projection::TaskContextCapturedDeliveryV2>,
    ) -> Result<SealedFullSignedSlotV2> {
        use oasis7_custody_core::approval_admission::{
            fullsigned_captured_storage_checks_v2, validate_fullsigned_captured_checked_v2,
        };
        self.check_fullsigned(&scope)?;
        let checks = fullsigned_captured_storage_checks_v2(&scope, world, prior, q, delivery)
            .map_err(|_| rejected())?;
        let mut verified = crate::context_body_signatures::verify(checks).await?;
        self.check_fullsigned(&scope)?;
        let body = validate_fullsigned_captured_checked_v2(
            scope,
            world,
            prior,
            q,
            delivery,
            &mut verified,
        )
        .map_err(|_| rejected())?;
        let record = self.seal_fullsigned_v2(&body).await?;
        let reference = body
            .reference(
                AttestedContextEvidenceV2::Captured {
                    request: Box::new(q.clone()),
                    projection: Box::new(delivery.payload.projection.clone()),
                },
                &record,
                world,
            )
            .map_err(|_| rejected())?;
        Ok(SealedFullSignedSlotV2 { record, reference })
    }
}
