use std::collections::HashMap;

use prost::Message;

use super::{validate_aggregation_name, Sort, Sorter};
use crate::model::rules::validate_column_name;
use crate::{
    error::OtsError,
    model::{ColumnValue, Row},
    protos::{plain_buffer::MASK_HEADER, search::AggregationType},
    OtsResult,
};

/// 在多元索引统计聚合中表示求平均值，用于返回一个字段的平均值，类似于 SQL 中的 `avg`。
///
/// 官方文档：<https://help.aliyun.com/zh/tablestore/developer-reference/avgaggregation>
#[derive(Debug, Default, Clone)]
pub struct AvgAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,

    /// 当某行数据中的字段为空时字段值的默认值
    ///
    /// - 如果未设置 missing value，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing value，则使用 missing value 作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,
}

impl AvgAggregation {
    pub fn new(name: &str, field_name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置字段缺失时的值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<AvgAggregation> for crate::protos::search::AvgAggregation {
    fn from(value: AvgAggregation) -> Self {
        let AvgAggregation {
            name: _,
            field_name,
            missing_value,
        } = value;

        crate::protos::search::AvgAggregation {
            field_name: Some(field_name),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
        }
    }
}

/// 在多元索引统计聚合中表示统计行数，用于返回指定字段值的数量或者多元索引数据总行数，类似于 SQL 中的 `count`。
#[derive(Debug, Default, Clone)]
pub struct CountAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,
}

impl CountAggregation {
    pub fn new(name: &str, field_name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<CountAggregation> for crate::protos::search::CountAggregation {
    fn from(value: CountAggregation) -> Self {
        let CountAggregation { name: _, field_name } = value;

        crate::protos::search::CountAggregation { field_name: Some(field_name) }
    }
}

/// 在多元索引统计聚合中表示去重统计行数，用于返回指定字段不同值的数量，类似于 SQL 中的 `count(distinct)`。
#[derive(Debug, Default, Clone)]
pub struct DistinctCountAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,

    /// 当某行数据中的字段为空时字段值的默认值
    ///
    /// - 如果未设置 missing value，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing value，则使用 missing value 作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,
}

impl DistinctCountAggregation {
    pub fn new(name: &str, field_name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置字段缺失时的值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<DistinctCountAggregation> for crate::protos::search::DistinctCountAggregation {
    fn from(value: DistinctCountAggregation) -> Self {
        let DistinctCountAggregation {
            name: _,
            field_name,
            missing_value,
        } = value;

        crate::protos::search::DistinctCountAggregation {
            field_name: Some(field_name),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
        }
    }
}

/// 在多元索引统计聚合中表示求最大值，用于返回一个字段中的最大值，类似于 SQL 中的 `max`。
#[derive(Debug, Default, Clone)]
pub struct MaxAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,

    /// 当某行数据中的字段为空时字段值的默认值
    ///
    /// - 如果未设置 missing value，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing value，则使用 missing value 作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,
}

impl MaxAggregation {
    pub fn new(name: &str, field_name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置字段缺失时的值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<MaxAggregation> for crate::protos::search::MaxAggregation {
    fn from(value: MaxAggregation) -> Self {
        let MaxAggregation {
            name: _,
            field_name,
            missing_value,
        } = value;

        crate::protos::search::MaxAggregation {
            field_name: Some(field_name),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
        }
    }
}

/// 在多元索引统计聚合中表示求最小值，用于返回一个字段中的最小值，类似于 SQL 中的 `min`。
#[derive(Debug, Default, Clone)]
pub struct MinAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,

    /// 当某行数据中的字段为空时字段值的默认值
    ///
    /// - 如果未设置 missing value，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing value，则使用 missing value 作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,
}

impl MinAggregation {
    pub fn new(name: &str, field_name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置字段缺失时的值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<MinAggregation> for crate::protos::search::MinAggregation {
    fn from(value: MinAggregation) -> Self {
        let MinAggregation {
            name: _,
            field_name,
            missing_value,
        } = value;

        crate::protos::search::MinAggregation {
            field_name: Some(field_name),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
        }
    }
}

/// 在多元索引统计聚合中表示百分位统计，百分位统计常用来统计一组数据的百分位分布情况，
/// 例如在日常系统运维中统计每次请求访问的耗时情况时，需要关注系统请求耗时的 P25、P50、P90、P99 值等分布情况。
#[derive(Debug, Default, Clone)]
pub struct PercentilesAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,

    /// 百分位分布例如50、90、99，可根据需要设置一个或者多个百分位
    pub percentiles: Vec<f64>,

    /// 当某行数据中的字段为空时字段值的默认值
    ///
    /// - 如果未设置 missing value，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing value，则使用 missing value 作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,
}

impl PercentilesAggregation {
    pub fn new(name: &str, field_name: &str, percentiles: impl IntoIterator<Item = f64>) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            percentiles: percentiles.into_iter().collect(),
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置字段缺失时的值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    /// 增加一个百分位数
    pub fn percentile(mut self, percentile: f64) -> Self {
        self.percentiles.push(percentile);

        self
    }

