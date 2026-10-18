use super::*;

#[test]
fn physics_parameter_specs_cover_serialized_physics_config_fields() {
    let physics_value = serde_json::to_value(PhysicsConfig::default()).expect("serialize physics");
    let physics_object = physics_value.as_object().expect("physics object");

    let mut config_keys: Vec<String> = physics_object.keys().cloned().collect();
    config_keys.sort();

    let mut spec_keys: Vec<String> = physics_parameter_specs()
        .iter()
        .map(|spec| spec.key.to_string())
        .collect();
    spec_keys.sort();

    assert_eq!(spec_keys, config_keys);
}

#[test]
fn physics_parameter_specs_define_units_ranges_and_default_coverage() {
    let physics_value = serde_json::to_value(PhysicsConfig::default()).expect("serialize physics");
    let physics_object = physics_value.as_object().expect("physics object");

    for spec in physics_parameter_specs() {
        assert!(
            !spec.unit.is_empty(),
            "unit should not be empty: {}",
            spec.key
        );
        assert!(
            !spec.tuning_impact.is_empty(),
            "tuning impact should not be empty: {}",
            spec.key
        );
        assert!(
            spec.recommended_min <= spec.recommended_max,
            "invalid range for {}",
            spec.key
        );

        let value = physics_object
            .get(spec.key)
            .unwrap_or_else(|| panic!("missing default value for {}", spec.key));
        let numeric = value
            .as_f64()
            .or_else(|| value.as_i64().map(|entry| entry as f64))
            .or_else(|| value.as_u64().map(|entry| entry as f64))
            .unwrap_or_else(|| panic!("non numeric value for {}", spec.key));

        assert!(
            numeric >= spec.recommended_min,
            "default {}={} below recommended min {}",
            spec.key,
            numeric,
            spec.recommended_min
        );
        assert!(
            numeric <= spec.recommended_max,
            "default {}={} above recommended max {}",
            spec.key,
            numeric,
            spec.recommended_max
        );
    }
}
