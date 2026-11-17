use std::{collections::HashMap, ops::Range};

use prost::Message;

use super::{validate_aggregation_name, validate_group_name, validate_timezone_string, Aggregation, AggregationResult, Duration, GeoPoint, Query};
use crate::model::rules::validate_column_name;
use crate::{
    error::OtsError,
    model::ColumnValue,
    protos::search::{FieldRange, GeoGrid, GeoHashPrecision, GroupByType, SortOrder},
    OtsResult,
};

/// 分组中的item排序规则集。
#[derive(Debug, Clone)]
pub enum GroupBySorter {
    /// 按照分组对应值排序的排序规则。
    GroupKey(SortOrder),

    /// 按照分组中总行数排序的排序规则
    RowCount(SortOrder),

    /// 按照某个子统计聚合排序的排序规则。元素 `.0` 是子统计聚合的名字
    SubAggregation(String, SortOrder),
}

impl From<GroupBySorter> for crate::protos::search::GroupBySorter {
    fn from(value: GroupBySorter) -> Self {
        let mut ret = crate::protos::search::GroupBySorter {
            group_key_sort: None,
            row_count_sort: None,
            sub_agg_sort: None,
        };

        match value {
            GroupBySorter::GroupKey(order) => {
                ret.group_key_sort = Some(crate::protos::search::GroupKeySort { order: Some(order as i32) });
            }
            GroupBySorter::RowCount(order) => {
                ret.row_count_sort = Some(crate::protos::search::RowCountSort { order: Some(order as i32) });
            }
            GroupBySorter::SubAggregation(name, order) => {
                ret.sub_agg_sort = Some(crate::protos::search::SubAggSort {
                    sub_agg_name: Some(name),
                    order: Some(order as i32),
                });
            }
        }

        ret
    }
}

