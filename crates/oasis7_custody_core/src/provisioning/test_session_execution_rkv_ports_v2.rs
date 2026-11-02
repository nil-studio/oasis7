//! Four explicit Account/service/Actor physical attempts. No Current/World edge is covered.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareRkvHopV2 {
    AccountToExecutorService,
    ExecutorServiceToActor,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrepareRkvGraphV2 {
    ExecutorServiceAndActorV2,
}
const HOPS: [ExecutionPrepareRkvHopV2; 2] = [
    ExecutionPrepareRkvHopV2::AccountToExecutorService,
    ExecutionPrepareRkvHopV2::ExecutorServiceToActor,
];
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareRkvTransportV2 {
    pub hop: ExecutionPrepareRkvHopV2,
    pub operation: ExecutionPrepareSignerOperationV2,
    pub factory_key: String,
    pub proof_digest: String,
    pub claimed_at: UnixMillis,
}
impl core::fmt::Debug for ExecutionPrepareRkvTransportV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionPrepareRkvTransportV2(<paid physical attempt>)")
    }
}
/// Only the successful first Account transaction returns this. Historical
/// proofs never recreate it; the store must commit before releasing the value.
pub struct ExecutionPrepareDispatchV2 {
    permits: SignedExecutionPreparePermitsV2,
    port_ids: [String; 4],
    bundle: ExecutionSigningBundleV2,
    accepted_ordinal: u64,
}
impl ExecutionPrepareDispatchV2 {
    pub fn accepted_ordinal(&self) -> u64 {
        self.accepted_ordinal
    }
    pub fn permits(&self) -> &SignedExecutionPreparePermitsV2 {
        &self.permits
    }
    pub fn port_ids(&self) -> &[String; 4] {
        &self.port_ids
    }
    pub fn into_attempts(self) -> [ExecutionPreparePhysicalAttemptV2; 2] {
        let [init_service, init_actor, sign_service, sign_actor] = self.port_ids;
        [
            ExecutionPreparePhysicalAttemptV2 {
                port_ids: [init_service, init_actor],
                proof: self.permits.initialize,
                bundle: self.bundle.clone(),
            },
            ExecutionPreparePhysicalAttemptV2 {
                port_ids: [sign_service, sign_actor],
                proof: self.permits.sign,
                bundle: self.bundle,
            },
        ]
    }
    pub(super) fn new(
        record: &ExecutionPrepareServiceRecordV2,
        bundle: ExecutionSigningBundleV2,
    ) -> Self {
        Self {
            permits: record.permits.clone(),
            port_ids: record.port_ids.clone(),
            bundle,
            accepted_ordinal: record.accepted_ordinal,
        }
    }
}
pub fn execution_prepare_physical_port_id_v2(
    proof: &SignedAccountExecutionPreparePermitV2,
    hop: ExecutionPrepareRkvHopV2,
) -> Result<String> {
    Ok(format!(
        "execution-rkv-v2:{}:{}:{}:{}",
        digest(&(
            "execution-rkv-session-v2",
            &proof.permit.session_id,
            &proof.permit.approved_input_digest
        ))?,
        digest(&(
            "execution-rkv-factory-v2",
            &proof.permit.physical_ingress_key
        ))?,
        digest(&("execution-rkv-original-v2", proof))?,
        digest(&(
            "execution-rkv-operation-v2",
            proof.permit.operation,
            hop,
            &proof.permit.recipient_route
        ))?
    ))
}
pub(super) fn port_id(
    s: &TestSessionAuthorityV2,
    factory: &str,
    proof: &SignedAccountExecutionPreparePermitV2,
    hop: ExecutionPrepareRkvHopV2,
) -> Result<String> {
    if s.input_digest()? != proof.permit.approved_input_digest
        || factory != proof.permit.physical_ingress_key
    {
        return Err(invalid());
    }
    execution_prepare_physical_port_id_v2(proof, hop)
}
pub(super) fn validate_ports(
    s: &TestSessionAuthorityV2,
    record: &ExecutionPrepareServiceRecordV2,
) -> Result<()> {
    let proofs = [&record.permits.initialize, &record.permits.sign];
    let mut unique = std::collections::BTreeSet::new();
    for (operation_index, proof) in proofs.into_iter().enumerate() {
        for (hop_index, hop) in HOPS.into_iter().enumerate() {
            let key = &record.port_ids[operation_index * 2 + hop_index];
            if key.len() <= 256
                || key != &port_id(s, &proof.permit.physical_ingress_key, proof, hop)?
                || !unique.insert(key)
            {
                return Err(invalid());
            }
            let Some(SessionPurposeTransportV1::ExecutionPrepareRkvV2(p)) =
                s.purpose_transports.get(key)
            else {
                return Err(invalid());
            };
            if p.hop != hop
                || p.operation != proof.permit.operation
                || p.factory_key != proof.permit.physical_ingress_key
                || p.proof_digest != digest(&("oasis7.custody.execution-signer.permit.v2", proof))?
                || p.claimed_at != record.recorded_at
            {
                return Err(invalid());
            }
        }
    }
    if s.purpose_transports
        .values()
        .filter(|v| matches!(v, SessionPurposeTransportV1::ExecutionPrepareRkvV2(_)))
        .count()
        != 4
    {
        return Err(invalid());
    }
    Ok(())
}
pub(in crate::provisioning) fn parent_valid(
    s: &TestSessionAuthorityV2,
    key: &str,
    port: &ExecutionPrepareRkvTransportV2,
) -> Result<()> {
    let record = s
        .execution_book
        .as_ref()
        .and_then(|b| b.prepare_service_record())
        .ok_or_else(invalid)?;
    validate_ports(s, record)?;
    if !record.port_ids.iter().any(|id| id == key)
        || ![&record.permits.initialize, &record.permits.sign]
            .into_iter()
            .any(|p| {
                p.permit.operation == port.operation
                    && p.permit.physical_ingress_key == port.factory_key
            })
    {
        return Err(invalid());
    }
    Ok(())
}

/// One actual fixed RKV RPC. No Clone, Deserialize or public constructor.
pub struct ExecutionPreparePhysicalAttemptV2 {
    port_ids: [String; 2],
    proof: SignedAccountExecutionPreparePermitV2,
    bundle: ExecutionSigningBundleV2,
}
impl ExecutionPreparePhysicalAttemptV2 {
    pub fn port_ids(&self) -> &[String; 2] {
        &self.port_ids
    }
    pub fn proof(&self) -> &SignedAccountExecutionPreparePermitV2 {
        &self.proof
    }
    pub fn into_parts(
        self,
    ) -> (
        [String; 2],
        SignedAccountExecutionPreparePermitV2,
        ExecutionSigningBundleV2,
    ) {
        (self.port_ids, self.proof, self.bundle)
    }
}
