//! Checked accounting primitives; callers persist changes with admission in one transaction.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawBudgetLedger")]
pub struct BudgetLedger {
    limit: DecimalU64,
    spent: DecimalU64,
    reservations: BTreeMap<String, DecimalU64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBudgetLedger {
    limit: DecimalU64,
    spent: DecimalU64,
    reservations: BTreeMap<String, DecimalU64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetError {
    Overflow,
    Exhausted,
    Conflict,
    Missing,
    Invalid,
}

impl TryFrom<RawBudgetLedger> for BudgetLedger {
    type Error = &'static str;
    fn try_from(raw: RawBudgetLedger) -> Result<Self, Self::Error> {
        let ledger = Self {
            limit: raw.limit,
            spent: raw.spent,
            reservations: raw.reservations,
        };
        if ledger.reservations.keys().any(String::is_empty) || ledger.remaining().is_err() {
            return Err("invalid persisted budget ledger");
        }
        Ok(ledger)
    }
}

impl BudgetLedger {
    pub fn new(limit: u64) -> Self {
        Self {
            limit: DecimalU64(limit),
            spent: DecimalU64(0),
            reservations: BTreeMap::new(),
        }
    }

    pub fn remaining(&self) -> Result<u64, BudgetError> {
        let reserved = self.reservations.values().try_fold(0_u64, |sum, value| {
            sum.checked_add(value.0).ok_or(BudgetError::Overflow)
        })?;
        self.limit
            .0
            .checked_sub(self.spent.0)
            .and_then(|v| v.checked_sub(reserved))
            .ok_or(BudgetError::Exhausted)
    }

    /// Repeated admission of the same logical request does not reserve again.
    pub fn reserve(&mut self, id: &str, amount: u64) -> Result<(), BudgetError> {
        if id.is_empty() {
            return Err(BudgetError::Invalid);
        }
        if let Some(existing) = self.reservations.get(id) {
            return if existing.0 == amount {
                Ok(())
            } else {
                Err(BudgetError::Conflict)
            };
        }
        if self.remaining()? < amount {
            return Err(BudgetError::Exhausted);
        }
        self.reservations.insert(id.to_owned(), DecimalU64(amount));
        Ok(())
    }

    /// Only authoritative settlement or proven non-dispatch permits release.
    /// Unknown transport outcomes must leave the reservation untouched.
    pub fn settle(&mut self, id: &str, consumed: u64) -> Result<(), BudgetError> {
        let reserved = self.reservations.get(id).ok_or(BudgetError::Missing)?.0;
        if consumed > reserved {
            return Err(BudgetError::Exhausted);
        }
        let spent = self
            .spent
            .0
            .checked_add(consumed)
            .ok_or(BudgetError::Overflow)?;
        self.spent = DecimalU64(spent);
        self.reservations.remove(id);
        Ok(())
    }

    /// Transport attempts are charged even when the downstream response is lost.
    pub fn charge(&mut self, amount: u64) -> Result<(), BudgetError> {
        if self.remaining()? < amount {
            return Err(BudgetError::Exhausted);
        }
        self.spent = DecimalU64(
            self.spent
                .0
                .checked_add(amount)
                .ok_or(BudgetError::Overflow)?,
        );
        Ok(())
    }
}

/// Independent cost dimensions; a transport retry never resets logical/business cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationBudgets {
    pub requests: BudgetLedger,
    pub transport: BudgetLedger,
    pub business: BudgetLedger,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservation_and_transport_are_independent_and_checked() {
        let mut ledger = BudgetLedger::new(u64::MAX);
        ledger.reserve("unknown", u64::MAX).unwrap();
        ledger.reserve("unknown", u64::MAX).unwrap();
        assert_eq!(ledger.reserve("other", 1), Err(BudgetError::Exhausted));
        assert_eq!(ledger.reserve("unknown", 0), Err(BudgetError::Conflict));
        assert_eq!(ledger.settle("unknown", u64::MAX), Ok(()));
        assert_eq!(ledger.charge(1), Err(BudgetError::Exhausted));
        assert_eq!(ledger.remaining(), Ok(0));
    }
    #[test]
    fn failed_settlement_preserves_reservation() {
        let mut ledger = BudgetLedger::new(10);
        ledger.reserve("a", 8).unwrap();
        assert_eq!(ledger.settle("a", 9), Err(BudgetError::Exhausted));
        assert_eq!(ledger.remaining(), Ok(2));
        ledger.settle("a", 3).unwrap();
        assert_eq!(ledger.remaining(), Ok(7));
    }

    #[test]
    fn retries_charge_transport_while_unknown_retains_business() {
        let mut budgets = OperationBudgets {
            requests: BudgetLedger::new(1),
            transport: BudgetLedger::new(2),
            business: BudgetLedger::new(10),
        };
        budgets.requests.reserve("request", 1).unwrap();
        budgets.business.reserve("request", 8).unwrap();
        budgets.transport.charge(1).unwrap();
        budgets.transport.charge(1).unwrap();
        assert_eq!(budgets.transport.charge(1), Err(BudgetError::Exhausted));
        assert_eq!(budgets.requests.remaining(), Ok(0));
        assert_eq!(budgets.business.remaining(), Ok(2));
    }
}
