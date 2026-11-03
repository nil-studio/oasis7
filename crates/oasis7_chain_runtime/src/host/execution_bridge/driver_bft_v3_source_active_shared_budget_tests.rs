//! Genuine original-input accounting across Pending -> another Ready -> cold retry.
use super::*;
use oasis7_node::{
    NodeBftExecutionContextV3, NodeBftPrivateCandidatePackageV3, NodeBftPrivateCandidateV3,
    NodeBftProtectedCandidateAdmissionV3, NodeExecutionHook,
};
use std::collections::BTreeSet;
type Input = BftPrivateRetentionAdmissionInputV1;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    schema_version: u32,
    domain: String,
    origin: bool,
    ledger_origin_digest: String,
    material_identity: String,
    instance_id: String,
    height: u64,
    value: Option<BftHeaderValueV3>,
    actions_cbor: Vec<u8>,
    references: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_reservation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_original_data_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_activation: Option<serde_cbor::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protected_source_origin_ref: Option<String>,
}
fn object(input: &Input, kind: BftPrivateRetentionInputObjectKindV1) -> &[u8] {
    &input
        .objects
        .iter()
        .find(|v| v.kind == kind)
        .expect("genuine exact input role")
        .bytes
}
fn pin_bytes(data: &Data) -> u64 {
    #[derive(serde::Serialize)]
    struct Receipt {
        pins: BTreeSet<String>,
    }
    let bytes = serde_cbor::to_vec(data).unwrap();
    let pins = data
        .references
        .iter()
        .cloned()
        .chain(data.publication_reservation_ref.iter().cloned())
        .chain(data.publication_original_data_ref.iter().cloned())
        .chain(data.protected_source_origin_ref.iter().cloned())
        .chain([blake3_hex(&bytes)])
        .collect();
    2 * serde_json::to_vec(&Receipt { pins }).unwrap().len() as u64
}
// Independent wire-size oracle: no production planner/budget helper is called.
fn quote_without_previous_input(input: &Input, baseline: &BTreeSet<String>) -> u64 {
    let raw = object(input, BftPrivateRetentionInputObjectKindV1::OriginalData);
    let data: Data = serde_cbor::from_slice(raw).unwrap();
    assert!(serde_cbor::to_vec(&data).unwrap() == raw);
    assert_eq!(data.schema_version, 12);
    assert!(data.source_activation.is_none() && data.publication_original_data_ref.is_none());
    let metadata = object(
        input,
        BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
    );
    #[derive(serde::Deserialize)]
    struct Cost {
        publication_bytes: u64,
    }
    let publication: Cost = serde_cbor::from_slice(metadata).unwrap();
    let dependencies: u64 = input
        .objects
        .iter()
        .filter(|v| {
            v.kind == BftPrivateRetentionInputObjectKindV1::Dependency
                && !baseline.contains(&v.reference.digest)
        })
        .map(|v| v.bytes.len() as u64)
        .sum();
    let mut choice = data.clone();
    choice.schema_version = 13;
    choice.domain = "oasis7.formal.bft.node_private_active_selection.v13".into();
    choice.actions_cbor.clear();
    choice.references = vec![blake3_hex(raw)];
    choice.publication_reservation_ref = None;
    choice.protected_source_origin_ref = None;
    2 * raw.len() as u64
        + publication.publication_bytes
        + metadata.len() as u64
        + pin_bytes(&data)
        + 2 * serde_cbor::to_vec(&choice).unwrap().len() as u64
        + pin_bytes(&choice)
        + dependencies
}

pub(super) struct PendingOriginal {
    reference: BftPrivateRetentionAdmissionInputRefV1,
    original: Arc<Input>,
    fact: BftPrivateRetentionFactV1,
    price: u64,
    deadline: Instant,
}
impl PendingOriginal {
    pub(super) fn original_deadline(&self) -> Instant {
        self.deadline
    }
}

