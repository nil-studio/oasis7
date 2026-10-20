use super::*;

#[test]
fn authority_bound_registry_rejects_missing_remote_signer() {
    let mut registry = GovernanceFinalitySignerRegistry {
        slot_id: "finality".into(),
        signer_bindings: BTreeMap::from([("finality.local".into(), "11".repeat(32))]),
        validator_stakes: BTreeMap::from([
            ("finality.local".into(), 1),
            ("finality.remote".into(), 1),
        ]),
        ..GovernanceFinalitySignerRegistry::default()
    };
    assert!(validate_world_finality_signer_completeness(&registry, &[]).is_err());
    registry
        .signer_bindings
        .insert("finality.remote".into(), "22".repeat(32));
    validate_world_finality_signer_completeness(&registry, &[]).unwrap();
    assert!(
        validate_world_finality_signer_completeness(
            &registry,
            &[PosValidator {
                validator_id: "unknown".into(),
                stake: 1
            }]
        )
        .is_err()
    );
}
