//! Bounded diagnostic observations, separate from durable canonical receipts.
use super::Receipt;
use serde::Serialize;
use std::collections::VecDeque;

const CAPACITY: usize = 64;

#[derive(Serialize)]
pub(super) struct Summary {
    /// Local notifications drained by this CLI invocation, not a World total.
    observed_receipts: u64,
    /// Earlier observations excluded from the bounded diagnostic tail.
    omitted_receipts: u64,
}

#[derive(Default)]
pub(super) struct Window {
    receipts: VecDeque<Receipt>,
    observed: u64,
}

impl Window {
    pub(super) fn push(&mut self, receipt: Receipt) -> Result<(), String> {
        self.observed = self
            .observed
            .checked_add(1)
            .ok_or("header Node receipt observation sequence exhausted")?;
        if self.receipts.len() == CAPACITY {
            self.receipts.pop_front();
        }
        self.receipts.push_back(receipt);
        Ok(())
    }

    pub(super) fn finish(self) -> (Vec<Receipt>, Option<Summary>) {
        let omitted = self.observed - self.receipts.len() as u64;
        let summary = (omitted != 0).then_some(Summary {
            observed_receipts: self.observed,
            omitted_receipts: omitted,
        });
        (self.receipts.into_iter().collect(), summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(height: u64) -> Receipt {
        Receipt {
            height,
            header_id: format!("header-{height}"),
            action_root: format!("actions-{height}"),
            action_count: 1,
        }
    }

    #[test]
    fn receipt_window_survives129_observations_and_reports_omitted_prefix() {
        let mut window = Window::default();
        for height in 1..=129 {
            window.push(receipt(height)).unwrap();
            assert!(window.receipts.len() <= 64);
            assert!(window.receipts.capacity() <= 64);
        }
        let (receipts, summary) = window.finish();
        assert_eq!(
            receipts.iter().map(|r| r.height).collect::<Vec<_>>(),
            (66..=129).collect::<Vec<_>>()
        );
        assert_eq!(receipts.len(), 64);
        let output = serde_json::to_value(super::super::Report {
            schema_version: "oasis7.formal_bft_header_node_receipt.v3",
            receipts,
            receipt_history: summary,
        })
        .unwrap();
        assert_eq!(
            output["receipt_history"],
            serde_json::json!({"observed_receipts": 129, "omitted_receipts": 65})
        );
        assert_eq!(output["receipts"].as_array().unwrap().len(), 64);
        assert_eq!(output["receipts"][0]["height"], 66);
        assert_eq!(output["receipts"][63]["height"], 129);
    }

    #[test]
    fn receipt_window_keeps_original_short_report_shape() {
        let mut window = Window::default();
        window.push(receipt(9)).unwrap();
        let (receipts, receipt_history) = window.finish();
        let report = super::super::Report {
            schema_version: "oasis7.formal_bft_header_node_receipt.v3",
            receipts,
            receipt_history,
        };
        assert_eq!(
            serde_json::to_value(report).unwrap(),
            serde_json::json!({
                "schema_version": "oasis7.formal_bft_header_node_receipt.v3",
                "receipts": [{"height": 9, "header_id": "header-9", "action_root": "actions-9", "action_count": 1}]
            })
        );
    }
}
