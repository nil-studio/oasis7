use super::{GeoPoint, NestedFilter};
use crate::model::rules::validate_column_name;
use crate::{
    error::OtsError,
    model::ColumnValue,
    protos::search::{DocSort, GeoDistanceType, PrimaryKeySort, ScoreSort, SortMode, SortOrder},
    OtsResult,
};

/// 多元索引中字段排序方式的配置。
#[derive(Debug, Default, Clone)]
pub struct FieldSort {
    /// 排序字段名称。
    pub field_name: String,

    /// 排序的顺序，支持升序排列和降序排列，默认为升序排列。
    pub order: Option<SortOrder>,

    /// 当字段存在多个值时的排序方式。只有当字段类型为数组类型时，才需要设置此参数。
    pub mode: Option<SortMode>,

    /// 嵌套类型的过滤条件。只有当字段类型为嵌套类型时，才需要设置此参数。
    pub nested_filter: Option<NestedFilter>,

    /// 当字段不存在时使用的字段默认值。
    pub missing_value: Option<ColumnValue>,

    /// 当指定字段不存在时使用的排序字段。
    pub missing_field: Option<String>,
}

impl FieldSort {
    pub fn new(field_name: &str) -> Self {
        Self {
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 设置排序顺序
    pub fn order(mut self, order: SortOrder) -> Self {
        self.order = Some(order);

        self
    }

    /// 设置多值字段的排序方式
    pub fn mode(mut self, mode: SortMode) -> Self {
        self.mode = Some(mode);

        self
    }

    /// 设置嵌套类型的过滤条件
    pub fn nested_filter(mut self, filter: NestedFilter) -> Self {
        self.nested_filter = Some(filter);

        self
    }

    /// 设置排序字段不存在时的默认值
    pub fn missing_value(mut self, value: ColumnValue) -> Self {
        self.missing_value = Some(value);

        self
    }

    /// 设置排序字段不存在时的备选字段
    pub fn missing_field(mut self, missing_field: impl Into<String>) -> Self {
        self.missing_field = Some(missing_field.into());

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid sort by name: {}", self.field_name)));
        }

        Ok(())
    }
}

impl From<FieldSort> for crate::protos::search::FieldSort {
    fn from(value: FieldSort) -> Self {
        let FieldSort {
            field_name,
            order,
            mode,
            nested_filter,
            missing_value,
            missing_field,
        } = value;

        crate::protos::search::FieldSort {
            field_name: Some(field_name),
            order: order.map(|o| o as i32),
            mode: mode.map(|m| m as i32),
            nested_filter: nested_filter.map(|nf| nf.into()),
            missing_value: missing_value.map(|v| v.encode_plain_buffer()),
            missing_field,
        }
    }
}

/// 地理位置排序方式
#[derive(Debug, Default, Clone)]
pub struct GeoDistanceSort {
    /// 排序字段名称。
    pub field_name: String,

    /// 中心坐标点，是一个经纬度值。
    pub points: Vec<GeoPoint>,

    /// 排序的顺序，支持升序排列和降序排列，默认为升序排列。
    pub order: Option<SortOrder>,

    /// 当字段存在多个值时的排序方式。只有当字段类型为数组类型时，才需要设置此参数。
    pub mode: Option<SortMode>,

    /// 距离计算方式。
    pub distance_type: Option<GeoDistanceType>,

    /// 嵌套类型的过滤条件。只有当字段类型为嵌套类型时，才需要设置此参数。
    pub nested_filter: Option<NestedFilter>,
}

impl GeoDistanceSort {
    /// 构造实例之后，还需要设置中心点
    pub fn new(field_name: &str) -> Self {
        Self {
            field_name: field_name.to_string(),
            ..Default::default()
        }
    }

    /// 设置字段名称
    pub fn field_name(mut self, field_name: &str) -> Self {
        self.field_name = field_name.to_string();

        self
    }

    /// 添加 1 个中心点
    pub fn point(mut self, point: GeoPoint) -> Self {
        self.points.push(point);

        self
    }

    /// 设置多个中心点
    pub fn points(mut self, points: impl IntoIterator<Item = GeoPoint>) -> Self {
        self.points = points.into_iter().collect();

        self
    }

    /// 设置排序顺序
    pub fn order(mut self, order: SortOrder) -> Self {
        self.order = Some(order);

        self
    }

    /// 设置多值字段的排序方式
    pub fn mode(mut self, mode: SortMode) -> Self {
        self.mode = Some(mode);

        self
    }

    /// 设置嵌套类型的过滤条件
    pub fn nested_filter(mut self, filter: NestedFilter) -> Self {
        self.nested_filter = Some(filter);

        self
    }

