//! Builtin scenario identifiers shared by launchers and the simulation kernel.
//!
//! These helpers classify names only. Scenario files, initialization and world
//! execution remain owned by the simulator.

/// Stable, ordered canonical IDs exposed by `WorldScenario::variants`.
pub const WORLD_SCENARIO_IDS: &[&str] = &[
    "minimal",
    "two_bases",
    "llm_bootstrap",
    "power_bootstrap",
    "resource_bootstrap",
    "twin_region_bootstrap",
    "triad_region_bootstrap",
    "triad_p2p_bootstrap",
    "asteroid_fragment_bootstrap",
    "asteroid_fragment_detail_bootstrap",
    "asteroid_fragment_twin_region_bootstrap",
    "asteroid_fragment_triad_region_bootstrap",
];

/// Resolve a builtin ID or existing CLI alias, preserving whitespace and case handling.
pub fn canonical_world_scenario_id(input: &str) -> Option<&'static str> {
    match input.trim().to_lowercase().as_str() {
        "minimal" => Some("minimal"),
        "two_bases" | "two-bases" => Some("two_bases"),
        "llm_bootstrap" | "llm-bootstrap" | "llm" => Some("llm_bootstrap"),
        "power_bootstrap" | "power-bootstrap" | "bootstrap" => Some("power_bootstrap"),
        "resource_bootstrap" | "resource-bootstrap" | "resources" => Some("resource_bootstrap"),
        "twin_region_bootstrap" | "twin-region-bootstrap" | "twin_regions" | "twin-regions" => {
            Some("twin_region_bootstrap")
        }
        "triad_region_bootstrap" | "triad-region-bootstrap" | "triad_regions" | "triad-regions" => {
            Some("triad_region_bootstrap")
        }
        "triad_p2p_bootstrap"
        | "triad-p2p-bootstrap"
        | "triad-p2p"
        | "p2p-triad"
        | "p2p-triad-bootstrap" => Some("triad_p2p_bootstrap"),
        "asteroid_fragment_bootstrap"
        | "asteroid-fragment-bootstrap"
        | "asteroid_fragment"
        | "asteroid-fragment" => Some("asteroid_fragment_bootstrap"),
        "asteroid_fragment_detail_bootstrap"
        | "asteroid-fragment-detail-bootstrap"
        | "asteroid_fragment_detail"
        | "asteroid-fragment-detail" => Some("asteroid_fragment_detail_bootstrap"),
        "asteroid_fragment_twin_region_bootstrap"
        | "asteroid-fragment-twin-region-bootstrap"
        | "asteroid-fragment-twin-regions"
        | "asteroid-fragment-regions" => Some("asteroid_fragment_twin_region_bootstrap"),
        "asteroid_fragment_triad_region_bootstrap"
        | "asteroid-fragment-triad-region-bootstrap"
        | "asteroid-fragment-triad-regions"
        | "asteroid-fragment-triad" => Some("asteroid_fragment_triad_region_bootstrap"),
        _ => None,
    }
}

/// Whether this builtin name requires the launcher's explicit debug-scenario opt-in.
pub fn is_debug_world_scenario(input: &str) -> bool {
    canonical_world_scenario_id(input) == Some("llm_bootstrap")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_ids_are_unique_and_resolve_without_aliases() {
        let ids: std::collections::BTreeSet<_> = WORLD_SCENARIO_IDS.iter().copied().collect();
        assert_eq!(ids.len(), WORLD_SCENARIO_IDS.len());
        for id in WORLD_SCENARIO_IDS {
            assert_eq!(canonical_world_scenario_id(id), Some(*id));
        }
        for unknown in ["", "   ", "unknown", "llm_bootstrap_extra", "llm bootstrap"] {
            assert_eq!(canonical_world_scenario_id(unknown), None);
            assert!(!is_debug_world_scenario(unknown));
        }
    }

    #[test]
    fn debug_classification_preserves_all_aliases_and_case_handling() {
        for alias in [
            "llm_bootstrap",
            "llm-bootstrap",
            "llm",
            " LLM-BOOTSTRAP ",
            "\tLlM\n",
        ] {
            assert_eq!(canonical_world_scenario_id(alias), Some("llm_bootstrap"));
            assert!(is_debug_world_scenario(alias));
        }
        for id in WORLD_SCENARIO_IDS {
            assert_eq!(is_debug_world_scenario(id), *id == "llm_bootstrap");
        }
        assert_eq!(
            canonical_world_scenario_id(" BOOTSTRAP "),
            Some("power_bootstrap")
        );
        assert!(!is_debug_world_scenario("bootstrap"));
    }
}
