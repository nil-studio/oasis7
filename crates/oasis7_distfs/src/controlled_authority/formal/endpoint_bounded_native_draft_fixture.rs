//! Fresh signed metadata and structural layout fixtures, never IWS eligibility.
use super::*;

pub(in super::super) enum NativeDraftFixtureV1 {
    Plain,
    Revision23,
    Revision255,
    Revision65535,
    Overflow,
    ExactPreimage,
    OverPreimage,
    Sealed,
}
pub(in super::super) fn with_native_draft_fixture(
    mode: NativeDraftFixtureV1,
    run: impl FnOnce(&Store, &Path),
) {
    let fixture = Fixture::new();
    if matches!(mode, NativeDraftFixtureV1::Sealed) {
        let mut coordinator = fixture.coordinator();
        coordinator.submit(fixture.initial.clone()).unwrap();
        let primary = coordinator.endpoints_mut().0;
        let checkpoint = primary.checkpoint().unwrap();
        primary.seal_epoch(&h(211), 2, &checkpoint).unwrap();
        assert!(primary.store.body.seal.is_some());
        run(&primary.store, &fixture.pdir);
        return;
    }
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    if matches!(mode, NativeDraftFixtureV1::Plain) {
        run(&endpoint.store, &fixture.pdir);
        return;
    }
    // Named DATA-ONLY metadata variants are constructed before the old Store
    // signs/opens them. No mutable Store/Hot/root setter escapes this fixture.
    // This is not a directory-authorized endpoint, recovery or accepted GEN.
    let mut initial = endpoint.store.body.clone();
    match mode {
        NativeDraftFixtureV1::Revision23 => initial.revision = 23,
        NativeDraftFixtureV1::Revision255 => initial.revision = 255,
        NativeDraftFixtureV1::Revision65535 => initial.revision = 65535,
        NativeDraftFixtureV1::Overflow => initial.revision = u64::MAX,
        NativeDraftFixtureV1::ExactPreimage | NativeDraftFixtureV1::OverPreimage => {
            initial.revision = 1;
            initial.activation_ref.clear();
            let target =
                64 * 1024 + usize::from(matches!(mode, NativeDraftFixtureV1::OverPreimage));
            let mut fill = target - Store::checkpoint_signing_bytes(&initial).unwrap().len();
            for _ in 0..8 {
                initial.activation_ref = "x".repeat(fill);
                let actual = Store::checkpoint_signing_bytes(&initial).unwrap().len();
                if actual == target {
                    break;
                }
                fill = if actual > target {
                    fill - (actual - target)
                } else {
                    fill + (target - actual)
                };
            }
            assert_eq!(
                Store::checkpoint_signing_bytes(&initial).unwrap().len(),
                target
            );
        }
        _ => unreachable!(),
    }
    let path = fixture.pdir.join("native-draft-metadata-only");
    let store = Store::open(&path, initial, &key(5), vec![]).unwrap();
    run(&store, &path);
}

pub(in super::super) enum NativeDraftLayoutFixtureV1 {
    Empty,
    Ten,
    HistoricalEleven,
    Fourteen,
}
pub(in super::super) fn with_native_draft_layout_fixture(
    shape: NativeDraftLayoutFixtureV1,
    run: impl FnOnce(&HotBody),
) {
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    let qualified = coordinator.submit(fixture.initial.clone()).unwrap();
    let primary = coordinator.endpoints_mut().0;
    let checkpoint = primary.checkpoint().unwrap();
    let seal = primary.seal_epoch(&h(212), 2, &checkpoint).unwrap();
    // Start from real protocol-produced data. The following structural variants
    // deliberately are not Qualified records or serving checkpoints, and are
    // only borrowed by the layout unit; no Store or draft constructor is given.
    let mut body = primary.store.body.clone();
    let mut record = qualified.decision.record.clone();
    let roles = [
        ArtifactRole::Input,
        ArtifactRole::Result,
        ArtifactRole::Snapshot,
        ArtifactRole::Journal,
        ArtifactRole::NonceIndex,
        ArtifactRole::EffectOutbox,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
        ArtifactRole::NativeExecutionEnvironment,
        ArtifactRole::IncrementalGenesisConfig,
        ArtifactRole::IncrementalRootSet,
        ArtifactRole::IncrementalCommitBody,
        ArtifactRole::IncrementalHistoryPrefix,
    ];
    record.artifact_roots.clear();
    for (i, role) in roles.iter().enumerate() {
        record.artifact_roots.insert(*role, h(i as u8 + 100));
    }
    match shape {
        NativeDraftLayoutFixtureV1::Empty => {
            for role in roles {
                record.artifact_roots.remove(&role);
            }
        }
        NativeDraftLayoutFixtureV1::Ten => {
            record.artifact_roots = roles[..10]
                .iter()
                .enumerate()
                .map(|(i, r)| (*r, h(i as u8 + 100)))
                .collect();
        }
        NativeDraftLayoutFixtureV1::HistoricalEleven => {
            for role in &roles[11..] {
                record.artifact_roots.remove(role);
            }
        }
        NativeDraftLayoutFixtureV1::Fourteen => {}
    }
    record.durable_set_transition_digest = Some(h(240));
    record.frozen_reservation_id = Some(h(241));
    record.handoff = Some(ProfileHandoffV1 {
        next_profile: "layout-only".into(),
        next_profile_version: 1,
        next_authority_digest: h(242),
        valid_from_position: 2,
    });
    body.base_parent = Some(store::CheckpointParent {
        record,
        finalizing_authority_id: fixture.authority.authority_id().unwrap(),
    });
    body.head_node = Some(h(243));
    body.imported_head_node = Some(h(244));
    body.import_digest = Some(h(245));
    body.index = Some(h(246));
    body.pending = Some(h(247));
    body.decision_head = Some(body.head.clone());
    body.data_closure = Some(h(248));
    let mut seal = seal.body;
    seal.tail.request = Some(crate::controlled_authority::LocalRequestIdentity {
        verified_subject: "layout subject".into(),
        operation_domain: "layout domain".into(),
        nonce_scope: "layout nonce".into(),
        request_id: "layout request".into(),
    });
    seal.tail.frozen_input_digest = Some(h(249));
    seal.tail.frozen_certificate_digest = Some(h(250));
    seal.tail.proposal_digest = Some(h(251));
    seal.tail.replica_prepare_digest = Some(h(252));
    seal.tail.primary_receipt_digest = Some(h(253));
    seal.tail.replica_receipt_digest = Some(h(254));
    body.seal = Some(seal);
    body.durable_set = Some(fencing::DurableSetNativeBindingV1 {
        transition_digest: h(230),
        independent_inventory_digest: h(231),
        activation_digest: Some(h(232)),
    });
    run(&body);
}
