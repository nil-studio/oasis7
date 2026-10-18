struct PowerOrderMatchCandidate {
    original_index: usize,
    order_id: u64,
    owner: ResourceOwner,
    remaining_amount: i64,
    limit_price_per_pu: i64,
}

impl WorldKernel {
    fn current_power_order_index(
        &self,
        candidate: &PowerOrderMatchCandidate,
        removed_original_indices: &[usize],
    ) -> Option<usize> {
        let removed_before = removed_original_indices
            .partition_point(|removed_index| *removed_index < candidate.original_index);
        let current_index = candidate.original_index.checked_sub(removed_before)?;
        if self
            .model
            .power_order_book
            .open_orders
            .get(current_index)
            .is_some_and(|entry| entry.order_id == candidate.order_id)
        {
            return Some(current_index);
        }
        self.find_power_order_index(candidate.order_id)
    }

    fn record_removed_power_order_original_index(
        removed_original_indices: &mut Vec<usize>,
        original_index: usize,
    ) {
        if let Err(insert_index) = removed_original_indices.binary_search(&original_index) {
            removed_original_indices.insert(insert_index, original_index);
        }
    }

    fn sorted_opposite_power_order_candidates(
        &self,
        incoming_side: PowerOrderSide,
    ) -> Vec<PowerOrderMatchCandidate> {
        let mut entries: Vec<PowerOrderMatchCandidate> = self
            .model
            .power_order_book
            .open_orders
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.side != incoming_side)
            .map(|(original_index, entry)| PowerOrderMatchCandidate {
                original_index,
                order_id: entry.order_id,
                owner: entry.owner.clone(),
                remaining_amount: entry.remaining_amount,
                limit_price_per_pu: entry.limit_price_per_pu,
            })
            .collect();
        entries.sort_by(|lhs, rhs| match incoming_side {
            PowerOrderSide::Buy => lhs
                .limit_price_per_pu
                .cmp(&rhs.limit_price_per_pu)
                .then_with(|| lhs.order_id.cmp(&rhs.order_id)),
            PowerOrderSide::Sell => rhs
                .limit_price_per_pu
                .cmp(&lhs.limit_price_per_pu)
                .then_with(|| lhs.order_id.cmp(&rhs.order_id)),
        });
        entries
    }

    fn power_orderbook_inconsistent_missing(order_id: u64) -> WorldEventKind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "power orderbook inconsistent: order {} missing during fill",
                    order_id
                )],
            },
        }
    }
}
