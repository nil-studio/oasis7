//! Readonly original-order rows for fixed formatter fact readers. Traversal
//! borrows the actual paid immutable graph and grants no source qualification.
use super::*;
use std::ops::ControlFlow;
impl<'a> ProjectionColumnRefV1<'a> {
    pub fn object_members_control_data_v1<E>(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
        emit: &mut impl FnMut(
            &'a str,
            ProjectionColumnRefV1<'a>,
            &mut dyn FnMut(u64, u64) -> Result<(), E>,
        ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>>,
    ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>> {
        if !self.node._fee.uses_budget(memory) {
            return Err(ProjectionColumnErrorV1::Invalid("projection member pool"));
        }
        let Kind::Object(items) = &self.node.kind else {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection member is not object",
            ));
        };
        fn walk<'a, E>(
            tree: &'a T,
            work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
            emit: &mut impl FnMut(
                &'a str,
                ProjectionColumnRefV1<'a>,
                &mut dyn FnMut(u64, u64) -> Result<(), E>,
            ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>>,
        ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>> {
            let Some(node) = tree else {
                return Ok(ControlFlow::Continue(()));
            };
            work(1, 0).map_err(ProjectionColumnErrorV1::Work)?;
            if walk(&node.left, work, emit)?.is_break() {
                return Ok(ControlFlow::Break(()));
            }
            let Key::Text(key) = &node.key else {
                return Err(ProjectionColumnErrorV1::Invalid("projection object key"));
            };
            if emit(&key.text, ProjectionColumnRefV1 { node: &node.value }, work)?.is_break() {
                return Ok(ControlFlow::Break(()));
            }
            walk(&node.right, work, emit)
        }
        walk(items, work, emit)
    }
    pub fn array_elements_data_v1<E>(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
        emit: &mut impl FnMut(
            usize,
            ProjectionColumnRefV1<'a>,
            &mut dyn FnMut(u64, u64) -> Result<(), E>,
        ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>>,
    ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>> {
        if !self.node._fee.uses_budget(memory) {
            return Err(ProjectionColumnErrorV1::Invalid("projection member pool"));
        }
        if let Kind::TextSet(items) = &self.node.kind {
            fn walk_set<'a, E>(
                tree: &'a T,
                next: &mut usize,
                work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
                emit: &mut impl FnMut(
                    usize,
                    ProjectionColumnRefV1<'a>,
                    &mut dyn FnMut(u64, u64) -> Result<(), E>,
                )
                    -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>>,
            ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>> {
                let Some(node) = tree else {
                    return Ok(ControlFlow::Continue(()));
                };
                work(1, 0).map_err(ProjectionColumnErrorV1::Work)?;
                if walk_set(&node.left, next, work, emit)?.is_break() {
                    return Ok(ControlFlow::Break(()));
                }
                let Key::Text(_) = &node.key else {
                    return Err(ProjectionColumnErrorV1::Invalid("projection set key"));
                };
                let index = *next;
                *next = next.checked_add(1).ok_or(ProjectionColumnErrorV1::Invalid(
                    "projection set length overflow",
                ))?;
                if emit(index, ProjectionColumnRefV1 { node: &node.value }, work)?.is_break() {
                    return Ok(ControlFlow::Break(()));
                }
                walk_set(&node.right, next, work, emit)
            }
            return walk_set(items, &mut 0, work, emit);
        }
        let Kind::Array { start, len, items } = &self.node.kind else {
            return Err(ProjectionColumnErrorV1::Invalid(
                "projection member is not array",
            ));
        };
        fn walk<'a, E>(
            tree: &'a T,
            start: usize,
            next: &mut usize,
            work: &mut dyn FnMut(u64, u64) -> Result<(), E>,
            emit: &mut impl FnMut(
                usize,
                ProjectionColumnRefV1<'a>,
                &mut dyn FnMut(u64, u64) -> Result<(), E>,
            ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>>,
        ) -> Result<ControlFlow<()>, ProjectionColumnErrorV1<E>> {
            let Some(node) = tree else {
                return Ok(ControlFlow::Continue(()));
            };
            work(1, 0).map_err(ProjectionColumnErrorV1::Work)?;
            if walk(&node.left, start, next, work, emit)?.is_break() {
                return Ok(ControlFlow::Break(()));
            }
            let expected = start
                .checked_add(*next)
                .ok_or(ProjectionColumnErrorV1::Invalid(
                    "projection array ordinal overflow",
                ))?;
            if !matches!(&node.key,Key::Index(value)if *value==expected) {
                return Err(ProjectionColumnErrorV1::Invalid("projection array ordinal"));
            }
            let index = *next;
            *next = next.checked_add(1).ok_or(ProjectionColumnErrorV1::Invalid(
                "projection array length overflow",
            ))?;
            if emit(index, ProjectionColumnRefV1 { node: &node.value }, work)?.is_break() {
                return Ok(ControlFlow::Break(()));
            }
            walk(&node.right, start, next, work, emit)
        }
        let mut next = 0;
        let result = walk(items, *start, &mut next, work, emit)?;
        if result.is_continue() && next != *len {
            return Err(ProjectionColumnErrorV1::Invalid("projection array length"));
        }
        Ok(result)
    }
}
