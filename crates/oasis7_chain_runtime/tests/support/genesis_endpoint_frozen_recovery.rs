//! Actual accepted GEN -> failed Frozen preparation -> original-owner recovery.
//! The request bytes are content fixtures, not a Node decision or business tick.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalArchiveReadWorkV1, join_normal_frozen_endpoints_bounded,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalFrozenBodyV1, IncrementalFrozenMaterialWorkV1, SignedFormalFrozenV1,
    frozen_material_digest_bounded_v1,
};
use oasis7_formal_protocol::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use oasis7_formal_protocol::incremental_artifacts::{
    ExecutionManifestModeV1, IwsExecutionManifestV1, OriginalArtifactRefV1,
};
use std::collections::BTreeMap;

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
        let source = primary
            .into_accepted_genesis_host_source_bounded(
                primary_provider.as_ref(),
                policy,
                memory,
                work,
                &mut || false,
            )
            .unwrap();
        let replica = replica
            .into_accepted_genesis_host_source_bounded(
                replica_provider.as_ref(),
                policy,
                memory,
                work,
                &mut || false,
            )
            .unwrap();
        let primary_hot = primary_root.join("formal-checkpoint-v1.cbor");
        let replica_hot = replica_root.join("formal-checkpoint-v1.cbor");
        let before_hot = std::fs::read(&primary_hot).unwrap();
        // A separate actual writer FD fails the first preparation after all
        // read-only Frozen checks, without a fault hook or damaged disk data.
        let guard = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(primary_root.join("formal-objects-v1/.physical-publication-guard-v1"))
            .unwrap();
        guard.try_lock().unwrap();
        let mut material_work = IncrementalFrozenMaterialWorkV1 {
            metadata_entries_remaining: 1_000_000,
            comparison_bytes_remaining: 64 << 20,
            hash_bytes_remaining: 32 << 20,
            frozen_wire_bytes_remaining: 64 << 20,
            original_calls_remaining: 32,
            original_bytes_remaining: 16 << 20,
            max_original_bytes: 8 << 20,
        };
        let failure = source
            .freeze_normal_bounded(
                &input,
                &material,
                primary_provider.as_ref(),
                policy,
                memory,
                work,
                &mut material_work,
                &mut || false,
            )
            .err()
            .expect("held physical writer must reject preparation");
        assert!(
            failure.to_string().contains("physical publication busy"),
            "{failure}"
        );
        assert_eq!(std::fs::read(&primary_hot).unwrap(), before_hot);
        let owner_bytes = memory.used();
        let remaining = (work.remaining_steps(), work.remaining_read_bytes());
        let foreign_pool = MemoryBudget::new(512 << 20, 0).unwrap();
        let failure = failure
            .recover_bounded(
                primary_provider.as_ref(),
                policy,
                &foreign_pool,
                work,
                &mut || false,
            )
            .err()
            .expect("recovery must reject a foreign memory pool");
        assert!(
            failure
                .to_string()
                .contains("normal Frozen recovery pool/Core")
        );
        assert_eq!(memory.used(), owner_bytes);
        assert_eq!(foreign_pool.used(), 0);
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            (remaining.0 - 1, remaining.1)
        );
        let remaining = (work.remaining_steps(), work.remaining_read_bytes());
        let foreign_core = FormalArchiveReadWorkV1::new(remaining.0, remaining.1).unwrap();
        let failure = failure
            .recover_bounded(
                primary_provider.as_ref(),
                policy,
                memory,
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
        assert_eq!(memory.used(), owner_bytes);
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
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
            .recover_bounded(primary_provider.as_ref(), policy, memory, work, &mut || {
                true
            })
            .err()
            .expect("cancelled recovery must retain the original owners");
        assert!(
            failure
                .to_string()
                .contains("bounded archive read cancelled")
        );
        assert_eq!(memory.used(), owner_bytes);
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            remaining
        );
        assert_eq!(std::fs::read(&primary_hot).unwrap(), before_hot);
        // Recovery itself can reach the same preparation failure. The real
        // reconciliation/new FD must not lose the owners on this second pass.
        let failure = failure
            .recover_bounded(primary_provider.as_ref(), policy, memory, work, &mut || {
                false
            })
            .err()
            .expect("the held physical writer still prevents preparation");
        assert!(failure.to_string().contains("physical publication busy"));
        assert_eq!(std::fs::read(&primary_hot).unwrap(), before_hot);
        assert!(work.remaining_steps() < remaining.0);
        assert!(work.remaining_read_bytes() < remaining.1);
        let remaining = (work.remaining_steps(), work.remaining_read_bytes());
        drop(guard);
        // No source, input, material or reservation can be supplied anew here.
        // Success requires the original failure owner and original Core/fees.
        let recovered = failure
            .recover_bounded(primary_provider.as_ref(), policy, memory, work, &mut || {
                false
            })
            .unwrap();
        assert!(recovered.uses_budget(memory));
        assert_eq!(recovered.reservation().body(), &input.body);
        assert_eq!(recovered.receipt().body.role, EndpointRole::Primary);
        assert_eq!(
            recovered.receipt().body.authority_id,
            input.body.authority_id
        );
        assert_eq!(
            recovered.receipt().body.reservation_id,
            input.body.reservation_id().unwrap()
        );
        assert_ne!(std::fs::read(&primary_hot).unwrap(), before_hot);
        assert!(work.remaining_steps() < remaining.0);
        assert!(work.remaining_read_bytes() < remaining.1);
        let replica = replica
            .freeze_normal_bounded(
                &input,
                &material,
                replica_provider.as_ref(),
                policy,
                memory,
                work,
                &mut material_work,
                &mut || false,
            )
            .unwrap();
        let primary_receipt_hot = std::fs::read(&primary_hot).unwrap();
        let replica_receipt_hot = std::fs::read(&replica_hot).unwrap();
        let pair = join_normal_frozen_endpoints_bounded(
            recovered,
            replica,
            primary_provider.as_ref(),
            replica_provider.as_ref(),
            policy,
            policy,
            memory,
            work,
            &mut || false,
        )
        .unwrap();
        assert!(pair.uses_budget(memory));
        assert_eq!(
            pair.certificate()
                .verify(authority.definition(), authority.activation_ref())
                .unwrap(),
            input.body.reservation_id().unwrap()
        );
        assert_ne!(std::fs::read(&primary_hot).unwrap(), primary_receipt_hot);
        assert_ne!(std::fs::read(&replica_hot).unwrap(), replica_receipt_hot);
        for role in [EndpointRole::Primary, EndpointRole::Replica] {
            let inspect = |view: oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1<'_>| {
                assert_eq!(view.endpoint_role(), role);
                assert_eq!(view.head().verified_record_id(), expected.verified_record_id());
                Ok::<_, formal::FormalError>(())
            };
            match role {
                EndpointRole::Primary => {
                    pair.with_primary_parent_read_view_bounded(memory, work, &mut || false, inspect)
                }
                EndpointRole::Replica => {
                    pair.with_replica_parent_read_view_bounded(memory, work, &mut || false, inspect)
                }
            }
            .unwrap();
        }
        let checked = pair
            .check_material(memory, work, &mut material_work, &mut || false)
            .unwrap();
        assert_eq!(checked.parent_record_id(), expected.verified_record_id());
        assert_eq!(
            hex::encode(checked.material_id()),
            input.body.frozen_material_digest
        );
        assert_eq!(material_work.original_calls_remaining, 32 - 3 * 6);
    });
}
