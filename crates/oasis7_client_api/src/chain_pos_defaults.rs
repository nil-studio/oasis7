use std::sync::OnceLock;

const CHAIN_POS_DEFAULTS_ENV: &str = include_str!("../../../config/chain-pos-defaults.env");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPosTimingDefaults {
    pub slot_duration_ms: u64,
    pub ticks_per_slot: u64,
    pub proposal_tick_phase: u64,
    pub max_past_slot_lag: u64,
}

static DEFAULTS: OnceLock<ChainPosTimingDefaults> = OnceLock::new();

pub fn chain_pos_timing_defaults() -> &'static ChainPosTimingDefaults {
    DEFAULTS.get_or_init(|| {
        parse_chain_pos_timing_defaults(CHAIN_POS_DEFAULTS_ENV)
            .expect("config/chain-pos-defaults.env must define valid PoS timing defaults")
    })
}

pub fn defaults() -> &'static ChainPosTimingDefaults {
    chain_pos_timing_defaults()
}

pub fn chain_pos_slot_duration_ms() -> u64 {
    chain_pos_timing_defaults().slot_duration_ms
}

pub fn chain_pos_ticks_per_slot() -> u64 {
    chain_pos_timing_defaults().ticks_per_slot
}

pub fn chain_pos_proposal_tick_phase() -> u64 {
    chain_pos_timing_defaults().proposal_tick_phase
}

pub fn chain_pos_max_past_slot_lag() -> u64 {
    chain_pos_timing_defaults().max_past_slot_lag
}

#[doc(hidden)]
pub fn parse_chain_pos_timing_defaults(raw: &str) -> Result<ChainPosTimingDefaults, String> {
    let slot_duration_ms = parse_required_u64(raw, "POS_SLOT_DURATION_MS")?;
    let ticks_per_slot = parse_required_u64(raw, "POS_TICKS_PER_SLOT")?;
    let proposal_tick_phase = parse_required_u64(raw, "POS_PROPOSAL_TICK_PHASE")?;
    let max_past_slot_lag = parse_required_u64(raw, "POS_MAX_PAST_SLOT_LAG")?;

    if slot_duration_ms == 0 {
        return Err("POS_SLOT_DURATION_MS must be positive".to_string());
    }
    if ticks_per_slot == 0 {
        return Err("POS_TICKS_PER_SLOT must be positive".to_string());
    }
    if proposal_tick_phase >= ticks_per_slot {
        return Err(format!(
            "POS_PROPOSAL_TICK_PHASE ({proposal_tick_phase}) must be less than POS_TICKS_PER_SLOT ({ticks_per_slot})"
        ));
    }

    Ok(ChainPosTimingDefaults {
        slot_duration_ms,
        ticks_per_slot,
        proposal_tick_phase,
        max_past_slot_lag,
    })
}

fn parse_required_u64(raw: &str, key: &str) -> Result<u64, String> {
    let value = raw
        .lines()
        .filter_map(parse_env_line)
        .find_map(|(line_key, line_value)| (line_key == key).then_some(line_value))
        .ok_or_else(|| format!("{key} is missing"))?;
    value
        .parse::<u64>()
        .map_err(|_| format!("{key} must be a non-negative integer, got `{value}`"))
}

fn parse_env_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_chain_pos_defaults_parse_to_current_profile() {
        let defaults = chain_pos_timing_defaults();
        assert_eq!(defaults.slot_duration_ms, 8_000);
        assert_eq!(defaults.ticks_per_slot, 10);
        assert_eq!(defaults.proposal_tick_phase, 9);
        assert_eq!(defaults.max_past_slot_lag, 256);
    }

    #[test]
    fn repo_chain_pos_defaults_reject_invalid_phase() {
        let err = parse_chain_pos_timing_defaults(
            "\
POS_SLOT_DURATION_MS=8000
POS_TICKS_PER_SLOT=10
POS_PROPOSAL_TICK_PHASE=10
POS_MAX_PAST_SLOT_LAG=256
",
        )
        .expect_err("phase equal to ticks per slot must be rejected");
        assert!(err.contains("must be less than"));
    }
}
