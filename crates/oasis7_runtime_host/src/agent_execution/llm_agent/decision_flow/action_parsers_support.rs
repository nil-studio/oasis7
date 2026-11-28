fn parse_accept_economic_contract(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let accepter_agent_id = parse_agent_identity_or_self(
        parsed.accepter_agent_id.as_deref(),
        "accept_economic_contract",
        "accepter_agent_id",
        agent_id,
    )?;
    let contract_id = parse_required_text(
        parsed.contract_id.as_deref(),
        "accept_economic_contract",
        "contract_id",
    )?;
    Ok(Action::AcceptEconomicContract {
        accepter_agent_id,
        contract_id,
    })
}

fn parse_settle_economic_contract(
    parsed: &LlmDecisionPayload,
    agent_id: &str,
) -> Result<Action, String> {
    let operator_agent_id = parse_agent_identity_or_self(
        parsed.operator_agent_id.as_deref(),
        "settle_economic_contract",
        "operator_agent_id",
        agent_id,
    )?;
    let contract_id = parse_required_text(
        parsed.contract_id.as_deref(),
        "settle_economic_contract",
        "contract_id",
    )?;
    let notes = parse_required_text(parsed.notes.as_deref(), "settle_economic_contract", "notes")?;
    let success = parsed.success.unwrap_or(true);

    Ok(Action::SettleEconomicContract {
        operator_agent_id,
        contract_id,
        success,
        notes,
    })
}

fn parse_required_owner(
    raw: Option<&str>,
    decision: &str,
    field_name: &str,
    agent_id: &str,
) -> Result<ResourceOwner, String> {
    let raw = raw.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    parse_owner_spec(raw, agent_id)
}

fn parse_owner_or_self(raw: Option<&str>, agent_id: &str) -> Result<ResourceOwner, String> {
    match raw {
        Some(owner) => parse_owner_spec(owner, agent_id),
        None => Ok(ResourceOwner::Agent {
            agent_id: agent_id.to_string(),
        }),
    }
}

fn parse_agent_identity_or_self(
    raw: Option<&str>,
    decision: &str,
    field_name: &str,
    agent_id: &str,
) -> Result<String, String> {
    let owner = match raw {
        Some(value) => parse_owner_spec(value, agent_id)?,
        None => ResourceOwner::Agent {
            agent_id: agent_id.to_string(),
        },
    };
    match owner {
        ResourceOwner::Agent { agent_id } => Ok(agent_id),
        ResourceOwner::Location { .. } => Err(format!(
            "{decision} `{field_name}` must be self or agent:<id>"
        )),
    }
}

fn parse_required_text(
    raw: Option<&str>,
    decision: &str,
    field_name: &str,
) -> Result<String, String> {
    let raw = raw.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(format!("{decision} `{field_name}` cannot be empty"));
    }
    Ok(normalized.to_string())
}

fn parse_positive_i64(value: Option<i64>, decision: &str, field_name: &str) -> Result<i64, String> {
    let value = value.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    if value <= 0 {
        return Err(format!("{decision} requires positive {field_name}"));
    }
    Ok(value)
}

fn parse_non_negative_i64_with_default(
    value: Option<i64>,
    decision: &str,
    field_name: &str,
    default: i64,
) -> Result<i64, String> {
    let value = value.unwrap_or(default);
    if value < 0 {
        return Err(format!("{decision} requires non-negative {field_name}"));
    }
    Ok(value)
}

fn parse_hex_bytes(raw: Option<&str>, decision: &str, field_name: &str) -> Result<Vec<u8>, String> {
    let raw = parse_required_text(raw, decision, field_name)?;
    let normalized = raw.strip_prefix("0x").unwrap_or(raw.as_str());
    let bytes = hex::decode(normalized)
        .map_err(|_| format!("{decision} `{field_name}` must be valid hex"))?;
    if bytes.is_empty() {
        return Err(format!(
            "{decision} `{field_name}` cannot decode to empty bytes"
        ));
    }
    Ok(bytes)
}

