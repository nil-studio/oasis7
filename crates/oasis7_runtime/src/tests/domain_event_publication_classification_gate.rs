#[test]
fn every_domain_event_has_an_explicit_fail_closed_publication_classification() {
    let domain_source = include_str!("../events/domain_event.rs");
    let publication_source = include_str!("../world/event_processing/publication.rs");
    let delta_source = include_str!("../world/event_processing/prepared_state_delta.rs");
    let core_source = include_str!("../state/core_policy_transition.rs");
    let classifier = [publication_source, delta_source, core_source].concat();

    let enum_body = domain_source
        .split_once("pub enum DomainEvent {")
        .expect("DomainEvent declaration")
        .1
        .split_once("\n}")
        .expect("DomainEvent closing brace")
        .0;
    let variants = enum_body
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix("    ")?;
            let name = line.strip_suffix(" {")?;
            name.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
                .then_some(name)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        variants.len(),
        103,
        "review newly added DomainEvent variants"
    );
    let missing = variants
        .iter()
        .filter(|variant| !classifier.contains(&format!("DomainEvent::{variant}")))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "unclassified DomainEvent variants: {missing:?}"
    );
}

#[test]
fn no_state_is_event_bound_and_legacy_route_only_seam_is_absent() {
    let publication_source = include_str!("../world/event_processing/publication.rs");
    let delta_source = include_str!("../world/event_processing/prepared_state_delta.rs");

    assert!(delta_source.contains("NoState(WorldEventBody)"));
    assert!(
        delta_source.contains("WorldEventBody::Domain(DomainEvent::ActionRejected { .. })")
            && delta_source.contains("Some(Self::NoState(body.clone()))")
            && delta_source.contains("Self::NoState(prepared_body) => prepared_body == body"),
        "intentional DomainEvent no-state classification must remain event-bound"
    );
    assert!(
        !delta_source.contains("\n    RouteOnly {"),
        "legacy unbound RouteOnly delta must be deleted"
    );
    assert!(
        !publication_source.contains("append_event_with_route_only_domain_event"),
        "legacy RouteOnly append seam must not recur"
    );
    assert!(
        !publication_source.contains("PreparedEventStateDelta::RouteOnly"),
        "publication must use event-bound typed deltas"
    );
}
