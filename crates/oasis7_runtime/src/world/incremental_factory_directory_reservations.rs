//! Bounded typed projections for pending recipe output-capacity checks.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{
        IndustryCellReadV1, IndustryCellV1, IndustryCollectionV1, IndustryColumnV1,
        IndustryFamilyReadContextV1, IndustryRecordKeyV1, IndustryRecordKindV1,
        IndustryStackColumnV1, IndustryTransitionReadContextV1, IndustryValueV1,
        IndustryWorkBudgetV1, read_industry_cell_overlay_v1, read_industry_cell_v1,
    },
};

struct CountWriter {
    bytes: usize,
}
impl std::io::Write for CountWriter {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(input.len())
            .ok_or_else(|| std::io::Error::other("recipe reservation payload overflow"))?;
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct CappedVecWriter<'a> {
    bytes: &'a mut Vec<u8>,
    max: usize,
}
impl std::io::Write for CappedVecWriter<'_> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(input.len())
            .filter(|next| *next <= self.max)
            .ok_or_else(|| std::io::Error::other("recipe reservation buffer bound"))?;
        self.bytes.extend_from_slice(input);
        debug_assert_eq!(self.bytes.len(), next);
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn reservation_invalid() -> ValueTreeError {
    ValueTreeError::Invalid("pending recipe output reservation")
}
fn reservation_limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("pending recipe output reservation")
}
fn read_reservation_value<F>(
    read: &mut F,
    cell: IndustryCellV1<'static>,
    output: &mut dyn std::io::Write,
) -> Result<IndustryValueV1, Error>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    match read(cell, output)? {
        IndustryCellReadV1::Value(value) => Ok(value),
        IndustryCellReadV1::RecordAbsent | IndustryCellReadV1::MemberAbsent => {
            Err(reservation_invalid().into())
        }
    }
}
fn reservation_cell_work(
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
) -> Result<(), Error> {
    directory.pending_entries_remaining = directory
        .pending_entries_remaining
        .checked_sub(1)
        .ok_or_else(reservation_limited)?;
    industry.cells_remaining = industry
        .cells_remaining
        .checked_sub(1)
        .ok_or_else(reservation_limited)?;
    Ok(())
}
fn recipe_output_count<F>(
    collection: IndustryCollectionV1,
    read: &mut F,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
) -> Result<usize, Error>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    reservation_cell_work(directory, industry)?;
    let value = read_reservation_value(
        read,
        IndustryCellV1::Collection(collection),
        &mut std::io::sink(),
    )?;
    match value {
        IndustryValueV1::Unsigned(count) => {
            spend(
                &mut directory.comparisons_remaining,
                std::mem::size_of::<u64>(),
            )?;
            usize::try_from(count).map_err(|_| reservation_limited().into())
        }
        _ => Err(reservation_invalid().into()),
    }
}
fn recipe_kind_len<F>(
    collection: IndustryCollectionV1,
    index: usize,
    max_payload_bytes: usize,
    read: &mut F,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
) -> Result<usize, Error>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    reservation_cell_work(directory, industry)?;
    let mut count = CountWriter { bytes: 0 };
    let value = read_reservation_value(
        read,
        IndustryCellV1::StackItem(
            collection,
            u64::try_from(index).map_err(|_| reservation_limited())?,
            IndustryStackColumnV1::Kind,
        ),
        &mut count,
    )?;
    let IndustryValueV1::Text { utf8_bytes } = value else {
        return Err(reservation_invalid().into());
    };
    if usize::try_from(utf8_bytes).ok() != Some(count.bytes) || count.bytes > max_payload_bytes {
        return Err(reservation_limited().into());
    }
    Ok(count.bytes)
}
fn recipe_stack_amount<F>(
    collection: IndustryCollectionV1,
    index: usize,
    read: &mut F,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
) -> Result<i64, Error>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    reservation_cell_work(directory, industry)?;
    match read_reservation_value(
        read,
        IndustryCellV1::StackItem(
            collection,
            u64::try_from(index).map_err(|_| reservation_limited())?,
            IndustryStackColumnV1::Amount,
        ),
        &mut std::io::sink(),
    )? {
        IndustryValueV1::Signed(amount) => {
            spend(
                &mut directory.comparisons_remaining,
                std::mem::size_of::<i64>(),
            )?;
            Ok(amount)
        }
        _ => Err(reservation_invalid().into()),
    }
}
fn recipe_ledger_info<F>(
    max_payload_bytes: usize,
    read: &mut F,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
) -> Result<
    (
        oasis7_world_state::incremental_ledger_value::LedgerValueInfoV1,
        usize,
    ),
    Error,
