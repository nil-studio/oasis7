//! Inactive canonical field identity for the legacy LLM decision payload visitor.
//! Preserves numeric positions and aliases; decoding and unknown-field policy
//! remain in the original live visitor until a separate adapter is accepted.

pub const FIELD_COUNT: usize = 101;
pub const FIELD_NAMES: [&str; FIELD_COUNT] = [
    "decision",
    "action",
    "ticks",
    "to",
    "from",
    "max_amount",
    "from_owner",
    "to_owner",
    "kind",
    "amount",
    "buyer",
    "seller",
    "price_per_pu",
    "side",
    "limit_price_per_pu",
    "order_id",
    "owner",
    "compound_mass_g",
    "location_id",
    "factory_id",
    "factory_kind",
    "facility_id",
    "target_id",
    "action_kind",
    "base_cost_class",
    "base_risk_class",
    "blocker_type",
    "recipe_id",
    "batches",
    "publisher",
    "installer",
    "bidder",
    "module_id",
    "module_version",
    "install_target_type",
    "install_target_location_id",
    "manifest_path",
    "source_files",
    "wasm_hash",
    "wasm_bytes_hex",
    "price_kind",
    "price_amount",
    "bid_order_id",
    "activate",
    "actor",
    "declarer",
    "subject",
    "object",
    "schema_id",
    "claim",
    "confidence_ppm",
    "evidence_event_ids",
    "ttl_ticks",
    "stake",
    "challenger",
    "fact_id",
    "reason",
    "adjudicator",
    "adjudication",
    "notes",
    "relation_kind",
    "weight_bps",
    "backing_fact_ids",
    "proposer_agent_id",
    "alliance_id",
    "members",
    "member_agent_id",
    "charter",
    "initiator_agent_id",
    "war_id",
    "aggressor_alliance_id",
    "defender_alliance_id",
    "objective",
    "intensity",
    "proposal_key",
    "title",
    "description",
    "options",
    "voting_window_ticks",
    "quorum_weight",
    "pass_threshold_bps",
    "voter_agent_id",
    "option",
    "weight",
    "resolver_agent_id",
    "crisis_id",
    "strategy",
    "success",
    "operator_agent_id",
    "target_agent_id",
    "track",
    "points",
    "achievement_id",
    "creator_agent_id",
    "contract_id",
    "counterparty_agent_id",
    "settlement_kind",
    "settlement_amount",
    "reputation_stake",
    "expires_at",
    "accepter_agent_id",
];

pub const STRUCT_FIELDS: &[&str] = &[
    "decision",
    "action",
    "ticks",
    "to",
    "from",
    "max_amount",
    "from_owner",
    "to_owner",
    "kind",
    "amount",
    "buyer",
    "seller",
    "price_per_pu",
    "side",
    "limit_price_per_pu",
    "order_id",
    "owner",
    "compound_mass_g",
    "location_id",
    "factory_id",
    "factory_kind",
    "facility_id",
    "target_id",
    "action_kind",
    "base_cost_class",
    "base_risk_class",
    "blocker_type",
    "recipe_id",
    "batches",
    "publisher",
    "installer",
    "bidder",
    "module_id",
    "module_version",
    "install_target_type",
    "install_target_location_id",
    "manifest_path",
    "source_files",
    "wasm_hash",
    "wasm_bytes_hex",
    "price_kind",
    "price_amount",
    "bid_order_id",
    "activate",
    "actor",
    "declarer",
    "subject",
    "object",
    "schema_id",
    "claim",
    "confidence_ppm",
    "evidence_event_ids",
    "ttl_ticks",
    "stake",
    "challenger",
    "fact_id",
    "reason",
    "adjudicator",
    "adjudication",
    "adjudication_decision",
    "verdict",
    "notes",
    "relation_kind",
    "weight_bps",
    "backing_fact_ids",
    "proposer_agent_id",
    "alliance_id",
    "members",
    "member_agent_id",
    "charter",
    "initiator_agent_id",
    "war_id",
    "aggressor_alliance_id",
    "defender_alliance_id",
    "objective",
    "intensity",
    "proposal_key",
    "title",
    "description",
    "options",
    "voting_window_ticks",
    "quorum_weight",
    "pass_threshold_bps",
    "voter_agent_id",
    "option",
    "weight",
    "resolver_agent_id",
    "crisis_id",
    "strategy",
    "success",
    "operator_agent_id",
    "target_agent_id",
    "track",
    "points",
    "achievement_id",
    "creator_agent_id",
    "contract_id",
    "counterparty_agent_id",
    "settlement_kind",
    "settlement_amount",
    "reputation_stake",
    "expires_at",
    "accepter_agent_id",
];

