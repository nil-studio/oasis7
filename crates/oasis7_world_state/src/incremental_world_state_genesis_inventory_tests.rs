use super::*;
fn input(memory: &MemoryBudget) -> ReservedValue<StateWriteV1> {
    let charge = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
    let field =
        crate::incremental_inventory_genesis::descriptor("resources", FieldShape::Map).unwrap();
    let key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
    charge.retain(StateWriteV1 {
        key: key.as_bytes().to_vec(),
        before: None,
        after: Some([3; 32]),
    })
}
#[test]
fn inventory_execution_conversion_retains_both_independent_admissions_through_sink() {
    let memory = MemoryBudget::new(4096, 0).unwrap();
    let row = input(&memory);
    let mut calls = 0;
    let mut sink = |output: ReservedValue<ExecutionWriteV1>| {
        calls += 1;
        assert_eq!(memory.used(), 1024 + output.reserved_bytes());
        assert_eq!(output.namespace, StateNamespaceV1::Business);
        assert_eq!(output.mutation.before, None);
        assert_eq!(output.mutation.after, Some([3; 32]));
        drop(output);
        assert_eq!(memory.used(), 1024);
        Ok(())
    };
    Adapter {
        memory: &memory,
        sink: &mut sink,
    }
    .put_write(row)
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(memory.used(), 0);
}
#[test]
fn inventory_conversion_cannot_reuse_or_detach_the_input_reservation() {
    let memory = MemoryBudget::new(1024, 0).unwrap();
    let row = input(&memory);
    let mut calls = 0;
    let mut sink = |_output: ReservedValue<ExecutionWriteV1>| {
        calls += 1;
        Ok(())
    };
    let result = Adapter {
        memory: &memory,
        sink: &mut sink,
    }
    .put_write(row);
    assert!(matches!(result, Err(ValueTreeError::Memory(_))));
    assert_eq!(calls, 0);
    assert_eq!(memory.used(), 0);
}
