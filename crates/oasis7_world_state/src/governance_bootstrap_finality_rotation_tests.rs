//! Persistence/projection vectors only. World separately verifies authority.
use super::*;
use serde::de;
use crate::{GovernanceEvent, Snapshot, WorldStateProjection};
use oasis7_agent_api::world_action::GovernanceAuthorizationV1;
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn record(id: &str) -> BootstrapFinalityKeyRotationRecordV1 {
    let context = GovernanceAuthorizationV1 {
        schema_version: 1,
        world_id: "rotation-vector".into(),
        chain_id: "chain".into(),
        genesis_digest: h(1),
        parent_position: 8,
        parent_record_id: h(2),
        parent_state_root: h(3),
        policy_digest: h(4),
        controller_account_id: "controller".into(),
        nonce: 1,
        valid_from_position: 9,
        valid_through_position: 9,
        signatures: BTreeMap::new(),
    };
    let body = BootstrapFinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: context.clone(),
        rotation_id: id.into(),
        bootstrap_registry_digest_hex: h(8),
        bootstrap_public_key_hex: h(9),
        node_id: "node".into(),
        expected_generation: 1,
        old_public_key_hex: h(5),
        new_public_key_hex: h(6),
        decision_height: 9,
        activation_height: 11,
        target_epoch: 11,
    };
    let old_key_proof = BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role: BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        signature_hex: "00".repeat(64),
    };
    let new_key_proof = BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role: BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
        signature_hex: "00".repeat(64),
    };
    let mut authorization = context;
    authorization.signatures.insert(h(7), "00".repeat(64));
    BootstrapFinalityKeyRotationRecordV1 {
        authorized_action: GovernanceAuthorizedActionV1 {
            action: Box::new(Action::RotateGovernanceBootstrapFinalityKeyV1 {
                controller_account_id: "controller".into(),
                body,
                old_key_proof,
                new_key_proof,
            }),
            authorization,
        },
        applied_at_tick: 9,
    }
}
fn snapshot() -> Snapshot {
    crate::codec::snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap()
}
#[derive(Deserialize)]
#[serde(transparent)]
struct History(
    #[serde(deserialize_with = "deserialize_bootstrap_finality_key_rotations")]
    BTreeMap<String, BootstrapFinalityKeyRotationRecordV1>,
);
#[test]
fn history_rejects_declared_overflow_duplicates_wrong_ids_and_non_rotation_actions() {
    let error = serde_cbor::from_slice::<History>(&[0xb9, 0x01, 0x01])
        .err()
        .unwrap();
    assert!(error.to_string().contains("declared count"), "{error}");
    let r = record("rotation-one");
    r.validate().unwrap();
    let json = serde_json::to_string(&r).unwrap();
    assert_eq!(
        serde_json::from_str::<History>(&format!("{{\"rotation-one\":{json}}}"))
            .unwrap()
            .0
            .len(),
        1
    );
    assert!(
        serde_json::from_str::<History>(&format!(
            "{{\"rotation-one\":{json},\"rotation-one\":{json}}}"
        ))
        .is_err()
    );
    assert!(serde_json::from_str::<History>(&format!("{{\"different-id\":{json}}}")).is_err());
    let mut invalid = r.clone();
    *invalid.authorized_action.action = Action::QueryObservation {
        agent_id: "agent".into(),
    };
    assert!(invalid.validate().is_err());
    let mut invalid = r.clone();
    invalid.applied_at_tick += 1;
    assert!(invalid.validate().is_err());
    let invalid_json = serde_json::to_string(&invalid).unwrap();
    assert!(
        serde_json::from_str::<History>(&format!("{{\"rotation-one\":{invalid_json}}}")).is_err()
    );
    let mut snapshot = snapshot();
    snapshot
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("rotation-one".into(), invalid);
    assert!(crate::codec::snapshot_cbor(&snapshot).is_err());
    let mut invalid = r;
    invalid.authorized_action.authorization.nonce += 1;
    assert!(invalid.validate().is_err());
}
#[test]
fn rotation_map_capacity_is_enforced_in_both_decode_and_persistence() {
    let mut snapshot = snapshot();
    for i in 0..super::super::finality_rotation::MAX_FINALITY_KEY_ROTATION_RECORDS_V1 {
        let id = format!("rotation-{i}");
        snapshot
            .state
            .governance_bootstrap_finality_key_rotations
            .insert(id.clone(), record(&id));
    }
    let bytes = crate::codec::snapshot_cbor(&snapshot).unwrap();
    assert_eq!(crate::codec::snapshot_from_cbor(&bytes).unwrap(), snapshot);
    snapshot
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("one-too-many".into(), record("one-too-many"));
    assert!(crate::codec::snapshot_cbor(&snapshot).is_err());
}
#[test]
fn empty_history_preserves_bytes_and_atomic_projection_matches_installed_record() {
    let base = snapshot();
    let before = crate::codec::snapshot_cbor(&base).unwrap();
    assert!(
        !crate::codec::snapshot_json(&base)
            .unwrap()
            .windows(b"governance_bootstrap_finality_key_rotations".len())
            .any(|w| w == b"governance_bootstrap_finality_key_rotations")
    );
    let mut explicit_empty = base.clone();
    explicit_empty
        .state
        .governance_bootstrap_finality_key_rotations
        .clear();
    assert_eq!(
        crate::codec::snapshot_cbor(&explicit_empty).unwrap(),
        before
    );
    let original = record("rotation-one");
    let prepared = crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent {
        event: GovernanceEvent::BootstrapFinalityKeyRotated {
            authorization: original.authorized_action.clone(),
        },
        controller_registry: base.state.governance_main_token_controller_registry.clone(),
        admissions: base.state.governance_validator_admissions.clone(),
        rotations: BTreeMap::new(),
        bootstrap_rotations: BTreeMap::from([("rotation-one".into(), original.clone())]),
        authorization_nonces: base.state.governance_authorization_nonces.clone(),
        identity_bindings: base.state.node_identity_bindings.clone(),
        account_bindings: base.state.node_main_token_account_bindings.clone(),
    };
    let projected = serde_json::to_value(
        WorldStateProjection::borrowed(&base.state).with_governance_registry_overlay(&prepared),
    )
    .unwrap();
    let mut installed = base.clone();
    prepared.install(&mut installed.state);
    assert_eq!(
        serde_json::to_value(WorldStateProjection::borrowed(&installed.state)).unwrap(),
        projected
    );
    assert_ne!(
        serde_json::to_value(WorldStateProjection::borrowed(&base.state)).unwrap(),
        projected
    );
    assert_eq!(
        installed.state.node_identity_bindings,
        base.state.node_identity_bindings
    );
    assert_eq!(
        installed.state.node_main_token_account_bindings,
        base.state.node_main_token_account_bindings
    );
    assert_eq!(
        installed.state.governance_validator_admissions,
        base.state.governance_validator_admissions
    );
    assert_eq!(
        installed.state.governance_bootstrap_finality_key_rotations["rotation-one"],
        original
    );
    assert_ne!(crate::codec::snapshot_cbor(&installed).unwrap(), before);
    let manifest_hash = h(20);
    let policy_hash = h(21);
    let root = |state: &crate::WorldState| {
        crate::codec::state_root_hash(&crate::codec::StateRootProjection {
            state: &WorldStateProjection::borrowed(state),
            manifest_hash: &manifest_hash,
            policy_hash: &policy_hash,
        })
        .unwrap()
    };
    assert_ne!(
        root(&installed.state),
        root(&base.state),
        "formal state-root commitments include nonempty rotation history"
    );
}
// Model the exact failure mode of an older serde reader: it ignores the new
// field and otherwise decodes a complete Snapshot. The formal delta consumer
// must reject that lossy result instead of accepting successful Deserialize.
#[derive(Serialize)]
#[serde(transparent)]
struct LossyLegacySnapshot(Snapshot);
impl<'de> Deserialize<'de> for LossyLegacySnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut value = serde_cbor::Value::deserialize(d)?;
        let serde_cbor::Value::Map(fields) = &mut value else {
            return Err(de::Error::custom("legacy snapshot map missing"));
        };
        let Some(serde_cbor::Value::Map(state)) =
            fields.get_mut(&serde_cbor::Value::Text("state".into()))
        else {
            return Err(de::Error::custom("legacy state missing"));
        };
        state.remove(&serde_cbor::Value::Text(
            "governance_bootstrap_finality_key_rotations".into(),
        ));
        serde_cbor::value::from_value(value)
            .map(Self)
            .map_err(de::Error::custom)
    }
}
#[test]
fn formal_snapshot_and_delta_keep_nonempty_history_and_reject_lossy_legacy_decode() {
    let base = snapshot();
    let mut rotated = base.clone();
    rotated
        .state
        .governance_bootstrap_finality_key_rotations
        .insert("rotation-one".into(), record("rotation-one"));
    let bytes = crate::codec::snapshot_cbor(&rotated).unwrap();
    let reference = blake3::hash(&bytes).to_hex().to_string();
    assert_eq!(
        crate::resolve_snapshot_artifact(&reference, &|_| Ok(bytes.clone())).unwrap(),
        rotated
    );
    let delta = crate::SnapshotDeltaV1::between(&base, &rotated).unwrap();
    assert_eq!(delta.apply(&base).unwrap(), rotated);
    let error = delta.apply(&LossyLegacySnapshot(base)).err().unwrap();
    assert!(error.contains("typed canonical output mismatch"), "{error}");
    let event = GovernanceEvent::BootstrapFinalityKeyRotated {
        authorization: record("rotation-one").authorized_action,
    };
    let encoded = serde_json::to_vec(&event).unwrap();
    assert_eq!(
        serde_json::from_slice::<GovernanceEvent>(&encoded).unwrap(),
        event
    );
    assert!(
        serde_json::from_value::<GovernanceEvent>(
            serde_json::json!({"BootstrapFinalityKeyRotated":{}})
        )
        .is_err()
    );
}

