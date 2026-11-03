//! Manual Serde deserialization for the large LLM decision payload.
//!
//! Field identity stays in the original declaration order. Map values are decoded
//! immediately through MapAccess; sequence values keep the same global positions.

use super::{
    LlmDecisionPayload, LlmSocialStakePayload, MicroDepotActionKind, MicroDepotPressureClass,
};
use serde::Deserialize;
use serde::de::{
    self, Deserializer, Error as _, Expected, IgnoredAny, MapAccess, SeqAccess, Visitor,
};
use std::collections::BTreeMap;
use std::fmt;

const FIELD_COUNT: usize = 101;
const FIELD_NAMES: [&str; FIELD_COUNT] = [
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

const STRUCT_FIELDS: &[&str] = &[
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

#[derive(Clone, Copy)]
enum FieldKey {
    Known(usize),
    Unknown,
}

struct FieldIdentifierVisitor;

impl<'de> Visitor<'de> for FieldIdentifierVisitor {
    type Value = FieldKey;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("field identifier")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(field_index(value).map_or(FieldKey::Unknown, FieldKey::Known))
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let value = String::from_utf8_lossy(value);
        self.visit_str(&value)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if value < FIELD_COUNT as u64 {
            Ok(FieldKey::Known(value as usize))
        } else {
            Ok(FieldKey::Unknown)
        }
    }
}

impl<'de> Deserialize<'de> for FieldKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_identifier(FieldIdentifierVisitor)
    }
}

fn field_index(value: &str) -> Option<usize> {
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

fn read_map_group_0<'de, A>(
    index: usize,
    map: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match index {
        0 => payload.decision = map.next_value::<String>()?,
        1 => payload.action = map.next_value::<Option<serde_json::Value>>()?,
        2 => payload.ticks = map.next_value::<Option<u64>>()?,
        3 => payload.to = map.next_value::<Option<String>>()?,
        4 => payload.from = map.next_value::<Option<String>>()?,
        5 => payload.max_amount = map.next_value::<Option<i64>>()?,
        6 => payload.from_owner = map.next_value::<Option<String>>()?,
        7 => payload.to_owner = map.next_value::<Option<String>>()?,
        8 => payload.kind = map.next_value::<Option<String>>()?,
        9 => payload.amount = map.next_value::<Option<i64>>()?,
        10 => payload.buyer = map.next_value::<Option<String>>()?,
        11 => payload.seller = map.next_value::<Option<String>>()?,
        12 => payload.price_per_pu = map.next_value::<Option<i64>>()?,
        13 => payload.side = map.next_value::<Option<String>>()?,
        14 => payload.limit_price_per_pu = map.next_value::<Option<i64>>()?,
        15 => payload.order_id = map.next_value::<Option<u64>>()?,
        16 => payload.owner = map.next_value::<Option<String>>()?,
        17 => payload.compound_mass_g = map.next_value::<Option<i64>>()?,
        18 => payload.location_id = map.next_value::<Option<String>>()?,
        19 => payload.factory_id = map.next_value::<Option<String>>()?,
        20 => payload.factory_kind = map.next_value::<Option<String>>()?,
        _ => {
            return Err(A::Error::custom(
                "invalid LlmDecisionPayload map group index",
            ));
        }
    }
    Ok(())
}