pub fn field_index(value: &str) -> Option<usize> {
    match value.as_bytes().first().copied() {
        Some(b'a') => parse_name_61(value),
        Some(b'b') => parse_name_62(value),
        Some(b'c') => parse_name_63(value),
        Some(b'd') => parse_name_64(value),
        Some(b'e') => parse_name_65(value),
        Some(b'f') => parse_name_66(value),
        Some(b'i') => parse_name_69(value),
        Some(b'k') => parse_name_6b(value),
        Some(b'l') => parse_name_6c(value),
        Some(b'm') => parse_name_6d(value),
        Some(b'n') => parse_name_6e(value),
        Some(b'o') => parse_name_6f(value),
        Some(b'p') => parse_name_70(value),
        Some(b'q') => parse_name_71(value),
        Some(b'r') => parse_name_72(value),
        Some(b's') => parse_name_73(value),
        Some(b't') => parse_name_74(value),
        Some(b'v') => parse_name_76(value),
        Some(b'w') => parse_name_77(value),
        _ => None,
    }
}

fn parse_name_61(value: &str) -> Option<usize> {
    match value {
        "action" => Some(1),
        "amount" => Some(9),
        "action_kind" => Some(23),
        "activate" => Some(43),
        "actor" => Some(44),
        "adjudicator" => Some(57),
        "adjudication" => Some(58),
        "adjudication_decision" => Some(58),
        "alliance_id" => Some(64),
        "aggressor_alliance_id" => Some(70),
        "achievement_id" => Some(92),
        "accepter_agent_id" => Some(100),
        _ => None,
    }
}

fn parse_name_62(value: &str) -> Option<usize> {
    match value {
        "buyer" => Some(10),
        "base_cost_class" => Some(24),
        "base_risk_class" => Some(25),
        "blocker_type" => Some(26),
        "batches" => Some(28),
        "bidder" => Some(31),
        "bid_order_id" => Some(42),
        "backing_fact_ids" => Some(62),
        _ => None,
    }
}

fn parse_name_63(value: &str) -> Option<usize> {
    match value {
        "compound_mass_g" => Some(17),
        "claim" => Some(49),
        "confidence_ppm" => Some(50),
        "challenger" => Some(54),
        "charter" => Some(67),
        "crisis_id" => Some(85),
        "creator_agent_id" => Some(93),
        "contract_id" => Some(94),
        "counterparty_agent_id" => Some(95),
        _ => None,
    }
}

fn parse_name_64(value: &str) -> Option<usize> {
    match value {
        "decision" => Some(0),
        "declarer" => Some(45),
        "defender_alliance_id" => Some(71),
        "description" => Some(76),
        _ => None,
    }
}

fn parse_name_65(value: &str) -> Option<usize> {
    match value {
        "evidence_event_ids" => Some(51),
        "expires_at" => Some(99),
        _ => None,
    }
}

fn parse_name_66(value: &str) -> Option<usize> {
    match value {
        "from" => Some(4),
        "from_owner" => Some(6),
        "factory_id" => Some(19),
        "factory_kind" => Some(20),
        "facility_id" => Some(21),
        "fact_id" => Some(55),
        _ => None,
    }
}

fn parse_name_69(value: &str) -> Option<usize> {
    match value {
        "installer" => Some(30),
        "install_target_type" => Some(34),
        "install_target_location_id" => Some(35),
        "initiator_agent_id" => Some(68),
        "intensity" => Some(73),
        _ => None,
    }
}

fn parse_name_6b(value: &str) -> Option<usize> {
    match value {
        "kind" => Some(8),
        _ => None,
    }
}

fn parse_name_6c(value: &str) -> Option<usize> {
    match value {
        "limit_price_per_pu" => Some(14),
        "location_id" => Some(18),
        _ => None,
    }
}

