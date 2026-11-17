//! 时序模型

mod field;
mod key;
mod meta;
mod query;
mod row;
pub(crate) mod rules;

pub use field::*;
pub use key::*;
pub use meta::*;
pub use query::*;
pub use row::*;

/// 直接使用 1 版本发送请求
pub const SUPPORTED_TABLE_VERSION: i64 = 1;

#[cfg(test)]
mod test_timeseries_model {
    use crate::test_util::setup;

    use super::{encode_flatbuf_rows, TimeseriesRow};

    #[test]
    fn test_flat_buffer_rows() {
        setup();
        let rows = vec![
            TimeseriesRow::new()
                .measurement_name("m-11")
                .datasource("ds-11")
                .tag("region", "region-11")
                .field_double("f11", 123.456),
            TimeseriesRow::new()
                .measurement_name("m-12")
                .datasource("ds-12")
                .tag("region", "region-12")
                .field_double("f12", 234.567),
        ];

        let _ = encode_flatbuf_rows(&rows);
    }
}
