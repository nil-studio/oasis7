//! Actual accepted GEN -> failed Frozen preparation -> original-owner recovery.
//! The request bytes are content fixtures, not a Node decision or business tick.
use super::*;
use oasis7_distfs::controlled_authority::fencing::{
    RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FORMAL_ARCHIVE_OBJECT_LIMIT, FileFormalArchiveSourceV1, FormalArchiveReadWorkV1,
    FormalArchiveSourceV1, FormalRequestStateV1, NormalFrozenPairFailureV1,
    PaidAcceptedGenesisHostSourceV1, PaidNormalFrozenEndpointV1, PaidNormalFrozenPairV1,
    join_normal_frozen_endpoints_bounded,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalEndpointPairPublicationRefsV1, FormalEndpointPublicationRefsV1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalFrozenBodyV1, FormalFrozenCertificateV1, FormalFrozenReceiptV1,
    IncrementalFrozenMaterialWorkV1, NativeHotBodyV1, SignedFormalFrozenV1,
    frozen_material_digest_bounded_v1,
};
use oasis7_formal_protocol::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use oasis7_formal_protocol::incremental_artifacts::{
    ExecutionManifestModeV1, IwsExecutionManifestV1, OriginalArtifactRefV1,
};
use std::{collections::BTreeMap, fs::File, path::Path};

fn next_frozen(fixture: &AcceptedGenesisFixture<'_>) -> (SignedFormalFrozenV1, ClosedRecord) {
    let parent = fixture.expected;
    let original = &fixture.candidate.material;
    let manifest_text = &original.roots[&ArtifactRole::ExecutionManifest];
    let manifest_bytes = &original
        .objects
        .iter()
        .find(|object| &object.content_hash == manifest_text)
        .unwrap()
        .bytes;
    let mut manifest = IwsExecutionManifestV1::from_canonical_bytes(manifest_bytes).unwrap();
    let input_bytes = b"Frozen preparation recovery input".to_vec();
    let continuation_bytes = b"Frozen preparation recovery continuation".to_vec();
    let input = *blake3::hash(&input_bytes).as_bytes();
    let continuation = OriginalArtifactRefV1 {
        id: *blake3::hash(&continuation_bytes).as_bytes(),
        byte_len: continuation_bytes.len() as u64,
    };
    let operation = *blake3::hash(b"Frozen preparation recovery operation").as_bytes();
    manifest.mode = ExecutionManifestModeV1::Execution {
        parent_commit: hex::decode(
            &parent.record().artifact_roots[&ArtifactRole::IncrementalCommitBody],
        )
        .unwrap()
        .try_into()
        .unwrap(),
        parent_roots: hex::decode(&parent.record().after_state_root)
            .unwrap()
            .try_into()
            .unwrap(),
        operation,
        input,
        continuation,
    };
    let manifest_bytes = manifest.canonical_bytes().unwrap();
    let manifest_id = *blake3::hash(&manifest_bytes).as_bytes();
    let mut roots = BTreeMap::new();
    for role in [
        ArtifactRole::IncrementalGenesisConfig,
        ArtifactRole::IncrementalRootSet,
        ArtifactRole::IncrementalCommitBody,
        ArtifactRole::IncrementalHistoryPrefix,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
        ArtifactRole::NativeExecutionEnvironment,
    ] {
        roots.insert(role, parent.record().artifact_roots[&role].clone());
    }
    roots.insert(ArtifactRole::Input, hex::encode(input));
    roots.insert(ArtifactRole::ExecutionManifest, hex::encode(manifest_id));
    // The actual GEN CommitBody references every retained old original. The
    // retention ledger stays reachable data, never an extra Frozen root role.
    let mut objects = original.objects.clone();
    let mut manifest_references = vec![
        hex::encode(manifest.policy.id),
        hex::encode(continuation.id),
    ];
    manifest_references.sort();
    manifest_references.dedup();
    for (id, bytes, references) in [
        (input, input_bytes, vec![]),
        (continuation.id, continuation_bytes, vec![]),
        (manifest_id, manifest_bytes, manifest_references),
    ] {
        objects.push(ArtifactObject {
            content_hash: hex::encode(id),
            bytes,
            references,
        });
    }
    objects.sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
    let material = ClosedRecord {
        payload_digest: hex::encode(input),
        before_state_root: parent.record().after_state_root.clone(),
        after_state_root: parent.record().after_state_root.clone(),
        roots,
        objects,
    };
    material.validate_iws().unwrap();
    let material_id =
        frozen_material_digest_bounded_v1(&material, fixture.memory, &mut (64 << 20), || false)
            .unwrap();
    let body = FormalFrozenBodyV1 {
        schema_version: formal::incremental::IWS_FROZEN_SCHEMA,
        world: parent.record().world.clone(),
        authority_id: fixture.authority.authority_id().unwrap(),
        epoch: fixture.authority.definition().epoch,
        activation_ref: fixture.authority.activation_ref().to_owned(),
        parent: parent.head(),
        next_position: parent.record().position + 1,
        request_digest: hex::encode(operation),
        payload_digest: hex::encode(input),
        runtime_manifest_digest: hex::encode(manifest_id),
        governing_manifest_digest: parent.record().governing_manifest_digest.clone(),
        execution_policy_digest: parent.record().execution_policy_digest.clone(),
        pre_state_root: parent.record().after_state_root.clone(),
        frozen_material_digest: hex::encode(material_id),
        handoff: None,
    };
    let signature_hex = signed::sign(formal::frozen_signing_bytes(&body).unwrap(), 4);
    (
        SignedFormalFrozenV1 {
            body,
            signature_hex,
        },
        material,
    )
}

