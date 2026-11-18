//! The original main-token transfer reducer over explicitly borrowed DATA.
//!
//! A reader and a returned delta confer no state, execution or publication
//! qualification. The ordinary World and the bounded transaction owner each
//! admit their own source and install the resulting values in their own scope.
use super::{MainTokenAccountBalance, WorldError, invalid};

pub trait MainTokenTransferReadV1 {
    type Error;

    fn transfer_nonce(&mut self, account: &str) -> Result<Option<u64>, Self::Error>;
    fn balance(&mut self, account: &str) -> Result<Option<MainTokenAccountBalance>, Self::Error>;
}

#[derive(Debug)]
pub enum MainTokenTransferErrorV1<E> {
    Read(E),
    Business(WorldError),
}

/// Only the two changed account rows and the sender's new nonce. In particular
/// this DATA does not own or reconstruct the World, supply or material maps.
#[derive(Debug)]
pub struct MainTokenTransferDeltaV1 {
    pub source: MainTokenAccountBalance,
    pub target: MainTokenAccountBalance,
    pub nonce: u64,
}

/// Preserve the original validation and read order: request, nonce, source,
/// then target. A source failure never reads the target or partially installs.
pub fn prepare_main_token_transfer_delta_v1<R: MainTokenTransferReadV1>(
    from: &str,
    to: &str,
    amount: u64,
    nonce: u64,
    read: &mut R,
) -> Result<MainTokenTransferDeltaV1, MainTokenTransferErrorV1<R::Error>> {
    use MainTokenTransferErrorV1::{Business, Read};
    let from = from.trim();
    let to = to.trim();
    if from.is_empty() {
        return Err(Business(invalid(
            "main token transfer from_account_id cannot be empty",
        )));
    }
    if to.is_empty() {
        return Err(Business(invalid(
            "main token transfer to_account_id cannot be empty",
        )));
    }
    if from == to {
        return Err(Business(invalid(format!(
            "main token transfer from_account_id and to_account_id cannot be the same: {from}"
        ))));
    }
    if amount == 0 {
        return Err(Business(invalid("main token transfer amount must be > 0")));
    }
    if nonce == 0 {
        return Err(Business(invalid("main token transfer nonce must be > 0")));
    }
    if let Some(last) = read.transfer_nonce(from).map_err(Read)?
        && nonce <= last
    {
        return Err(Business(invalid(format!(
            "main token transfer nonce replay: from_account_id={from} nonce={nonce} last_nonce={last}"
        ))));
    }
    let mut source = read.balance(from).map_err(Read)?.ok_or_else(|| {
        Business(invalid(format!(
            "main token transfer source account not found: {from}"
        )))
    })?;
    if source.account_id != from {
        return Err(Business(invalid(format!(
            "main token transfer source account key mismatch: key={from} value={}",
            source.account_id
        ))));
    }
    if source.liquid_balance < amount {
        return Err(Business(invalid(format!(
            "main token transfer source balance insufficient: from_account_id={from} balance={} amount={amount}",
            source.liquid_balance
        ))));
    }
    let target_before = read.balance(to).map_err(Read)?;
    if let Some(target) = &target_before
        && target.account_id != to
    {
        return Err(Business(invalid(format!(
            "main token transfer target account key mismatch: key={to} value={}",
            target.account_id
        ))));
    }
    let target_liquid = target_before
        .as_ref()
        .map(|account| account.liquid_balance)
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or_else(|| {
            Business(invalid(format!(
                "main token transfer target balance overflow: to_account_id={to} amount={amount}"
            )))
        })?;
    source.liquid_balance -= amount;
    let mut target = target_before.unwrap_or_else(|| MainTokenAccountBalance {
        account_id: to.to_string(),
        ..MainTokenAccountBalance::default()
    });
    target.liquid_balance = target_liquid;
    Ok(MainTokenTransferDeltaV1 {
        source,
        target,
        nonce,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Read {
        nonce: Option<u64>,
        source: Option<MainTokenAccountBalance>,
        target: Option<MainTokenAccountBalance>,
        fail_source: bool,
        calls: Vec<String>,
    }
    impl MainTokenTransferReadV1 for Read {
        type Error = &'static str;
        fn transfer_nonce(&mut self, account: &str) -> Result<Option<u64>, Self::Error> {
            self.calls.push(format!("nonce:{account}"));
            Ok(self.nonce)
        }
        fn balance(
            &mut self,
            account: &str,
        ) -> Result<Option<MainTokenAccountBalance>, Self::Error> {
            self.calls.push(format!("balance:{account}"));
            if account == "alice" {
                if self.fail_source {
                    return Err("source original unavailable");
                }
                Ok(self.source.clone())
            } else {
                assert_eq!(account, "bob");
                Ok(self.target.clone())
            }
        }
    }
    fn read() -> Read {
        Read {
            nonce: Some(2),
            source: Some(MainTokenAccountBalance {
                account_id: "alice".into(),
                liquid_balance: 8,
                vested_balance: 11,
                restricted_starter_claim_balance: 7,
            }),
            target: Some(MainTokenAccountBalance {
                account_id: "bob".into(),
                liquid_balance: 3,
                vested_balance: 17,
                restricted_starter_claim_balance: 19,
            }),
            fail_source: false,
            calls: vec![],
        }
    }
    #[test]
    fn transfer_reducer_preserves_original_first_error_and_point_read_order() {
        let mut r = read();
        assert!(matches!(
            prepare_main_token_transfer_delta_v1("", "bob", 0, 0, &mut r),
            Err(MainTokenTransferErrorV1::Business(_))
        ));
        assert!(r.calls.is_empty());
        assert!(matches!(
            prepare_main_token_transfer_delta_v1("alice", "bob", 99, 2, &mut r),
            Err(MainTokenTransferErrorV1::Business(_))
        ));
        assert_eq!(r.calls, ["nonce:alice"]);
        r.calls.clear();
        assert!(matches!(
            prepare_main_token_transfer_delta_v1("alice", "bob", 9, 3, &mut r),
            Err(MainTokenTransferErrorV1::Business(_))
        ));
        assert_eq!(r.calls, ["nonce:alice", "balance:alice"]);
        r.calls.clear();
        r.fail_source = true;
        assert!(matches!(
            prepare_main_token_transfer_delta_v1("alice", "bob", 1, 3, &mut r),
            Err(MainTokenTransferErrorV1::Read(
                "source original unavailable"
            ))
        ));
        assert_eq!(r.calls, ["nonce:alice", "balance:alice"]);
    }
    #[test]
    fn transfer_delta_preserves_other_balances_and_overflow_never_partially_applies() {
        let mut r = read();
        r.target.as_mut().unwrap().liquid_balance = u64::MAX;
        let error = prepare_main_token_transfer_delta_v1("alice", "bob", 2, 3, &mut r).unwrap_err();
        assert!(matches!(&error, MainTokenTransferErrorV1::Business(_)));
        assert!(format!("{error:?}").contains("target balance overflow"));
        assert_eq!(r.source.as_ref().unwrap().liquid_balance, 8);
        assert_eq!(r.nonce, Some(2));
        r.target.as_mut().unwrap().liquid_balance = 3;
        r.calls.clear();
        let out = prepare_main_token_transfer_delta_v1(" alice ", " bob ", 2, 3, &mut r).unwrap();
        assert_eq!(r.calls, ["nonce:alice", "balance:alice", "balance:bob"]);
        assert_eq!(
            (
                out.source.liquid_balance,
                out.source.vested_balance,
                out.source.restricted_starter_claim_balance
            ),
            (6, 11, 7)
        );
        assert_eq!(
            (
                out.target.liquid_balance,
                out.target.vested_balance,
                out.target.restricted_starter_claim_balance
            ),
            (5, 17, 19)
        );
        assert_eq!(out.nonce, 3);
        assert_eq!(
            r.source.as_ref().unwrap().liquid_balance,
            8,
            "DATA preparation cannot install state"
        );
    }
}
