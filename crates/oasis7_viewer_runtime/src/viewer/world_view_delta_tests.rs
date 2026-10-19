use super::*;
use crate::simulator::WorldKernel;
fn object(id: &str, x: i64) -> projection::Object {
    projection::Object {
        key: format!("entity:factory:{id}"),
        relation: false,
        bounds: None,
        value: json!({"entity_id":id,"kind":"factory","position":{"x_cm":x.to_string()},"provenance":{"freshness":"current","valid_at_source_cut":"a","observed_at_tick":"1"}}),
    }
}
fn view(cut: &str) -> Value {
    json!({"source_cut":cut,"reorg_epoch":"0","observed_at_tick":"1","completeness":"complete","next_cursor":null,"source_frozen":false,"entities":[],"relations":[],"aggregates":[],"events":[]})
}
fn prepared() -> Prepared {
    prepare(
        &WorldKernel::new().snapshot(),
        "w",
        0,
        "public",
        &SourceAuthority::local_runtime(),
    )
    .unwrap()
}
fn baseline(objects: &[projection::Object]) -> (Option<delta::Baseline>, String) {
    let mut baseline = None;
    let mut v = view("a");
    delta::snapshot(&mut v, &mut baseline, "q".into(), objects, true);
    (baseline, v["resume_cursor"].as_str().unwrap().into())
}
#[test]
fn delta_carries_unchanged_records_and_rotates_exact_sequence() {
    let initial = vec![object("a", 1)];
    let (mut baseline, token) = baseline(&initial);
    let mut next = initial.clone();
    next[0].value["provenance"]["valid_at_source_cut"] = json!("b");
    next[0].value["provenance"]["observed_at_tick"] = json!("2");
    let mut v = view("b");
    delta::update(&mut v, &mut baseline, "q", &token, &next, &prepared(), true).unwrap();
    assert_eq!(v["delivery"], "unchanged");
    assert_eq!(v["previous_sequence"], "1");
    assert_eq!(v["update_sequence"], "2");
    assert_eq!(v["carry_forward"], true);
    assert_ne!(v["resume_cursor"], token);
    assert!(
        delta::update(
            &mut view("b"),
            &mut baseline,
            "q",
            &token,
            &next,
            &prepared(),
            true
        )
        .is_err()
    );
}
#[test]
fn delta_uses_only_explicit_recycle_evidence_for_world_deletion() {
    let initial = vec![
        object("recycled", 1),
        object("hidden", 2),
        object("outside", 3),
    ];
    let (mut baseline, token) = baseline(&initial);
    let mut p = prepared();
    p.deleted_factories.insert("recycled".into());
    p.objects.push(object("outside", 3));
    let mut v = view("b");
    delta::update(&mut v, &mut baseline, "q", &token, &[], &p, true).unwrap();
    let removals = v["removals"].as_array().unwrap();
    let find = |id| removals.iter().find(|r| r["id"] == id).unwrap();
    assert_eq!(find("recycled")["world_deleted"], true);
    assert_eq!(find("hidden")["reason"], "projection_removed");
    assert_eq!(find("hidden")["world_deleted"], false);
    assert_eq!(find("outside")["reason"], "left_query_scope");
}
#[test]
fn delta_move_upserts_and_query_or_source_change_rejects_cursor() {
    let (mut baseline, token) = baseline(&[object("a", 1)]);
    assert!(
        delta::update(
            &mut view("b"),
            &mut baseline,
            "different",
            &token,
            &[],
            &prepared(),
            true
        )
        .is_err()
    );
    assert!(
        delta::update(
            &mut view("b"),
            &mut baseline,
            "q",
            &token,
            &[],
            &prepared(),
            false
        )
        .is_err()
    );
    let mut v = view("b");
    delta::update(
        &mut v,
        &mut baseline,
        "q",
        &token,
        &[object("a", 2)],
        &prepared(),
        true,
    )
    .unwrap();
    assert_eq!(v["delivery"], "delta");
    assert_eq!(v["entities"][0]["position"]["x_cm"], "2");
}
#[test]
fn delta_is_atomic_and_bounded_across_record_kinds() {
    let (mut baseline, token) = baseline(&[]);
    let objects: Vec<_> = (0..201).map(|i| object(&i.to_string(), i)).collect();
    assert_eq!(
        delta::update(
            &mut view("b"),
            &mut baseline,
            "q",
            &token,
            &objects,
            &prepared(),
            true
        ),
        Err("delta_operation_limit_exceeded")
    );
    let mut v = view("b");
    delta::update(&mut v, &mut baseline, "q", &token, &[], &prepared(), true).unwrap();
    assert_eq!(v["previous_sequence"], "1");
}
#[test]
fn only_complete_current_bounded_baselines_receive_resume_tokens() {
    for (field, value) in [
        ("source_frozen", json!(true)),
        ("next_cursor", json!("page")),
        ("completeness", json!("partial")),
    ] {
        let mut v = view("a");
        v[field] = value;
        let mut baseline = None;
        delta::snapshot(&mut v, &mut baseline, "q".into(), &[], true);
        assert!(v["resume_cursor"].is_null());
    }
    let mut huge = object("a", 1);
    huge.value["label"] = json!("x".repeat(2 * 1024 * 1024));
    let mut v = view("a");
    let mut baseline = None;
    delta::snapshot(&mut v, &mut baseline, "q".into(), &[huge], true);
    assert!(baseline.is_none());
    let many: Vec<_> = (0..1001).map(|i| object(&i.to_string(), i)).collect();
    delta::snapshot(&mut v, &mut baseline, "q".into(), &many, true);
    assert!(baseline.is_none());
}