struct RecoveryContext<'a> {
    input: &'a SignedFormalFrozenV1,
    material: &'a ClosedRecord,
    primary_provider: &'a RemoteMonotonicEndpointProvider,
    replica_provider: &'a RemoteMonotonicEndpointProvider,
    policy: &'a TrustedFencingPolicyV1,
    memory: &'a MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    authority: &'a formal::VerifiedAuthorityV1,
    expected: &'a formal::VerifiedFormalCommitV1,
    primary_root: &'a Path,
    replica_root: &'a Path,
    primary_hot: &'a Path,
    replica_hot: &'a Path,
}

fn publication_guard(root: &Path) -> File {
    let guard = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("formal-objects-v1/.physical-publication-guard-v1"))
        .unwrap();
    guard.try_lock().unwrap();
    guard
}

// Keep phases out of the one test callback's frame. These are ordinary calls
// on the default test thread, not additional threads or an enlarged stack.
#[inline(never)]
fn accepted_sources(
    primary: FileFormalEndpoint,
    replica: FileFormalEndpoint,
    c: &RecoveryContext<'_>,
) -> (
    PaidAcceptedGenesisHostSourceV1,
    PaidAcceptedGenesisHostSourceV1,
) {
    let primary = primary
        .into_accepted_genesis_host_source_bounded(
            c.primary_provider,
            c.policy,
            c.memory,
            c.work,
            &mut || false,
        )
        .unwrap();
    let replica = replica
        .into_accepted_genesis_host_source_bounded(
            c.replica_provider,
            c.policy,
            c.memory,
            c.work,
            &mut || false,
        )
        .unwrap();
    (primary, replica)
}