    /// 直接设置全部的百分位数
    pub fn percentiles(mut self, percentiles: impl IntoIterator<Item = f64>) -> Self {
        self.percentiles = percentiles.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        if self.percentiles.is_empty() {
            return Err(OtsError::ValidationFailed("percentiles must not be empty".to_string()));
        }

        Ok(())
    }
}

impl From<PercentilesAggregation> for crate::protos::search::PercentilesAggregation {
    fn from(value: PercentilesAggregation) -> Self {
        let PercentilesAggregation {
            name: _,
            field_name,
            missing_value,
            percentiles,
        } = value;

        crate::protos::search::PercentilesAggregation {
            field_name: Some(field_name),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
            percentiles,
        }
    }
}

/// 在多元索引统计聚合中表示求和，用于返回数值字段值的总和，类似于 SQL 中的 `sum`。
#[derive(Debug, Default, Clone)]
pub struct SumAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 用于统计聚合的字段。
    pub field_name: String,

    /// 当某行数据中的字段为空时字段值的默认值
    ///
    /// - 如果未设置 missing value，则在统计聚合时会忽略该行。
    /// - 如果设置了 missing value，则使用 missing value 作为字段值的默认值参与统计聚合。
    pub missing_value: Option<ColumnValue>,
}

impl SumAggregation {
    pub fn new(name: &str, field_name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置聚合字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置字段缺失时的值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation field name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<SumAggregation> for crate::protos::search::SumAggregation {
    fn from(value: SumAggregation) -> Self {
        let SumAggregation {
            name: _,
            field_name,
            missing_value,
        } = value;

        crate::protos::search::SumAggregation {
            field_name: Some(field_name),
            missing: missing_value.map(|v| v.encode_plain_buffer()),
        }
    }
}

/// 在多元索引统计聚合中表示获取统计聚合分组中的行，
/// 用于在对查询结果进行分组后获取每个分组内的一些行数据，
/// 可实现和 MySQL 中 `ANY_VALUE(field)` 类似的功能。
#[derive(Debug, Clone)]
pub struct TopRowsAggregation {
    /// 此聚合的名称，用来从响应中提取聚合结果
    pub name: String,

    /// 每个分组内最多返回的数据行数
    pub limit: u32,

    /// 分组内数据的排序方式。
    pub sorters: Vec<Sorter>,

    /// 当指定非 PrimaryKeySort 的 sorter 时，默认情况下会主动添加 PrimaryKeySort，
    /// 通过该参数可禁止主动添加 PrimaryKeySort
    pub disable_default_pk_sorter: bool,
}

impl Default for TopRowsAggregation {
    fn default() -> Self {
        Self {
            name: String::new(),
            limit: 1,
            sorters: vec![],
            disable_default_pk_sorter: false,
        }
    }
}

impl TopRowsAggregation {
    pub fn new(name: &str, limit: u32) -> Self {
        Self {
            name: name.to_string(),
            limit,
            ..Default::default()
        }
    }

    /// 设置聚合名称
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();

        self
    }

    /// 设置每组最多返回的行数
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;

        self
    }

