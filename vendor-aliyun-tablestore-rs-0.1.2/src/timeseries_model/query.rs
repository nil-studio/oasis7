use prost::Message;

use crate::{
    error::OtsError,
    protos::timeseries::{MetaQueryCompositeOperator, MetaQueryConditionType, MetaQuerySingleOperator},
    OtsResult,
};

/// 度量名称查询。
/// 变体中的 `String` 是指查询值
#[derive(Debug, Clone)]
pub enum MeasurementMetaQuery {
    Equal(String),
    GreaterThan(String),
    GreaterEqual(String),
    LessThan(String),
    LessEqual(String),
    Prefix(String),
}

impl From<MeasurementMetaQuery> for crate::protos::timeseries::MetaQueryMeasurementCondition {
    fn from(value: MeasurementMetaQuery) -> Self {
        match value {
            MeasurementMetaQuery::Equal(value) => Self {
                op: MetaQuerySingleOperator::OpEqual as i32,
                value,
            },

            MeasurementMetaQuery::GreaterThan(value) => Self {
                op: MetaQuerySingleOperator::OpGreaterThan as i32,
                value,
            },

            MeasurementMetaQuery::GreaterEqual(value) => Self {
                op: MetaQuerySingleOperator::OpGreaterEqual as i32,
                value,
            },

            MeasurementMetaQuery::LessThan(value) => Self {
                op: MetaQuerySingleOperator::OpLessThan as i32,
                value,
            },

            MeasurementMetaQuery::LessEqual(value) => Self {
                op: MetaQuerySingleOperator::OpLessEqual as i32,
                value,
            },

            MeasurementMetaQuery::Prefix(value) => Self {
                op: MetaQuerySingleOperator::OpPrefix as i32,
                value,
            },
        }
    }
}

/// 数据源查询。
/// 变体中的 `String` 是指查询值
#[derive(Debug, Clone)]
pub enum DatasourceMetaQuery {
    Equal(String),
    GreaterThan(String),
    GreaterEqual(String),
    LessThan(String),
    LessEqual(String),
    Prefix(String),
}

impl From<DatasourceMetaQuery> for crate::protos::timeseries::MetaQuerySourceCondition {
    fn from(value: DatasourceMetaQuery) -> Self {
        match value {
            DatasourceMetaQuery::Equal(value) => Self {
                op: MetaQuerySingleOperator::OpEqual as i32,
                value,
            },

            DatasourceMetaQuery::GreaterThan(value) => Self {
                op: MetaQuerySingleOperator::OpGreaterThan as i32,
                value,
            },

            DatasourceMetaQuery::GreaterEqual(value) => Self {
                op: MetaQuerySingleOperator::OpGreaterEqual as i32,
                value,
            },

            DatasourceMetaQuery::LessThan(value) => Self {
                op: MetaQuerySingleOperator::OpLessThan as i32,
                value,
            },

            DatasourceMetaQuery::LessEqual(value) => Self {
                op: MetaQuerySingleOperator::OpLessEqual as i32,
                value,
            },

            DatasourceMetaQuery::Prefix(value) => Self {
                op: MetaQuerySingleOperator::OpPrefix as i32,
                value,
            },
        }
    }
}

/// 标签查询。
/// 变体中的 `.0` 是查询的标签名，`.1` 是查询的标签值
#[derive(Debug, Clone)]
pub enum TagMetaQuery {
    Equal(String, String),
    GreaterThan(String, String),
    GreaterEqual(String, String),
    LessThan(String, String),
    LessEqual(String, String),
    Prefix(String, String),
}