#[inline(never)]
fn recover_primary_prepare_failure(
    source: PaidAcceptedGenesisHostSourceV1,
    c: &RecoveryContext<'_>,
    material_work: &mut IncrementalFrozenMaterialWorkV1,
) -> PaidNormalFrozenEndpointV1 {
    let before_hot = std::fs::read(c.primary_hot).unwrap();
    // A separate actual writer FD rejects preparation without changing bytes.
    let guard = publication_guard(c.primary_root);
    let failure = source
        .freeze_normal_bounded(
            c.input,
            c.material,
            FormalEndpointPublicationRefsV1 {
                provider: c.primary_provider,
                policy: c.policy,
                memory: c.memory,
                work: c.work,
            },
            material_work,
            &mut || false,
        )
        .err()
        .expect("held physical writer must reject preparation");
    assert!(
        failure.to_string().contains("physical publication busy"),
        "{failure}"
    );
    assert_eq!(std::fs::read(c.primary_hot).unwrap(), before_hot);
    let owner_bytes = c.memory.used();
    let remaining = (c.work.remaining_steps(), c.work.remaining_read_bytes());
    let foreign_pool = MemoryBudget::new(512 << 20, 0).unwrap();
    let failure = failure
        .recover_bounded(
            c.primary_provider,
            c.policy,
            &foreign_pool,
            c.work,
            &mut || false,
        )
        .err()
        .expect("recovery must reject a foreign memory pool");
    assert!(
        failure
            .to_string()
            .contains("normal Frozen recovery pool/Core")
    );
    assert_eq!(c.memory.used(), owner_bytes);
    assert_eq!(foreign_pool.used(), 0);
    assert_eq!(
        (c.work.remaining_steps(), c.work.remaining_read_bytes()),
        (remaining.0 - 1, remaining.1)
    );
    let remaining = (c.work.remaining_steps(), c.work.remaining_read_bytes());
    let foreign_core = FormalArchiveReadWorkV1::new(remaining.0, remaining.1).unwrap();
    let failure = failure
        .recover_bounded(
            c.primary_provider,
            c.policy,
            c.memory,
            &foreign_core,
            &mut || false,
        )
        .err()
        .expect("equal balances must not replace the original Core");
    assert!(
        failure
            .to_string()
            .contains("normal Frozen recovery pool/Core")
    );
    assert_eq!(c.memory.used(), owner_bytes);
    assert_eq!(
        (c.work.remaining_steps(), c.work.remaining_read_bytes()),
        remaining
    );
    assert_eq!(
        (
            foreign_core.remaining_steps(),
            foreign_core.remaining_read_bytes()
        ),
        (remaining.0 - 1, remaining.1)
    );
    let failure = failure
        .recover_bounded(c.primary_provider, c.policy, c.memory, c.work, &mut || true)
        .err()
        .expect("cancelled recovery must retain the original owners");
    assert!(
        failure
            .to_string()
            .contains("bounded archive read cancelled")
    );
    assert_eq!(c.memory.used(), owner_bytes);
    assert_eq!(
        (c.work.remaining_steps(), c.work.remaining_read_bytes()),
        remaining
    );
    assert_eq!(std::fs::read(c.primary_hot).unwrap(), before_hot);
    // Retry still encounters the held writer, preserving its original owners.
    let failure = failure
        .recover_bounded(c.primary_provider, c.policy, c.memory, c.work, &mut || {
            false
        })
        .err()
        .expect("the held physical writer still prevents preparation");
    assert!(failure.to_string().contains("physical publication busy"));
    assert_eq!(std::fs::read(c.primary_hot).unwrap(), before_hot);
    assert!(c.work.remaining_steps() < remaining.0);
    assert!(c.work.remaining_read_bytes() < remaining.1);
    let remaining = (c.work.remaining_steps(), c.work.remaining_read_bytes());
    drop(guard);
    // No source, material, reservation or replacement Core enters recovery.
    let recovered = failure
        .recover_bounded(c.primary_provider, c.policy, c.memory, c.work, &mut || {
            false
        })
        .unwrap();
    assert!(recovered.uses_budget(c.memory));
    assert_eq!(recovered.reservation().body(), &c.input.body);
    assert_eq!(recovered.receipt().body.role, EndpointRole::Primary);
    assert_eq!(
        recovered.receipt().body.authority_id,
        c.input.body.authority_id
    );
    assert_eq!(
        recovered.receipt().body.reservation_id,
        c.input.body.reservation_id().unwrap()
    );
    assert_ne!(std::fs::read(c.primary_hot).unwrap(), before_hot);
    assert!(c.work.remaining_steps() < remaining.0);
    assert!(c.work.remaining_read_bytes() < remaining.1);
    recovered
}