    /// 添加一个排序方式
    pub fn sorter(mut self, sorter: Sorter) -> Self {
        self.sorters.push(sorter);

        self
    }

    /// 直接设置全部的排序方式
    pub fn sorters(mut self, sorters: impl IntoIterator<Item = Sorter>) -> Self {
        self.sorters = sorters.into_iter().collect();

        self
    }

    /// 设置是否禁用主动添加 PrimaryKeySort，
    pub fn disable_default_pk_sorter(mut self, disable_default_pk_sorter: bool) -> Self {
        self.disable_default_pk_sorter = disable_default_pk_sorter;

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_aggregation_name(&self.name) {
            return Err(OtsError::ValidationFailed(format!("invalid aggregation name: {}", self.name)));
        }

        if self.limit > i32::MAX as u32 {
            return Err(OtsError::ValidationFailed(format!("limit is too large: {}", self.limit)));
        }

        for s in &self.sorters {
            s.validate()?;
        }

        Ok(())
    }
}

impl From<TopRowsAggregation> for crate::protos::search::TopRowsAggregation {
    fn from(value: TopRowsAggregation) -> Self {
        let TopRowsAggregation {
            name: _,
            limit,
            sorters,
            disable_default_pk_sorter,
        } = value;

        crate::protos::search::TopRowsAggregation {
            limit: Some(limit as i32),
            sort: Some(Sort::with_sorters(sorters, disable_default_pk_sorter).into()),
        }
    }
}

/// 聚合枚举
#[derive(Debug, Clone)]
pub enum Aggregation {
    Min(MinAggregation),
    Max(MaxAggregation),
    Avg(AvgAggregation),
    Count(CountAggregation),
    DistinctCount(DistinctCountAggregation),
    Sum(SumAggregation),
    TopRows(TopRowsAggregation),
    Percentiles(PercentilesAggregation),
}

impl From<Aggregation> for crate::protos::search::Aggregation {
    fn from(value: Aggregation) -> Self {
        match value {
            Aggregation::Min(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggMin as i32),
                    body: Some(crate::protos::search::MinAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::Max(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggMax as i32),
                    body: Some(crate::protos::search::MaxAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::Avg(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggAvg as i32),
                    body: Some(crate::protos::search::AvgAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::Count(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggCount as i32),
                    body: Some(crate::protos::search::CountAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::DistinctCount(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggDistinctCount as i32),
                    body: Some(crate::protos::search::DistinctCountAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::Sum(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggSum as i32),
                    body: Some(crate::protos::search::SumAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::TopRows(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggTopRows as i32),
                    body: Some(crate::protos::search::TopRowsAggregation::from(aggr).encode_to_vec()),
                }
            }
            Aggregation::Percentiles(aggr) => {
                let name = aggr.name.clone();

                crate::protos::search::Aggregation {
                    name: Some(name),
                    r#type: Some(crate::protos::search::AggregationType::AggPercentiles as i32),
                    body: Some(crate::protos::search::PercentilesAggregation::from(aggr).encode_to_vec()),
                }
            }
        }
    }
}

impl Aggregation {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            Aggregation::Min(a) => a.validate(),
            Aggregation::Max(a) => a.validate(),
            Aggregation::Avg(a) => a.validate(),
            Aggregation::Count(a) => a.validate(),
            Aggregation::DistinctCount(a) => a.validate(),
            Aggregation::Sum(a) => a.validate(),
            Aggregation::TopRows(a) => a.validate(),
            Aggregation::Percentiles(a) => a.validate(),
        }
    }
}

impl<T, A> From<T> for crate::protos::search::Aggregations
where
    T: IntoIterator<Item = A>,
    A: Into<crate::protos::search::Aggregation>,
{
    fn from(value: T) -> Self {
        Self {
            aggs: value.into_iter().map(|a| a.into()).collect(),
        }
    }
}

/// 在百分位统计返回结果中表示返回的单个百分位信息。
#[derive(Debug, Clone)]
pub struct PercentilesAggregationItem {
    /// 每个百分位的值
    pub key: f64,

    /// 每个百分位的分布情况
    pub value: ColumnValue,
}

impl TryFrom<crate::protos::search::PercentilesAggregationItem> for PercentilesAggregationItem {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::PercentilesAggregationItem) -> Result<Self, Self::Error> {
        let crate::protos::search::PercentilesAggregationItem { key, value } = value;

        Ok(Self {
            key: key.unwrap_or_default(),
            value: if let Some(bytes) = value {
                ColumnValue::decode_plain_buffer(bytes)?
            } else {
                // WILL THIS HAPPEN?
                ColumnValue::Null
            },
        })
    }
}

impl TryFrom<crate::protos::search::PercentilesAggregationResult> for Vec<PercentilesAggregationItem> {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::PercentilesAggregationResult) -> Result<Self, Self::Error> {
        let mut items = vec![];

