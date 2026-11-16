//! Original sorted string sets represented without mutable array ordinals.
//! Initialization is explicit at GEN/cold; normal changes path-copy the real
//! text-key tree. Serialization remains the original ordered JSON array.
use super::*;

impl<E> Build<'_, '_, E> {
    fn set_key(&mut self, text: &str) -> Result<Key, ProjectionColumnErrorV1<E>> {
        let bytes = std::mem::size_of::<KeyText>()
            .checked_add(text.len())
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
            .ok_or(ProjectionColumnErrorV1::Invalid(
                "projection set key layout",
            ))?;
        let fee = self.reserve(bytes)?;
        self.pay(1, text.len())?;
        let text = text.to_owned();
        if text.capacity() != text.len() {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection set key capacity",
            ));
        }
        Ok(Key::Text(Arc::new(KeyText { text, _fee: fee })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, ops::ControlFlow};
    fn pay(_: u64, _: u64) -> Result<(), &'static str> {
        Ok(())
    }

    #[test]
    fn original_sorted_array_survives_member_changes_without_relabeling() {
        let memory = MemoryBudget::new(32 << 20, 0).unwrap();
        let mut expected: BTreeSet<_> = (0..513).map(|n| format!("member-{n:04}")).collect();
        let original = ProjectionColumnDataV1::from_json_data(
            serde_json::to_value(&expected).unwrap(),
            &memory,
            &mut pay,
        )
        .unwrap();
        let mut current = original
            .as_ordered_text_set_data_v1(&memory, &mut pay)
            .unwrap();
        for n in (0..513).step_by(3) {
            let member = format!("member-{n:04}");
            let mut calls = 0;
            current = current
                .with_text_set_member_data_v1(&member, false, &memory, &mut |_, _| {
                    calls += 1;
                    Ok::<_, &'static str>(())
                })
                .unwrap();
            assert!(calls < 200, "one deletion must not walk the whole set");
            expected.remove(&member);
        }
        for member in ["aardvark", "member-0255", "zebra"] {
            current = current
                .with_text_set_member_data_v1(member, true, &memory, &mut pay)
                .unwrap();
            expected.insert(member.to_owned());
        }
        assert_eq!(current.root_kind(), "array");
        assert_eq!(
            current.array_len_data_v1(&memory, &mut pay).unwrap(),
            expected.len()
        );
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(original.array_len_data_v1(&memory, &mut pay).unwrap(), 513);
        let mut observed = Vec::new();
        current
            .root_data()
            .array_elements_data_v1(&memory, &mut pay, &mut |index, row, _| {
                assert_eq!(index, observed.len());
                observed.push(row.text_data().unwrap().to_owned());
                Ok(ControlFlow::Continue(()))
            })
            .unwrap();
        assert_eq!(observed, expected.into_iter().collect::<Vec<_>>());
        assert!(
            current
                .array_text_contains_data("aardvark", &memory, &mut pay)
                .unwrap()
        );
        assert!(
            !current
                .array_text_contains_data("member-0000", &memory, &mut pay)
                .unwrap()
        );
        drop(current);
        drop(original);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn invalid_original_sets_and_cancelled_updates_preserve_parent_fees() {
        let memory = MemoryBudget::new(32 << 20, 0).unwrap();
        for value in [
            serde_json::json!(["z", "a"]),
            serde_json::json!(["a", "a"]),
            serde_json::json!([1]),
        ] {
            let original =
                ProjectionColumnDataV1::from_json_data(value, &memory, &mut pay).unwrap();
            let used = memory.used();
            assert!(matches!(
                original.as_ordered_text_set_data_v1(&memory, &mut pay),
                Err(ProjectionColumnErrorV1::Invalid(_))
            ));
            assert_eq!(memory.used(), used);
        }
        let original = ProjectionColumnDataV1::from_json_data(
            serde_json::json!(["a", "c"]),
            &memory,
            &mut pay,
        )
        .unwrap();
        let current = original
            .as_ordered_text_set_data_v1(&memory, &mut pay)
            .unwrap();
        let used = memory.used();
        let mut calls = 0;
        assert!(matches!(
            current.with_text_set_member_data_v1("b", true, &memory, &mut |_, _| {
                calls += 1;
                if calls == 8 { Err("cancelled") } else { Ok(()) }
            }),
            Err(ProjectionColumnErrorV1::Work("cancelled"))
        ));
        assert_eq!(memory.used(), used);
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::json!(["a", "c"])
        );
        drop(current);
        drop(original);
        assert_eq!(memory.used(), 0);
    }
}