fn read_map_group_1<'de, A>(
    index: usize,
    map: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match index {
        21 => payload.facility_id = map.next_value::<Option<String>>()?,
        22 => payload.target_id = map.next_value::<Option<String>>()?,
        23 => payload.action_kind = map.next_value::<Option<MicroDepotActionKind>>()?,
        24 => payload.base_cost_class = map.next_value::<Option<MicroDepotPressureClass>>()?,
        25 => payload.base_risk_class = map.next_value::<Option<MicroDepotPressureClass>>()?,
        26 => payload.blocker_type = map.next_value::<Option<String>>()?,
        27 => payload.recipe_id = map.next_value::<Option<String>>()?,
        28 => payload.batches = map.next_value::<Option<i64>>()?,
        29 => payload.publisher = map.next_value::<Option<String>>()?,
        30 => payload.installer = map.next_value::<Option<String>>()?,
        31 => payload.bidder = map.next_value::<Option<String>>()?,
        32 => payload.module_id = map.next_value::<Option<String>>()?,
        33 => payload.module_version = map.next_value::<Option<String>>()?,
        34 => payload.install_target_type = map.next_value::<Option<String>>()?,
        35 => payload.install_target_location_id = map.next_value::<Option<String>>()?,
        36 => payload.manifest_path = map.next_value::<Option<String>>()?,
        37 => payload.source_files = map.next_value::<Option<BTreeMap<String, String>>>()?,
        38 => payload.wasm_hash = map.next_value::<Option<String>>()?,
        39 => payload.wasm_bytes_hex = map.next_value::<Option<String>>()?,
        40 => payload.price_kind = map.next_value::<Option<String>>()?,
        41 => payload.price_amount = map.next_value::<Option<i64>>()?,
        42 => payload.bid_order_id = map.next_value::<Option<u64>>()?,
        43 => payload.activate = map.next_value::<Option<bool>>()?,
        _ => {
            return Err(A::Error::custom(
                "invalid LlmDecisionPayload map group index",
            ));
        }
    }
    Ok(())
}

fn read_map_group_2<'de, A>(
    index: usize,
    map: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match index {
        44 => payload.actor = map.next_value::<Option<String>>()?,
        45 => payload.declarer = map.next_value::<Option<String>>()?,
        46 => payload.subject = map.next_value::<Option<String>>()?,
        47 => payload.object = map.next_value::<Option<String>>()?,
        48 => payload.schema_id = map.next_value::<Option<String>>()?,
        49 => payload.claim = map.next_value::<Option<String>>()?,
        50 => payload.confidence_ppm = map.next_value::<Option<i64>>()?,
        51 => payload.evidence_event_ids = map.next_value::<Option<Vec<u64>>>()?,
        52 => payload.ttl_ticks = map.next_value::<Option<u64>>()?,
        53 => payload.stake = map.next_value::<Option<LlmSocialStakePayload>>()?,
        54 => payload.challenger = map.next_value::<Option<String>>()?,
        55 => payload.fact_id = map.next_value::<Option<u64>>()?,
        56 => payload.reason = map.next_value::<Option<String>>()?,
        57 => payload.adjudicator = map.next_value::<Option<String>>()?,
        58 => payload.adjudication = map.next_value::<Option<String>>()?,
        59 => payload.notes = map.next_value::<Option<String>>()?,
        60 => payload.relation_kind = map.next_value::<Option<String>>()?,
        61 => payload.weight_bps = map.next_value::<Option<i64>>()?,
        62 => payload.backing_fact_ids = map.next_value::<Option<Vec<u64>>>()?,
        _ => {
            return Err(A::Error::custom(
                "invalid LlmDecisionPayload map group index",
            ));
        }
    }
    Ok(())
}

fn read_map_group_3<'de, A>(
    index: usize,
    map: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match index {
        63 => payload.proposer_agent_id = map.next_value::<Option<String>>()?,
        64 => payload.alliance_id = map.next_value::<Option<String>>()?,
        65 => payload.members = map.next_value::<Option<Vec<String>>>()?,
        66 => payload.member_agent_id = map.next_value::<Option<String>>()?,
        67 => payload.charter = map.next_value::<Option<String>>()?,
        68 => payload.initiator_agent_id = map.next_value::<Option<String>>()?,
        69 => payload.war_id = map.next_value::<Option<String>>()?,
        70 => payload.aggressor_alliance_id = map.next_value::<Option<String>>()?,
        71 => payload.defender_alliance_id = map.next_value::<Option<String>>()?,
        72 => payload.objective = map.next_value::<Option<String>>()?,
        73 => payload.intensity = map.next_value::<Option<u32>>()?,
        _ => {
            return Err(A::Error::custom(
                "invalid LlmDecisionPayload map group index",
            ));
        }
    }
    Ok(())
}