        for item in value.percentiles_aggregation_items {
            items.push(item.try_into()?)
        }

        Ok(items)
    }
}

impl TryFrom<crate::protos::search::TopRowsAggregationResult> for Vec<Row> {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::TopRowsAggregationResult) -> Result<Self, Self::Error> {
        let crate::protos::search::TopRowsAggregationResult { rows: rows_bytes } = value;

        let mut rows = vec![];

        for row_bytes in rows_bytes {
            if !row_bytes.is_empty() {
                rows.push(Row::decode_plain_buffer(row_bytes, MASK_HEADER)?);
            }
        }

        Ok(rows)
    }
}

/// 聚合结果枚举
#[derive(Debug, Clone)]
pub enum AggregationResult {
    Min(f64),
    Max(f64),
    Avg(f64),
    Sum(f64),
    Count(u64),
    DistinctCount(u64),
    TopRows(Vec<Row>),
    Percentiles(Vec<PercentilesAggregationItem>),
}

impl TryFrom<crate::protos::search::AggregationResult> for AggregationResult {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::AggregationResult) -> Result<Self, Self::Error> {
        let crate::protos::search::AggregationResult { name, r#type, agg_result } = value;

        if name.is_none() || r#type.is_none() {
            return Err(OtsError::ValidationFailed("invalid aggregation result type or name".to_string()));
        }

        let aggr_type = match AggregationType::try_from(r#type.unwrap_or_default()) {
            Ok(t) => t,
            Err(_) => return Err(OtsError::ValidationFailed(format!("invalid aggregation result type: {}", r#type.unwrap()))),
        };

        match aggr_type {
            AggregationType::AggAvg => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::AvgAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::Avg(msg.value()))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggMax => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::MaxAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::Max(msg.value()))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggMin => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::MinAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::Min(msg.value()))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggSum => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::SumAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::Sum(msg.value()))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggCount => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::CountAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::Count(msg.value() as u64))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggDistinctCount => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::DistinctCountAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::DistinctCount(msg.value() as u64))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggTopRows => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::TopRowsAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::TopRows(msg.try_into()?))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }

            AggregationType::AggPercentiles => {
                if let Some(bytes) = agg_result {
                    let msg = crate::protos::search::PercentilesAggregationResult::decode(bytes.as_slice())?;
                    Ok(Self::Percentiles(msg.try_into()?))
                } else {
                    Err(OtsError::ValidationFailed("invalid aggregation result data".to_string()))
                }
            }
        }
    }
}

impl TryFrom<crate::protos::search::AggregationsResult> for HashMap<String, AggregationResult> {
    type Error = OtsError;

    fn try_from(value: crate::protos::search::AggregationsResult) -> Result<Self, Self::Error> {
        let crate::protos::search::AggregationsResult { agg_results } = value;

        let mut map = HashMap::new();

        for ar in agg_results {
            let name = ar.name().to_string();
            let result = AggregationResult::try_from(ar)?;
            map.insert(name, result);
        }

        Ok(map)
    }
}
