use flatbuffers::{FlatBufferBuilder, WIPOffset};

use crate::{
    error::OtsError,
    model::{Column, ColumnValue, PrimaryKey, PrimaryKeyColumn, PrimaryKeyValue},
    protos::fbs::timeseries::{
        BytesValueBuilder, DataType, FieldValuesBuilder, FlatBufferRowGroup, FlatBufferRowGroupBuilder, FlatBufferRowInGroupBuilder, FlatBufferRowsBuilder,
        TagBuilder,
    },
    OtsResult,
};

use super::{parse_tags, rules::validate_timeseries_field_name, TimeseriesKey};

/// 时序表中的数据行
#[derive(Debug, Default, Clone)]
pub struct TimeseriesRow {
    /// 时间线标识
    pub key: TimeseriesKey,

    /// 时间戳（微秒）
    pub timestamp_us: u64,

    /// 列数据
    pub fields: Vec<Column>,
}

impl TimeseriesRow {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置度量名称
    pub fn measurement_name(mut self, measurement: impl Into<String>) -> Self {
        self.key.measurement_name = Some(measurement.into());
        self
    }

    /// 设置源
    pub fn datasource(mut self, source: impl Into<String>) -> Self {
        self.key.datasource = Some(source.into());
        self
    }

    /// 增加一个标签
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.key.tags.insert(key.into(), value.into());
        self
    }

    /// 设置所有标签
    pub fn tags(mut self, tags: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        self.key.tags = tags.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        self
    }

    /// 设置时间戳，微秒为单位
    pub fn timestamp_us(mut self, ts_us: u64) -> Self {
        self.timestamp_us = ts_us;

        self
    }

    pub fn field(mut self, col: Column) -> Self {
        self.fields.push(col);

        self
    }

    pub fn fields(mut self, cols: impl IntoIterator<Item = Column>) -> Self {
        self.fields = cols.into_iter().collect();

        self
    }

    /// 添加/更新字符串类型的列
    pub fn field_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.fields.push(Column::from_string(name, value));

        self
    }

    /// 添加/更新整数列
    pub fn field_integer(mut self, name: &str, value: i64) -> Self {
        self.fields.push(Column::from_integer(name, value));

        self
    }

    /// 添加/更新双精度列
    pub fn field_double(mut self, name: &str, value: f64) -> Self {
        self.fields.push(Column::from_double(name, value));

        self
    }

    /// 添加/更新布尔值列
    pub fn field_bool(mut self, name: &str, value: bool) -> Self {
        self.fields.push(Column::from_bool(name, value));

        self
    }

    /// 添加/更新二进制列
    pub fn field_blob(mut self, name: &str, value: impl Into<Vec<u8>>) -> Self {
        self.fields.push(Column::from_blob(name, value));

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        for f in &self.fields {
            if !validate_timeseries_field_name(&f.name) {
                return Err(OtsError::ValidationFailed(format!("invalid field name: {}", f.name)));
            }

            match &f.value {
                ColumnValue::Integer(_) | ColumnValue::Double(_) | ColumnValue::Boolean(_) | ColumnValue::String(_) | ColumnValue::Blob(_) => {}

                ColumnValue::InfMin | ColumnValue::InfMax | ColumnValue::Null => {
                    return Err(OtsError::ValidationFailed(format!(
                        "invalid field value of field {}: can not be NULL, INF_MIN, INF_MAX",
                        f.name
                    )));
                }
            }
        }

        if self.fields.len() > super::rules::MAX_FIELD_COUNT {
            return Err(OtsError::ValidationFailed(format!(
                "invalid field. field count exceeds max field count: {}",
                super::rules::MAX_FIELD_COUNT
            )));
        }

        Ok(())
    }

    /// 将 TimeseriesRow 编码成 Flat Buffer 的
    /// 虽然返回的是 `FlatBufferRowGroup` 但是实际上这里仅仅包含一行 `TimeseriesRow` 数据
    ///
    pub(crate) fn build_flatbuf_row<'a>(&'a self, fbb: &mut FlatBufferBuilder<'a>) -> OtsResult<WIPOffset<FlatBufferRowGroup<'a>>> {
        let mut field_types = vec![];
        let mut field_names = vec![];

        let mut long_values = vec![];
        let mut double_values = vec![];
        let mut string_values = vec![];
        let mut bool_values = vec![];
        let mut binary_values = vec![];

        for col in &self.fields {
            field_types.push(DataType::from(&col.value));
            field_names.push(fbb.create_string(&col.name));

            match &col.value {
                ColumnValue::Integer(n) => {
                    long_values.push(*n);
                }

                ColumnValue::Double(d) => {
                    double_values.push(*d);
                }

                ColumnValue::Boolean(b) => {
                    bool_values.push(*b);
                }

                ColumnValue::String(s) => {
                    string_values.push(fbb.create_string(s));
                }

                ColumnValue::Blob(items) => {
                    let bytes = fbb.create_vector(&items.iter().map(|b| *b as i8).collect::<Vec<_>>());
                    let mut bv_builder = BytesValueBuilder::new(fbb);
                    bv_builder.add_value(bytes);
                    binary_values.push(bv_builder.finish());
                }

                other => {
                    return Err(OtsError::ValidationFailed(format!("invalid column data type: {:?}", other)));
                }
            }
        }

        let field_names = fbb.create_vector(&field_names);
        let field_types = fbb.create_vector(&field_types);

        let long_values = fbb.create_vector(&long_values);
        let bool_values = fbb.create_vector(&bool_values);
        let string_values = fbb.create_vector(&string_values);
        let double_values = fbb.create_vector(&double_values);
        let binary_values = fbb.create_vector(&binary_values);

        let mut fv_builder = FieldValuesBuilder::new(fbb);
        fv_builder.add_long_values(long_values);
        fv_builder.add_double_values(double_values);
        fv_builder.add_bool_values(bool_values);
        fv_builder.add_string_values(string_values);
        fv_builder.add_binary_values(binary_values);

        let fv = fv_builder.finish();

        let datasource = if let Some(s) = &self.key.datasource {
            fbb.create_string(s)
        } else {
            fbb.create_string("")
        };

        let tag_list = if !self.key.tags.is_empty() {
            let mut items = self.key.tags.iter().collect::<Vec<_>>();
            items.sort_by(|a, b| a.0.cmp(b.0));

            let pairs = items.into_iter().map(|(k, v)| (fbb.create_string(k), fbb.create_string(v))).collect::<Vec<_>>();

            let mut tags = vec![];

            for (k, v) in pairs {
                let mut tag_builder = TagBuilder::new(fbb);
                tag_builder.add_name(k);
                tag_builder.add_value(v);
                tags.push(tag_builder.finish());
            }

            tags
        } else {
            vec![]
        };

        let tag_list = fbb.create_vector(&tag_list);

        // RowInGroup
        let mut rig_builder = FlatBufferRowInGroupBuilder::new(fbb);
        rig_builder.add_data_source(datasource);
        rig_builder.add_field_values(fv);
        rig_builder.add_time(self.timestamp_us as i64);
        rig_builder.add_meta_cache_update_time(60);
        rig_builder.add_tag_list(tag_list);

        let row_in_group = rig_builder.finish();

        let rows = fbb.create_vector(&[row_in_group]);

        let measure_name = if let Some(s) = &self.key.measurement_name {
            fbb.create_string(s)
        } else {
            fbb.create_string("")
        };

        // RowGroup
        let mut rg_builder = FlatBufferRowGroupBuilder::new(fbb);
        rg_builder.add_measurement_name(measure_name);
        rg_builder.add_field_names(field_names);
        rg_builder.add_field_types(field_types);
        rg_builder.add_rows(rows);

        let row_group = rg_builder.finish();

        Ok(row_group)
    }
}