fn read_map_group_4<'de, A>(
    index: usize,
    map: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match index {
        74 => payload.proposal_key = map.next_value::<Option<String>>()?,
        75 => payload.title = map.next_value::<Option<String>>()?,
        76 => payload.description = map.next_value::<Option<String>>()?,
        77 => payload.options = map.next_value::<Option<Vec<String>>>()?,
        78 => payload.voting_window_ticks = map.next_value::<Option<u64>>()?,
        79 => payload.quorum_weight = map.next_value::<Option<u64>>()?,
        80 => payload.pass_threshold_bps = map.next_value::<Option<u16>>()?,
        81 => payload.voter_agent_id = map.next_value::<Option<String>>()?,
        82 => payload.option = map.next_value::<Option<String>>()?,
        83 => payload.weight = map.next_value::<Option<u32>>()?,
        84 => payload.resolver_agent_id = map.next_value::<Option<String>>()?,
        85 => payload.crisis_id = map.next_value::<Option<String>>()?,
        86 => payload.strategy = map.next_value::<Option<String>>()?,
        87 => payload.success = map.next_value::<Option<bool>>()?,
        _ => {
            return Err(A::Error::custom(
                "invalid LlmDecisionPayload map group index",
            ));
        }
    }
    Ok(())
}

fn read_map_group_5<'de, A>(
    index: usize,
    map: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    match index {
        88 => payload.operator_agent_id = map.next_value::<Option<String>>()?,
        89 => payload.target_agent_id = map.next_value::<Option<String>>()?,
        90 => payload.track = map.next_value::<Option<String>>()?,
        91 => payload.points = map.next_value::<Option<i64>>()?,
        92 => payload.achievement_id = map.next_value::<Option<String>>()?,
        93 => payload.creator_agent_id = map.next_value::<Option<String>>()?,
        94 => payload.contract_id = map.next_value::<Option<String>>()?,
        95 => payload.counterparty_agent_id = map.next_value::<Option<String>>()?,
        96 => payload.settlement_kind = map.next_value::<Option<String>>()?,
        97 => payload.settlement_amount = map.next_value::<Option<i64>>()?,
        98 => payload.reputation_stake = map.next_value::<Option<i64>>()?,
        99 => payload.expires_at = map.next_value::<Option<u64>>()?,
        100 => payload.accepter_agent_id = map.next_value::<Option<String>>()?,
        _ => {
            return Err(A::Error::custom(
                "invalid LlmDecisionPayload map group index",
            ));
        }
    }
    Ok(())
}

fn read_seq_group_0<'de, A>(
    sequence: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
    A::Error: de::Error,
{
    payload.decision = sequence
        .next_element::<String>()?
        .ok_or_else(|| A::Error::invalid_length(0, &SEQUENCE_EXPECTED))?;
    payload.action = sequence
        .next_element::<Option<serde_json::Value>>()?
        .ok_or_else(|| A::Error::invalid_length(1, &SEQUENCE_EXPECTED))?;
    payload.ticks = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(2, &SEQUENCE_EXPECTED))?;
    payload.to = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(3, &SEQUENCE_EXPECTED))?;
    payload.from = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(4, &SEQUENCE_EXPECTED))?;
    payload.max_amount = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(5, &SEQUENCE_EXPECTED))?;
    payload.from_owner = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(6, &SEQUENCE_EXPECTED))?;
    payload.to_owner = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(7, &SEQUENCE_EXPECTED))?;
    payload.kind = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(8, &SEQUENCE_EXPECTED))?;
    payload.amount = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(9, &SEQUENCE_EXPECTED))?;
    payload.buyer = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(10, &SEQUENCE_EXPECTED))?;
    payload.seller = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(11, &SEQUENCE_EXPECTED))?;
    payload.price_per_pu = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(12, &SEQUENCE_EXPECTED))?;
    payload.side = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(13, &SEQUENCE_EXPECTED))?;
    payload.limit_price_per_pu = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(14, &SEQUENCE_EXPECTED))?;
    payload.order_id = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(15, &SEQUENCE_EXPECTED))?;
    payload.owner = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(16, &SEQUENCE_EXPECTED))?;
    payload.compound_mass_g = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(17, &SEQUENCE_EXPECTED))?;
    payload.location_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(18, &SEQUENCE_EXPECTED))?;
    payload.factory_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(19, &SEQUENCE_EXPECTED))?;
    payload.factory_kind = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(20, &SEQUENCE_EXPECTED))?;
    Ok(())
}

