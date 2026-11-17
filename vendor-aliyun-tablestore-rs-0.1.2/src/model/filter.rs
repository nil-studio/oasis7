use std::io::Cursor;

use prost::Message;

use crate::protos::filter::{ComparatorType, FilterType, LogicalOperator, ValueTransferRule};

use super::Column;

/// 单条件过滤器
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/singlecolumnvaluefilter>
#[derive(Debug, Clone)]
pub struct SingleColumnValueFilter {
    pub comparator: ComparatorType,
    pub column: Column,

    /// 当某行的该列不存在时，设置条件是否过滤。
    pub filter_if_missing: bool,

    /// 是否只对最新版本有效。默认为 `true`
    pub latest_version_only: bool,

    /// 使用正则表达式匹配到字符串后，将字符串转换为 `String` 、 `Integer` 或者 `Double` 类型。
    /// 当某些列中存储了自定义格式数据（例如JSON格式字符串）时，如果用户希望通过某个子字段值来过滤查询该列数据，则需要设置此参数。
    ///
    /// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/valuetransferrule>
    pub value_transfer_rule: Option<ValueTransferRule>,
}

impl SingleColumnValueFilter {
    pub fn new() -> Self {
        Self {
            comparator: ComparatorType::CtEqual,
            column: Column::null(""),
            filter_if_missing: false,
            latest_version_only: true,
            value_transfer_rule: None,
        }
    }

    /// 等于
    pub fn equal_column(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtEqual;
        self.column = col;

        self
    }

    /// 不等于
    pub fn not_equal(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtNotEqual;
        self.column = col;

        self
    }

    /// 大于
    pub fn greater_than(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtGreaterThan;
        self.column = col;

        self
    }

    /// 大于等于
    pub fn greater_equal(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtGreaterEqual;
        self.column = col;

        self
    }

    /// 小于
    pub fn less_than(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtLessThan;
        self.column = col;

        self
    }

    /// 小于等于
    pub fn less_equal(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtLessEqual;
        self.column = col;

        self
    }

    /// 存在
    pub fn exists(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtExist;
        self.column = col;

        self
    }

    /// 不存在
    pub fn not_exists(mut self, col: Column) -> Self {
        self.comparator = ComparatorType::CtNotExist;
        self.column = col;

        self
    }

    /// 设置当某行的该列不存在时，设置条件是否过滤。
    pub fn filter_if_missing(mut self, value: bool) -> Self {
        self.filter_if_missing = value;

        self
    }

    /// 设置是否只对最新版本有效。默认为 `true`
    pub fn latest_version_only(mut self, value: bool) -> Self {
        self.latest_version_only = value;

        self
    }

    /// 设置转换规则
    pub fn value_transfer_rule(mut self, value: ValueTransferRule) -> Self {
        self.value_transfer_rule = Some(value);

        self
    }

    /// Convert to protobuf bytes
    pub fn into_protobuf_bytes(self) -> Vec<u8> {
        let msg: crate::protos::filter::SingleColumnValueFilter = self.into();
        msg.encode_to_vec()
    }
}

impl Default for SingleColumnValueFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// 将自定义的 SingleColumnFilter 转换成 protobuf 的 SingleColumnFilter
impl From<SingleColumnValueFilter> for crate::protos::filter::SingleColumnValueFilter {
    fn from(value: SingleColumnValueFilter) -> Self {
        let SingleColumnValueFilter {
            comparator,
            column,
            filter_if_missing,
            latest_version_only,
            value_transfer_rule,
        } = value;

        let Column {
            name,
            value,
            op: _,
            timestamp: _,
        } = column;

        // 这里写出的数据不包含 CELL_VALUE 前缀的 4 个字节，
        let mut cursor = Cursor::new(vec![0u8; value.compute_size() as usize]);
        value.write_plain_buffer(&mut cursor);
        let filter_bytes = cursor.into_inner()[4..].into();

        crate::protos::filter::SingleColumnValueFilter {
            comparator: comparator as i32,
            column_name: name,
            column_value: filter_bytes,
            filter_if_missing,
            latest_version_only,
            value_trans_rule: value_transfer_rule,
        }
    }
}

