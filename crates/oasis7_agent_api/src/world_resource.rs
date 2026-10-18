//! Shared resource values and checked stock arithmetic.

use crate::world_action::ResourceKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResourceStock {
    pub amounts: BTreeMap<ResourceKind, i64>,
}

impl ResourceStock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, kind: ResourceKind) -> i64 {
        *self.amounts.get(&kind).unwrap_or(&0)
    }

    pub fn set(&mut self, kind: ResourceKind, amount: i64) -> Result<(), StockError> {
        if amount < 0 {
            return Err(StockError::NegativeAmount { amount });
        }
        if amount == 0 {
            self.amounts.remove(&kind);
        } else {
            self.amounts.insert(kind, amount);
        }
        Ok(())
    }

    pub fn add(&mut self, kind: ResourceKind, amount: i64) -> Result<(), StockError> {
        if amount < 0 {
            return Err(StockError::NegativeAmount { amount });
        }
        let current = self.get(kind);
        let next = current.checked_add(amount).ok_or(StockError::Overflow {
            kind,
            current,
            delta: amount,
        })?;
        self.set(kind, next)
    }

    pub fn remove(&mut self, kind: ResourceKind, amount: i64) -> Result<(), StockError> {
        if amount < 0 {
            return Err(StockError::NegativeAmount { amount });
        }
        let current = self.get(kind);
        if current < amount {
            return Err(StockError::Insufficient {
                kind,
                requested: amount,
                available: current,
            });
        }
        self.set(kind, current - amount)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StockError {
    NegativeAmount {
        amount: i64,
    },
    Overflow {
        kind: ResourceKind,
        current: i64,
        delta: i64,
    },
    Insufficient {
        kind: ResourceKind,
        requested: i64,
        available: i64,
    },
}

const PPM_BASE: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentElementKind {
    Oxygen,
    Silicon,
    Magnesium,
    Aluminum,
    Calcium,
    Iron,
    Nickel,
    Cobalt,
    Titanium,
    Chromium,
    Hydrogen,
    Carbon,
    Nitrogen,
    Sulfur,
    Copper,
    Zinc,
    Lithium,
    Neodymium,
    Uranium,
    Thorium,
}

impl FragmentElementKind {
    /// Stable player/protocol identifier for element-resource summaries.
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Oxygen => "oxygen",
            Self::Silicon => "silicon",
            Self::Magnesium => "magnesium",
            Self::Aluminum => "aluminum",
            Self::Calcium => "calcium",
            Self::Iron => "iron",
            Self::Nickel => "nickel",
            Self::Cobalt => "cobalt",
            Self::Titanium => "titanium",
            Self::Chromium => "chromium",
            Self::Hydrogen => "hydrogen",
            Self::Carbon => "carbon",
            Self::Nitrogen => "nitrogen",
            Self::Sulfur => "sulfur",
            Self::Copper => "copper",
            Self::Zinc => "zinc",
            Self::Lithium => "lithium",
            Self::Neodymium => "neodymium",
            Self::Uranium => "uranium",
            Self::Thorium => "thorium",
        }
    }
}

#[cfg(test)]
mod fragment_element_kind_tests {
    use super::FragmentElementKind;