fn read_seq_group_1<'de, A>(
    sequence: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
    A::Error: de::Error,
{
    payload.facility_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(21, &SEQUENCE_EXPECTED))?;
    payload.target_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(22, &SEQUENCE_EXPECTED))?;
    payload.action_kind = sequence
        .next_element::<Option<MicroDepotActionKind>>()?
        .ok_or_else(|| A::Error::invalid_length(23, &SEQUENCE_EXPECTED))?;
    payload.base_cost_class = sequence
        .next_element::<Option<MicroDepotPressureClass>>()?
        .ok_or_else(|| A::Error::invalid_length(24, &SEQUENCE_EXPECTED))?;
    payload.base_risk_class = sequence
        .next_element::<Option<MicroDepotPressureClass>>()?
        .ok_or_else(|| A::Error::invalid_length(25, &SEQUENCE_EXPECTED))?;
    payload.blocker_type = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(26, &SEQUENCE_EXPECTED))?;
    payload.recipe_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(27, &SEQUENCE_EXPECTED))?;
    payload.batches = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(28, &SEQUENCE_EXPECTED))?;
    payload.publisher = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(29, &SEQUENCE_EXPECTED))?;
    payload.installer = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(30, &SEQUENCE_EXPECTED))?;
    payload.bidder = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(31, &SEQUENCE_EXPECTED))?;
    payload.module_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(32, &SEQUENCE_EXPECTED))?;
    payload.module_version = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(33, &SEQUENCE_EXPECTED))?;
    payload.install_target_type = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(34, &SEQUENCE_EXPECTED))?;
    payload.install_target_location_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(35, &SEQUENCE_EXPECTED))?;
    payload.manifest_path = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(36, &SEQUENCE_EXPECTED))?;
    payload.source_files = sequence
        .next_element::<Option<BTreeMap<String, String>>>()?
        .ok_or_else(|| A::Error::invalid_length(37, &SEQUENCE_EXPECTED))?;
    payload.wasm_hash = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(38, &SEQUENCE_EXPECTED))?;
    payload.wasm_bytes_hex = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(39, &SEQUENCE_EXPECTED))?;
    payload.price_kind = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(40, &SEQUENCE_EXPECTED))?;
    payload.price_amount = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(41, &SEQUENCE_EXPECTED))?;
    payload.bid_order_id = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(42, &SEQUENCE_EXPECTED))?;
    payload.activate = sequence
        .next_element::<Option<bool>>()?
        .ok_or_else(|| A::Error::invalid_length(43, &SEQUENCE_EXPECTED))?;
    Ok(())
}