>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    reservation_cell_work(directory, industry)?;
    let mut count = CountWriter { bytes: 0 };
    let value = read_reservation_value(
        read,
        IndustryCellV1::Column(IndustryColumnV1::RecordOutputLedger),
        &mut count,
    )?;
    let IndustryValueV1::Ledger(info) = value else {
        return Err(reservation_invalid().into());
    };
    let expected = usize::try_from(info.id_bytes)
        .ok()
        .and_then(|id| {
            id.checked_add(oasis7_world_state::incremental_ledger_value::LEDGER_VALUE_HEADER_BYTES)
        })
        .ok_or_else(reservation_limited)?;
    if count.bytes != expected || count.bytes > max_payload_bytes {
        return Err(reservation_invalid().into());
    }
    spend(&mut directory.comparisons_remaining, count.bytes)?;
    Ok((info, count.bytes))
}
fn ledger_id_bytes(value: &crate::MaterialLedgerId) -> usize {
    match value {
        crate::MaterialLedgerId::World => 0,
        crate::MaterialLedgerId::Agent(id)
        | crate::MaterialLedgerId::Site(id)
        | crate::MaterialLedgerId::Factory(id) => id.len(),
    }
}
fn output_stack_text_bytes(
    produce_lengths: &[usize],
    byproduct_lengths: &[usize],
) -> Result<usize, Error> {
    produce_lengths
        .iter()
        .chain(byproduct_lengths)
        .try_fold(0usize, |sum, length| sum.checked_add(*length))
        .ok_or_else(|| reservation_limited().into())
}
fn read_output_stack_list<F>(
    collection: IndustryCollectionV1,
    lengths: &[usize],
    read: &mut F,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
) -> Result<Vec<crate::MaterialStack>, Error>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    let mut stacks = Vec::with_capacity(lengths.len());
    if stacks.capacity() != lengths.len() {
        return Err(reservation_limited().into());
    }
    for (index, expected_kind_bytes) in lengths.iter().copied().enumerate() {
        if directory.pending_entries_remaining == 0 || industry.cells_remaining == 0 {
            return Err(reservation_limited().into());
        }
        spend(&mut directory.comparisons_remaining, expected_kind_bytes)?;
        let mut kind_bytes = Vec::with_capacity(expected_kind_bytes);
        if kind_bytes.capacity() != expected_kind_bytes {
            return Err(reservation_limited().into());
        }
        reservation_cell_work(directory, industry)?;
        let value = {
            let mut output = CappedVecWriter {
                bytes: &mut kind_bytes,
                max: expected_kind_bytes,
            };
            read_reservation_value(
                read,
                IndustryCellV1::StackItem(
                    collection,
                    u64::try_from(index).map_err(|_| reservation_limited())?,
                    IndustryStackColumnV1::Kind,
                ),
                &mut output,
            )?
        };
        let IndustryValueV1::Text { utf8_bytes } = value else {
            return Err(reservation_invalid().into());
        };
        if usize::try_from(utf8_bytes).ok() != Some(expected_kind_bytes)
            || kind_bytes.len() != expected_kind_bytes
            || kind_bytes.capacity() != expected_kind_bytes
        {
            return Err(reservation_invalid().into());
        }
        let kind = String::from_utf8(kind_bytes).map_err(|_| reservation_invalid())?;
        if kind.capacity() > expected_kind_bytes {
            return Err(reservation_limited().into());
        }
        let amount = recipe_stack_amount(collection, index, read, directory, industry)?;
        stacks.push(crate::MaterialStack { kind, amount });
    }
    Ok(stacks)
}