/// 宽行读取过滤条件。
#[derive(Debug, Default, Clone, Copy)]
pub struct ColumnPaginationFilter {
    /// 起始列的位置，表示从第几列开始读。
    pub offset: i32,

    /// 读取的列的个数。
    pub limit: i32,
}

impl ColumnPaginationFilter {
    pub fn new(offset: i32, limit: i32) -> Self {
        Self { offset, limit }
    }

    /// 编码成 Protobuf 字节
    pub fn into_protobuf_bytes(self) -> Vec<u8> {
        let msg: crate::protos::filter::ColumnPaginationFilter = self.into();
        msg.encode_to_vec()
    }
}

impl From<ColumnPaginationFilter> for crate::protos::filter::ColumnPaginationFilter {
    fn from(value: ColumnPaginationFilter) -> Self {
        crate::protos::filter::ColumnPaginationFilter {
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// 多个组合条件，例如 `column_a > 5 AND column_b = 10` 等。适用于条件更新（ConditionUpdate）和过滤器（Filter）功能。
#[derive(Debug, Clone, Default)]
pub struct CompositeColumnValueFilter {
    /// 逻辑操作符
    pub combinator: LogicalOperator,

    /// 子条件表达式。
    pub sub_filters: Vec<Filter>,
}

impl CompositeColumnValueFilter {
    pub fn new(combinator: LogicalOperator) -> Self {
        Self {
            combinator,
            sub_filters: vec![],
        }
    }

    /// 添加一个 Filter
    pub fn sub_filter(mut self, filter: Filter) -> Self {
        self.sub_filters.push(filter);

        self
    }

    /// 设置子过滤器
    pub fn sub_filters(mut self, filters: impl IntoIterator<Item = Filter>) -> Self {
        self.sub_filters = filters.into_iter().collect();

        self
    }

    /// 编码到 protobuf 字节
    pub fn into_protobuf_bytes(self) -> Vec<u8> {
        let msg: crate::protos::filter::CompositeColumnValueFilter = self.into();

        msg.encode_to_vec()
    }
}

impl From<CompositeColumnValueFilter> for crate::protos::filter::CompositeColumnValueFilter {
    fn from(value: CompositeColumnValueFilter) -> Self {
        let CompositeColumnValueFilter { combinator, sub_filters } = value;

        crate::protos::filter::CompositeColumnValueFilter {
            combinator: combinator as i32,
            sub_filters: sub_filters.into_iter().map(|f| f.into()).collect(),
        }
    }
}

/// 过滤器枚举
#[derive(Debug, Clone)]
pub enum Filter {
    Single(SingleColumnValueFilter),
    Pagination(ColumnPaginationFilter),
    Composite(CompositeColumnValueFilter),
}

impl From<Filter> for crate::protos::filter::Filter {
    fn from(value: Filter) -> Self {
        match value {
            Filter::Single(f) => crate::protos::filter::Filter {
                r#type: FilterType::FtSingleColumnValue as i32,
                filter: f.into_protobuf_bytes(),
            },

            Filter::Pagination(f) => crate::protos::filter::Filter {
                r#type: FilterType::FtColumnPagination as i32,
                filter: f.into_protobuf_bytes(),
            },

            Filter::Composite(f) => crate::protos::filter::Filter {
                r#type: FilterType::FtCompositeColumnValue as i32,
                filter: f.into_protobuf_bytes(),
            },
        }
    }
}

impl Filter {
    /// 编码到 protobuf 字节
    pub fn into_protobuf_bytes(self) -> Vec<u8> {
        let msg: crate::protos::filter::Filter = self.into();

        msg.encode_to_vec()
    }
}
