use super::quote_economy::{native_stacks, stacks};
use super::*;
use oasis7_agent_api::world_quote as native;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceFactoryPreviewV1 {
    pub consume_ledger: ResourceLedgerV1,
    pub consume: Vec<ResourceMaterialStackV1>,
    pub duration_ticks: u32,
    pub site_authority_revision: DecimalU64,
    pub site_location_id: String,
    pub location_anchor_revision: DecimalU64,
    pub construction_power_obligation: ResourceBuildPowerV1,
}
impl From<&native::BuiltinFactoryStartPreview> for ResourceFactoryPreviewV1 {
    fn from(v: &native::BuiltinFactoryStartPreview) -> Self {
        Self {
            consume_ledger: (&v.consume_ledger).into(),
            consume: stacks(&v.consume),
            duration_ticks: v.duration_ticks,
            site_authority_revision: DecimalU64(v.site_authority_revision),
            site_location_id: v.site_location_id.clone(),
            location_anchor_revision: DecimalU64(v.location_anchor_revision),
            construction_power_obligation: (&v.construction_power_obligation).into(),
        }
    }
}
impl From<ResourceFactoryPreviewV1> for native::BuiltinFactoryStartPreview {
    fn from(v: ResourceFactoryPreviewV1) -> Self {
        Self {
            consume_ledger: v.consume_ledger.into(),
            consume: native_stacks(v.consume),
            duration_ticks: v.duration_ticks,
            site_authority_revision: v.site_authority_revision.0,
            site_location_id: v.site_location_id,
            location_anchor_revision: v.location_anchor_revision.0,
            construction_power_obligation: v.construction_power_obligation.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRecipePreviewV1 {
    pub consume_ledger: ResourceLedgerV1,
    pub output_ledger: ResourceLedgerV1,
    pub consume: Vec<ResourceMaterialStackV1>,
    pub produce: Vec<ResourceMaterialStackV1>,
    pub byproducts: Vec<ResourceMaterialStackV1>,
    pub power_required: ResourceDecimalI64V1,
    pub duration_ticks: u32,
}
impl From<&native::BuiltinRecipeStartPreview> for ResourceRecipePreviewV1 {
    fn from(v: &native::BuiltinRecipeStartPreview) -> Self {
        Self {
            consume_ledger: (&v.consume_ledger).into(),
            output_ledger: (&v.output_ledger).into(),
            consume: stacks(&v.consume),
            produce: stacks(&v.produce),
            byproducts: stacks(&v.byproducts),
            power_required: ResourceDecimalI64V1(v.power_required),
            duration_ticks: v.duration_ticks,
        }
    }
}
impl From<ResourceRecipePreviewV1> for native::BuiltinRecipeStartPreview {
    fn from(v: ResourceRecipePreviewV1) -> Self {
        Self {
            consume_ledger: v.consume_ledger.into(),
            output_ledger: v.output_ledger.into(),
            consume: native_stacks(v.consume),
            produce: native_stacks(v.produce),
            byproducts: native_stacks(v.byproducts),
            power_required: v.power_required.0,
            duration_ticks: v.duration_ticks,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBuiltinFactoryQuoteV1 {
    pub action: ResourceQuoteActionV1,
    pub quoted_at_tick: DecimalU64,
    pub authorization: ResourceAuthorizationV1,
    pub electricity_before: ResourceDecimalI64V1,
    #[serde(deserialize_with = "required_option")]
    pub electricity_after: Option<ResourceDecimalI64V1>,
    #[serde(deserialize_with = "required_option")]
    pub rejection: Option<ResourceQuoteRejectionV1>,
    #[serde(deserialize_with = "required_option")]
    pub preview: Option<ResourceFactoryPreviewV1>,
}
impl TryFrom<&native::NativeBuiltinFactoryQuote> for ResourceBuiltinFactoryQuoteV1 {
    type Error = String;
    fn try_from(v: &native::NativeBuiltinFactoryQuote) -> Result<Self, String> {
        Ok(Self {
            action: (&v.action).try_into()?,
            quoted_at_tick: DecimalU64(v.quoted_at_tick),
            authorization: (&v.authorization).into(),
            electricity_before: ResourceDecimalI64V1(v.electricity_before),
            electricity_after: v.electricity_after.map(ResourceDecimalI64V1),
            rejection: v.rejection.as_ref().map(TryInto::try_into).transpose()?,
            preview: v.preview.as_ref().map(Into::into),
        })
    }
}
impl TryFrom<ResourceBuiltinFactoryQuoteV1> for native::NativeBuiltinFactoryQuote {
    type Error = String;
    fn try_from(v: ResourceBuiltinFactoryQuoteV1) -> Result<Self, String> {
        Ok(Self {
            action: v.action.into(),
            quoted_at_tick: v.quoted_at_tick.0,
            authorization: v.authorization.into(),
            electricity_before: v.electricity_before.0,
            electricity_after: v.electricity_after.map(|v| v.0),
            rejection: v.rejection.map(TryInto::try_into).transpose()?,
            preview: v.preview.map(Into::into),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBuiltinRecipeQuoteV1 {
    pub action: ResourceQuoteActionV1,
    pub quoted_at_tick: DecimalU64,
    pub authorization: ResourceAuthorizationV1,
    pub electricity_before: ResourceDecimalI64V1,
    #[serde(deserialize_with = "required_option")]
    pub electricity_after: Option<ResourceDecimalI64V1>,
    #[serde(deserialize_with = "required_option")]
    pub rejection: Option<ResourceQuoteRejectionV1>,
    #[serde(deserialize_with = "required_option")]
    pub preview: Option<ResourceRecipePreviewV1>,
}
impl TryFrom<&native::NativeBuiltinRecipeQuote> for ResourceBuiltinRecipeQuoteV1 {
    type Error = String;
    fn try_from(v: &native::NativeBuiltinRecipeQuote) -> Result<Self, String> {
        Ok(Self {
            action: (&v.action).try_into()?,
            quoted_at_tick: DecimalU64(v.quoted_at_tick),
            authorization: (&v.authorization).into(),
            electricity_before: ResourceDecimalI64V1(v.electricity_before),
            electricity_after: v.electricity_after.map(ResourceDecimalI64V1),
            rejection: v.rejection.as_ref().map(TryInto::try_into).transpose()?,
            preview: v.preview.as_ref().map(Into::into),
        })
    }
}
impl TryFrom<ResourceBuiltinRecipeQuoteV1> for native::NativeBuiltinRecipeQuote {
    type Error = String;
    fn try_from(v: ResourceBuiltinRecipeQuoteV1) -> Result<Self, String> {
        Ok(Self {
            action: v.action.into(),
            quoted_at_tick: v.quoted_at_tick.0,
            authorization: v.authorization.into(),
            electricity_before: v.electricity_before.0,
            electricity_after: v.electricity_after.map(|v| v.0),
            rejection: v.rejection.map(TryInto::try_into).transpose()?,
            preview: v.preview.map(Into::into),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLogisticsTransferQuoteV1 {
    pub requester_agent_id: String,
    pub from_ledger: ResourceLedgerV1,
    pub to_ledger: ResourceLedgerV1,
    pub kind: String,
    pub requested_amount: ResourceDecimalI64V1,
    pub submission_feasible: bool,
    pub max_transferable_amount: ResourceDecimalI64V1,
    pub sent_amount: ResourceDecimalI64V1,
    pub distance_km: ResourceDecimalI64V1,
    pub loss_bps: ResourceDecimalI64V1,
    pub expected_loss_amount: ResourceDecimalI64V1,
    pub expected_received_amount: ResourceDecimalI64V1,
    pub source_amount_before: ResourceDecimalI64V1,
    pub source_amount_after: ResourceDecimalI64V1,
    pub destination_amount_before: ResourceDecimalI64V1,
    pub destination_expected_amount_after: ResourceDecimalI64V1,
    pub ticks_until_arrival: DecimalU64,
    pub ready_at: DecimalU64,
    pub effective_priority: oasis7_agent_api::world_action::MaterialTransitPriority,
    pub priority_reason: String,
    pub inflight_before: DecimalU64,
    pub inflight_capacity: DecimalU64,
    #[serde(deserialize_with = "required_option")]
    pub path_id: Option<String>,
    pub route_ids: Vec<String>,
    pub tariff_electricity_total: ResourceDecimalI64V1,
    pub reroute_count: u32,
    pub recommendation: String,
    pub conditional: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceTransferQuoteV1 {
    pub action: ResourceQuoteActionV1,
    pub authorization: ResourceAuthorizationV1,
    pub quote: ResourceLogisticsTransferQuoteV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "quote",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ResourceBasisQuoteV1 {
    Transfer(Box<ResourceTransferQuoteV1>),
    Build(Box<ResourceBuiltinFactoryQuoteV1>),
    Schedule(Box<ResourceBuiltinRecipeQuoteV1>),
}