    #[test]
    fn fragment_element_wire_labels_are_explicit_stable_snake_case() {
        let cases = [
            (FragmentElementKind::Oxygen, "oxygen"),
            (FragmentElementKind::Silicon, "silicon"),
            (FragmentElementKind::Magnesium, "magnesium"),
            (FragmentElementKind::Aluminum, "aluminum"),
            (FragmentElementKind::Calcium, "calcium"),
            (FragmentElementKind::Iron, "iron"),
            (FragmentElementKind::Nickel, "nickel"),
            (FragmentElementKind::Cobalt, "cobalt"),
            (FragmentElementKind::Titanium, "titanium"),
            (FragmentElementKind::Chromium, "chromium"),
            (FragmentElementKind::Hydrogen, "hydrogen"),
            (FragmentElementKind::Carbon, "carbon"),
            (FragmentElementKind::Nitrogen, "nitrogen"),
            (FragmentElementKind::Sulfur, "sulfur"),
            (FragmentElementKind::Copper, "copper"),
            (FragmentElementKind::Zinc, "zinc"),
            (FragmentElementKind::Lithium, "lithium"),
            (FragmentElementKind::Neodymium, "neodymium"),
            (FragmentElementKind::Uranium, "uranium"),
            (FragmentElementKind::Thorium, "thorium"),
        ];

        for (element, expected) in cases {
            assert_eq!(element.wire_label(), expected);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ElementComposition {
    pub ppm: BTreeMap<FragmentElementKind, u32>,
}

impl ElementComposition {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, kind: FragmentElementKind) -> u32 {
        *self.ppm.get(&kind).unwrap_or(&0)
    }

    pub fn set(&mut self, kind: FragmentElementKind, value: u32) {
        if value == 0 {
            self.ppm.remove(&kind);
        } else {
            self.ppm.insert(kind, value);
        }
    }

    pub fn total_ppm(&self) -> u64 {
        self.ppm.values().map(|value| *value as u64).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FragmentResourceBudget {
    pub total_by_element_g: BTreeMap<FragmentElementKind, i64>,
    pub remaining_by_element_g: BTreeMap<FragmentElementKind, i64>,
}

impl FragmentResourceBudget {
    pub fn get_total(&self, kind: FragmentElementKind) -> i64 {
        *self.total_by_element_g.get(&kind).unwrap_or(&0)
    }

    pub fn get_remaining(&self, kind: FragmentElementKind) -> i64 {
        *self.remaining_by_element_g.get(&kind).unwrap_or(&0)
    }

    pub fn from_mass_and_elements(
        mass_g: i64,
        elements: &ElementComposition,
        recoverability_ppm: i64,
    ) -> Self {
        if mass_g <= 0 {
            return Self::default();
        }
        let recoverability_ppm = recoverability_ppm.clamp(0, PPM_BASE);
        let mut out = Self::default();

        for (element, ppm) in &elements.ppm {
            if *ppm == 0 {
                continue;
            }
            let total = mass_g
                .saturating_mul(*ppm as i64)
                .saturating_mul(recoverability_ppm)
                .saturating_div(PPM_BASE)
                .saturating_div(PPM_BASE);
            if total > 0 {
                out.total_by_element_g.insert(*element, total);
                out.remaining_by_element_g.insert(*element, total);
            }
        }

        out
    }

    pub fn consume(
        &mut self,
        kind: FragmentElementKind,
        amount_g: i64,
    ) -> Result<i64, ElementBudgetError> {
        if amount_g <= 0 {
            return Err(ElementBudgetError::InvalidAmount { amount_g });
        }

        let available = self.get_remaining(kind);
        if available < amount_g {
            return Err(ElementBudgetError::Insufficient {
                kind,
                requested_g: amount_g,
                remaining_g: available,
            });
        }

        let next = available - amount_g;
        if next == 0 {
            self.remaining_by_element_g.remove(&kind);
        } else {
            self.remaining_by_element_g.insert(kind, next);
        }
        Ok(amount_g)
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining_by_element_g
            .values()
            .all(|value| *value <= 0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChunkResourceBudget {
    pub total_by_element_g: BTreeMap<FragmentElementKind, i64>,
    pub remaining_by_element_g: BTreeMap<FragmentElementKind, i64>,
}

impl ChunkResourceBudget {
    pub fn get_total(&self, kind: FragmentElementKind) -> i64 {
        *self.total_by_element_g.get(&kind).unwrap_or(&0)
    }

    pub fn get_remaining(&self, kind: FragmentElementKind) -> i64 {
        *self.remaining_by_element_g.get(&kind).unwrap_or(&0)
    }

    pub fn accumulate_fragment(&mut self, fragment: &FragmentResourceBudget) {
        for (element, total) in &fragment.total_by_element_g {
            if *total <= 0 {
                continue;
            }
            let entry = self.total_by_element_g.entry(*element).or_insert(0);
            *entry = entry.saturating_add(*total);
        }
        for (element, remaining) in &fragment.remaining_by_element_g {
            if *remaining <= 0 {
                continue;
            }
            let entry = self.remaining_by_element_g.entry(*element).or_insert(0);
            *entry = entry.saturating_add(*remaining);
        }
    }

    pub fn consume(
        &mut self,
        kind: FragmentElementKind,
        amount_g: i64,
    ) -> Result<i64, ElementBudgetError> {
        if amount_g <= 0 {
            return Err(ElementBudgetError::InvalidAmount { amount_g });
        }

        let available = self.get_remaining(kind);
        if available < amount_g {
            return Err(ElementBudgetError::Insufficient {
                kind,
                requested_g: amount_g,
                remaining_g: available,
            });
        }

        let next = available - amount_g;
        if next == 0 {
            self.remaining_by_element_g.remove(&kind);
        } else {
            self.remaining_by_element_g.insert(kind, next);
        }
        Ok(amount_g)
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining_by_element_g
            .values()
            .all(|value| *value <= 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementBudgetError {
    InvalidAmount {
        amount_g: i64,
    },
    Insufficient {
        kind: FragmentElementKind,
        requested_g: i64,
        remaining_g: i64,
    },
}

mod manifest;
pub use manifest::*;