fn read_seq_group_2<'de, A>(
    sequence: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
    A::Error: de::Error,
{
    payload.actor = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(44, &SEQUENCE_EXPECTED))?;
    payload.declarer = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(45, &SEQUENCE_EXPECTED))?;
    payload.subject = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(46, &SEQUENCE_EXPECTED))?;
    payload.object = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(47, &SEQUENCE_EXPECTED))?;
    payload.schema_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(48, &SEQUENCE_EXPECTED))?;
    payload.claim = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(49, &SEQUENCE_EXPECTED))?;
    payload.confidence_ppm = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(50, &SEQUENCE_EXPECTED))?;
    payload.evidence_event_ids = sequence
        .next_element::<Option<Vec<u64>>>()?
        .ok_or_else(|| A::Error::invalid_length(51, &SEQUENCE_EXPECTED))?;
    payload.ttl_ticks = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(52, &SEQUENCE_EXPECTED))?;
    payload.stake = sequence
        .next_element::<Option<LlmSocialStakePayload>>()?
        .ok_or_else(|| A::Error::invalid_length(53, &SEQUENCE_EXPECTED))?;
    payload.challenger = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(54, &SEQUENCE_EXPECTED))?;
    payload.fact_id = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(55, &SEQUENCE_EXPECTED))?;
    payload.reason = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(56, &SEQUENCE_EXPECTED))?;
    payload.adjudicator = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(57, &SEQUENCE_EXPECTED))?;
    payload.adjudication = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(58, &SEQUENCE_EXPECTED))?;
    payload.notes = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(59, &SEQUENCE_EXPECTED))?;
    payload.relation_kind = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(60, &SEQUENCE_EXPECTED))?;
    payload.weight_bps = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(61, &SEQUENCE_EXPECTED))?;
    payload.backing_fact_ids = sequence
        .next_element::<Option<Vec<u64>>>()?
        .ok_or_else(|| A::Error::invalid_length(62, &SEQUENCE_EXPECTED))?;
    Ok(())
}

fn read_seq_group_3<'de, A>(
    sequence: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
    A::Error: de::Error,
{
    payload.proposer_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(63, &SEQUENCE_EXPECTED))?;
    payload.alliance_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(64, &SEQUENCE_EXPECTED))?;
    payload.members = sequence
        .next_element::<Option<Vec<String>>>()?
        .ok_or_else(|| A::Error::invalid_length(65, &SEQUENCE_EXPECTED))?;
    payload.member_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(66, &SEQUENCE_EXPECTED))?;
    payload.charter = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(67, &SEQUENCE_EXPECTED))?;
    payload.initiator_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(68, &SEQUENCE_EXPECTED))?;
    payload.war_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(69, &SEQUENCE_EXPECTED))?;
    payload.aggressor_alliance_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(70, &SEQUENCE_EXPECTED))?;
    payload.defender_alliance_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(71, &SEQUENCE_EXPECTED))?;
    payload.objective = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(72, &SEQUENCE_EXPECTED))?;
    payload.intensity = sequence
        .next_element::<Option<u32>>()?
        .ok_or_else(|| A::Error::invalid_length(73, &SEQUENCE_EXPECTED))?;
    Ok(())
}

fn read_seq_group_4<'de, A>(
    sequence: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
    A::Error: de::Error,
{
    payload.proposal_key = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(74, &SEQUENCE_EXPECTED))?;
    payload.title = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(75, &SEQUENCE_EXPECTED))?;
    payload.description = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(76, &SEQUENCE_EXPECTED))?;
    payload.options = sequence
        .next_element::<Option<Vec<String>>>()?
        .ok_or_else(|| A::Error::invalid_length(77, &SEQUENCE_EXPECTED))?;
    payload.voting_window_ticks = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(78, &SEQUENCE_EXPECTED))?;
    payload.quorum_weight = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(79, &SEQUENCE_EXPECTED))?;
    payload.pass_threshold_bps = sequence
        .next_element::<Option<u16>>()?
        .ok_or_else(|| A::Error::invalid_length(80, &SEQUENCE_EXPECTED))?;
    payload.voter_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(81, &SEQUENCE_EXPECTED))?;
    payload.option = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(82, &SEQUENCE_EXPECTED))?;
    payload.weight = sequence
        .next_element::<Option<u32>>()?
        .ok_or_else(|| A::Error::invalid_length(83, &SEQUENCE_EXPECTED))?;
    payload.resolver_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(84, &SEQUENCE_EXPECTED))?;
    payload.crisis_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(85, &SEQUENCE_EXPECTED))?;
    payload.strategy = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(86, &SEQUENCE_EXPECTED))?;
    payload.success = sequence
        .next_element::<Option<bool>>()?
        .ok_or_else(|| A::Error::invalid_length(87, &SEQUENCE_EXPECTED))?;
    Ok(())
}