#[inline(never)]
fn freeze_replica(
    source: PaidAcceptedGenesisHostSourceV1,
    c: &RecoveryContext<'_>,
    material_work: &mut IncrementalFrozenMaterialWorkV1,
) -> PaidNormalFrozenEndpointV1 {
    source
        .freeze_normal_bounded(
            c.input,
            c.material,
            FormalEndpointPublicationRefsV1 {
                provider: c.replica_provider,
                policy: c.policy,
                memory: c.memory,
                work: c.work,
            },
            material_work,
            &mut || false,
        )
        .unwrap()
}

// Assertion DATA from actual Hot and pending originals, never an authority
// constructor. Keep the large RequestState decode in its own short-lived frame.
#[inline(never)]
fn observed_certificate(
    root: &Path,
    hot_bytes: &[u8],
    receipt: &FormalFrozenReceiptV1,
    c: &RecoveryContext<'_>,
) -> Option<FormalFrozenCertificateV1> {
    #[derive(serde::Deserialize)]
    struct ObservedHot {
        body: NativeHotBodyV1,
    }
    let hot: ObservedHot = serde_cbor::from_slice(hot_bytes).unwrap();
    let archive = FileFormalArchiveSourceV1::new(root.to_path_buf()).unwrap();
    let raw = archive
        .read_object_bounded(
            hot.body
                .pending
                .as_deref()
                .expect("published pending original"),
            usize::try_from(FORMAL_ARCHIVE_OBJECT_LIMIT).unwrap(),
            c.memory,
            c.work,
            &mut || false,
        )
        .unwrap()
        .expect("published pending resolves in the actual archive");
    let state: FormalRequestStateV1 = serde_cbor::from_slice(raw.as_slice()).unwrap();
    let FormalRequestStateV1::Frozen {
        input,
        material,
        receipt: actual_receipt,
        certificate,
    } = state
    else {
        panic!("actual pending must remain the same Frozen request");
    };
    assert_eq!(&input, c.input);
    assert_eq!(&material, c.material);
    assert_eq!(actual_receipt.as_ref(), Some(receipt));
    certificate
}

struct PartialJoin {
    failure: NormalFrozenPairFailureV1,
    replica_guard: File,
    primary_receipt: FormalFrozenReceiptV1,
    replica_receipt: FormalFrozenReceiptV1,
    primary_receipt_hot: Vec<u8>,
    replica_receipt_hot: Vec<u8>,
    primary_partial_hot: Vec<u8>,
    persisted_certificate: FormalFrozenCertificateV1,
    material_calls: u64,
}