#[test]
fn bootstrap_source_digest_pins_typed_registry_framing_and_all_fields() {
    let base = crate::GovernanceFinalitySignerRegistry::default();
    // Independently framed SHA-256 vector for the literal typed CBOR map.
    assert_eq!(
        hex::encode(serde_cbor::to_vec(&base).unwrap()),
        "a567736c6f745f696460697468726573686f6c64006d7468726573686f6c645f627073006f7369676e65725f62696e64696e6773a07076616c696461746f725f7374616b6573a0"
    );
    let digest = bootstrap_finality_registry_digest_v1(&base).unwrap();
    assert_eq!(
        digest,
        "04219df55c4cccb98c858a85cae693ec9f087e31238ef4280722106fe666370e"
    );
    for field in 0..5 {
        let mut changed = base.clone();
        match field {
            0 => changed.slot_id = "slot".into(),
            1 => changed.threshold = 1,
            2 => changed.threshold_bps = 1,
            3 => {
                changed.signer_bindings.insert("node".into(), h(3));
            }
            _ => {
                changed.validator_stakes.insert("node".into(), 1);
            }
        }
        assert_ne!(
            bootstrap_finality_registry_digest_v1(&changed).unwrap(),
            digest
        );
    }
}
#[test]
fn both_sources_share_capacity_without_changing_individual_bounds() {
    use crate::governance::validate_finality_key_rotation_history_capacity as capacity;
    assert!(capacity(128, 128).is_ok());
    assert!(capacity(128, 129).is_err());
    assert!(capacity(usize::MAX, 1).is_err());
    let mut state = snapshot();
    let admitted = super::super::finality_rotation::tests::record("admitted");
    state
        .state
        .governance_finality_key_rotations
        .insert("admitted".into(), admitted);
    for i in 0..256 {
        let id = format!("bootstrap-{i}");
        state
            .state
            .governance_bootstrap_finality_key_rotations
            .insert(id.clone(), record(&id));
    }
    assert!(crate::codec::snapshot_cbor(&state).is_err());
    // Bypass the current serializer to exercise concrete snapshot decoding.
    let mut value = serde_json::to_value(snapshot()).unwrap();
    value["state"]["governance_finality_key_rotations"] =
        serde_json::to_value(&state.state.governance_finality_key_rotations).unwrap();
    value["state"]["governance_bootstrap_finality_key_rotations"] =
        serde_json::to_value(&state.state.governance_bootstrap_finality_key_rotations).unwrap();
    assert!(crate::codec::snapshot_from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn explicit_empty_bootstrap_history_preserves_legacy_bytes_and_state_root() {
    let legacy = snapshot();
    let before = crate::codec::snapshot_cbor(&legacy).unwrap();
    let mut value = serde_json::to_value(&legacy).unwrap();
    value["state"]["governance_bootstrap_finality_key_rotations"] = serde_json::json!({});
    let decoded = crate::codec::snapshot_from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(crate::codec::snapshot_cbor(&decoded).unwrap(), before);
    assert_eq!(
        serde_json::to_value(WorldStateProjection::borrowed(&decoded.state)).unwrap(),
        serde_json::to_value(WorldStateProjection::borrowed(&legacy.state)).unwrap()
    );
}