/// 从宽表行转换出来时序行
impl From<crate::model::Row> for TimeseriesRow {
    fn from(value: crate::model::Row) -> Self {
        let crate::model::Row {
            primary_key,
            columns,
            deleted: _,
        } = value;

        let mut key = TimeseriesKey::default();
        let mut timestamp_us = 0;

        for column in primary_key.columns {
            let PrimaryKeyColumn { name, value } = column;

            match name.as_str() {
                "_m_name" => {
                    if let PrimaryKeyValue::String(s) = value {
                        key.measurement_name = Some(s);
                    }
                }

                "_data_source" => {
                    if let PrimaryKeyValue::String(s) = value {
                        key.datasource = Some(s);
                    }
                }

                "_tags" => {
                    if let PrimaryKeyValue::String(s) = value {
                        key.tags = parse_tags(&s);
                    }
                }

                "_time" => {
                    if let PrimaryKeyValue::Integer(ts) = value {
                        timestamp_us = ts;
                    }
                }

                _ => {}
            }
        }

        Self {
            key,
            timestamp_us: timestamp_us as u64,
            fields: columns,
        }
    }
}

impl From<TimeseriesRow> for crate::model::Row {
    fn from(value: TimeseriesRow) -> Self {
        let TimeseriesRow { key, timestamp_us, fields } = value;

        let TimeseriesKey {
            measurement_name,
            datasource,
            tags,
        } = key;

        let mut primary_key = PrimaryKey::new();

        if let Some(s) = measurement_name {
            primary_key = primary_key.column_string("_m_name", s);
        }

        if let Some(s) = datasource {
            primary_key = primary_key.column_string("_data_source", s);
        }

        if !tags.is_empty() {
            let mut items = tags.into_iter().collect::<Vec<_>>();
            items.sort_by(|a, b| a.0.cmp(&b.0));

            primary_key = primary_key.column_string(
                "_tags",
                format!("[{}]", items.into_iter().map(|(k, v)| format!("\"{}={}\"", k, v)).collect::<Vec<_>>().join(",")),
            );
        }

        primary_key = primary_key.column_integer("_time", timestamp_us as i64);

        Self {
            primary_key,
            columns: fields,
            ..Default::default()
        }
    }
}

/// 这里没有处理 MIN 和 MAX 的数据...
impl From<&ColumnValue> for DataType {
    fn from(value: &ColumnValue) -> Self {
        match value {
            ColumnValue::Null => DataType::NONE,
            ColumnValue::Integer(_) => DataType::LONG,
            ColumnValue::Double(_) => DataType::DOUBLE,
            ColumnValue::Boolean(_) => DataType::BOOLEAN,
            ColumnValue::String(_) => DataType::STRING,
            ColumnValue::Blob(_) => DataType::BINARY,
            _ => DataType::NONE,
        }
    }
}

/// 将时序表的行集合以 flat buffer 的格式编码
pub(crate) fn encode_flatbuf_rows(rows: &[TimeseriesRow]) -> OtsResult<Vec<u8>> {
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let mut fbb = FlatBufferBuilder::new();

    // First, collect all row offsets
    let mut fb_row_groups = Vec::with_capacity(rows.len());

    for row in rows {
        let r = row.build_flatbuf_row(&mut fbb)?;
        fb_row_groups.push(r)
    }

    let fb_rows = fbb.create_vector(&fb_row_groups);
    let mut rows_builder = FlatBufferRowsBuilder::new(&mut fbb);
    rows_builder.add_row_groups(fb_rows);

    let fb_rows = rows_builder.finish();

    fbb.finish(fb_rows, None);

    let bytes = fbb.finished_data();

    Ok(bytes.to_vec())
}