fn read_seq_group_5<'de, A>(
    sequence: &mut A,
    payload: &mut LlmDecisionPayload,
) -> Result<(), A::Error>
where
    A: SeqAccess<'de>,
    A::Error: de::Error,
{
    payload.operator_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(88, &SEQUENCE_EXPECTED))?;
    payload.target_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(89, &SEQUENCE_EXPECTED))?;
    payload.track = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(90, &SEQUENCE_EXPECTED))?;
    payload.points = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(91, &SEQUENCE_EXPECTED))?;
    payload.achievement_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(92, &SEQUENCE_EXPECTED))?;
    payload.creator_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(93, &SEQUENCE_EXPECTED))?;
    payload.contract_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(94, &SEQUENCE_EXPECTED))?;
    payload.counterparty_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(95, &SEQUENCE_EXPECTED))?;
    payload.settlement_kind = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(96, &SEQUENCE_EXPECTED))?;
    payload.settlement_amount = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(97, &SEQUENCE_EXPECTED))?;
    payload.reputation_stake = sequence
        .next_element::<Option<i64>>()?
        .ok_or_else(|| A::Error::invalid_length(98, &SEQUENCE_EXPECTED))?;
    payload.expires_at = sequence
        .next_element::<Option<u64>>()?
        .ok_or_else(|| A::Error::invalid_length(99, &SEQUENCE_EXPECTED))?;
    payload.accepter_agent_id = sequence
        .next_element::<Option<String>>()?
        .ok_or_else(|| A::Error::invalid_length(100, &SEQUENCE_EXPECTED))?;
    Ok(())
}

struct SequenceExpected;

impl Expected for SequenceExpected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct LlmDecisionPayload with 101 elements")
    }
}

static SEQUENCE_EXPECTED: SequenceExpected = SequenceExpected;

struct LlmDecisionPayloadVisitor;

impl<'de> Visitor<'de> for LlmDecisionPayloadVisitor {
    type Value = LlmDecisionPayload;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct LlmDecisionPayload")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut payload = LlmDecisionPayload::default();
        let mut seen = [false; FIELD_COUNT];

        while let Some(field) = map.next_key::<FieldKey>()? {
            match field {
                FieldKey::Unknown => {
                    let _: IgnoredAny = map.next_value()?;
                }
                FieldKey::Known(index) => {
                    if seen[index] {
                        return Err(A::Error::duplicate_field(FIELD_NAMES[index]));
                    }
                    seen[index] = true;
                    match index {
                        0..=20 => read_map_group_0(index, &mut map, &mut payload)?,
                        21..=43 => read_map_group_1(index, &mut map, &mut payload)?,
                        44..=62 => read_map_group_2(index, &mut map, &mut payload)?,
                        63..=73 => read_map_group_3(index, &mut map, &mut payload)?,
                        74..=87 => read_map_group_4(index, &mut map, &mut payload)?,
                        88..=100 => read_map_group_5(index, &mut map, &mut payload)?,
                        _ => {
                            return Err(A::Error::custom("invalid LlmDecisionPayload field index"));
                        }
                    }
                }
            }
        }

        if !seen[0] {
            return Err(A::Error::missing_field("decision"));
        }

        Ok(payload)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
        A::Error: de::Error,
    {
        let mut payload = LlmDecisionPayload::default();
        read_seq_group_0(&mut sequence, &mut payload)?;
        read_seq_group_1(&mut sequence, &mut payload)?;
        read_seq_group_2(&mut sequence, &mut payload)?;
        read_seq_group_3(&mut sequence, &mut payload)?;
        read_seq_group_4(&mut sequence, &mut payload)?;
        read_seq_group_5(&mut sequence, &mut payload)?;
        Ok(payload)
    }
}

impl<'de> Deserialize<'de> for LlmDecisionPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            "LlmDecisionPayload",
            STRUCT_FIELDS,
            LlmDecisionPayloadVisitor,
        )
    }
}

#[cfg(test)]
#[path = "llm_decision_payload_deserialize_tests.rs"]
mod tests;