/// Parent fixture supplies two independently actual-executed h5 alternatives.
/// B is an external Received package, never a replacement local selection.
pub(super) fn reserve_a_then_ready_b(
    hook: &mut dyn NodeExecutionHook,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    backend: &mut BftPrivateRetentionMemoryBackendV1,
    context: &NodeBftExecutionContextV3,
    a: &NodeBftPrivateCandidateV3,
    b: &NodeBftPrivateCandidatePackageV3,
    namespace: &str,
    baseline: &BTreeSet<String>,
    deadline: Instant,
) -> PendingOriginal {
    let before = backend
        .load_private_retention_current(&binding.digest().unwrap())
        .unwrap();
    assert!(!before
        .retention
        .as_ref()
        .unwrap()
        .map
        .entries
        .iter()
        .any(|v| v.key.ledger_origin_digest == namespace));
    assert_ne!(
        a.public_value.block_id().unwrap(),
        b.candidate.public_value.block_id().unwrap()
    );
    let error = hook
        .retain_bft_v3_protected_candidate(
            binding,
            &mut super::ready_fault::StopBeforeReady::new(lease),
            context,
            a,
            deadline,
        )
        .unwrap_err();
    assert!(
        error.contains("stopped before first Ready"),
        "fault must be the real post-Reserve Ready boundary; actual rejection: {error}"
    );
    let pending = backend
        .load_private_retention_current(&binding.digest().unwrap())
        .unwrap();
    let entry = pending
        .retention
        .as_ref()
        .unwrap()
        .map
        .entries
        .iter()
        .find(|v| {
            v.key.ledger_origin_digest == namespace
                && v.key.header_id == a.public_value.block_id().unwrap()
        })
        .unwrap();
    assert_eq!(
        entry.lifecycle,
        BftPrivateRetentionLifecycleV1::ReservedPending
    );
    let reference = BftPrivateRetentionAdmissionInputRefV1::from_entry(
        &pending.retention.as_ref().unwrap().map.source,
        entry,
    )
    .unwrap();
    let original = pending
        .inputs
        .get(&reference.admission_digest)
        .unwrap()
        .clone();
    reference.validate_input(&original).unwrap();
    let data: Data = serde_cbor::from_slice(object(
        &original,
        BftPrivateRetentionInputObjectKindV1::OriginalData,
    ))
    .unwrap();
    assert_eq!(data.schema_version, 11);
    let a_refs: BTreeSet<_> = original
        .objects
        .iter()
        .filter(|v| v.kind == BftPrivateRetentionInputObjectKindV1::Dependency)
        .map(|v| v.reference.digest.clone())
        .collect();
    let shared_bytes: u64 = b
        .dependency_nodes
        .iter()
        .filter(|(digest, _)| a_refs.contains(digest) && !baseline.contains(digest))
        .map(|(_, bytes)| bytes.len() as u64)
        .sum();
    assert!(
        shared_bytes > 0,
        "real business alternatives must actually share a new dependency; never insert padding"
    );
    assert!(matches!(
        hook.retain_received_bft_v3_protected_candidate(binding, lease, context, b, deadline)
            .unwrap(),
        NodeBftProtectedCandidateAdmissionV3::Retained(_)
    ));
    let after = backend
        .load_private_retention_current(&binding.digest().unwrap())
        .unwrap();
    let ledger = after.retention.as_ref().unwrap();
    assert_eq!(
        ledger
            .map
            .entries
            .iter()
            .find(|v| v.key == reference.key)
            .unwrap()
            .lifecycle,
        BftPrivateRetentionLifecycleV1::ReservedPending
    );
    let b_entry = ledger
        .map
        .entries
        .iter()
        .find(|v| {
            v.key.header_id == b.candidate.public_value.block_id().unwrap()
                && v.key.ledger_origin_digest == namespace
        })
        .unwrap();
    assert_eq!(b_entry.lifecycle, BftPrivateRetentionLifecycleV1::Ready);
    let b_reference =
        BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, b_entry).unwrap();
    let b_input = after.inputs.get(&b_reference.admission_digest).unwrap();
    assert_eq!(b_input.publication_reservation_bytes.checked_add(shared_bytes).unwrap(), quote_without_previous_input(b_input, baseline),
        "B must deduct exact already-charged Pending A dependencies, independently of Ready/WAL status");
    PendingOriginal {
        reference,
        price: original.publication_reservation_bytes,
        original,
        fact: entry.fact.clone(),
        deadline,
    }
}

/// Called only after the parent drops every old hook, lease, service and Noise
/// owner and reopens the same backend/root/namespace with genuine original pins.
pub(super) fn assert_cold_a_kept_original(
    backend: &mut BftPrivateRetentionMemoryBackendV1,
    binding: &BftSignerBindingV3,
    original: &PendingOriginal,
) {
    let after = backend
        .load_private_retention_current(&binding.digest().unwrap())
        .unwrap();
    let entries = &after.retention.as_ref().unwrap().map.entries;
    assert_eq!(
        entries
            .iter()
            .filter(|v| v.key == original.reference.key)
            .count(),
        1
    );
    let entry = entries
        .iter()
        .find(|v| v.key == original.reference.key)
        .unwrap();
    assert_eq!(entry.lifecycle, BftPrivateRetentionLifecycleV1::Ready);
    assert!(
        entry.fact == original.fact,
        "cold retry preserves the exact original protected fact"
    );
    let input = after
        .inputs
        .get(&original.reference.admission_digest)
        .unwrap();
    assert!(
        Arc::ptr_eq(input, &original.original),
        "same backend retains the original Arc without replacement"
    );
    assert_eq!(input.publication_reservation_bytes, original.price);
    original.reference.validate_input(input).unwrap();
}

#[test]
fn actual_source_active_pending_shared_dependency_price_and_cold_original_retry() {
    super::run_case(super::CaseMode::SharedBudget);
}