impl From<TagMetaQuery> for crate::protos::timeseries::MetaQueryTagCondition {
    fn from(value: TagMetaQuery) -> Self {
        match value {
            TagMetaQuery::Equal(tag_name, value) => Self {
                op: MetaQuerySingleOperator::OpEqual as i32,
                tag_name,
                value,
            },

            TagMetaQuery::GreaterThan(tag_name, value) => Self {
                op: MetaQuerySingleOperator::OpGreaterThan as i32,
                tag_name,
                value,
            },

            TagMetaQuery::GreaterEqual(tag_name, value) => Self {
                op: MetaQuerySingleOperator::OpGreaterEqual as i32,
                tag_name,
                value,
            },

            TagMetaQuery::LessThan(tag_name, value) => Self {
                op: MetaQuerySingleOperator::OpLessThan as i32,
                tag_name,
                value,
            },

            TagMetaQuery::LessEqual(tag_name, value) => Self {
                op: MetaQuerySingleOperator::OpLessEqual as i32,
                tag_name,
                value,
            },

            TagMetaQuery::Prefix(tag_name, value) => Self {
                op: MetaQuerySingleOperator::OpPrefix as i32,
                tag_name,
                value,
            },
        }
    }
}

/// 标签查询。
/// 变体中的 `.0` 是查询的属性名，`.1` 是查询的属性值
#[derive(Debug, Clone)]
pub enum AttributMetaQuery {
    Equal(String, String),
    GreaterThan(String, String),
    GreaterEqual(String, String),
    LessThan(String, String),
    LessEqual(String, String),
    Prefix(String, String),
}

impl From<AttributMetaQuery> for crate::protos::timeseries::MetaQueryAttributeCondition {
    fn from(value: AttributMetaQuery) -> Self {
        match value {
            AttributMetaQuery::Equal(attr_name, value) => Self {
                op: MetaQuerySingleOperator::OpEqual as i32,
                attr_name,
                value,
            },

            AttributMetaQuery::GreaterThan(attr_name, value) => Self {
                op: MetaQuerySingleOperator::OpGreaterThan as i32,
                attr_name,
                value,
            },

            AttributMetaQuery::GreaterEqual(attr_name, value) => Self {
                op: MetaQuerySingleOperator::OpGreaterEqual as i32,
                attr_name,
                value,
            },

            AttributMetaQuery::LessThan(attr_name, value) => Self {
                op: MetaQuerySingleOperator::OpLessThan as i32,
                attr_name,
                value,
            },

            AttributMetaQuery::LessEqual(attr_name, value) => Self {
                op: MetaQuerySingleOperator::OpLessEqual as i32,
                attr_name,
                value,
            },

            AttributMetaQuery::Prefix(attr_name, value) => Self {
                op: MetaQuerySingleOperator::OpPrefix as i32,
                attr_name,
                value,
            },
        }
    }
}

/// 更新时间查询
/// `.0` 是要查询的时间戳微秒
#[derive(Debug, Clone, Copy)]
pub enum UpdateTimeMetaQuery {
    Equal(u64),
    GreaterThan(u64),
    GreaterEqual(u64),
    LessThan(u64),
    LessEqual(u64),
    Prefix(u64), // Is this supported?
}

impl From<UpdateTimeMetaQuery> for crate::protos::timeseries::MetaQueryUpdateTimeCondition {
    fn from(value: UpdateTimeMetaQuery) -> Self {
        match value {
            UpdateTimeMetaQuery::Equal(value) => Self {
                op: MetaQuerySingleOperator::OpEqual as i32,
                value: value as i64,
            },

            UpdateTimeMetaQuery::GreaterThan(value) => Self {
                op: MetaQuerySingleOperator::OpGreaterThan as i32,
                value: value as i64,
            },

            UpdateTimeMetaQuery::GreaterEqual(value) => Self {
                op: MetaQuerySingleOperator::OpGreaterEqual as i32,
                value: value as i64,
            },

            UpdateTimeMetaQuery::LessThan(value) => Self {
                op: MetaQuerySingleOperator::OpLessThan as i32,
                value: value as i64,
            },

            UpdateTimeMetaQuery::LessEqual(value) => Self {
                op: MetaQuerySingleOperator::OpLessEqual as i32,
                value: value as i64,
            },

            UpdateTimeMetaQuery::Prefix(value) => Self {
                op: MetaQuerySingleOperator::OpPrefix as i32,
                value: value as i64,
            },
        }
    }
}

