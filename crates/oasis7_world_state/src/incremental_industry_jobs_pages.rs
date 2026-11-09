//! Actual typed-family reachability and provisional physical transfer.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};

/// A completed mark set for these five families, including all six top-level
/// state proof trees and their typed Industry descendants. Other owners' value
/// objects must be unioned by the World owner registry before global GC/transfer.
pub struct IndustryFamilyPagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    qualification: ReservedValue<IndustryFamilyQualificationV1>,
}
impl IndustryFamilyPagesV1 {
    pub fn visit_pages<E>(
        &self,
        mut visitor: impl FnMut(&[u8; 32]) -> Result<(), E>,
    ) -> Result<(), E> {
        for page in self.pages.borrow().iter() {
            visitor(page)?;
        }
        Ok(())
    }
    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }
    pub fn contains(&self, id: &[u8; 32]) -> bool {
        self.pages.borrow().contains(id)
    }
    pub fn qualification(&self) -> &ReservedValue<IndustryFamilyQualificationV1> {
        &self.qualification
    }
}
struct Tracking<'a, W> {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    max: usize,
    sink: Option<&'a mut W>,
    error: Option<ValueTreeError>,
}
impl<W: ValueNodeWriter> Tracking<'_, W> {
    fn note(
        &mut self,
        id: [u8; 32],
        bytes: &[u8],
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if *blake3::hash(bytes).as_bytes() != id {
            return Err(ValueTreeError::Invalid("industry transfer content hash"));
        }
        if self.pages.borrow().contains(&id) {
            return Ok(());
        }
        if self.pages.borrow().len() == self.max {
            return Err(ValueTreeError::ResourceLimited("industry marked pages"));
        }
        if let Some(sink) = self.sink.as_mut() {
            let charge = memory.reserve(bytes.len(), MemoryClass::Ordinary)?;
            sink.put_node(id, charge.retain(bytes.to_vec()))?;
        }
        self.pages.borrow_mut().insert(id);
        Ok(())
    }
}
struct State<'a, 's, M, W> {
    source: &'a M,
    tracking: &'a RefCell<Tracking<'s, W>>,
    memory: &'a MemoryBudget,
}
impl<M: MerkleNodeReader, W: ValueNodeWriter> MerkleNodeReader for State<'_, '_, M, W> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let page = self.source.read_node(digest, max)?;
        if let Some(page) = page.as_ref() {
            if page.as_ref().len() > max {
                self.tracking.borrow_mut().error =
                    Some(ValueTreeError::Invalid("industry state source bound"));
                return Err("industry state source bound".into());
            }
            let hash = blake3::hash(page.as_ref());
            let result = if hash.to_hex().as_str() != digest {
                Err(ValueTreeError::Invalid("industry transfer state digest"))
            } else {
                self.tracking
                    .borrow_mut()
                    .note(*hash.as_bytes(), page.as_ref(), self.memory)
            };
            if let Err(error) = result {
                self.tracking.borrow_mut().error = Some(error);
                return Err("industry state page tracking".into());
            }
        }
        Ok(page)
    }
}
struct Values<'a, 's, R, W> {
    source: &'a R,
    tracking: &'a RefCell<Tracking<'s, W>>,
}
impl<R: ValueNodeReader, W: ValueNodeWriter> ValueNodeReader for Values<'_, '_, R, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self.source.read_node(id, max, memory, class)?;
        if page.len() > max || !page.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("industry value source bound/pool"));
        }
        self.tracking.borrow_mut().note(*id, &page, memory)?;
        Ok(page)
    }
}
struct NoSink;
impl ValueNodeWriter for NoSink {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        unreachable!("mark does not write")
    }
}

fn collect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
>(
    roots: &RootSetV1,
    context: &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut IndustryWorkBudgetV1,
    max_pages: usize,
    sink: Option<&mut W>,
) -> Result<IndustryFamilyPagesV1, ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    // BTreeSet worst-case per-entry overhead is admitted up front. The caller's
    // explicit finite page cap limits both retention and deduplication work.
    let bytes = max_pages
        .checked_mul(128)
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited("industry mark allowance"))?;
    let charge = memory.reserve(bytes, MemoryClass::Ordinary)?;
    let tracking = RefCell::new(Tracking {
        pages: charge.retain(RefCell::new(BTreeSet::new())),
        max: max_pages,
        sink,
        error: None,
    });
    let state = State {
        source: context.state,
        tracking: &tracking,
        memory,
    };
    let values = Values {
        source: context.values.source,
        tracking: &tracking,
    };
    let qualification = qualify_industry_family_v1(
        roots,
        &mut IndustryFamilyReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            max_payload_bytes: context.max_payload_bytes,
            outer_scan: &mut *context.outer_scan,
            take_state_error: || {
                tracking
                    .borrow_mut()
                    .error
                    .take()
                    .or_else(&mut context.take_state_error)
            },
            cancelled: &mut context.cancelled,
        },
        work,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(IndustryFamilyPagesV1 {
        pages: tracking.into_inner().pages,
        qualification,
    })
}
/// A successful set can be unioned into the global GC live set. No set or token
/// escapes if any state/typed-child/value page, quota or cancellation fails.
pub fn mark_industry_family_pages_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut IndustryWorkBudgetV1,
    max_pages: usize,
) -> Result<IndustryFamilyPagesV1, ValueTreeError> {
    collect::<M, R, E, C, NoSink>(roots, context, work, max_pages, None)
}
/// Copies each reached physical page to the caller's provisional destination,
/// including nested StateRoots; partial objects never imply a publishable head.
/// Success returns the exact mark set and same-root schema qualification.
pub fn transfer_industry_family_pages_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
>(
    roots: &RootSetV1,
    context: &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut IndustryWorkBudgetV1,
    max_pages: usize,
    sink: &mut W,
) -> Result<IndustryFamilyPagesV1, ValueTreeError> {
    collect(roots, context, work, max_pages, Some(sink))
}