fn output_reservation_layout(
    stack_count: usize,
    stack_text_bytes: usize,
    ledger_id_bytes: usize,
    ledger_frame_bytes: usize,
) -> Result<usize, Error> {
    stack_count
        .checked_mul(std::mem::size_of::<crate::MaterialStack>())
        .and_then(|n| n.checked_add(stack_text_bytes))
        .and_then(|n| n.checked_add(ledger_id_bytes))
        .and_then(|n| n.checked_add(ledger_frame_bytes))
        .and_then(|n| n.checked_add(std::mem::size_of::<Vec<u8>>()))
        .ok_or_else(|| reservation_limited().into())
}

fn read_output_reservation_v1<F>(
    job_id: u64,
    memory: &MemoryBudget,
    max_payload_bytes: u64,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
    mut read: F,
) -> Result<RecipeOutputReservationV1, Error>
where
    F: FnMut(IndustryCellV1<'static>, &mut dyn std::io::Write) -> Result<IndustryCellReadV1, Error>,
{
    let max_payload_bytes =
        usize::try_from(max_payload_bytes).map_err(|_| reservation_limited())?;
    reservation_cell_work(directory, industry)?;
    let actual_job_id = match read_reservation_value(
        &mut read,
        IndustryCellV1::Column(IndustryColumnV1::RecordJobId),
        &mut std::io::sink(),
    )? {
        IndustryValueV1::Unsigned(id) => id,
        _ => return Err(reservation_invalid().into()),
    };
    if actual_job_id != job_id {
        return Err(reservation_invalid().into());
    }
    spend(
        &mut directory.comparisons_remaining,
        std::mem::size_of::<u64>(),
    )?;
    let (ledger_info, ledger_frame_bytes) =
        recipe_ledger_info(max_payload_bytes, &mut read, directory, industry)?;
    let produce_count = recipe_output_count(
        IndustryCollectionV1::Produce,
        &mut read,
        directory,
        industry,
    )?;
    let byproduct_count = recipe_output_count(
        IndustryCollectionV1::Byproducts,
        &mut read,
        directory,
        industry,
    )?;
    let total_stack_count = produce_count
        .checked_add(byproduct_count)
        .ok_or_else(reservation_limited)?;
    let length_slots = total_stack_count
        .checked_mul(std::mem::size_of::<usize>())
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<Vec<usize>>()))
        .ok_or_else(reservation_limited)?;
    let _length_workspace = memory
        .reserve(length_slots, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut produce_lengths = Vec::with_capacity(produce_count);
    let mut byproduct_lengths = Vec::with_capacity(byproduct_count);
    if produce_lengths.capacity() != produce_count
        || byproduct_lengths.capacity() != byproduct_count
    {
        return Err(reservation_limited().into());
    }
    for index in 0..produce_count {
        let length = recipe_kind_len(
            IndustryCollectionV1::Produce,
            index,
            max_payload_bytes,
            &mut read,
            directory,
            industry,
        )?;
        spend(&mut directory.comparisons_remaining, length)?;
        let _ = recipe_stack_amount(
            IndustryCollectionV1::Produce,
            index,
            &mut read,
            directory,
            industry,
        )?;
        produce_lengths.push(length);
    }
    for index in 0..byproduct_count {
        let length = recipe_kind_len(
            IndustryCollectionV1::Byproducts,
            index,
            max_payload_bytes,
            &mut read,
            directory,
            industry,
        )?;
        spend(&mut directory.comparisons_remaining, length)?;
        let _ = recipe_stack_amount(
            IndustryCollectionV1::Byproducts,
            index,
            &mut read,
            directory,
            industry,
        )?;
        byproduct_lengths.push(length);
    }
    let stack_text_bytes = output_stack_text_bytes(&produce_lengths, &byproduct_lengths)?;
    spend(
        &mut directory.comparisons_remaining,
        total_stack_count
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or_else(reservation_limited)?,
    )?;
    let ledger_id_len = usize::try_from(ledger_info.id_bytes).map_err(|_| reservation_limited())?;
    let allocation = memory
        .reserve(
            output_reservation_layout(
                total_stack_count,
                stack_text_bytes,
                ledger_id_len,
                ledger_frame_bytes,
            )?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let produce = read_output_stack_list(
        IndustryCollectionV1::Produce,
        &produce_lengths,
        &mut read,
        directory,
        industry,
    )?;
    let byproducts = read_output_stack_list(
        IndustryCollectionV1::Byproducts,
        &byproduct_lengths,
        &mut read,
        directory,
        industry,
    )?;
    reservation_cell_work(directory, industry)?;
    let mut ledger_frame = Vec::with_capacity(ledger_frame_bytes);
    if ledger_frame.capacity() != ledger_frame_bytes {
        return Err(reservation_limited().into());
    }
    let ledger_value = {
        let mut output = CappedVecWriter {
            bytes: &mut ledger_frame,
            max: ledger_frame_bytes,
        };
        read_reservation_value(
            &mut read,
            IndustryCellV1::Column(IndustryColumnV1::RecordOutputLedger),
            &mut output,
        )?
    };
    let IndustryValueV1::Ledger(actual_ledger_info) = ledger_value else {
        return Err(reservation_invalid().into());
    };
    if actual_ledger_info != ledger_info || ledger_frame.len() != ledger_frame_bytes {
        return Err(reservation_invalid().into());
    }
    spend(&mut directory.comparisons_remaining, ledger_frame_bytes)?;
    let output_ledger =
        match oasis7_world_state::incremental_ledger_value::read_ledger_value_v1(&ledger_frame)
            .map_err(ValueTreeError::Invalid)?
        {
            oasis7_world_state::incremental_ledger_value::LedgerValueRefV1::World => {
                crate::MaterialLedgerId::World
            }
            oasis7_world_state::incremental_ledger_value::LedgerValueRefV1::Agent(id) => {
                crate::MaterialLedgerId::Agent(id.to_owned())
            }
            oasis7_world_state::incremental_ledger_value::LedgerValueRefV1::Site(id) => {
                crate::MaterialLedgerId::Site(id.to_owned())
            }
            oasis7_world_state::incremental_ledger_value::LedgerValueRefV1::Factory(id) => {
                crate::MaterialLedgerId::Factory(id.to_owned())
            }
        };
    let actual_id_bytes = ledger_id_bytes(&output_ledger);
    if actual_id_bytes != ledger_id_len {
        return Err(reservation_invalid().into());
    }
    if let crate::MaterialLedgerId::Agent(id)
    | crate::MaterialLedgerId::Site(id)
    | crate::MaterialLedgerId::Factory(id) = &output_ledger
        && id.capacity() > id.len()
    {
        return Err(reservation_limited().into());
    }
    Ok(RecipeOutputReservationV1 {
        job_id,
        output_ledger,
        produce,
        byproducts,
        _allocation: allocation,
    })
}

pub(in crate::world) fn clone_output_reservation_v1(
    source: &RecipeOutputReservationV1,
    memory: &MemoryBudget,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<RecipeOutputReservationV1, Error> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let stack_count = source
        .produce
        .len()
        .checked_add(source.byproducts.len())
        .ok_or_else(reservation_limited)?;
    let mut text_bytes = 0usize;
    for stack in source.produce.iter().chain(&source.byproducts) {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        spend(
            &mut directory.comparisons_remaining,
            std::mem::size_of::<usize>(),
        )?;
        text_bytes = text_bytes
            .checked_add(stack.kind.len())
            .ok_or_else(reservation_limited)?;
    }
    let id_bytes = ledger_id_bytes(&source.output_ledger);
    let bytes = stack_count
        .checked_mul(std::mem::size_of::<crate::MaterialStack>())
        .and_then(|n| n.checked_add(text_bytes))
        .and_then(|n| n.checked_add(id_bytes))
        .ok_or_else(reservation_limited)?;
    let allocation = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let structure_bytes = stack_count
        .checked_mul(std::mem::size_of::<crate::MaterialStack>())
        .and_then(|n| n.checked_add(id_bytes))
        .ok_or_else(reservation_limited)?;
    spend(&mut directory.comparisons_remaining, structure_bytes)?;
    let output_ledger = source.output_ledger.clone();
    if let crate::MaterialLedgerId::Agent(id)
    | crate::MaterialLedgerId::Site(id)
    | crate::MaterialLedgerId::Factory(id) = &output_ledger
        && id.capacity() > id.len()
    {
        return Err(reservation_limited().into());
    }
    let produce = clone_output_stacks_v1(&source.produce, directory, cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let byproducts = clone_output_stacks_v1(&source.byproducts, directory, cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(RecipeOutputReservationV1 {
        job_id: source.job_id,
        output_ledger,
        produce,
        byproducts,
        _allocation: allocation,
    })
}

fn clone_output_stacks_v1(
    input: &[crate::MaterialStack],
    directory: &mut FactoryRecipeDirectoryWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<crate::MaterialStack>, Error> {
    let mut output = Vec::with_capacity(input.len());
    if output.capacity() != input.len() {
        return Err(reservation_limited().into());
    }
    for stack in input {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        spend(&mut directory.comparisons_remaining, stack.kind.len())?;
        let kind = stack.kind.clone();
        if kind.capacity() > stack.kind.len() {
            return Err(reservation_limited().into());
        }
        output.push(crate::MaterialStack {
            kind,
            amount: stack.amount,
        });
    }
    Ok(output)
}

pub(in crate::world) fn read_parent_output_reservation_v1<M, S, R, E, C, X>(
    roots: &RootSetV1,
    job_id: u64,
    tx: &mut ExecutionTransaction<'_, M>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    recipe: &mut IndustryFamilyReadContextV1<'_, S, R, E, C>,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
    other_cancelled: &mut X,
) -> Result<RecipeOutputReservationV1, Error>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    X: FnMut() -> bool,
{
    let memory = recipe.values.memory;
    let max_payload_bytes = recipe.max_payload_bytes;
    let key = IndustryRecordKeyV1 {
        kind: IndustryRecordKindV1::Recipe,
        id: job_id,
    };
    let recipe_cancelled = &mut recipe.cancelled;
    let take_state_error = &mut recipe.take_state_error;
    let mut cancelled = || (recipe_cancelled)() || other_cancelled();
    let mut field = WorldFieldReadContextV1 {
        root: roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler),
        state: recipe.state,
        values: ValueReadContext {
            source: recipe.values.source,
            memory,
            class: recipe.values.class,
            io: &mut *recipe.values.io,
        },
        key_work: &mut *recipe.key_work,
        semantic_bytes: recipe.semantic_bytes,
        record_bytes: recipe.record_bytes,
        take_state_error,
        cancelled: &mut cancelled,
    };
    read_output_reservation_v1(
        job_id,
        memory,
        max_payload_bytes,
        directory,
        industry,
        |cell, mut output| {
            tx.validate_guard(guard, live).map_err(Error::from)?;
            let result =
                read_industry_cell_v1(key, cell, &mut field, max_payload_bytes, &mut output)
                    .map_err(Error::from)?;
            tx.validate_guard(guard, live).map_err(Error::from)?;
            Ok(result)
        },
    )
}

pub(in crate::world) fn read_current_output_reservation_v1<M, R, C, X>(
    tx: &mut ExecutionTransaction<'_, M>,
    job_id: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    recipe: &mut IndustryTransitionReadContextV1<'_, R, C>,
    directory: &mut FactoryRecipeDirectoryWorkV1,
    industry: &mut IndustryWorkBudgetV1,
    other_cancelled: &mut X,
) -> Result<RecipeOutputReservationV1, Error>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
    X: FnMut() -> bool,
{
    let memory = recipe.values.memory;
    let max_payload_bytes = recipe.max_payload_bytes;
    let key = IndustryRecordKeyV1 {
        kind: IndustryRecordKindV1::Recipe,
        id: job_id,
    };
    let recipe_cancelled = &mut recipe.cancelled;
    let mut cancelled = || (recipe_cancelled)() || other_cancelled();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: recipe.values.source,
            memory,
            class: recipe.values.class,
            io: &mut *recipe.values.io,
        },
        key_work: &mut *recipe.key_work,
        semantic_bytes: recipe.semantic_bytes,
        record_bytes: recipe.record_bytes,
        take_state_error: || None,
        cancelled: &mut cancelled,
    };
    read_output_reservation_v1(
        job_id,
        memory,
        max_payload_bytes,
        directory,
        industry,
        |cell, mut output| {
            overlay
                .transaction
                .validate_guard(guard, live)
                .map_err(Error::from)?;
            let result = read_industry_cell_overlay_v1(
                key,
                cell,
                &mut overlay,
                max_payload_bytes,
                &mut output,
            )
            .map_err(Error::from)?;
            overlay
                .transaction
                .validate_guard(guard, live)
                .map_err(Error::from)?;
            Ok(result)
        },
    )
}
