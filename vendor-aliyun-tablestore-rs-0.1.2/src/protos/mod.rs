//! protobuf、plain buffer、simple row matrix、flatbuf 相关的

// Mapping to Java SDK: ots_internal_api.proto
include!("./table_store.rs");

include!("./timeseries_fbs.rs");

pub mod filter {
    // Mapping to Java SDK: ots_filter.proto
    include!("./table_store_filter.rs");
}

pub mod search {
    // Mapping to Java SDK: search.proto
    // Line 999 and Line 1000 are updated to add `table_store.` prefix to `ConsumedCapacity`
    include!("./table_store_search.rs");
}

pub mod timeseries {
    include!("./timeseries.rs");
}

pub mod plain_buffer;
pub mod simple_row_matrix;

#[cfg(test)]
mod test_protos {
    use flatbuffers::FlatBufferBuilder;

    use crate::{test_util::setup, util::debug_bytes};

    use super::fbs::timeseries::{DataType, FieldValuesBuilder, FlatBufferRowGroupBuilder, FlatBufferRowInGroupBuilder, FlatBufferRowsBuilder};

    #[test]
    fn test_flat_buffer() {
        setup();

        let mut fbb = FlatBufferBuilder::new();
        let v = fbb.create_vector(&[123.456]);
        let mut fv = FieldValuesBuilder::new(&mut fbb);
        fv.add_double_values(v);
        let field_values = fv.finish();

        let ds = fbb.create_string("data_11");
        let tags = fbb.create_string("[\"tag1=value1\"]");

        // Create a FlatBufferRowInGroup
        let mut row_in_group = FlatBufferRowInGroupBuilder::new(&mut fbb);
        row_in_group.add_data_source(ds);
        row_in_group.add_tags(tags);
        row_in_group.add_time(1744102993984000);
        row_in_group.add_field_values(field_values);
        let row = row_in_group.finish();

        // Create vector of rows
        let rows_vec = fbb.create_vector(&[row]);
        let measure = fbb.create_string("measure_11");
        let field_name = &[fbb.create_string("temp")];
        let field_names = fbb.create_vector(field_name);
        let field_types = fbb.create_vector(&[DataType::DOUBLE]);

        let mut row_group_builder = FlatBufferRowGroupBuilder::new(&mut fbb);
        row_group_builder.add_measurement_name(measure);
        row_group_builder.add_field_names(field_names);
        row_group_builder.add_field_types(field_types);
        row_group_builder.add_rows(rows_vec);
        let fbrg = row_group_builder.finish();

        let row_groups = fbb.create_vector(&[fbrg]);

        // Build the FlatBufferRows
        let mut rows_builder = FlatBufferRowsBuilder::new(&mut fbb);
        rows_builder.add_row_groups(row_groups);
        let rows = rows_builder.finish();

        fbb.finish(rows, None);

        let bytes = fbb.finished_data();
        debug_bytes(bytes);
    }
}
