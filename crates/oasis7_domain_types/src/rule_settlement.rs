//! Shared L0 selection of rule settlement scope across non-authoritative
//! Simulator projections and authoritative Runtime execution.
//!
//! Rule evaluation already produces signed resource deltas. Selecting one of
//! those original values is not a license to commit it: only the Runtime
//! accepted transition may install the result, once, after its durable gate.

/// The policy's authority scope. Simulator quotes never debit resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSettlementMode {
    AdvisoryOnly,
    AuthoritativeDebit,
}

/// A final move rejection excludes pricing that only applies to real travel.
/// Other rejections and non-movement actions retain their existing rule path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleActionDisposition {
    Accepted,
    RejectedMovement,
}

/// Borrow the actual general-rule cost and combined price from their owner.
/// This generic carrier never re-encodes or copies a domain-specific delta.
pub struct RuleSettlementInput<'a, T> {
    pub general: &'a T,
    pub with_distance: &'a T,
    pub disposition: RuleActionDisposition,
}

pub struct RuleSettlementQuote<'a, T> {
    pub selected: &'a T,
    pub mode: RuleSettlementMode,
}

impl<T> RuleSettlementQuote<'_, T> {
    /// An advisory quote must never be interpreted as permission to debit.
    pub fn is_authoritative(&self) -> bool {
        self.mode == RuleSettlementMode::AuthoritativeDebit
    }
}

/// The same pure decision is used by two real consumers, without a shadow fee
/// calculation or a second mutable resource owner.
pub trait RuleSettlementPolicy {
    const MODE: RuleSettlementMode;

    fn quote<'a, T>(&self, input: RuleSettlementInput<'a, T>) -> RuleSettlementQuote<'a, T> {
        let selected = match input.disposition {
            RuleActionDisposition::Accepted => input.with_distance,
            RuleActionDisposition::RejectedMovement => input.general,
        };
        RuleSettlementQuote {
            selected,
            mode: Self::MODE,
        }
    }
}

pub struct AdvisoryOnly;
impl RuleSettlementPolicy for AdvisoryOnly {
    const MODE: RuleSettlementMode = RuleSettlementMode::AdvisoryOnly;
}

pub struct AuthoritativeDebit;
impl RuleSettlementPolicy for AuthoritativeDebit {
    const MODE: RuleSettlementMode = RuleSettlementMode::AuthoritativeDebit;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn same_original_cost_scope_but_distinct_authority() {
        let general = BTreeMap::from([("electricity", -2_i64)]);
        let priced = BTreeMap::from([("electricity", -9_i64), ("data", -3_i64)]);
        for disposition in [
            RuleActionDisposition::Accepted,
            RuleActionDisposition::RejectedMovement,
        ] {
            let input = || RuleSettlementInput {
                general: &general,
                with_distance: &priced,
                disposition,
            };
            let advisory = AdvisoryOnly.quote(input());
            let authoritative = AuthoritativeDebit.quote(input());
            let selected = if disposition == RuleActionDisposition::Accepted {
                &priced
            } else {
                &general
            };
            assert!(std::ptr::eq(advisory.selected, selected));
            assert!(std::ptr::eq(authoritative.selected, selected));
            assert!(!advisory.is_authoritative());
            assert!(authoritative.is_authoritative());
        }
        assert_eq!(general.get("electricity"), Some(&-2));
        assert_eq!(priced.get("data"), Some(&-3));
    }

    #[test]
    fn denied_move_cannot_inherit_distance_only_price_even_with_extreme_values() {
        let attempt = i64::MIN;
        let priced = i64::MAX;
        let rejected = AuthoritativeDebit.quote(RuleSettlementInput {
            general: &attempt,
            with_distance: &priced,
            disposition: RuleActionDisposition::RejectedMovement,
        });
        assert_eq!(*rejected.selected, i64::MIN);
        let accepted = AdvisoryOnly.quote(RuleSettlementInput {
            general: &attempt,
            with_distance: &priced,
            disposition: RuleActionDisposition::Accepted,
        });
        assert_eq!(*accepted.selected, i64::MAX);
    }
}