fn parse_source_files(
    raw: Option<&BTreeMap<String, String>>,
    decision: &str,
    field_name: &str,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let Some(raw) = raw else {
        return Err(format!("{decision} missing `{field_name}`"));
    };
    if raw.is_empty() {
        return Err(format!("{decision} `{field_name}` cannot be empty"));
    }
    let mut files = BTreeMap::new();
    for (path, content) in raw {
        let normalized_path = path.trim();
        if normalized_path.is_empty() {
            return Err(format!("{decision} `{field_name}` contains empty path"));
        }
        if content.is_empty() {
            return Err(format!(
                "{decision} `{field_name}` contains empty content for path {}",
                normalized_path
            ));
        }
        files.insert(normalized_path.to_string(), content.as_bytes().to_vec());
    }
    Ok(files)
}

fn parse_positive_u64(value: Option<u64>, decision: &str, field_name: &str) -> Result<u64, String> {
    let value = value.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    if value == 0 {
        return Err(format!("{decision} requires positive {field_name}"));
    }
    Ok(value)
}

fn parse_positive_u64_list(
    value: Option<&[u64]>,
    decision: &str,
    field_name: &str,
) -> Result<Vec<u64>, String> {
    let value = value.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    if value.is_empty() {
        return Err(format!("{decision} `{field_name}` cannot be empty"));
    }
    if value.contains(&0) {
        return Err(format!("{decision} `{field_name}` must be positive"));
    }
    Ok(value.to_vec())
}

fn parse_optional_positive_u64(
    value: Option<u64>,
    decision: &str,
    field_name: &str,
) -> Result<Option<u64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value == 0 {
        return Err(format!("{decision} `{field_name}` must be >= 1"));
    }
    Ok(Some(value))
}

fn parse_required_text_list(
    value: Option<&[String]>,
    decision: &str,
    field_name: &str,
) -> Result<Vec<String>, String> {
    let value = value.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    if value.is_empty() {
        return Err(format!("{decision} `{field_name}` cannot be empty"));
    }
    let mut out = Vec::with_capacity(value.len());
    for item in value {
        let normalized = item.trim();
        if normalized.is_empty() {
            return Err(format!("{decision} `{field_name}` contains empty item"));
        }
        out.push(normalized.to_string());
    }
    Ok(out)
}

fn parse_required_agent_identity_list(
    value: Option<&[String]>,
    decision: &str,
    field_name: &str,
    agent_id: &str,
) -> Result<Vec<String>, String> {
    let value = value.ok_or_else(|| format!("{decision} missing `{field_name}`"))?;
    if value.is_empty() {
        return Err(format!("{decision} `{field_name}` cannot be empty"));
    }
    let mut out = Vec::with_capacity(value.len());
    for item in value {
        let member =
            parse_agent_identity_or_self(Some(item.as_str()), decision, field_name, agent_id)?;
        out.push(member);
    }
    Ok(out)
}

fn parse_optional_non_empty_text(
    raw: Option<&str>,
    decision: &str,
    field_name: &str,
) -> Result<Option<String>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(format!("{decision} `{field_name}` cannot be empty"));
    }
    Ok(Some(normalized.to_string()))
}

fn parse_social_stake(
    payload: Option<&LlmSocialStakePayload>,
    decision: &str,
) -> Result<Option<SocialStake>, String> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    let raw_kind = payload
        .kind
        .as_deref()
        .ok_or_else(|| format!("{decision} stake missing `kind`"))?;
    let kind = parse_resource_kind(raw_kind)
        .ok_or_else(|| format!("{decision} stake invalid kind: {raw_kind}"))?;
    let amount = payload
        .amount
        .ok_or_else(|| format!("{decision} stake missing `amount`"))?;
    if amount <= 0 {
        return Err(format!("{decision} stake requires positive amount"));
    }
    Ok(Some(SocialStake { kind, amount }))
}

fn parse_social_adjudication_decision(value: &str) -> Option<SocialAdjudicationDecision> {
    match value.trim().to_ascii_lowercase().as_str() {
        "confirm" => Some(SocialAdjudicationDecision::Confirm),
        "retract" => Some(SocialAdjudicationDecision::Retract),
        _ => None,
    }
}

fn parse_power_order_side(value: &str) -> Option<PowerOrderSide> {
    match value.trim().to_ascii_lowercase().as_str() {
        "buy" => Some(PowerOrderSide::Buy),
        "sell" => Some(PowerOrderSide::Sell),
        _ => None,
    }
}
