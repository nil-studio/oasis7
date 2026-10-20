//! Profile-only substitution contracts. These types grant no execution authority.
use serde::{Deserialize, Serialize};

pub const MAX_SUBSTITUTION_RULES: usize = 64;
pub const MAX_SUBSTITUTION_CHOICES: usize = 32;
const MAX_REF_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionProfileRefV1 {
    pub id: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstitutionRoundingV1 {
    Exact,
    Ceil,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstitutionResidualV1 {
    Reject,
    Retain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubstitutionConversionV1 {
    None,
    Required { converter: SubstitutionProfileRefV1 },
}

/// All causal dimensions are explicit immutable authority references, including
/// explicit zero/no-impact profiles. Equality of strings alone proves no cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionImpactV1 {
    pub outputs: SubstitutionProfileRefV1,
    pub quality: SubstitutionProfileRefV1,
    pub power: SubstitutionProfileRefV1,
    pub time: SubstitutionProfileRefV1,
    pub byproducts: SubstitutionProfileRefV1,
    pub terminal: SubstitutionProfileRefV1,
    pub value_class: SubstitutionProfileRefV1,
    pub logistics: SubstitutionProfileRefV1,
    pub buffer: SubstitutionProfileRefV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionRuleV1 {
    pub rule: SubstitutionProfileRefV1,
    pub recipe: SubstitutionProfileRefV1,
    pub input_edge: String,
    pub required_material: SubstitutionProfileRefV1,
    pub allowed_materials: Vec<SubstitutionProfileRefV1>,
    pub accepted_specs: Vec<String>,
    pub accepted_quality: Vec<String>,
    pub accepted_custody: Vec<String>,
    pub owner_ledger_path_policy: SubstitutionProfileRefV1,
    pub source_unit: String,
    pub target_unit: String,
    /// Source units required per target unit, before explicit source loss.
    pub ratio_numerator: u64,
    pub ratio_denominator: u64,
    pub executable_target_quantum: u64,
    pub rounding: SubstitutionRoundingV1,
    pub residual: SubstitutionResidualV1,
    /// Quantization is over this fixed canonical batch/cycle domain.
    pub aggregation_domain: SubstitutionProfileRefV1,
    /// Additional source loss units per target unit; must resolve exactly.
    pub source_loss_numerator: u64,
    pub source_loss_denominator: u64,
    pub impacts: SubstitutionImpactV1,
    pub conversion: SubstitutionConversionV1,
    pub applicable_input_states: Vec<String>,
    pub disposition_policy: SubstitutionProfileRefV1,
    pub recheck_policy: SubstitutionProfileRefV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubstitutionRuleError {
    InvalidReference,
    RecipeMismatch,
    InvalidRatio,
    InvalidUnit,
    InvalidQuantum,
    NonCanonicalSet,
    CapacityExceeded,
    ArithmeticOverflow,
    QuantizationBlocked,
    DomainMismatch,
}

impl std::fmt::Display for SubstitutionRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "substitution rule rejected: {self:?}")
    }
}
impl std::error::Error for SubstitutionRuleError {}

fn reference(value: &str) -> Result<(), SubstitutionRuleError> {
    if value.is_empty() || value.len() > MAX_REF_BYTES || value.trim() != value {
        return Err(SubstitutionRuleError::InvalidReference);
    }
    Ok(())
}
impl SubstitutionProfileRefV1 {
    pub fn validate(&self) -> Result<(), SubstitutionRuleError> {
        reference(&self.id)?;
        reference(&self.version)?;
        if self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(SubstitutionRuleError::InvalidReference);
        }
        Ok(())
    }
}
fn canonical(values: &[String]) -> Result<(), SubstitutionRuleError> {
    if values.is_empty() || values.len() > MAX_SUBSTITUTION_CHOICES {
        return Err(SubstitutionRuleError::CapacityExceeded);
    }
    for value in values {
        reference(value)?;
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(SubstitutionRuleError::NonCanonicalSet);
    }
    Ok(())
}
impl SubstitutionRuleV1 {
    pub fn validate(&self, expected_recipe_id: &str) -> Result<(), SubstitutionRuleError> {
        for item in [
            &self.rule,
            &self.recipe,
            &self.required_material,
            &self.owner_ledger_path_policy,
            &self.aggregation_domain,
            &self.disposition_policy,
            &self.recheck_policy,
            &self.impacts.outputs,
            &self.impacts.quality,
            &self.impacts.power,
            &self.impacts.time,
            &self.impacts.byproducts,
            &self.impacts.terminal,
            &self.impacts.value_class,
            &self.impacts.logistics,
            &self.impacts.buffer,
        ] {
            item.validate()?;
        }
        if self.recipe.id != expected_recipe_id {
            return Err(SubstitutionRuleError::RecipeMismatch);
        }
        reference(&self.input_edge)?;
        for unit in [&self.source_unit, &self.target_unit] {
            reference(unit).map_err(|_| SubstitutionRuleError::InvalidUnit)?;
        }
        if self.ratio_numerator == 0
            || self.ratio_denominator == 0
            || self.source_loss_denominator == 0
        {
            return Err(SubstitutionRuleError::InvalidRatio);
        }
        if self.executable_target_quantum == 0 {
            return Err(SubstitutionRuleError::InvalidQuantum);
        }
        for values in [
            &self.accepted_specs,
            &self.accepted_quality,
            &self.accepted_custody,
            &self.applicable_input_states,
        ] {
            canonical(values)?;
        }
        if self.allowed_materials.is_empty()
            || self.allowed_materials.len() > MAX_SUBSTITUTION_CHOICES
        {
            return Err(SubstitutionRuleError::CapacityExceeded);
        }
        for material in &self.allowed_materials {
            material.validate()?;
        }
        if self
            .allowed_materials
            .windows(2)
            .any(|p| p[0].id >= p[1].id)
        {
            return Err(SubstitutionRuleError::NonCanonicalSet);
        }
        if let SubstitutionConversionV1::Required { converter } = &self.conversion {
            converter.validate()?;
        }
        Ok(())
    }

