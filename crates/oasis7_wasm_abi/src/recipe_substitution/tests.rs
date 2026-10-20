use super::*;
#[test]
fn missing_authority_is_not_defaulted() {
    assert!(serde_json::from_str::<SubstitutionRuleV1>("{}").is_err());
}
#[test]
fn checked_ratio_rejects_zero_and_overflow() {
    assert_eq!(
        checked_source_requirement(1, 0, 1, SubstitutionRoundingV1::Exact),
        Err(SubstitutionRuleError::InvalidRatio)
    );
    assert_eq!(
        checked_source_requirement(u64::MAX, u64::MAX, 1, SubstitutionRoundingV1::Exact),
        Err(SubstitutionRuleError::ArithmeticOverflow)
    );
    assert_eq!(
        checked_source_requirement(1, 1, 2, SubstitutionRoundingV1::Exact),
        Err(SubstitutionRuleError::QuantizationBlocked)
    );
    assert_eq!(
        checked_source_requirement(1, 1, 2, SubstitutionRoundingV1::Ceil),
        Ok((1, 1))
    );
}
fn profile(id: &str) -> SubstitutionProfileRefV1 {
    SubstitutionProfileRefV1 {
        id: id.into(),
        version: "1".into(),
        digest: "a".repeat(64),
    }
}
fn rule() -> SubstitutionRuleV1 {
    SubstitutionRuleV1 {
        rule: profile("rule"),
        recipe: profile("recipe"),
        input_edge: "input".into(),
        required_material: profile("standard"),
        allowed_materials: vec![profile("alternate")],
        accepted_specs: vec!["spec".into()],
        accepted_quality: vec!["quality".into()],
        accepted_custody: vec!["custody".into()],
        owner_ledger_path_policy: profile("path"),
        source_unit: "source-unit".into(),
        target_unit: "target-unit".into(),
        ratio_numerator: 3,
        ratio_denominator: 2,
        executable_target_quantum: 1,
        rounding: SubstitutionRoundingV1::Ceil,
        residual: SubstitutionResidualV1::Retain,
        aggregation_domain: profile("cycle"),
        source_loss_numerator: 0,
        source_loss_denominator: 1,
        impacts: SubstitutionImpactV1 {
            outputs: profile("outputs"),
            quality: profile("quality"),
            power: profile("power"),
            time: profile("time"),
            byproducts: profile("byproducts"),
            terminal: profile("terminal"),
            value_class: profile("value"),
            logistics: profile("logistics"),
            buffer: profile("buffer"),
        },
        conversion: SubstitutionConversionV1::Required {
            converter: profile("converter"),
        },
        applicable_input_states: vec!["unconsumed".into()],
        disposition_policy: profile("disposition"),
        recheck_policy: profile("recheck"),
    }
}
fn plan(
    r: &SubstitutionRuleV1,
    total: u64,
) -> Result<SubstitutionQuantityPlanV1, SubstitutionRuleError> {
    r.quantity_plan(
        r.aggregation_domain.clone(),
        "canonical-cycle-1".into(),
        total,
        &r.source_unit,
        &r.target_unit,
    )
}
#[test]
fn every_authority_field_is_required_on_wire() {
    let value = serde_json::to_value(rule()).unwrap();
    for key in value.as_object().unwrap().keys() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<SubstitutionRuleV1>(missing).is_err(),
            "{key}"
        );
    }
}
#[test]
fn domain_total_quantization_preserves_exact_residual() {
    let r = rule();
    let one = plan(&r, 1).unwrap();
    assert_eq!(
        (
            one.source_required,
            one.residual_numerator,
            one.residual_denominator
        ),
        (2, 1, 2)
    );
    let whole = plan(&r, 2).unwrap();
    assert_eq!((whole.source_required, whole.residual_numerator), (3, 0));
    assert_eq!(whole.domain_key, one.domain_key);
    assert_eq!(whole.unmet_target, 0);
    assert_eq!(whole.source_loss, 0);
}
#[test]
fn malformed_profiles_and_sets_fail_closed() {
    let mut r = rule();
    r.ratio_denominator = 0;
    assert_eq!(
        r.validate("recipe"),
        Err(SubstitutionRuleError::InvalidRatio)
    );
    r = rule();
    r.source_unit.clear();
    assert_eq!(
        r.validate("recipe"),
        Err(SubstitutionRuleError::InvalidUnit)
    );
    r = rule();
    r.executable_target_quantum = 0;
    assert_eq!(
        r.validate("recipe"),
        Err(SubstitutionRuleError::InvalidQuantum)
    );
    r = rule();
    r.accepted_quality.push("quality".into());
    assert_eq!(
        r.validate("recipe"),
        Err(SubstitutionRuleError::NonCanonicalSet)
    );
    r = rule();
    r.allowed_materials.push(profile("alternate"));
    assert_eq!(
        r.validate("recipe"),
        Err(SubstitutionRuleError::NonCanonicalSet)
    );
    assert_eq!(
        validate_substitution_rules(&[rule(), rule()], "recipe"),
        Err(SubstitutionRuleError::NonCanonicalSet)
    );
    assert_eq!(
        validate_substitution_rules(&vec![rule(); MAX_SUBSTITUTION_RULES + 1], "recipe"),
        Err(SubstitutionRuleError::CapacityExceeded)
    );
}
#[test]
fn quantity_checks_domain_unit_quantum_and_loss() {
    let mut r = rule();
    assert_eq!(
        r.quantity_plan(
            profile("other"),
            "cycle".into(),
            2,
            &r.source_unit,
            &r.target_unit
        ),
        Err(SubstitutionRuleError::DomainMismatch)
    );
    assert_eq!(
        r.quantity_plan(
            r.aggregation_domain.clone(),
            "cycle".into(),
            2,
            "wrong",
            &r.target_unit
        ),
        Err(SubstitutionRuleError::InvalidUnit)
    );
    r.executable_target_quantum = 2;
    assert_eq!(plan(&r, 1), Err(SubstitutionRuleError::QuantizationBlocked));
    assert_eq!(plan(&r, 0), Err(SubstitutionRuleError::QuantizationBlocked));
    r.source_loss_numerator = 1;
    r.source_loss_denominator = 2;
    let p = plan(&r, 2).unwrap();
    assert_eq!((p.source_required, p.source_loss), (4, 1));
    r.executable_target_quantum = 1;
    assert_eq!(plan(&r, 1), Err(SubstitutionRuleError::QuantizationBlocked));
    r = rule();
    r.residual = SubstitutionResidualV1::Reject;
    assert_eq!(plan(&r, 1), Err(SubstitutionRuleError::QuantizationBlocked));
}
#[test]
fn conversion_authority_and_bounded_choices_are_required() {
    let mut value = serde_json::to_value(rule()).unwrap();
    value["conversion"] = serde_json::json!({"mode":"required"});
    assert!(serde_json::from_value::<SubstitutionRuleV1>(value).is_err());
    let mut r = rule();
    r.accepted_custody = (0..=MAX_SUBSTITUTION_CHOICES)
        .map(|i| format!("custody-{i:03}"))
        .collect();
    assert_eq!(
        r.validate("recipe"),
        Err(SubstitutionRuleError::CapacityExceeded)
    );
    r.accepted_custody.pop();
    assert!(r.validate("recipe").is_ok());
    assert_eq!(
        r.validate("other"),
        Err(SubstitutionRuleError::RecipeMismatch)
    );
}
#[test]
fn source_loss_addition_overflow_is_rejected() {
    let mut r = rule();
    r.ratio_numerator = 1;
    r.ratio_denominator = 1;
    r.source_loss_numerator = 1;
    assert_eq!(
        plan(&r, u64::MAX),
        Err(SubstitutionRuleError::ArithmeticOverflow)
    );
}
#[test]
fn profile_digests_require_canonical_sha256_encoding() {
    let mut r = rule();
    for digest in ["short".to_string(), "A".repeat(64), "g".repeat(64)] {
        r.rule.digest = digest;
        assert_eq!(
            r.validate("recipe"),
            Err(SubstitutionRuleError::InvalidReference)
        );
    }
}
