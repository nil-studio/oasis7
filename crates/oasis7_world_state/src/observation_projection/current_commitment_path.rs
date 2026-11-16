//! Strict paid path-copy updates for original readonly JSON DATA. Missing
//! intermediate containers are errors; only a real source container patch may
//! install a new container first. No path/value grants authorization or state.
use super::*;
#[derive(Clone, Copy)]
pub enum ProjectionDataPathRefV1<'a> {
    Field(&'a str),
    Index(usize),
}
impl<E> Build<'_, '_, E> {
    fn owned_text_key(&mut self, key: &str) -> Result<Key, ProjectionColumnErrorV1<E>> {
        let n = std::mem::size_of::<KeyText>()
            .checked_add(key.len())
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>()))
            .ok_or(ProjectionColumnErrorV1::Invalid(
                "projection path key layout",
            ))?;
        let fee = self.reserve(n)?;
        let text = key.to_owned();
        if text.capacity() != key.len() {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection path key capacity",
            ));
        }
        Ok(Key::Text(Arc::new(KeyText { text, _fee: fee })))
    }
    fn find_text_node(
        &mut self,
        tree: &T,
        key: &str,
    ) -> Result<Option<J>, ProjectionColumnErrorV1<E>> {
        let mut next = tree.as_ref();
        while let Some(node) = next {
            match self.compare_text(key, &node.key)? {
                Ordering::Less => next = node.left.as_ref(),
                Ordering::Greater => next = node.right.as_ref(),
                Ordering::Equal => return Ok(Some(Arc::clone(&node.value))),
            }
        }
        Ok(None)
    }
    fn find_index_node(
        &mut self,
        tree: &T,
        key: usize,
    ) -> Result<Option<J>, ProjectionColumnErrorV1<E>> {
        let mut next = tree.as_ref();
        while let Some(node) = next {
            self.pay(1, 0)?;
            let Key::Index(index) = &node.key else {
                return Err(ProjectionColumnErrorV1::Invalid(
                    "projection path array key",
                ));
            };
            match key.cmp(index) {
                Ordering::Less => next = node.left.as_ref(),
                Ordering::Greater => next = node.right.as_ref(),
                Ordering::Equal => return Ok(Some(Arc::clone(&node.value))),
            }
        }
        Ok(None)
    }
    fn remove_index(&mut self, tree: &T, key: usize) -> Result<T, ProjectionColumnErrorV1<E>> {
        self.pay(1, 0)?;
        let Some(node) = tree else { return Ok(None) };
        let Key::Index(index) = &node.key else {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection path array key",
            ));
        };
        match key.cmp(index) {
            Ordering::Less => {
                let left = self.remove_index(&node.left, key)?;
                self.balance(
                    node.key.shared(),
                    Arc::clone(&node.value),
                    left,
                    node.right.clone(),
                )
                .map(Some)
            }
            Ordering::Greater => {
                let right = self.remove_index(&node.right, key)?;
                self.balance(
                    node.key.shared(),
                    Arc::clone(&node.value),
                    node.left.clone(),
                    right,
                )
                .map(Some)
            }
            Ordering::Equal => match (&node.left, &node.right) {
                (None, _) => Ok(node.right.clone()),
                (_, None) => Ok(node.left.clone()),
                (Some(_), Some(right)) => {
                    let mut successor = right;
                    while let Some(left) = &successor.left {
                        self.pay(1, 0)?;
                        successor = left
                    }
                    let Key::Index(next) = &successor.key else {
                        return Err(ProjectionColumnErrorV1::Invalid(
                            "projection array successor key",
                        ));
                    };
                    let right = self.remove_index(&node.right, *next)?;
                    self.balance(
                        successor.key.shared(),
                        Arc::clone(&successor.value),
                        node.left.clone(),
                        right,
                    )
                    .map(Some)
                }
            },
        }
    }
    fn update_data_path(
        &mut self,
        node: &J,
        path: &[ProjectionDataPathRefV1<'_>],
        after: Option<&J>,
    ) -> Result<Option<J>, ProjectionColumnErrorV1<E>> {
        self.pay(1, 0)?;
        let Some((segment, rest)) = path.split_first() else {
            return Ok(after.map(Arc::clone));
        };
        match (&node.kind, segment) {
            (Kind::Object(items), ProjectionDataPathRefV1::Field(key)) => {
                self.pay(1, key.len())?;
                let next = if rest.is_empty() {
                    after.map(Arc::clone)
                } else {
                    let child = self.find_text_node(items, key)?.ok_or(
                        ProjectionColumnErrorV1::Invalid("projection path parent member missing"),
                    )?;
                    self.update_data_path(&child, rest, after)?
                };
                let items = if let Some(next) = next {
                    let key = self.owned_text_key(key)?;
                    self.put(items, key, next)?
                } else {
                    self.remove_text(items, key)?
                };
                let fee = self
                    .reserve(std::mem::size_of::<JsonNode>() + 2 * std::mem::size_of::<usize>())?;
                Ok(Some(Arc::new(JsonNode {
                    kind: Kind::Object(items),
                    _fee: fee,
                })))
            }
            (Kind::Array { start, len, items }, ProjectionDataPathRefV1::Index(index)) => {
                if *index > *len || (*index == *len && (!rest.is_empty() || after.is_none())) {
                    return Err(ProjectionColumnErrorV1::Invalid(
                        "projection array path index",
                    ));
                }
                let ordinal = start
                    .checked_add(*index)
                    .ok_or(ProjectionColumnErrorV1::Invalid(
                        "projection array path ordinal",
                    ))?;
                let next = if rest.is_empty() {
                    after.map(Arc::clone)
                } else {
                    let child = self.find_index_node(items, ordinal)?.ok_or(
                        ProjectionColumnErrorV1::Invalid("projection array path item missing"),
                    )?;
                    self.update_data_path(&child, rest, after)?
                };
                let (items, len) = if let Some(next) = next {
                    let length = if *index == *len {
                        len.checked_add(1).ok_or(ProjectionColumnErrorV1::Invalid(
                            "projection array path length",
                        ))?
                    } else {
                        *len
                    };
                    start
                        .checked_add(length)
                        .ok_or(ProjectionColumnErrorV1::Invalid(
                            "projection array path end",
                        ))?;
                    (self.put(items, Key::Index(ordinal), next)?, length)
                } else {
                    // Removing an interior Vec element requires actual suffix changes. Never
                    // relabel old immutable ordinals and pretend they were source changes.
                    if index.checked_add(1) != Some(*len) {
                        return Err(ProjectionColumnErrorV1::Invalid(
                            "projection array removal requires actual suffix updates",
                        ));
                    }
                    (
                        self.remove_index(items, ordinal)?,
                        len.checked_sub(1).ok_or(ProjectionColumnErrorV1::Invalid(
                            "projection array path removal",
                        ))?,
                    )
                };
                let fee = self
                    .reserve(std::mem::size_of::<JsonNode>() + 2 * std::mem::size_of::<usize>())?;
                Ok(Some(Arc::new(JsonNode {
                    kind: Kind::Array {
                        start: *start,
                        len,
                        items,
                    },
                    _fee: fee,
                })))
            }
            _ => Err(ProjectionColumnErrorV1::Invalid(
                "projection path original container kind",
            )),
        }
    }
}
impl ProjectionColumnDataV1 {
    /// `Index` refers to an original JSON array's current relative ordinal, while
    /// the retained AVL keeps its stable internal offset through prefix pruning.
    pub fn with_path_data_v1<E>(
        &self,
        path: &[ProjectionDataPathRefV1<'_>],
        after: Option<&Self>,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Self, ProjectionColumnErrorV1<E>> {
        if !self.uses_budget(memory) || after.is_some_and(|v| !v.uses_budget(memory)) {
            return Err(ProjectionColumnErrorV1::Invalid("projection path pool"));
        }
        let mut build = Build { memory, work };
        let fee = build.reserve(std::mem::size_of::<Self>())?;
        let root = build
            .update_data_path(&self.root, path, after.map(|v| &v.root))?
            .ok_or(ProjectionColumnErrorV1::Invalid(
                "projection root cannot be absent",
            ))?;
        Ok(Self { root, _fee: fee })
    }
}

impl ProjectionColumnDataV1 {
    /// Paid strict lookup of an original path. Array indices are relative to
    /// the retained suffix; ordered sets expose their original serialized rank.
    pub fn select_path_data_v1<E>(
        &self,
        path: &[ProjectionDataPathRefV1<'_>],
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Option<Self>, ProjectionColumnErrorV1<E>> {
        if !self.uses_budget(memory) {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection selected path pool",
            ));
        }
        let mut build = Build { memory, work };
        let fee = build.reserve(std::mem::size_of::<Self>())?;
        let mut current = Arc::clone(&self.root);
        for step in path {
            build.pay(1, 0)?;
            let selected = match (&current.kind, step) {
                (Kind::Object(items), ProjectionDataPathRefV1::Field(key)) => {
                    build.find_text_node(items, key)?
                }
                (Kind::Array { start, len, items }, ProjectionDataPathRefV1::Index(index)) => {
                    if index >= len {
                        return Ok(None);
                    }
                    let index =
                        start
                            .checked_add(*index)
                            .ok_or(ProjectionColumnErrorV1::Invalid(
                                "projection selected array ordinal",
                            ))?;
                    build.find_index_node(items, index)?
                }
                (Kind::TextSet(items), ProjectionDataPathRefV1::Index(index)) => {
                    let mut next = items.as_ref();
                    let mut rank = *index;
                    let mut value = None;
                    while let Some(node) = next {
                        build.pay(1, 0)?;
                        let left = size(&node.left);
                        match rank.cmp(&left) {
                            Ordering::Less => next = node.left.as_ref(),
                            Ordering::Equal => {
                                value = Some(Arc::clone(&node.value));
                                break;
                            }
                            Ordering::Greater => {
                                rank -= left + 1;
                                next = node.right.as_ref();
                            }
                        }
                    }
                    value
                }
                _ => {
                    return Err(ProjectionColumnErrorV1::Invalid(
                        "projection selected path container",
                    ));
                }
            };
            let Some(selected) = selected else {
                return Ok(None);
            };
            current = selected;
        }
        Ok(Some(Self {
            root: current,
            _fee: fee,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_proto::memory_budget::MemoryBudget;
    fn budget() -> MemoryBudget {
        MemoryBudget::new(32 << 20, 0).unwrap()
    }
    fn work(_: u64, _: u64) -> Result<(), &'static str> {
        Ok(())
    }
    #[test]
    fn nested_updates_preserve_parent_and_pruned_array_ordinals() {
        let memory = budget();
        let parent = ProjectionColumnDataV1::from_json_data(
            serde_json::json!({"runtime":{"record":{"generation":1}},"events":[0,1,2,3]}),
            &memory,
            &mut work,
        )
        .unwrap();
        let generation =
            ProjectionColumnDataV1::from_json_data(serde_json::json!(2), &memory, &mut work)
                .unwrap();
        let after = parent
            .with_path_data_v1(
                &[
                    ProjectionDataPathRefV1::Field("runtime"),
                    ProjectionDataPathRefV1::Field("record"),
                    ProjectionDataPathRefV1::Field("generation"),
                ],
                Some(&generation),
                &memory,
                &mut work,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(&parent).unwrap()["runtime"]["record"]["generation"],
            1
        );
        assert_eq!(
            serde_json::to_value(&after).unwrap()["runtime"]["record"]["generation"],
            2
        );
        let mut events = after
            .object_member_data("events", &memory, &mut work)
            .unwrap()
            .unwrap()
            .share_data(&memory, &mut work)
            .unwrap();
        events = events
            .without_array_prefix_data_v1(2, &memory, &mut work)
            .unwrap();
        events = events
            .with_path_data_v1(
                &[ProjectionDataPathRefV1::Index(1)],
                Some(&generation),
                &memory,
                &mut work,
            )
            .unwrap();
        events = events
            .with_path_data_v1(
                &[ProjectionDataPathRefV1::Index(2)],
                Some(&generation),
                &memory,
                &mut work,
            )
            .unwrap();
        events = events
            .with_path_data_v1(
                &[ProjectionDataPathRefV1::Index(2)],
                None,
                &memory,
                &mut work,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(&events).unwrap(),
            serde_json::json!([2, 2])
        );
        drop(events);
        drop(after);
        drop(parent);
        drop(generation);
        assert_eq!(memory.used(), 0);
    }
    #[test]
    fn missing_containers_and_cancelled_paths_do_not_mutate_or_leak() {
        let memory = budget();
        let parent = ProjectionColumnDataV1::from_json_data(
            serde_json::json!({"runtime":{"a":1}}),
            &memory,
            &mut work,
        )
        .unwrap();
        let value =
            ProjectionColumnDataV1::from_json_data(serde_json::json!(2), &memory, &mut work)
                .unwrap();
        let used = memory.used();
        assert!(matches!(
            parent.with_path_data_v1(
                &[
                    ProjectionDataPathRefV1::Field("absent"),
                    ProjectionDataPathRefV1::Field("a")
                ],
                Some(&value),
                &memory,
                &mut work
            ),
            Err(ProjectionColumnErrorV1::Invalid(_))
        ));
        let mut calls = 0;
        assert!(matches!(
            parent.with_path_data_v1(
                &[
                    ProjectionDataPathRefV1::Field("runtime"),
                    ProjectionDataPathRefV1::Field("a")
                ],
                Some(&value),
                &memory,
                &mut |_, _| {
                    calls += 1;
                    if calls == 7 { Err("cancelled") } else { Ok(()) }
                }
            ),
            Err(ProjectionColumnErrorV1::Work("cancelled"))
        ));
        assert_eq!(memory.used(), used);
        assert_eq!(
            serde_json::to_value(&parent).unwrap(),
            serde_json::json!({"runtime":{"a":1}})
        );
    }
}