impl UpdateTimeMetaQuery {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            UpdateTimeMetaQuery::Equal(ts_us)
            | UpdateTimeMetaQuery::GreaterThan(ts_us)
            | UpdateTimeMetaQuery::GreaterEqual(ts_us)
            | UpdateTimeMetaQuery::LessThan(ts_us)
            | UpdateTimeMetaQuery::LessEqual(ts_us)
            | UpdateTimeMetaQuery::Prefix(ts_us) => {
                if *ts_us > i64::MAX as u64 {
                    return Err(OtsError::ValidationFailed(format!("timestamp in us is too large for i64: {}", *ts_us)));
                }
            }
        }

        Ok(())
    }
}

/// 组合查询
#[derive(Debug, Clone)]
pub struct CompositeMetaQuery {
    pub operator: MetaQueryCompositeOperator,
    pub sub_queries: Vec<MetaQuery>,
}

impl CompositeMetaQuery {
    pub fn new(operator: MetaQueryCompositeOperator) -> Self {
        Self { operator, sub_queries: vec![] }
    }

    /// 设置逻辑操作
    pub fn operator(mut self, operator: MetaQueryCompositeOperator) -> Self {
        self.operator = operator;

        self
    }

    /// 添加一个子查询
    pub fn sub_query(mut self, q: MetaQuery) -> Self {
        self.sub_queries.push(q);

        self
    }

    /// 设置子查询
    pub fn sub_queries(mut self, qs: impl IntoIterator<Item = MetaQuery>) -> Self {
        self.sub_queries = qs.into_iter().collect();

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if self.sub_queries.is_empty() {
            return Err(OtsError::ValidationFailed("sub queries can not be empty".to_string()));
        }

        Ok(())
    }
}

impl From<CompositeMetaQuery> for crate::protos::timeseries::MetaQueryCompositeCondition {
    fn from(value: CompositeMetaQuery) -> Self {
        let CompositeMetaQuery { operator, sub_queries } = value;

        Self {
            op: operator as i32,
            sub_conditions: sub_queries.into_iter().map(crate::protos::timeseries::MetaQueryCondition::from).collect(),
        }
    }
}

/// 元数据查询
#[derive(Debug, Clone)]
pub enum MetaQuery {
    Measurement(MeasurementMetaQuery),
    Datasource(DatasourceMetaQuery),
    Tag(TagMetaQuery),
    Attribute(AttributMetaQuery),
    UpdateTime(UpdateTimeMetaQuery),
    Composite(Box<CompositeMetaQuery>),
}

impl From<MetaQuery> for crate::protos::timeseries::MetaQueryCondition {
    fn from(value: MetaQuery) -> Self {
        match value {
            MetaQuery::Measurement(q) => Self {
                r#type: MetaQueryConditionType::MeasurementCondition as i32,
                proto_data: crate::protos::timeseries::MetaQueryMeasurementCondition::from(q).encode_to_vec(),
            },

            MetaQuery::Datasource(q) => Self {
                r#type: MetaQueryConditionType::SourceCondition as i32,
                proto_data: crate::protos::timeseries::MetaQuerySourceCondition::from(q).encode_to_vec(),
            },

            MetaQuery::Tag(q) => Self {
                r#type: MetaQueryConditionType::TagCondition as i32,
                proto_data: crate::protos::timeseries::MetaQueryTagCondition::from(q).encode_to_vec(),
            },

            MetaQuery::Attribute(q) => Self {
                r#type: MetaQueryConditionType::AttributeCondition as i32,
                proto_data: crate::protos::timeseries::MetaQueryAttributeCondition::from(q).encode_to_vec(),
            },

            MetaQuery::UpdateTime(q) => Self {
                r#type: MetaQueryConditionType::UpdateTimeCondition as i32,
                proto_data: crate::protos::timeseries::MetaQueryUpdateTimeCondition::from(q).encode_to_vec(),
            },

            MetaQuery::Composite(q) => Self {
                r#type: MetaQueryConditionType::CompositeCondition as i32,
                proto_data: crate::protos::timeseries::MetaQueryCompositeCondition::from(*q).encode_to_vec(),
            },
        }
    }
}

impl MetaQuery {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            MetaQuery::UpdateTime(q) => {
                q.validate()?;
            }

            MetaQuery::Composite(q) => {
                q.validate()?;
            }

            _ => {}
        }

        Ok(())
    }
}