fn parse_name_6d(value: &str) -> Option<usize> {
    match value {
        "max_amount" => Some(5),
        "module_id" => Some(32),
        "module_version" => Some(33),
        "manifest_path" => Some(36),
        "members" => Some(65),
        "member_agent_id" => Some(66),
        _ => None,
    }
}

fn parse_name_6e(value: &str) -> Option<usize> {
    match value {
        "notes" => Some(59),
        _ => None,
    }
}

fn parse_name_6f(value: &str) -> Option<usize> {
    match value {
        "order_id" => Some(15),
        "owner" => Some(16),
        "object" => Some(47),
        "objective" => Some(72),
        "options" => Some(77),
        "option" => Some(82),
        "operator_agent_id" => Some(88),
        _ => None,
    }
}

fn parse_name_70(value: &str) -> Option<usize> {
    match value {
        "price_per_pu" => Some(12),
        "publisher" => Some(29),
        "price_kind" => Some(40),
        "price_amount" => Some(41),
        "proposer_agent_id" => Some(63),
        "proposal_key" => Some(74),
        "pass_threshold_bps" => Some(80),
        "points" => Some(91),
        _ => None,
    }
}

fn parse_name_71(value: &str) -> Option<usize> {
    match value {
        "quorum_weight" => Some(79),
        _ => None,
    }
}

fn parse_name_72(value: &str) -> Option<usize> {
    match value {
        "recipe_id" => Some(27),
        "reason" => Some(56),
        "relation_kind" => Some(60),
        "resolver_agent_id" => Some(84),
        "reputation_stake" => Some(98),
        _ => None,
    }
}

fn parse_name_73(value: &str) -> Option<usize> {
    match value {
        "seller" => Some(11),
        "side" => Some(13),
        "source_files" => Some(37),
        "subject" => Some(46),
        "schema_id" => Some(48),
        "stake" => Some(53),
        "strategy" => Some(86),
        "success" => Some(87),
        "settlement_kind" => Some(96),
        "settlement_amount" => Some(97),
        _ => None,
    }
}

fn parse_name_74(value: &str) -> Option<usize> {
    match value {
        "ticks" => Some(2),
        "to" => Some(3),
        "to_owner" => Some(7),
        "target_id" => Some(22),
        "ttl_ticks" => Some(52),
        "title" => Some(75),
        "target_agent_id" => Some(89),
        "track" => Some(90),
        _ => None,
    }
}

fn parse_name_76(value: &str) -> Option<usize> {
    match value {
        "verdict" => Some(58),
        "voting_window_ticks" => Some(78),
        "voter_agent_id" => Some(81),
        _ => None,
    }
}

fn parse_name_77(value: &str) -> Option<usize> {
    match value {
        "wasm_hash" => Some(38),
        "wasm_bytes_hex" => Some(39),
        "weight_bps" => Some(61),
        "war_id" => Some(69),
        "weight" => Some(83),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_legacy_fields_keep_their_numeric_identity() {
        for (index, name) in FIELD_NAMES.iter().enumerate() {
            assert_eq!(field_index(name), Some(index), "{name}");
        }
        assert_eq!(FIELD_COUNT, 101);
    }

    #[test]
    fn legacy_aliases_share_the_adjudication_slot() {
        assert_eq!(field_index("adjudication"), Some(58));
        assert_eq!(field_index("adjudication_decision"), Some(58));
        assert_eq!(field_index("verdict"), Some(58));
        for name in STRUCT_FIELDS {
            assert!(field_index(name).is_some(), "{name}");
        }
    }

    #[test]
    fn unknown_fields_do_not_acquire_a_known_identity() {
        for name in ["", "Decision", "unknown", "adjudication_extra", "☃"] {
            assert_eq!(field_index(name), None);
        }
    }

    #[test]
    fn original_all_fields_golden_covers_each_slot_exactly_once() {
        let raw = include_str!("fixtures/llm_decision_payload_legacy_all_fields.json");
        let value: serde_json::Value = serde_json::from_str(raw).expect("original golden payload");
        let object = value.as_object().expect("golden object");
        assert_eq!(object.len(), FIELD_COUNT);
        let mut occupied = [false; FIELD_COUNT];
        for name in object.keys() {
            let index = field_index(name).expect("known golden field");
            assert!(!occupied[index], "duplicate field identity: {name}");
            occupied[index] = true;
        }
        assert!(occupied.iter().all(|slot| *slot));
    }
}