    /// Computes the ENTIRE canonical domain requirement, never a split delta.
    /// Callers must persist the domain total/consumption; this pure arithmetic
    /// function does not prevent replay or authorize a debit.
    pub fn quantity_plan(
        &self,
        domain: SubstitutionProfileRefV1,
        domain_key: String,
        total_target: u64,
        source_unit: &str,
        target_unit: &str,
    ) -> Result<SubstitutionQuantityPlanV1, SubstitutionRuleError> {
        self.validate(&self.recipe.id)?;
        reference(&domain_key)?;
        if domain != self.aggregation_domain {
            return Err(SubstitutionRuleError::DomainMismatch);
        }
        if source_unit != self.source_unit || target_unit != self.target_unit {
            return Err(SubstitutionRuleError::InvalidUnit);
        }
        if total_target == 0 || total_target % self.executable_target_quantum != 0 {
            return Err(SubstitutionRuleError::QuantizationBlocked);
        }
        let (source, residual) = checked_source_requirement(
            total_target,
            self.ratio_numerator,
            self.ratio_denominator,
            self.rounding,
        )?;
        if residual != 0 && self.residual == SubstitutionResidualV1::Reject {
            return Err(SubstitutionRuleError::QuantizationBlocked);
        }
        let loss = if self.source_loss_numerator == 0 {
            0
        } else {
            checked_source_requirement(
                total_target,
                self.source_loss_numerator,
                self.source_loss_denominator,
                SubstitutionRoundingV1::Exact,
            )?
            .0
        };
        let source_required = source
            .checked_add(loss)
            .ok_or(SubstitutionRuleError::ArithmeticOverflow)?;
        Ok(SubstitutionQuantityPlanV1 {
            rule: self.rule.clone(),
            domain,
            domain_key,
            source_unit: self.source_unit.clone(),
            target_unit: self.target_unit.clone(),
            source_required,
            source_loss: loss,
            target_executable: total_target,
            residual_numerator: residual,
            residual_denominator: self.ratio_denominator,
            unmet_target: 0,
        })
    }
}

/// Residual is the exact excess source numerator over the ratio denominator.
fn checked_source_requirement(
    target: u64,
    numerator: u64,
    denominator: u64,
    rounding: SubstitutionRoundingV1,
) -> Result<(u64, u64), SubstitutionRuleError> {
    if numerator == 0 || denominator == 0 {
        return Err(SubstitutionRuleError::InvalidRatio);
    }
    let raw = target
        .checked_mul(numerator)
        .ok_or(SubstitutionRuleError::ArithmeticOverflow)?;
    let quotient = raw / denominator;
    let remainder = raw % denominator;
    if remainder == 0 {
        return Ok((quotient, 0));
    }
    if rounding == SubstitutionRoundingV1::Exact {
        return Err(SubstitutionRuleError::QuantizationBlocked);
    }
    Ok((
        quotient
            .checked_add(1)
            .ok_or(SubstitutionRuleError::ArithmeticOverflow)?,
        denominator - remainder,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionQuantityPlanV1 {
    pub rule: SubstitutionProfileRefV1,
    pub domain: SubstitutionProfileRefV1,
    pub domain_key: String,
    pub source_unit: String,
    pub target_unit: String,
    pub source_required: u64,
    pub source_loss: u64,
    pub target_executable: u64,
    pub residual_numerator: u64,
    pub residual_denominator: u64,
    pub unmet_target: u64,
}

pub fn validate_substitution_rules(
    rules: &[SubstitutionRuleV1],
    recipe: &str,
) -> Result<(), SubstitutionRuleError> {
    if rules.len() > MAX_SUBSTITUTION_RULES {
        return Err(SubstitutionRuleError::CapacityExceeded);
    }
    for rule in rules {
        rule.validate(recipe)?;
    }
    if rules.windows(2).any(|p| p[0].rule.id >= p[1].rule.id) {
        return Err(SubstitutionRuleError::NonCanonicalSet);
    }
    Ok(())
}
#[cfg(test)]
mod tests;
