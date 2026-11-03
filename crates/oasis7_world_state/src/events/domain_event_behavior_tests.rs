use super::behavior_samples::{debug_samples, settlement_event};
use super::{DomainEvent, MaterialLedgerId};

#[test]
fn equality_is_reflexive_for_debug_samples() {
    for (_, event) in debug_samples() {
        assert_eq!(event, event.clone());
    }
}

#[test]
fn equality_still_compares_fields_and_variant_identity() {
    let original = DomainEvent::FactoryDurabilityChanged {
        factory_id: "factory-a".to_owned(),
        previous_durability_ppm: 1,
        durability_ppm: 2,
        reason: "wear".to_owned(),
    };
    let changed = DomainEvent::FactoryDurabilityChanged {
        factory_id: "factory-a".to_owned(),
        previous_durability_ppm: 1,
        durability_ppm: 3,
        reason: "wear".to_owned(),
    };
    let different_variant = DomainEvent::FactoryMaintained {
        operator_agent_id: "operator".to_owned(),
        factory_id: "factory-a".to_owned(),
        consume_ledger: MaterialLedgerId::world(),
        consumed_parts: 1,
        durability_ppm: 2,
    };
    assert_ne!(original, changed);
    assert_ne!(original, different_variant);
}

#[test]
fn equality_preserves_nan_behavior_in_nested_fields() {
    let event = settlement_event(f64::NAN);
    assert_ne!(event, event.clone());
}

#[test]
fn debug_matches_legacy_normal_and_pretty_output() {
    let expected: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("domain_event_debug_baseline.json"))
            .expect("legacy Debug baseline JSON");
    let samples = debug_samples();
    assert_eq!(samples.len(), expected.len());
    for (name, event) in samples {
        let record = expected
            .iter()
            .find(|record| record["name"] == name)
            .expect("sample must have a legacy oracle");
        assert_eq!(
            format!("{event:?}"),
            record["normal"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            format!("{event:#?}"),
            record["pretty"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn equality_rejects_different_sample_groups() {
    let samples = debug_samples();
    for (i, (_, left)) in samples.iter().enumerate() {
        for (j, (_, right)) in samples.iter().enumerate() {
            if i != j {
                assert_ne!(left, right);
            }
        }
    }
}