#[inline(never)]
fn begin_partial_join(
    primary: PaidNormalFrozenEndpointV1,
    replica: PaidNormalFrozenEndpointV1,
    c: &RecoveryContext<'_>,
    material_work: &IncrementalFrozenMaterialWorkV1,
) -> PartialJoin {
    let primary_receipt = primary.receipt().clone();
    let replica_receipt = replica.receipt().clone();
    let primary_receipt_hot = std::fs::read(c.primary_hot).unwrap();
    let replica_receipt_hot = std::fs::read(c.replica_hot).unwrap();
    assert!(
        observed_certificate(c.primary_root, &primary_receipt_hot, &primary_receipt, c).is_none()
    );
    assert!(
        observed_certificate(c.replica_root, &replica_receipt_hot, &replica_receipt, c).is_none()
    );
    let replica_guard = publication_guard(c.replica_root);
    let before_join = (c.work.remaining_steps(), c.work.remaining_read_bytes());
    let material_calls = material_work.original_calls_remaining;
    assert_eq!(material_calls, 32 - 2 * 6);
    let failure = join_normal_frozen_endpoints_bounded(
        primary,
        replica,
        FormalEndpointPairPublicationRefsV1 {
            primary_provider: c.primary_provider,
            replica_provider: c.replica_provider,
            primary_policy: c.policy,
            replica_policy: c.policy,
            memory: c.memory,
            work: c.work,
        },
        &mut || false,
    )
    .err()
    .expect("replica publication lock must prevent certificate installation");
    assert!(
        failure
            .to_string()
            .contains("replica certificate persistence requires recovery"),
        "{failure}"
    );
    let primary_partial_hot = std::fs::read(c.primary_hot).unwrap();
    assert_ne!(primary_partial_hot, primary_receipt_hot);
    assert_eq!(std::fs::read(c.replica_hot).unwrap(), replica_receipt_hot);
    assert!(c.work.remaining_steps() < before_join.0);
    assert!(c.work.remaining_read_bytes() < before_join.1);
    assert_eq!(material_work.original_calls_remaining, material_calls);
    let persisted_certificate =
        observed_certificate(c.primary_root, &primary_partial_hot, &primary_receipt, c)
            .expect("primary actual pending already contains the full certificate");
    assert_eq!(&persisted_certificate.reservation, c.input);
    assert_eq!(persisted_certificate.primary, primary_receipt);
    assert_eq!(persisted_certificate.replica, replica_receipt);
    assert_eq!(
        persisted_certificate
            .verify(c.authority.definition(), c.authority.activation_ref())
            .unwrap(),
        c.input.body.reservation_id().unwrap()
    );
    PartialJoin {
        failure,
        replica_guard,
        primary_receipt,
        replica_receipt,
        primary_receipt_hot,
        replica_receipt_hot,
        primary_partial_hot,
        persisted_certificate,
        material_calls,
    }
}

#[inline(never)]
fn finish_partial_join(
    partial: PartialJoin,
    c: &RecoveryContext<'_>,
    material_work: &IncrementalFrozenMaterialWorkV1,
) -> PaidNormalFrozenPairV1 {
    let PartialJoin {
        failure,
        replica_guard,
        primary_receipt,
        replica_receipt,
        primary_receipt_hot,
        replica_receipt_hot,
        primary_partial_hot,
        persisted_certificate,
        material_calls,
    } = partial;
    // All actual archive observations have finished before this cancellation
    // snapshot. Recovery must poll stop and retain the same original holders.
    let owner_bytes = c.memory.used();
    let remaining = (c.work.remaining_steps(), c.work.remaining_read_bytes());
    let mut cancel_polls = 0;
    let failure = failure
        .recover_bounded(
            FormalEndpointPairPublicationRefsV1 {
                primary_provider: c.primary_provider,
                replica_provider: c.replica_provider,
                primary_policy: c.policy,
                replica_policy: c.policy,
                memory: c.memory,
                work: c.work,
            },
            &mut || {
                cancel_polls += 1;
                true
            },
        )
        .err()
        .expect("cancelled pair recovery must preserve the original owners");
    assert!(
        cancel_polls > 0,
        "recovery must actually observe cancellation"
    );
    assert_eq!(c.memory.used(), owner_bytes);
    assert_eq!(
        (c.work.remaining_steps(), c.work.remaining_read_bytes()),
        remaining
    );
    assert_eq!(std::fs::read(c.primary_hot).unwrap(), primary_partial_hot);
    assert_eq!(std::fs::read(c.replica_hot).unwrap(), replica_receipt_hot);
    assert_eq!(material_work.original_calls_remaining, material_calls);
    drop(replica_guard);
    let pair = failure
        .recover_bounded(
            FormalEndpointPairPublicationRefsV1 {
                primary_provider: c.primary_provider,
                replica_provider: c.replica_provider,
                primary_policy: c.policy,
                replica_policy: c.policy,
                memory: c.memory,
                work: c.work,
            },
            &mut || false,
        )
        .unwrap();
    assert!(pair.uses_budget(c.memory));
    assert_eq!(&pair.certificate().reservation, c.input);
    assert_eq!(pair.certificate().primary, primary_receipt);
    assert_eq!(pair.certificate().replica, replica_receipt);
    assert!(c.work.remaining_steps() < remaining.0);
    assert!(c.work.remaining_read_bytes() < remaining.1);
    assert_eq!(material_work.original_calls_remaining, material_calls);
    assert_eq!(
        pair.certificate()
            .verify(c.authority.definition(), c.authority.activation_ref())
            .unwrap(),
        c.input.body.reservation_id().unwrap()
    );
    assert_ne!(std::fs::read(c.primary_hot).unwrap(), primary_receipt_hot);
    assert_ne!(std::fs::read(c.replica_hot).unwrap(), replica_receipt_hot);
    for (root, hot_path, receipt) in [
        (c.primary_root, c.primary_hot, &primary_receipt),
        (c.replica_root, c.replica_hot, &replica_receipt),
    ] {
        let hot = std::fs::read(hot_path).unwrap();
        let actual = observed_certificate(root, &hot, receipt, c)
            .expect("both actual pending originals must contain the completed certificate");
        assert_eq!(&actual, pair.certificate());
        assert_eq!(actual, persisted_certificate);
    }
    pair
}