    /// 设置距离计算方式
    pub fn distance_type(mut self, distanct_type: GeoDistanceType) -> Self {
        self.distance_type = Some(distanct_type);

        self
    }

    pub(crate) fn validate(&self) -> OtsResult<()> {
        if !validate_column_name(&self.field_name) {
            return Err(OtsError::ValidationFailed(format!("invalid sort by name: {}", self.field_name)));
        }

        if self.points.is_empty() {
            return Err(OtsError::ValidationFailed("points must not be empty".to_string()));
        }

        Ok(())
    }
}

impl From<GeoDistanceSort> for crate::protos::search::GeoDistanceSort {
    fn from(value: GeoDistanceSort) -> Self {
        let GeoDistanceSort {
            field_name,
            points,
            order,
            mode,
            distance_type,
            nested_filter,
        } = value;

        crate::protos::search::GeoDistanceSort {
            field_name: Some(field_name),
            points: points.into_iter().map(|p| format!("{}", p)).collect(),
            order: order.map(|o| o as i32),
            mode: mode.map(|m| m as i32),
            distance_type: distance_type.map(|d| d as i32),
            nested_filter: nested_filter.map(|f| f.into()),
        }
    }
}

/// 索引的排序方式
#[derive(Debug, Clone)]
pub enum Sorter {
    /// 主键排序方式。
    PrimaryKey(SortOrder),

    /// 分数排序方式。
    Score(SortOrder),

    /// 按照数据行在多元索引中的存储顺序排序
    DocSort(SortOrder),

    /// 字段值排序方式。
    Field(FieldSort),

    /// 地理位置排序方式。
    GeoDistance(GeoDistanceSort),
}

impl Sorter {
    pub(crate) fn validate(&self) -> OtsResult<()> {
        match self {
            Self::Field(s) => s.validate(),
            Self::GeoDistance(s) => s.validate(),
            _ => Ok(()),
        }
    }
}

impl From<Sorter> for crate::protos::search::Sorter {
    fn from(value: Sorter) -> Self {
        let mut ret = crate::protos::search::Sorter::default();

        match value {
            Sorter::PrimaryKey(sort_order) => {
                ret.pk_sort = Some(PrimaryKeySort {
                    order: Some(sort_order as i32),
                });
            }
            Sorter::Score(sort_order) => {
                ret.score_sort = Some(ScoreSort {
                    order: Some(sort_order as i32),
                });
            }
            Sorter::DocSort(sort_order) => {
                ret.doc_sort = Some(DocSort {
                    order: Some(sort_order as i32),
                });
            }
            Sorter::Field(field_sort) => {
                ret.field_sort = Some(crate::protos::search::FieldSort::from(field_sort));
            }
            Sorter::GeoDistance(geo_distance_sort) => {
                ret.geo_distance_sort = Some(crate::protos::search::GeoDistanceSort::from(geo_distance_sort));
            }
        }

        ret
    }
}

impl<T, S> From<T> for crate::protos::search::Sort
where
    T: IntoIterator<Item = S>,
    S: Into<crate::protos::search::Sorter>,
{
    fn from(value: T) -> Self {
        crate::protos::search::Sort {
            sorter: value.into_iter().map(|i| i.into()).collect(),
            disable_default_pk_sorter: Some(false),
        }
    }
}

/// 封装 Sort
#[derive(Debug, Default, Clone)]
pub struct Sort {
    /// 排序器
    pub sorters: Vec<Sorter>,

    /// 当指定非 PrimaryKeySort 的 sorter 时，默认情况下会主动添加 PrimaryKeySort，
    /// 通过该参数可禁止主动添加 PrimaryKeySort
    pub disable_default_pk_sorter: bool,
}

impl Sort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sorters(sorters: Vec<Sorter>, disable_default_pk_sorter: bool) -> Self {
        Self {
            sorters,
            disable_default_pk_sorter,
        }
    }

    /// 添加一个排序器
    pub fn sorter(mut self, sorter: Sorter) -> Self {
        self.sorters.push(sorter);

        self
    }

    /// 设置排序器
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
        for sorter in &self.sorters {
            sorter.validate()?;
        }

        Ok(())
    }
}

impl From<Sort> for crate::protos::search::Sort {
    fn from(value: Sort) -> Self {
        let Sort {
            sorters,
            disable_default_pk_sorter,
        } = value;
        Self {
            sorter: sorters.into_iter().map(crate::protos::search::Sorter::from).collect(),
            disable_default_pk_sorter: Some(disable_default_pk_sorter),
        }
    }
}
