use super::*;

pub(super) fn assert_fine_grain_translation_roundtrip(
    value: &serde_json::Value,
    kernel: &WorldKernel,
    journal_before: &WorldJournal,
) {
    for (expected_value_class, expected_translation) in [
        (
            "replacement_available",
            serde_json::json!({"requested_granularity":"place a block at the smelter site","why_fine_action_deferred":"block editing is not an available world action","canonical_replacement_action":"build_factory_smelter_mk1","closest_playable_goal":"establish the first smelter capability","player_next_step_hint":"Use Build the first smelter to create the supported facility.","replacement_value_class":"replacement_available"}),
        ),
        (
            "no_safe_replacement",
            serde_json::json!({"requested_granularity":"attack the blocked conveyor directly","why_fine_action_deferred":"direct combat and local physics are not available world actions","canonical_replacement_action":null,"closest_playable_goal":"choose a supported recovery or a new goal","player_next_step_hint":"Reprioritize before committing another action; no published action safely replaces this request.","replacement_value_class":"no_safe_replacement"}),
        ),
        (
            "future_embodied_candidate",
            serde_json::json!({"requested_granularity":"jump onto the refinery platform","why_fine_action_deferred":"embodied movement is only a future candidate, not a current runtime capability","canonical_replacement_action":null,"closest_playable_goal":"inspect and advance the supported refinery plan","player_next_step_hint":"Use the published goal actions now; revisit embodied movement only when that capability is opened.","replacement_value_class":"future_embodied_candidate"}),
        ),
    ] {
        let mut translated_value = value.clone();
        translated_value["player_gameplay"]["fine_grain_action_translation"] =
            expected_translation.clone();
        let translated = WorldSnapshot::from_json(
            &serde_json::to_string(&translated_value).expect("serialize translated snapshot"),
        )
        .expect("load fine-grain translation snapshot");
        let translation = serde_json::to_value(translated).expect("serialize translated snapshot")
            ["player_gameplay"]["fine_grain_action_translation"]
            .clone();
        for field in [
            "requested_granularity",
            "why_fine_action_deferred",
            "canonical_replacement_action",
            "closest_playable_goal",
            "player_next_step_hint",
            "replacement_value_class",
        ] {
            assert_eq!(
                translation[field], expected_translation[field],
                "{expected_value_class} retains {field}"
            );
        }
    }
    assert_eq!(
        kernel.journal_snapshot(),
        *journal_before,
        "advisory fine-grain translations must not append runtime journal entries"
    );
}

pub(super) fn assert_legacy_micro_depot_evidence(gameplay: &PlayerGameplaySnapshot) {
    let facility = gameplay
        .micro_depot_facilities
        .first()
        .expect("canonical player gameplay snapshot exposes the micro-depot facility");
    assert_eq!(facility.facility_id, "depot-public-snapshot");
    assert_eq!(facility.available_units_by_kind.get("data"), Some(&5));
    assert_eq!(facility.module_id, "regional.micro_depot");
    assert_eq!(facility.module_version, "0.2.0");
    assert_eq!(
        facility.wasm_hash, "sha256:micro-depot-public-evidence",
        "canonical player gameplay snapshot preserves module evidence"
    );
    assert_eq!(
        facility.last_receipt_id.as_deref(),
        Some("receipt-micro-depot-public")
    );
    assert_eq!(
        facility.last_proposal_hash.as_deref(),
        Some("sha256:proposal-public")
    );
}

pub(super) fn assert_legacy_omits_fine_grain_translation(snapshot: &WorldSnapshot) {
    let value = serde_json::to_value(snapshot).expect("serialize legacy player gameplay snapshot");
    assert!(
        value["player_gameplay"]
            .get("fine_grain_action_translation")
            .is_none(),
        "legacy snapshots deserialize without inventing a fine-grain translation"
    );
}
