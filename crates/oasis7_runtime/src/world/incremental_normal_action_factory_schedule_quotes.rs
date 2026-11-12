//! Sole original market/scarcity predicates, with closed real read sources.
use super::*;
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait FactoryMarketFactsV1: sealed::Sealed {
    type Error;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_work(
        &mut self,
        consume: &[MaterialStack],
        tags: &[String],
    ) -> Result<(), Self::Error>;
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error>;
    fn transit_loss_bps(&mut self, kind: &str) -> Result<i64, Self::Error>;
    fn governance_tax_bps(&mut self) -> Result<u16, Self::Error>;
}
pub(in crate::world) struct LegacyFactoryMarketFactsV1<'a> {
    pub world: &'a World,
}
impl sealed::Sealed for LegacyFactoryMarketFactsV1<'_> {}
impl FactoryMarketFactsV1 for LegacyFactoryMarketFactsV1<'_> {
    type Error = std::convert::Infallible;
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_work(&mut self, _: &[MaterialStack], _: &[String]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        Ok(self.world.ledger_material_balance(ledger, kind))
    }
    fn transit_loss_bps(&mut self, kind: &str) -> Result<i64, Self::Error> {
        Ok(material_transit_loss_bps_for_kind(self.world, kind))
    }
    fn governance_tax_bps(&mut self) -> Result<u16, Self::Error> {
        Ok(governance_tax_bps_for_material_quotes(self.world))
    }
}
pub(in crate::world) fn scarcity_delay_from_facts_v1<S: FactoryMarketFactsV1>(
    source: &mut S,
    preferred_consume_ledger: &MaterialLedgerId,
    consume_ledger: &MaterialLedgerId,
    consume: &[MaterialStack],
    bottleneck_tags: &[String],
) -> Result<u32, S::Error> {
    if *preferred_consume_ledger == MaterialLedgerId::world()
        || *consume_ledger != MaterialLedgerId::world()
        || bottleneck_tags.is_empty()
    {
        return Ok(0);
    }

    source.reserve_work(consume, bottleneck_tags)?;
    let bottleneck_set: BTreeSet<&str> = bottleneck_tags.iter().map(String::as_str).collect();
    let mut requested_total: i128 = 0;
    let mut deficit_total: i128 = 0;
    for stack in consume {
        if stack.amount <= 0 {
            continue;
        }
        source.charge(stack.kind.len())?;
        let normalized_kind = stack.kind.to_ascii_lowercase();
        if !bottleneck_set.contains(normalized_kind.as_str()) {
            continue;
        }
        let requested = stack.amount as i128;
        let available = source
            .material(preferred_consume_ledger, stack.kind.as_str())?
            .max(0) as i128;
        requested_total = requested_total.saturating_add(requested);
        deficit_total = deficit_total.saturating_add(requested.saturating_sub(available).max(0));
    }

    if requested_total <= 0 || deficit_total <= 0 {
        return Ok(0);
    }
    let deficit_ratio_bps = deficit_total
        .saturating_mul(10_000)
        .saturating_div(requested_total);
    Ok(if deficit_ratio_bps >= 7_000 { 2 } else { 1 })
}
pub(in crate::world) fn market_quotes_from_facts_v1<S: FactoryMarketFactsV1>(
    source: &mut S,
    preferred_consume_ledger: &MaterialLedgerId,
    consume: &[MaterialStack],
) -> Result<Vec<MaterialMarketQuote>, S::Error> {
    source.reserve_work(consume, &[])?;
    let mut requested_by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for stack in consume {
        source.charge(stack.kind.len())?;
        if stack.amount <= 0 {
            continue;
        }
        let entry = requested_by_kind.entry(stack.kind.clone()).or_insert(0);
        *entry = entry.saturating_add(stack.amount);
    }
    if requested_by_kind.is_empty() {
        return Ok(Vec::new());
    }

    let governance_tax_bps = source.governance_tax_bps()?;
    let mut quotes = Vec::with_capacity(requested_by_kind.len());
    for (kind, requested_amount) in requested_by_kind {
        let transit_loss_bps = source.transit_loss_bps(kind.as_str())?;
        let local_available_amount = source
            .material(preferred_consume_ledger, kind.as_str())?
            .max(0);
        let world_available_amount = source
            .material(&MaterialLedgerId::world(), kind.as_str())?
            .max(0);
        let local_deficit_amount = requested_amount
            .saturating_sub(local_available_amount)
            .max(0);
        let deficit_ratio_bps = if requested_amount > 0 {
            ((local_deficit_amount as i128)
                .saturating_mul(10_000)
                .saturating_div(requested_amount as i128)) as i64
        } else {
            0
        };
        let effective_cost_index_ppm = 1_000_000_i64
            .saturating_add(deficit_ratio_bps.saturating_mul(100))
            .saturating_add(transit_loss_bps.saturating_mul(100))
            .saturating_add(i64::from(governance_tax_bps).saturating_mul(100));
        quotes.push(MaterialMarketQuote {
            kind,
            requested_amount,
            local_available_amount,
            world_available_amount,
            local_deficit_amount,
            transit_loss_bps,
            governance_tax_bps,
            effective_cost_index_ppm,
        });
    }
    Ok(quotes)
}