impl GroupBySorter {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            Self::SubAggregation(name, _) => {
                if !validate_aggregation_name(name) {
                    return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", name)));
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// 从一组排序配置生成 GroupBySort
impl<T, S> From<T> for crate::protos::search::GroupBySort
where
    T: IntoIterator<Item = S>,
    S: Into<crate::protos::search::GroupBySorter>,
{
    fn from(value: T) -> Self {
        Self {
            sorters: value.into_iter().map(Into::into).collect(),
        }
    }
}

/// 对某一个字段进行分组统计。
///
/// 举例：库存账单里有“篮球”、“足球”、“羽毛球”等，对这一个字段进行聚合，返回： “篮球：10个”，“足球：5个”，“网球：1个”这样的聚合信息。
#[derive(Debug, Default, Clone)]
pub struct GroupByField {
    /// GroupBy 的名字，之后从 GroupBy 结果列表中根据该名字拿到 GroupBy 结果
    pub name: String,

    /// 字段名字
    pub field_name: String,

    /// 返回的分组数量，默认值为 `10`。最大值为 `2000`。当分组数量超过 `2000` 时，只会返回前 `2000` 个分组。
    pub size: u32,

    /// 分组中的 item 排序规则，默认按照分组中item的数量降序排序，多个排序则按照添加的顺序进行排列。
    pub sorters: Vec<GroupBySorter>,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,

    /// 最小行数。当分组中的行数小于最小行数时，不会返回此分组的统计结果。
    pub min_doc_count: Option<u64>,
}

impl GroupByField {
    pub fn new(name: &str, field_name: &str, size: u32) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            size,
            ..Default::default()
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置分组数量
    pub fn size(mut self, size: u32) -> Self {
        self.size = size;

        self
    }

    /// 设置最小行数
    pub fn min_doc_count(mut self, min_doc_count: u64) -> Self {
        self.min_doc_count = Some(min_doc_count);

        self
    }

    /// 增加排序配置
    pub fn sorter(mut self, sorter: GroupBySorter) -> Self {
        self.sorters.push(sorter);

        self
    }

    /// 设置排序配置
    pub fn sorters(mut self, sorters: impl IntoIterator<Item = GroupBySorter>) -> Self {
        self.sorters = sorters.into_iter().collect();

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    /// 验证数据
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        if self.size == 0 {
            return Err(OtsError::ValidationFailed("invalid size: 0".to_string()));
        }

        if self.size > i32::MAX as u32 {
            return Err(OtsError::ValidationFailed(format!("size is too large: {}", self.size)));
        }

        for s in &self.sorters {
            s.validate()?;
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByField> for crate::protos::search::GroupByField {
    fn from(value: GroupByField) -> Self {
        let GroupByField {
            name: _,
            field_name,
            size,
            sorters,
            sub_aggregations,
            sub_group_bys,
            min_doc_count,
        } = value;

        Self {
            field_name: Some(field_name),
            size: Some(size as i32),
            sort: Some(crate::protos::search::GroupBySort::from(sorters)),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
            min_doc_count: min_doc_count.map(|n| n as i64),
        }
    }
}

/// 在多元索引统计聚合中表示过滤条件分组，用于按照过滤条件对查询结果进行分组，获取每个过滤条件匹配到的数量，返回结果的顺序和添加过滤条件的顺序一致。
#[derive(Debug, Clone, Default)]
pub struct GroupByFilter {
    /// 分组名称
    pub name: String,

    /// 过滤器
    pub filters: Vec<Query>,

    /// 子聚合
    pub sub_aggregations: Vec<Aggregation>,

    /// 子分组
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByFilter {
    pub fn new(name: &str, filters: impl IntoIterator<Item = Query>) -> Self {
        Self {
            name: name.to_string(),
            filters: filters.into_iter().collect(),
            ..Default::default()
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    /// 添加一个过滤器
    pub fn filter(mut self, filter: Query) -> Self {
        self.filters.push(filter);

        self
    }

    /// 设置过滤器
    pub fn filters(mut self, filters: impl IntoIterator<Item = Query>) -> Self {
        self.filters = filters.into_iter().collect();

        self
    }

    /// 验证数据
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if self.filters.is_empty() {
            return Err(OtsError::ValidationFailed("filters are required, please set a valid value".to_string()));
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByFilter> for crate::protos::search::GroupByFilter {
    fn from(value: GroupByFilter) -> Self {
        let GroupByFilter {
            name: _,
            filters,
            sub_aggregations,
            sub_group_bys,
        } = value;

        Self {
            filters: filters.into_iter().map(crate::protos::search::Query::from).collect(),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
        }
    }
}

/// 在多元索引统计聚合中表示直方图统计，用于按照指定数据间隔对查询结果进行分组，字段值在相同范围内放到同一分组内，返回每个分组的值和该值对应的个数。
#[derive(Debug, Clone, Default)]
pub struct GroupByHistogram {
    /// 分组名称
    pub name: String,

    /// 字段名字
    pub field_name: String,

    /// 统计间隔
    pub interval: ColumnValue,

    /// 统计范围，与interval参数配合使用限制分组的数量。
    /// `(max_value - min_value) / interval` 的值不能超过 `2000`
    pub min_value: ColumnValue,

    /// 统计范围，与interval参数配合使用限制分组的数量。
    pub max_value: ColumnValue,

    /// 分组偏差量
    pub offset: Option<ColumnValue>,

    /// 当某行数据中的字段为空时，字段值的默认值
    ///
    /// - 如果未设置 missing 值，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing 值，则使用 missing 值作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,

    /// 最小行数。当分组中的行数小于最小行数时，不会返回此分组的统计结果。
    pub min_doc_count: Option<u64>,

    /// 分组中的item排序规则，默认按照分组中item的数量降序排序，多个排序则按照添加的顺序进行排列。
    pub sorters: Vec<GroupBySorter>,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByHistogram {
    pub fn new(name: &str, field_name: &str, min_value: ColumnValue, max_value: ColumnValue, interval: ColumnValue) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            min_value,
            max_value,
            interval,
            ..Default::default()
        }
    }
    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置间隔
    pub fn interval(mut self, interval: ColumnValue) -> Self {
        self.interval = interval;

        self
    }

    /// 设置最小值
    pub fn min_value(mut self, min_value: ColumnValue) -> Self {
        self.min_value = min_value;

        self
    }

    /// 设置最大值
    pub fn max_value(mut self, max_value: ColumnValue) -> Self {
        self.max_value = max_value;

        self
    }

    /// 设置最小行数
    pub fn min_doc_count(mut self, min_doc_count: u64) -> Self {
        self.min_doc_count = Some(min_doc_count);

        self
    }

    /// 设置分组偏差量
    pub fn offset(mut self, offset: ColumnValue) -> Self {
        self.offset = Some(offset);

        self
    }

    /// 增加排序配置
    pub fn sorter(mut self, sorter: GroupBySorter) -> Self {
        self.sorters.push(sorter);

        self
    }

    /// 设置排序配置
    pub fn sorters(mut self, sorters: impl IntoIterator<Item = GroupBySorter>) -> Self {
        self.sorters = sorters.into_iter().collect();

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    /// 验证数据
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        if self.interval == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("interval is required, please set a valid value".to_string()));
        }

        if self.min_value == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("field_range.min is required, please set a valid value".to_string()));
        }

        if self.max_value == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("field_range.max is required, please set a valid value".to_string()));
        }

        for s in &self.sorters {
            s.validate()?;
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByHistogram> for crate::protos::search::GroupByHistogram {
    fn from(value: GroupByHistogram) -> Self {
        let GroupByHistogram {
            name: _,
            field_name,
            interval,
            min_value,
            max_value,
            missing_value,
            min_doc_count,
            sorters,
            sub_aggregations,
            sub_group_bys,
            offset,
        } = value;

        Self {
            field_name: Some(field_name),
            interval: Some(interval.encode_plain_buffer()),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
            min_doc_count: min_doc_count.map(|v| v as i64),
            sort: Some(crate::protos::search::GroupBySort::from(sorters)),
            field_range: Some(crate::protos::search::FieldRange {
                min: Some(min_value.encode_plain_buffer()),
                max: Some(max_value.encode_plain_buffer()),
            }),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
            offset: offset.map(|v| v.encode_plain_buffer()),
        }
    }
}

/// 在多元索引统计聚合中表示范围分组，用于根据一个字段的范围对查询结果进行分组，字段值在某范围内放到同一分组内，返回每个范围中相应的item个数。
#[derive(Debug, Clone, Default)]
pub struct GroupByRange {
    // GroupBy 的名字，之后从 GroupBy 结果列表中根据该名字拿到 GroupBy 结果
    pub name: String,

    /// 字段名字
    pub field_name: String,

    /// 分组的范围配置，范围为左闭右开的区间。
    pub ranges: Vec<Range<f64>>,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByRange {
    pub fn new(name: &str, field_name: &str, ranges: impl IntoIterator<Item = Range<f64>>) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            ranges: ranges.into_iter().collect(),
            ..Default::default()
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 增加一组范围配置
    pub fn range(mut self, range: Range<f64>) -> Self {
        self.ranges.push(range);

        self
    }

    /// 设置分组范围
    pub fn ranges(mut self, ranges: impl IntoIterator<Item = Range<f64>>) -> Self {
        self.ranges = ranges.into_iter().collect();

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    /// 验证数据
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        if self.ranges.is_empty() {
            return Err(OtsError::ValidationFailed("ranges is required, please set a valid value".to_string()));
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByRange> for crate::protos::search::GroupByRange {
    fn from(value: GroupByRange) -> Self {
        let GroupByRange {
            name: _,
            field_name,
            ranges,
            sub_aggregations,
            sub_group_bys,
        } = value;

        Self {
            field_name: Some(field_name),
            ranges: ranges.into_iter().map(|r| r.into()).collect(),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
        }
    }
}

/// 在多元索引统计聚合中表示日期直方图统计，用于对日期字段类型的数据按照指定间隔对查询结果进行分组，字段值在相同范围内放到同一分组内，返回每个分组的值和该值对应的个数。
#[derive(Debug, Clone, Default)]
pub struct GroupByDateHistogram {
    /// GroupBy 的名字，之后从 GroupBy 结果列表中根据该名字拿到 GroupBy 结果
    pub name: String,

    /// 字段名字
    pub field_name: String,

    /// 统计间隔。**注意**这个是必填属性。为了可以 Default 搞成了这个样子
    pub interval: Option<Duration>,

    /// 分组偏差量
    pub offset: Option<Duration>,

    /// 统计范围，与interval参数配合使用限制分组的数量。
    /// `(max_value - min_value) / interval` 的值不能超过 `2000`
    pub min_value: ColumnValue,

    /// 统计范围，与interval参数配合使用限制分组的数量。
    pub max_value: ColumnValue,

    /// 当某行数据中的字段为空时，字段值的默认值
    ///
    /// - 如果未设置 missing 值，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing 值，则使用 missing 值作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,

    /// 最小行数。当分组中的行数小于最小行数时，不会返回此分组的统计结果。
    pub min_doc_count: Option<u64>,

    /// 时区。格式为 `+hh:mm` 或者 `-hh:mm` ，例如 `+08:00` 、 `-09:00`。
    pub timezone: Option<String>,

    /// 分组中的item排序规则，默认按照分组中item的数量降序排序，多个排序则按照添加的顺序进行排列。
    pub sorters: Vec<GroupBySorter>,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByDateHistogram {
    pub fn new(name: &str, field_name: &str, min_value: ColumnValue, max_value: ColumnValue, interval: Duration) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            min_value,
            max_value,
            interval: Some(interval),
            ..Default::default()
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置间隔
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);

        self
    }

    /// 设置最小值
    pub fn min_value(mut self, min_value: ColumnValue) -> Self {
        self.min_value = min_value;

        self
    }

    /// 设置最大值
    pub fn max_value(mut self, max_value: ColumnValue) -> Self {
        self.max_value = max_value;

        self
    }

    /// 设置最小行数
    pub fn min_doc_count(mut self, min_doc_count: u64) -> Self {
        self.min_doc_count = Some(min_doc_count);

        self
    }

    /// 设置分组偏差量
    pub fn offset(mut self, offset: Duration) -> Self {
        self.offset = Some(offset);

        self
    }

    /// 增加排序配置
    pub fn sorter(mut self, sorter: GroupBySorter) -> Self {
        self.sorters.push(sorter);

        self
    }

    /// 设置排序配置
    pub fn sorters(mut self, sorters: impl IntoIterator<Item = GroupBySorter>) -> Self {
        self.sorters = sorters.into_iter().collect();

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    /// 验证数据
    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        if self.interval.is_none() {
            return Err(OtsError::ValidationFailed("interval is required, please set a valid value".to_string()));
        }

        if self.min_value == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("field_range.min is required, please set a valid value".to_string()));
        }

        if self.max_value == ColumnValue::Null {
            return Err(OtsError::ValidationFailed("field_range.max is required, please set a valid value".to_string()));
        }

        if let Some(n) = self.min_doc_count {
            if n > i64::MAX as u64 {
                return Err(OtsError::ValidationFailed("min_doc_count must be less than or equal to i64::MAX".to_string()));
            }
        }

        if let Some(v) = &self.missing_value {
            if v == &ColumnValue::Null {
                return Err(OtsError::ValidationFailed("Please set a valid missing value".to_string()));
            }
        }

        if let Some(s) = &self.timezone {
            if !validate_timezone_string(s.as_str()) {
                return Err(OtsError::ValidationFailed(format!(
                    "invalid timezone string: {}. It should be like `+08:00` or `-08:00`",
                    s
                )));
            }
        }

        for s in &self.sorters {
            s.validate()?;
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByDateHistogram> for crate::protos::search::GroupByDateHistogram {
    fn from(value: GroupByDateHistogram) -> Self {
        let GroupByDateHistogram {
            name: _,
            field_name,
            interval,
            offset,
            min_value,
            max_value,
            missing_value,
            min_doc_count,
            timezone,
            sorters,
            sub_aggregations,
            sub_group_bys,
        } = value;

        Self {
            field_name: Some(field_name),
            interval: interval.map(|i| i.into()),
            offset: offset.map(|i| i.into()),
            field_range: Some(FieldRange {
                min: Some(min_value.encode_plain_buffer()),
                max: Some(max_value.encode_plain_buffer()),
            }),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
            min_doc_count: min_doc_count.map(|v| v as i64),
            time_zone: timezone,
            sort: Some(crate::protos::search::GroupBySort::from(sorters)),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
        }
    }
}

/// 对 GeoPoint 类型的字段按照地理区域进行分组统计
#[derive(Debug, Clone, Default)]
pub struct GroupByGeoGrid {
    /// 分组名称
    pub name: String,

    /// 字段名称
    pub field_name: String,

    /// 返回的分组数量
    pub size: u32,

    /// GroupBy 的精度
    pub precision: GeoHashPrecision,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByGeoGrid {
    pub fn new(name: &str, field_name: &str, size: u32, precision: GeoHashPrecision) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            size,
            precision,
            ..Default::default()
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置分组数量
    pub fn size(mut self, size: u32) -> Self {
        self.size = size;

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        if self.size > i32::MAX as u32 {
            return Err(OtsError::ValidationFailed("size is too large".to_string()));
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByGeoGrid> for crate::protos::search::GroupByGeoGrid {
    fn from(value: GroupByGeoGrid) -> Self {
        let GroupByGeoGrid {
            name: _,
            field_name,
            size,
            precision,
            sub_aggregations,
            sub_group_bys,
        } = value;

        Self {
            field_name: Some(field_name),
            precision: Some(precision as i32),
            size: Some(size as i32),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
        }
    }
}

/// 根据地理位置坐标进行分组。
#[derive(Debug, Default, Clone)]
pub struct GroupByGeoDistance {
    // 分组名称
    pub name: String,

    /// 字段名称
    pub field_name: String,

    /// 设置起始中心点坐标
    pub origin: GeoPoint,

    /// 分组的依据范围
    pub ranges: Vec<Range<f64>>,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByGeoDistance {
    pub fn new(name: &str, field_name: &str, origin: GeoPoint, ranges: impl IntoIterator<Item = Range<f64>>) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            origin,
            ranges: ranges.into_iter().collect(),
            sub_aggregations: Vec::new(),
            sub_group_bys: Vec::new(),
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置字段名
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置起始中心点
    pub fn origin(mut self, origin: GeoPoint) -> Self {
        self.origin = origin;

        self
    }

    /// 增加一个分组范围
    pub fn range(mut self, range: Range<f64>) -> Self {
        self.ranges.push(range);

        self
    }

    /// 设置分组范围
    pub fn ranges(mut self, ranges: impl IntoIterator<Item = Range<f64>>) -> Self {
        self.ranges = ranges.into_iter().collect();

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid field name: {}", self.field_name)));
        }

        if self.ranges.is_empty() {
            return Err(OtsError::ValidationFailed("ranges must not be empty".to_string()));
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByGeoDistance> for crate::protos::search::GroupByGeoDistance {
    fn from(value: GroupByGeoDistance) -> Self {
        let GroupByGeoDistance {
            name: _,
            field_name,
            origin,
            ranges,
            sub_aggregations,
            sub_group_bys,
        } = value;

        Self {
            field_name: Some(field_name),
            origin: Some(crate::protos::search::GeoPoint::from(origin)),
            ranges: ranges.into_iter().map(|r| r.into()).collect(),
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
        }
    }
}

/// 组合式分组
#[derive(Debug, Clone, Default)]
pub struct GroupByComposite {
    /// GroupBy 的名字，之后从 GroupBy 结果列表中根据该名字拿到 GroupBy 结果
    pub name: String,

    /// 返回的分组数量
    pub size: u32,

    /// 返回分组的数量；软限制，允许设置大于服务端最大限制值。当该值超过服务端最大值限制后被修正为最大值。
    ///
    /// - 实际值返回分组结果数量为：min(suggestedSize, 服务端分组数量限制，总分组数量)
    /// - 适用场景：重吞吐的场景，一般是对接计算系统，比如spark、presto等。
    pub suggested_size: Option<u32>,

    /// GroupByComposite 结果内会返回 `nextToken`，用于支持分组翻页
    pub next_token: Option<String>,

    /// 支持对多种类型的多列进行分组统计
    ///
    /// - [`GroupByField`]
    /// - [`GroupByHistogram`]
    /// - [`GroupByDateHistogram`]
    pub sources: Vec<GroupBy>,

    /// 子统计聚合Aggregation，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_aggregations: Vec<Aggregation>,

    /// 子统计聚合GroupBy，子统计聚合会根据分组内容再进行一次统计聚合分析。
    pub sub_group_bys: Vec<GroupBy>,
}

impl GroupByComposite {
    pub fn new(name: &str, size: u32) -> Self {
        Self {
            name: name.to_string(),
            size,
            ..Default::default()
        }
    }

    /// 设置名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置分组数量
    pub fn size(mut self, size: u32) -> Self {
        self.size = size;

        self
    }

    /// 设置软限制分组数量
    pub fn suggested_size(mut self, suggested_size: u32) -> Self {
        self.suggested_size = Some(suggested_size);

        self
    }

    /// 添加一个分组设置
    pub fn source(mut self, source: GroupBy) -> Self {
        self.sources.push(source);

        self
    }

    /// 设置分组集合
    pub fn sources(mut self, sources: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sources = sources.into_iter().collect();

        self
    }

    /// 设置分页 token
    pub fn next_token(mut self, next_token: impl Into<String>) -> Self {
        self.next_token = Some(next_token.into());

        self
    }

    /// 增加子聚合
    pub fn sub_aggregation(mut self, aggr: Aggregation) -> Self {
        self.sub_aggregations.push(aggr);

        self
    }

    /// 设置子聚合
    pub fn sub_aggregations(mut self, aggregations: impl IntoIterator<Item = Aggregation>) -> Self {
        self.sub_aggregations = aggregations.into_iter().collect();

        self
    }

    /// 增加子分组
    pub fn sub_group_by(mut self, sub_group_by: GroupBy) -> Self {
        self.sub_group_bys.push(sub_group_by);

        self
    }

    /// 设置子分组
    pub fn sub_group_bys(mut self, sub_group_bys: impl IntoIterator<Item = GroupBy>) -> Self {
        self.sub_group_bys = sub_group_bys.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_group_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid group name: {}", self.name)));
        }

        if self.size > i32::MAX as u32 {
            return Err(OtsError::ValidationFailed("size is too large".to_string()));
        }

        if let Some(n) = self.suggested_size {
            if n > i32::MAX as u32 {
                return Err(OtsError::ValidationFailed("suggested size is too large".to_string()));
            }
        }

        if self.sources.is_empty() {
            return Err(OtsError::ValidationFailed("sources must not be empty".to_string()));
        }

        for g in &self.sources {
            g.validate()?;
        }

        for g in &self.sub_group_bys {
            g.validate()?;
        }

        for a in &self.sub_aggregations {
            a.validate()?;
        }

        Ok(())
    }
}

impl From<GroupByComposite> for crate::protos::search::GroupByComposite {
    fn from(value: GroupByComposite) -> Self {
        let GroupByComposite {
            name: _,
            size,
            suggested_size,
            next_token,
            sources,
            sub_aggregations,
            sub_group_bys,
        } = value;

        Self {
            sources: Some(crate::protos::search::GroupBys::from(sources)),
            size: Some(size as i32),
            suggested_size: suggested_size.map(|n| n as i32),
            next_token,
            sub_aggs: Some(crate::protos::search::Aggregations::from(sub_aggregations)),
            sub_group_bys: Some(crate::protos::search::GroupBys::from(sub_group_bys)),
        }
    }
}

/// 分组设置
#[derive(Debug, Clone)]
pub enum GroupBy {
    Field(GroupByField),
    Filter(GroupByFilter),
    Range(GroupByRange),
    Histogram(GroupByHistogram),
    DateHistogram(GroupByDateHistogram),
    GeoGrid(GroupByGeoGrid),
    GeoDistance(GroupByGeoDistance),
    Composite(GroupByComposite),
}

impl From<GroupBy> for crate::protos::search::GroupBy {
    fn from(value: GroupBy) -> Self {
        match value {
            GroupBy::Field(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByField as i32),
                body: Some(crate::protos::search::GroupByField::from(gb).encode_to_vec()),
            },

            GroupBy::Filter(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByFilter as i32),
                body: Some(crate::protos::search::GroupByFilter::from(gb).encode_to_vec()),
            },

            GroupBy::Histogram(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByHistogram as i32),
                body: Some(crate::protos::search::GroupByHistogram::from(gb).encode_to_vec()),
            },

            GroupBy::DateHistogram(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByDateHistogram as i32),
                body: Some(crate::protos::search::GroupByDateHistogram::from(gb).encode_to_vec()),
            },

            GroupBy::Range(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByRange as i32),
                body: Some(crate::protos::search::GroupByRange::from(gb).encode_to_vec()),
            },

            GroupBy::GeoGrid(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByGeoGrid as i32),
                body: Some(crate::protos::search::GroupByGeoGrid::from(gb).encode_to_vec()),
            },

            GroupBy::GeoDistance(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByGeoDistance as i32),
                body: Some(crate::protos::search::GroupByGeoDistance::from(gb).encode_to_vec()),
            },

            GroupBy::Composite(gb) => Self {
                name: Some(gb.name.clone()),
                r#type: Some(GroupByType::GroupByComposite as i32),
                body: Some(crate::protos::search::GroupByComposite::from(gb).encode_to_vec()),
            },
        }
    }
}

impl GroupBy {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            GroupBy::Field(gb) => gb.validate(),
            GroupBy::Filter(gb) => gb.validate(),
            GroupBy::Range(gb) => gb.validate(),
            GroupBy::Histogram(gb) => gb.validate(),
            GroupBy::DateHistogram(gb) => gb.validate(),
            GroupBy::GeoGrid(gb) => gb.validate(),
            GroupBy::GeoDistance(gb) => gb.validate(),
            GroupBy::Composite(gb) => gb.validate(),
        }
    }
}

impl<T, G> From<T> for crate::protos::search::GroupBys
where
    T: IntoIterator<Item = G>,
    G: Into<crate::protos::search::GroupBy>,
{
    fn from(value: T) -> Self {
        Self {
            group_bys: value.into_iter().map(|g| g.into()).collect(),
        }
    }
}

/// 在字段值分组的返回结果中表示单个字段值的分组信息。
#[derive(Debug, Clone)]
pub struct GroupByFieldResultItem {
    /// 单个分组的字段值
    pub value: String,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组 SubGroupBy 的返回信息。
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByFieldResultItem> for GroupByFieldResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByFieldResultItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByFieldResultItem {
            key,
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        Ok(Self {
            value: key.unwrap_or_default(),
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },

            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 过滤器分组结果条目
#[derive(Debug, Clone)]
pub struct GroupByFilterResultItem {
    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByFilterResultItem> for GroupByFilterResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByFilterResultItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByFilterResultItem {
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        Ok(Self {
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 范围分组返回结果条目
#[derive(Debug, Clone)]
pub struct GroupByRangeResultItem {
    /// 单个分组的范围起始值
    pub value_from: f64,

    /// 单个分组的范围结束值
    pub value_to: f64,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByRangeResultItem> for GroupByRangeResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByRangeResultItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByRangeResultItem {
            from,
            to,
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        Ok(Self {
            value_from: from.unwrap_or_default(),
            value_to: to.unwrap_or_default(),
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 直方图分组统计结果条目
#[derive(Debug, Clone)]
pub struct GroupByHistogramResultItem {
    /// 单个分组的字段值
    pub value: ColumnValue,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByHistogramItem> for GroupByHistogramResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByHistogramItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByHistogramItem {
            key,
            value: row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        let value = if let Some(bytes) = key {
            ColumnValue::decode_plain_buffer(bytes)?
        } else {
            return Err(OtsError::ValidationFailed(
                "invalid column value from group by histogram result item".to_string(),
            ));
        };

        Ok(Self {
            value,
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 日期直方图统计结果
#[derive(Debug, Clone)]
pub struct GroupByDateHistogramResultItem {
    /// 单个分组的时间戳，毫秒为单位
    pub value: i64,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByDateHistogramItem> for GroupByDateHistogramResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByDateHistogramItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByDateHistogramItem {
            timestamp,
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        Ok(Self {
            value: timestamp.unwrap_or_default(),
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct GroupByGeoGridResultItem {
    pub value: String,

    pub geo_grid: GeoGrid,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByGeoGridResultItem> for GroupByGeoGridResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByGeoGridResultItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByGeoGridResultItem {
            key,
            geo_grid,
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        Ok(Self {
            value: key.unwrap_or_default(),
            geo_grid: geo_grid.unwrap_or_default(),
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 距离分组结果条目
#[derive(Debug, Clone)]
pub struct GroupByGeoDistanceResultItem {
    /// 单个分组的范围起始值
    pub value_from: f64,

    /// 单个分组的范围结束值
    pub value_to: f64,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByGeoDistanceResultItem> for GroupByGeoDistanceResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByGeoDistanceResultItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByGeoDistanceResultItem {
            from,
            to,
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        Ok(Self {
            value_from: from.unwrap_or_default(),
            value_to: to.unwrap_or_default(),
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 组合式分组结果条目
#[derive(Debug, Clone)]
pub struct GroupByCompositeResultItem {
    /// 每个分组的字段值，如果分组数据对应字段不存在，这个值可能为 `None`
    pub values: Vec<Option<String>>,

    /// 单个分组对应的总行数
    pub row_count: u64,

    /// 子统计聚合结果
    pub sub_aggregation_results: HashMap<String, AggregationResult>,

    /// 子统计分组结果
    pub sub_group_by_results: HashMap<String, GroupByResult>,
}

impl TryFrom<crate::protos::search::GroupByCompositeResultItem> for GroupByCompositeResultItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByCompositeResultItem) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByCompositeResultItem {
            keys,
            is_null_keys,
            row_count,
            sub_aggs_result,
            sub_group_bys_result,
        } = value;

        // 不知道为什么是这么一个逻辑，从 Java SDK 转换过来的
        let values = if keys.len() != is_null_keys.len() {
            keys.into_iter().map(Some).collect::<Vec<_>>()
        } else {
            (0..keys.len())
                .map(|idx| {
                    if let Some(b) = is_null_keys.get(idx) {
                        if *b {
                            None
                        } else {
                            keys.get(idx).map(|s| s.to_string())
                        }
                    } else {
                        keys.get(idx).map(|s| s.to_string())
                    }
                })
                .collect::<Vec<_>>()
        };

        Ok(Self {
            values,
            row_count: row_count.unwrap_or_default() as u64,
            sub_aggregation_results: if let Some(agg_results) = sub_aggs_result {
                agg_results.try_into()?
            } else {
                HashMap::new()
            },
            sub_group_by_results: if let Some(sub_results) = sub_group_bys_result {
                sub_results.try_into()?
            } else {
                HashMap::new()
            },
        })
    }
}

/// 统计聚合 GroupBy 的返回信息。
#[derive(Debug, Clone)]
pub enum GroupByResult {
    Field(Vec<GroupByFieldResultItem>),
    Filter(Vec<GroupByFilterResultItem>),
    Range(Vec<GroupByRangeResultItem>),
    Histogram(Vec<GroupByHistogramResultItem>),
    DateHistogram(Vec<GroupByDateHistogramResultItem>),
    GeoGrid(Vec<GroupByGeoGridResultItem>),
    GeoDistance(Vec<GroupByGeoDistanceResultItem>),
    Composite(Vec<GroupByCompositeResultItem>),
}

impl TryFrom<crate::protos::search::GroupByResult> for GroupByResult {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupByResult) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupByResult {
            name: _,
            r#type: result_type,
            group_by_result,
        } = value;

        let result_type = match GroupByType::try_from(result_type.unwrap_or_default()) {
            Ok(t) => t,
            Err(_) => {
                return Err(OtsError::ValidationFailed(format!(
                    "invalid group by result type: {}",
                    result_type.unwrap_or_default()
                )));
            }
        };

        match result_type {
            GroupByType::GroupByField => {
                if let Some(bytes) = group_by_result {
                    let by_field_results = crate::protos::search::GroupByFieldResult::decode(bytes.as_slice())?;
                    let mut items = vec![];
                    for result_item in by_field_results.group_by_field_result_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::Field(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByFilter => {
                if let Some(bytes) = group_by_result {
                    let by_filter_results = crate::protos::search::GroupByFilterResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_filter_results.group_by_filter_result_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::Filter(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByRange => {
                if let Some(bytes) = group_by_result {
                    let by_range_results = crate::protos::search::GroupByRangeResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_range_results.group_by_range_result_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::Range(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByHistogram => {
                if let Some(bytes) = group_by_result {
                    let by_his_results = crate::protos::search::GroupByHistogramResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_his_results.group_by_histogra_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::Histogram(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByDateHistogram => {
                if let Some(bytes) = group_by_result {
                    let by_date_his_results = crate::protos::search::GroupByDateHistogramResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_date_his_results.group_by_date_histogram_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::DateHistogram(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByGeoGrid => {
                if let Some(bytes) = group_by_result {
                    let by_geo_grid_results = crate::protos::search::GroupByGeoGridResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_geo_grid_results.group_by_geo_grid_result_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::GeoGrid(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByGeoDistance => {
                if let Some(bytes) = group_by_result {
                    let by_geo_dis_results = crate::protos::search::GroupByGeoDistanceResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_geo_dis_results.group_by_geo_distance_result_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::GeoDistance(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }

            GroupByType::GroupByComposite => {
                if let Some(bytes) = group_by_result {
                    let by_comp_results = crate::protos::search::GroupByCompositeResult::decode(bytes.as_slice())?;
                    let mut items = vec![];

                    for result_item in by_comp_results.group_by_composite_result_items {
                        items.push(result_item.try_into()?);
                    }

                    Ok(Self::Composite(items))
                } else {
                    Err(OtsError::ValidationFailed("invalid group by result bytes data".to_string()))
                }
            }
        }
    }
}

impl TryFrom<crate::protos::search::GroupBysResult> for HashMap<String, GroupByResult> {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::GroupBysResult) -> Result<Self, Self::Error> {
        let crate::protos::search::GroupBysResult { group_by_results } = value;

        let mut map = HashMap::new();

        for r in group_by_results {
            map.insert(r.name().to_string(), GroupByResult::try_from(r)?);
        }

        Ok(map)
    }
}

#[cfg(test)]
mod test_group_by {
    use std::collections::HashMap;

    use prost::Message;

    use crate::test_util::setup;

    use super::GroupByResult;

    #[test]
    fn test_group_by_result_parser() {
        setup();

        let bytes = std::fs::read("/home/yuanyq/Downloads/aliyun-plainbuffer/group-bys.data").unwrap();
        let msg = crate::protos::search::GroupBysResult::decode(bytes.as_slice()).unwrap();
        let map = HashMap::<String, GroupByResult>::try_from(msg);
        log::debug!("{:?}", map);
    }
}