impl ProjectionColumnDataV1 {
    /// Call once on an actual original set column during GEN/cold construction.
    /// Arbitrary arrays, duplicate strings and non-original ordering are rejected.
    pub fn as_ordered_text_set_data_v1<E>(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Self, ProjectionColumnErrorV1<E>> {
        if !self.uses_budget(memory) {
            return Err(ProjectionColumnErrorV1::Invalid("projection set pool"));
        }
        if matches!(&self.root.kind, Kind::TextSet(_)) {
            return self.share_data(memory, work);
        }
        let Kind::Array { items, .. } = &self.root.kind else {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection set is not array",
            ));
        };
        let mut build = Build { memory, work };
        let fee = build.reserve(std::mem::size_of::<Self>())?;
        let root_fee =
            build.reserve(std::mem::size_of::<JsonNode>() + 2 * std::mem::size_of::<usize>())?;
        let mut after = None;
        let mut previous: Option<J> = None;
        visit_map(items, &mut |_, value| {
            let Kind::Text(text) = &value.kind else {
                return Err(ProjectionColumnErrorV1::Invalid(
                    "projection set member type",
                ));
            };
            let previous_text = previous.as_ref().and_then(|node| match &node.kind {
                Kind::Text(text) => Some(text.as_str()),
                _ => None,
            });
            build.pay(
                1,
                text.len().saturating_add(previous_text.map_or(0, str::len)),
            )?;
            if previous_text.is_some_and(|old| old >= text.as_str()) {
                return Err(ProjectionColumnErrorV1::Invalid(
                    "projection set original order",
                ));
            }
            previous = Some(Arc::clone(value));
            let key = build.set_key(text)?;
            after = build.put(&after, key, Arc::clone(value))?;
            Ok(())
        })?;
        Ok(Self {
            root: Arc::new(JsonNode {
                kind: Kind::TextSet(after),
                _fee: root_fee,
            }),
            _fee: fee,
        })
    }

    /// The selected field's fixed typed decoder supplies this member delta.
    /// No sorting, full-set cloning or ordinal relabeling occurs on normal ticks.
    pub fn with_text_set_member_data_v1<E>(
        &self,
        member: &str,
        present: bool,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Self, ProjectionColumnErrorV1<E>> {
        if !self.uses_budget(memory) {
            return Err(ProjectionColumnErrorV1::Invalid("projection set pool"));
        }
        let Kind::TextSet(items) = &self.root.kind else {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection set requires initialized source",
            ));
        };
        let mut build = Build { memory, work };
        let fee = build.reserve(std::mem::size_of::<Self>())?;
        let root_fee =
            build.reserve(std::mem::size_of::<JsonNode>() + 2 * std::mem::size_of::<usize>())?;
        let after = if present {
            let key = build.set_key(member)?;
            let n = std::mem::size_of::<JsonNode>()
                .checked_add(member.len())
                .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
                .ok_or(ProjectionColumnErrorV1::Invalid(
                    "projection set text layout",
                ))?;
            let value_fee = build.reserve(n)?;
            build.pay(1, member.len())?;
            let text = member.to_owned();
            if text.capacity() != member.len() {
                return Err(ProjectionColumnErrorV1::Invalid(
                    "projection set text capacity",
                ));
            }
            let value = Arc::new(JsonNode {
                kind: Kind::Text(text),
                _fee: value_fee,
            });
            build.put(items, key, value)?
        } else {
            build.remove_text(items, member)?
        };
        Ok(Self {
            root: Arc::new(JsonNode {
                kind: Kind::TextSet(after),
                _fee: root_fee,
            }),
            _fee: fee,
        })
    }
}