fn bound_source() -> WorldSnapshot {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.model.agents.clear();
    snapshot.model.locations.clear();
    snapshot.model.factories.clear();
    let runtime =
        crate::runtime::World::new_with_state(crate::runtime::WorldState::default()).snapshot();
    snapshot.time = runtime.state.time;
    snapshot.runtime_snapshot = Some(runtime);
    snapshot
}
fn authority(snapshot: &WorldSnapshot) -> SourceAuthority {
    SourceAuthority::local_bound(
        crate::viewer::world_view_authority::spatial_state_digest(
            &snapshot.runtime_snapshot.as_ref().unwrap().state,
        )
        .unwrap(),
    )
}
#[test]
fn evolving_session_resets_other_audience_without_reading_old_projection() {
    let source = bound_source();
    let mut cache = None;
    let q = json!({"request_generation":"1","page_size":200});
    let first = build_session_evolving(
        &mut cache,
        || source.clone(),
        ("w", 0, "alice"),
        q.clone(),
        authority(&source),
        true,
    );
    assert!(first["resume_cursor"].is_string(), "{first}");
    let mut resume = q;
    resume["resume_cursor"] = first["resume_cursor"].clone();
    let reset = build_session_evolving(
        &mut cache,
        || panic!("must reject before reading"),
        ("w", 0, "bob"),
        resume,
        authority(&source),
        true,
    );
    assert_eq!(reset["delivery"], "reset");
    assert!(reset["entities"].as_array().unwrap().is_empty());
    assert!(cache.is_none());
}
#[test]
fn evolving_session_confirms_new_cut_without_republishing_unchanged_records() {
    let source = bound_source();
    let mut cache = None;
    let q = json!({"request_generation":"1","page_size":200});
    let first = build_session_evolving(
        &mut cache,
        || source.clone(),
        ("w", 0, "public"),
        q.clone(),
        authority(&source),
        true,
    );
    let mut advanced = source.clone();
    advanced.time += 1;
    advanced.runtime_snapshot.as_mut().unwrap().state.time = advanced.time;
    let mut resume = q;
    resume["resume_cursor"] = first["resume_cursor"].clone();
    let second = build_session_evolving(
        &mut cache,
        || advanced.clone(),
        ("w", 0, "public"),
        resume,
        authority(&advanced),
        true,
    );
    assert_eq!(second["delivery"], "unchanged", "{second}");
    assert_eq!(second["previous_sequence"], "1");
    assert_eq!(second["update_sequence"], "2");
    assert_ne!(second["source_cut"], first["source_cut"]);
}

#[test]
fn retirement_candidates_include_only_previously_visible_factory_entities() {
    let mut agent = object("agent", 0);
    agent.value["kind"] = json!("agent");
    let (baseline, _) = baseline(&[object("visible-factory", 0), agent]);
    assert_eq!(
        delta::seen_factory_ids(&baseline),
        std::collections::BTreeSet::from(["visible-factory".into()])
    );
    assert!(delta::seen_factory_ids(&None).is_empty());
}