#[inline(never)]
fn assert_pair_parent_and_material(
    pair: &PaidNormalFrozenPairV1,
    c: &RecoveryContext<'_>,
    material_work: &mut IncrementalFrozenMaterialWorkV1,
) {
    for role in [EndpointRole::Primary, EndpointRole::Replica] {
        let inspect = |view: oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1<'_>| {
            assert_eq!(view.endpoint_role(), role);
            assert_eq!(view.head().verified_record_id(), c.expected.verified_record_id());
            Ok::<_, formal::FormalError>(())
        };
        match role {
            EndpointRole::Primary => {
                pair.with_primary_parent_read_view_bounded(c.memory, c.work, &mut || false, inspect)
            }
            EndpointRole::Replica => {
                pair.with_replica_parent_read_view_bounded(c.memory, c.work, &mut || false, inspect)
            }
        }
        .unwrap();
    }
    let checked = pair
        .check_material(c.memory, c.work, material_work, &mut || false)
        .unwrap();
    assert_eq!(checked.parent_record_id(), c.expected.verified_record_id());
    assert_eq!(
        hex::encode(checked.material_id()),
        c.input.body.frozen_material_digest
    );
    assert_eq!(material_work.original_calls_remaining, 32 - 3 * 6);
}

#[test]
fn real_genesis_frozen_prepare_failure_recovers_original_owner_and_joins_pair() {
    with_real_accepted_genesis(|fixture| {
        let (input, material) = next_frozen(&fixture);
        let AcceptedGenesisFixture {
            primary,
            replica,
            primary_provider,
            replica_provider,
            policy,
            memory,
            work,
            authority,
            expected,
            primary_root,
            replica_root,
            ..
        } = fixture;
        let primary_hot = primary_root.join("formal-checkpoint-v1.cbor");
        let replica_hot = replica_root.join("formal-checkpoint-v1.cbor");
        let c = RecoveryContext {
            input: &input,
            material: &material,
            primary_provider: primary_provider.as_ref(),
            replica_provider: replica_provider.as_ref(),
            policy,
            memory,
            work,
            authority,
            expected,
            primary_root,
            replica_root,
            primary_hot: &primary_hot,
            replica_hot: &replica_hot,
        };
        let mut material_work = IncrementalFrozenMaterialWorkV1 {
            metadata_entries_remaining: 1_000_000,
            comparison_bytes_remaining: 64 << 20,
            hash_bytes_remaining: 32 << 20,
            frozen_wire_bytes_remaining: 64 << 20,
            original_calls_remaining: 32,
            original_bytes_remaining: 16 << 20,
            max_original_bytes: 8 << 20,
        };
        let (primary, replica) = accepted_sources(primary, replica, &c);
        let primary = recover_primary_prepare_failure(primary, &c, &mut material_work);
        let replica = freeze_replica(replica, &c, &mut material_work);
        let partial = begin_partial_join(primary, replica, &c, &material_work);
        let pair = finish_partial_join(partial, &c, &material_work);
        assert_pair_parent_and_material(&pair, &c, &mut material_work);
    });
}
